use crate::common::{CoralError, CoralResult};
use crate::types::{ObjectId, ObjectType};

use super::{CounterRef, ObjectRegistry};

/// The internal state of a collaborative document.
///
/// `DocInner` holds the actual CRDT state: the causal graph, the change
/// store, the shared arena, and all container states. It is wrapped by
/// [`Document`](crate::Document) which provides the public API.
#[derive(Debug, Default)]
pub struct DocInner {
  registry: ObjectRegistry,
}

impl DocInner {
  /// Creates a new empty `DocInner`.
  pub fn new() -> Self {
    Self::default()
  }

  /// Returns a reference to the counter object with the given name.
  ///
  /// If the object does not yet exist, a new entry is allocated in the
  /// registry. If the name is already used by a different type, returns
  /// a type mismatch error.
  pub fn get_counter(&mut self, name: &str) -> CoralResult<CounterRef<'_>> {
    if let Some(index) = self.registry.get_root(name) {
      let typ = index.typ()?;
      if typ != ObjectType::Counter {
        return Err(CoralError::TypeMismatch {
          expected: "Counter".to_string(),
          actual: typ.to_string(),
        });
      }
      return Ok(CounterRef::new(self, index));
    }

    let id = ObjectId::Root {
      name: name.to_string(),
      typ: ObjectType::Counter,
    };
    let index = self.registry.alloc_root(name.to_string(), id);
    Ok(CounterRef::new(self, index))
  }
}
