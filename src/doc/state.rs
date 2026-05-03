//! DocState — collection and dispatch of all container states.
//!
//! [`DocState`] holds the live CRDT state for every container in the document.
//! It is rebuilt by replaying the [`OpLog`](crate::oplog::OpLog) and updated
//! incrementally when local or remote changes arrive.

use crate::core::container::ContainerIdx;
use crate::memory::arena::SharedArena;
use crate::op::Op;
use crate::state::CounterState;
use crate::state::container_state::{ContainerState, InternalDiff};
use crate::types::{ContainerType, CoralValue};
use rustc_hash::FxHashMap;

/// Live CRDT state for the entire document.
#[derive(Debug)]
pub struct DocState {
  #[allow(dead_code)]
  arena: SharedArena,
  states: FxHashMap<ContainerIdx, Box<dyn ContainerState>>,
}

impl DocState {
  /// Creates an empty `DocState` backed by the given arena.
  pub fn new(arena: SharedArena) -> Self {
    Self {
      arena,
      states: FxHashMap::default(),
    }
  }

  /// Apply a local operation to the appropriate container state.
  ///
  /// If the container has never been accessed before, a fresh state of the
  /// correct type is created on demand.
  pub fn apply_local_op(&mut self, op: &Op, _arena: &SharedArena) {
    match op.container.get_type() {
      ContainerType::Counter => {
        let state = self.get_or_create_state(op.container);
        state.apply_local_op(op);
      }
      _ => {
        // TODO(Phase 10+): Map, List, Text, Tree, MovableList
      }
    }
  }

  /// Finalise any pending state changes at the end of a transaction.
  ///
  /// Currently a no-op for Counter; will trigger state-level commit hooks
  /// (e.g. flushing btree caches) when more complex container states land.
  pub fn commit(&mut self) {
    // TODO(Phase 10+): state-level commit logic for Map/List/Text/Tree
  }

  /// Get the current value of a container.
  ///
  /// For containers that have never been touched, a type-appropriate default
  /// value is returned (e.g. `0.0` for Counter).
  pub fn get_value(&self, idx: ContainerIdx) -> CoralValue {
    match self.states.get(&idx) {
      Some(state) => state.get_value(),
      None => match idx.get_type() {
        ContainerType::Counter => CoralValue::Double(0.0),
        _ => CoralValue::Null,
      },
    }
  }

  /// Apply an internal diff to a container state.
  ///
  /// Used during checkout, time-travel, or remote sync.
  #[allow(dead_code)]
  pub(crate) fn apply_diff(&mut self, idx: ContainerIdx, diff: InternalDiff) {
    match idx.get_type() {
      ContainerType::Counter => {
        let state = self.get_or_create_state(idx);
        state.apply_diff(diff);
      }
      _ => {
        // TODO(Phase 10+): Map, List, Text, Tree, MovableList
      }
    }
  }

  /// Look up or lazily create the [`ContainerState`] for `idx`.
  fn get_or_create_state(&mut self, idx: ContainerIdx) -> &mut Box<dyn ContainerState> {
    self
      .states
      .entry(idx)
      .or_insert_with(|| match idx.get_type() {
        ContainerType::Counter => Box::new(CounterState::new(idx)),
        other => panic!(
          "DocState::get_or_create_state: unsupported container type {:?} \
           (Counter is the only implemented type)",
          other
        ),
      })
  }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::container::ContainerIdx;
  use crate::memory::arena::{InnerArena, SharedArena};
  use crate::types::ContainerType;

  fn make_doc_state() -> DocState {
    DocState::new(SharedArena::new(InnerArena::new()))
  }

  fn counter_idx() -> ContainerIdx {
    ContainerIdx::from_index_and_type(0, ContainerType::Counter)
  }

  #[test]
  fn test_doc_state_counter_default_value() {
    let doc = make_doc_state();
    let idx = counter_idx();
    assert_eq!(doc.get_value(idx), CoralValue::Double(0.0));
  }

  #[test]
  fn test_doc_state_apply_local_op_counter() {
    let mut doc = make_doc_state();
    let idx = counter_idx();
    doc.apply_local_op(
      &Op::new(0, idx, OpContent::Counter(3.0)),
      &doc.arena.clone(),
    );
    assert_eq!(doc.get_value(idx), CoralValue::Double(3.0));
  }

  #[test]
  fn test_doc_state_apply_diff_counter() {
    let mut doc = make_doc_state();
    let idx = counter_idx();
    doc.apply_diff(idx, InternalDiff::Counter(5.0));
    assert_eq!(doc.get_value(idx), CoralValue::Double(5.0));
  }

  use crate::op::OpContent;
}
