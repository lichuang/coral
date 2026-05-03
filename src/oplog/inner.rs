//! OpLog — the central history store.
//!
//! [`OpLog`] owns the causal DAG ([`AppDag`]), the [`ChangeStore`], the
//! [`PendingChanges`] queue, and the shared [`Arena`](crate::memory::arena::SharedArena).
//! It is the **only** place where new changes are accepted into the system.

use crate::core::change::Change;
use crate::memory::arena::InnerArena;
use crate::memory::arena::SharedArena;
use crate::op::RichOp;
use crate::oplog::{AppDag, ChangeStore, PendingChanges};
use crate::rle::{HasLength, Sliceable};
use crate::types::{Counter, ID, PeerID};
use crate::version::{Frontiers, IdSpan, VersionVector};

// ═══════════════════════════════════════════════════════════════════════════
// ChangeState
// ═══════════════════════════════════════════════════════════════════════════

/// Classification of a remote change relative to the current DAG state.
enum ChangeState {
  /// Change is already fully included in our DAG.
  Applied,
  /// Change can be imported immediately.
  CanApplyDirectly,
  /// Change is blocked on `missing_dep`.
  AwaitingMissingDependency(ID),
}

// ═══════════════════════════════════════════════════════════════════════════
// OpLog
// ═══════════════════════════════════════════════════════════════════════════

/// The central history store.
///
/// # Invariants
///
/// - The causal graph is always a DAG and complete (no missing deps in
///   imported history).  If deps are missing, the change is queued in
///   [`PendingChanges`] instead.
/// - `insert_new_change` is the **only** way to add a change to the system.
#[allow(dead_code)]
#[derive(Debug)]
pub struct OpLog {
  pub(crate) dag: AppDag,
  pub(crate) arena: SharedArena,
  pub(crate) change_store: ChangeStore,
  pub(crate) pending_changes: PendingChanges,
}

impl Default for OpLog {
  fn default() -> Self {
    Self::new()
  }
}

impl OpLog {
  /// Creates a new empty `OpLog`.
  pub fn new() -> Self {
    Self::with_arena(SharedArena::new(InnerArena::new()))
  }

  /// Creates a new `OpLog` with the given shared arena.
  ///
  /// The arena is shared with the document layer so that container IDs,
  /// values and strings are resolved consistently across `OpLog` and `DocState`.
  pub fn with_arena(arena: SharedArena) -> Self {
    let change_store = ChangeStore::new(arena.clone());
    Self {
      dag: AppDag::new(),
      arena,
      change_store,
      pending_changes: PendingChanges::new(),
    }
  }

  // ── Insertion (single entry-point) ───────────────────────────────────────

  /// The **only** place where a change is accepted into the system.
  ///
  /// Updates the DAG, the change store, and version metadata.
  pub fn insert_new_change(&mut self, change: Change, from_local: bool) {
    let len = change.content_len();
    if from_local {
      // Must run before handle_new_change so that pending_txn_node is set.
      self
        .dag
        .update_version_on_new_local_op(change.deps(), change.id(), change.lamport(), len);
    }
    self.dag.handle_new_change(&change, from_local);
    self.change_store.insert_change(&change, from_local);
  }

  /// Imports a change produced by the local peer.
  pub fn import_local_change(&mut self, change: Change) {
    self.insert_new_change(change, true);
  }

  /// Imports a change from a remote peer.
  ///
  /// If the change's dependencies are not yet known, it is queued in
  /// [`PendingChanges`] and will be retried automatically when the missing
  /// history arrives.
  pub fn import_remote_change(&mut self, change: Change) {
    match self.remote_change_apply_state(&change) {
      ChangeState::Applied => {}
      ChangeState::CanApplyDirectly => {
        let id_last = change.id_last();
        self.apply_change_from_remote(change);
        self.try_apply_pending(&[id_last]);
      }
      ChangeState::AwaitingMissingDependency(miss_dep) => {
        self.pending_changes.push(miss_dep, change);
      }
    }
  }

  /// Applies a remote change that has already passed the state check.
  fn apply_change_from_remote(&mut self, change: Change) {
    let Some(change) = self.trim_the_known_part_of_change(change) else {
      return;
    };
    self.insert_new_change(change, false);
  }

  /// Retries pending changes that may have been unblocked by newly-imported
  /// history.
  ///
  /// `new_ids` are the IDs of changes that were just applied.  Any pending
  /// change whose missing dependency is covered by one of these IDs becomes
  /// eligible for re-evaluation.
  pub(crate) fn try_apply_pending(&mut self, new_ids: &[ID]) {
    let mut ids_to_check: Vec<ID> = new_ids.to_vec();

    while let Some(id) = ids_to_check.pop() {
      let taken = self.pending_changes.take_related(&[id]);
      for change in taken {
        match self.remote_change_apply_state(&change) {
          ChangeState::CanApplyDirectly => {
            ids_to_check.push(change.id_last());
            self.apply_change_from_remote(change);
          }
          ChangeState::Applied => {}
          ChangeState::AwaitingMissingDependency(miss_dep) => {
            self.pending_changes.push(miss_dep, change);
          }
        }
      }
    }
  }

  // ── Queries ──────────────────────────────────────────────────────────────

  /// Next available [`ID`] for `peer`.
  pub fn next_id(&self, peer: PeerID) -> ID {
    let cnt = self.dag.vv().get(peer).copied().unwrap_or(0);
    ID::new(peer, cnt)
  }

  /// Current version vector.
  pub fn vv(&self) -> &VersionVector {
    self.dag.vv()
  }

  /// Current frontiers.
  pub fn frontiers(&self) -> &Frontiers {
    self.dag.frontiers()
  }

  /// Number of changes currently in the pending queue.
  pub fn pending_changes_len(&self) -> usize {
    self.pending_changes.len()
  }

  /// Iterate over [`RichOp`]s in the given span.
  pub fn iter_ops(&self, id_span: IdSpan) -> impl Iterator<Item = RichOp<'_>> + '_ {
    let span_start = id_span.counter.start;
    let span_end = id_span.counter.end;

    self
      .change_store
      .iter_changes(id_span)
      .flat_map(move |change| {
        let peer = change.id().peer;
        let lamport = change.lamport();
        let timestamp = change.timestamp();
        change.ops().iter().filter_map(move |op| {
          let op_start = op.counter;
          let op_end = op.counter + op.atom_len() as Counter;
          if op_start < span_end && op_end > span_start {
            Some(RichOp::new(op, peer, lamport, timestamp))
          } else {
            None
          }
        })
      })
  }

  /// Lookup a change by ID.
  pub fn get_change_at(&self, id: ID) -> Option<Change> {
    self.change_store.get_change(id)
  }

  // ── Internal helpers ─────────────────────────────────────────────────────

  /// Determines whether a remote change can be applied, is already applied,
  /// or is blocked on a missing dependency.
  fn remote_change_apply_state(&self, change: &Change) -> ChangeState {
    let peer = change.id().peer;
    let start = change.id().counter;
    let end = start + change.content_len() as Counter;
    let vv_latest_ctr = self.dag.vv().get(peer).copied().unwrap_or(0);

    if vv_latest_ctr >= end {
      return ChangeState::Applied;
    }

    if vv_latest_ctr < start {
      return ChangeState::AwaitingMissingDependency(change.id().inc(-1));
    }

    for dep in change.deps().iter() {
      let dep_vv_latest_ctr = self.dag.vv().get(dep.peer).copied().unwrap_or(0);
      if dep_vv_latest_ctr == 0 || dep_vv_latest_ctr - 1 < dep.counter {
        return ChangeState::AwaitingMissingDependency(dep);
      }
    }

    ChangeState::CanApplyDirectly
  }

  /// Removes the prefix of `change` that is already present in our DAG.
  ///
  /// Returns `None` if the entire change is already known.
  fn trim_the_known_part_of_change(&self, change: Change) -> Option<Change> {
    let known_end = self.dag.vv().get(change.id().peer).copied().unwrap_or(0);

    if change.id().counter >= known_end {
      return Some(change);
    }

    let change_end = change.id().counter + change.content_len() as Counter;
    if change_end <= known_end {
      return None;
    }

    let offset = (known_end - change.id().counter) as usize;
    Some(change.slice(offset, change.content_len()))
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
  use crate::types::{ContainerID, ContainerType};
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

  // ── construction ─────────────────────────────────────────────────────────

  #[test]
  fn test_new_is_empty() {
    let oplog = OpLog::new();
    assert!(oplog.vv().is_empty());
    assert!(oplog.frontiers().is_empty());
    assert!(oplog.pending_changes.is_empty());
  }

  // ── local import ─────────────────────────────────────────────────────────

  #[test]
  fn test_import_local_change() {
    let mut oplog = OpLog::new();
    let c = make_change(1, 0, 1, Frontiers::new(), 2);
    oplog.import_local_change(c);

    assert_eq!(oplog.vv().get(1).copied(), Some(2));
    assert!(oplog.get_change_at(ID::new(1, 0)).is_some());
    assert!(oplog.get_change_at(ID::new(1, 1)).is_some());
  }

  // ── next_id ──────────────────────────────────────────────────────────────

  #[test]
  fn test_next_id() {
    let mut oplog = OpLog::new();
    assert_eq!(oplog.next_id(1), ID::new(1, 0));

    let c = make_change(1, 0, 1, Frontiers::new(), 3);
    oplog.import_local_change(c);
    assert_eq!(oplog.next_id(1), ID::new(1, 3));
  }

  // ── remote import linear ─────────────────────────────────────────────────

  #[test]
  fn test_import_remote_linear() {
    let mut oplog = OpLog::new();
    let c = make_change(1, 0, 1, Frontiers::new(), 2);
    oplog.import_remote_change(c);

    assert_eq!(oplog.vv().get(1).copied(), Some(2));
    assert!(oplog.pending_changes.is_empty());
  }

  // ── remote import out-of-order ───────────────────────────────────────────

  #[test]
  fn test_import_remote_out_of_order() {
    let mut oplog = OpLog::new();

    // A (peer=1, cnt=0)
    let a = make_change(1, 0, 1, Frontiers::new(), 1);
    // B depends on A
    let b = make_change(1, 1, 2, Frontiers::from_id(ID::new(1, 0)), 1);

    // Import B first — deps missing, should go to pending.
    oplog.import_remote_change(b.clone());
    assert!(!oplog.pending_changes.is_empty());
    assert_eq!(oplog.vv().get(1).copied(), None);

    // Now import A.
    oplog.import_remote_change(a);
    // A should unlock B through try_apply_pending.
    assert!(oplog.pending_changes.is_empty());
    assert_eq!(oplog.vv().get(1).copied(), Some(2));
  }

  // ── remote import chain ──────────────────────────────────────────────────

  #[test]
  fn test_import_remote_chain() {
    let mut oplog = OpLog::new();

    // A ──► B ──► C
    let a = make_change(1, 0, 1, Frontiers::new(), 1);
    let b = make_change(1, 1, 2, Frontiers::from_id(ID::new(1, 0)), 1);
    let c = make_change(1, 2, 3, Frontiers::from_id(ID::new(1, 1)), 1);

    // Import C then B (out of order).
    oplog.import_remote_change(c);
    oplog.import_remote_change(b);
    assert_eq!(oplog.pending_changes.len(), 2);

    // Import A — unlocks B, which unlocks C.
    oplog.import_remote_change(a);
    assert!(oplog.pending_changes.is_empty());
    assert_eq!(oplog.vv().get(1).copied(), Some(3));
  }

  // ── remote import already applied ────────────────────────────────────────

  #[test]
  fn test_import_remote_already_applied() {
    let mut oplog = OpLog::new();
    let c = make_change(1, 0, 1, Frontiers::new(), 2);
    oplog.import_local_change(c.clone());

    // Re-import the same change remotely.
    oplog.import_remote_change(c);
    assert_eq!(oplog.vv().get(1).copied(), Some(2));
    // Should not double-count.
  }

  // ── frontiers after merge ────────────────────────────────────────────────

  #[test]
  fn test_frontiers_after_concurrent_import() {
    let mut oplog = OpLog::new();

    let a = make_change(1, 0, 1, Frontiers::new(), 1);
    let b = make_change(2, 0, 2, Frontiers::new(), 1);

    oplog.import_local_change(a);
    oplog.import_local_change(b);

    let frontiers = oplog.frontiers();
    assert_eq!(frontiers.len(), 2);
    assert!(frontiers.iter().any(|id| id == ID::new(1, 0)));
    assert!(frontiers.iter().any(|id| id == ID::new(2, 0)));
  }

  // ── iter_ops ─────────────────────────────────────────────────────────────

  #[test]
  fn test_iter_ops() {
    let mut oplog = OpLog::new();
    let c = make_change(1, 0, 1, Frontiers::new(), 3);
    oplog.import_local_change(c);

    let span = IdSpan::new(1, 0, 3);
    let ops: Vec<_> = oplog.iter_ops(span).collect();
    assert_eq!(ops.len(), 3);
  }

  // ── get_change_at ────────────────────────────────────────────────────────

  #[test]
  fn test_get_change_at() {
    let mut oplog = OpLog::new();
    let c = make_change(1, 0, 1, Frontiers::new(), 2);
    oplog.import_local_change(c);

    assert!(oplog.get_change_at(ID::new(1, 0)).is_some());
    assert!(oplog.get_change_at(ID::new(1, 1)).is_some());
    assert!(oplog.get_change_at(ID::new(1, 2)).is_none());
  }

  // ── trim known part ──────────────────────────────────────────────────────

  #[test]
  fn test_trim_known_part_of_change() {
    let mut oplog = OpLog::new();
    let a = make_change(1, 0, 1, Frontiers::new(), 2); // counters 0,1
    oplog.import_local_change(a);

    // Re-import a change that overlaps with existing history.
    // counters 0,1,2 — first two are known.
    let b = make_change(1, 0, 1, Frontiers::new(), 3);
    oplog.import_remote_change(b);

    // VV should still be 2 (the new part counter=2 is added).
    assert_eq!(oplog.vv().get(1).copied(), Some(3));
  }
}
