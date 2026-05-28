use crate::rle::{HasLength, Mergeable};

#[derive(Debug, Clone, PartialEq)]
pub enum Cmd {
  IncCounter { delta: f64 },
}

impl HasLength for Cmd {
  fn content_len(&self) -> usize {
    1
  }
}

impl Mergeable for Cmd {
  fn is_mergeable(&self, _other: &Self) -> bool {
    false
  }

  fn merge(&mut self, _other: &Self) {
    unreachable!()
  }
}
