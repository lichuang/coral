pub mod counter_state;

pub use counter_state::CounterState;

use crate::common::CoralResult;
use crate::operation::Op;

/// Per-container state for a CRDT object.
///
/// Each variant wraps a type-specific state machine.  The enum is the
/// dispatch layer that routes operations to the correct implementation.
///
/// Two entry points exist:
/// - [`apply`](ObjectState::apply) — called for **local** operations.
///   The op has already taken effect optimistically; this records it.
/// - [`merge`](ObjectState::merge) — called for **remote** operations
///   imported via [`import_json`](crate::Document::import_json).
///   Type-specific merge semantics live here (e.g. LWW for Map).
pub enum ObjectState {
  Counter(CounterState),
}

impl ObjectState {
  /// Apply a **local** operation to this state.
  ///
  /// Called from [`DocInner::push_local_op`] immediately when the user
  /// performs an action (e.g. `counter.increment(5)`).  The state is
  /// updated synchronously so that subsequent reads reflect the change.
  pub fn apply(&mut self, op: &Op) -> CoralResult<()> {
    match self {
      Self::Counter(s) => s.apply(op),
    }
  }

  /// Merge a **remote** operation into this state.
  ///
  /// Called from [`DocInner::merge_diff`] after a batch of commits has
  /// been saved and the version-vector diff computed.  Each container
  /// type defines its own merge semantics:
  /// - Counter: accumulate deltas (commutative).
  /// - Map (future): last-writer-wins per key using Lamport timestamps.
  /// - List/Text (future): position-aware insert/delete resolution.
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
