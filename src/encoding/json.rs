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
