use super::{HasLength, Mergeable};

/// A vector that automatically merges adjacent elements using run-length
/// encoding (RLE) when possible.
///
/// `RleVec` stores elements in a contiguous array, but when a new element
/// is pushed, it first checks whether the last stored element can be merged
/// with the new one. If so, the two are combined in-place rather than
/// appending a new entry. This can dramatically reduce memory usage when
/// the underlying data contains long runs of mergeable items.
#[derive(Debug, Clone, PartialEq)]
pub struct RleVec<T> {
  // TODO: use smallvec intead
  vec: Vec<T>,
}

impl<T> RleVec<T> {
  /// Creates an empty `RleVec`.
  pub fn new() -> Self {
    Self { vec: Vec::new() }
  }

  /// Creates an empty `RleVec` with the given capacity.
  pub fn with_capacity(capacity: usize) -> Self {
    Self {
      vec: Vec::with_capacity(capacity),
    }
  }

  /// Returns the number of stored elements.
  pub fn len(&self) -> usize {
    self.vec.len()
  }

  /// Returns `true` if there are no stored elements.
  pub fn is_empty(&self) -> bool {
    self.vec.is_empty()
  }

  /// Returns a reference to the element at the given index, or `None` if
  /// out of bounds.
  pub fn get(&self, index: usize) -> Option<&T> {
    self.vec.get(index)
  }

  /// Returns a reference to the last element, or `None` if empty.
  pub fn last(&self) -> Option<&T> {
    self.vec.last()
  }

  /// Returns an iterator over the stored elements.
  pub fn iter(&self) -> std::slice::Iter<'_, T> {
    self.vec.iter()
  }
}

impl<T: Mergeable + HasLength> RleVec<T> {
  /// Pushes a new element into the vector.
  ///
  /// If the last stored element is mergeable with `value`, the two are
  /// combined in-place and no new entry is appended.
  pub fn push(&mut self, value: T) {
    if let Some(last) = self.vec.last()
      && last.is_mergeable(&value)
    {
      let last = self.vec.last_mut().unwrap();
      last.merge(&value);
      return;
    }
    self.vec.push(value);
  }
}

impl<T> Default for RleVec<T> {
  fn default() -> Self {
    Self::new()
  }
}

impl<T> std::ops::Deref for RleVec<T> {
  type Target = [T];

  fn deref(&self) -> &Self::Target {
    &self.vec
  }
}

impl<T> From<Vec<T>> for RleVec<T> {
  fn from(vec: Vec<T>) -> Self {
    Self { vec }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[derive(Debug, Clone, PartialEq)]
  struct Run {
    start: u32,
    len: usize,
  }

  impl HasLength for Run {
    fn content_len(&self) -> usize {
      self.len
    }
  }

  impl Mergeable for Run {
    fn is_mergeable(&self, other: &Self) -> bool {
      self.start + self.len as u32 == other.start
    }

    fn merge(&mut self, other: &Self) {
      self.len += other.len;
    }
  }

  #[test]
  fn test_rle_vec_push_and_merge() {
    let mut rle = RleVec::new();
    rle.push(Run { start: 0, len: 3 });
    rle.push(Run { start: 3, len: 2 });
    rle.push(Run { start: 10, len: 1 });

    assert_eq!(rle.len(), 2);
    assert_eq!(rle.get(0), Some(&Run { start: 0, len: 5 }));
    assert_eq!(rle.get(1), Some(&Run { start: 10, len: 1 }));
  }

  #[test]
  fn test_rle_vec_no_merge() {
    let mut rle = RleVec::new();
    rle.push(Run { start: 0, len: 3 });
    rle.push(Run { start: 5, len: 2 });

    assert_eq!(rle.len(), 2);
    assert_eq!(rle.get(0), Some(&Run { start: 0, len: 3 }));
    assert_eq!(rle.get(1), Some(&Run { start: 5, len: 2 }));
  }

  #[test]
  fn test_rle_vec_is_empty() {
    let rle: RleVec<Run> = RleVec::new();
    assert!(rle.is_empty());
  }

  #[test]
  fn test_rle_vec_from_vec() {
    let rle: RleVec<Run> = vec![Run { start: 0, len: 3 }, Run { start: 3, len: 2 }].into();

    // From<Vec> does not merge, so len is 2
    assert_eq!(rle.len(), 2);
  }
}
