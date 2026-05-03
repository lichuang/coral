//! Transaction structure and lifecycle methods.

use crate::core::change::Change;
use crate::core::container::ContainerIdx;
use crate::doc::CoralDoc;
use crate::memory::arena::SharedArena;
use crate::op::{Op, OpContent, RawOpContent};
use crate::rle::{HasLength, RleVec};
use crate::types::{Counter, ID, Lamport, PeerID, Timestamp};
use crate::version::Frontiers;

use super::EventHint;

/// A local editing transaction — buffers ops and commits them as a single Change.
///
/// # Lifecycle
///
/// 1. **Create** — [`Transaction::new`] locks the doc and allocates
///    counter/lamport range.
/// 2. **Apply ops** — [`apply_local_op`](Transaction::apply_local_op)
///    converts raw content, applies it to [`DocState`](crate::doc::DocState),
///    and buffers the op.
/// 3. **Commit** — [`commit`](Transaction::commit) builds a [`Change`],
///    imports it into the [`OpLog`](crate::oplog::OpLog), finalises state,
///    and unlocks the doc.
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
  /// Optional origin tag (e.g. `"user"`, `"sync"`).
  pub origin: Option<String>,
  /// Optional callback run after the transaction is successfully committed.
  pub on_commit: Option<Box<dyn FnOnce()>>,
}

impl Transaction {
  /// Creates a new transaction bound to the given document.
  ///
  /// # Panics
  ///
  /// Panics if another transaction is already active on `doc`.
  pub fn new(doc: &mut CoralDoc, origin: Option<String>) -> Self {
    assert!(
      !doc.txn_in_progress,
      "another transaction is already in progress"
    );
    doc.txn_in_progress = true;

    let peer = doc.peer_id;
    let frontiers = doc.oplog.frontiers().clone();
    let next_id = doc.oplog.next_id(peer);
    let lamport = doc
      .oplog
      .dag
      .get_change_lamport_from_deps(&frontiers)
      .unwrap_or(0);

    Self {
      peer,
      start_counter: next_id.counter,
      next_counter: next_id.counter,
      start_lamport: lamport,
      next_lamport: lamport,
      frontiers,
      local_ops: Vec::new(),
      arena: doc.arena.clone(),
      finished: false,
      event_hints: Vec::new(),
      origin,
      on_commit: None,
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
  /// [`Op`], applies it to `doc.state`, and buffers it.
  ///
  /// # Panics
  ///
  /// Panics if the transaction has already been finished.
  pub fn apply_local_op(
    &mut self,
    doc: &mut CoralDoc,
    container: ContainerIdx,
    raw_content: RawOpContent<'_>,
    event_hint: Option<EventHint>,
  ) {
    assert!(!self.finished, "cannot apply op to a finished transaction");

    let content = raw_content.to_op_content(&self.arena);
    let op = Op::new(self.next_counter, container, content);
    self.next_counter += op.atom_len() as Counter;

    // Apply to live state.
    doc.state.apply_local_op(&op, &self.arena);

    self.local_ops.push(op);

    if let Some(hint) = event_hint {
      self.event_hints.push(hint);
    }
  }

  /// Commit this transaction, producing a [`Change`] and importing it into the doc.
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
    doc: &mut CoralDoc,
    timestamp: Timestamp,
  ) -> Option<Change> {
    assert!(!self.finished, "cannot commit a finished transaction");
    self.finished = true;
    doc.txn_in_progress = false;

    if self.local_ops.is_empty() {
      return None;
    }

    let ops = RleVec::from(self.local_ops);
    let change = Change::new(
      ops,
      self.frontiers,
      ID::new(self.peer, self.start_counter),
      self.start_lamport,
      timestamp,
    );

    doc.oplog.import_local_change(change.clone());
    doc.state.commit();

    if let Some(cb) = self.on_commit.take() {
      cb();
    }

    Some(change)
  }

  /// Commit with the current wall-clock time.
  ///
  /// Delegates to [`commit_with_timestamp`](Transaction::commit_with_timestamp)
  /// using `std::time::SystemTime::now()`.
  pub fn commit(self, doc: &mut CoralDoc) -> Option<Change> {
    let timestamp = std::time::SystemTime::now()
      .duration_since(std::time::UNIX_EPOCH)
      .unwrap_or_default()
      .as_secs() as Timestamp;
    self.commit_with_timestamp(doc, timestamp)
  }
}

impl std::fmt::Debug for Transaction {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("Transaction")
      .field("peer", &self.peer)
      .field("start_counter", &self.start_counter)
      .field("next_counter", &self.next_counter)
      .field("start_lamport", &self.start_lamport)
      .field("next_lamport", &self.next_lamport)
      .field("frontiers", &self.frontiers)
      .field("local_ops", &self.local_ops)
      .field("arena", &self.arena)
      .field("finished", &self.finished)
      .field("event_hints", &self.event_hints)
      .field("origin", &self.origin)
      .field("on_commit", &self.on_commit.is_some())
      .finish()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::op::OpContent;
  use crate::types::{ContainerID, ContainerType, CoralValue, ID};
  use std::sync::Arc;
  use std::sync::atomic::{AtomicBool, Ordering};

  #[test]
  fn test_txn_new() {
    let mut doc = CoralDoc::new(1);
    let frontiers = doc.oplog.frontiers().clone();
    let txn = Transaction::new(&mut doc, None);

    assert_eq!(txn.peer, 1);
    assert_eq!(txn.start_counter, 0);
    assert_eq!(txn.next_counter, 0);
    assert_eq!(txn.start_lamport, 0);
    assert_eq!(txn.next_lamport, 0);
    assert_eq!(txn.frontiers, frontiers);
    assert!(txn.is_empty());
    assert_eq!(txn.len(), 0);
    assert!(!txn.finished);
    assert!(doc.txn_in_progress);
  }

  #[test]
  fn test_txn_new_with_origin() {
    let mut doc = CoralDoc::new(1);
    let txn = Transaction::new(&mut doc, Some("user".to_string()));
    assert_eq!(txn.origin, Some("user".to_string()));
  }

  #[test]
  #[should_panic(expected = "another transaction is already in progress")]
  fn test_txn_double_new_panics() {
    let mut doc = CoralDoc::new(1);
    let _txn1 = Transaction::new(&mut doc, None);
    let _txn2 = Transaction::new(&mut doc, None);
  }

  #[test]
  fn test_txn_add_op_counter() {
    let mut doc = CoralDoc::new(1);
    let container = doc
      .arena
      .register(&ContainerID::new_root("c", ContainerType::Counter));
    let mut txn = Transaction::new(&mut doc, None);

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
    let mut doc = CoralDoc::new(1);
    let container = doc
      .arena
      .register(&ContainerID::new_root("c", ContainerType::Counter));
    let mut txn = Transaction::new(&mut doc, None);

    txn.apply_local_op(
      &mut doc,
      container,
      RawOpContent::Counter(2.5),
      Some(EventHint::Map {
        key: "k".to_string(),
        value: Some(CoralValue::from(42i32)),
      }),
    );

    assert_eq!(txn.len(), 1);
    assert_eq!(txn.next_counter, 1);
    assert_eq!(txn.event_hints.len(), 1);
  }

  #[test]
  fn test_txn_empty_commit_returns_none() {
    let mut doc = CoralDoc::new(1);
    let txn = Transaction::new(&mut doc, None);
    assert!(doc.txn_in_progress);

    let result = txn.commit_with_timestamp(&mut doc, 1_700_000_000);
    assert!(result.is_none());
    assert!(!doc.txn_in_progress);
  }

  #[test]
  fn test_txn_commit_imports_to_oplog() {
    let mut doc = CoralDoc::new(1);
    let container = doc
      .arena
      .register(&ContainerID::new_root("c", ContainerType::Counter));
    let mut txn = Transaction::new(&mut doc, None);

    txn.add_op(container, OpContent::Counter(3.0));
    txn.add_op(container, OpContent::Counter(2.0));

    let change = txn
      .commit_with_timestamp(&mut doc, 1_700_000_000)
      .expect("commit should produce a Change");

    // Verify the returned Change
    assert_eq!(change.peer(), 1);
    assert_eq!(change.id(), ID::new(1, 0));
    assert_eq!(change.lamport(), 0);
    assert_eq!(change.len(), 2);
    assert_eq!(change.timestamp(), 1_700_000_000);

    // Verify the OpLog received it
    assert_eq!(doc.oplog.vv().get(1), Some(&2));
    assert_eq!(doc.oplog.frontiers().as_single(), Some(ID::new(1, 1)));

    // Verify the doc is unlocked
    assert!(!doc.txn_in_progress);
  }

  #[test]
  fn test_txn_commit_counter_continuity() {
    let mut doc = CoralDoc::new(1);
    let c1 = doc
      .arena
      .register(&ContainerID::new_root("a", ContainerType::Counter));
    let c2 = doc
      .arena
      .register(&ContainerID::new_root("b", ContainerType::Map));
    let mut txn = Transaction::new(&mut doc, None);

    txn.add_op(c1, OpContent::Counter(1.0));
    txn.add_op(
      c2,
      OpContent::Map(crate::container::map::MapSet {
        key: "k".into(),
        value: Some(CoralValue::from("v")),
      }),
    );

    let change = txn.commit_with_timestamp(&mut doc, 1).unwrap();
    let ops: Vec<_> = change.ops().iter().collect();
    assert_eq!(ops.len(), 2);
    assert_eq!(ops[0].counter, 0);
    assert_eq!(ops[1].counter, 1);
    assert_eq!(ops[1].id(change.peer()), ID::new(1, 1));
  }

  #[test]
  #[should_panic(expected = "cannot add op to a finished transaction")]
  fn test_txn_add_op_after_finish_panics() {
    let mut doc = CoralDoc::new(1);
    let container = doc
      .arena
      .register(&ContainerID::new_root("c", ContainerType::Counter));
    let mut txn = Transaction::new(&mut doc, None);
    txn.finished = true; // simulate a committed/dropped transaction

    // This should panic — transaction is already finished.
    txn.add_op(container, OpContent::Counter(1.0));
  }

  #[test]
  #[should_panic(expected = "cannot commit a finished transaction")]
  fn test_txn_double_commit_panics() {
    let mut doc = CoralDoc::new(1);
    let mut txn = Transaction::new(&mut doc, None);
    txn.finished = true; // simulate prior commit

    let _ = txn.commit_with_timestamp(&mut doc, 1);
  }

  #[test]
  fn test_txn_apply_local_op_map() {
    let mut doc = CoralDoc::new(1);
    let container = doc
      .arena
      .register(&ContainerID::new_root("m", ContainerType::Map));
    let mut txn = Transaction::new(&mut doc, None);

    txn.apply_local_op(
      &mut doc,
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

  #[test]
  fn test_txn_on_commit_callback() {
    let mut doc = CoralDoc::new(1);
    let container = doc
      .arena
      .register(&ContainerID::new_root("c", ContainerType::Counter));
    let mut txn = Transaction::new(&mut doc, None);

    let called = Arc::new(AtomicBool::new(false));
    let called_clone = Arc::clone(&called);
    txn.on_commit = Some(Box::new(move || {
      called_clone.store(true, Ordering::SeqCst);
    }));

    txn.add_op(container, OpContent::Counter(1.0));
    let _ = txn.commit_with_timestamp(&mut doc, 1);

    assert!(called.load(Ordering::SeqCst));
  }

  #[test]
  fn test_txn_commit_unlocks_doc() {
    let mut doc = CoralDoc::new(1);
    let _container = doc
      .arena
      .register(&ContainerID::new_root("c", ContainerType::Counter));
    let txn = Transaction::new(&mut doc, None);
    assert!(doc.txn_in_progress);

    let _ = txn.commit_with_timestamp(&mut doc, 1);
    assert!(!doc.txn_in_progress);

    // A new transaction can now be created.
    let _txn2 = Transaction::new(&mut doc, None);
  }

  #[test]
  fn test_two_consecutive_txns() {
    let mut doc = CoralDoc::new(1);
    let c = doc
      .arena
      .register(&ContainerID::new_root("c", ContainerType::Counter));

    // First transaction: counters 0..1
    let mut txn1 = Transaction::new(&mut doc, None);
    txn1.add_op(c, OpContent::Counter(1.0));
    let change1 = txn1.commit_with_timestamp(&mut doc, 1).unwrap();
    assert_eq!(change1.id(), ID::new(1, 0));
    assert_eq!(change1.len(), 1);

    // Second transaction: counters 1..2
    let mut txn2 = Transaction::new(&mut doc, None);
    txn2.add_op(c, OpContent::Counter(2.0));
    let change2 = txn2.commit_with_timestamp(&mut doc, 2).unwrap();
    assert_eq!(change2.id(), ID::new(1, 1));
    assert_eq!(change2.len(), 1);

    // Verify deps: second transaction depends on the first.
    assert_eq!(change2.deps().as_single(), Some(ID::new(1, 0)));

    // Verify OpLog state
    assert_eq!(doc.oplog.vv().get(1), Some(&2));
    assert_eq!(doc.oplog.frontiers().as_single(), Some(ID::new(1, 1)));
  }
}
