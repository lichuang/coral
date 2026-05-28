//! Minimal set of leaf IDs that identifies a document version.
//!
//! See [`Heads`] for details.
//!
//! This implementation uses a three-state enum design:
//! - [`Heads::Empty`] — empty document (zero allocation)
//! - [`Heads::Linear`] — linear history, exactly one tip (most common, zero allocation)
//! - [`Heads::Concurrent`] — concurrent edits with multiple tips (Arc-shared HashMap)

use crate::types::{Counter, OpId, PeerID};
use rustc_hash::FxHashMap;
use std::collections::hash_map;
use std::sync::Arc;

/// The minimal set of leaf IDs that identifies a document version.
///
/// When history is linear, there is exactly one head OpId.
/// When there are concurrent edits, there may be multiple.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Heads {
  /// Empty heads (root / no operations).
  #[default]
  Empty,
  /// Exactly one head tip — the common case for linear history.
  Linear(OpId),
  /// Multiple concurrent tips (always ≥ 2 peers).
  ///
  /// Stored as `Arc<FxHashMap<PeerID, Counter>>` so that cloning a
  /// [`Heads`] in the multi-peer case is O(1).
  Concurrent(Arc<FxHashMap<PeerID, Counter>>),
}

impl Heads {
  /// Creates empty heads.
  #[inline]
  pub fn new() -> Self {
    Self::Empty
  }

  /// Creates heads containing a single OpId.
  #[inline]
  pub fn from_id(id: OpId) -> Self {
    Self::Linear(id)
  }

  /// Number of head IDs.
  pub fn len(&self) -> usize {
    match self {
      Self::Empty => 0,
      Self::Linear(_) => 1,
      Self::Concurrent(m) => m.len(),
    }
  }

  /// Returns `true` if there are no head IDs.
  #[inline]
  pub fn is_empty(&self) -> bool {
    matches!(self, Self::Empty)
  }

  /// Returns the single OpId if this heads contains exactly one.
  pub fn as_single(&self) -> Option<OpId> {
    match self {
      Self::Linear(id) => Some(*id),
      _ => None,
    }
  }

  /// Returns `true` if the heads contains the given OpId.
  ///
  /// For the [`Concurrent`] variant, an OpId is considered present if its peer
  /// exists in the map and the stored counter matches exactly.
  pub fn contains(&self, id: &OpId) -> bool {
    match self {
      Self::Empty => false,
      Self::Linear(i) => i == id,
      Self::Concurrent(m) => m.get(&id.peer).copied() == Some(id.counter),
    }
  }

  /// Adds a new leaf OpId, replacing any existing OpId from the same peer
  /// with a smaller counter.
  ///
  /// If the new OpId introduces a second peer, the heads is
  /// automatically promoted from [`Linear`] to [`Concurrent`].
  pub fn push(&mut self, id: OpId) {
    match self {
      Self::Empty => {
        *self = Self::Linear(id);
      }
      Self::Linear(old) => {
        if old.peer == id.peer {
          // Same peer: keep the larger counter.
          if old.counter < id.counter {
            *old = id;
          }
        } else {
          // Different peer: promote to Map.
          let mut map = FxHashMap::default();
          map.insert(old.peer, old.counter);
          map.insert(id.peer, id.counter);
          *self = Self::Concurrent(Arc::new(map));
        }
      }
      Self::Concurrent(map) => {
        let map = Arc::make_mut(map);
        match map.entry(id.peer) {
          hash_map::Entry::Occupied(mut entry) => {
            if *entry.get() < id.counter {
              entry.insert(id.counter);
            }
          }
          hash_map::Entry::Vacant(entry) => {
            entry.insert(id.counter);
          }
        }
      }
    }
  }

  /// Removes the given OpId from the heads.
  ///
  /// If removing the OpId leaves exactly one peer, the heads is
  /// automatically demoted from [`Concurrent`] to [`Linear`].
  /// If the heads becomes empty, it is demoted to [`Empty`].
  pub fn remove(&mut self, id: &OpId) {
    match self {
      Self::Empty => {}
      Self::Linear(old) => {
        if old == id {
          *self = Self::Empty;
        }
      }
      Self::Concurrent(map) => {
        let map = Arc::make_mut(map);
        if let Some(counter) = map.get_mut(&id.peer)
          && *counter == id.counter
        {
          map.remove(&id.peer);
        }
        match map.len() {
          0 => *self = Self::Empty,
          1 => {
            let (&peer, &counter) = map.iter().next().unwrap();
            *self = Self::Linear(OpId::new(peer, counter));
          }
          _ => {}
        }
      }
    }
  }
}

impl From<Vec<OpId>> for Heads {
  fn from(ids: Vec<OpId>) -> Self {
    let mut heads = Heads::new();
    for id in ids {
      heads.push(id);
    }
    heads
  }
}

impl FromIterator<OpId> for Heads {
  fn from_iter<I: IntoIterator<Item = OpId>>(iter: I) -> Self {
    let mut heads = Heads::new();
    for id in iter {
      heads.push(id);
    }
    heads
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_heads_empty() {
    let f = Heads::new();
    assert!(f.is_empty());
    assert_eq!(f.len(), 0);
    assert_eq!(f.as_single(), None);
    assert!(!f.contains(&OpId::new(1, 0)));
    assert!(matches!(f, Heads::Empty));
  }

  #[test]
  fn test_heads_from_id() {
    let f = Heads::from_id(OpId::new(1, 10));
    assert!(!f.is_empty());
    assert_eq!(f.len(), 1);
    assert_eq!(f.as_single(), Some(OpId::new(1, 10)));
    assert!(f.contains(&OpId::new(1, 10)));
    assert!(!f.contains(&OpId::new(1, 11)));
    assert!(matches!(f, Heads::Linear(_)));
  }

  #[test]
  fn test_heads_push_same_peer_upgrades_counter() {
    let mut f = Heads::from_id(OpId::new(1, 5));
    f.push(OpId::new(1, 10));
    assert_eq!(f.as_single(), Some(OpId::new(1, 10)));

    // Smaller counter is ignored.
    f.push(OpId::new(1, 7));
    assert_eq!(f.as_single(), Some(OpId::new(1, 10)));
    assert!(matches!(f, Heads::Linear(_)));
  }

  #[test]
  fn test_heads_push_different_peer_promotes_to_concurrent() {
    let mut f = Heads::from_id(OpId::new(1, 5));
    f.push(OpId::new(2, 3));
    assert!(matches!(f, Heads::Concurrent(_)));
    assert_eq!(f.len(), 2);
    assert_eq!(f.as_single(), None);
    assert!(f.contains(&OpId::new(1, 5)));
    assert!(f.contains(&OpId::new(2, 3)));
  }

  #[test]
  fn test_heads_remove_demotes_to_linear() {
    let mut f = Heads::from(vec![OpId::new(1, 5), OpId::new(2, 3)]);
    f.remove(&OpId::new(1, 5));
    assert!(matches!(f, Heads::Linear(_)));
    assert_eq!(f.as_single(), Some(OpId::new(2, 3)));
  }

  #[test]
  fn test_heads_remove_demotes_to_empty() {
    let mut f = Heads::from_id(OpId::new(1, 5));
    f.remove(&OpId::new(1, 5));
    assert!(matches!(f, Heads::Empty));
    assert!(f.is_empty());
  }

  #[test]
  fn test_heads_remove_no_op_when_counter_mismatch() {
    let mut f = Heads::from(vec![OpId::new(1, 5), OpId::new(2, 3)]);
    f.remove(&OpId::new(1, 4));
    assert_eq!(f.len(), 2);
    assert!(f.contains(&OpId::new(1, 5)));
    assert!(f.contains(&OpId::new(2, 3)));
  }

  #[test]
  fn test_heads_from_vec() {
    let f = Heads::from(vec![OpId::new(1, 10), OpId::new(2, 5)]);
    assert_eq!(f.len(), 2);
    assert!(f.contains(&OpId::new(1, 10)));
    assert!(f.contains(&OpId::new(2, 5)));
    assert!(matches!(f, Heads::Concurrent(_)));
  }

  #[test]
  fn test_heads_from_iterator() {
    let f: Heads = [OpId::new(1, 1), OpId::new(2, 2)].iter().copied().collect();
    assert_eq!(f.len(), 2);
    assert!(matches!(f, Heads::Concurrent(_)));
  }

  #[test]
  fn test_heads_clone_is_cheap_for_concurrent() {
    let f = Heads::from(vec![OpId::new(1, 1), OpId::new(2, 2), OpId::new(3, 3)]);
    let cloned = f.clone();
    // Both Concurrent variants share the same Arc allocation.
    if let (Heads::Concurrent(a), Heads::Concurrent(b)) = (&f, &cloned) {
      assert!(Arc::ptr_eq(a, b));
    }
  }

  #[test]
  fn test_heads_contains_exact_match() {
    let f = Heads::from_id(OpId::new(1, 5));
    assert!(f.contains(&OpId::new(1, 5)));
    assert!(!f.contains(&OpId::new(1, 4)));
    assert!(!f.contains(&OpId::new(1, 6)));
    assert!(!f.contains(&OpId::new(2, 0)));
  }

  #[test]
  fn test_heads_contains_concurrent_exact_match() {
    let f = Heads::from(vec![OpId::new(1, 5), OpId::new(2, 3)]);
    assert!(f.contains(&OpId::new(1, 5)));
    assert!(f.contains(&OpId::new(2, 3)));
    assert!(!f.contains(&OpId::new(1, 4)));
    assert!(!f.contains(&OpId::new(1, 6)));
    assert!(!f.contains(&OpId::new(3, 0)));
  }

  #[test]
  fn test_heads_default_is_empty() {
    let f: Heads = Default::default();
    assert!(f.is_empty());
    assert_eq!(f.len(), 0);
    assert!(matches!(f, Heads::Empty));
  }
}
