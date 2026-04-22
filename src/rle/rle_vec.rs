use std::{
  fmt::Debug,
  marker::PhantomData,
  ops::{Deref, Index, Range},
};

use num::FromPrimitive;
use num::cast::AsPrimitive;
use smallvec::{Array, SmallVec};

use super::{HasIndex, HasLength, Mergable, RleCollection, SearchResult, SliceIterator, Sliceable};

/// A vector that automatically run-length encodes adjacent mergeable elements.
///
/// `push` tries to merge the new element with the last stored run; if they are
/// not mergeable the element is appended as a new run.
///
/// # Ordering invariant
///
/// All index-based queries (`search_atom_index`, `get_by_atom_index`,
/// `slice_by_index`, etc.) rely on binary search and thus require that the
/// underlying runs are stored in **strictly ascending order of
/// `get_start_index()`**.  This holds automatically when elements are pushed
/// in order; out-of-order insertion will break these methods.
///
/// Backed by `SmallVec<A>` so that small arrays stay on the stack.
pub struct RleVec<A: Array> {
  _p: PhantomData<fn() -> A::Item>,
  vec: SmallVec<A>,
}

impl<A: Array> RleVec<A> {
  #[inline]
  pub fn is_empty(&self) -> bool {
    self.vec.is_empty()
  }

  #[inline]
  pub fn new() -> Self {
    RleVec {
      vec: SmallVec::new(),
      _p: PhantomData,
    }
  }

  #[inline]
  pub fn with_capacity(size: usize) -> Self {
    RleVec {
      vec: SmallVec::with_capacity(size),
      _p: PhantomData,
    }
  }

  #[inline]
  pub fn capacity(&self) -> usize {
    self.vec.capacity()
  }

  /// Number of merged runs (not atoms).
  pub fn len(&self) -> usize {
    self.vec.len()
  }

  pub fn reverse(&mut self) {
    self.vec.reverse()
  }

  pub fn clear(&mut self) {
    self.vec.clear()
  }
}

impl<A: Array> IntoIterator for RleVec<A> {
  type Item = A::Item;
  type IntoIter = smallvec::IntoIter<A>;

  fn into_iter(self) -> Self::IntoIter {
    self.vec.into_iter()
  }
}

impl<A: Array> Debug for RleVec<A>
where
  A::Item: Debug,
{
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("RleVec").field("vec", &self.vec).finish()
  }
}

impl<A: Array> Clone for RleVec<A>
where
  A::Item: Clone,
{
  fn clone(&self) -> Self {
    Self {
      vec: self.vec.clone(),
      _p: PhantomData,
    }
  }
}

impl<A: Array> Index<usize> for RleVec<A> {
  type Output = A::Item;

  fn index(&self, index: usize) -> &Self::Output {
    &self.vec[index]
  }
}

impl<A: Array> PartialEq for RleVec<A>
where
  A::Item: PartialEq,
{
  fn eq(&self, other: &Self) -> bool {
    self.vec == other.vec
  }
}

impl<A: Array> Eq for RleVec<A> where A::Item: Eq + PartialEq {}

// ---------------------------------------------------------------------------
// RleVec — push
// ---------------------------------------------------------------------------

impl<A: Array> RleVec<A>
where
  A::Item: Mergable + HasLength,
{
  /// Pushes a value, merging with the last run when possible.
  ///
  /// Returns `true` if a merge happened.
  pub fn push(&mut self, value: A::Item) -> bool {
    if let Some(last) = self.vec.last_mut()
      && last.is_mergable(&value, &())
    {
      last.merge(&value, &());
      return true;
    }
    self.vec.push(value);
    false
  }
}

// ---------------------------------------------------------------------------
// RleVec — HasIndex integration
// ---------------------------------------------------------------------------

impl<A: Array> RleVec<A>
where
  A::Item: Mergable + HasLength + HasIndex,
{
  pub fn span(&self) -> <A::Item as HasIndex>::Int {
    match (self.vec.first(), self.vec.last()) {
      (Some(first), Some(last)) => last.get_end_index() - first.get_start_index(),
      _ => <<A::Item as HasIndex>::Int as FromPrimitive>::from_usize(0).unwrap(),
    }
  }

  #[inline]
  pub fn start(&self) -> <A::Item as HasIndex>::Int {
    self
      .vec
      .first()
      .map(|x| x.get_start_index())
      .unwrap_or_default()
  }

  pub fn iter_by_index(
    &self,
    from: <A::Item as HasIndex>::Int,
    to: <A::Item as HasIndex>::Int,
  ) -> SliceIterator<'_, A::Item> {
    if from == to {
      return SliceIterator::new_empty();
    }

    let start = self.get_by_atom_index(from);
    if start.is_none() {
      return SliceIterator::new_empty();
    }

    let start = start.unwrap();
    let end = self.get_by_atom_index(to);
    if let Some(end) = end {
      SliceIterator {
        vec: &self.vec,
        cur_index: start.merged_index,
        cur_offset: start.offset.as_(),
        end_index: Some(end.merged_index),
        end_offset: Some(end.offset.as_()),
      }
    } else {
      SliceIterator {
        vec: &self.vec,
        cur_index: start.merged_index,
        cur_offset: start.offset.as_(),
        end_index: None,
        end_offset: None,
      }
    }
  }

  #[inline]
  pub fn end(&self) -> <A::Item as HasIndex>::Int {
    self
      .vec
      .last()
      .map(|x| x.get_end_index())
      .unwrap_or_else(|| <<A::Item as HasIndex>::Int as FromPrimitive>::from_usize(0).unwrap())
  }

  pub fn get_by_atom_index(
    &self,
    index: <A::Item as HasIndex>::Int,
  ) -> Option<SearchResult<'_, A::Item, <A::Item as HasIndex>::Int>> {
    if index > self.end() {
      return None;
    }

    let merged_index = self.search_atom_index(index);
    let value = &self.vec[merged_index];
    Some(SearchResult {
      merged_index,
      element: value,
      offset: index - self[merged_index].get_start_index(),
    })
  }

  /// Returns the index of the merged run that contains `index`.
  ///
  /// Uses binary search, so the runs must be sorted by `get_start_index()`.
  /// The returned index is the greatest run whose `get_start_index()` is
  /// less than or equal to `index`.
  pub fn search_atom_index(&self, index: <<A as Array>::Item as HasIndex>::Int) -> usize {
    let mut start = 0;
    let mut end = self.vec.len().saturating_sub(1);
    while start < end {
      let mid = (start + end) / 2;
      match self[mid].get_start_index().cmp(&index) {
        std::cmp::Ordering::Equal => {
          start = mid;
          break;
        }
        std::cmp::Ordering::Less => {
          start = mid + 1;
        }
        std::cmp::Ordering::Greater => {
          end = mid;
        }
      }
    }

    if !self.is_empty() && index < self[start].get_start_index() {
      start = start.saturating_sub(1);
    }
    start
  }

  pub fn slice_iter(
    &self,
    from: <A::Item as HasIndex>::Int,
    to: <A::Item as HasIndex>::Int,
  ) -> SliceIterator<'_, A::Item> {
    if from == to || self.merged_len() == 0 {
      return SliceIterator::new_empty();
    }

    let from_result = self.get_by_atom_index(from);
    if from_result.is_none() {
      return SliceIterator::new_empty();
    }

    let from_result = from_result.unwrap();
    let to_result = if to == self.atom_len() {
      None
    } else {
      self.get_by_atom_index(to)
    };
    if let Some(to_result) = to_result {
      SliceIterator {
        vec: &self.vec,
        cur_index: from_result.merged_index,
        cur_offset: from_result.offset.as_(),
        end_index: Some(to_result.merged_index),
        end_offset: Some(to_result.offset.as_()),
      }
    } else {
      SliceIterator {
        vec: &self.vec,
        cur_index: from_result.merged_index,
        cur_offset: from_result.offset.as_(),
        end_index: None,
        end_offset: None,
      }
    }
  }

  #[inline]
  pub fn slice_merged(&self, range: Range<usize>) -> &[A::Item] {
    &self.vec[range]
  }

  pub fn atom_len(&self) -> <A::Item as HasIndex>::Int {
    self
      .vec
      .last()
      .map(|x| x.get_end_index() - self.vec.first().unwrap().get_start_index())
      .unwrap_or(<A::Item as HasIndex>::Int::from_usize(0).unwrap())
  }
}

impl<A: Array> RleCollection<A::Item> for RleVec<A>
where
  A::Item: Mergable + HasLength + HasIndex,
{
  fn start(&self) -> <A::Item as HasIndex>::Int {
    self
      .vec
      .first()
      .map(|x| x.get_start_index())
      .unwrap_or_default()
  }

  fn end(&self) -> <A::Item as HasIndex>::Int {
    self
      .vec
      .last()
      .map(|x| x.get_end_index())
      .unwrap_or_else(|| <<A::Item as HasIndex>::Int as FromPrimitive>::from_usize(0).unwrap())
  }

  fn sum_atom_len(&self) -> <A::Item as HasIndex>::Int {
    self.end() - self.start()
  }

  fn search_atom_index(&self, index: <<A as Array>::Item as HasIndex>::Int) -> usize {
    let mut start = 0;
    let mut end = self.vec.len().saturating_sub(1);
    while start < end {
      let mid = (start + end) / 2;
      match self[mid].get_start_index().cmp(&index) {
        std::cmp::Ordering::Equal => {
          start = mid;
          break;
        }
        std::cmp::Ordering::Less => {
          start = mid + 1;
        }
        std::cmp::Ordering::Greater => {
          end = mid;
        }
      }
    }

    if !self.is_empty() && index < self[start].get_start_index() {
      start = start.saturating_sub(1);
    }
    start
  }

  fn get_by_atom_index(
    &self,
    index: <A::Item as HasIndex>::Int,
  ) -> Option<SearchResult<'_, A::Item, <A::Item as HasIndex>::Int>> {
    if index > self.end() {
      return None;
    }

    let merged_index = self.search_atom_index(index);
    let value = &self.vec[merged_index];
    Some(SearchResult {
      merged_index,
      element: value,
      offset: index - self[merged_index].get_start_index(),
    })
  }
}

impl<A: Array> RleVec<A>
where
  A::Item: Mergable + HasLength + HasIndex + Sliceable,
{
  /// This is different from [Sliceable::slice].
  /// This slice method is based on each element's [HasIndex].
  /// [Sliceable::slice] is based on the accumulated length of each element.
  pub fn slice_by_index(
    &self,
    from: <A::Item as HasIndex>::Int,
    to: <A::Item as HasIndex>::Int,
  ) -> Self {
    self
      .iter_by_index(from, to)
      .map(|x| x.value.slice(x.start, x.end))
      .collect()
  }
}

// ---------------------------------------------------------------------------
// Conversions
// ---------------------------------------------------------------------------

impl<A: Array> From<Vec<A::Item>> for RleVec<A>
where
  A::Item: Mergable + HasLength,
{
  fn from(vec: Vec<A::Item>) -> Self {
    let mut ans: RleVec<A> = RleVec::with_capacity(vec.len());
    for v in vec {
      ans.push(v);
    }
    ans.vec.shrink_to_fit();
    ans
  }
}

impl<A: Array> From<&[A::Item]> for RleVec<A>
where
  A::Item: Mergable + HasLength + Clone,
{
  fn from(value: &[A::Item]) -> Self {
    let mut ans: RleVec<A> = RleVec::with_capacity(value.len());
    for v in value.iter() {
      ans.push(v.clone());
    }
    ans.vec.shrink_to_fit();
    ans
  }
}

impl<A: Array> From<SmallVec<A>> for RleVec<A> {
  fn from(value: SmallVec<A>) -> Self {
    RleVec {
      vec: value,
      _p: PhantomData,
    }
  }
}

impl<A: Array> From<RleVec<A>> for SmallVec<A> {
  fn from(value: RleVec<A>) -> Self {
    value.vec
  }
}

// ---------------------------------------------------------------------------
// RleVec — accessors
// ---------------------------------------------------------------------------

impl<A: Array> RleVec<A> {
  #[inline(always)]
  pub fn merged_len(&self) -> usize {
    self.vec.len()
  }

  #[inline(always)]
  pub fn vec(&self) -> &SmallVec<A> {
    &self.vec
  }

  #[inline(always)]
  pub fn vec_mut(&mut self) -> &mut SmallVec<A> {
    &mut self.vec
  }

  #[inline(always)]
  pub fn iter(&self) -> std::slice::Iter<'_, A::Item> {
    self.vec.iter()
  }

  #[inline(always)]
  pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, A::Item> {
    self.vec.iter_mut()
  }

  #[inline(always)]
  pub fn get_merged(&self, index: usize) -> Option<&A::Item> {
    self.vec.get(index)
  }
}

impl<A: Array> Default for RleVec<A> {
  fn default() -> Self {
    Self::new()
  }
}

impl<A: Array> FromIterator<A::Item> for RleVec<A>
where
  A::Item: Mergable + HasLength,
{
  fn from_iter<I: IntoIterator<Item = A::Item>>(iter: I) -> Self {
    let mut vec = RleVec::new();
    for item in iter {
      vec.push(item);
    }
    vec
  }
}

// ---------------------------------------------------------------------------
// Mergable & Sliceable for RleVec itself
// ---------------------------------------------------------------------------

impl<A: Array> Mergable for RleVec<A>
where
  A::Item: Clone + Mergable + HasLength + Sliceable,
{
  fn is_mergable(&self, other: &Self, _: &()) -> bool {
    self.vec.len() + other.vec.len() < self.capacity()
  }

  fn merge(&mut self, other: &Self, _: &()) {
    for item in other.vec.iter() {
      self.push(item.clone());
    }
  }
}

impl<A: Array> Sliceable for RleVec<A>
where
  A::Item: Mergable + HasLength + Sliceable,
{
  fn slice(&self, start: usize, end: usize) -> Self {
    if start >= end {
      return Self::new();
    }

    let mut ans = SmallVec::new();
    let mut index = 0;
    for i in 0..self.vec.len() {
      if index >= end {
        break;
      }

      let len = self[i].atom_len();
      if start < index + len {
        ans.push(self[i].slice(start.saturating_sub(index), (end - index).min(len)))
      }

      index += len;
    }

    Self {
      vec: ans,
      _p: PhantomData,
    }
  }
}

// ---------------------------------------------------------------------------
// Deref
// ---------------------------------------------------------------------------

impl<A: Array> Deref for RleVec<A> {
  type Target = [A::Item];

  fn deref(&self) -> &Self::Target {
    &self.vec
  }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  #[allow(clippy::single_range_in_vec_init)]
  fn slice() {
    let mut a: RleVec<[Range<usize>; 4]> = RleVec::new();
    assert!(!a.push(0..5));
    assert!(a.push(5..8));
    assert!(!a.push(10..13));
    assert_eq!(&*a.slice(0, 5), &vec![0..5]);
    assert_eq!(&*a.slice(5, 10), &vec![5..8, 10..12]);
    assert_eq!(&*a.slice(5, 5), &vec![]);

    let ans = a.slice_by_index(3, 11);
    assert_eq!(&*ans, &vec![3..8, 10..11]);
    let ans = a.slice_by_index(3, 100);
    assert_eq!(&*ans, &vec![3..8, 10..13]);
    assert_eq!(*a.last().unwrap(), 10..13);
    for k in a.iter() {
      println!("{k:?}");
    }
  }

  #[test]
  fn push_merge_contiguous() {
    let mut a: RleVec<[Range<usize>; 4]> = RleVec::new();
    assert!(!a.push(0..2));
    assert!(a.push(2..5));
    assert_eq!(a.len(), 1);
    assert_eq!(a[0], 0..5);
  }

  #[test]
  fn push_no_merge_gap() {
    let mut a: RleVec<[Range<usize>; 4]> = RleVec::new();
    assert!(!a.push(0..2));
    assert!(!a.push(5..8));
    assert_eq!(a.len(), 2);
  }

  #[test]
  fn slice_empty_range() {
    let mut a: RleVec<[Range<usize>; 4]> = RleVec::new();
    a.push(0..3);
    let sliced = a.slice(1, 1);
    assert!(sliced.is_empty());
  }

  #[test]
  fn from_vec() {
    let v = vec![0..2, 2..5, 10..12];
    let a: RleVec<[Range<usize>; 4]> = RleVec::from(v);
    assert_eq!(a.len(), 2); // 0..2 + 2..5 merges, 10..12 stays
    assert_eq!(a[0], 0..5);
    assert_eq!(a[1], 10..12);
  }

  #[test]
  fn index_search() {
    let mut a: RleVec<[Range<usize>; 4]> = RleVec::new();
    a.push(0..5);
    a.push(6..10); // gap of 1 to prevent merge
    assert_eq!(a.search_atom_index(0), 0);
    assert_eq!(a.search_atom_index(4), 0);
    assert_eq!(a.search_atom_index(5), 0); // gap falls into previous run's end
    assert_eq!(a.search_atom_index(6), 1);
    assert_eq!(a.search_atom_index(9), 1);
  }
}
