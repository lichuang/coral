use crate::common::CoralResult;
use crate::encoding::json::{JsonCommit, JsonOpId};
use crate::object::ObjectRegistry;
use crate::operation::Op;
use crate::rle::{HasLength, RleVec};
use crate::types::{Counter, Lamport, ObjectId, ObjectIndex, OpId, Timestamp};
use crate::version::Heads;

/// A group of operations produced by a single peer at one causal moment.
///
/// A `Commit` is the atomic unit of collaboration: it bundles one or more
/// [`Op`]s together with metadata that describes *when* and *after what*
/// the commit happened. Peers exchange `Commit`s (not individual `Op`s)
/// during sync.
///
/// # Fields
///
/// - `id` — the starting [`OpId`] (`peer` + `counter`) of this commit.
///   All contained ops share the same `peer` and have consecutive counters.
/// - `lamport` — Lamport timestamp used for causal ordering.
/// - `timestamp` — physical wall-clock time (seconds since Unix epoch).
/// - `deps` — the [`Heads`] this commit depends on (the DAG frontier right
///   before this commit was created).
/// - `ops` — the actual operations, stored in a run-length encoded vector.
/// - `from_local` — `true` if this commit originated from the local peer,
///   `false` if it was imported from a remote peer during sync.
#[derive(Debug, Clone, PartialEq)]
pub struct Commit {
  pub id: OpId,
  pub lamport: Lamport,
  pub timestamp: Timestamp,
  pub deps: Heads,
  pub ops: RleVec<Op>,
  pub from_local: bool,
}

impl Commit {
  /// Creates a new empty `Commit` with the given metadata.
  pub fn new(
    id: OpId,
    lamport: Lamport,
    timestamp: Timestamp,
    deps: Heads,
    from_local: bool,
  ) -> Self {
    Self {
      id,
      lamport,
      timestamp,
      deps,
      ops: RleVec::new(),
      from_local,
    }
  }

  /// Appends an operation to this commit.
  ///
  /// If the new op is mergeable with the last stored op, they are combined
  /// in-place via the [`RleVec`](crate::rle::RleVec) mechanism.
  pub fn push_op(&mut self, op: Op) {
    self.ops.push(op);
  }

  /// Returns the exclusive end counter of this commit's operation range.
  ///
  /// The commit covers counters `[id.counter, end_counter())`.
  pub fn end_counter(&self) -> Counter {
    self.id.counter
      + self
        .ops
        .iter()
        .map(|op| op.content_len() as Counter)
        .sum::<Counter>()
  }

  /// Debug-only check that all ops form a contiguous counter range
  /// starting at `id.counter`.
  #[cfg(debug_assertions)]
  pub fn assert_contiguous(&self) {
    let mut expected = self.id.counter;
    for op in self.ops.iter() {
      debug_assert_eq!(
        op.counter, expected,
        "ops not contiguous: expected counter {} but found {}",
        expected, op.counter
      );
      expected += op.content_len() as Counter;
    }
  }

  pub fn to_json_commit(&self, registry: &ObjectRegistry) -> CoralResult<JsonCommit> {
    let mut ops = Vec::new();
    for op in self.ops.iter() {
      ops.push(op.to_json_op(registry)?);
    }

    Ok(JsonCommit {
      id: JsonOpId {
        peer: self.id.peer,
        counter: self.id.counter,
      },
      lamport: self.lamport,
      timestamp: self.timestamp,
      deps: self.deps.to_json_ids(),
      ops,
    })
  }

  pub fn from_json_commit(
    jc: JsonCommit,
    ensure_container: &mut dyn FnMut(&str, ObjectId) -> CoralResult<ObjectIndex>,
  ) -> CoralResult<Self> {
    let deps = Heads::from_json_ids(jc.deps);

    let mut commit = Commit::new(
      OpId::new(jc.id.peer, jc.id.counter),
      jc.lamport,
      jc.timestamp,
      deps,
      false,
    );

    for jop in jc.ops {
      let mut op = Op::from_json_op(jop, ensure_container)?;
      op.counter = commit.end_counter();
      commit.push_op(op);
    }

    Ok(commit)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::encoding::json::{JsonCmd, JsonOp};
  use crate::operation::Cmd;
  use crate::types::ObjectType;

  fn simple_registry() -> ObjectRegistry {
    let mut reg = ObjectRegistry::new();
    reg.alloc_root(
      "score".into(),
      ObjectId::Root {
        name: "score".into(),
        typ: ObjectType::Counter,
      },
    );
    reg.alloc_root(
      "hits".into(),
      ObjectId::Root {
        name: "hits".into(),
        typ: ObjectType::Counter,
      },
    );
    reg
  }

  fn simple_ensure_container(_name: &str, id: ObjectId) -> CoralResult<ObjectIndex> {
    let typ = id.typ();
    Ok(ObjectIndex::new(0, typ))
  }

  #[test]
  fn test_commit_to_json_empty_deps() {
    let commit = Commit::new(OpId::new(42, 0), 5, 1000, Heads::new(), false);
    let reg = ObjectRegistry::new();
    let jc = commit.to_json_commit(&reg).unwrap();
    assert!(jc.deps.is_empty());
    assert_eq!(jc.id.peer, 42);
    assert_eq!(jc.id.counter, 0);
    assert_eq!(jc.lamport, 5);
    assert_eq!(jc.timestamp, 1000);
    assert!(jc.ops.is_empty());
  }

  #[test]
  fn test_commit_to_json_single_dep() {
    let commit = Commit::new(
      OpId::new(10, 0),
      1,
      0,
      Heads::from_id(OpId::new(5, 3)),
      false,
    );
    let reg = ObjectRegistry::new();
    let jc = commit.to_json_commit(&reg).unwrap();
    assert_eq!(jc.deps.len(), 1);
    assert_eq!(jc.deps[0].peer, 5);
    assert_eq!(jc.deps[0].counter, 3);
  }

  #[test]
  fn test_commit_to_json_concurrent_deps() {
    let mut heads = Heads::new();
    heads.push(OpId::new(1, 0));
    heads.push(OpId::new(2, 0));
    let commit = Commit::new(OpId::new(3, 0), 2, 0, heads, false);
    let reg = ObjectRegistry::new();
    let jc = commit.to_json_commit(&reg).unwrap();
    assert_eq!(jc.deps.len(), 2);
  }

  #[test]
  fn test_commit_to_json_with_ops() {
    let reg = simple_registry();
    let mut commit = Commit::new(OpId::new(42, 0), 0, 0, Heads::new(), false);
    let idx = reg.get_root("score").unwrap();
    commit.push_op(Op::new(0, idx, Cmd::IncCounter { delta: 1.0 }));
    commit.push_op(Op::new(1, idx, Cmd::IncCounter { delta: 2.0 }));

    let jc = commit.to_json_commit(&reg).unwrap();
    assert_eq!(jc.ops.len(), 2);

    assert_eq!(jc.ops[0].container.name(), "score");
    assert_eq!(jc.ops[0].cmd, JsonCmd::IncCounter { delta: 1.0 });
    assert_eq!(jc.ops[1].cmd, JsonCmd::IncCounter { delta: 2.0 });
  }

  #[test]
  fn test_commit_json_roundtrip_no_deps() {
    let reg = simple_registry();
    let idx = reg.get_root("score").unwrap();

    let mut commit = Commit::new(OpId::new(100, 0), 3, 999, Heads::new(), false);
    commit.push_op(Op::new(0, idx, Cmd::IncCounter { delta: 5.0 }));

    let jc = commit.to_json_commit(&reg).unwrap();
    let json = serde_json::to_string(&jc).unwrap();
    let jc2: JsonCommit = serde_json::from_str(&json).unwrap();
    let restored = Commit::from_json_commit(jc2, &mut simple_ensure_container).unwrap();

    assert_eq!(restored.id, OpId::new(100, 0));
    assert_eq!(restored.lamport, 3);
    assert_eq!(restored.timestamp, 999);
    assert!(matches!(restored.deps, Heads::Empty));
    assert_eq!(restored.ops.len(), 1);
  }

  #[test]
  fn test_commit_json_roundtrip_with_deps() {
    let reg = simple_registry();
    let idx = reg.get_root("hits").unwrap();

    let mut heads = Heads::new();
    heads.push(OpId::new(10, 0));
    heads.push(OpId::new(20, 0));

    let mut commit = Commit::new(OpId::new(30, 0), 5, 0, heads, false);
    commit.push_op(Op::new(0, idx, Cmd::IncCounter { delta: -1.0 }));

    let jc = commit.to_json_commit(&reg).unwrap();
    let json = serde_json::to_string(&jc).unwrap();
    let jc2: JsonCommit = serde_json::from_str(&json).unwrap();
    let restored = Commit::from_json_commit(jc2, &mut simple_ensure_container).unwrap();

    assert_eq!(restored.id.peer, 30);
    assert!(matches!(restored.deps, Heads::Concurrent(_)));
  }

  #[test]
  fn test_commit_json_roundtrip_multi_ops() {
    let reg = simple_registry();
    let score = reg.get_root("score").unwrap();
    let hits = reg.get_root("hits").unwrap();

    let mut commit = Commit::new(OpId::new(1, 0), 0, 0, Heads::new(), false);
    commit.push_op(Op::new(0, score, Cmd::IncCounter { delta: 1.0 }));
    commit.push_op(Op::new(1, hits, Cmd::IncCounter { delta: 10.0 }));
    commit.push_op(Op::new(2, score, Cmd::IncCounter { delta: 2.0 }));

    let jc = commit.to_json_commit(&reg).unwrap();
    let json = serde_json::to_string(&jc).unwrap();
    let jc2: JsonCommit = serde_json::from_str(&json).unwrap();

    let mut alloc_idx = 0u32;
    let restored = Commit::from_json_commit(jc2, &mut |_name, _id| {
      let idx = ObjectIndex::new(alloc_idx, ObjectType::Counter);
      alloc_idx += 1;
      Ok(idx)
    })
    .unwrap();

    assert_eq!(restored.ops.len(), 3);
    assert_eq!(restored.end_counter(), 3);
  }

  #[test]
  fn test_commit_json_output_format() {
    let reg = simple_registry();
    let idx = reg.get_root("score").unwrap();

    let mut commit = Commit::new(OpId::new(42, 0), 1, 0, Heads::new(), false);
    commit.push_op(Op::new(0, idx, Cmd::IncCounter { delta: 3.5 }));

    let jc = commit.to_json_commit(&reg).unwrap();
    let json = serde_json::to_string(&jc).unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["id"]["peer"], 42);
    assert_eq!(parsed["id"]["counter"], 0);
    assert_eq!(parsed["lamport"], 1);
    assert!(parsed["deps"].as_array().unwrap().is_empty());
    assert_eq!(parsed["ops"][0]["container"]["name"], "score");
    assert_eq!(parsed["ops"][0]["container"]["type"], "counter");
    assert_eq!(parsed["ops"][0]["type"], "inc_counter");
  }

  #[test]
  fn test_commit_from_json_node_container_rejected() {
    let jc = JsonCommit {
      id: JsonOpId {
        peer: 1,
        counter: 0,
      },
      lamport: 0,
      timestamp: 0,
      deps: vec![],
      ops: vec![JsonOp {
        container: ObjectId::Node {
          op: OpId::new(1, 42),
          typ: ObjectType::Counter,
        },
        cmd: JsonCmd::IncCounter { delta: 1.0 },
      }],
    };
    let result = Commit::from_json_commit(jc, &mut simple_ensure_container);
    assert!(result.is_err());
  }
}
