use crate::types::{ObjectId, ObjectIndex};
use rustc_hash::FxHashMap;

/// Manages bidirectional mappings between logical [`ObjectId`]s and runtime
/// [`ObjectIndex`]es.
///
/// Indexes are allocated globally and monotonically per document, starting
/// from 0. This allows the reverse lookup table to be a dense `Vec`.
#[derive(Debug, Default)]
pub struct ObjectRegistry {
  by_root_name: FxHashMap<String, ObjectIndex>,
  by_id: FxHashMap<ObjectId, ObjectIndex>,
  by_index: Vec<ObjectId>,
}

impl ObjectRegistry {
  /// Creates a new empty registry.
  pub fn new() -> Self {
    Self::default()
  }

  /// Looks up a root object by name.
  pub fn get_root(&self, name: &str) -> Option<ObjectIndex> {
    self.by_root_name.get(name).copied()
  }

  /// Looks up the [`ObjectIndex`] for a given [`ObjectId`].
  pub fn get_by_id(&self, id: &ObjectId) -> Option<ObjectIndex> {
    self.by_id.get(id).copied()
  }

  /// Looks up the [`ObjectId`] for a given [`ObjectIndex`].
  pub fn get_by_index(&self, index: ObjectIndex) -> Option<&ObjectId> {
    self.by_index.get(index.index() as usize)
  }

  /// Looks up the [`ObjectId`] by raw sequence number.
  pub fn get_by_index_from(&self, seq: usize) -> Option<&ObjectId> {
    self.by_index.get(seq)
  }

  /// Returns the total number of registered objects.
  pub fn object_count(&self) -> usize {
    self.by_index.len()
  }

  /// Allocates a new globally unique [`ObjectIndex`] for a root-level object
  /// and registers all mappings.
  pub fn alloc_root(&mut self, name: String, id: ObjectId) -> ObjectIndex {
    let seq = self.by_index.len() as u32;
    let index = ObjectIndex::new(seq, id.typ());
    self.by_root_name.insert(name, index);
    self.by_id.insert(id.clone(), index);
    self.by_index.push(id);
    index
  }
}
