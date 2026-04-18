//! Change — a group of operations produced by a single transaction.
//!
//! A [`Change`] is the atomic unit of history.  All [`Op`]s in a Change
//! share the same `deps`, `lamport`, and `timestamp`.  The causal graph
//! (DAG) is built from Change to Change, not from Op to Op.

use crate::op::Op;
use crate::types::{ID, Lamport};
use crate::version::Frontiers;

/// A group of operations produced by a single transaction commit.
///
/// # Invariants
///
/// - All ops belong to the same peer (`id.peer`).
/// - Op IDs are contiguous: the nth op has counter `id.counter + n`.
/// - `deps` points to the end of predecessor Changes (never to an individual Op).
#[derive(Debug, Clone)]
pub struct Change {
  /// ID of the first Op in this change.
  pub id: ID,

  /// Lamport timestamp.  Monotonically increases across all Changes
  /// in a peer's history.
  pub lamport: Lamport,

  /// Physical timestamp in milliseconds (Unix epoch).
  pub timestamp: i64,

  /// Direct predecessors in the causal DAG.
  pub deps: Frontiers,

  /// The operations contained in this change.
  pub ops: Vec<Op>,
}

impl Change {
  /// Creates a new `Change`.
  pub fn new(id: ID, lamport: Lamport, timestamp: i64, deps: Frontiers, ops: Vec<Op>) -> Self {
    Self {
      id,
      lamport,
      timestamp,
      deps,
      ops,
    }
  }

  /// Returns the peer that produced this change.
  #[inline]
  pub fn peer(&self) -> u64 {
    self.id.peer
  }

  /// Total number of atomic operations.
  #[inline]
  pub fn len(&self) -> usize {
    self.ops.len()
  }

  #[inline]
  pub fn is_empty(&self) -> bool {
    self.ops.is_empty()
  }

  /// The ID of the last Op in this change.
  ///
  /// For an empty change this is the same as `id`.
  #[inline]
  pub fn id_last(&self) -> ID {
    if self.ops.is_empty() {
      self.id
    } else {
      self.id.inc(self.ops.len() as i32 - 1)
    }
  }

  /// The exclusive end ID (first ID after this change).
  #[inline]
  pub fn id_end(&self) -> ID {
    self.id.inc(self.ops.len() as i32)
  }

  /// Returns `true` if `id` falls inside this change's counter range.
  #[inline]
  pub fn contains_id(&self, id: ID) -> bool {
    if self.ops.is_empty() {
      id == self.id
    } else {
      id.peer == self.id.peer && id.counter >= self.id.counter && id.counter < self.id_end().counter
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::arena::Arena;
  use crate::op::{CounterOp, Op, OpContent};
  use crate::types::{ContainerID, ContainerType};

  #[test]
  fn test_change_new() {
    let mut arena = Arena::new();
    let id = ID::new(1, 0);
    let deps = Frontiers::from_id(ID::new(0, 0));
    let container = arena.register(&ContainerID::new_root("counter", ContainerType::Counter));
    let op = Op::new(0, container, OpContent::Counter(CounterOp));
    let change = Change::new(id, 5, 1_700_000_000_000, deps.clone(), vec![op]);
    assert_eq!(change.peer(), 1);
    assert_eq!(change.lamport, 5);
    assert_eq!(change.timestamp, 1_700_000_000_000);
    assert_eq!(change.deps, deps);
    assert_eq!(change.len(), 1);
  }

  #[test]
  fn test_change_empty() {
    let change = Change::new(ID::new(1, 0), 1, 0, Frontiers::new(), vec![]);
    assert!(change.is_empty());
  }
}
