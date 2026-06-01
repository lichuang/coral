use crate::types::Value;

/// A reference to a substring stored in [`SharedArena::str_buf`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StrSlice {
  pub start: usize,
  pub len: usize,
}

/// A shared memory arena for storing [`Value`]s and strings.
///
/// Values are stored in a contiguous vector; strings are appended to a
/// single `String` buffer and referenced via [`StrSlice`].
#[derive(Debug, Clone, Default)]
pub struct SharedArena {
  values: Vec<Value>,
  str_buf: String,
}

impl SharedArena {
  /// Creates an empty `SharedArena`.
  pub fn new() -> Self {
    Self::default()
  }

  /// Returns `true` if both the value pool and the string buffer are empty.
  pub fn is_empty(&self) -> bool {
    self.values.is_empty() && self.str_buf.is_empty()
  }

  /// Returns the number of stored values.
  pub fn value_count(&self) -> usize {
    self.values.len()
  }

  /// Returns the total length of the string buffer.
  pub fn str_len(&self) -> usize {
    self.str_buf.len()
  }

  /// Appends a [`Value`] and returns its index.
  pub fn push_value(&mut self, value: Value) -> usize {
    let idx = self.values.len();
    self.values.push(value);
    idx
  }

  /// Returns a reference to the [`Value`] at the given index.
  pub fn get_value(&self, idx: usize) -> Option<&Value> {
    self.values.get(idx)
  }

  /// Appends a string slice and returns a [`StrSlice`] pointing to it.
  pub fn push_str(&mut self, s: &str) -> StrSlice {
    let start = self.str_buf.len();
    self.str_buf.push_str(s);
    StrSlice {
      start,
      len: s.len(),
    }
  }

  /// Returns the substring described by the given [`StrSlice`].
  pub fn get_str(&self, slice: StrSlice) -> &str {
    &self.str_buf[slice.start..slice.start + slice.len]
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_arena_new_is_empty() {
    let arena = SharedArena::new();
    assert!(arena.is_empty());
    assert_eq!(arena.value_count(), 0);
    assert_eq!(arena.str_len(), 0);
  }

  #[test]
  fn test_push_and_get_value() {
    let mut arena = SharedArena::new();

    let idx0 = arena.push_value(Value::I64(42));
    let idx1 = arena.push_value(Value::Bool(true));
    let idx2 = arena.push_value(Value::Double(3.14));

    assert_eq!(idx0, 0);
    assert_eq!(idx1, 1);
    assert_eq!(idx2, 2);
    assert_eq!(arena.value_count(), 3);

    assert_eq!(arena.get_value(idx0), Some(&Value::I64(42)));
    assert_eq!(arena.get_value(idx1), Some(&Value::Bool(true)));
    assert_eq!(arena.get_value(idx2), Some(&Value::Double(3.14)));
    assert_eq!(arena.get_value(99), None);
  }

  #[test]
  fn test_push_and_get_str() {
    let mut arena = SharedArena::new();

    let slice_a = arena.push_str("hello");
    let slice_b = arena.push_str("world");

    assert_eq!(slice_a.start, 0);
    assert_eq!(slice_a.len, 5);
    assert_eq!(slice_b.start, 5);
    assert_eq!(slice_b.len, 5);
    assert_eq!(arena.str_len(), 10);

    assert_eq!(arena.get_str(slice_a), "hello");
    assert_eq!(arena.get_str(slice_b), "world");
  }

  #[test]
  fn test_push_empty_str() {
    let mut arena = SharedArena::new();

    let slice = arena.push_str("");
    assert_eq!(slice.start, 0);
    assert_eq!(slice.len, 0);
    assert_eq!(arena.get_str(slice), "");
  }

  #[test]
  fn test_push_unicode_str() {
    let mut arena = SharedArena::new();

    let slice_a = arena.push_str("中文");
    let slice_b = arena.push_str("🎉");

    assert_eq!(arena.get_str(slice_a), "中文");
    assert_eq!(arena.get_str(slice_b), "🎉");
    assert_eq!(arena.str_len(), "中文".len() + "🎉".len());
  }
}
