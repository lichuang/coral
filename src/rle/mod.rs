//! Run-Length Encoding (RLE) infrastructure.
//!
//! This module provides the core traits and containers that enable `Change`s
//! and `Op`s to be merged, sliced, and stored compactly — the same primitives
//! used inside `RleVec` and causal-graph iterators.
//!
//! # Design note
//!
//! The API and internal structure of `RleVec` are intentionally aligned with
//! the reference design so that future algorithms (checkout,
//! diff, encoding) can be done with minimal friction.

pub mod rle_impl;
pub mod rle_vec;

pub use rle_vec::*;

use num::FromPrimitive;
use num::Integer;
use num::cast::AsPrimitive;
use smallvec::Array;

// ---------------------------------------------------------------------------
// Mergable
// ---------------------------------------------------------------------------

/// A type whose adjacent instances can be merged into one.
///
/// This is the foundation of run-length encoding: two consecutive `Op`s that
/// touch the same container and have compatible content can be stored as a
/// single wider run.
///
/// The generic parameter `Cfg` allows external merge policy to be passed in.
/// When no configuration is needed, use the default `()`.
pub trait Mergable<Cfg = ()> {
  /// Returns `true` if `self` and `other` can be merged.
  fn is_mergable(&self, _other: &Self, _conf: &Cfg) -> bool
  where
    Self: Sized,
  {
    false
  }

  /// Merges `other` into `self`.
  ///
  /// # Panics
  ///
  /// May panic if `is_mergable` would return `false`; callers must check
  /// first.
  fn merge(&mut self, _other: &Self, _conf: &Cfg)
  where
    Self: Sized,
  {
    unreachable!()
  }
}

// ---------------------------------------------------------------------------
// Sliceable
// ---------------------------------------------------------------------------

/// A type that can be sliced by atom indices.
///
/// # Contract
///
/// `slice(from, to)` requires `from < to` and `to <= self.atom_len()`.
/// The returned value must satisfy `result.atom_len() == to - from`.
///
/// NOTE: [Sliceable] implementation should be coherent with [Mergable]:
/// - For all k, `a.slice(0,k).merge(a.slice(k, a.len())) == a`
pub trait Sliceable {
  /// Returns a new instance containing only atoms `[from, to)`.
  fn slice(&self, from: usize, to: usize) -> Self;
}

// ---------------------------------------------------------------------------
// HasLength
// ---------------------------------------------------------------------------

/// A type that has a measurable length in "atoms" (individual operations).
///
/// In RLE terms an element may represent a *run* of many atomic operations.
/// `content_len` is the semantic length (e.g. 3 inserted characters) while
/// `atom_len` is the number of indivisible operations (usually the same, but
/// can differ for compressed representations).
pub trait HasLength {
  /// Semantic length of the content.
  fn content_len(&self) -> usize;

  /// Number of atomic operations represented by this element.
  ///
  /// Defaults to `content_len`; override when the two differ.
  fn atom_len(&self) -> usize {
    self.content_len()
  }
}

// ---------------------------------------------------------------------------
// GlobalIndex & HasIndex
// ---------------------------------------------------------------------------

/// Integer type used for counter-based indexing inside `HasIndex`.
pub trait GlobalIndex:
  std::fmt::Debug + Integer + Copy + Default + FromPrimitive + AsPrimitive<usize>
{
}

impl<T: std::fmt::Debug + Integer + Copy + Default + FromPrimitive + AsPrimitive<usize>> GlobalIndex
  for T
{
}

/// A type that has a starting index, used to locate an element inside a
/// sequence (e.g. the starting `Counter` of a `Change`).
pub trait HasIndex: HasLength {
  /// The integer type used for the index (usually `Counter` / `i32`).
  type Int: GlobalIndex;

  /// Returns the start index.
  fn get_start_index(&self) -> Self::Int;

  /// Returns the end index (start + atom_len).
  #[inline]
  fn get_end_index(&self) -> Self::Int {
    self.get_start_index() + Self::Int::from_usize(self.atom_len()).unwrap()
  }
}

// ---------------------------------------------------------------------------
// Slice
// ---------------------------------------------------------------------------

/// A borrowed view of a slice of an RLE element.
#[derive(Debug, Clone, Copy)]
pub struct Slice<'a, T> {
  pub value: &'a T,
  pub start: usize,
  pub end: usize,
}

impl<T: Sliceable> Slice<'_, T> {
  /// Materializes this borrowed slice into an owned value.
  pub fn into_inner(&self) -> T {
    self.value.slice(self.start, self.end)
  }
}

// ---------------------------------------------------------------------------
// SearchResult
// ---------------------------------------------------------------------------

/// Result of a binary search inside an `RleVec` by atom index.
#[derive(Clone)]
pub struct SearchResult<'a, T, I: Integer> {
  pub element: &'a T,
  pub merged_index: usize,
  pub offset: I,
}

// ---------------------------------------------------------------------------
// SliceIterator
// ---------------------------------------------------------------------------

/// Iterator over the runs intersecting a given atom-index range.
pub struct SliceIterator<'a, T> {
  vec: &'a [T],
  cur_index: usize,
  cur_offset: usize,
  end_index: Option<usize>,
  end_offset: Option<usize>,
}

impl<T> SliceIterator<'_, T> {
  pub(crate) fn new_empty() -> Self {
    Self {
      vec: &[],
      cur_index: 0,
      cur_offset: 0,
      end_index: None,
      end_offset: None,
    }
  }
}

impl<'a, T: HasLength> Iterator for SliceIterator<'a, T> {
  type Item = Slice<'a, T>;

  fn next(&mut self) -> Option<Self::Item> {
    if self.vec.is_empty() {
      return None;
    }

    let end_index = self.end_index.unwrap_or(self.vec.len() - 1);
    if self.cur_index == end_index {
      let elem = &self.vec[self.cur_index];
      let end = self.end_offset.unwrap_or_else(|| elem.atom_len());
      if self.cur_offset == end {
        return None;
      }

      let ans = Slice {
        value: elem,
        start: self.cur_offset,
        end,
      };
      self.cur_offset = end;
      return Some(ans);
    }

    let ans = Slice {
      value: &self.vec[self.cur_index],
      start: self.cur_offset,
      end: self.vec[self.cur_index].atom_len(),
    };

    self.cur_index += 1;
    self.cur_offset = 0;
    Some(ans)
  }
}

// ---------------------------------------------------------------------------
// RlePush
// ---------------------------------------------------------------------------

/// Push an element that may be merged with the last one.
pub trait RlePush<T> {
  fn push_rle_element(&mut self, element: T);
}

impl<T: Mergable> RlePush<T> for Vec<T> {
  fn push_rle_element(&mut self, element: T) {
    match self.last_mut() {
      Some(last) if last.is_mergable(&element, &()) => {
        last.merge(&element, &());
      }
      _ => {
        self.push(element);
      }
    }
  }
}

impl<A: Array> RlePush<A::Item> for smallvec::SmallVec<A>
where
  A::Item: Mergable,
{
  fn push_rle_element(&mut self, element: A::Item) {
    match self.last_mut() {
      Some(last) if last.is_mergable(&element, &()) => {
        last.merge(&element, &());
      }
      _ => {
        self.push(element);
      }
    }
  }
}

// ---------------------------------------------------------------------------
// RleCollection
// ---------------------------------------------------------------------------

/// Collection operations for types whose elements implement `HasIndex`.
pub trait RleCollection<T: HasIndex> {
  fn start(&self) -> T::Int;
  fn end(&self) -> T::Int;
  fn sum_atom_len(&self) -> T::Int;
  fn search_atom_index(&self, index: T::Int) -> usize;
  fn get_by_atom_index(&self, index: T::Int) -> Option<SearchResult<'_, T, T::Int>>;
}
