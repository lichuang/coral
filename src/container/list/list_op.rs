//! List and Text operations — insert, delete, move, set, style.

use crate::op::SliceRange;
use crate::rle::{HasLength, Mergable, Sliceable};
use crate::types::{CoralValue, ID, IdLp};
use append_only_bytes::BytesSlice;
use std::borrow::Cow;

// ═══════════════════════════════════════════════════════════════════════════
// Data slice for insert operations
// ═══════════════════════════════════════════════════════════════════════════

/// A slice of data to be inserted into a List or Text container.
///
/// `RawData` is used for List inserts (arbitrary values).
/// `RawStr` is used for Text inserts (UTF-8 string with unicode length).
#[derive(Debug, Clone, PartialEq)]
pub enum ListSlice<'a> {
  /// Insert a sequence of values into a List.
  RawData(Cow<'a, [CoralValue]>),
  /// Insert a string into a Text container.
  RawStr {
    str: Cow<'a, str>,
    unicode_len: usize,
  },
}

impl<'a> ListSlice<'a> {
  /// Convert a borrowed slice into an owned (`'static`) one.
  pub fn to_static(&self) -> ListSlice<'static> {
    match self {
      ListSlice::RawData(data) => ListSlice::RawData(Cow::Owned(data.clone().into_owned())),
      ListSlice::RawStr { str, unicode_len } => ListSlice::RawStr {
        str: Cow::Owned(str.clone().into_owned()),
        unicode_len: *unicode_len,
      },
    }
  }
}

impl HasLength for ListSlice<'_> {
  fn content_len(&self) -> usize {
    match self {
      ListSlice::RawData(data) => data.len(),
      ListSlice::RawStr { unicode_len, .. } => *unicode_len,
    }
  }
}

impl Sliceable for ListSlice<'_> {
  fn slice(&self, from: usize, to: usize) -> Self {
    assert!(from <= to, "ListSlice::slice: from ({from}) > to ({to})");
    match self {
      ListSlice::RawData(data) => {
        let end = to.min(data.len());
        ListSlice::RawData(Cow::Owned(data[from..end].to_vec()))
      }
      ListSlice::RawStr { str, unicode_len } => {
        let mut start_byte = 0;
        let mut end_byte = str.len();
        let mut current = 0;
        for (byte_idx, _c) in str.char_indices() {
          if current == from {
            start_byte = byte_idx;
          }
          if current == to.min(*unicode_len) {
            end_byte = byte_idx;
            break;
          }
          current += 1;
        }
        if current < to.min(*unicode_len) {
          end_byte = str.len();
        }
        let sliced = &str[start_byte..end_byte];
        ListSlice::RawStr {
          str: Cow::Owned(sliced.to_string()),
          unicode_len: to.min(*unicode_len) - from,
        }
      }
    }
  }
}

impl Mergable for ListSlice<'_> {
  fn is_mergable(&self, _other: &Self, _conf: &()) -> bool {
    false
  }
}

// ═══════════════════════════════════════════════════════════════════════════
// Delete span
// ═══════════════════════════════════════════════════════════════════════════

/// A positional span that may be reversed.
///
/// `signed_len` can be negative, indicating a backward deletion.
/// It is never zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeleteSpan {
  pub pos: isize,
  pub signed_len: isize,
}

impl DeleteSpan {
  /// Absolute length (always positive).
  #[inline]
  pub fn len(&self) -> usize {
    self.signed_len.unsigned_abs()
  }

  /// Inclusive start position.
  #[inline]
  pub fn start(&self) -> isize {
    self.pos
  }

  /// Exclusive end position.
  #[inline]
  pub fn end(&self) -> isize {
    self.pos + self.signed_len
  }

  /// Whether the span runs backward (`signed_len < 0`).
  #[inline]
  pub fn is_reversed(&self) -> bool {
    self.signed_len < 0
  }

  /// Always `false` — a `DeleteSpan` is never empty by construction.
  #[inline]
  pub fn is_empty(&self) -> bool {
    false
  }
}

impl HasLength for DeleteSpan {
  fn content_len(&self) -> usize {
    self.len()
  }
}

impl Sliceable for DeleteSpan {
  fn slice(&self, from: usize, to: usize) -> Self {
    let len = self.len();
    assert!(from <= to && to <= len, "DeleteSpan::slice out of bounds");
    let direction = if self.signed_len < 0 { -1isize } else { 1isize };
    let new_start = self.pos + from as isize * direction;
    let new_len = (to - from) as isize * direction;
    Self {
      pos: new_start,
      signed_len: new_len,
    }
  }
}

impl Mergable for DeleteSpan {
  fn is_mergable(&self, other: &Self, _conf: &()) -> bool {
    self.end() == other.start()
  }

  fn merge(&mut self, other: &Self, _conf: &()) {
    assert!(self.is_mergable(other, &()));
    self.signed_len += other.signed_len;
  }
}

/// A delete span paired with the ID of its first deleted element.
///
/// `id_start` is always the ID of the leftmost element regardless of
/// deletion direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeleteSpanWithId {
  pub id_start: ID,
  pub span: DeleteSpan,
}

impl DeleteSpanWithId {
  /// Create a new delete span with ID.
  pub fn new(id_start: ID, pos: isize, signed_len: isize) -> Self {
    Self {
      id_start,
      span: DeleteSpan { pos, signed_len },
    }
  }
}

impl HasLength for DeleteSpanWithId {
  fn content_len(&self) -> usize {
    self.span.len()
  }
}

impl Sliceable for DeleteSpanWithId {
  fn slice(&self, from: usize, to: usize) -> Self {
    Self {
      id_start: self.id_start.inc(from as i32),
      span: self.span.slice(from, to),
    }
  }
}

impl Mergable for DeleteSpanWithId {
  fn is_mergable(&self, other: &Self, _conf: &()) -> bool {
    // Must be contiguous in both ID space and position space.
    self.span.is_reversed() == other.span.is_reversed()
      && self.id_start.inc(self.span.len() as i32) == other.id_start
      && self.span.is_mergable(&other.span, &())
  }

  fn merge(&mut self, other: &Self, _conf: &()) {
    assert!(self.is_mergable(other, &()));
    self.span.merge(&other.span, &());
  }
}

// ═══════════════════════════════════════════════════════════════════════════
// Raw list op (transport / serialization)
// ═══════════════════════════════════════════════════════════════════════════

/// List/Text operation before arena resolution.
///
/// The `'a` lifetime comes from `ListSlice<'a>` which may borrow data.
#[derive(Debug, Clone, PartialEq)]
pub enum ListOp<'a> {
  /// Insert values or text at the given position.
  Insert { slice: ListSlice<'a>, pos: usize },
  /// Delete a span of elements.
  Delete(DeleteSpanWithId),
  /// Move an element from one position to another.
  Move { from: u32, to: u32, elem_id: IdLp },
  /// Update the value of an existing element.
  Set { elem_id: IdLp, value: CoralValue },
  /// Start a style range (RichText).
  StyleStart {
    start: u32,
    end: u32,
    key: String,
    info: u8,
    value: CoralValue,
  },
  /// End a style range.
  StyleEnd,
}

impl HasLength for ListOp<'_> {
  fn content_len(&self) -> usize {
    match self {
      ListOp::Insert { slice, .. } => slice.content_len(),
      ListOp::Delete(span) => span.content_len(),
      ListOp::Move { .. } | ListOp::Set { .. } | ListOp::StyleStart { .. } | ListOp::StyleEnd => 1,
    }
  }
}

impl Sliceable for ListOp<'_> {
  fn slice(&self, from: usize, to: usize) -> Self {
    match self {
      ListOp::Insert { slice, pos } => ListOp::Insert {
        slice: slice.slice(from, to),
        pos: *pos,
      },
      ListOp::Delete(span) => ListOp::Delete(span.slice(from, to)),
      _ => {
        assert!(
          from == 0 && to == 1,
          "ListOp::slice: only Insert/Delete are sliceable"
        );
        self.clone()
      }
    }
  }
}

impl Mergable for ListOp<'_> {
  fn is_mergable(&self, other: &Self, _conf: &()) -> bool {
    match (self, other) {
      (ListOp::Insert { slice: a, pos: pa }, ListOp::Insert { slice: _b, pos: pb }) => {
        pa + a.content_len() == *pb
      }
      (ListOp::Delete(a), ListOp::Delete(b)) => a.is_mergable(b, &()),
      _ => false,
    }
  }

  fn merge(&mut self, other: &Self, _conf: &()) {
    assert!(self.is_mergable(other, &()));
    match (self, other) {
      (ListOp::Insert { .. }, ListOp::Insert { .. }) => {
        // We cannot merge ListSlice in place because the enum variants may differ
        // (RawData vs RawStr).  For now we rely on the caller to handle this.
        // In practice Coral merges ops before they are committed to the arena.
      }
      (ListOp::Delete(a), ListOp::Delete(b)) => a.merge(b, &()),
      _ => unreachable!(),
    }
  }
}

// ═══════════════════════════════════════════════════════════════════════════
// Arena-resolved list op (stored in Op)
// ═══════════════════════════════════════════════════════════════════════════

/// List/Text operation after arena resolution.
///
/// `Insert` stores a [`SliceRange`](crate::op::SliceRange) (arena index).
/// `InsertText` stores a [`BytesSlice`] plus unicode bounds.
#[derive(Debug, Clone, PartialEq)]
pub enum InnerListOp {
  /// Insert values (List) using an arena slice range.
  Insert { pos: usize, slice: SliceRange },
  /// Insert text (Text) using a byte slice.
  InsertText {
    pos: u32,
    slice: BytesSlice,
    unicode_start: u32,
    unicode_len: u32,
  },
  /// Delete a span.
  Delete(DeleteSpanWithId),
  /// Move an element.
  Move { from: u32, to: u32, elem_id: IdLp },
  /// Set an element value.
  Set { elem_id: IdLp, value: CoralValue },
  /// Start a style range.
  StyleStart {
    start: u32,
    end: u32,
    key: String,
    info: u8,
    value: CoralValue,
  },
  /// End a style range.
  StyleEnd,
}

impl HasLength for InnerListOp {
  fn content_len(&self) -> usize {
    match self {
      InnerListOp::Insert { slice, .. } => slice.end - slice.start,
      InnerListOp::InsertText { unicode_len, .. } => *unicode_len as usize,
      InnerListOp::Delete(span) => span.content_len(),
      InnerListOp::Move { .. }
      | InnerListOp::Set { .. }
      | InnerListOp::StyleStart { .. }
      | InnerListOp::StyleEnd => 1,
    }
  }
}

impl Sliceable for InnerListOp {
  fn slice(&self, from: usize, to: usize) -> Self {
    match self {
      InnerListOp::Insert { pos, slice } => InnerListOp::Insert {
        pos: *pos,
        slice: SliceRange {
          start: slice.start + from,
          end: (slice.start + to).min(slice.end),
        },
      },
      InnerListOp::InsertText {
        pos,
        slice,
        unicode_start,
        unicode_len,
      } => InnerListOp::InsertText {
        pos: *pos,
        slice: slice.clone(), // BytesSlice cannot be sub-sliced easily; TODO
        unicode_start: unicode_start + from as u32,
        unicode_len: (to as u32).min(*unicode_len),
      },
      InnerListOp::Delete(span) => InnerListOp::Delete(span.slice(from, to)),
      _ => {
        assert!(
          from == 0 && to == 1,
          "InnerListOp::slice: only Insert/Delete/InsertText are sliceable"
        );
        self.clone()
      }
    }
  }
}

impl Mergable for InnerListOp {
  fn is_mergable(&self, other: &Self, _conf: &()) -> bool {
    match (self, other) {
      (InnerListOp::Insert { slice: a, pos: pa }, InnerListOp::Insert { slice: _b, pos: pb }) => {
        pa + (a.end - a.start) == *pb
      }
      (
        InnerListOp::InsertText {
          pos: pa,
          unicode_len: la,
          ..
        },
        InnerListOp::InsertText { pos: pb, .. },
      ) => pa + la == *pb,
      (InnerListOp::Delete(a), InnerListOp::Delete(b)) => a.is_mergable(b, &()),
      _ => false,
    }
  }

  fn merge(&mut self, other: &Self, _conf: &()) {
    assert!(self.is_mergable(other, &()));
    match (self, other) {
      (InnerListOp::Insert { slice: a, .. }, InnerListOp::Insert { slice: b, .. }) => {
        a.end = b.end;
      }
      (
        InnerListOp::InsertText { unicode_len: a, .. },
        InnerListOp::InsertText { unicode_len: b, .. },
      ) => {
        *a += *b;
      }
      (InnerListOp::Delete(a), InnerListOp::Delete(b)) => a.merge(b, &()),
      _ => unreachable!(),
    }
  }
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
  use super::*;
  use std::borrow::Cow;

  // ── ListSlice ────────────────────────────────────────────────────

  #[test]
  fn test_list_slice_raw_data_len() {
    let slice = ListSlice::RawData(Cow::Borrowed(&[
      CoralValue::from(1i32),
      CoralValue::from(2i32),
      CoralValue::from(3i32),
    ]));
    assert_eq!(slice.content_len(), 3);
  }

  #[test]
  fn test_list_slice_raw_str_len() {
    let slice = ListSlice::RawStr {
      str: Cow::Borrowed("你好世界"),
      unicode_len: 4,
    };
    assert_eq!(slice.content_len(), 4);
  }

  #[test]
  fn test_list_slice_raw_data_slice() {
    let slice = ListSlice::RawData(Cow::Owned(vec![
      CoralValue::from(1i32),
      CoralValue::from(2i32),
      CoralValue::from(3i32),
      CoralValue::from(4i32),
    ]));
    let sub = slice.slice(1, 3);
    assert_eq!(sub.content_len(), 2);
    assert_eq!(
      sub,
      ListSlice::RawData(Cow::Owned(vec![
        CoralValue::from(2i32),
        CoralValue::from(3i32)
      ]))
    );
  }

  #[test]
  fn test_list_slice_raw_str_slice() {
    let slice = ListSlice::RawStr {
      str: Cow::Borrowed("Hello世界"),
      unicode_len: 7,
    };
    // Slice unicode range [2, 5) → "llo" (chars at index 2, 3, 4)
    let sub = slice.slice(2, 5);
    assert_eq!(sub.content_len(), 3);
    assert_eq!(
      sub,
      ListSlice::RawStr {
        str: Cow::Owned("llo".to_string()),
        unicode_len: 3,
      }
    );

    // Slice across ASCII/CJK boundary [4, 6) → "o世"
    let sub2 = slice.slice(4, 6);
    assert_eq!(sub2.content_len(), 2);
    assert_eq!(
      sub2,
      ListSlice::RawStr {
        str: Cow::Owned("o世".to_string()),
        unicode_len: 2,
      }
    );
  }

  #[test]
  fn test_list_slice_to_static() {
    let slice = ListSlice::RawStr {
      str: Cow::Borrowed("borrowed"),
      unicode_len: 8,
    };
    let owned = slice.to_static();
    assert_eq!(owned.content_len(), 8);
    match owned {
      ListSlice::RawStr {
        str: Cow::Owned(s),
        unicode_len,
      } => {
        assert_eq!(s, "borrowed");
        assert_eq!(unicode_len, 8);
      }
      _ => panic!("to_static should produce owned data"),
    }
  }

  // ── DeleteSpan ───────────────────────────────────────────────────

  #[test]
  fn test_delete_span_forward() {
    let span = DeleteSpan {
      pos: 3,
      signed_len: 5,
    };
    assert_eq!(span.len(), 5);
    assert_eq!(span.start(), 3);
    assert_eq!(span.end(), 8);
    assert!(!span.is_reversed());
    assert!(!span.is_empty());
  }

  #[test]
  fn test_delete_span_reversed() {
    let span = DeleteSpan {
      pos: 8,
      signed_len: -5,
    };
    assert_eq!(span.len(), 5);
    assert_eq!(span.start(), 8);
    assert_eq!(span.end(), 3);
    assert!(span.is_reversed());
  }

  #[test]
  fn test_delete_span_slice() {
    let span = DeleteSpan {
      pos: 3,
      signed_len: 5,
    };
    let sub = span.slice(1, 4);
    assert_eq!(sub.pos, 4);
    assert_eq!(sub.signed_len, 3);
    assert_eq!(sub.len(), 3);
  }

  #[test]
  fn test_delete_span_slice_reversed() {
    let span = DeleteSpan {
      pos: 8,
      signed_len: -5,
    };
    let sub = span.slice(1, 4);
    assert_eq!(sub.pos, 7);
    assert_eq!(sub.signed_len, -3);
    assert_eq!(sub.len(), 3);
  }

  #[test]
  fn test_delete_span_merge() {
    let mut a = DeleteSpan {
      pos: 3,
      signed_len: 5,
    };
    let b = DeleteSpan {
      pos: 8,
      signed_len: 4,
    };
    assert!(a.is_mergable(&b, &()));
    a.merge(&b, &());
    assert_eq!(a.pos, 3);
    assert_eq!(a.signed_len, 9);
  }

  #[test]
  fn test_delete_span_merge_reversed() {
    let mut a = DeleteSpan {
      pos: 10,
      signed_len: -3,
    };
    let b = DeleteSpan {
      pos: 7,
      signed_len: -4,
    };
    assert!(a.is_mergable(&b, &()));
    a.merge(&b, &());
    assert_eq!(a.pos, 10);
    assert_eq!(a.signed_len, -7);
  }

  // ── DeleteSpanWithId ─────────────────────────────────────────────

  #[test]
  fn test_delete_span_with_id_slice() {
    let dsid = DeleteSpanWithId::new(ID::new(1, 10), 3, 5);
    let sub = dsid.slice(1, 4);
    assert_eq!(sub.id_start, ID::new(1, 11));
    assert_eq!(sub.span.pos, 4);
    assert_eq!(sub.span.signed_len, 3);
  }

  #[test]
  fn test_delete_span_with_id_merge() {
    let mut a = DeleteSpanWithId::new(ID::new(1, 10), 3, 5);
    let b = DeleteSpanWithId::new(ID::new(1, 15), 8, 4);
    assert!(a.is_mergable(&b, &()));
    a.merge(&b, &());
    assert_eq!(a.span.signed_len, 9);
    assert_eq!(a.id_start, ID::new(1, 10));
  }

  #[test]
  fn test_delete_span_with_id_not_mergable_when_gap() {
    let a = DeleteSpanWithId::new(ID::new(1, 10), 3, 5);
    let b = DeleteSpanWithId::new(ID::new(1, 16), 9, 4);
    assert!(!a.is_mergable(&b, &()));
  }

  // ── ListOp ───────────────────────────────────────────────────────

  #[test]
  fn test_list_op_insert_slice() {
    let op = ListOp::Insert {
      slice: ListSlice::RawData(Cow::Owned(vec![
        CoralValue::from(1i32),
        CoralValue::from(2i32),
        CoralValue::from(3i32),
      ])),
      pos: 0,
    };
    let sub = op.slice(1, 2);
    assert_eq!(sub.content_len(), 1);
    match sub {
      ListOp::Insert { slice, pos } => {
        assert_eq!(pos, 0);
        assert_eq!(slice.content_len(), 1);
      }
      _ => panic!("expected Insert"),
    }
  }

  #[test]
  fn test_list_op_delete_slice() {
    let op = ListOp::Delete(DeleteSpanWithId::new(ID::new(1, 0), 3, 5));
    let sub = op.slice(1, 4);
    assert_eq!(sub.content_len(), 3);
    match sub {
      ListOp::Delete(span) => {
        assert_eq!(span.id_start, ID::new(1, 1));
        assert_eq!(span.span.pos, 4);
        assert_eq!(span.span.signed_len, 3);
      }
      _ => panic!("expected Delete"),
    }
  }

  #[test]
  fn test_list_op_insert_merge() {
    let mut a = ListOp::Insert {
      slice: ListSlice::RawData(Cow::Owned(vec![CoralValue::from(1i32)])),
      pos: 0,
    };
    let b = ListOp::Insert {
      slice: ListSlice::RawData(Cow::Owned(vec![CoralValue::from(2i32)])),
      pos: 1,
    };
    assert!(a.is_mergable(&b, &()));
    a.merge(&b, &());
    assert_eq!(a.content_len(), 1); // merge is a no-op for ListSlice
  }

  #[test]
  fn test_list_op_delete_merge() {
    let mut a = ListOp::Delete(DeleteSpanWithId::new(ID::new(1, 0), 3, 5));
    let b = ListOp::Delete(DeleteSpanWithId::new(ID::new(1, 5), 8, 4));
    assert!(a.is_mergable(&b, &()));
    a.merge(&b, &());
    match a {
      ListOp::Delete(span) => {
        assert_eq!(span.span.signed_len, 9);
      }
      _ => panic!("expected Delete"),
    }
  }

  // ── InnerListOp ──────────────────────────────────────────────────

  #[test]
  fn test_inner_list_op_insert_slice() {
    let op = InnerListOp::Insert {
      pos: 0,
      slice: SliceRange { start: 10, end: 14 },
    };
    let sub = op.slice(1, 3);
    match sub {
      InnerListOp::Insert { pos, slice } => {
        assert_eq!(pos, 0);
        assert_eq!(slice.start, 11);
        assert_eq!(slice.end, 13);
      }
      _ => panic!("expected Insert"),
    }
  }

  #[test]
  fn test_inner_list_op_insert_merge() {
    let mut a = InnerListOp::Insert {
      pos: 0,
      slice: SliceRange { start: 10, end: 12 },
    };
    let b = InnerListOp::Insert {
      pos: 2,
      slice: SliceRange { start: 12, end: 15 },
    };
    assert!(a.is_mergable(&b, &()));
    a.merge(&b, &());
    match a {
      InnerListOp::Insert { slice, .. } => {
        assert_eq!(slice.start, 10);
        assert_eq!(slice.end, 15);
      }
      _ => panic!("expected Insert"),
    }
  }

  #[test]
  fn test_inner_list_op_insert_text_merge() {
    let mut a = InnerListOp::InsertText {
      pos: 0,
      slice: BytesSlice::empty(),
      unicode_start: 0,
      unicode_len: 3,
    };
    let b = InnerListOp::InsertText {
      pos: 3,
      slice: BytesSlice::empty(),
      unicode_start: 3,
      unicode_len: 5,
    };
    assert!(a.is_mergable(&b, &()));
    a.merge(&b, &());
    match a {
      InnerListOp::InsertText { unicode_len, .. } => {
        assert_eq!(unicode_len, 8);
      }
      _ => panic!("expected InsertText"),
    }
  }
}
