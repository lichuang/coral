//! Arena — compact container index mapping.
//!
//! [`Arena`] maintains the bidirectional mapping between [`ContainerID`] and
//! [`ContainerIdx`](crate::core::container::ContainerIdx) so that full IDs can be
//! recovered when needed for external APIs or serialization.

use crate::core::container::ContainerIdx;
use crate::types::ContainerID;
use rustc_hash::FxHashMap;

/// Manages the bidirectional mapping between [`ContainerID`] and [`ContainerIdx`].
#[derive(Debug, Clone, Default)]
pub struct Arena {
  id_to_idx: FxHashMap<ContainerID, ContainerIdx>,
  idx_to_id: Vec<ContainerID>,
  parents: Vec<Option<ContainerIdx>>,
}

impl Arena {
  /// Creates an empty arena.
  pub fn new() -> Self {
    Self::default()
  }

  /// Registers a container ID and returns its compact index.
  ///
  /// If the ID is already registered, the existing index is returned.
  pub fn register(&mut self, id: &ContainerID) -> ContainerIdx {
    if let Some(&idx) = self.id_to_idx.get(id) {
      return idx;
    }

    let index = self.idx_to_id.len() as u32;
    let idx = ContainerIdx::from_index_and_type(index, id.container_type());
    self.idx_to_id.push(id.clone());
    self.parents.push(None);
    self.id_to_idx.insert(id.clone(), idx);
    idx
  }

  /// Looks up the [`ContainerID`] for a given compact index.
  pub fn get_id(&self, idx: ContainerIdx) -> Option<&ContainerID> {
    self.idx_to_id.get(idx.to_index() as usize)
  }

  /// Looks up the compact index for a given [`ContainerID`].
  pub fn get_idx(&self, id: &ContainerID) -> Option<ContainerIdx> {
    self.id_to_idx.get(id).copied()
  }

  /// Records the parent of a container.
  pub fn set_parent(&mut self, child: ContainerIdx, parent: Option<ContainerIdx>) {
    let index = child.to_index() as usize;
    if index < self.parents.len() {
      self.parents[index] = parent;
    }
  }

  /// Returns the parent of a container, if any.
  pub fn get_parent(&self, child: ContainerIdx) -> Option<ContainerIdx> {
    self
      .parents
      .get(child.to_index() as usize)
      .copied()
      .flatten()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::types::{ContainerID, ContainerType};

  #[test]
  fn test_arena_roundtrip() {
    let mut arena = Arena::new();
    let id = ContainerID::new_root("my_map", ContainerType::Map);
    let idx = arena.register(&id);

    assert_eq!(arena.get_id(idx), Some(&id));
    assert_eq!(arena.get_idx(&id), Some(idx));
  }

  #[test]
  fn test_arena_deduplicate() {
    let mut arena = Arena::new();
    let id = ContainerID::new_root("my_list", ContainerType::List);
    let idx1 = arena.register(&id);
    let idx2 = arena.register(&id);
    assert_eq!(idx1, idx2);
  }

  #[test]
  fn test_arena_parent() {
    let mut arena = Arena::new();
    let child_id = ContainerID::new_root("child", ContainerType::Map);
    let parent_id = ContainerID::new_root("parent", ContainerType::List);
    let child = arena.register(&child_id);
    let parent = arena.register(&parent_id);

    arena.set_parent(child, Some(parent));
    assert_eq!(arena.get_parent(child), Some(parent));
  }
}
