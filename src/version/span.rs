//! Span types for representing ranges of operations.
//!
//! [`CounterSpan`] is a simple right-open interval `[start, end)` where
//! `start <= end` is always true.  It is used by [`VersionVector`](super::VersionVector)
//! to record how many operations from a peer are known.
//!
//! **Design note**:  Direction semantics for deletions (e.g. a reversed delete
//! span) live in a separate [`DeleteSpan`](crate::op::DeleteSpan) type rather
//! than being folded into `CounterSpan`.  Keeping the two concerns separate
//! matches Loro's design and avoids the ambiguity that `start > end` creates
//! for a generic range type.

use crate::types::{Counter, ID, PeerID};

/// A right-open counter interval `[start, end)`.
///
/// Invariant: `start <= end`.  Callers are responsible for ensuring this;
/// `CounterSpan` itself does not swap or clamp arguments in [`new`](Self::new).
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct CounterSpan {
  /// Inclusive lower bound.
  pub start: Counter,
  /// Exclusive upper bound.
  pub end: Counter,
}

impl std::fmt::Debug for CounterSpan {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}~{}", self.start, self.end)
  }
}

impl CounterSpan {
  /// Creates a new span `[from, to)`.
  ///
  /// # Panics
  ///
  /// Debug builds panic when `from > to` to catch logic errors early.
  #[inline]
  pub fn new(from: Counter, to: Counter) -> Self {
    debug_assert!(
      from <= to,
      "CounterSpan requires start <= end, got {from} > {to}"
    );
    Self {
      start: from,
      end: to,
    }
  }

  /// Number of atomic operations in this span.
  #[inline]
  pub fn len(&self) -> usize {
    (self.end - self.start) as usize
  }

  #[inline]
  pub fn is_empty(&self) -> bool {
    self.start == self.end
  }

  /// Returns `true` if `v` lies inside `[start, end)`.
  #[inline]
  pub fn contains(&self, v: Counter) -> bool {
    self.start <= v && v < self.end
  }

  /// Set the start bound, clamped so that `start <= end`.
  pub fn set_start(&mut self, new_start: Counter) {
    self.start = new_start.min(self.end);
  }

  /// Set the end bound, clamped so that `end >= start`.
  pub fn set_end(&mut self, new_end: Counter) {
    self.end = new_end.max(self.start);
  }

  /// Expand this span to include both bounds.
  pub fn extend_include(&mut self, new_start: Counter, new_end: Counter) {
    self.start = self.start.min(new_start);
    self.end = self.end.max(new_end);
  }

  /// Intersection with another span.
  fn get_intersection(&self, other: &Self) -> Option<Self> {
    let start = self.start.max(other.start);
    let end = self.end.min(other.end);
    if start < end {
      Some(Self { start, end })
    } else {
      None
    }
  }
}

/// A span identified by both a peer and a counter range.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct IdSpan {
  pub peer: PeerID,
  pub counter: CounterSpan,
}

impl IdSpan {
  #[inline]
  pub fn new(peer: PeerID, from: Counter, to: Counter) -> Self {
    Self {
      peer,
      counter: CounterSpan::new(from, to),
    }
  }

  #[inline]
  pub fn contains(&self, id: ID) -> bool {
    self.peer == id.peer && self.counter.contains(id.counter)
  }

  /// The start ID (`start` counter).
  #[inline]
  pub fn id_start(&self) -> ID {
    ID::new(self.peer, self.counter.start)
  }

  /// The exclusive end ID (`end` counter).
  #[inline]
  pub fn id_end(&self) -> ID {
    ID::new(self.peer, self.counter.end)
  }

  /// Length of this span in atomic operations.
  #[inline]
  pub fn len(&self) -> usize {
    self.counter.len()
  }

  #[inline]
  pub fn is_empty(&self) -> bool {
    self.counter.is_empty()
  }

  pub fn get_intersection(&self, other: &Self) -> Option<Self> {
    if self.peer != other.peer {
      return None;
    }
    let counter = self.counter.get_intersection(&other.counter)?;
    Some(Self {
      peer: self.peer,
      counter,
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  // ── CounterSpan ──────────────────────────────────────────────────

  #[test]
  fn test_counter_span_new_and_contains() {
    let s = CounterSpan::new(0, 5);
    assert!(s.contains(0));
    assert!(s.contains(4));
    assert!(!s.contains(5));
    assert!(!s.contains(-1));
  }

  #[test]
  fn test_counter_span_get_intersection() {
    let a = CounterSpan::new(0, 5);
    let b = CounterSpan::new(3, 8);
    assert_eq!(a.get_intersection(&b), Some(CounterSpan::new(3, 5)));
    let c = CounterSpan::new(5, 10);
    assert!(a.get_intersection(&c).is_none());
  }

  #[test]
  fn test_counter_span_len() {
    assert_eq!(CounterSpan::new(0, 5).len(), 5);
    assert_eq!(CounterSpan::new(3, 3).len(), 0);
  }

  #[test]
  fn test_counter_span_is_empty() {
    assert!(CounterSpan::new(3, 3).is_empty());
    assert!(!CounterSpan::new(0, 5).is_empty());
  }

  #[test]
  fn test_counter_span_set_start() {
    let mut s = CounterSpan::new(0, 5);
    s.set_start(2);
    assert_eq!(s, CounterSpan::new(2, 5));

    // clamped to end
    s.set_start(10);
    assert_eq!(s, CounterSpan::new(5, 5));
  }

  #[test]
  fn test_counter_span_set_end() {
    let mut s = CounterSpan::new(0, 5);
    s.set_end(3);
    assert_eq!(s, CounterSpan::new(0, 3));

    // clamped to start
    s.set_end(-1);
    assert_eq!(s, CounterSpan::new(0, 0));
  }

  #[test]
  fn test_counter_span_extend_include() {
    let mut s = CounterSpan::new(3, 5);
    s.extend_include(0, 7);
    assert_eq!(s, CounterSpan::new(0, 7));
  }

  #[test]
  fn test_counter_span_get_intersection_no_overlap() {
    let a = CounterSpan::new(0, 3);
    let b = CounterSpan::new(5, 8);
    assert!(a.get_intersection(&b).is_none());
  }

  // ── IdSpan ───────────────────────────────────────────────────────

  #[test]
  fn test_id_span_new_and_contains() {
    let span = IdSpan::new(1, 0, 5);
    assert!(span.contains(ID::new(1, 0)));
    assert!(span.contains(ID::new(1, 4)));
    assert!(!span.contains(ID::new(1, 5)));
    assert!(!span.contains(ID::new(2, 0)));
  }

  #[test]
  fn test_id_span_get_intersection() {
    let a = IdSpan::new(1, 0, 5);
    let b = IdSpan::new(1, 3, 8);
    assert_eq!(a.get_intersection(&b), Some(IdSpan::new(1, 3, 5)));
    let c = IdSpan::new(2, 0, 5);
    assert!(a.get_intersection(&c).is_none());
  }

  #[test]
  fn test_id_span_id_start_and_end() {
    let span = IdSpan::new(1, 0, 5);
    assert_eq!(span.id_start(), ID::new(1, 0));
    assert_eq!(span.id_end(), ID::new(1, 5));
  }

  #[test]
  fn test_id_span_len_and_is_empty() {
    let span = IdSpan::new(1, 0, 5);
    assert_eq!(span.len(), 5);
    assert!(!span.is_empty());

    let empty = IdSpan::new(1, 3, 3);
    assert_eq!(empty.len(), 0);
    assert!(empty.is_empty());
  }

  #[test]
  fn test_id_span_get_intersection_no_overlap() {
    let a = IdSpan::new(1, 0, 3);
    let b = IdSpan::new(1, 5, 8);
    assert!(a.get_intersection(&b).is_none());
  }

  #[test]
  fn test_id_span_contains_wrong_peer() {
    let span = IdSpan::new(1, 0, 5);
    assert!(!span.contains(ID::new(2, 2)));
    assert!(!span.contains(ID::new(0, 2)));
  }
}
