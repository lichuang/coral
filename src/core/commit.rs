use crate::operation::Op;
use crate::rle::{HasLength, RleVec};
use crate::types::{Counter, Lamport, OpId, Timestamp};
use crate::version::Heads;

/// A group of operations produced by a single peer at one causal moment.
///
/// A `Commit` is the atomic unit of collaboration: it bundles one or more
/// [`Op`]s together with metadata that describes *when* and *after what*
/// the commit happened. Peers exchange `Commit`s (not individual `Op`s)
/// during sync.
///
/// # Fields
///
/// - `id` — the starting [`OpId`] (`peer` + `counter`) of this commit.
///   All contained ops share the same `peer` and have consecutive counters.
/// - `lamport` — Lamport timestamp used for causal ordering.
/// - `timestamp` — physical wall-clock time (seconds since Unix epoch).
/// - `deps` — the [`Heads`] this commit depends on (the DAG frontier right
///   before this commit was created).
/// - `ops` — the actual operations, stored in a run-length encoded vector.
/// - `from_local` — `true` if this commit originated from the local peer,
///   `false` if it was imported from a remote peer during sync.
#[derive(Debug, Clone, PartialEq)]
pub struct Commit {
  pub id: OpId,
  pub lamport: Lamport,
  pub timestamp: Timestamp,
  pub deps: Heads,
  pub ops: RleVec<Op>,
  pub from_local: bool,
}

impl Commit {
  /// Creates a new empty `Commit` with the given metadata.
  pub fn new(
    id: OpId,
    lamport: Lamport,
    timestamp: Timestamp,
    deps: Heads,
    from_local: bool,
  ) -> Self {
    Self {
      id,
      lamport,
      timestamp,
      deps,
      ops: RleVec::new(),
      from_local,
    }
  }

  /// Appends an operation to this commit.
  ///
  /// If the new op is mergeable with the last stored op, they are combined
  /// in-place via the [`RleVec`](crate::rle::RleVec) mechanism.
  pub fn push_op(&mut self, op: Op) {
    self.ops.push(op);
  }

  /// Returns the exclusive end counter of this commit's operation range.
  ///
  /// The commit covers counters `[id.counter, end_counter())`.
  pub fn end_counter(&self) -> Counter {
    self.id.counter
      + self
        .ops
        .iter()
        .map(|op| op.content_len() as Counter)
        .sum::<Counter>()
  }

  /// Debug-only check that all ops form a contiguous counter range
  /// starting at `id.counter`.
  #[cfg(debug_assertions)]
  pub fn assert_contiguous(&self) {
    let mut expected = self.id.counter;
    for op in self.ops.iter() {
      debug_assert_eq!(
        op.counter, expected,
        "ops not contiguous: expected counter {} but found {}",
        expected, op.counter
      );
      expected += op.content_len() as Counter;
    }
  }
}
