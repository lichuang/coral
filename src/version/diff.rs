//! Difference between two version vectors.
//!
//! [`VersionVectorDiff`] captures the spans that need to be applied (forward)
//! or undone (retreat) to move from one version to another.

use crate::types::PeerID;
use crate::version::span::{CounterSpan, IdSpan};
use rustc_hash::FxHashMap;

/// A map from peer to counter span.
pub type IdSpanVector = FxHashMap<PeerID, CounterSpan>;

/// The difference between two version vectors.
///
/// - `retreat`: spans present in the **left** side but not in the **right**
///   side — operations that need to be undone.
/// - `forward`: spans present in the **right** side but not in the **left**
///   side — operations that need to be applied.
#[derive(Default, Debug, PartialEq, Eq)]
pub struct VersionVectorDiff {
  pub retreat: IdSpanVector,
  pub forward: IdSpanVector,
}

impl VersionVectorDiff {
  /// Merge a span into the retreat side.
  #[inline]
  pub fn merge_left(&mut self, span: IdSpan) {
    merge_span(&mut self.retreat, span);
  }

  /// Merge a span into the forward side.
  #[inline]
  pub fn merge_right(&mut self, span: IdSpan) {
    merge_span(&mut self.forward, span);
  }

  /// Trim the start of a retreat span.
  #[inline]
  pub fn subtract_start_left(&mut self, span: IdSpan) {
    subtract_start(&mut self.retreat, span);
  }

  /// Trim the start of a forward span.
  #[inline]
  pub fn subtract_start_right(&mut self, span: IdSpan) {
    subtract_start(&mut self.forward, span);
  }

  /// Iterate over the retreat spans as [`IdSpan`]s.
  pub fn get_id_spans_left(&self) -> impl Iterator<Item = IdSpan> + '_ {
    self.retreat.iter().map(|(&peer, &span)| IdSpan {
      peer,
      counter: span,
    })
  }

  /// Iterate over the forward spans as [`IdSpan`]s.
  pub fn get_id_spans_right(&self) -> impl Iterator<Item = IdSpan> + '_ {
    self.forward.iter().map(|(&peer, &span)| IdSpan {
      peer,
      counter: span,
    })
  }
}

/// Merge a span into an `IdSpanVector`, extending an existing entry
/// for the same peer if present.
fn merge_span(m: &mut IdSpanVector, target: IdSpan) {
  if let Some(span) = m.get_mut(&target.peer) {
    span.start = span.start.min(target.counter.start);
    span.end = span.end.max(target.counter.end);
  } else {
    m.insert(target.peer, target.counter);
  }
}

/// Trim the start of a span in an `IdSpanVector`.
fn subtract_start(m: &mut IdSpanVector, target: IdSpan) {
  if let Some(span) = m.get_mut(&target.peer)
    && span.start < target.counter.end
  {
    span.start = target.counter.end;
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_vv_diff_merge_left_extends_existing() {
    let mut d = VersionVectorDiff::default();
    d.merge_left(IdSpan::new(1, 0, 3));
    d.merge_left(IdSpan::new(1, 5, 8));
    let spans: Vec<_> = d.get_id_spans_left().collect();
    assert_eq!(spans.len(), 1);
    // merged: start=min(0,5)=0, end=max(3,8)=8
    assert_eq!(spans[0].counter, CounterSpan::new(0, 8));
  }

  #[test]
  fn test_vv_diff_merge_right() {
    let mut d = VersionVectorDiff::default();
    d.merge_right(IdSpan::new(2, 0, 3));
    let spans: Vec<_> = d.get_id_spans_right().collect();
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].peer, 2);
    assert_eq!(spans[0].counter, CounterSpan::new(0, 3));
  }

  #[test]
  fn test_vv_diff_subtract_start_left() {
    let mut d = VersionVectorDiff::default();
    d.merge_left(IdSpan::new(1, 0, 10));
    d.subtract_start_left(IdSpan::new(1, 0, 3));
    let spans: Vec<_> = d.get_id_spans_left().collect();
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].counter, CounterSpan::new(3, 10));
  }

  #[test]
  fn test_vv_diff_subtract_start_left_no_op_when_after() {
    let mut d = VersionVectorDiff::default();
    d.merge_left(IdSpan::new(1, 5, 10));
    d.subtract_start_left(IdSpan::new(1, 0, 3));
    // 5 >= 3, so no change
    let spans: Vec<_> = d.get_id_spans_left().collect();
    assert_eq!(spans[0].counter, CounterSpan::new(5, 10));
  }

  #[test]
  fn test_vv_diff_subtract_start_left_missing_peer() {
    let mut d = VersionVectorDiff::default();
    d.subtract_start_left(IdSpan::new(1, 0, 3));
    assert!(d.retreat.is_empty());
  }

  #[test]
  fn test_vv_diff_subtract_start_right() {
    let mut d = VersionVectorDiff::default();
    d.merge_right(IdSpan::new(2, 0, 10));
    d.subtract_start_right(IdSpan::new(2, 0, 4));
    let spans: Vec<_> = d.get_id_spans_right().collect();
    assert_eq!(spans[0].counter, CounterSpan::new(4, 10));
  }

  #[test]
  fn test_vv_diff_both_sides() {
    let mut d = VersionVectorDiff::default();
    d.merge_left(IdSpan::new(1, 0, 5));
    d.merge_right(IdSpan::new(2, 0, 3));
    assert_eq!(d.get_id_spans_left().count(), 1);
    assert_eq!(d.get_id_spans_right().count(), 1);
    assert_eq!(d.retreat.get(&1).copied(), Some(CounterSpan::new(0, 5)));
    assert_eq!(d.forward.get(&2).copied(), Some(CounterSpan::new(0, 3)));
  }

  #[test]
  fn test_vv_diff_default_is_empty() {
    let d = VersionVectorDiff::default();
    assert!(d.retreat.is_empty());
    assert!(d.forward.is_empty());
    assert_eq!(d.get_id_spans_left().count(), 0);
    assert_eq!(d.get_id_spans_right().count(), 0);
  }
}
