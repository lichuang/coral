use crate::operation::Op;
use crate::rle::RleVec;
use crate::types::{Lamport, OpId, PeerID};
use crate::version::Heads;

use super::Commit;

/// Accumulates operations for a pending transaction.
///
/// `CommitBuilder` lives inside [`DocInner`](super::DocInner) as an
/// `Option<CommitBuilder>`. Operations are pushed into it as they happen;
/// when the user calls `commit()`, the accumulated ops are converted into a
/// single [`Commit`].
pub struct CommitBuilder {
  peer_id: PeerID,
  lamport: Lamport,
  deps: Heads,
  ops: RleVec<Op>,
}

impl std::fmt::Debug for CommitBuilder {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("CommitBuilder")
      .field("peer_id", &self.peer_id)
      .field("lamport", &self.lamport)
      .field("deps", &self.deps)
      .field("ops_len", &self.ops.len())
      .finish()
  }
}

impl CommitBuilder {
  /// Creates a new `CommitBuilder` with the given metadata and no ops.
  pub fn new(peer_id: PeerID, lamport: Lamport, deps: Heads) -> Self {
    Self {
      peer_id,
      lamport,
      deps,
      ops: RleVec::new(),
    }
  }

  /// Returns `true` if no ops have been pushed.
  pub fn is_empty(&self) -> bool {
    self.ops.is_empty()
  }

  /// Pushes an op into the pending transaction.
  ///
  /// If the op is mergeable with the last stored op (same container,
  /// consecutive counter, mergeable command), they are combined in-place.
  pub fn push_op(&mut self, op: Op) {
    self.ops.push(op);
  }

  /// Consumes the builder and returns a [`Commit`] if any ops were pushed,
  /// or `None` if the transaction was empty.
  pub fn into_commit(self) -> Option<Commit> {
    let first_op = self.ops.first()?;
    Some(Commit {
      id: OpId::new(self.peer_id, first_op.counter),
      lamport: self.lamport,
      timestamp: 0,
      deps: self.deps,
      ops: self.ops,
      from_local: true,
    })
  }
}
