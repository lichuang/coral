use crate::operation::Cmd;
use crate::rle::{HasLength, Mergeable};
use crate::types::{ContainerIndex, Counter, CounterExt};

#[derive(Debug, Clone, PartialEq)]
pub struct Op {
  /// Absolute counter value within the peer's sequence.
  ///
  /// This is the full counter (not a relative offset) and matches the
  /// corresponding position in the enclosing [`Change`](crate::core::Change).
  pub counter: Counter,
  pub container: ContainerIndex,
  pub cmd: Cmd,
}

impl Op {
  pub const fn new(counter: Counter, container: ContainerIndex, cmd: Cmd) -> Self {
    Self {
      counter,
      container,
      cmd,
    }
  }
}

impl HasLength for Op {
  fn content_len(&self) -> usize {
    1
  }
}

impl Mergeable for Op {
  fn is_mergeable(&self, other: &Self) -> bool {
    self
      .counter
      .is_consecutive(other.counter, self.content_len() as Counter)
      && self.container == other.container
      && self.cmd.is_mergeable(&other.cmd)
  }

  fn merge(&mut self, other: &Self) {
    self.cmd.merge(&other.cmd);
  }
}
