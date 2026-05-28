use crate::operation::Cmd;
use crate::rle::{HasLength, Mergeable};
use crate::types::{ContainerIndex, Counter, OpId};

#[derive(Debug, Clone, PartialEq)]
pub struct Op {
  pub id: OpId,
  pub container: ContainerIndex,
  pub cmd: Cmd,
}

impl Op {
  pub const fn new(id: OpId, container: ContainerIndex, cmd: Cmd) -> Self {
    Self { id, container, cmd }
  }
}

impl HasLength for Op {
  fn content_len(&self) -> usize {
    self.cmd.content_len()
  }
}

impl Mergeable for Op {
  fn is_mergeable(&self, other: &Self) -> bool {
    self
      .id
      .is_consecutive(&other.id, self.content_len() as Counter)
      && self.container == other.container
      && self.cmd.is_mergeable(&other.cmd)
  }

  fn merge(&mut self, other: &Self) {
    self.cmd.merge(&other.cmd);
  }
}
