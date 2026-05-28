//! Traits enabling run-length encoding of mergeable items.
//!
//! Types that implement [`HasLength`] and [`Mergeable`] can be stored
//! compactly inside an [`RleVec`](super::RleVec), where consecutive
//! compatible items are collapsed into a single entry.

/// A type that has a content length.
pub trait HasLength {
  /// Returns the length of the content.
  fn content_len(&self) -> usize;
}

/// A type that can be merged with another of the same type.
pub trait Mergeable {
  /// Returns whether `self` can be merged with `other`.
  fn is_mergeable(&self, other: &Self) -> bool;

  /// Merges `other` into `self`.
  ///
  /// # Panics
  ///
  /// May panic if `is_mergeable` returns `false`.
  fn merge(&mut self, other: &Self);
}
