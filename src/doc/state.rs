//! DocState — collection and dispatch of all container states.
//!
//! [`DocState`] holds the live CRDT state for every container in the document.
//! It is rebuilt by replaying the [`OpLog`](crate::oplog::OpLog) and updated
//! incrementally when local or remote changes arrive.
//!
//! # Phase note
//!
//! Currently a skeleton: container states (Counter, Map, List, …) will be
//! added in Phases 9–12.  Until then `apply_local_op` and `commit` are no-ops.

use crate::memory::arena::SharedArena;
use crate::op::Op;

/// Live CRDT state for the entire document.
#[derive(Debug)]
pub struct DocState {
  #[allow(dead_code)]
  arena: SharedArena,
}

impl DocState {
  /// Creates an empty `DocState` backed by the given arena.
  pub fn new(arena: SharedArena) -> Self {
    Self { arena }
  }

  /// Apply a local operation to the appropriate container state.
  ///
  /// # TODO
  ///
  /// Route the op to the correct [`ContainerState`] implementation once
  /// container states land in Phase 9+.
  pub fn apply_local_op(&mut self, _op: &Op, _arena: &SharedArena) {
    // TODO(Phase 9+): route to the correct ContainerState
  }

  /// Finalise any pending state changes at the end of a transaction.
  ///
  /// # TODO
  ///
  /// Will trigger state-level commit hooks (e.g. flushing btree caches)
  /// when container states are implemented.
  pub fn commit(&mut self) {
    // TODO(Phase 9+): state-level commit logic
  }
}
