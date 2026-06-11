use std::collections::BTreeMap;

use super::Commit;
use crate::types::OpId;
use crate::version::{IdSpan, VersionVectorDiff};

/// An indexed store for [`Commit`]s, keyed by `(peer, counter)`.
///
/// Internally a `BTreeMap<OpId, Commit>`, which is naturally sorted by peer
/// then counter. This allows efficient range queries for version-vector-based
/// sync operations.
pub struct CommitStore {
  inner: BTreeMap<OpId, Commit>,
}

impl std::fmt::Debug for CommitStore {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("CommitStore")
      .field("len", &self.inner.len())
      .finish()
  }
}

impl Default for CommitStore {
  fn default() -> Self {
    Self::new()
  }
}

impl CommitStore {
  pub fn new() -> Self {
    Self {
      inner: BTreeMap::new(),
    }
  }

  pub fn insert(&mut self, commit: Commit) {
    self.inner.insert(commit.id, commit);
  }

  pub fn len(&self) -> usize {
    self.inner.len()
  }

  pub fn is_empty(&self) -> bool {
    self.inner.is_empty()
  }

  pub fn iter(&self) -> impl Iterator<Item = &Commit> {
    self.inner.values()
  }

  /// Returns the last stored commit for the given peer, if any.
  pub fn last_for_peer(&self, peer: crate::types::PeerID) -> Option<&Commit> {
    let max_key = OpId::new(peer, crate::types::Counter::MAX);
    self
      .inner
      .range(..=max_key)
      .next_back()
      .filter(|(key, _)| key.peer == peer)
      .map(|(_, commit)| commit)
  }

  /// Finds all commits whose counter range overlaps with the given [`IdSpan`].
  ///
  /// A commit covers `[commit.id.counter, commit.end_counter())`. It overlaps
  /// with `span` when `commit.id.counter < span.end && commit.end_counter() > span.start`.
  ///
  /// Algorithm:
  /// 1. Look backwards from `OpId(peer, span.start)` to find a commit that
  ///    starts before `span.start` but extends into the range.
  /// 2. Scan forward from that point (or from `span.start`) up to
  ///    `OpId(peer, span.end)`.
  pub fn iter_span<F>(&self, span: &IdSpan, mut f: F)
  where
    F: FnMut(&Commit),
  {
    if span.is_empty() {
      return;
    }

    let span_start_key = OpId::new(span.peer, span.start);
    let span_end_key = OpId::new(span.peer, span.end);

    if let Some((key, commit)) = self.inner.range(..span_start_key).next_back()
      && key.peer == span.peer
      && commit.end_counter() > span.start
    {
      f(commit);
    }

    for (_key, commit) in self.inner.range(span_start_key..span_end_key) {
      f(commit);
    }
  }

  /// Finds all commits overlapping with any span in the given diff, deduplicated.
  pub fn iter_diff<F>(&self, diff: &VersionVectorDiff, mut f: F)
  where
    F: FnMut(&Commit),
  {
    let mut seen = std::collections::BTreeSet::new();
    for span in &diff.spans {
      self.iter_span(span, |commit| {
        if seen.insert(commit.id) {
          f(commit);
        }
      });
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::commit::Commit;
  use crate::operation::{Cmd, Op};
  use crate::types::{Counter, Lamport, ObjectIndex, ObjectType, PeerID};
  use crate::version::Heads;

  fn make_commit(
    peer: PeerID,
    counter: Counter,
    num_ops: usize,
    deps: Heads,
    lamport: Lamport,
  ) -> Commit {
    let id = OpId::new(peer, counter);
    let mut commit = Commit::new(id, lamport, 0, deps, true);
    for i in 0..num_ops {
      let op = Op::new(
        counter + i as Counter,
        ObjectIndex::new(0, ObjectType::Counter),
        Cmd::IncCounter { delta: 1.0 },
      );
      commit.push_op(op);
    }
    commit
  }

  #[test]
  fn test_insert_and_iter() {
    let mut store = CommitStore::new();
    store.insert(make_commit(1, 0, 3, Heads::new(), 0));
    store.insert(make_commit(2, 0, 2, Heads::new(), 1));

    assert_eq!(store.len(), 2);
    let commits: Vec<_> = store.iter().collect();
    assert_eq!(commits.len(), 2);
  }

  #[test]
  fn test_iter_span_single_commit_fully_contained() {
    let mut store = CommitStore::new();
    store.insert(make_commit(1, 0, 5, Heads::new(), 0));

    let span = IdSpan::new(1, 0, 5);
    let mut result = Vec::new();
    store.iter_span(&span, |c| result.push(c.id));
    assert_eq!(result.len(), 1);
    assert_eq!(result[0], OpId::new(1, 0));
  }

  #[test]
  fn test_iter_span_commit_extends_before_span() {
    let mut store = CommitStore::new();
    store.insert(make_commit(1, 0, 10, Heads::new(), 0));

    let span = IdSpan::new(1, 5, 15);
    let mut end = 0;
    store.iter_span(&span, |c| end = c.end_counter());
    assert_eq!(end, 10);
  }

  #[test]
  fn test_iter_span_gap_between_commits() {
    let mut store = CommitStore::new();
    store.insert(make_commit(1, 0, 3, Heads::new(), 0));
    store.insert(make_commit(1, 10, 3, Heads::new(), 1));

    let span = IdSpan::new(1, 3, 10);
    let mut count = 0;
    store.iter_span(&span, |_| count += 1);
    assert_eq!(count, 0);
  }

  #[test]
  fn test_iter_span_multiple_commits() {
    let mut store = CommitStore::new();
    store.insert(make_commit(1, 0, 5, Heads::new(), 0));
    store.insert(make_commit(1, 5, 5, Heads::new(), 1));
    store.insert(make_commit(1, 10, 5, Heads::new(), 2));

    let span = IdSpan::new(1, 3, 12);
    let mut count = 0;
    store.iter_span(&span, |_| count += 1);
    assert_eq!(count, 3);
  }

  #[test]
  fn test_iter_span_wrong_peer() {
    let mut store = CommitStore::new();
    store.insert(make_commit(1, 0, 5, Heads::new(), 0));

    let span = IdSpan::new(2, 0, 5);
    let mut count = 0;
    store.iter_span(&span, |_| count += 1);
    assert_eq!(count, 0);
  }

  #[test]
  fn test_iter_span_empty() {
    let mut store = CommitStore::new();
    store.insert(make_commit(1, 0, 5, Heads::new(), 0));

    let span = IdSpan::new(1, 3, 3);
    let mut count = 0;
    store.iter_span(&span, |_| count += 1);
    assert_eq!(count, 0);
  }

  #[test]
  fn test_iter_diff_deduplicates() {
    let mut store = CommitStore::new();
    store.insert(make_commit(1, 0, 10, Heads::new(), 0));

    // Two overlapping spans both covering the same commit
    let diff = VersionVectorDiff::new(vec![IdSpan::new(1, 0, 5), IdSpan::new(1, 3, 10)]);
    let mut count = 0;
    store.iter_diff(&diff, |_| count += 1);
    assert_eq!(count, 1);
  }
}
