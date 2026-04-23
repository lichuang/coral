//! Minimal set of leaf IDs that identifies a document version.
//!
//! See [`Frontiers`] for details.
//!
//! This implementation aligns with Loro's three-state enum design:
//! - [`Frontiers::None`] — empty document (zero allocation)
//! - [`Frontiers::ID`] — linear history, exactly one tip (most common, zero allocation)
//! - [`Frontiers::Map`] — concurrent edits with multiple tips (Arc-shared HashMap)

use crate::types::{Counter, ID, PeerID};
use rustc_hash::FxHashMap;
use std::collections::hash_map;
use std::sync::Arc;

/// The minimal set of leaf IDs that identifies a document version.
///
/// When history is linear, there is exactly one frontier ID.
/// When there are concurrent edits, there may be multiple.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Frontiers {
  /// Empty frontiers (root / no operations).
  #[default]
  None,
  /// Exactly one frontier tip — the common case for linear history.
  ID(ID),
  /// Multiple concurrent tips (always ≥ 2 peers).
  ///
  /// Stored as `Arc<FxHashMap<PeerID, Counter>>` so that cloning a
  /// [`Frontiers`] in the multi-peer case is O(1).
  Map(Arc<FxHashMap<PeerID, Counter>>),
}

impl Frontiers {
  /// Creates empty frontiers.
  #[inline]
  pub fn new() -> Self {
    Self::None
  }

  /// Creates frontiers containing a single ID.
  #[inline]
  pub fn from_id(id: ID) -> Self {
    Self::ID(id)
  }

  /// Number of frontier IDs.
  pub fn len(&self) -> usize {
    match self {
      Self::None => 0,
      Self::ID(_) => 1,
      Self::Map(m) => m.len(),
    }
  }

  /// Returns `true` if there are no frontier IDs.
  #[inline]
  pub fn is_empty(&self) -> bool {
    matches!(self, Self::None)
  }

  /// Returns the single ID if this frontiers contains exactly one.
  pub fn as_single(&self) -> Option<ID> {
    match self {
      Self::ID(id) => Some(*id),
      _ => None,
    }
  }

  /// Returns `true` if the frontiers contains the given ID.
  ///
  /// For the [`Map`] variant, an ID is considered present if its peer
  /// exists in the map and the stored counter matches exactly.
  pub fn contains(&self, id: &ID) -> bool {
    match self {
      Self::None => false,
      Self::ID(i) => i == id,
      Self::Map(m) => m.get(&id.peer).copied() == Some(id.counter),
    }
  }

  /// Iterates over the frontier IDs.
  #[inline]
  pub fn iter(&self) -> FrontiersIter<'_> {
    FrontiersIter::new(self)
  }

  /// Adds a new leaf ID, replacing any existing ID from the same peer
  /// with a smaller counter.
  ///
  /// If the new ID introduces a second peer, the frontiers is
  /// automatically promoted from [`ID`] to [`Map`].
  pub fn push(&mut self, id: ID) {
    match self {
      Self::None => {
        *self = Self::ID(id);
      }
      Self::ID(old) => {
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
          *self = Self::Map(Arc::new(map));
        }
      }
      Self::Map(map) => {
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

  /// Removes the given ID from the frontiers.
  ///
  /// If removing the ID leaves exactly one peer, the frontiers is
  /// automatically demoted from [`Map`] to [`ID`].
  /// If the frontiers becomes empty, it is demoted to [`None`].
  pub fn remove(&mut self, id: &ID) {
    match self {
      Self::None => {}
      Self::ID(old) => {
        if old == id {
          *self = Self::None;
        }
      }
      Self::Map(map) => {
        let map = Arc::make_mut(map);
        if let Some(counter) = map.get_mut(&id.peer)
          && *counter == id.counter
        {
          map.remove(&id.peer);
        }
        match map.len() {
          0 => *self = Self::None,
          1 => {
            let (&peer, &counter) = map.iter().next().unwrap();
            *self = Self::ID(ID::new(peer, counter));
          }
          _ => {}
        }
      }
    }
  }

  /// Retains only the IDs satisfying the predicate.
  ///
  /// Automatically demotes [`Map`] → [`ID`] → [`None`] as peers are removed.
  pub fn retain<F>(&mut self, mut f: F)
  where
    F: FnMut(&ID) -> bool,
  {
    match self {
      Self::None => {}
      Self::ID(id) => {
        if !f(id) {
          *self = Self::None;
        }
      }
      Self::Map(map) => {
        let map = Arc::make_mut(map);
        map.retain(|&peer, _counter| f(&ID::new(peer, *_counter)));
        match map.len() {
          0 => *self = Self::None,
          1 => {
            let (&peer, &counter) = map.iter().next().unwrap();
            *self = Self::ID(ID::new(peer, counter));
          }
          _ => {}
        }
      }
    }
  }

  /// Update frontiers when a new change is added.
  ///
  /// Removes all dependency IDs from the frontiers (they now have
  /// successors), then adds the new change's last ID.
  pub fn update_frontiers_on_new_change(&mut self, id: ID, deps: &Frontiers) {
    if self.len() <= 8 && self == deps {
      *self = Frontiers::from_id(id);
      return;
    }

    for dep in deps.iter() {
      self.remove(&dep);
    }
    self.push(id);
  }

  /// Merges another frontiers into this one, taking the maximum counter
  /// per peer.
  pub fn merge_with_greater(&mut self, other: &Frontiers) {
    for id in other.iter() {
      self.push(id);
    }
  }
}

impl From<Vec<ID>> for Frontiers {
  fn from(ids: Vec<ID>) -> Self {
    let mut frontiers = Frontiers::new();
    for id in ids {
      frontiers.push(id);
    }
    frontiers
  }
}

impl FromIterator<ID> for Frontiers {
  fn from_iter<I: IntoIterator<Item = ID>>(iter: I) -> Self {
    let mut frontiers = Frontiers::new();
    for id in iter {
      frontiers.push(id);
    }
    frontiers
  }
}

// ---------------------------------------------------------------------------
// Iterator
// ---------------------------------------------------------------------------

/// Zero-allocation iterator over frontier IDs.
pub struct FrontiersIter<'a> {
  inner: FrontiersIterInner<'a>,
}

enum FrontiersIterInner<'a> {
  Empty,
  Single(Option<ID>),
  Map(hash_map::Iter<'a, PeerID, Counter>),
}

impl<'a> FrontiersIter<'a> {
  fn new(frontiers: &'a Frontiers) -> Self {
    let inner = match frontiers {
      Frontiers::None => FrontiersIterInner::Empty,
      Frontiers::ID(id) => FrontiersIterInner::Single(Some(*id)),
      Frontiers::Map(map) => FrontiersIterInner::Map(map.iter()),
    };
    Self { inner }
  }
}

impl Iterator for FrontiersIter<'_> {
  type Item = ID;

  fn next(&mut self) -> Option<ID> {
    match &mut self.inner {
      FrontiersIterInner::Empty => None,
      FrontiersIterInner::Single(id) => id.take(),
      FrontiersIterInner::Map(iter) => iter.next().map(|(&peer, &counter)| ID::new(peer, counter)),
    }
  }

  fn size_hint(&self) -> (usize, Option<usize>) {
    let len = match &self.inner {
      FrontiersIterInner::Empty => 0,
      FrontiersIterInner::Single(id) => id.is_some() as usize,
      FrontiersIterInner::Map(iter) => iter.len(),
    };
    (len, Some(len))
  }
}

impl ExactSizeIterator for FrontiersIter<'_> {}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_frontiers_none() {
    let f = Frontiers::new();
    assert!(f.is_empty());
    assert_eq!(f.len(), 0);
    assert_eq!(f.as_single(), None);
    assert_eq!(f.iter().count(), 0);
  }

  #[test]
  fn test_frontiers_from_id() {
    let f = Frontiers::from_id(ID::new(1, 10));
    assert!(!f.is_empty());
    assert_eq!(f.len(), 1);
    assert_eq!(f.as_single(), Some(ID::new(1, 10)));
    assert!(f.contains(&ID::new(1, 10)));
    assert!(!f.contains(&ID::new(1, 11)));
  }

  #[test]
  fn test_frontiers_push_same_peer_upgrades_counter() {
    let mut f = Frontiers::from_id(ID::new(1, 5));
    f.push(ID::new(1, 10));
    assert_eq!(f.as_single(), Some(ID::new(1, 10)));

    // Smaller counter is ignored.
    f.push(ID::new(1, 7));
    assert_eq!(f.as_single(), Some(ID::new(1, 10)));
  }

  #[test]
  fn test_frontiers_push_different_peer_promotes_to_map() {
    let mut f = Frontiers::from_id(ID::new(1, 5));
    f.push(ID::new(2, 3));
    assert!(matches!(f, Frontiers::Map(_)));
    assert_eq!(f.len(), 2);
    assert_eq!(f.as_single(), None);
    assert!(f.contains(&ID::new(1, 5)));
    assert!(f.contains(&ID::new(2, 3)));
  }

  #[test]
  fn test_frontiers_remove_demotes_to_id() {
    let mut f = Frontiers::from(vec![ID::new(1, 5), ID::new(2, 3)]);
    f.remove(&ID::new(1, 5));
    assert!(matches!(f, Frontiers::ID(_)));
    assert_eq!(f.as_single(), Some(ID::new(2, 3)));
  }

  #[test]
  fn test_frontiers_remove_demotes_to_none() {
    let mut f = Frontiers::from_id(ID::new(1, 5));
    f.remove(&ID::new(1, 5));
    assert!(matches!(f, Frontiers::None));
  }

  #[test]
  fn test_frontiers_retain() {
    let mut f = Frontiers::from(vec![ID::new(1, 5), ID::new(2, 3), ID::new(3, 7)]);
    f.retain(|id| id.peer != 2);
    assert_eq!(f.len(), 2);
    assert!(f.contains(&ID::new(1, 5)));
    assert!(f.contains(&ID::new(3, 7)));
  }

  #[test]
  fn test_frontiers_retain_demotes() {
    let mut f = Frontiers::from(vec![ID::new(1, 5), ID::new(2, 3)]);
    f.retain(|id| id.peer == 2);
    assert!(matches!(f, Frontiers::ID(_)));
    assert_eq!(f.as_single(), Some(ID::new(2, 3)));
  }

  #[test]
  fn test_frontiers_update_on_new_change_linear() {
    let mut f = Frontiers::from_id(ID::new(1, 5));
    let deps = Frontiers::from_id(ID::new(1, 5));
    f.update_frontiers_on_new_change(ID::new(1, 7), &deps);
    assert_eq!(f.as_single(), Some(ID::new(1, 7)));
  }

  #[test]
  fn test_frontiers_update_on_new_change_concurrent() {
    // Peer 1 at counter 5, peer 2 at counter 3 — concurrent tips.
    let mut f = Frontiers::from(vec![ID::new(1, 5), ID::new(2, 3)]);
    // Peer 1 makes a new change depending on its own last op.
    let deps = Frontiers::from_id(ID::new(1, 5));
    f.update_frontiers_on_new_change(ID::new(1, 7), &deps);
    // Peer 1's tip moves forward; peer 2 stays.
    assert_eq!(f.len(), 2);
    assert!(f.contains(&ID::new(1, 7)));
    assert!(f.contains(&ID::new(2, 3)));
  }

  #[test]
  fn test_frontiers_merge_with_greater() {
    let mut a = Frontiers::from(vec![ID::new(1, 5), ID::new(2, 3)]);
    let b = Frontiers::from(vec![ID::new(1, 10), ID::new(3, 7)]);
    a.merge_with_greater(&b);
    assert_eq!(a.len(), 3);
    assert!(a.contains(&ID::new(1, 10))); // upgraded
    assert!(a.contains(&ID::new(2, 3)));
    assert!(a.contains(&ID::new(3, 7)));
  }

  #[test]
  fn test_frontiers_from_vec() {
    let f = Frontiers::from(vec![ID::new(1, 10), ID::new(2, 5)]);
    assert_eq!(f.len(), 2);
    let ids: Vec<_> = f.iter().collect();
    assert_eq!(ids.len(), 2);
    assert!(ids.contains(&ID::new(1, 10)));
    assert!(ids.contains(&ID::new(2, 5)));
  }

  #[test]
  fn test_frontiers_from_iterator() {
    let f: Frontiers = [ID::new(1, 1), ID::new(2, 2)].iter().copied().collect();
    assert_eq!(f.len(), 2);
  }

  #[test]
  fn test_frontiers_clone_is_cheap_for_map() {
    let f = Frontiers::from(vec![ID::new(1, 1), ID::new(2, 2), ID::new(3, 3)]);
    let cloned = f.clone();
    // Both Map variants share the same Arc allocation.
    if let (Frontiers::Map(a), Frontiers::Map(b)) = (&f, &cloned) {
      assert!(Arc::ptr_eq(a, b));
    }
  }

  #[test]
  fn test_frontiers_iter_exact_size() {
    let f = Frontiers::from(vec![ID::new(1, 1), ID::new(2, 2)]);
    let iter = f.iter();
    assert_eq!(iter.len(), 2);
  }

  #[test]
  fn test_frontiers_does_not_over_remove_on_update() {
    // Regression test for the old Vec-based implementation bug:
    // when updating with a new change from peer 1, it used to
    // remove *all* peer-1 IDs with counter < last_id, not just
    // the ancestors in deps.
    let mut f = Frontiers::from(vec![ID::new(1, 5), ID::new(2, 3)]);
    let deps = Frontiers::from_id(ID::new(1, 5));
    f.update_frontiers_on_new_change(ID::new(1, 7), &deps);
    // Peer 2 should NOT be affected.
    assert!(f.contains(&ID::new(2, 3)));
  }
}
