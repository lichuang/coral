//! Counter CRDT state — PN-Counter (arithmetic merge).
//!
//! The counter stores a single `f64` value.  Local ops carry a delta which is
//! added to the current value.  Concurrent increments from different peers are
//! merged by simple addition, so the result is the algebraic sum of all deltas.
//!
//! Idempotency at the OpLog layer (duplicate changes are ignored) means that
//! the *same* logical op will never be applied twice to the state.

use crate::core::container::ContainerIdx;
use crate::op::{Op, OpContent};
use crate::state::container_state::{ApplyLocalOpReturn, ContainerState, Diff, InternalDiff};
use crate::types::{ContainerType, CoralValue};

// ---------------------------------------------------------------------------
// CounterState
// ---------------------------------------------------------------------------

/// State for a [`ContainerType::Counter`](crate::types::ContainerType::Counter).
#[derive(Debug, Clone)]
pub struct CounterState {
  #[allow(dead_code)]
  idx: ContainerIdx,
  value: f64,
}

impl CounterState {
  /// Creates a new counter state with value `0.0`.
  pub fn new(idx: ContainerIdx) -> Self {
    Self { idx, value: 0.0 }
  }

  /// Returns the current numeric value.
  pub fn value(&self) -> f64 {
    self.value
  }
}

impl ContainerState for CounterState {
  fn container_idx(&self) -> ContainerIdx {
    self.idx
  }

  fn container_type(&self) -> ContainerType {
    ContainerType::Counter
  }

  fn is_state_empty(&self) -> bool {
    false
  }

  fn apply_local_op(&mut self, op: &Op) -> ApplyLocalOpReturn {
    match &op.content {
      OpContent::Counter(delta) => {
        self.value += delta;
      }
      other => {
        panic!(
          "CounterState::apply_local_op: expected Counter op, got {:?}",
          other
        )
      }
    }
    ApplyLocalOpReturn::default()
  }

  fn apply_diff_and_convert(&mut self, diff: InternalDiff) -> Diff {
    let InternalDiff::Counter(delta) = diff;
    self.value += delta;
    Diff::Counter(self.value)
  }

  fn apply_diff(&mut self, diff: InternalDiff) {
    let InternalDiff::Counter(delta) = diff;
    self.value += delta;
  }

  fn to_diff(&self) -> InternalDiff {
    InternalDiff::Counter(self.value)
  }

  fn get_value(&self) -> CoralValue {
    CoralValue::Double(self.value)
  }

  fn fork(&self) -> Box<dyn ContainerState> {
    Box::new(self.clone())
  }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::container::ContainerIdx;
  use crate::types::ContainerType;

  fn make_idx() -> ContainerIdx {
    ContainerIdx::from_index_and_type(0, ContainerType::Counter)
  }

  fn make_state() -> CounterState {
    CounterState::new(make_idx())
  }

  #[test]
  fn test_counter_new_is_zero() {
    let s = make_state();
    assert_eq!(s.value(), 0.0);
    assert_eq!(s.get_value(), CoralValue::Double(0.0));
  }

  #[test]
  fn test_counter_single_op() {
    let mut s = make_state();
    let op = Op::new(0, make_idx(), OpContent::Counter(3.0));
    s.apply_local_op(&op);
    assert_eq!(s.value(), 3.0);
    assert_eq!(s.get_value(), CoralValue::Double(3.0));
  }

  #[test]
  fn test_counter_accumulate() {
    let mut s = make_state();
    s.apply_local_op(&Op::new(0, make_idx(), OpContent::Counter(3.0)));
    s.apply_local_op(&Op::new(1, make_idx(), OpContent::Counter(-2.0)));
    assert_eq!(s.value(), 1.0);
  }

  #[test]
  fn test_counter_apply_diff() {
    let mut s = make_state();
    s.apply_diff(InternalDiff::Counter(5.0));
    assert_eq!(s.value(), 5.0);
  }

  #[test]
  fn test_counter_to_diff_apply_diff_inverse() {
    let mut s = make_state();
    s.apply_local_op(&Op::new(0, make_idx(), OpContent::Counter(7.0)));

    let diff = s.to_diff();
    let mut empty = CounterState::new(make_idx());
    empty.apply_diff(diff);
    assert_eq!(empty.value(), s.value());
  }

  #[test]
  fn test_counter_fork() {
    let mut s = make_state();
    s.apply_local_op(&Op::new(0, make_idx(), OpContent::Counter(4.0)));
    let forked = s.fork();
    assert_eq!(forked.get_value(), CoralValue::Double(4.0));
    // Fork is independent — further changes to original do not affect fork.
    s.apply_local_op(&Op::new(1, make_idx(), OpContent::Counter(1.0)));
    assert_eq!(s.value(), 5.0);
    assert_eq!(forked.get_value(), CoralValue::Double(4.0));
  }

  #[test]
  fn test_counter_apply_diff_and_convert() {
    let mut s = make_state();
    let diff = s.apply_diff_and_convert(InternalDiff::Counter(2.5));
    assert_eq!(s.value(), 2.5);
    assert_eq!(diff, Diff::Counter(2.5));
  }

  #[test]
  fn test_counter_container_type() {
    let s = make_state();
    assert_eq!(s.container_idx(), make_idx());
    assert_eq!(s.container_type(), ContainerType::Counter);
    assert!(!s.is_state_empty());
  }

  #[test]
  fn test_counter_concurrent_merge_algebraic() {
    // Peer A increments by +3, peer B increments by -2.
    // Merged result should be +1 (algebraic sum).
    let mut peer_a = make_state();
    let mut peer_b = make_state();

    peer_a.apply_local_op(&Op::new(0, make_idx(), OpContent::Counter(3.0)));
    peer_b.apply_local_op(&Op::new(0, make_idx(), OpContent::Counter(-2.0)));

    // Capture each peer's diff *before* cross-applying.
    let diff_a = peer_a.to_diff();
    let diff_b = peer_b.to_diff();

    // Simulate merge: apply B's diff to A, and A's diff to B.
    peer_a.apply_diff(diff_b);
    peer_b.apply_diff(diff_a);

    assert_eq!(peer_a.value(), 1.0);
    assert_eq!(peer_b.value(), 1.0);
    assert_eq!(peer_a.get_value(), peer_b.get_value());
  }
}
