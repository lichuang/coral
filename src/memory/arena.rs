//! InnerArena — compact container index mapping.
//!
//! [`InnerArena`] maintains the bidirectional mapping between [`ContainerID`] and
//! [`ContainerIdx`](crate::core::container::ContainerIdx) so that full IDs can
//! be recovered when needed for external APIs or serialization.
//!
//! It also tracks parent-child relationships and per-container nesting depth,
//! aligned with Loro's `SharedArena` design.

use crate::core::container::ContainerIdx;
use crate::memory::str_arena::{StrAllocResult, StrArena};
use crate::types::{ContainerID, CoralValue};
use rustc_hash::FxHashMap;
use std::num::NonZeroU16;
use std::sync::{Arc, Mutex};

/// Manages the bidirectional mapping between [`ContainerID`] and [`ContainerIdx`].
///
/// Each field is protected by a [`Mutex`], matching Loro's `InnerSharedArena`
/// design so that the arena can be shared between `OpLog` and `DocState`.
#[derive(Debug, Default)]
pub struct InnerArena {
  id_to_idx: Mutex<FxHashMap<ContainerID, ContainerIdx>>,
  idx_to_id: Mutex<Vec<ContainerID>>,
  parents: Mutex<FxHashMap<ContainerIdx, Option<ContainerIdx>>>,
  depths: Mutex<Vec<Option<NonZeroU16>>>,
  /// Append-only value storage.  Each [`CoralValue`] is pushed in order and
  /// never removed; indices are stable for the lifetime of the arena.
  values: Mutex<Vec<CoralValue>>,
  /// Append-only string storage with unicode indexing.
  str_arena: Mutex<StrArena>,
}

impl InnerArena {
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

  /// Append a [`CoralValue`] to the value arena and return its stable index.
  pub fn alloc_value(&self, value: CoralValue) -> usize {
    let mut values = self.values.lock().unwrap();
    let idx = values.len();
    values.push(value);
    idx
  }

  /// Retrieve a cloned [`CoralValue`] by its arena index.
  pub fn get_value(&self, idx: usize) -> Option<CoralValue> {
    let values = self.values.lock().unwrap();
    values.get(idx).cloned()
  }

  /// Append a string to the string arena and return a handle describing it.
  pub fn alloc_str(&self, s: &str) -> StrAllocResult {
    let mut str_arena = self.str_arena.lock().unwrap();
    str_arena.alloc(s)
  }

  /// Retrieve the string described by a previous [`StrAllocResult`].
  ///
  /// Returns `None` when the result was produced by a different arena instance
  /// or when it is otherwise stale.
  pub fn get_str(&self, result: &StrAllocResult) -> Option<String> {
    let str_arena = self.str_arena.lock().unwrap();
    // Sanity check: the allocation must fit within the current buffer.
    if result.start_byte > str_arena.len_bytes() {
      return None;
    }
    Some(str_arena.get_str(result).to_string())
  }

  /// Deep-clone the arena state.
  ///
  /// All mappings, parent links, values and string buffers are duplicated so
  /// that modifications to the clone do not affect the original.
  pub fn fork(&self) -> Self {
    Self {
      id_to_idx: Mutex::new(self.id_to_idx.lock().unwrap().clone()),
      idx_to_id: Mutex::new(self.idx_to_id.lock().unwrap().clone()),
      parents: Mutex::new(self.parents.lock().unwrap().clone()),
      depths: Mutex::new(self.depths.lock().unwrap().clone()),
      values: Mutex::new(self.values.lock().unwrap().clone()),
      str_arena: Mutex::new(self.str_arena.lock().unwrap().clone()),
    }
  }
}

/// Thread-safe, reference-counted wrapper around [`InnerArena`].
///
/// `Clone` performs a shallow copy (increments the reference count).
/// Use [`SharedArena::fork`] to obtain an independent deep copy.
#[derive(Debug, Clone)]
pub struct SharedArena(Arc<InnerArena>);

impl SharedArena {
  /// Wrap an existing [`InnerArena`] in a reference-counted pointer.
  pub fn new(arena: InnerArena) -> Self {
    Self(Arc::new(arena))
  }

  /// Create an independent deep copy of the arena state.
  pub fn fork(&self) -> Self {
    Self(Arc::new(self.0.fork()))
  }
}

impl std::ops::Deref for SharedArena {
  type Target = InnerArena;
  fn deref(&self) -> &Self::Target {
    &self.0
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::types::{ContainerID, ContainerType};

  #[test]
  fn test_arena_roundtrip() {
    let arena = InnerArena::new();
    let id = ContainerID::new_root("my_map", ContainerType::Map);
    let idx = arena.register(&id);

    assert_eq!(arena.get_id(idx), Some(id.clone()));
    assert_eq!(arena.get_idx(&id), Some(idx));
  }

  #[test]
  fn test_arena_deduplicate() {
    let arena = InnerArena::new();
    let id = ContainerID::new_root("my_list", ContainerType::List);
    let idx1 = arena.register(&id);
    let idx2 = arena.register(&id);
    assert_eq!(idx1, idx2);
  }

  #[test]
  fn test_arena_parent() {
    let arena = InnerArena::new();
    let child_id = ContainerID::new_root("child", ContainerType::Map);
    let parent_id = ContainerID::new_root("parent", ContainerType::List);
    let child = arena.register(&child_id);
    let parent = arena.register(&parent_id);

    arena.set_parent(child, Some(parent));
    assert_eq!(arena.get_parent(child), Some(parent));
  }

  #[test]
  fn test_arena_depth_computed() {
    let arena = InnerArena::new();
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
    let arena = InnerArena::new();
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
    let arena = InnerArena::new();
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

  #[test]
  fn test_arena_alloc_value_and_get() {
    let arena = InnerArena::new();
    let idx0 = arena.alloc_value(CoralValue::from("hello"));
    let idx1 = arena.alloc_value(CoralValue::from(42i32));
    let idx2 = arena.alloc_value(CoralValue::from(true));

    assert_eq!(idx0, 0);
    assert_eq!(idx1, 1);
    assert_eq!(idx2, 2);

    assert_eq!(arena.get_value(idx0), Some(CoralValue::from("hello")));
    assert_eq!(arena.get_value(idx1), Some(CoralValue::from(42i32)));
    assert_eq!(arena.get_value(idx2), Some(CoralValue::from(true)));
    assert_eq!(arena.get_value(99), None);
  }

  #[test]
  fn test_arena_alloc_str_and_get() {
    let arena = InnerArena::new();
    let r1 = arena.alloc_str("Hello");
    let r2 = arena.alloc_str("World");
    let r3 = arena.alloc_str("你好");

    assert_eq!(arena.get_str(&r1), Some("Hello".to_string()));
    assert_eq!(arena.get_str(&r2), Some("World".to_string()));
    assert_eq!(arena.get_str(&r3), Some("你好".to_string()));

    // Unicode length is tracked correctly.
    assert_eq!(r1.unicode_len, 5);
    assert_eq!(r2.unicode_len, 5);
    assert_eq!(r3.unicode_len, 2);
  }

  #[test]
  fn test_arena_fork_independence() {
    let original = InnerArena::new();
    let id = ContainerID::new_root("map", ContainerType::Map);
    let idx = original.register(&id);
    original.set_parent(idx, None);
    let v0 = original.alloc_value(CoralValue::from("original"));
    let s0 = original.alloc_str("hello");

    let forked = original.fork();

    // Forked sees the pre-fork state.
    assert_eq!(forked.get_id(idx), Some(id.clone()));
    assert_eq!(forked.get_parent(idx), None);
    assert_eq!(forked.get_value(v0), Some(CoralValue::from("original")));
    assert_eq!(forked.get_str(&s0), Some("hello".to_string()));

    // Modify original — forked must remain unchanged.
    original.set_parent(idx, Some(idx));
    original.alloc_value(CoralValue::from("extra"));
    original.alloc_str(" world");

    assert_eq!(forked.get_parent(idx), None);
    assert_eq!(forked.get_value(v0 + 1), None);
    assert_eq!(
      forked.get_str(&StrAllocResult {
        start_byte: s0.start_byte + s0.unicode_len + 1,
        unicode_len: 6,
      }),
      None
    );

    // Modify forked — original must remain unchanged.
    forked.alloc_value(CoralValue::from("forked"));
    assert_eq!(original.get_value(v0 + 1), Some(CoralValue::from("extra")));
  }

  #[test]
  fn test_shared_arena_clone_is_shallow() {
    let shared = SharedArena::new(InnerArena::new());
    let cloned = shared.clone();

    let id = ContainerID::new_root("list", ContainerType::List);
    let idx = shared.register(&id);

    // Clone sees the same data because it shares the inner Arc.
    assert_eq!(cloned.get_id(idx), Some(id));
  }

  #[test]
  fn test_shared_arena_fork_is_deep() {
    let shared = SharedArena::new(InnerArena::new());
    let id = ContainerID::new_root("text", ContainerType::Text);
    let idx = shared.register(&id);

    let forked = shared.fork();

    // Modify through the original shared handle.
    shared.set_parent(idx, Some(idx));

    // Forked must remain independent.
    assert_eq!(forked.get_parent(idx), None);
  }
}
