use crate::common::CoralResult;
use crate::operation::{Cmd, Op};

#[derive(Debug, Clone, Default)]
pub struct CounterState {
  value: f64,
}

impl CounterState {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn apply(&mut self, op: &Op) -> CoralResult<()> {
    match &op.cmd {
      Cmd::IncCounter { delta } => {
        self.value += delta;
        Ok(())
      }
    }
  }

  pub fn value(&self) -> f64 {
    self.value
  }
}
