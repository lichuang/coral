use serde::{Deserialize, Serialize};

use crate::common::CoralResult;
use crate::core::DocInner;
use crate::types::{Counter, Lamport, ObjectId, PeerID, Timestamp};
use crate::version::VersionVector;

#[derive(Serialize, Deserialize)]
pub struct JsonSchema {
  pub schema_version: u8,
  pub commits: Vec<JsonCommit>,
}

#[derive(Serialize, Deserialize)]
pub struct JsonCommit {
  pub id: JsonOpId,
  pub lamport: Lamport,
  pub timestamp: Timestamp,
  pub deps: Vec<JsonOpId>,
  pub ops: Vec<JsonOp>,
}

#[derive(Serialize, Deserialize)]
pub struct JsonOpId {
  pub peer: PeerID,
  pub counter: Counter,
}

#[derive(Serialize, Deserialize)]
pub struct JsonOp {
  pub container: ObjectId,
  #[serde(flatten)]
  pub cmd: JsonCmd,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum JsonCmd {
  IncCounter { delta: f64 },
}

const SCHEMA_VERSION: u8 = 1;

pub fn build_schema(
  doc: &DocInner,
  start_vv: &VersionVector,
  end_vv: &VersionVector,
) -> CoralResult<JsonSchema> {
  let registry = doc.registry();

  let mut commits = Vec::new();
  doc.iter_commits_in_range(start_vv, end_vv, |commit| {
    if let Ok(jc) = commit.to_json_commit(registry) {
      commits.push(jc);
    }
  });

  Ok(JsonSchema {
    schema_version: SCHEMA_VERSION,
    commits,
  })
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::Document;
  use crate::common::CoralError;

  #[test]
  fn test_export_json_full_range() {
    let mut doc = Document::new();
    let peer = doc.peer_id();

    {
      let mut counter = doc.get_counter("hits").unwrap();
      counter.increment(1.0).unwrap();
      counter.increment(2.0).unwrap();
    }
    doc.commit();

    let start_vv = VersionVector::new();
    let end_vv = doc.causal_graph().vv().clone();
    let json = crate::encoding::export_json(&doc, &start_vv, &end_vv).unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["schema_version"], SCHEMA_VERSION);
    assert_eq!(parsed["commits"].as_array().unwrap().len(), 1);

    let commit = &parsed["commits"][0];
    assert_eq!(commit["id"]["peer"], peer);
    assert_eq!(commit["id"]["counter"], 0);
    assert_eq!(commit["lamport"], 0);
    assert_eq!(commit["ops"].as_array().unwrap().len(), 2);
  }

  #[test]
  fn test_export_json_partial_range() {
    let mut doc = Document::new();
    let peer = doc.peer_id();

    {
      let mut counter = doc.get_counter("c").unwrap();
      counter.increment(1.0).unwrap();
      counter.increment(1.0).unwrap();
      counter.increment(1.0).unwrap();
    }
    doc.commit();

    {
      let mut counter = doc.get_counter("c").unwrap();
      counter.increment(1.0).unwrap();
    }
    doc.commit();

    let mut start_vv = VersionVector::new();
    start_vv.insert(peer, 3);
    let end_vv = doc.causal_graph().vv().clone();
    let json = crate::encoding::export_json(&doc, &start_vv, &end_vv).unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["commits"].as_array().unwrap().len(), 1);
    assert_eq!(parsed["commits"][0]["id"]["counter"], 3);
  }

  #[test]
  fn test_export_json_empty_range() {
    let mut doc = Document::new();
    let peer = doc.peer_id();

    {
      let mut counter = doc.get_counter("c").unwrap();
      counter.increment(1.0).unwrap();
    }
    doc.commit();

    let mut vv = VersionVector::new();
    vv.insert(peer, 1);
    let json = crate::encoding::export_json(&doc, &vv, &vv).unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["commits"].as_array().unwrap().len(), 0);
  }

  #[test]
  fn test_export_json_cmd_fields() {
    let mut doc = Document::new();
    let peer = doc.peer_id();

    {
      let mut counter = doc.get_counter("score").unwrap();
      counter.increment(5.0).unwrap();
    }
    doc.commit();

    let start_vv = VersionVector::new();
    let mut end_vv = VersionVector::new();
    end_vv.insert(peer, 1);
    let json = crate::encoding::export_json(&doc, &start_vv, &end_vv).unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let op = &parsed["commits"][0]["ops"][0];
    assert_eq!(op["container"]["name"], "score");
    assert_eq!(op["container"]["type"], "counter");
    assert_eq!(op["type"], "inc_counter");
    assert!(op["delta"].as_f64().unwrap() - 5.0 < f64::EPSILON);
  }

  #[test]
  fn test_import_json_roundtrip() {
    let mut doc_a = Document::new();

    {
      let mut counter = doc_a.get_counter("hits").unwrap();
      counter.increment(3.0).unwrap();
      counter.increment(2.0).unwrap();
    }
    doc_a.commit();

    let start_vv = VersionVector::new();
    let end_vv = doc_a.causal_graph().vv().clone();
    let json = crate::encoding::export_json(&doc_a, &start_vv, &end_vv).unwrap();

    let mut doc_b = Document::new();
    doc_b.import_json(&json).unwrap();

    assert_eq!(doc_b.commit_store().len(), 1);
    let counter_b = doc_b.get_counter("hits").unwrap();
    assert_eq!(counter_b.value(), 5.0);
  }

  #[test]
  fn test_import_json_multiple_commits() {
    let mut doc_a = Document::new();

    {
      let mut counter = doc_a.get_counter("c").unwrap();
      counter.increment(1.0).unwrap();
    }
    doc_a.commit();

    {
      let mut counter = doc_a.get_counter("c").unwrap();
      counter.increment(2.0).unwrap();
    }
    doc_a.commit();

    let start_vv = VersionVector::new();
    let end_vv = doc_a.causal_graph().vv().clone();
    let json = crate::encoding::export_json(&doc_a, &start_vv, &end_vv).unwrap();

    let mut doc_b = Document::new();
    doc_b.import_json(&json).unwrap();

    assert_eq!(doc_b.commit_store().len(), 2);
    let counter_b = doc_b.get_counter("c").unwrap();
    assert_eq!(counter_b.value(), 3.0);
  }

  #[test]
  fn test_import_json_partial_export() {
    let mut doc_a = Document::new();
    let peer_a = doc_a.peer_id();

    {
      let mut counter = doc_a.get_counter("c").unwrap();
      counter.increment(1.0).unwrap();
    }
    doc_a.commit();

    {
      let mut counter = doc_a.get_counter("c").unwrap();
      counter.increment(2.0).unwrap();
    }
    doc_a.commit();

    // Bootstrap doc_b with the first commit so deps are satisfied.
    let mut bootstrap_start = VersionVector::new();
    bootstrap_start.insert(peer_a, 0);
    let mut bootstrap_end = VersionVector::new();
    bootstrap_end.insert(peer_a, 1);
    let bootstrap_json =
      crate::encoding::export_json(&doc_a, &bootstrap_start, &bootstrap_end).unwrap();

    let mut doc_b = Document::new();
    doc_b.import_json(&bootstrap_json).unwrap();
    assert_eq!(doc_b.commit_store().len(), 1);
    assert_eq!(doc_b.get_counter("c").unwrap().value(), 1.0);

    // Now import the partial export (second commit only).
    let mut start_vv = VersionVector::new();
    start_vv.insert(peer_a, 1);
    let end_vv = doc_a.causal_graph().vv().clone();
    let partial_json = crate::encoding::export_json(&doc_a, &start_vv, &end_vv).unwrap();

    doc_b.import_json(&partial_json).unwrap();

    assert_eq!(doc_b.commit_store().len(), 2);
    let counter_b = doc_b.get_counter("c").unwrap();
    assert_eq!(counter_b.value(), 3.0);
  }

  #[test]
  fn test_import_json_empty_commits() {
    let json = r#"{"schema_version":1,"commits":[]}"#;
    let mut doc = Document::new();
    doc.import_json(json).unwrap();
    assert_eq!(doc.commit_store().len(), 0);
  }

  #[test]
  fn test_import_json_container_includes_type() {
    let mut doc = Document::new();

    {
      let mut counter = doc.get_counter("mycounter").unwrap();
      counter.increment(1.0).unwrap();
    }
    doc.commit();

    let start_vv = VersionVector::new();
    let end_vv = doc.causal_graph().vv().clone();
    let json = crate::encoding::export_json(&doc, &start_vv, &end_vv).unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let container = &parsed["commits"][0]["ops"][0]["container"];
    assert_eq!(container["name"], "mycounter");
    assert_eq!(container["type"], "counter");
  }

  #[test]
  fn test_import_json_from_raw_json() {
    let json = r#"{
      "schema_version": 1,
      "commits": [{
        "id": {"peer": 999, "counter": 0},
        "lamport": 0,
        "timestamp": 0,
        "deps": [],
        "ops": [{
          "container": {"name": "votes", "type": "counter"},
          "type": "inc_counter",
          "delta": 7.0
        }]
      }]
    }"#;

    let mut doc = Document::new();
    doc.import_json(json).unwrap();
    assert_eq!(doc.commit_store().len(), 1);
    let counter = doc.get_counter("votes").unwrap();
    assert_eq!(counter.value(), 7.0);
  }

  #[test]
  fn test_import_json_same_container_two_commits() {
    let json = r#"{
      "schema_version": 1,
      "commits": [
        {
          "id": {"peer": 100, "counter": 0},
          "lamport": 0,
          "timestamp": 0,
          "deps": [],
          "ops": [{"container": {"name": "score", "type": "counter"}, "type": "inc_counter", "delta": 3.0}]
        },
        {
          "id": {"peer": 200, "counter": 0},
          "lamport": 1,
          "timestamp": 0,
          "deps": [{"peer": 100, "counter": 0}],
          "ops": [{"container": {"name": "score", "type": "counter"}, "type": "inc_counter", "delta": 4.0}]
        }
      ]
    }"#;

    let mut doc = Document::new();
    doc.import_json(json).unwrap();
    assert_eq!(doc.commit_store().len(), 2);
    let counter = doc.get_counter("score").unwrap();
    assert_eq!(counter.value(), 7.0);
  }

  #[test]
  fn test_import_json_type_mismatch() {
    let json = r#"{
      "schema_version": 1,
      "commits": [{
        "id": {"peer": 999, "counter": 0},
        "lamport": 0,
        "timestamp": 0,
        "deps": [],
        "ops": [{"container": {"name": "score", "type": "unknown"}, "type": "inc_counter", "delta": 1.0}]
      }]
    }"#;

    let mut doc = Document::new();
    assert!(doc.import_json(json).is_err());
  }

  #[test]
  fn test_import_json_reimport_same_container_ok() {
    let json = r#"{
      "schema_version": 1,
      "commits": [{
        "id": {"peer": 100, "counter": 0},
        "lamport": 0,
        "timestamp": 0,
        "deps": [],
        "ops": [{"container": {"name": "hits", "type": "counter"}, "type": "inc_counter", "delta": 3.0}]
      }]
    }"#;

    let mut doc = Document::new();
    doc.import_json(json).unwrap();
    assert_eq!(doc.get_counter("hits").unwrap().value(), 3.0);

    let json2 = r#"{
      "schema_version": 1,
      "commits": [{
        "id": {"peer": 200, "counter": 0},
        "lamport": 1,
        "timestamp": 0,
        "deps": [{"peer": 100, "counter": 0}],
        "ops": [{"container": {"name": "hits", "type": "counter"}, "type": "inc_counter", "delta": 4.0}]
      }]
    }"#;
    doc.import_json(json2).unwrap();
    assert_eq!(doc.commit_store().len(), 2);
    assert_eq!(doc.get_counter("hits").unwrap().value(), 7.0);
  }

  #[test]
  fn test_import_json_node_container_rejected() {
    let json = r#"{
      "schema_version": 1,
      "commits": [{
        "id": {"peer": 100, "counter": 0},
        "lamport": 0,
        "timestamp": 0,
        "deps": [],
        "ops": [{"container": {"op": "100@42", "type": "counter"}, "type": "inc_counter", "delta": 1.0}]
      }]
    }"#;

    let mut doc = Document::new();
    let result = doc.import_json(json);
    assert!(result.is_err());
    match result.unwrap_err() {
      CoralError::InvalidImport(msg) => {
        assert!(msg.contains("node container"), "unexpected msg: {}", msg);
      }
      other => panic!("expected InvalidImport, got {:?}", other),
    }
  }

  /// Helper: full export of a document (empty start → current vv).
  fn full_export(doc: &Document) -> String {
    let start = VersionVector::new();
    let end = doc.causal_graph().vv().clone();
    crate::encoding::export_json(doc, &start, &end).unwrap()
  }

  /// Helper: incremental export from `from_vv` to `doc`'s current vv.
  fn incremental_export(doc: &Document, from_vv: &VersionVector) -> String {
    let end = doc.causal_graph().vv().clone();
    crate::encoding::export_json(doc, from_vv, &end).unwrap()
  }

  fn counter_value(doc: &mut Document, name: &str) -> f64 {
    doc.get_counter(name).unwrap().value()
  }

  #[test]
  fn test_concurrent_two_peers_sync() {
    let mut a = Document::new();
    let mut b = Document::new();

    // A: +10, -3 → 7
    a.get_counter("hits").unwrap().increment(10.0).unwrap();
    a.get_counter("hits").unwrap().increment(-3.0).unwrap();
    a.commit();

    // B: +5, -2 → 3
    b.get_counter("hits").unwrap().increment(5.0).unwrap();
    b.get_counter("hits").unwrap().increment(-2.0).unwrap();
    b.commit();

    // A → B
    let json_a = full_export(&a);
    b.import_json(&json_a).unwrap();
    assert_eq!(counter_value(&mut b, "hits"), 10.0);

    // B → A (incremental)
    let vv_a = a.causal_graph().vv().clone();
    let json_b = incremental_export(&b, &vv_a);
    a.import_json(&json_b).unwrap();
    assert_eq!(counter_value(&mut a, "hits"), 10.0);
  }

  #[test]
  fn test_concurrent_three_peers_star_sync() {
    let mut a = Document::new();
    let mut b = Document::new();
    let mut c = Document::new();

    // A: +5 → 5
    a.get_counter("votes").unwrap().increment(5.0).unwrap();
    a.commit();

    // B: +3, -1 → 2
    b.get_counter("votes").unwrap().increment(3.0).unwrap();
    b.get_counter("votes").unwrap().increment(-1.0).unwrap();
    b.commit();

    // C: -2, +4 → 2
    c.get_counter("votes").unwrap().increment(-2.0).unwrap();
    c.get_counter("votes").unwrap().increment(4.0).unwrap();
    c.commit();

    // All sync to A (hub)
    a.import_json(&full_export(&b)).unwrap();
    a.import_json(&full_export(&c)).unwrap();
    assert_eq!(counter_value(&mut a, "votes"), 9.0);

    // A distributes back to B and C
    let vv_b = b.causal_graph().vv().clone();
    let vv_c = c.causal_graph().vv().clone();
    b.import_json(&incremental_export(&a, &vv_b)).unwrap();
    c.import_json(&incremental_export(&a, &vv_c)).unwrap();
    assert_eq!(counter_value(&mut b, "votes"), 9.0);
    assert_eq!(counter_value(&mut c, "votes"), 9.0);
  }

  #[test]
  fn test_concurrent_multiple_commits_same_peer() {
    let mut a = Document::new();
    let mut b = Document::new();

    // A: 3 commits with mixed +/- → 4
    a.get_counter("score").unwrap().increment(10.0).unwrap();
    a.commit();
    a.get_counter("score").unwrap().increment(-3.0).unwrap();
    a.commit();
    a.get_counter("score").unwrap().increment(-3.0).unwrap();
    a.commit();
    assert_eq!(counter_value(&mut a, "score"), 4.0);

    // B: +10, -2 → 8
    b.get_counter("score").unwrap().increment(10.0).unwrap();
    b.get_counter("score").unwrap().increment(-2.0).unwrap();
    b.commit();

    let json_a = full_export(&a);
    let json_b = full_export(&b);
    a.import_json(&json_b).unwrap();
    b.import_json(&json_a).unwrap();

    assert_eq!(counter_value(&mut a, "score"), 12.0);
    assert_eq!(counter_value(&mut b, "score"), 12.0);
  }

  #[test]
  fn test_concurrent_chain_sync() {
    // A → B → C (chain propagation)
    let mut a = Document::new();
    let mut b = Document::new();
    let mut c = Document::new();

    // A: +7, -2 → 5
    a.get_counter("views").unwrap().increment(7.0).unwrap();
    a.get_counter("views").unwrap().increment(-2.0).unwrap();
    a.commit();

    // B: +3 → 3
    b.get_counter("views").unwrap().increment(3.0).unwrap();
    b.commit();

    // C: +5, -1 → 4
    c.get_counter("views").unwrap().increment(5.0).unwrap();
    c.get_counter("views").unwrap().increment(-1.0).unwrap();
    c.commit();

    // A → B
    b.import_json(&full_export(&a)).unwrap();
    assert_eq!(counter_value(&mut b, "views"), 8.0);

    // B → C (carries A + B)
    let vv_c = c.causal_graph().vv().clone();
    c.import_json(&incremental_export(&b, &vv_c)).unwrap();
    assert_eq!(counter_value(&mut c, "views"), 12.0);

    // C → A (carries everything)
    let vv_a = a.causal_graph().vv().clone();
    a.import_json(&incremental_export(&c, &vv_a)).unwrap();
    assert_eq!(counter_value(&mut a, "views"), 12.0);
  }

  #[test]
  fn test_concurrent_incremental_round_trip() {
    let mut a = Document::new();
    let mut b = Document::new();

    // Round 1: A edits, syncs to B
    a.get_counter("ticks").unwrap().increment(5.0).unwrap();
    a.commit();
    b.import_json(&full_export(&a)).unwrap();
    assert_eq!(counter_value(&mut b, "ticks"), 5.0);

    // Round 2: Both edit concurrently
    a.get_counter("ticks").unwrap().increment(-1.0).unwrap();
    a.commit();
    b.get_counter("ticks").unwrap().increment(-2.0).unwrap();
    b.commit();

    let vv_a = a.causal_graph().vv().clone();
    let vv_b = b.causal_graph().vv().clone();
    a.import_json(&incremental_export(&b, &vv_a)).unwrap();
    b.import_json(&incremental_export(&a, &vv_b)).unwrap();
    assert_eq!(counter_value(&mut a, "ticks"), 2.0);
    assert_eq!(counter_value(&mut b, "ticks"), 2.0);

    // Round 3: Both edit again
    a.get_counter("ticks").unwrap().increment(3.0).unwrap();
    a.commit();
    b.get_counter("ticks").unwrap().increment(-1.0).unwrap();
    b.commit();

    let vv_a = a.causal_graph().vv().clone();
    let vv_b = b.causal_graph().vv().clone();
    a.import_json(&incremental_export(&b, &vv_a)).unwrap();
    b.import_json(&incremental_export(&a, &vv_b)).unwrap();
    assert_eq!(counter_value(&mut a, "ticks"), 4.0);
    assert_eq!(counter_value(&mut b, "ticks"), 4.0);
  }

  #[test]
  fn test_concurrent_two_counters_independent() {
    let mut a = Document::new();
    let mut b = Document::new();

    a.get_counter("x").unwrap().increment(10.0).unwrap();
    a.get_counter("x").unwrap().increment(-2.0).unwrap();
    a.get_counter("y").unwrap().increment(20.0).unwrap();
    a.commit();

    b.get_counter("x").unwrap().increment(-1.0).unwrap();
    b.get_counter("y").unwrap().increment(2.0).unwrap();
    b.get_counter("y").unwrap().increment(-3.0).unwrap();
    b.commit();

    // A → B
    b.import_json(&full_export(&a)).unwrap();
    // B → A (incremental: only B's own ops)
    let vv_a = a.causal_graph().vv().clone();
    a.import_json(&incremental_export(&b, &vv_a)).unwrap();

    // x: (10-2) + (-1) = 7
    assert_eq!(counter_value(&mut a, "x"), 7.0);
    assert_eq!(counter_value(&mut b, "x"), 7.0);
    // y: 20 + (2-3) = 19
    assert_eq!(counter_value(&mut a, "y"), 19.0);
    assert_eq!(counter_value(&mut b, "y"), 19.0);
  }
}
