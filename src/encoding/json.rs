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
}
