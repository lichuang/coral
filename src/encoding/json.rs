use serde::Serialize;

use crate::common::CoralResult;
use crate::core::DocInner;
use crate::operation::Cmd;
use crate::types::{Counter, Lamport, ObjectId, PeerID, Timestamp};
use crate::version::{Heads, VersionVector};

#[derive(Serialize)]
pub struct JsonSchema {
  schema_version: u8,
  commits: Vec<JsonCommit>,
}

#[derive(Serialize)]
struct JsonCommit {
  id: JsonOpId,
  lamport: Lamport,
  timestamp: Timestamp,
  deps: Vec<JsonOpId>,
  ops: Vec<JsonOp>,
}

#[derive(Serialize)]
struct JsonOpId {
  peer: PeerID,
  counter: Counter,
}

#[derive(Serialize)]
struct JsonOp {
  container: String,
  #[serde(flatten)]
  cmd: JsonCmd,
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum JsonCmd {
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
    let deps = match &commit.deps {
      Heads::Empty => Vec::new(),
      Heads::Linear(id) => vec![JsonOpId {
        peer: id.peer,
        counter: id.counter,
      }],
      Heads::Concurrent(map) => map
        .iter()
        .map(|(&p, &c)| JsonOpId {
          peer: p,
          counter: c,
        })
        .collect(),
    };

    let mut ops = Vec::new();
    for op in commit.ops.iter() {
      let container_name = registry
        .get_by_index(op.container)
        .and_then(|id| match id {
          ObjectId::Root { name, .. } => Some(name.clone()),
          ObjectId::Node { .. } => None,
        })
        .unwrap_or_else(|| format!("<node:{}>", op.container.index()));

      let cmd = match &op.cmd {
        Cmd::IncCounter { delta } => JsonCmd::IncCounter { delta: *delta },
      };

      ops.push(JsonOp {
        container: container_name,
        cmd,
      });
    }

    commits.push(JsonCommit {
      id: JsonOpId {
        peer: commit.id.peer,
        counter: commit.id.counter,
      },
      lamport: commit.lamport,
      timestamp: commit.timestamp,
      deps,
      ops,
    });
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
    assert_eq!(op["container"], "score");
    assert_eq!(op["type"], "inc_counter");
    assert!(op["delta"].as_f64().unwrap() - 5.0 < f64::EPSILON);
  }
}
