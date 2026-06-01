use crate::common::CoralResult;

use super::CounterRef;

/// The internal state of a collaborative document.
///
/// `DocInner` holds the actual CRDT state: the causal graph, the change
/// store, the shared arena, and all container states. It is wrapped by
/// [`Document`](crate::Document) which provides the public API.
#[derive(Debug, Default)]
pub struct DocInner {}

impl DocInner {
  /// Creates a new empty `DocInner`.
  pub fn new() -> Self {
    Self::default()
  }

  /// Returns a reference to the counter object with the given name.
  ///
  /// The object must already exist and be a counter.
  pub fn get_counter(&self, _name: &str) -> CoralResult<CounterRef<'_>> {
    todo!()
  }

  /// Ensures a counter object with the given name exists, creating it if
  /// necessary.
  ///
  /// Returns an error if the name is already used by a different object type.
  pub fn ensure_counter(&mut self, _name: &str) -> CoralResult<CounterRef<'_>> {
    todo!()
  }
}
