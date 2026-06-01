use core::CounterRef;
use std::ops::{Deref, DerefMut};

pub mod common;
pub mod core;
pub mod memory;
pub mod operation;
pub mod rle;
pub mod types;
pub mod version;

pub use common::{CoralError, CoralResult};

/// The public facade for a Coral collaborative document.
///
/// `Document` is the primary entry point for users. All actual state is
/// held inside [`DocInner`](core::DocInner).
#[derive(Debug, Default)]
pub struct Document {
  inner: core::DocInner,
}

impl Document {
  /// Returns a reference to the counter object with the given name.
  ///
  /// The object must already exist and be a counter.
  pub fn get_counter(&self, name: &str) -> CoralResult<CounterRef<'_>> {
    self.inner.get_counter(name)
  }

  /// Ensures a counter object with the given name exists, creating it if
  /// necessary.
  ///
  /// Returns an error if the name is already used by a different object type.
  pub fn ensure_counter(&mut self, name: &str) -> CoralResult<CounterRef<'_>> {
    self.inner.ensure_counter(name)
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
