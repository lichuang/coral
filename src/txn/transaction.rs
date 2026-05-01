//! Transaction structure and lifecycle methods.

use crate::core::change::Change;
use crate::core::container::ContainerIdx;
use crate::memory::arena::SharedArena;
use crate::op::{Op, OpContent, RawOpContent};
use crate::oplog::OpLog;
use crate::rle::{HasLength, RleVec};
use crate::types::{Counter, Lamport, PeerID, Timestamp};
use crate::version::Frontiers;

use super::EventHint;

/// A local editing transaction — buffers ops and commits them as a single Change.
///
/// # Lifecycle
///
/// 1. **Create** — [`Transaction::new`] allocates counter/lamport range.
/// 2. **Apply ops** — [`apply_local_op`](Transaction::apply_local_op)
///    (or [`add_op`](Transaction::add_op)) buffers operations.
/// 3. **Commit** — [`commit`](Transaction::commit) builds a [`Change`] and
///    imports it into the [`OpLog`].
#[derive(Debug)]
pub struct Transaction {
  /// Peer that owns this transaction.
  pub peer: PeerID,
  /// Counter of the first op in this transaction.
  pub start_counter: Counter,
  /// Next available counter (incremented as ops are added).
  pub next_counter: Counter,
  /// Lamport timestamp for this transaction's Change.
  pub start_lamport: Lamport,
  /// Next lamport (currently identical to `start_lamport`; reserved for future use).
  pub next_lamport: Lamport,
  /// Frontiers at transaction start — become the Change's `deps`.
  pub frontiers: Frontiers,
  /// Accumulated ops in this transaction.
  pub local_ops: Vec<Op>,
  /// Shared arena for value/string allocation.
  pub arena: SharedArena,
  /// `true` after the transaction has been committed or dropped.
  pub finished: bool,
  /// Event hints parallel to `local_ops`.
  pub event_hints: Vec<EventHint>,
}

impl Transaction {
  /// Creates a new transaction with the given causal metadata.
  ///
  /// `counter` and `lamport` are typically obtained from the [`OpLog`]
  /// (`oplog.next_id(peer).counter` and
  /// `dag.get_change_lamport_from_deps(&frontiers)`).
  pub fn new(
    peer: PeerID,
    counter: Counter,
    lamport: Lamport,
    frontiers: Frontiers,
    arena: SharedArena,
  ) -> Self {
    Self {
      peer,
      start_counter: counter,
      next_counter: counter,
      start_lamport: lamport,
      next_lamport: lamport,
      frontiers,
      local_ops: Vec::new(),
      arena,
      finished: false,
      event_hints: Vec::new(),
    }
  }

  /// Returns `true` if no ops have been buffered.
  pub fn is_empty(&self) -> bool {
    self.local_ops.is_empty()
  }

  /// Total number of atomic operations buffered.
  pub fn len(&self) -> usize {
    self.local_ops.iter().map(|op| op.atom_len()).sum()
  }

  /// Directly add an arena-resolved op to this transaction.
  ///
  /// This is the low-level entry point; callers are responsible for any
  /// arena allocation required by `content`.
  ///
  /// # Panics
  ///
  /// Panics if the transaction has already been finished.
  pub fn add_op(&mut self, container: ContainerIdx, content: OpContent) {
    assert!(!self.finished, "cannot add op to a finished transaction");

    let op = Op::new(self.next_counter, container, content);
    self.next_counter += op.atom_len() as Counter;
    self.local_ops.push(op);
  }

  /// Apply a local operation to this transaction.
  ///
  /// Converts `raw_content` into arena-resolved [`OpContent`], creates an
  /// [`Op`], and buffers it.
  ///
  /// # Panics
  ///
  /// Panics if the transaction has already been finished.
  pub fn apply_local_op(
    &mut self,
    container: ContainerIdx,
    raw_content: RawOpContent<'_>,
    event_hint: Option<EventHint>,
  ) {
    assert!(!self.finished, "cannot apply op to a finished transaction");

    let content = raw_content.to_op_content(&self.arena);
    let op = Op::new(self.next_counter, container, content);
    self.next_counter += op.atom_len() as Counter;
    self.local_ops.push(op);

    if let Some(hint) = event_hint {
      self.event_hints.push(hint);
    }

    // TODO(Phase 9+): apply op to DocState when DocState is implemented
  }

  /// Commit this transaction, producing a [`Change`] and importing it into the OpLog.
  ///
  /// `timestamp` is the physical wall-clock time in **seconds** since the Unix epoch.
  ///
  /// Returns `None` if the transaction is empty (no ops were applied).
  ///
  /// # Panics
  ///
  /// Panics if the transaction has already been finished.
  pub fn commit_with_timestamp(
    mut self,
    oplog: &mut OpLog,
    timestamp: Timestamp,
  ) -> Option<Change> {
    assert!(!self.finished, "cannot commit a finished transaction");
    self.finished = true;

    if self.local_ops.is_empty() {
      return None;
    }

    let ops = RleVec::from(self.local_ops);
    let change = Change::new(
      ops,
      self.frontiers,
      crate::types::ID::new(self.peer, self.start_counter),
      self.start_lamport,
      timestamp,
    );

    oplog.import_local_change(change.clone());
    Some(change)
  }

  /// Commit with the current wall-clock time.
  ///
  /// Delegates to [`commit_with_timestamp`](Transaction::commit_with_timestamp)
  /// using `std::time::SystemTime::now()`.
  pub fn commit(self, oplog: &mut OpLog) -> Option<Change> {
    let timestamp = std::time::SystemTime::now()
      .duration_since(std::time::UNIX_EPOCH)
      .unwrap_or_default()
      .as_secs() as Timestamp;
    self.commit_with_timestamp(oplog, timestamp)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::memory::arena::InnerArena;
  use crate::op::OpContent;
  use crate::types::{ContainerID, ContainerType, CoralValue, ID};

  #[test]
  fn test_txn_new() {
    let arena = SharedArena::new(InnerArena::new());
    let frontiers = Frontiers::from_id(ID::new(0, 0));
    let txn = Transaction::new(1, 10, 5, frontiers.clone(), arena);

    assert_eq!(txn.peer, 1);
    assert_eq!(txn.start_counter, 10);
    assert_eq!(txn.next_counter, 10);
    assert_eq!(txn.start_lamport, 5);
    assert_eq!(txn.next_lamport, 5);
    assert_eq!(txn.frontiers, frontiers);
    assert!(txn.is_empty());
    assert_eq!(txn.len(), 0);
    assert!(!txn.finished);
  }

  #[test]
  fn test_txn_add_op_counter() {
    let arena = SharedArena::new(InnerArena::new());
    let container = arena.register(&ContainerID::new_root("c", ContainerType::Counter));
    let mut txn = Transaction::new(1, 0, 1, Frontiers::new(), arena);

    txn.add_op(container, OpContent::Counter(3.0));
    assert!(!txn.is_empty());
    assert_eq!(txn.len(), 1);
    assert_eq!(txn.next_counter, 1);
    assert_eq!(txn.local_ops.len(), 1);
    assert_eq!(txn.local_ops[0].counter, 0);

    txn.add_op(container, OpContent::Counter(-1.0));
    assert_eq!(txn.len(), 2);
    assert_eq!(txn.next_counter, 2);
    assert_eq!(txn.local_ops[1].counter, 1);
  }

  #[test]
  fn test_txn_apply_local_op_counter() {
    let arena = SharedArena::new(InnerArena::new());
    let container = arena.register(&ContainerID::new_root("c", ContainerType::Counter));
    let mut txn = Transaction::new(1, 5, 10, Frontiers::new(), arena);

    txn.apply_local_op(
      container,
      RawOpContent::Counter(2.5),
      Some(EventHint::Map {
        key: "k".to_string(),
        value: Some(CoralValue::from(42i32)),
      }),
    );

    assert_eq!(txn.len(), 1);
    assert_eq!(txn.next_counter, 6);
    assert_eq!(txn.event_hints.len(), 1);
  }

  #[test]
  fn test_txn_empty_commit_returns_none() {
    let arena = SharedArena::new(InnerArena::new());
    let mut oplog = OpLog::new();
    let txn = Transaction::new(1, 0, 1, Frontiers::new(), arena);

    let result = txn.commit_with_timestamp(&mut oplog, 1_700_000_000);
    assert!(result.is_none());
  }

  #[test]
  fn test_txn_commit_imports_to_oplog() {
    let arena = SharedArena::new(InnerArena::new());
    let container = arena.register(&ContainerID::new_root("c", ContainerType::Counter));
    let mut oplog = OpLog::new();
    let mut txn = Transaction::new(1, 0, 1, Frontiers::new(), arena);

    txn.add_op(container, OpContent::Counter(3.0));
    txn.add_op(container, OpContent::Counter(2.0));

    let change = txn
      .commit_with_timestamp(&mut oplog, 1_700_000_000)
      .expect("commit should produce a Change");

    // Verify the returned Change
    assert_eq!(change.peer(), 1);
    assert_eq!(change.id(), ID::new(1, 0));
    assert_eq!(change.lamport(), 1);
    assert_eq!(change.len(), 2);
    assert_eq!(change.timestamp(), 1_700_000_000);

    // Verify the OpLog received it
    assert_eq!(oplog.vv().get(1), Some(&2));
    assert_eq!(oplog.frontiers().as_single(), Some(ID::new(1, 1)));
  }

  #[test]
  fn test_txn_commit_counter_continuity() {
    let arena = SharedArena::new(InnerArena::new());
    let c1 = arena.register(&ContainerID::new_root("a", ContainerType::Counter));
    let c2 = arena.register(&ContainerID::new_root("b", ContainerType::Map));
    let mut oplog = OpLog::new();
    let mut txn = Transaction::new(1, 0, 1, Frontiers::new(), arena);

    txn.add_op(c1, OpContent::Counter(1.0));
    txn.add_op(
      c2,
      OpContent::Map(crate::container::map::MapSet {
        key: "k".into(),
        value: Some(CoralValue::from("v")),
      }),
    );

    let change = txn.commit_with_timestamp(&mut oplog, 1).unwrap();
    let ops: Vec<_> = change.ops().iter().collect();
    assert_eq!(ops.len(), 2);
    assert_eq!(ops[0].counter, 0);
    assert_eq!(ops[1].counter, 1);
    assert_eq!(ops[1].id(change.peer()), ID::new(1, 1));
  }

  #[test]
  #[should_panic(expected = "cannot add op to a finished transaction")]
  fn test_txn_add_op_after_finish_panics() {
    let arena = SharedArena::new(InnerArena::new());
    let container = arena.register(&ContainerID::new_root("c", ContainerType::Counter));
    let mut txn = Transaction::new(1, 0, 1, Frontiers::new(), arena);
    txn.finished = true; // simulate a committed/dropped transaction

    // This should panic — transaction is already finished.
    txn.add_op(container, OpContent::Counter(1.0));
  }

  #[test]
  #[should_panic(expected = "cannot commit a finished transaction")]
  fn test_txn_double_commit_panics() {
    let arena = SharedArena::new(InnerArena::new());
    let mut txn = Transaction::new(1, 0, 1, Frontiers::new(), arena);
    txn.finished = true; // simulate prior commit

    let mut oplog = OpLog::new();
    let _ = txn.commit_with_timestamp(&mut oplog, 1);
  }

  #[test]
  fn test_txn_apply_local_op_map() {
    let arena = SharedArena::new(InnerArena::new());
    let container = arena.register(&ContainerID::new_root("m", ContainerType::Map));
    let mut txn = Transaction::new(1, 0, 1, Frontiers::new(), arena);

    txn.apply_local_op(
      container,
      RawOpContent::Map(crate::container::map::MapSet {
        key: "hello".into(),
        value: Some(CoralValue::from(42i32)),
      }),
      Some(EventHint::Map {
        key: "hello".to_string(),
        value: Some(CoralValue::from(42i32)),
      }),
    );

    assert_eq!(txn.len(), 1);
    assert_eq!(txn.event_hints.len(), 1);
    match &txn.local_ops[0].content {
      OpContent::Map(set) => {
        assert_eq!(set.key, "hello");
        assert_eq!(set.value, Some(CoralValue::from(42i32)));
      }
      _ => panic!("expected Map content"),
    }
  }

  #[test]
  fn test_txn_event_hint_roundtrip() {
    let hints = vec![
      EventHint::InsertText {
        pos: 5,
        event_len: 3,
        unicode_len: 3,
      },
      EventHint::Map {
        key: "k".to_string(),
        value: None,
      },
      EventHint::List { pos: 0 },
      EventHint::Tree {},
    ];

    // Just verify the variants can be constructed and cloned.
    for hint in &hints {
      let _cloned = hint.clone();
    }
  }
}
