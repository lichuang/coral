use super::DocInner;
use crate::types::ObjectIndex;

/// Type markers for [`ObjectRef`].
///
/// These are zero-sized types used only at compile time to distinguish
/// the kind of object a reference points to.
pub mod marker {
  /// Marker for a counter object.
  pub struct Counter;
}

/// A typed reference to a CRDT object within a document.
///
/// `ObjectRef` is the unified handle type for all container operations.
/// The generic parameter `T` distinguishes the object kind at compile time,
/// allowing each kind to expose its own methods via dedicated `impl` blocks.
#[allow(dead_code)]
pub struct ObjectRef<'a, T> {
  doc: &'a mut DocInner,
  index: ObjectIndex,
  _marker: std::marker::PhantomData<T>,
}

impl<'a, T> ObjectRef<'a, T> {
  /// Creates a new `ObjectRef`.
  #[allow(dead_code)]
  pub(crate) fn new(doc: &'a mut DocInner, index: ObjectIndex) -> Self {
    Self {
      doc,
      index,
      _marker: std::marker::PhantomData,
    }
  }

  /// Returns the [`ObjectIndex`] of the referenced object.
  pub fn index(&self) -> ObjectIndex {
    self.index
  }
}

/// Alias for [`ObjectRef`] pointing to a counter.
pub type CounterRef<'a> = ObjectRef<'a, marker::Counter>;

impl ObjectRef<'_, marker::Counter> {
  /// Returns the current value of the counter.
  pub fn value(&self) -> f64 {
    todo!()
  }

  /// Increments the counter by `delta`.
  ///
  /// This may generate a new operation and append it to the current change.
  pub fn increment(&mut self, delta: f64) {
    let _ = delta;
    todo!()
  }
}
