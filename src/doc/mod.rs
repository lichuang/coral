//! CoralDoc — top-level document handle and live CRDT state.
//!
//! [`CoralDoc`] is the user-facing entry point.  It owns the [`OpLog`],
//! the [`DocState`](state::DocState), and the shared
//! [`Arena`](crate::memory::arena::SharedArena).
//! All editing happens through a [`Transaction`](crate::txn::Transaction).

pub mod state;

pub use state::DocState;

use crate::memory::arena::{InnerArena, SharedArena};
use crate::oplog::OpLog;
use crate::types::PeerID;

/// Top-level document handle.
///
/// # Invariants
///
/// - Only one [`Transaction`](crate::txn::Transaction) may be active at a time.
///   `Transaction::new` panics if another transaction is already in progress.
#[derive(Debug)]
pub struct CoralDoc {
  /// The peer ID for this document replica.
  pub peer_id: PeerID,
  /// Shared arena for container indexing and value/string storage.
  pub arena: SharedArena,
  /// Operation log — the causal DAG and history storage.
  pub oplog: OpLog,
  /// Live CRDT state.
  pub state: DocState,
  /// `true` while a local transaction is active.
  pub(crate) txn_in_progress: bool,
}

impl CoralDoc {
  /// Creates a new empty document for the given peer.
  pub fn new(peer_id: PeerID) -> Self {
    let arena = SharedArena::new(InnerArena::new());
    let oplog = OpLog::with_arena(arena.clone());
    let state = DocState::new(arena.clone());
    Self {
      peer_id,
      arena,
      oplog,
      state,
      txn_in_progress: false,
    }
  }
}
