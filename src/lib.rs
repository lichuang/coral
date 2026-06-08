use std::ops::{Deref, DerefMut};

pub mod common;
pub mod core;
pub mod encoding;
pub mod memory;
pub mod object;
pub mod operation;
pub mod rle;
pub mod types;
pub mod version;

pub use common::{CoralError, CoralResult};
pub use object::CounterRef;

/// The public facade for a Coral collaborative document.
///
/// `Document` is the primary entry point for users. All actual state is
/// held inside [`DocInner`](core::DocInner).
#[derive(Debug)]
pub struct Document {
  inner: core::DocInner,
}

impl Document {
  /// Creates a new document with a randomly generated peer ID.
  pub fn new() -> Self {
    Self {
      inner: core::DocInner::new(),
    }
  }

  /// Returns a reference to the counter object with the given name.
  ///
  /// The object must already exist and be a counter.
  pub fn get_counter(&mut self, name: &str) -> CoralResult<CounterRef<'_>> {
    self.inner.get_counter(name)
  }

  /// Commits all pending operations into the history and causal graph.
  ///
  /// Operations (e.g. [`CounterRef::increment`]) are applied to the
  /// document state immediately but are not recorded as a
  /// [`Commit`](core::Commit) until this method is called.
  pub fn commit(&mut self) {
    self.inner.commit();
  }

  /// Exports the full document state as a JSON string.
  ///
  /// Commits any pending operations first so the output reflects the
  /// latest state.
  pub fn export_json(&mut self) -> CoralResult<String> {
    self.inner.commit();
    encoding::export_json(&self.inner)
  }
}

impl Default for Document {
  fn default() -> Self {
    Self::new()
  }
}

impl Drop for Document {
  fn drop(&mut self) {
    self.inner.commit();
  }
}

impl Deref for Document {
  type Target = core::DocInner;

  fn deref(&self) -> &Self::Target {
    &self.inner
  }
}

impl DerefMut for Document {
  fn deref_mut(&mut self) -> &mut Self::Target {
    &mut self.inner
  }
}
