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
use crate::rle::{HasIndex, HasLength, Mergable, Sliceable};
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

  /// Inclusive start counter of this op.
  #[inline]
  pub fn ctr_start(&self) -> Counter {
    self.counter
  }

  /// Exclusive end counter of this op.
  #[inline]
  pub fn ctr_end(&self) -> Counter {
    self.counter + self.atom_len() as Counter
  }
}

impl HasIndex for Op {
  type Int = Counter;

  fn get_start_index(&self) -> Self::Int {
    self.counter
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

// ── RLE traits for Op ──────────────────────────────────────────────────────

impl HasLength for Op {
  fn content_len(&self) -> usize {
    self.content.content_len()
  }
}

impl Sliceable for Op {
  fn slice(&self, from: usize, to: usize) -> Self {
    let len = self.atom_len();
    assert!(
      from < to && to <= len,
      "Op::slice out of bounds: [{from}, {to}) for len {len}"
    );
    if from == 0 && to == len {
      self.clone()
    } else {
      Self {
        counter: self.counter,
        container: self.container,
        content: self.content.slice(from, to),
      }
    }
  }
}

impl Mergable for Op {
  fn is_mergable(&self, other: &Self, _conf: &()) -> bool {
    self.container == other.container
      && self.counter + self.atom_len() as Counter == other.counter
      && std::mem::discriminant(&self.content) == std::mem::discriminant(&other.content)
      && self.content.is_mergable(&other.content, &())
  }

  fn merge(&mut self, other: &Self, _conf: &()) {
    assert!(
      self.is_mergable(other, &()),
      "cannot merge Ops: counters={}..{}, discriminant mismatch, or content not mergable",
      self.counter,
      other.counter
    );
    self.content.merge(&other.content, &());
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::memory::arena::InnerArena;
  use crate::types::{ContainerID, ContainerType};

  #[test]
  fn test_op_new() {
    let arena = InnerArena::new();
    let container = arena.register(&ContainerID::new_root("my_map", ContainerType::Map));
    let op = Op::new(7, container, OpContent::Map(MapOp));
    assert_eq!(op.counter, 7);
    assert_eq!(op.container, container);
  }

  #[test]
  fn test_op_id() {
    let arena = InnerArena::new();
    let container = arena.register(&ContainerID::new_root("c", ContainerType::Counter));
    let op = Op::new(3, container, OpContent::Counter(CounterOp));
    assert_eq!(op.id(42), ID::new(42, 3));
  }

  #[test]
  fn test_op_with_id() {
    let arena = InnerArena::new();
    let container = arena.register(&ContainerID::new_root("c", ContainerType::Counter));
    let op = Op::new(5, container, OpContent::Counter(CounterOp));
    let op_with_id = OpWithId { peer: 99, op };
    assert_eq!(op_with_id.id(), ID::new(99, 5));
  }

  #[test]
  fn test_op_content_variants() {
    let arena = InnerArena::new();
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

  // ── RLE traits ───────────────────────────────────────────────────

  #[test]
  fn test_op_has_length() {
    let arena = InnerArena::new();
    let c = arena.register(&ContainerID::new_root("c", ContainerType::Counter));
    let op = Op::new(0, c, OpContent::Counter(CounterOp));
    assert_eq!(op.content_len(), 1);
    assert_eq!(op.atom_len(), 1);
  }

  #[test]
  fn test_op_sliceable_whole() {
    let arena = InnerArena::new();
    let c = arena.register(&ContainerID::new_root("c", ContainerType::Counter));
    let op = Op::new(5, c, OpContent::Counter(CounterOp));
    let sliced = op.slice(0, 1);
    assert_eq!(sliced.counter, 5);
    assert_eq!(sliced.container, c);
  }

  #[test]
  #[should_panic(expected = "Op::slice out of bounds")]
  fn test_op_sliceable_empty_range_panics() {
    let arena = InnerArena::new();
    let c = arena.register(&ContainerID::new_root("c", ContainerType::Counter));
    let op = Op::new(0, c, OpContent::Counter(CounterOp));
    let _ = op.slice(0, 0); // empty range, should panic on assert in Op::slice
  }

  #[test]
  #[should_panic(expected = "OpContent is atomic")]
  fn test_op_content_sliceable_panics() {
    let content = OpContent::Counter(CounterOp);
    let _ = content.slice(0, 0); // empty range on atomic content
  }

  #[test]
  fn test_op_mergable_same_container_contiguous() {
    let arena = InnerArena::new();
    let c = arena.register(&ContainerID::new_root("c", ContainerType::Counter));
    let a = Op::new(0, c, OpContent::Counter(CounterOp));
    let b = Op::new(1, c, OpContent::Counter(CounterOp));
    // Placeholder content is not mergable (default false), so Op::is_mergable returns false.
    assert!(!a.is_mergable(&b, &()));
  }

  #[test]
  fn test_op_not_mergable_different_container() {
    let arena = InnerArena::new();
    let c1 = arena.register(&ContainerID::new_root("a", ContainerType::Counter));
    let c2 = arena.register(&ContainerID::new_root("b", ContainerType::Counter));
    let a = Op::new(0, c1, OpContent::Counter(CounterOp));
    let b = Op::new(1, c2, OpContent::Counter(CounterOp));
    assert!(!a.is_mergable(&b, &()));
  }

  #[test]
  fn test_op_not_mergable_non_contiguous() {
    let arena = InnerArena::new();
    let c = arena.register(&ContainerID::new_root("c", ContainerType::Counter));
    let a = Op::new(0, c, OpContent::Counter(CounterOp));
    let b = Op::new(2, c, OpContent::Counter(CounterOp));
    assert!(!a.is_mergable(&b, &()));
  }

  #[test]
  fn test_op_not_mergable_different_variant() {
    let arena = InnerArena::new();
    let c1 = arena.register(&ContainerID::new_root("c", ContainerType::Counter));
    let c2 = arena.register(&ContainerID::new_root("m", ContainerType::Map));
    let a = Op::new(0, c1, OpContent::Counter(CounterOp));
    let b = Op::new(1, c2, OpContent::Map(MapOp));
    assert!(!a.is_mergable(&b, &()));
  }
}
