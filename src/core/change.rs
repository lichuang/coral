use crate::operation::Op;
use crate::rle::RleVec;
use crate::types::{Lamport, OpId, Timestamp};
use crate::version::Heads;

/// A group of operations produced by a single peer at one causal moment.
///
/// A `Change` is the atomic unit of collaboration: it bundles one or more
/// [`Op`]s together with metadata that describes *when* and *after what*
/// the change happened. Peers exchange `Change`s (not individual `Op`s)
/// during sync.
///
/// # Fields
///
/// - `id` — the starting [`OpId`] (`peer` + `counter`) of this change.
///   All contained ops share the same `peer` and have consecutive counters.
/// - `lamport` — Lamport timestamp used for causal ordering.
/// - `timestamp` — physical wall-clock time (seconds since Unix epoch).
/// - `deps` — the [`Heads`] this change depends on (the DAG frontier right
///   before this change was created).
/// - `ops` — the actual operations, stored in a run-length encoded vector.
#[derive(Debug, Clone, PartialEq)]
pub struct Change {
  pub id: OpId,
  pub lamport: Lamport,
  pub timestamp: Timestamp,
  pub deps: Heads,
  pub ops: RleVec<Op>,
}

impl Change {
  /// Creates a new empty `Change` with the given metadata.
  pub fn new(id: OpId, lamport: Lamport, timestamp: Timestamp, deps: Heads) -> Self {
    Self {
      id,
      lamport,
      timestamp,
      deps,
      ops: RleVec::new(),
    }
  }

  /// Appends an operation to this change.
  ///
  /// If the new op is mergeable with the last stored op, they are combined
  /// in-place via the [`RleVec`](crate::rle::RleVec) mechanism.
  pub fn push_op(&mut self, op: Op) {
    self.ops.push(op);
  }
}
