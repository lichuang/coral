//! Operation unit — the smallest change that can be applied to a container.
//!
//! An [`Op`] targets a single container and carries a payload ([`OpContent`])
//! describing what to do.  The concrete operation types (`MapSet`, `InnerListOp`, …)
//! are defined in sub-modules and re-exported here.
//!
//! # Design note
//!
//! `Op` stores only a [`Counter`](crate::types::Counter) — the peer ID is
//! implicit from the enclosing [`Change`](crate::core::change::Change).  The
//! container field uses the compact [`ContainerIdx`](crate::core::container::ContainerIdx)
//! instead of the full [`ContainerID`](crate::types::ContainerID).

mod content;

pub use crate::container::list::{DeleteSpan, DeleteSpanWithId, InnerListOp, ListOp, ListSlice};
pub use crate::container::map::MapSet;
pub use crate::container::tree::{FractionalIndex as TreeFractionalIndex, TreeID, TreeOp};
pub use content::{OpContent, RawOpContent};

use crate::core::container::ContainerIdx;
use crate::rle::{HasIndex, HasLength, Mergable, Sliceable};
use crate::types::{Counter, ID, Lamport, PeerID, Timestamp};

/// A range of values in the value arena.
///
/// Used by [`InnerListOp::Insert`](crate::container::list::InnerListOp::Insert)
/// to reference contiguous values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SliceRange {
  pub start: usize,
  pub end: usize,
}

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
  pub fn id_span(&self) -> crate::version::IdSpan {
    let start = self.op.counter;
    let end = start + self.op.atom_len() as i32;
    crate::version::IdSpan::new(self.peer, start, end)
  }
}

// ═══════════════════════════════════════════════════════════════════════════
// RawOp — transport / serialization form
// ═══════════════════════════════════════════════════════════════════════════

/// A raw operation in transport / serialization form.
///
/// Unlike [`Op`], `RawOp` carries the full [`ID`] (including peer) and
/// lamport directly, because it may be transmitted outside the context of
/// an enclosing [`Change`](crate::core::change::Change).
#[derive(Debug, Clone)]
pub struct RawOp<'a> {
  /// Full identifier (peer + counter).
  pub id: ID,
  /// Lamport timestamp.
  pub lamport: Lamport,
  /// Target container (compact arena index).
  pub container: ContainerIdx,
  /// Operation payload (borrowed / transport form).
  pub content: RawOpContent<'a>,
}

impl HasLength for RawOp<'_> {
  fn content_len(&self) -> usize {
    self.content.content_len()
  }
}

impl HasIndex for RawOp<'_> {
  type Int = Counter;

  fn get_start_index(&self) -> Self::Int {
    self.id.counter
  }
}

// ═══════════════════════════════════════════════════════════════════════════
// RichOp — op with full causal metadata
// ═══════════════════════════════════════════════════════════════════════════

/// A rich view of an [`Op`] with full causal metadata.
///
/// `RichOp` is produced when iterating over the operations inside a
/// [`Change`](crate::core::change::Change).  It carries the same
/// `peer`/`lamport`/`timestamp` as the enclosing change, plus `start`/`end`
/// offsets for the case where the change has been sliced and this op is
/// only partially included.
#[derive(Debug, Clone, Copy)]
pub struct RichOp<'a> {
  /// Reference to the underlying operation.
  pub op: &'a Op,
  /// Peer that produced this operation.
  pub peer: PeerID,
  /// Lamport timestamp (from the enclosing change).
  pub lamport: Lamport,
  /// Physical timestamp in seconds (from the enclosing change).
  pub timestamp: Timestamp,
  /// Inclusive start offset within the op's content.
  pub start: usize,
  /// Exclusive end offset within the op's content.
  pub end: usize,
}

impl<'a> RichOp<'a> {
  /// Creates a new `RichOp` representing the full op (start = 0, end = atom_len).
  pub fn new(op: &'a Op, peer: PeerID, lamport: Lamport, timestamp: Timestamp) -> Self {
    let end = op.atom_len();
    Self {
      op,
      peer,
      lamport,
      timestamp,
      start: 0,
      end,
    }
  }

  /// Returns the full [`ID`] of this operation.
  #[inline]
  pub fn id(&self) -> ID {
    ID::new(self.peer, self.op.counter)
  }

  /// Returns the counter span covered by this rich view.
  pub fn id_span(&self) -> crate::version::IdSpan {
    let start = self.op.counter + self.start as Counter;
    let end = self.op.counter + self.end as Counter;
    crate::version::IdSpan::new(self.peer, start, end)
  }
}

impl std::ops::Deref for RichOp<'_> {
  type Target = Op;

  fn deref(&self) -> &Self::Target {
    self.op
  }
}

impl HasLength for RichOp<'_> {
  fn content_len(&self) -> usize {
    self.end - self.start
  }
}

impl HasIndex for RichOp<'_> {
  type Int = Counter;

  fn get_start_index(&self) -> Self::Int {
    self.op.counter + self.start as Counter
  }
}

impl Sliceable for RichOp<'_> {
  fn slice(&self, from: usize, to: usize) -> Self {
    let len = self.content_len();
    assert!(
      from < to && to <= len,
      "RichOp::slice out of bounds: [{from}, {to}) for len {len}"
    );
    Self {
      op: self.op,
      peer: self.peer,
      lamport: self.lamport,
      timestamp: self.timestamp,
      start: self.start + from,
      end: self.start + to,
    }
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
  use std::sync::Arc;

  #[test]
  fn test_op_new() {
    let arena = InnerArena::new();
    let container = arena.register(&ContainerID::new_root("my_map", ContainerType::Map));
    let op = Op::new(
      7,
      container,
      OpContent::Map(MapSet {
        key: "k".into(),
        value: None,
      }),
    );
    assert_eq!(op.counter, 7);
    assert_eq!(op.container, container);
  }

  #[test]
  fn test_op_id() {
    let arena = InnerArena::new();
    let container = arena.register(&ContainerID::new_root("c", ContainerType::List));
    let op = Op::new(
      3,
      container,
      OpContent::List(InnerListOp::Insert {
        pos: 0,
        slice: SliceRange { start: 0, end: 1 },
      }),
    );
    assert_eq!(op.id(42), ID::new(42, 3));
  }

  #[test]
  fn test_op_with_id() {
    let arena = InnerArena::new();
    let container = arena.register(&ContainerID::new_root("c", ContainerType::List));
    let op = Op::new(
      5,
      container,
      OpContent::List(InnerListOp::Insert {
        pos: 0,
        slice: SliceRange { start: 0, end: 1 },
      }),
    );
    let op_with_id = OpWithId { peer: 99, op };
    assert_eq!(op_with_id.id(), ID::new(99, 5));
  }

  #[test]
  fn test_op_content_variants() {
    let arena = InnerArena::new();
    let map_c = arena.register(&ContainerID::new_root("m", ContainerType::Map));
    let list_c = arena.register(&ContainerID::new_root("l", ContainerType::List));
    let tree_c = arena.register(&ContainerID::new_root("tr", ContainerType::Tree));

    let _ = Op::new(
      0,
      map_c,
      OpContent::Map(MapSet {
        key: "k".into(),
        value: None,
      }),
    );
    let _ = Op::new(
      0,
      list_c,
      OpContent::List(InnerListOp::Insert {
        pos: 0,
        slice: SliceRange { start: 0, end: 1 },
      }),
    );
    let _ = Op::new(
      0,
      tree_c,
      OpContent::Tree(Arc::new(TreeOp::Delete {
        target: ID::new(0, 0),
      })),
    );
  }

  // ── RLE traits ───────────────────────────────────────────────────

  #[test]
  fn test_op_has_length() {
    let arena = InnerArena::new();
    let c = arena.register(&ContainerID::new_root("c", ContainerType::List));
    let op = Op::new(
      0,
      c,
      OpContent::List(InnerListOp::Insert {
        pos: 0,
        slice: SliceRange { start: 0, end: 3 },
      }),
    );
    assert_eq!(op.content_len(), 3);
    assert_eq!(op.atom_len(), 3);
  }

  #[test]
  fn test_op_sliceable_whole() {
    let arena = InnerArena::new();
    let c = arena.register(&ContainerID::new_root("c", ContainerType::List));
    let op = Op::new(
      5,
      c,
      OpContent::List(InnerListOp::Insert {
        pos: 0,
        slice: SliceRange { start: 0, end: 2 },
      }),
    );
    let sliced = op.slice(0, 2);
    assert_eq!(sliced.counter, 5);
    assert_eq!(sliced.container, c);
  }

  #[test]
  #[should_panic(expected = "Op::slice out of bounds")]
  fn test_op_sliceable_empty_range_panics() {
    let arena = InnerArena::new();
    let c = arena.register(&ContainerID::new_root("c", ContainerType::List));
    let op = Op::new(
      0,
      c,
      OpContent::List(InnerListOp::Insert {
        pos: 0,
        slice: SliceRange { start: 0, end: 1 },
      }),
    );
    let _ = op.slice(0, 0); // empty range, should panic on assert in Op::slice
  }

  #[test]
  #[should_panic(expected = "OpContent::slice: Map/Tree/Counter are atomic")]
  fn test_op_content_sliceable_panics() {
    let content = OpContent::Map(MapSet {
      key: "k".into(),
      value: None,
    });
    let _ = content.slice(0, 0); // empty range on atomic content
  }

  #[test]
  fn test_op_mergable_same_container_contiguous() {
    let arena = InnerArena::new();
    let c = arena.register(&ContainerID::new_root("c", ContainerType::List));
    let a = Op::new(
      0,
      c,
      OpContent::List(InnerListOp::Insert {
        pos: 0,
        slice: SliceRange { start: 0, end: 2 },
      }),
    );
    let b = Op::new(
      2,
      c,
      OpContent::List(InnerListOp::Insert {
        pos: 2,
        slice: SliceRange { start: 2, end: 4 },
      }),
    );
    assert!(a.is_mergable(&b, &()));
  }

  #[test]
  fn test_op_not_mergable_different_container() {
    let arena = InnerArena::new();
    let c1 = arena.register(&ContainerID::new_root("a", ContainerType::List));
    let c2 = arena.register(&ContainerID::new_root("b", ContainerType::List));
    let a = Op::new(
      0,
      c1,
      OpContent::List(InnerListOp::Insert {
        pos: 0,
        slice: SliceRange { start: 0, end: 1 },
      }),
    );
    let b = Op::new(
      1,
      c2,
      OpContent::List(InnerListOp::Insert {
        pos: 0,
        slice: SliceRange { start: 0, end: 1 },
      }),
    );
    assert!(!a.is_mergable(&b, &()));
  }

  #[test]
  fn test_op_not_mergable_non_contiguous() {
    let arena = InnerArena::new();
    let c = arena.register(&ContainerID::new_root("c", ContainerType::List));
    let a = Op::new(
      0,
      c,
      OpContent::List(InnerListOp::Insert {
        pos: 0,
        slice: SliceRange { start: 0, end: 1 },
      }),
    );
    let b = Op::new(
      2,
      c,
      OpContent::List(InnerListOp::Insert {
        pos: 0,
        slice: SliceRange { start: 0, end: 1 },
      }),
    );
    assert!(!a.is_mergable(&b, &()));
  }

  #[test]
  fn test_op_not_mergable_different_variant() {
    let arena = InnerArena::new();
    let c1 = arena.register(&ContainerID::new_root("c", ContainerType::List));
    let c2 = arena.register(&ContainerID::new_root("m", ContainerType::Map));
    let a = Op::new(
      0,
      c1,
      OpContent::List(InnerListOp::Insert {
        pos: 0,
        slice: SliceRange { start: 0, end: 1 },
      }),
    );
    let b = Op::new(
      1,
      c2,
      OpContent::Map(MapSet {
        key: "k".into(),
        value: None,
      }),
    );
    assert!(!a.is_mergable(&b, &()));
  }

  // ── RawOp tests ───────────────────────────────────────────────────────────

  #[test]
  fn test_raw_op_has_length_and_index() {
    let arena = InnerArena::new();
    let c = arena.register(&ContainerID::new_root("c", ContainerType::List));
    let raw = RawOp {
      id: ID::new(1, 10),
      lamport: 5,
      container: c,
      content: RawOpContent::Counter(3.14),
    };
    assert_eq!(raw.content_len(), 1);
    assert_eq!(raw.get_start_index(), 10);
  }

  #[test]
  fn test_raw_op_list_content_len() {
    let arena = InnerArena::new();
    let c = arena.register(&ContainerID::new_root("c", ContainerType::List));
    let raw = RawOp {
      id: ID::new(1, 0),
      lamport: 1,
      container: c,
      content: RawOpContent::List(ListOp::Insert {
        pos: 0,
        slice: crate::container::list::ListSlice::RawData(std::borrow::Cow::Owned(vec![
          crate::types::CoralValue::from(1i32),
          crate::types::CoralValue::from(2i32),
        ])),
      }),
    };
    assert_eq!(raw.content_len(), 2);
  }

  // ── RichOp tests ──────────────────────────────────────────────────────────

  #[test]
  fn test_rich_op_deref_and_id() {
    let arena = InnerArena::new();
    let c = arena.register(&ContainerID::new_root("c", ContainerType::Map));
    let op = Op::new(
      7,
      c,
      OpContent::Map(MapSet {
        key: "k".into(),
        value: None,
      }),
    );
    let rich = RichOp::new(&op, 42, 100, 1_700_000_000);
    assert_eq!(rich.id(), ID::new(42, 7));
    assert_eq!(rich.counter, 7); // via Deref
    assert_eq!(rich.container, c); // via Deref
  }

  #[test]
  fn test_rich_op_has_length_and_index() {
    let arena = InnerArena::new();
    let c = arena.register(&ContainerID::new_root("c", ContainerType::List));
    let op = Op::new(
      10,
      c,
      OpContent::List(InnerListOp::Insert {
        pos: 0,
        slice: SliceRange { start: 0, end: 3 },
      }),
    );
    let rich = RichOp::new(&op, 1, 5, 0);
    assert_eq!(rich.content_len(), 3);
    assert_eq!(rich.get_start_index(), 10);

    // slice to [1, 3)
    let sliced = rich.slice(1, 3);
    assert_eq!(sliced.content_len(), 2);
    assert_eq!(sliced.get_start_index(), 11);
    assert_eq!(sliced.start, 1);
    assert_eq!(sliced.end, 3);
  }

  #[test]
  fn test_rich_op_id_span() {
    let arena = InnerArena::new();
    let c = arena.register(&ContainerID::new_root("c", ContainerType::List));
    let op = Op::new(
      5,
      c,
      OpContent::List(InnerListOp::Insert {
        pos: 0,
        slice: SliceRange { start: 0, end: 4 },
      }),
    );
    let rich = RichOp::new(&op, 99, 10, 0);
    let span = rich.id_span();
    assert_eq!(span.peer, 99);
    assert_eq!(span.counter.start, 5);
    assert_eq!(span.counter.end, 9);

    // sliced rich op
    let sliced = rich.slice(1, 3);
    let span2 = sliced.id_span();
    assert_eq!(span2.counter.start, 6);
    assert_eq!(span2.counter.end, 8);
  }

  #[test]
  #[should_panic(expected = "RichOp::slice out of bounds")]
  fn test_rich_op_slice_panics() {
    let arena = InnerArena::new();
    let c = arena.register(&ContainerID::new_root("c", ContainerType::Map));
    let op = Op::new(
      0,
      c,
      OpContent::Map(MapSet {
        key: "k".into(),
        value: None,
      }),
    );
    let rich = RichOp::new(&op, 1, 1, 0);
    let _ = rich.slice(0, 2); // atomic op has len 1, slicing [0,2) is out of bounds
  }
}
