//! Change — a group of operations produced by a single transaction.
//!
//! A [`Change`] is the atomic unit of history.  All [`Op`]s in a Change
//! share the same `deps`, `lamport`, and `timestamp`.  The causal graph
//! (DAG) is built from Change to Change, not from Op to Op.

use std::fmt::Debug;
use std::sync::Arc;

use crate::core::dag::DagNode;
use crate::op::Op;
use crate::rle::{HasIndex, HasLength, Mergable, RleVec, Sliceable};
use crate::types::{Counter, ID, Lamport, PeerID, Timestamp};
use crate::version::Frontiers;
use num::traits::AsPrimitive;

/// A group of operations produced by a single transaction commit.
///
/// # Invariants
///
/// - All ops belong to the same peer (`id.peer`).
/// - Op IDs are contiguous: the nth op has counter `id.counter + n`.
/// - `deps` points to the end of predecessor Changes (never to an individual Op).
#[derive(Debug, Clone, PartialEq)]
pub struct Change<O = Op> {
  /// id of the first op in the change
  pub(crate) id: ID,
  /// Lamport timestamp of the change. It can be calculated from deps
  pub(crate) lamport: Lamport,
  pub(crate) deps: Frontiers,
  /// Physical timestamp in seconds (Unix epoch).
  pub(crate) timestamp: Timestamp,
  pub(crate) commit_msg: Option<Arc<str>>,
  /// The operations contained in this change.
  pub(crate) ops: RleVec<[O; 1]>,
}

impl<O> Change<O> {
  pub fn new(
    ops: RleVec<[O; 1]>,
    deps: Frontiers,
    id: ID,
    lamport: Lamport,
    timestamp: Timestamp,
  ) -> Self {
    Change {
      ops,
      deps,
      id,
      lamport,
      timestamp,
      commit_msg: None,
    }
  }

  #[inline]
  pub fn ops(&self) -> &RleVec<[O; 1]> {
    &self.ops
  }

  #[inline]
  pub fn deps(&self) -> &Frontiers {
    &self.deps
  }

  #[inline]
  pub fn peer(&self) -> PeerID {
    self.id.peer
  }

  #[inline]
  pub fn lamport(&self) -> Lamport {
    self.lamport
  }

  #[inline]
  pub fn timestamp(&self) -> Timestamp {
    self.timestamp
  }

  #[inline]
  pub fn id(&self) -> ID {
    self.id
  }

  #[inline]
  pub fn deps_on_self(&self) -> bool {
    if let Some(id) = self.deps.as_single() {
      id.peer == self.id.peer
    } else {
      false
    }
  }

  pub fn message(&self) -> Option<&Arc<str>> {
    self.commit_msg.as_ref()
  }
}

impl<O: Mergable + HasLength + HasIndex + Debug> Change<O> {
  /// Total number of atomic operations.
  #[inline]
  pub fn len(&self) -> usize {
    self.ops.span().as_()
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
      self.id.inc(self.len() as i32 - 1)
    }
  }

  /// The exclusive end ID (first ID after this change).
  #[inline]
  pub fn id_end(&self) -> ID {
    self.id.inc(self.len() as i32)
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

  /// Whether this change can be merged with `other` on the right.
  pub fn can_merge_right(&self, other: &Self, merge_interval: Timestamp) -> bool {
    if other.id.peer == self.id.peer
      && other.id.counter == self.id.counter + self.content_len() as Counter
      && other.deps.len() == 1
      && other.deps.as_single().unwrap().peer == self.id.peer
      && other.timestamp - self.timestamp <= merge_interval
      && self.commit_msg == other.commit_msg
    {
      debug_assert!(other.timestamp >= self.timestamp);
      debug_assert!(other.lamport == self.lamport + self.len() as Lamport);
      true
    } else {
      false
    }
  }
}

// ── DagNode trait for Change ───────────────────────────────────────────────

impl<O: Mergable + HasLength + HasIndex<Int = Counter> + Sliceable + Debug> DagNode for Change<O> {
  fn deps(&self) -> &Frontiers {
    &self.deps
  }

  fn lamport(&self) -> Lamport {
    self.lamport
  }

  fn id_start(&self) -> ID {
    self.id
  }

  fn len(&self) -> usize {
    self.len()
  }
}

impl<O> crate::core::dag::HasId for Change<O> {
  fn id(&self) -> ID {
    self.id
  }
}

impl<O> crate::core::dag::HasCounter for Change<O> {
  fn counter(&self) -> Counter {
    self.id.counter
  }
}

impl<O> crate::core::dag::HasLamport for Change<O> {
  fn lamport(&self) -> Lamport {
    self.lamport
  }
}

// ── RLE traits for Change ──────────────────────────────────────────────────

impl<O: Mergable + HasLength + HasIndex + Debug> HasLength for Change<O> {
  fn content_len(&self) -> usize {
    self.ops.span().as_()
  }
}

impl<O: Mergable + HasLength + HasIndex + Debug> HasIndex for Change<O> {
  type Int = Counter;

  fn get_start_index(&self) -> Self::Int {
    self.id.counter
  }
}

/// Change is never merged at the RLE level.
///
/// Two adjacent `Change`s may be *coalesced* by the `OpLog` layer
/// (see [`can_merge_right`](Change::can_merge_right)), but that is a
/// higher-level operation that rebuilds the `Change` object rather than
/// calling `Mergable::merge`.  Therefore `is_mergable` always returns `false`
/// and `merge` is intentionally unreachable.
impl<O> Mergable for Change<O> {
  /// Always returns `false` — Changes are not RLE-merged in place.
  fn is_mergable(&self, _other: &Self, _conf: &()) -> bool
  where
    Self: Sized,
  {
    false
  }

  /// # Panics
  ///
  /// This method is never called because [`is_mergable`](Mergable::is_mergable)
  /// always returns `false`.  If it is ever invoked, it indicates a logic error
  /// in the caller (e.g. `RleVec::push` not checking `is_mergable` first).
  fn merge(&mut self, _other: &Self, _conf: &())
  where
    Self: Sized,
  {
    unreachable!("Change::merge should never be called: is_mergable always returns false")
  }
}

/// Slicing a `Change` extracts a sub-range of its ops as a new `Change`.
///
/// This is the primitive used by checkout / time-travel: when rewinding to a
/// version that falls in the middle of a `Change`, we need to cut out the
/// prefix or suffix and produce a valid new `Change` whose metadata is
/// consistent with the smaller op set.
///
/// # Metadata adjustments
///
/// - `id`      → `self.id.inc(from)` (first op of the slice becomes the new id)
/// - `lamport` → `self.lamport + from as Lamport`
/// - `deps`    → if `from == 0` keep original deps; otherwise depend on the
///   op just before the slice start (`self.id.inc(from - 1)`)
/// - `timestamp` and `commit_msg` are carried over unchanged
///
/// # Invariants checked at runtime
///
/// - `from < to`
/// - `to <= self.atom_len()`
/// - First sliced op starts exactly at `from_counter`
/// - Last sliced op ends exactly at `to_counter`
impl<O: Mergable + HasLength + HasIndex<Int = Counter> + Sliceable + Debug> Sliceable
  for Change<O>
{
  /// Returns a new `Change` containing only ops in the atom range `[from, to)`.
  fn slice(&self, from: usize, to: usize) -> Self {
    assert!(from < to);
    assert!(to <= self.atom_len());

    // Convert atom offsets into absolute counters.
    let from_counter = self.id.counter + from as Counter;
    let to_counter = self.id.counter + to as Counter;

    let ops = {
      if from >= to {
        RleVec::new()
      } else {
        let mut ans: smallvec::SmallVec<[O; 1]> = smallvec::SmallVec::new();
        let mut start_index = 0;

        // Fast path: binary search to locate the first op that intersects
        // the slice range. Only worthwhile when there are many runs.
        if self.ops.len() >= 8 {
          let result = self
            .ops
            .binary_search_by(|op| op.get_end_index().cmp(&from_counter));
          start_index = match result {
            Ok(i) => i,
            Err(i) => i,
          };
        }

        // Walk forward from the candidate start, slicing each intersecting op.
        for i in start_index..self.ops.len() {
          let op = &self.ops[i];

          // Past the slice end — done.
          if op.get_start_index() >= to_counter {
            break;
          }

          // Before the slice start — skip.
          if op.get_end_index() <= from_counter {
            continue;
          }

          // Compute the sub-range inside this op.
          let start_offset =
            ((from_counter - op.get_start_index()).max(0) as usize).min(op.atom_len());
          let end_offset = ((to_counter - op.get_start_index()).max(0) as usize).min(op.atom_len());
          assert_ne!(start_offset, end_offset);
          ans.push(op.slice(start_offset, end_offset));
        }

        RleVec::from(ans)
      }
    };

    // Sanity checks: the reconstructed ops must exactly cover the requested range.
    assert_eq!(ops.first().unwrap().get_start_index(), from_counter);
    assert_eq!(ops.last().unwrap().get_end_index(), to_counter);

    Self {
      ops,
      deps: if from > 0 {
        Frontiers::from_id(self.id.inc(from as Counter - 1))
      } else {
        self.deps.clone()
      },
      id: self.id.inc(from as Counter),
      lamport: self.lamport + from as Lamport,
      timestamp: self.timestamp,
      commit_msg: self.commit_msg.clone(),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::memory::arena::InnerArena;
  use crate::op::{Op, OpContent};
  use crate::types::{ContainerID, ContainerType};

  fn ops_from_vec(ops: Vec<Op>) -> RleVec<[Op; 1]> {
    RleVec::from(ops)
  }

  #[test]
  fn test_change_new() {
    let arena = InnerArena::new();
    let id = ID::new(1, 0);
    let deps = Frontiers::from_id(ID::new(0, 0));
    let container = arena.register(&ContainerID::new_root("counter", ContainerType::Counter));
    let op = Op::new(0, container, OpContent::Counter(1.0));
    let change = Change::new(
      ops_from_vec(vec![op]),
      deps.clone(),
      id,
      5,
      1_700_000_000_000,
    );
    assert_eq!(change.peer(), 1);
    assert_eq!(change.lamport(), 5);
    assert_eq!(change.timestamp(), 1_700_000_000_000);
    assert_eq!(change.deps(), &deps);
    assert_eq!(change.len(), 1);
  }

  #[test]
  fn test_change_empty() {
    let change: Change<Op> = Change::new(RleVec::new(), Frontiers::new(), ID::new(1, 0), 1, 0);
    assert!(change.is_empty());
  }

  // ── RLE traits ───────────────────────────────────────────────────

  #[test]
  fn test_change_has_length_empty() {
    let change: Change<Op> = Change::new(RleVec::new(), Frontiers::new(), ID::new(1, 0), 1, 0);
    assert_eq!(change.content_len(), 0);
    assert_eq!(change.atom_len(), 0);
  }

  #[test]
  fn test_change_has_length_single_op() {
    let arena = InnerArena::new();
    let c = arena.register(&ContainerID::new_root("c", ContainerType::Counter));
    let ops = vec![Op::new(0, c, OpContent::Counter(1.0))];
    let change = Change::new(ops_from_vec(ops), Frontiers::new(), ID::new(1, 0), 5, 0);
    assert_eq!(change.content_len(), 1);
    assert_eq!(change.atom_len(), 1);
  }

  #[test]
  fn test_change_has_length_multiple_ops() {
    let arena = InnerArena::new();
    let c = arena.register(&ContainerID::new_root("c", ContainerType::Counter));
    let ops = vec![
      Op::new(0, c, OpContent::Counter(1.0)),
      Op::new(1, c, OpContent::Counter(1.0)),
      Op::new(2, c, OpContent::Counter(1.0)),
    ];
    let change = Change::new(ops_from_vec(ops), Frontiers::new(), ID::new(1, 0), 5, 0);
    assert_eq!(change.content_len(), 3);
    assert_eq!(change.atom_len(), 3);
    // content_len and atom_len should be identical for Change
    assert_eq!(change.content_len(), change.atom_len());
  }

  #[test]
  fn test_change_has_index_empty() {
    let change: Change<Op> = Change::new(RleVec::new(), Frontiers::new(), ID::new(1, 7), 5, 0);
    assert_eq!(change.get_start_index(), 7);
    // get_end_index defaults to start + atom_len, so 7 + 0 == 7
    assert_eq!(change.get_end_index(), 7);
  }

  #[test]
  fn test_change_has_index_non_empty() {
    let arena = InnerArena::new();
    let c = arena.register(&ContainerID::new_root("c", ContainerType::Counter));
    let ops = vec![
      Op::new(10, c, OpContent::Counter(1.0)),
      Op::new(11, c, OpContent::Counter(1.0)),
    ];
    // id.counter is 10, but ops start at counter 10 as well
    let change = Change::new(ops_from_vec(ops), Frontiers::new(), ID::new(1, 10), 5, 0);
    assert_eq!(change.get_start_index(), 10);
    assert_eq!(change.get_end_index(), 12); // 10 + 2 ops
  }

  #[test]
  fn test_change_has_index_after_slice() {
    let arena = InnerArena::new();
    let c = arena.register(&ContainerID::new_root("c", ContainerType::Counter));
    let ops = vec![
      Op::new(0, c, OpContent::Counter(1.0)),
      Op::new(1, c, OpContent::Counter(1.0)),
      Op::new(2, c, OpContent::Counter(1.0)),
    ];
    let change = Change::new(ops_from_vec(ops), Frontiers::new(), ID::new(1, 0), 5, 0);

    let sliced = change.slice(1, 3);
    assert_eq!(sliced.get_start_index(), 1);
    assert_eq!(sliced.get_end_index(), 3); // 1 + 2
    assert_eq!(sliced.atom_len(), 2);
  }

  /// Slicing from the start of a Change preserves id, lamport and deps.
  #[test]
  fn test_change_slice_from_start() {
    let arena = InnerArena::new();
    let c = arena.register(&ContainerID::new_root("c", ContainerType::Counter));
    let deps = Frontiers::from_id(ID::new(0, 0));
    let ops = vec![
      Op::new(0, c, OpContent::Counter(1.0)),
      Op::new(1, c, OpContent::Counter(1.0)),
      Op::new(2, c, OpContent::Counter(1.0)),
    ];
    let change = Change::new(ops_from_vec(ops), deps.clone(), ID::new(1, 0), 5, 1_000);

    let sliced = change.slice(0, 2);
    assert_eq!(sliced.id(), ID::new(1, 0));
    assert_eq!(sliced.lamport(), 5);
    assert_eq!(sliced.deps(), &deps);
    assert_eq!(sliced.ops().len(), 2);
    assert_eq!(sliced.ops()[0].counter, 0);
    assert_eq!(sliced.ops()[1].counter, 1);
  }

  /// Slicing from the middle adjusts id, lamport and deps to the new start.
  #[test]
  fn test_change_slice_from_middle() {
    let arena = InnerArena::new();
    let c = arena.register(&ContainerID::new_root("c", ContainerType::Counter));
    let deps = Frontiers::from_id(ID::new(0, 0));
    let ops = vec![
      Op::new(0, c, OpContent::Counter(1.0)),
      Op::new(1, c, OpContent::Counter(1.0)),
      Op::new(2, c, OpContent::Counter(1.0)),
    ];
    let change = Change::new(ops_from_vec(ops), deps.clone(), ID::new(1, 0), 5, 1_000);

    let sliced = change.slice(1, 3);
    assert_eq!(sliced.id(), ID::new(1, 1));
    assert_eq!(sliced.lamport(), 6);
    assert_eq!(sliced.deps(), &Frontiers::from_id(ID::new(1, 0)));
    assert_eq!(sliced.ops().len(), 2);
    assert_eq!(sliced.ops()[0].counter, 1);
    assert_eq!(sliced.ops()[1].counter, 2);
  }

  /// Slicing out exactly one op from the middle produces a single-op Change.
  #[test]
  fn test_change_slice_single_op() {
    let arena = InnerArena::new();
    let c = arena.register(&ContainerID::new_root("c", ContainerType::Counter));
    let ops = vec![
      Op::new(0, c, OpContent::Counter(1.0)),
      Op::new(1, c, OpContent::Counter(1.0)),
    ];
    let change = Change::new(ops_from_vec(ops), Frontiers::new(), ID::new(1, 0), 5, 0);

    let sliced = change.slice(1, 2);
    assert_eq!(sliced.id(), ID::new(1, 1));
    assert_eq!(sliced.lamport(), 6);
    assert_eq!(sliced.ops().len(), 1);
    assert_eq!(sliced.ops()[0].counter, 1);
  }

  /// Slicing the first op keeps original deps because the start offset is zero.
  #[test]
  fn test_change_slice_first_op() {
    let arena = InnerArena::new();
    let c = arena.register(&ContainerID::new_root("c", ContainerType::Counter));
    let deps = Frontiers::from_id(ID::new(0, 0));
    let ops = vec![
      Op::new(0, c, OpContent::Counter(1.0)),
      Op::new(1, c, OpContent::Counter(1.0)),
      Op::new(2, c, OpContent::Counter(1.0)),
    ];
    let change = Change::new(ops_from_vec(ops), deps.clone(), ID::new(1, 0), 5, 1_000);

    let sliced = change.slice(0, 1);
    assert_eq!(sliced.id(), ID::new(1, 0));
    assert_eq!(sliced.lamport(), 5);
    assert_eq!(sliced.deps(), &deps); // from == 0, keep original deps
    assert_eq!(sliced.ops().len(), 1);
    assert_eq!(sliced.ops()[0].counter, 0);
  }

  /// Slicing the last op makes deps point to the op immediately before the slice.
  #[test]
  fn test_change_slice_last_op() {
    let arena = InnerArena::new();
    let c = arena.register(&ContainerID::new_root("c", ContainerType::Counter));
    let deps = Frontiers::from_id(ID::new(0, 0));
    let ops = vec![
      Op::new(0, c, OpContent::Counter(1.0)),
      Op::new(1, c, OpContent::Counter(1.0)),
      Op::new(2, c, OpContent::Counter(1.0)),
    ];
    let change = Change::new(ops_from_vec(ops), deps.clone(), ID::new(1, 0), 5, 1_000);

    let sliced = change.slice(2, 3);
    assert_eq!(sliced.id(), ID::new(1, 2));
    assert_eq!(sliced.lamport(), 7);
    // deps should point to the op before the slice start
    assert_eq!(sliced.deps(), &Frontiers::from_id(ID::new(1, 1)));
    assert_eq!(sliced.ops().len(), 1);
    assert_eq!(sliced.ops()[0].counter, 2);
  }

  /// Slicing the entire Change produces an equivalent clone.
  #[test]
  fn test_change_slice_whole() {
    let arena = InnerArena::new();
    let c = arena.register(&ContainerID::new_root("c", ContainerType::Counter));
    let deps = Frontiers::from_id(ID::new(0, 0));
    let ops = vec![
      Op::new(0, c, OpContent::Counter(1.0)),
      Op::new(1, c, OpContent::Counter(1.0)),
    ];
    let change = Change::new(ops_from_vec(ops), deps.clone(), ID::new(1, 0), 5, 1_000);

    let sliced = change.slice(0, 2);
    assert_eq!(sliced.id(), change.id());
    assert_eq!(sliced.lamport(), change.lamport());
    assert_eq!(sliced.deps(), change.deps());
    assert_eq!(sliced.timestamp(), change.timestamp());
    assert_eq!(sliced.ops().len(), 2);
    assert_eq!(sliced.ops()[0].counter, 0);
    assert_eq!(sliced.ops()[1].counter, 1);
  }

  /// After slicing, the remaining ops must still have contiguous counters.
  #[test]
  fn test_change_slice_counter_continuity() {
    let arena = InnerArena::new();
    let c = arena.register(&ContainerID::new_root("c", ContainerType::Counter));
    let ops = vec![
      Op::new(0, c, OpContent::Counter(1.0)),
      Op::new(1, c, OpContent::Counter(1.0)),
      Op::new(2, c, OpContent::Counter(1.0)),
      Op::new(3, c, OpContent::Counter(1.0)),
    ];
    let change = Change::new(ops_from_vec(ops), Frontiers::new(), ID::new(1, 0), 5, 0);

    let sliced = change.slice(1, 3);
    let ops = sliced.ops();
    assert_eq!(ops[0].counter, 1);
    assert_eq!(ops[1].counter, 2);
    // counters must be contiguous
    assert_eq!(ops[0].ctr_end(), ops[1].ctr_start());
  }

  #[test]
  fn test_change_slice_many_ops_binary_search() {
    // When ops.len() >= 8, the slice uses binary search to locate the start.
    let arena = InnerArena::new();
    let c = arena.register(&ContainerID::new_root("c", ContainerType::Counter));
    let mut ops = Vec::new();
    for i in 0..10 {
      ops.push(Op::new(i, c, OpContent::Counter(1.0)));
    }
    let change = Change::new(ops_from_vec(ops), Frontiers::new(), ID::new(1, 0), 5, 0);
    assert_eq!(change.ops().len(), 10); // placeholder ops don't merge

    // Slice from the middle, forcing binary search to skip the first runs.
    let sliced = change.slice(3, 7);
    assert_eq!(sliced.id(), ID::new(1, 3));
    assert_eq!(sliced.lamport(), 8);
    assert_eq!(sliced.atom_len(), 4);
    assert_eq!(sliced.ops().len(), 4);
    assert_eq!(sliced.ops()[0].counter, 3);
    assert_eq!(sliced.ops()[3].counter, 6);

    // Slice a single op deep inside the array.
    let sliced2 = change.slice(5, 6);
    assert_eq!(sliced2.id(), ID::new(1, 5));
    assert_eq!(sliced2.ops().len(), 1);
    assert_eq!(sliced2.ops()[0].counter, 5);
  }

  /// An empty slice range (from == to) is rejected.
  #[test]
  #[should_panic(expected = "assertion failed")]
  fn test_change_slice_empty_range_panics() {
    let arena = InnerArena::new();
    let c = arena.register(&ContainerID::new_root("c", ContainerType::Counter));
    let ops = vec![
      Op::new(0, c, OpContent::Counter(1.0)),
      Op::new(1, c, OpContent::Counter(1.0)),
    ];
    let change = Change::new(ops_from_vec(ops), Frontiers::new(), ID::new(1, 0), 5, 0);
    let _ = change.slice(1, 1);
  }

  /// Slicing an empty Change panics because to > atom_len (0).
  #[test]
  #[should_panic(expected = "assertion failed")]
  fn test_change_slice_empty_panics() {
    let change: Change<Op> = Change::new(RleVec::new(), Frontiers::new(), ID::new(1, 0), 5, 0);
    let _ = change.slice(0, 1);
  }

  #[test]
  #[should_panic(expected = "Change::merge should never be called")]
  fn test_change_merge_panics() {
    use crate::rle::Mergable;

    let arena = InnerArena::new();
    let c = arena.register(&ContainerID::new_root("c", ContainerType::Counter));
    let ops = vec![Op::new(0, c, OpContent::Counter(1.0))];
    let mut a = Change::new(ops_from_vec(ops), Frontiers::new(), ID::new(1, 0), 5, 0);
    let b = Change::new(RleVec::new(), Frontiers::new(), ID::new(1, 1), 6, 0);
    a.merge(&b, &());
  }

  /// Slicing beyond the Change's atom length panics.
  #[test]
  #[should_panic(expected = "assertion failed")]
  fn test_change_slice_out_of_bounds() {
    let arena = InnerArena::new();
    let c = arena.register(&ContainerID::new_root("c", ContainerType::Counter));
    let ops = vec![Op::new(0, c, OpContent::Counter(1.0))];
    let change = Change::new(ops_from_vec(ops), Frontiers::new(), ID::new(1, 0), 5, 0);
    let _ = change.slice(0, 2);
  }
}
