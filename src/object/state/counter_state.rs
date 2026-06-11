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

  #[allow(unreachable_patterns)]
  pub fn apply(&mut self, op: &Op) -> CoralResult<()> {
    match &op.cmd {
      Cmd::IncCounter { delta } => {
        self.value += delta;
        Ok(())
      }
      _ => unreachable!("unsupported cmd for Counter"),
    }
  }

  #[allow(unreachable_patterns)]
  pub fn merge(&mut self, op: &Op) -> CoralResult<()> {
    match &op.cmd {
      Cmd::IncCounter { delta } => {
        self.value += delta;
        Ok(())
      }
      _ => unreachable!("unsupported cmd for Counter"),
    }
  }

  pub fn value(&self) -> f64 {
    self.value
  }
}
