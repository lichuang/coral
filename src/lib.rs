use std::ops::{Deref, DerefMut};

pub mod common;
pub mod core;
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
}

impl Default for Document {
  fn default() -> Self {
    Self::new()
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
