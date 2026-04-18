//! Operation unit — the smallest change that can be applied to a container.
//!
//! An [`Op`] targets a single container and carries a payload ([`OpContent`])
//! describing what to do.  The concrete operation types (`MapOp`, `ListOp`, …)
//! are defined in the [`content`] sub-module and re-exported here.
//!
//! # Design note
//!
//! `Op` stores only a [`Counter`](crate::types::Counter) — the peer ID is
//! implicit from the enclosing [`Change`](crate::core::change::Change).  The
//! container field uses the compact [`ContainerIdx`](crate::core::container::ContainerIdx)
//! instead of the full [`ContainerID`](crate::types::ContainerID).

mod content;

pub use content::*;

use crate::core::container::ContainerIdx;
use crate::types::{Counter, ID, PeerID};

/// A single atomic operation.
///
/// `Op` is intentionally compact: peer and lamport live on the enclosing
/// [`Change`](crate::core::change::Change), and the container is referenced by
/// a 4-byte [`ContainerIdx`] rather than the full [`ContainerID`].
#[derive(Debug, Clone)]
pub struct Op {
  /// The operation counter (peer is implicit from the Change).
  pub counter: Counter,

  /// The target container (compact arena index).
  pub container: ContainerIdx,

  /// What to do in the target container.
  pub content: OpContent,
}

impl Op {
  /// Creates a new `Op`.
  pub fn new(counter: Counter, container: ContainerIdx, content: OpContent) -> Self {
    Self {
      counter,
      container,
      content,
    }
  }

  /// Reconstructs the full [`ID`] given the peer that produced this op.
  #[inline]
  pub fn id(&self, peer: PeerID) -> ID {
    ID::new(peer, self.counter)
  }
}

/// An [`Op`] paired with its peer so that the full [`ID`] can be recovered.
///
/// This is used when iterating over ops inside a [`Change`](crate::core::change::Change),
/// where all ops share the same peer but have individual counters.
#[derive(Debug, Clone)]
pub struct OpWithId {
  pub peer: PeerID,
  pub op: Op,
}

impl OpWithId {
  /// Returns the full [`ID`] of this op.
  #[inline]
  pub fn id(&self) -> ID {
    ID::new(self.peer, self.op.counter)
  }

  /// Returns the counter span covered by this op.
  ///
  /// For now each placeholder op has atom length 1.
  pub fn id_span(&self) -> crate::version::IdSpan {
    let start = self.op.counter;
    let end = start + 1;
    crate::version::IdSpan::new(self.peer, start, end)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::arena::Arena;
  use crate::types::{ContainerID, ContainerType};

  #[test]
  fn test_op_new() {
    let mut arena = Arena::new();
    let container = arena.register(&ContainerID::new_root("my_map", ContainerType::Map));
    let op = Op::new(7, container, OpContent::Map(MapOp));
    assert_eq!(op.counter, 7);
    assert_eq!(op.container, container);
  }

  #[test]
  fn test_op_id() {
    let mut arena = Arena::new();
    let container = arena.register(&ContainerID::new_root("c", ContainerType::Counter));
    let op = Op::new(3, container, OpContent::Counter(CounterOp));
    assert_eq!(op.id(42), ID::new(42, 3));
  }

  #[test]
  fn test_op_with_id() {
    let mut arena = Arena::new();
    let container = arena.register(&ContainerID::new_root("c", ContainerType::Counter));
    let op = Op::new(5, container, OpContent::Counter(CounterOp));
    let op_with_id = OpWithId { peer: 99, op };
    assert_eq!(op_with_id.id(), ID::new(99, 5));
  }

  #[test]
  fn test_op_content_variants() {
    let mut arena = Arena::new();
    let map_c = arena.register(&ContainerID::new_root("m", ContainerType::Map));
    let list_c = arena.register(&ContainerID::new_root("l", ContainerType::List));
    let text_c = arena.register(&ContainerID::new_root("t", ContainerType::Text));
    let tree_c = arena.register(&ContainerID::new_root("tr", ContainerType::Tree));
    let counter_c = arena.register(&ContainerID::new_root("c", ContainerType::Counter));

    let _ = Op::new(0, map_c, OpContent::Map(MapOp));
    let _ = Op::new(0, list_c, OpContent::List(ListOp));
    let _ = Op::new(0, text_c, OpContent::Text(TextOp));
    let _ = Op::new(0, tree_c, OpContent::Tree(TreeOp));
    let _ = Op::new(0, counter_c, OpContent::Counter(CounterOp));
  }
}
