//! Span types for representing ranges of operations.

use crate::types::{Counter, ID, PeerID};

/// A range of counters that may be in reverse representation.
///
/// `start` may be greater than `end` for convenience when representing
/// deletions. Call [`normalize`](CounterSpan::normalize) to ensure
/// `start < end` before iteration.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct CounterSpan {
  pub start: Counter,
  pub end: Counter,
}

impl std::fmt::Debug for CounterSpan {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}~{}", self.start, self.end)
  }
}

impl CounterSpan {
  #[inline]
  pub fn new(from: Counter, to: Counter) -> Self {
    Self {
      start: from,
      end: to,
    }
  }

  /// Reverse the span in-place.
  #[inline]
  pub fn reverse(&mut self) {
    if self.start == self.end {
      return;
    }
    if self.start < self.end {
      (self.start, self.end) = (self.end - 1, self.start - 1);
    } else {
      (self.start, self.end) = (self.end + 1, self.start + 1);
    }
  }

  /// Ensure `end >= start`.
  pub fn normalize(&mut self) {
    if self.end < self.start {
      self.reverse();
    }
  }

  #[inline]
  pub fn is_reversed(&self) -> bool {
    self.end < self.start
  }

  #[inline]
  pub fn bidirectional(&self) -> bool {
    (self.end - self.start).abs() == 1
  }

  #[inline]
  pub fn direction(&self) -> i32 {
    if self.start < self.end { 1 } else { -1 }
  }

  /// Smallest counter included in the span (regardless of direction).
  #[inline]
  pub fn min(&self) -> Counter {
    if self.start < self.end {
      self.start
    } else {
      self.end + 1
    }
  }

  /// Largest counter included in the span (regardless of direction).
  #[inline]
  pub fn max(&self) -> Counter {
    if self.start > self.end {
      self.start
    } else {
      self.end - 1
    }
  }

  /// The normalized (exclusive) end.
  #[inline]
  pub fn norm_end(&self) -> Counter {
    if self.start < self.end {
      self.end
    } else {
      self.start + 1
    }
  }

  /// Number of atomic operations in this span.
  #[inline]
  pub fn len(&self) -> usize {
    (self.start - self.end).unsigned_abs() as usize
  }

  #[inline]
  pub fn is_empty(&self) -> bool {
    self.start == self.end
  }

  /// Returns `true` if `v` lies inside this span.
  #[inline]
  pub fn contains(&self, v: Counter) -> bool {
    if self.start < self.end {
      self.start <= v && v < self.end
    } else {
      self.start >= v && v > self.end
    }
  }

  /// Set the start bound while keeping direction valid.
  pub fn set_start(&mut self, new_start: Counter) {
    if self.start < self.end {
      self.start = new_start.min(self.end);
    } else {
      self.start = new_start.max(self.end);
    }
  }

  /// Set the end bound while keeping direction valid.
  pub fn set_end(&mut self, new_end: Counter) {
    if self.start < self.end {
      self.end = new_end.max(self.start);
    } else {
      self.end = new_end.min(self.start);
    }
  }

  /// Expand this span to include both bounds.
  pub fn extend_include(&mut self, new_start: Counter, new_end: Counter) {
    self.set_start(new_start);
    self.set_end(new_end);
  }

  /// Intersection with another span.
  pub fn get_intersection(&self, other: &Self) -> Option<Self> {
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

  #[inline]
  pub fn normalize(&mut self) {
    self.counter.normalize();
  }

  #[inline]
  pub fn is_reversed(&self) -> bool {
    self.counter.is_reversed()
  }

  #[inline]
  pub fn reverse(&mut self) {
    self.counter.reverse();
  }

  /// The normalized start ID (smallest counter).
  #[inline]
  pub fn norm_id_start(&self) -> ID {
    ID::new(self.peer, self.counter.min())
  }

  /// The normalized (exclusive) end ID.
  #[inline]
  pub fn norm_id_end(&self) -> ID {
    ID::new(self.peer, self.counter.norm_end())
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
  fn test_counter_span_reverse() {
    let mut s = CounterSpan::new(5, 0);
    assert!(s.is_reversed());
    // 5~0 contains {1,2,3,4,5}; after normalize it becomes 1~6
    s.normalize();
    assert!(!s.is_reversed());
    assert_eq!(s, CounterSpan::new(1, 6));
  }

  #[test]
  fn test_counter_span_min_max_norm_end() {
    let s = CounterSpan::new(3, 10);
    assert_eq!(s.min(), 3);
    assert_eq!(s.max(), 9);
    assert_eq!(s.norm_end(), 10);

    let s2 = CounterSpan::new(10, 3);
    assert_eq!(s2.min(), 4);
    assert_eq!(s2.max(), 10);
    assert_eq!(s2.norm_end(), 11);
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
    assert_eq!(CounterSpan::new(5, 0).len(), 5);
    assert_eq!(CounterSpan::new(3, 3).len(), 0);
  }

  #[test]
  fn test_counter_span_is_empty() {
    assert!(CounterSpan::new(3, 3).is_empty());
    assert!(!CounterSpan::new(0, 5).is_empty());
    assert!(!CounterSpan::new(5, 0).is_empty());
  }

  #[test]
  fn test_counter_span_bidirectional() {
    assert!(CounterSpan::new(0, 1).bidirectional());
    assert!(CounterSpan::new(1, 0).bidirectional());
    assert!(!CounterSpan::new(0, 2).bidirectional());
    assert!(!CounterSpan::new(0, 0).bidirectional());
  }

  #[test]
  fn test_counter_span_direction() {
    assert_eq!(CounterSpan::new(0, 5).direction(), 1);
    assert_eq!(CounterSpan::new(5, 0).direction(), -1);
  }

  #[test]
  fn test_counter_span_contains_reversed() {
    let s = CounterSpan::new(5, 0);
    // 5~0 means {5,4,3,2,1}
    assert!(s.contains(5));
    assert!(s.contains(1));
    assert!(!s.contains(0));
    assert!(!s.contains(6));
  }

  #[test]
  fn test_counter_span_set_start() {
    let mut s = CounterSpan::new(0, 5);
    s.set_start(2);
    assert_eq!(s, CounterSpan::new(2, 5));

    // reversed span: start=5, end=0
    let mut s2 = CounterSpan::new(5, 0);
    s2.set_start(2);
    // set_start for reversed: self.start = new_start.max(self.end) = 2.max(0) = 2
    assert_eq!(s2.start, 2);
  }

  #[test]
  fn test_counter_span_set_end() {
    let mut s = CounterSpan::new(0, 5);
    s.set_end(3);
    assert_eq!(s, CounterSpan::new(0, 3));

    // reversed span: start=5, end=0
    let mut s2 = CounterSpan::new(5, 0);
    s2.set_end(2);
    // set_end for reversed: self.end = new_end.min(self.start) = 2.min(5) = 2
    assert_eq!(s2.end, 2);
  }

  #[test]
  fn test_counter_span_extend_include_normal() {
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

  #[test]
  fn test_counter_span_get_intersection_reversed() {
    // get_intersection does not normalize; it does raw start/end comparison.
    let a = CounterSpan::new(0, 5);
    let b = CounterSpan::new(5, 0);
    assert!(a.get_intersection(&b).is_none());

    // After normalizing b, they overlap at 1..5.
    let mut b_norm = CounterSpan::new(5, 0);
    b_norm.normalize();
    let inter = a.get_intersection(&b_norm);
    assert_eq!(inter, Some(CounterSpan::new(1, 5)));
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
  fn test_id_span_normalize() {
    let mut span = IdSpan::new(1, 5, 0);
    span.normalize();
    assert!(!span.is_reversed());
    // 5~0 contains {1,2,3,4,5}; after normalize 1~6 -> start=1, end=6
    assert_eq!(span.norm_id_start(), ID::new(1, 1));
    assert_eq!(span.norm_id_end(), ID::new(1, 6));
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
  fn test_id_span_contains_reversed() {
    let span = IdSpan::new(1, 5, 0);
    // 5~0 contains {5,4,3,2,1}
    assert!(span.contains(ID::new(1, 5)));
    assert!(span.contains(ID::new(1, 1)));
    assert!(!span.contains(ID::new(1, 0)));
    assert!(!span.contains(ID::new(1, 6)));
  }

  #[test]
  fn test_id_span_reverse() {
    let mut span = IdSpan::new(1, 0, 5);
    assert!(!span.is_reversed());
    span.reverse();
    assert!(span.is_reversed());
    // 0~5 forward -> after reverse: 5~-1, i.e. reversed
    assert_eq!(span.counter.start, 4);
    assert_eq!(span.counter.end, -1);
  }

  #[test]
  fn test_id_span_norm_id_on_normal() {
    let span = IdSpan::new(1, 0, 5);
    assert_eq!(span.norm_id_start(), ID::new(1, 0));
    assert_eq!(span.norm_id_end(), ID::new(1, 5));
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
