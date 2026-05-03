//! [`ContainerState`] trait — unified interface for all CRDT container states.
//!
//! Every container type (Counter, Map, List, Text, Tree, …) implements this
//! trait so that [`DocState`](crate::doc::DocState) can dispatch operations
//! without knowing the concrete type.

use crate::core::container::ContainerIdx;
use crate::op::Op;
use crate::types::{ContainerID, ContainerType, CoralValue};

// ---------------------------------------------------------------------------
// ApplyLocalOpReturn
// ---------------------------------------------------------------------------

/// Side-effects produced by [`ContainerState::apply_local_op`].
///
/// For example, deleting a Tree node may invalidate its child containers.
#[derive(Debug, Default, Clone)]
pub(crate) struct ApplyLocalOpReturn {
  /// Child containers that were logically deleted as a side-effect.
  #[allow(dead_code)]
  pub deleted_containers: Vec<ContainerIdx>,
}

// ---------------------------------------------------------------------------
// InternalDiff
// ---------------------------------------------------------------------------

/// Crate-internal diff representation used for state reconstruction,
/// checkout, and cross-container synchronisation.
///
/// Variants will expand as Map, List, Text and Tree states are implemented.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) enum InternalDiff {
  /// Counter delta.
  Counter(f64),
}

// ---------------------------------------------------------------------------
// Diff
// ---------------------------------------------------------------------------

/// Public diff representation exposed to users and event subscribers.
///
/// For Counter this is the **current absolute value** (not a delta).
#[derive(Debug, Clone, PartialEq)]
pub enum Diff {
  /// Counter current value.
  Counter(f64),
}

// ---------------------------------------------------------------------------
// ContainerState trait
// ---------------------------------------------------------------------------

/// Unified interface for all CRDT container states.
#[allow(dead_code)]
pub(crate) trait ContainerState: std::fmt::Debug {
  /// The compact index of this container.
  fn container_idx(&self) -> ContainerIdx;

  /// The runtime type of this container.
  fn container_type(&self) -> ContainerType;

  /// Whether the state is considered "empty".
  ///
  /// Counter always returns `false` because it always holds a value.
  fn is_state_empty(&self) -> bool;

  /// Apply a local operation (incremental update).
  ///
  /// # Panics
  ///
  /// Panics if `op` is not targeted at this container type (invariant
  /// violation).
  fn apply_local_op(&mut self, op: &Op) -> ApplyLocalOpReturn;

  /// Apply an internal diff and return the user-facing diff for event
  /// notifications.
  ///
  /// # Panics
  ///
  /// Panics if the diff variant does not match this container type.
  fn apply_diff_and_convert(&mut self, diff: InternalDiff) -> Diff;

  /// Apply an internal diff (used for batch rebuild, checkout, or sync).
  ///
  /// # Panics
  ///
  /// Panics if the diff variant does not match this container type.
  fn apply_diff(&mut self, diff: InternalDiff);

  /// Export the current state as an internal diff.
  ///
  /// Applying [`to_diff`](ContainerState::to_diff) output to an empty state
  /// of the same type should reproduce the original state.
  fn to_diff(&self) -> InternalDiff;

  /// Get the current user-visible value.
  fn get_value(&self) -> CoralValue;

  /// Look up a child container by its [`ContainerID`].
  ///
  /// Default implementation returns `None` (no children).
  fn get_child_index(&self, _id: &ContainerID) -> Option<ContainerIdx> {
    None
  }

  /// Return all child container IDs.
  ///
  /// Default implementation returns an empty vec.
  fn get_child_containers(&self) -> Vec<ContainerID> {
    Vec::new()
  }

  /// Check whether this state contains a given child container.
  ///
  /// Default implementation returns `false`.
  fn contains_child(&self, _id: &ContainerID) -> bool {
    false
  }

  /// Clone the state into a boxed trait object (for fork / snapshot).
  fn fork(&self) -> Box<dyn ContainerState>;
}
