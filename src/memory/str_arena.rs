//! String arena — append-only text storage with unicode indexing.
//!
//! [`StrArena`] stores all inserted text in a single contiguous
//! [`AppendOnlyBytes`] buffer.  A sparse unicode→byte index is built
//! every 128 bytes so that slicing by unicode range is O(log n) instead
//! of O(n).
//!
//! This implementation is aligned with Loro's `StrArena`.

#![allow(dead_code)]

use append_only_bytes::{AppendOnlyBytes, BytesSlice};
use std::ops::{Bound, RangeBounds};

/// Sampling interval for the unicode index, in **bytes**.
///
/// When the total byte length grows by more than this amount since the last
/// sampled point, a new [`Index`] entry is appended.  A smaller value trades
/// memory for faster `slice_by_unicode` (less linear scanning), while a larger
/// value is more memory-efficient.  128 bytes is the same value used by Loro.
const INDEX_INTERVAL: u32 = 128;

/// Append-only string storage with O(log n) unicode slicing.
///
/// All strings are concatenated into a single [`AppendOnlyBytes`] buffer.
/// To support slicing by unicode code-point position (rather than byte offset),
/// `StrArena` maintains a sparse vector of [`Index`] samples.  The sampling
/// strategy is described on [`INDEX_INTERVAL`].
#[derive(Default, Debug, Clone)]
pub(crate) struct StrArena {
  /// Raw byte buffer.  Never shrinks; strings are only ever appended.
  bytes: AppendOnlyBytes,
  /// Sparse index: one entry every ~[`INDEX_INTERVAL`] bytes.
  ///
  /// The first entry is always `(0, 0, 0)`.  Each subsequent entry records the
  /// cumulative byte offset, UTF-16 code-unit count and unicode scalar count
  /// at the point it was sampled.
  unicode_indexes: Vec<Index>,
  /// Cumulative length of everything appended so far.
  len: Index,
}

/// A single sampled point in the sparse unicode index.
///
/// All fields are **cumulative** — they describe the state of the arena at the
/// exact byte position where this sample was taken.
#[derive(Debug, Default, Clone, Copy)]
struct Index {
  /// Total bytes stored up to and including this sample.
  bytes: u32,
  /// Total UTF-16 code units up to and including this sample.
  utf16: u32,
  /// Total unicode scalar values up to and including this sample.
  unicode: u32,
}

impl StrArena {
  /// Returns `true` if no text has been allocated yet.
  #[inline]
  pub fn is_empty(&self) -> bool {
    self.len.bytes == 0
  }

  /// Total number of bytes stored in the arena.
  #[inline]
  pub fn len_bytes(&self) -> usize {
    self.len.bytes as usize
  }

  /// Total number of UTF-16 code units stored in the arena.
  #[inline]
  pub fn len_utf16(&self) -> usize {
    self.len.utf16 as usize
  }

  /// Total number of unicode scalar values stored in the arena.
  #[inline]
  pub fn len_unicode(&self) -> usize {
    self.len.unicode as usize
  }

  /// Append a string to the arena, updating the unicode index as needed.
  ///
  /// The input is internally chunked into segments of roughly
  /// [`INDEX_INTERVAL`] bytes.  Each chunk is passed to [`StrArena::_alloc`],
  /// which appends the bytes and may insert a new [`Index`] sample if the
  /// byte distance since the last sample exceeds the interval.
  ///
  /// # Note
  ///
  /// Even if `input` is shorter than [`INDEX_INTERVAL`], it is still appended
  /// as a single chunk.  The sampling is **best-effort** — it guarantees that
  /// consecutive samples are at least `INDEX_INTERVAL` bytes apart, but does
  /// not force a sample on every call.
  pub fn alloc(&mut self, input: &str) {
    let mut utf16 = 0;
    let mut unicode_len = 0;
    let mut last_save_index = 0;
    // Walk the input char-by-char, accumulating counts.  Whenever the
    // accumulated byte length crosses INDEX_INTERVAL, flush the chunk.
    for (byte_index, c) in input.char_indices() {
      let byte_index = byte_index + c.len_utf8();
      utf16 += c.len_utf16() as u32;
      unicode_len += 1;
      if byte_index - last_save_index > INDEX_INTERVAL as usize {
        self._alloc(&input[last_save_index..byte_index], utf16, unicode_len);
        last_save_index = byte_index;
        utf16 = 0;
        unicode_len = 0;
      }
    }

    // Flush any trailing bytes that did not cross the interval threshold.
    if last_save_index != input.len() {
      self._alloc(&input[last_save_index..], utf16, unicode_len);
    }
  }

  /// Low-level append of a single chunk.
  ///
  /// Updates cumulative lengths, pushes bytes into the backing buffer, and
  /// records a new [`Index`] sample when the byte distance since the last
  /// sample exceeds [`INDEX_INTERVAL`].
  fn _alloc(&mut self, input: &str, utf16: u32, unicode_len: i32) {
    self.len.bytes += input.len() as u32;
    self.len.utf16 += utf16;
    self.len.unicode += unicode_len as u32;
    self.bytes.push_str(input);
    let cur_len = self.len;

    // Ensure the index always has an origin point at (0, 0, 0).
    if self.unicode_indexes.is_empty() {
      self.unicode_indexes.push(Index {
        bytes: 0,
        utf16: 0,
        unicode: 0,
      });
    }

    // Insert a new sample only when the byte delta is large enough.
    let last = self.unicode_indexes.last().unwrap();
    if cur_len.bytes - last.bytes > INDEX_INTERVAL {
      self.unicode_indexes.push(cur_len);
    }
  }

  /// Returns a [`BytesSlice`] covering the given unicode code-point range.
  ///
  /// The range is interpreted in **unicode scalar values**, not bytes or
  /// UTF-16 code units.  For example, `slice_by_unicode(0..2)` on `"你好"`
  /// returns the first two CJK characters (6 bytes total).
  ///
  /// # Panics
  ///
  /// Panics if the range extends beyond the total unicode length.
  #[inline]
  pub fn slice_by_unicode(&mut self, range: impl RangeBounds<usize>) -> BytesSlice {
    let (start, end) = self.unicode_range_to_utf8_range(range);
    self.bytes.slice(start..end)
  }

  /// Returns a `&str` covering the given unicode code-point range.
  ///
  /// See [`StrArena::slice_by_unicode`] for semantics.
  ///
  /// # Safety
  ///
  /// The byte offsets produced by [`unicode_range_to_utf8_range`] always fall
  /// on UTF-8 character boundaries, so the unchecked conversion is sound.
  #[inline]
  pub fn slice_str_by_unicode(&mut self, range: impl RangeBounds<usize>) -> &str {
    let (start, end) = self.unicode_range_to_utf8_range(range);
    // SAFETY: we know that the range is valid UTF-8 boundary
    unsafe { std::str::from_utf8_unchecked(&self.bytes[start..end]) }
  }

  /// Returns a [`BytesSlice`] covering the given **byte** range.
  ///
  /// This is a thin wrapper around [`AppendOnlyBytes::slice`] and does not
  /// involve unicode indexing.
  #[inline]
  pub fn slice_bytes(&self, range: impl RangeBounds<usize>) -> BytesSlice {
    self.bytes.slice(range)
  }

  /// Convert a unicode-code-point range into a UTF-8 byte range.
  ///
  /// The conversion is two-phase:
  /// 1. Use [`unicode_to_byte_index`] to map the start and end unicode
  ///    positions to byte offsets.
  /// 2. Return the resulting `(start_byte, end_byte)` pair.
  fn unicode_range_to_utf8_range(&mut self, range: impl RangeBounds<usize>) -> (usize, usize) {
    if self.is_empty() {
      return (0, 0);
    }

    let start = match range.start_bound() {
      Bound::Included(&i) => unicode_to_byte_index(&self.unicode_indexes, i as u32, &self.bytes),
      Bound::Excluded(&_i) => unreachable!(),
      Bound::Unbounded => 0,
    };

    let end = match range.end_bound() {
      Bound::Included(&i) => {
        unicode_to_byte_index(&self.unicode_indexes, i as u32 + 1, &self.bytes)
      }
      Bound::Excluded(&i) => unicode_to_byte_index(&self.unicode_indexes, i as u32, &self.bytes),
      Bound::Unbounded => self.len.bytes as usize,
    };

    (start, end)
  }
}

/// Map a single unicode position to its corresponding byte offset.
///
/// Algorithm:
/// 1. Binary-search `index` for the largest sample whose `unicode` count is
///    ≤ `unicode_index`.  If `unicode_index` hits a sample exactly, return
///    the pre-recorded byte offset immediately.
/// 2. Otherwise, start from the nearest preceding sample and linearly scan
///    forward through the raw bytes, counting unicode scalars until the
///    desired position is reached.
///
/// The binary search makes the lookup O(log n) in the number of samples;
/// the linear scan is bounded by [`INDEX_INTERVAL`] bytes, so it is effectively
/// O(1) amortised.
fn unicode_to_byte_index(index: &[Index], unicode_index: u32, bytes: &AppendOnlyBytes) -> usize {
  let i = match index.binary_search_by_key(&unicode_index, |x| x.unicode) {
    Ok(i) => i,
    Err(i) => i.saturating_sub(1),
  };

  let idx = index[i];
  if idx.unicode == unicode_index {
    return idx.bytes as usize;
  }

  // SAFETY: we know that the index must be valid, because we record and calculate the valid index
  let s = unsafe { std::str::from_utf8_unchecked(&bytes[idx.bytes as usize..]) };
  unicode_to_utf8_index(s, (unicode_index - idx.unicode) as usize).unwrap() + idx.bytes as usize
}

/// Convert a unicode position to a UTF-8 byte offset within `s`.
///
/// Performs a linear scan over `s.chars()`.  Returns `Some(byte_offset)` when
/// `unicode_index` is within `[0, char_count]`; returns `None` if it is past
/// the end of the string.
fn unicode_to_utf8_index(s: &str, unicode_index: usize) -> Option<usize> {
  let mut current = 0;
  for (byte_idx, _c) in s.char_indices() {
    if current == unicode_index {
      return Some(byte_idx);
    }
    current += 1;
  }
  if current == unicode_index {
    Some(s.len())
  } else {
    None
  }
}

#[cfg(test)]
mod tests {
  use std::ops::Deref;

  use super::*;

  /// Basic append and retrieval of ASCII text.
  #[test]
  fn test_alloc_and_slice() {
    let mut arena = StrArena::default();
    arena.alloc("Hello");
    let slice = arena.slice_by_unicode(0..5);
    assert_eq!(slice.deref(), b"Hello");
    arena.alloc("World");
    let slice = arena.slice_by_unicode(5..10);
    assert_eq!(slice.deref(), b"World");
  }

  /// Unicode indexing across mixed ASCII and CJK text.
  ///
  /// Verifies that `slice_by_unicode` correctly accounts for multi-byte
  /// characters and that consecutive allocations are treated as one flat
  /// sequence.
  #[test]
  fn test_unicode_indexing() {
    let mut arena = StrArena::default();
    arena.alloc("Hello");
    arena.alloc("World");
    arena.alloc("你好");
    arena.alloc("世界");
    let slice = arena.slice_by_unicode(0..4);
    assert_eq!(slice.deref(), b"Hell");

    let slice = arena.slice_by_unicode(4..8);
    assert_eq!(slice.deref(), b"oWor");

    let slice = arena.slice_by_unicode(8..10);
    assert_eq!(slice.deref(), b"ld");

    let slice = arena.slice_by_unicode(10..12);
    assert_eq!(slice.deref(), "你好".as_bytes());

    let slice = arena.slice_by_unicode(12..14);
    assert_eq!(slice.deref(), "世界".as_bytes());
  }

  /// Long-text slicing that forces multiple index samples.
  ///
  /// Repeats a 10-character string (mix of 3-byte CJK and ASCII) 100 times.
  /// The resulting arena is large enough to trigger several
  /// [`INDEX_INTERVAL`]-based samples, exercising the binary-search → linear-scan
  /// fallback path in [`unicode_to_byte_index`].
  #[test]
  fn test_long_unicode_slicing() {
    let mut arena = StrArena::default();
    let src = "一二34567八九零";
    for s in std::iter::repeat_n(src, 100) {
      arena.alloc(s);
    }

    let slice = arena.slice_by_unicode(110..120);
    assert_eq!(slice.deref(), src.as_bytes());
    let slice = arena.slice_by_unicode(111..121);
    assert_eq!(slice.deref(), "二34567八九零一".as_bytes());
  }
}
