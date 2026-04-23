//! Arena — compact container index mapping.
//!
//! [`Arena`] maintains the bidirectional mapping between [`ContainerID`] and
//! [`ContainerIdx`](crate::core::container::ContainerIdx) so that full IDs can
//! be recovered when needed for external APIs or serialization.
//!
//! It also tracks parent-child relationships and per-container nesting depth,
//! aligned with Loro's `SharedArena` design.

use crate::core::container::ContainerIdx;
use crate::types::ContainerID;
use rustc_hash::FxHashMap;
use std::num::NonZeroU16;
use std::sync::Mutex;

/// Manages the bidirectional mapping between [`ContainerID`] and [`ContainerIdx`].
///
/// Each field is protected by a [`Mutex`], matching Loro's `InnerSharedArena`
/// design so that the arena can be shared between `OpLog` and `DocState`.
#[derive(Debug, Default)]
pub struct Arena {
  id_to_idx: Mutex<FxHashMap<ContainerID, ContainerIdx>>,
  idx_to_id: Mutex<Vec<ContainerID>>,
  parents: Mutex<FxHashMap<ContainerIdx, Option<ContainerIdx>>>,
  depths: Mutex<Vec<Option<NonZeroU16>>>,
}

impl Arena {
  /// Creates an empty arena.
  pub fn new() -> Self {
    Self::default()
  }

  /// Registers a container ID and returns its compact index.
  ///
  /// If the ID is already registered, the existing index is returned.
  pub fn register(&self, id: &ContainerID) -> ContainerIdx {
    let mut id_to_idx = self.id_to_idx.lock().unwrap();
    if let Some(&idx) = id_to_idx.get(id) {
      return idx;
    }

    let mut idx_to_id = self.idx_to_id.lock().unwrap();
    let index = idx_to_id.len() as u32;
    let idx = ContainerIdx::from_index_and_type(index, id.container_type());
    idx_to_id.push(id.clone());
    drop(idx_to_id);

    let mut depths = self.depths.lock().unwrap();
    depths.push(None);

    id_to_idx.insert(id.clone(), idx);
    idx
  }

  /// Looks up the [`ContainerID`] for a given compact index.
  pub fn get_id(&self, idx: ContainerIdx) -> Option<ContainerID> {
    let idx_to_id = self.idx_to_id.lock().unwrap();
    idx_to_id.get(idx.to_index() as usize).cloned()
  }

  /// Looks up the compact index for a given [`ContainerID`].
  pub fn get_idx(&self, id: &ContainerID) -> Option<ContainerIdx> {
    let id_to_idx = self.id_to_idx.lock().unwrap();
    id_to_idx.get(id).copied()
  }

  /// Records the parent of a container and updates its nesting depth.
  ///
  /// Depth is computed automatically from the parent's depth:
  /// - Parent is `None` → child depth is `None` (root-level container).
  /// - Parent has depth `d` → child depth is `d + 1`.
  /// - Parent has no depth recorded → child depth is `1`.
  pub fn set_parent(&self, child: ContainerIdx, parent: Option<ContainerIdx>) {
    let mut parents = self.parents.lock().unwrap();
    parents.insert(child, parent);
    drop(parents);

    let mut depths = self.depths.lock().unwrap();
    let depth = parent.and_then(|p| {
      depths
        .get(p.to_index() as usize)
        .copied()
        .flatten()
        .and_then(|d| NonZeroU16::new(d.get() + 1))
        .or_else(|| NonZeroU16::new(1))
    });

    let child_index = child.to_index() as usize;
    if child_index < depths.len() {
      depths[child_index] = depth;
    }
  }

  /// Returns the parent of a container, if any.
  pub fn get_parent(&self, child: ContainerIdx) -> Option<ContainerIdx> {
    let parents = self.parents.lock().unwrap();
    parents.get(&child).copied().flatten()
  }

  /// Returns the nesting depth of a container.
  ///
  /// - `None` → the container is at the root level (no parent).
  /// - `Some(d)` → the container is `d` levels below the root.
  pub fn get_depth(&self, idx: ContainerIdx) -> Option<NonZeroU16> {
    let depths = self.depths.lock().unwrap();
    depths.get(idx.to_index() as usize).copied().flatten()
  }

  /// Walks the parent chain from `child` up to the root and returns the
  /// sequence of indices in child-to-root order.
  ///
  /// The returned vector starts with `child` itself and ends with the
  /// top-most ancestor whose parent is `None`.
  pub fn get_path_to_root(&self, child: ContainerIdx) -> Vec<ContainerIdx> {
    let parents = self.parents.lock().unwrap();
    let mut path = vec![child];
    let mut current = child;
    // Defensive limit: max nesting depth is bounded by u16::MAX,
    // but we use a much smaller guard to catch accidental cycles.
    for _ in 0..1024 {
      match parents.get(&current).copied().flatten() {
        Some(parent) => {
          path.push(parent);
          current = parent;
        }
        None => break,
      }
    }
    path
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::types::{ContainerID, ContainerType};

  #[test]
  fn test_arena_roundtrip() {
    let arena = Arena::new();
    let id = ContainerID::new_root("my_map", ContainerType::Map);
    let idx = arena.register(&id);

    assert_eq!(arena.get_id(idx), Some(id.clone()));
    assert_eq!(arena.get_idx(&id), Some(idx));
  }

  #[test]
  fn test_arena_deduplicate() {
    let arena = Arena::new();
    let id = ContainerID::new_root("my_list", ContainerType::List);
    let idx1 = arena.register(&id);
    let idx2 = arena.register(&id);
    assert_eq!(idx1, idx2);
  }

  #[test]
  fn test_arena_parent() {
    let arena = Arena::new();
    let child_id = ContainerID::new_root("child", ContainerType::Map);
    let parent_id = ContainerID::new_root("parent", ContainerType::List);
    let child = arena.register(&child_id);
    let parent = arena.register(&parent_id);

    arena.set_parent(child, Some(parent));
    assert_eq!(arena.get_parent(child), Some(parent));
  }

  #[test]
  fn test_arena_depth_computed() {
    let arena = Arena::new();
    let root = arena.register(&ContainerID::new_root("root", ContainerType::Map));
    let child = arena.register(&ContainerID::new_root("child", ContainerType::List));
    let grandchild = arena.register(&ContainerID::new_root("grandchild", ContainerType::Map));

    // Root has no parent → depth is None.
    arena.set_parent(root, None);
    assert_eq!(arena.get_depth(root), None);

    // Child of root → depth 1.
    arena.set_parent(child, Some(root));
    assert_eq!(arena.get_depth(child), NonZeroU16::new(1));

    // Grandchild of root → depth 2.
    arena.set_parent(grandchild, Some(child));
    assert_eq!(arena.get_depth(grandchild), NonZeroU16::new(2));
  }

  #[test]
  fn test_arena_path_to_root() {
    let arena = Arena::new();
    let root = arena.register(&ContainerID::new_root("root", ContainerType::Map));
    let child = arena.register(&ContainerID::new_root("child", ContainerType::List));
    let grandchild = arena.register(&ContainerID::new_root("grandchild", ContainerType::Map));

    arena.set_parent(root, None);
    arena.set_parent(child, Some(root));
    arena.set_parent(grandchild, Some(child));

    let path = arena.get_path_to_root(grandchild);
    assert_eq!(path, vec![grandchild, child, root]);

    let path_child = arena.get_path_to_root(child);
    assert_eq!(path_child, vec![child, root]);

    let path_root = arena.get_path_to_root(root);
    assert_eq!(path_root, vec![root]);
  }

  #[test]
  fn test_arena_parent_none_after_some() {
    let arena = Arena::new();
    let parent = arena.register(&ContainerID::new_root("parent", ContainerType::Map));
    let child = arena.register(&ContainerID::new_root("child", ContainerType::List));

    arena.set_parent(child, Some(parent));
    assert_eq!(arena.get_parent(child), Some(parent));
    assert_eq!(arena.get_depth(child), NonZeroU16::new(1));

    // Unsetting parent resets depth to None.
    arena.set_parent(child, None);
    assert_eq!(arena.get_parent(child), None);
    assert_eq!(arena.get_depth(child), None);
  }
}
