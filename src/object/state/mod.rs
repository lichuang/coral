pub mod counter_state;

pub use counter_state::CounterState;

use crate::common::CoralResult;
use crate::operation::Op;

pub enum ObjectState {
  Counter(CounterState),
}

impl ObjectState {
  pub fn apply(&mut self, op: &Op) -> CoralResult<()> {
    match self {
      Self::Counter(s) => s.apply(op),
    }
  }

  pub fn merge(&mut self, op: &Op) -> CoralResult<()> {
    match self {
      Self::Counter(s) => s.merge(op),
    }
  }

  pub fn as_counter(&self) -> Option<&CounterState> {
    match self {
      Self::Counter(s) => Some(s),
    }
  }
}

impl Default for ObjectState {
  fn default() -> Self {
    Self::Counter(CounterState::new())
  }
}
