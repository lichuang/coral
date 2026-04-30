//! Integration tests for the OpLog layer.
//!
//! These tests exercise the full pipeline: change creation → import →
//! DAG update → ChangeStore persistence → PendingChanges resolution.

use crate::core::change::Change;
use crate::memory::arena::InnerArena;
use crate::op::{Op, OpContent};
use crate::oplog::OpLog;
use crate::rle::RleVec;
use crate::types::{ContainerID, ContainerType, ID};
use crate::version::Frontiers;

/// Helper: build a single-op change.
fn make_change(id: ID, lamport: u32, deps: Frontiers) -> Change {
  let arena = InnerArena::new();
  let container = arena.register(&ContainerID::new_root("c", ContainerType::Counter));
  let ops = RleVec::from(vec![Op::new(
    id.counter,
    container,
    OpContent::Counter(1.0),
  )]);
  Change::new(ops, deps, id, lamport, 1_700_000_000)
}

// ═══════════════════════════════════════════════════════════════════════════
// Linear history
// ═══════════════════════════════════════════════════════════════════════════

/// Verifies that a single peer committing 10 consecutive changes produces
/// a linear DAG and correct version metadata.
///
/// # Setup
/// Peer 1 creates changes 0..9, each depending on the previous one.
///
/// # Expected
/// - VV records all 10 ops: `{1: 10}`.
/// - Frontiers collapses to a single leaf: `[1@9]`.
#[test]
fn test_linear_history_10_changes() {
  let mut oplog = OpLog::new();

  for i in 0..10 {
    let deps = if i == 0 {
      Frontiers::new()
    } else {
      Frontiers::from_id(ID::new(1, i - 1))
    };
    let c = make_change(ID::new(1, i), i as u32 + 1, deps);
    oplog.import_local_change(c);
  }

  assert_eq!(oplog.vv().get(1).copied(), Some(10));

  let frontiers = oplog.frontiers();
  assert_eq!(frontiers.len(), 1);
  assert!(frontiers.iter().any(|id| id == ID::new(1, 9)));
}

// ═══════════════════════════════════════════════════════════════════════════
// Fork / concurrent peers
// ═══════════════════════════════════════════════════════════════════════════

/// Two peers edit concurrently without seeing each other.
///
/// # Setup
/// Peer 1 creates A, peer 2 creates B.  Neither change references the other.
///
/// # Expected
/// - VV covers both peers.
/// - Frontiers contains **both** leaf IDs because they are concurrent
///   (neither is an ancestor of the other).
#[test]
fn test_concurrent_peers_frontiers() {
  let mut oplog = OpLog::new();

  // Peer 1 creates A.
  let a = make_change(ID::new(1, 0), 1, Frontiers::new());
  oplog.import_local_change(a);

  // Peer 2 creates B concurrently (no knowledge of A).
  let b = make_change(ID::new(2, 0), 2, Frontiers::new());
  oplog.import_local_change(b);

  // VV should now cover both peers.
  assert_eq!(oplog.vv().get(1).copied(), Some(1));
  assert_eq!(oplog.vv().get(2).copied(), Some(1));

  // Frontiers must contain both leaf IDs because they are concurrent.
  let frontiers = oplog.frontiers();
  assert_eq!(frontiers.len(), 2);
  assert!(frontiers.iter().any(|id| id == ID::new(1, 0)));
  assert!(frontiers.iter().any(|id| id == ID::new(2, 0)));
}

/// A diamond-shaped DAG: one root, two concurrent branches, then a merge.
///
/// # Setup
/// ```text
///       A (1,0)
///      / \
///     B   C
///   (2,0)(1,1)
///      \ /
///       D (1,2)
/// ```
/// D depends on both B and C, closing the fork.
///
/// # Expected
/// - VV = `{1: 3, 2: 1}` (peer 1 has A+C+D, peer 2 has B).
/// - Frontiers converges to a single leaf: `[D]`.
#[test]
fn test_diamond_merge() {
  let mut oplog = OpLog::new();

  let id_a = ID::new(1, 0);
  let a = make_change(id_a, 1, Frontiers::new());
  oplog.import_local_change(a);

  let id_b = ID::new(2, 0);
  let b = make_change(id_b, 2, Frontiers::from_id(id_a));
  oplog.import_local_change(b);

  let id_c = ID::new(1, 1);
  let c = make_change(id_c, 3, Frontiers::from_id(id_a));
  oplog.import_local_change(c);

  // D depends on both B and C.
  let id_d = ID::new(1, 2);
  let d = make_change(id_d, 4, Frontiers::from_iter([id_b, id_c]));
  oplog.import_local_change(d);

  assert_eq!(oplog.vv().get(1).copied(), Some(3));
  assert_eq!(oplog.vv().get(2).copied(), Some(1));

  let frontiers = oplog.frontiers();
  assert_eq!(frontiers.len(), 1);
  assert!(frontiers.iter().any(|id| id == id_d));
}

// ═══════════════════════════════════════════════════════════════════════════
// Out-of-order remote import
// ═══════════════════════════════════════════════════════════════════════════

/// Chain of dependencies arrives in reverse order.
///
/// # Setup
/// ```text
/// A ──► B ──► C
/// ```
/// Arrival order: C → B → A.
///
/// # Expected
/// - C and B are queued in `PendingChanges` because their deps are missing.
/// - When A finally arrives, `try_apply_pending` unlocks B, then C.
/// - Final VV = `{1: 3}`, frontiers = `[C]`.
#[test]
fn test_out_of_order_import() {
  let mut oplog = OpLog::new();

  let id_a = ID::new(1, 0);
  let a = make_change(id_a, 1, Frontiers::new());

  let id_b = ID::new(1, 1);
  let b = make_change(id_b, 2, Frontiers::from_id(id_a));

  let id_c = ID::new(1, 2);
  let c = make_change(id_c, 3, Frontiers::from_id(id_b));

  // Import C first — should land in pending.
  oplog.import_remote_change(c);
  assert_eq!(oplog.pending_changes_len(), 1);
  assert_eq!(oplog.vv().get(1).copied(), None);

  // Import B — still missing A.
  oplog.import_remote_change(b);
  assert_eq!(oplog.pending_changes_len(), 2);
  assert_eq!(oplog.vv().get(1).copied(), None);

  // Import A — unlocks B, which unlocks C.
  oplog.import_remote_change(a);
  assert_eq!(oplog.pending_changes_len(), 0);
  assert_eq!(oplog.vv().get(1).copied(), Some(3));

  let frontiers = oplog.frontiers();
  assert_eq!(frontiers.len(), 1);
  assert!(frontiers.iter().any(|id| id == id_c));
}

/// Some pending changes are resolvable, others stay blocked forever.
///
/// # Setup
/// - A and B form a normal chain (A → B).
/// - D depends on a non-existent peer (99@0), so it can never be applied.
/// Arrival order: D → B → A.
///
/// # Expected
/// - After D and B: both are pending (2 items).
/// - After A: B is unlocked and applied, but D remains blocked.
/// - Final VV = `{1: 2}` (A and B only).
#[test]
fn test_out_of_order_partial() {
  let mut oplog = OpLog::new();

  let id_a = ID::new(1, 0);
  let a = make_change(id_a, 1, Frontiers::new());

  let id_b = ID::new(1, 1);
  let b = make_change(id_b, 2, Frontiers::from_id(id_a));

  // d depends on an ID that will never arrive (peer 99).
  let id_d = ID::new(1, 3);
  let d = make_change(id_d, 4, Frontiers::from_id(ID::new(99, 0)));

  oplog.import_remote_change(d.clone());
  oplog.import_remote_change(b.clone());

  assert_eq!(oplog.pending_changes_len(), 2);

  // A arrives — B is unblocked, but d remains blocked.
  oplog.import_remote_change(a);
  assert_eq!(oplog.pending_changes_len(), 1);
  assert_eq!(oplog.vv().get(1).copied(), Some(2));
}

/// Diamond-shaped dependencies arrive out of order.
///
/// # Setup
/// ```text
///       A
///      / \
///     B   C
///      \ /
///       D
/// ```
/// Arrival order: D → B → C → A.
///
/// # Expected
/// - D is blocked on B and C; B and C are each blocked on A.
/// - When A arrives, it unlocks both B and C; together they unlock D.
/// - All four changes are eventually applied.
/// - Final VV = `{1: 2, 2: 1, 3: 1}`.
#[test]
fn test_out_of_order_diamond() {
  let mut oplog = OpLog::new();

  let id_a = ID::new(1, 0);
  let a = make_change(id_a, 1, Frontiers::new());

  let id_b = ID::new(2, 0);
  let b = make_change(id_b, 2, Frontiers::from_id(id_a));

  let id_c = ID::new(3, 0);
  let c = make_change(id_c, 3, Frontiers::from_id(id_a));

  let id_d = ID::new(1, 1);
  let d = make_change(id_d, 4, Frontiers::from_iter([id_b, id_c]));

  // Import D first (deps missing).
  oplog.import_remote_change(d);
  assert_eq!(oplog.pending_changes_len(), 1);

  // Import B and C (still missing A).
  oplog.import_remote_change(b);
  oplog.import_remote_change(c);
  assert_eq!(oplog.pending_changes_len(), 3);

  // Import A — unlocks B and C, which together unlock D.
  oplog.import_remote_change(a);
  assert_eq!(oplog.pending_changes_len(), 0);
  assert_eq!(oplog.vv().get(1).copied(), Some(2));
  assert_eq!(oplog.vv().get(2).copied(), Some(1));
  assert_eq!(oplog.vv().get(3).copied(), Some(1));
}
