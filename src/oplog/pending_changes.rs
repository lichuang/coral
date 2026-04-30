//! Pending change queue for out-of-order remote imports.
//!
//! When a remote change arrives before its dependencies, it cannot be
//! applied immediately.  [`PendingChanges`] stores such changes indexed by
//! their **first missing dependency** — when that dependency is later
//! satisfied, only the relevant bucket needs to be checked.
//!
//! This design mirrors Loro: `PendingChanges` is a pure data structure;
//! the actual application logic lives in [`OpLog`](super::OpLog) (Phase 7.3).

use crate::core::change::Change;
use crate::types::{Counter, ID, PeerID};
use rustc_hash::FxHashMap;
use std::collections::BTreeMap;

/// Queue of changes whose dependencies are not yet known.
///
/// Changes are indexed by the **ID of the first missing dependency**
/// (`missing_dep.peer → missing_dep.counter → Vec<Change>`).
/// This allows `try_apply_pending` to touch only the buckets that could
/// have been unblocked by a newly-imported change.
#[derive(Debug, Default, Clone)]
pub struct PendingChanges {
  changes: FxHashMap<PeerID, BTreeMap<Counter, Vec<Change>>>,
}

impl PendingChanges {
  /// Creates an empty pending queue.
  pub fn new() -> Self {
    Self {
      changes: FxHashMap::default(),
    }
  }

  /// Returns the total number of changes currently queued.
  pub fn len(&self) -> usize {
    self
      .changes
      .values()
      .map(|tree| tree.values().map(Vec::len).sum::<usize>())
      .sum()
  }

  /// Returns `true` if no changes are pending.
  pub fn is_empty(&self) -> bool {
    self.changes.is_empty()
  }

  // ── Core API ─────────────────────────────────────────────────────────────

  /// Stores a change keyed by its first missing dependency.
  ///
  /// The caller (e.g. `OpLog`) is responsible for determining *which*
  /// dep is missing (via `remote_change_apply_state`).
  pub fn push(&mut self, missing_dep: ID, change: Change) {
    self
      .changes
      .entry(missing_dep.peer)
      .or_default()
      .entry(missing_dep.counter)
      .or_default()
      .push(change);
  }

  /// Removes and returns all changes whose key is "covered" by `ids`.
  ///
  /// For each `id` in `ids`, every bucket with the same peer and a
  /// counter **≤ id.counter** is drained.
  pub fn take_related(&mut self, ids: &[ID]) -> Vec<Change> {
    let mut taken = Vec::new();

    for id in ids {
      let Some(tree) = self.changes.get_mut(&id.peer) else {
        continue;
      };

      let to_remove: Vec<Counter> = tree.range(..=id.counter).map(|(&k, _)| k).collect();

      for cnt in to_remove {
        if let Some(changes) = tree.remove(&cnt) {
          taken.extend(changes);
        }
      }

      if tree.is_empty() {
        self.changes.remove(&id.peer);
      }
    }

    taken
  }
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
  use super::*;
  use crate::memory::arena::InnerArena;
  use crate::op::{Op, OpContent};
  use crate::rle::RleVec;
  use crate::types::{ContainerID, ContainerType, ID};
  use crate::version::Frontiers;

  fn make_change(
    peer: u64,
    counter: Counter,
    lamport: u32,
    deps: Frontiers,
    op_count: usize,
  ) -> Change {
    let arena = InnerArena::new();
    let container = arena.register(&ContainerID::new_root("c", ContainerType::Counter));
    let ops = RleVec::from(
      (0..op_count)
        .map(|i| Op::new(counter + i as Counter, container, OpContent::Counter(1.0)))
        .collect::<Vec<_>>(),
    );
    Change::new(ops, deps, ID::new(peer, counter), lamport, 1_700_000_000)
  }

  fn empty_pending() -> PendingChanges {
    PendingChanges::new()
  }

  // ── push / basic storage ─────────────────────────────────────────────────

  #[test]
  fn test_push_and_len() {
    let mut pending = empty_pending();
    let c = make_change(1, 0, 1, Frontiers::new(), 1);

    pending.push(ID::new(2, 5), c.clone());
    assert_eq!(pending.len(), 1);
    assert!(!pending.is_empty());

    pending.push(ID::new(2, 5), c.clone());
    assert_eq!(pending.len(), 2);
  }

  #[test]
  fn test_push_different_keys() {
    let mut pending = empty_pending();
    let a = make_change(1, 0, 1, Frontiers::new(), 1);
    let b = make_change(1, 1, 2, Frontiers::new(), 1);

    pending.push(ID::new(2, 0), a);
    pending.push(ID::new(3, 7), b);
    assert_eq!(pending.len(), 2);
  }

  // ── take_related ─────────────────────────────────────────────────────────

  #[test]
  fn test_take_related_exact_match() {
    let mut pending = empty_pending();
    let c = make_change(1, 0, 1, Frontiers::new(), 1);
    pending.push(ID::new(2, 5), c.clone());

    let taken = pending.take_related(&[ID::new(2, 5)]);
    assert_eq!(taken.len(), 1);
    assert!(pending.is_empty());
  }

  #[test]
  fn test_take_related_greater_counter_covers_smaller() {
    let mut pending = empty_pending();
    let a = make_change(1, 0, 1, Frontiers::new(), 1);
    let b = make_change(1, 1, 2, Frontiers::new(), 1);

    pending.push(ID::new(2, 3), a.clone());
    pending.push(ID::new(2, 7), b.clone());

    // id.counter=10 covers both 3 and 7.
    let taken = pending.take_related(&[ID::new(2, 10)]);
    assert_eq!(taken.len(), 2);
    assert!(pending.is_empty());
  }

  #[test]
  fn test_take_related_partial_match() {
    let mut pending = empty_pending();
    let a = make_change(1, 0, 1, Frontiers::new(), 1);
    let b = make_change(1, 1, 2, Frontiers::new(), 1);

    pending.push(ID::new(2, 3), a.clone());
    pending.push(ID::new(2, 7), b.clone());

    // id.counter=5 covers 3 but not 7.
    let taken = pending.take_related(&[ID::new(2, 5)]);
    assert_eq!(taken.len(), 1);
    assert_eq!(taken[0].id(), a.id());
    assert_eq!(pending.len(), 1);
  }

  #[test]
  fn test_take_related_no_match() {
    let mut pending = empty_pending();
    let c = make_change(1, 0, 1, Frontiers::new(), 1);
    pending.push(ID::new(2, 5), c.clone());

    // Different peer → no match.
    let taken = pending.take_related(&[ID::new(3, 10)]);
    assert!(taken.is_empty());
    assert_eq!(pending.len(), 1);
  }

  #[test]
  fn test_take_related_smaller_counter_no_match() {
    let mut pending = empty_pending();
    let c = make_change(1, 0, 1, Frontiers::new(), 1);
    pending.push(ID::new(2, 5), c.clone());

    // id.counter=3 < 5 → does not cover.
    let taken = pending.take_related(&[ID::new(2, 3)]);
    assert!(taken.is_empty());
    assert_eq!(pending.len(), 1);
  }

  #[test]
  fn test_take_related_multiple_ids() {
    let mut pending = empty_pending();
    let a = make_change(1, 0, 1, Frontiers::new(), 1);
    let b = make_change(1, 1, 2, Frontiers::new(), 1);

    pending.push(ID::new(2, 3), a.clone());
    pending.push(ID::new(3, 7), b.clone());

    let taken = pending.take_related(&[ID::new(2, 3), ID::new(3, 7)]);
    assert_eq!(taken.len(), 2);
    assert!(pending.is_empty());
  }

  #[test]
  fn test_take_related_drains_peer_when_empty() {
    let mut pending = empty_pending();
    let c = make_change(1, 0, 1, Frontiers::new(), 1);
    pending.push(ID::new(2, 5), c.clone());

    pending.take_related(&[ID::new(2, 5)]);
    // Internal BTreeMap and FxHashMap entries should be cleaned up.
    assert!(pending.changes.get(&2).is_none());
  }
}
