//! Version vectors and frontiers.
//!
//! [`VersionVector`] tracks how many operations from each peer are known.
//! [`Frontiers`] is the minimal set of leaf IDs that identifies a version.

mod frontiers;

pub use frontiers::Frontiers;

use crate::types::{Counter, ID, PeerID};
use rustc_hash::FxHashMap;
use std::cmp::Ordering;
use std::ops::{Deref, DerefMut};

/// A map from peer to the exclusive upper bound of known operations.
///
/// A `VersionVector` of `{A: 3, B: 5}` means we have seen operations
/// `A@0..3` and `B@0..5` (i.e. 3 ops from A, 5 ops from B).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VersionVector(FxHashMap<PeerID, Counter>);

impl VersionVector {
  /// Creates an empty version vector.
  pub fn new() -> Self {
    Self(FxHashMap::default())
  }

  /// Records that we have seen all operations up to and including `id`.
  pub fn set_last(&mut self, id: ID) {
    self.0.insert(id.peer, id.counter + 1);
  }

  /// Returns the exclusive end counter for a peer, if any.
  pub fn get(&self, peer: PeerID) -> Option<&Counter> {
    self.0.get(&peer)
  }

  /// Returns the inclusive last counter for a peer, if any and non-zero.
  pub fn get_last(&self, client_id: PeerID) -> Option<Counter> {
    self
      .0
      .get(&client_id)
      .and_then(|&x| if x == 0 { None } else { Some(x - 1) })
  }

  /// Sets the exclusive ending point. The target id will NOT be included by self.
  pub fn set_end(&mut self, id: ID) {
    if id.counter <= 0 {
      self.0.remove(&id.peer);
    } else {
      self.0.insert(id.peer, id.counter);
    }
  }

  /// Updates the end counter of the given client if the end is greater.
  ///
  /// Returns `true` if the vector was modified.
  pub fn try_update_last(&mut self, id: ID) -> bool {
    if let Some(end) = self.0.get_mut(&id.peer) {
      if *end < id.counter + 1 {
        *end = id.counter + 1;
        true
      } else {
        false
      }
    } else {
      self.0.insert(id.peer, id.counter + 1);
      true
    }
  }

  /// Extends this vector to include all entries from the given iterator.
  ///
  /// For each peer, the counter is updated only if the incoming value is
  /// greater. This is the primitive that `merge` is built on — it embodies
  /// the "some data cannot be merged" rule: entries that are already up-to-
  /// date (or ahead) are skipped.
  pub fn extend_to_include_vv<'a>(&mut self, vv: impl Iterator<Item = (&'a PeerID, &'a Counter)>) {
    for (&client_id, &counter) in vv {
      if let Some(my_counter) = self.get_mut(&client_id) {
        if *my_counter < counter {
          *my_counter = counter;
        }
      } else {
        self.0.insert(client_id, counter);
      }
    }
  }

  /// Extends this vector to include the given `id`.
  ///
  /// If the peer is already known with a counter **greater than** `id.counter`,
  /// this is a no-op — the id is already covered.
  pub fn extend_to_include_last_id(&mut self, id: ID) {
    if let Some(counter) = self.get_mut(&id.peer) {
      if *counter <= id.counter {
        *counter = id.counter + 1;
      }
    } else {
      self.set_last(id);
    }
  }

  /// Extends this vector to include the given end `id`.
  ///
  /// Like [`extend_to_include_last_id`], but treats `id.counter` as the
  /// exclusive end bound rather than the last included counter.
  pub fn extend_to_include_end_id(&mut self, id: ID) {
    if let Some(counter) = self.get_mut(&id.peer) {
      if *counter < id.counter {
        *counter = id.counter;
      }
    } else {
      self.set_end(id);
    }
  }

  /// Merges another version vector into this one (taking the max per peer).
  pub fn merge(&mut self, other: &Self) {
    self.extend_to_include_vv(other.iter());
  }

  /// Returns `true` if `self` includes every operation known by `other`.
  pub fn includes(&self, other: &Self) -> bool {
    other
      .0
      .iter()
      .all(|(peer, counter)| self.0.get(peer).is_some_and(|&c| c >= *counter))
  }

  /// Returns `true` if `self` includes the given operation id.
  pub fn includes_id(&self, id: ID) -> bool {
    if let Some(end) = self.get(id.peer)
      && *end > id.counter
    {
      return true;
    }
    false
  }

  /// Returns the [`Frontiers`] representation of this version vector.
  ///
  /// Each frontier ID is the *last* operation we know from that peer.
  pub fn get_frontiers(&self) -> Frontiers {
    self
      .0
      .iter()
      .filter_map(|(&peer, &counter)| {
        if counter > 0 {
          Some(ID::new(peer, counter - 1))
        } else {
          None
        }
      })
      .collect()
  }
}

impl Deref for VersionVector {
  type Target = FxHashMap<PeerID, Counter>;

  fn deref(&self) -> &Self::Target {
    &self.0
  }
}

impl DerefMut for VersionVector {
  fn deref_mut(&mut self) -> &mut Self::Target {
    &mut self.0
  }
}

impl From<FxHashMap<PeerID, Counter>> for VersionVector {
  fn from(map: FxHashMap<PeerID, Counter>) -> Self {
    Self(map)
  }
}

impl FromIterator<ID> for VersionVector {
  fn from_iter<T: IntoIterator<Item = ID>>(iter: T) -> Self {
    let mut vv = Self::new();
    for id in iter {
      vv.set_last(id);
    }
    vv
  }
}

impl PartialOrd for VersionVector {
  fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
    let mut self_greater = true;
    let mut other_greater = true;
    let mut eq = true;

    for (peer, &other_end) in other.0.iter() {
      match self.0.get(peer) {
        Some(&self_end) => {
          if self_end < other_end {
            self_greater = false;
            eq = false;
          }
          if self_end > other_end {
            other_greater = false;
            eq = false;
          }
        }
        None => {
          if other_end > 0 {
            self_greater = false;
            eq = false;
          }
        }
      }
    }

    for (peer, &self_end) in self.0.iter() {
      if !other.0.contains_key(peer) && self_end > 0 {
        other_greater = false;
        eq = false;
      }
    }

    if eq {
      Some(Ordering::Equal)
    } else if self_greater {
      Some(Ordering::Greater)
    } else if other_greater {
      Some(Ordering::Less)
    } else {
      None
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  // ── construction ─────────────────────────────────────────────────

  #[test]
  fn test_vv_new_is_empty() {
    let vv = VersionVector::new();
    assert!(vv.is_empty());
    assert_eq!(vv.len(), 0);
  }

  #[test]
  fn test_vv_default_is_empty() {
    let vv = VersionVector::default();
    assert!(vv.is_empty());
  }

  #[test]
  fn test_vv_from_iter() {
    let vv = VersionVector::from_iter([ID::new(1, 2), ID::new(2, 3)]);
    assert_eq!(vv.get(1).copied(), Some(3)); // exclusive end = counter + 1
    assert_eq!(vv.get(2).copied(), Some(4));
    assert!(vv.get(3).is_none());
  }

  #[test]
  fn test_vv_from_hashmap() {
    let mut map = FxHashMap::default();
    map.insert(1u64, 5);
    map.insert(2u64, 7);
    let vv = VersionVector::from(map);
    assert_eq!(vv.get(1).copied(), Some(5));
    assert_eq!(vv.get(2).copied(), Some(7));
  }

  // ── basic accessors ──────────────────────────────────────────────

  #[test]
  fn test_vv_set_last_and_get() {
    let mut vv = VersionVector::new();
    vv.set_last(ID::new(1, 10));
    assert_eq!(vv.get(1).copied(), Some(11)); // exclusive end
    vv.set_last(ID::new(1, 5)); // smaller, still overwrites
    assert_eq!(vv.get(1).copied(), Some(6));
  }

  #[test]
  fn test_vv_get_last() {
    let mut vv = VersionVector::new();
    assert!(vv.get_last(1).is_none());
    vv.set_last(ID::new(1, 10));
    assert_eq!(vv.get_last(1), Some(10));
    vv.set_end(ID::new(1, 0));
    assert!(vv.get_last(1).is_none());
  }

  #[test]
  fn test_vv_set_end() {
    let mut vv = VersionVector::new();
    vv.set_end(ID::new(1, 5));
    assert_eq!(vv.get(1).copied(), Some(5));
    // counter <= 0 removes the entry
    vv.set_end(ID::new(1, 0));
    assert!(vv.get(1).is_none());
  }

  #[test]
  fn test_vv_try_update_last() {
    let mut vv = VersionVector::new();
    // insert new
    assert!(vv.try_update_last(ID::new(1, 10)));
    assert_eq!(vv.get(1).copied(), Some(11));
    // larger id updates
    assert!(vv.try_update_last(ID::new(1, 15)));
    assert_eq!(vv.get(1).copied(), Some(16));
    // smaller id does not update
    assert!(!vv.try_update_last(ID::new(1, 5)));
    assert_eq!(vv.get(1).copied(), Some(16));
  }

  #[test]
  fn test_vv_deref_iter_and_contains_key() {
    let vv = VersionVector::from_iter([ID::new(1, 2), ID::new(2, 3)]);
    assert!(vv.contains_key(&1));
    assert!(!vv.contains_key(&3));
    let mut keys: Vec<_> = vv.iter().map(|(&k, _)| k).collect();
    keys.sort();
    assert_eq!(keys, vec![1, 2]);
  }

  // ── extend helpers ───────────────────────────────────────────────

  #[test]
  fn test_vv_extend_to_include_vv() {
    let mut a = VersionVector::from_iter([ID::new(1, 2)]);
    let b = VersionVector::from_iter([ID::new(1, 5), ID::new(2, 3)]);
    a.extend_to_include_vv(b.iter());
    assert_eq!(a.get(1).copied(), Some(6)); // max(3, 6)
    assert_eq!(a.get(2).copied(), Some(4));
  }

  #[test]
  fn test_vv_extend_to_include_last_id_already_covered() {
    let mut vv = VersionVector::from_iter([ID::new(1, 10)]);
    vv.extend_to_include_last_id(ID::new(1, 5)); // already covered
    assert_eq!(vv.get(1).copied(), Some(11));
  }

  #[test]
  fn test_vv_extend_to_include_last_id_extends() {
    let mut vv = VersionVector::from_iter([ID::new(1, 5)]);
    vv.extend_to_include_last_id(ID::new(1, 8));
    assert_eq!(vv.get(1).copied(), Some(9));
  }

  #[test]
  fn test_vv_extend_to_include_end_id_already_covered() {
    let mut vv = VersionVector::from_iter([ID::new(1, 10)]);
    vv.extend_to_include_end_id(ID::new(1, 5)); // end=5 < 11, no-op
    assert_eq!(vv.get(1).copied(), Some(11));
  }

  #[test]
  fn test_vv_extend_to_include_end_id_extends() {
    let mut vv = VersionVector::from_iter([ID::new(1, 5)]);
    vv.extend_to_include_end_id(ID::new(1, 12));
    assert_eq!(vv.get(1).copied(), Some(12));
  }

  // ── merge ────────────────────────────────────────────────────────

  #[test]
  fn test_vv_merge_takes_max() {
    let mut a = VersionVector::from_iter([ID::new(1, 2), ID::new(2, 3)]);
    let b = VersionVector::from_iter([ID::new(1, 5), ID::new(3, 1)]);
    a.merge(&b);
    assert_eq!(a.get(1).copied(), Some(6));
    assert_eq!(a.get(2).copied(), Some(4));
    assert_eq!(a.get(3).copied(), Some(2));
  }

  #[test]
  fn test_vv_merge_skips_when_self_is_ahead() {
    let mut a = VersionVector::from_iter([ID::new(1, 10)]);
    let b = VersionVector::from_iter([ID::new(1, 5)]);
    a.merge(&b);
    assert_eq!(a.get(1).copied(), Some(11)); // unchanged
  }

  #[test]
  fn test_vv_merge_with_empty() {
    let mut a = VersionVector::from_iter([ID::new(1, 5)]);
    let b = VersionVector::new();
    a.merge(&b);
    assert_eq!(a.get(1).copied(), Some(6));
  }

  #[test]
  fn test_vv_merge_into_empty() {
    let mut a = VersionVector::new();
    let b = VersionVector::from_iter([ID::new(1, 5)]);
    a.merge(&b);
    assert_eq!(a.get(1).copied(), Some(6));
  }

  // ── includes / includes_id ───────────────────────────────────────

  #[test]
  fn test_vv_includes() {
    let a = VersionVector::from_iter([ID::new(1, 5), ID::new(2, 3)]);
    let b = VersionVector::from_iter([ID::new(1, 2), ID::new(2, 3)]);
    assert!(a.includes(&b));
    assert!(!b.includes(&a));
  }

  #[test]
  fn test_vv_includes_empty() {
    let a = VersionVector::from_iter([ID::new(1, 5)]);
    let b = VersionVector::new();
    assert!(a.includes(&b));
  }

  #[test]
  fn test_vv_includes_id() {
    let vv = VersionVector::from_iter([ID::new(1, 5)]);
    assert!(vv.includes_id(ID::new(1, 4))); // included
    assert!(vv.includes_id(ID::new(1, 0))); // included
    assert!(vv.includes_id(ID::new(1, 5))); // included (last op)
    assert!(!vv.includes_id(ID::new(1, 6))); // not included (beyond last)
    assert!(!vv.includes_id(ID::new(2, 0))); // peer not known
  }

  // ── frontiers ────────────────────────────────────────────────────

  #[test]
  fn test_vv_get_frontiers() {
    let vv = VersionVector::from_iter([ID::new(1, 5), ID::new(2, 3)]);
    let frontiers = vv.get_frontiers();
    assert_eq!(frontiers.len(), 2);
    let ids: Vec<_> = frontiers.iter().collect();
    assert!(ids.contains(&ID::new(1, 5)));
    assert!(ids.contains(&ID::new(2, 3)));
  }

  #[test]
  fn test_vv_get_frontiers_excludes_zero() {
    let mut vv = VersionVector::new();
    vv.set_end(ID::new(1, 0)); // counter=0 entry should not appear
    let frontiers = vv.get_frontiers();
    assert!(frontiers.is_empty());
  }

  // ── partial_ord ──────────────────────────────────────────────────

  #[test]
  fn test_vv_partial_ord_greater() {
    let a = VersionVector::from_iter([ID::new(1, 2), ID::new(2, 3)]);
    let b = VersionVector::from_iter([ID::new(1, 1), ID::new(2, 2)]);
    assert_eq!(a.partial_cmp(&b), Some(Ordering::Greater));
    assert_eq!(b.partial_cmp(&a), Some(Ordering::Less));
  }

  #[test]
  fn test_vv_partial_ord_equal() {
    let a = VersionVector::from_iter([ID::new(1, 2), ID::new(2, 3)]);
    let b = VersionVector::from_iter([ID::new(1, 2), ID::new(2, 3)]);
    assert_eq!(a.partial_cmp(&b), Some(Ordering::Equal));
  }

  #[test]
  fn test_vv_partial_ord_concurrent() {
    let a = VersionVector::from_iter([ID::new(1, 3), ID::new(2, 1)]);
    let b = VersionVector::from_iter([ID::new(1, 2), ID::new(2, 3)]);
    assert_eq!(a.partial_cmp(&b), None);
  }

  #[test]
  fn test_vv_partial_ord_one_empty() {
    let a = VersionVector::from_iter([ID::new(1, 2)]);
    let b = VersionVector::new();
    assert_eq!(a.partial_cmp(&b), Some(Ordering::Greater));
    assert_eq!(b.partial_cmp(&a), Some(Ordering::Less));
  }

  #[test]
  fn test_vv_partial_ord_zero_entries_equal() {
    // entries with counter=0 should be treated as absent
    let mut a = VersionVector::new();
    a.insert(1, 0);
    let b = VersionVector::new();
    assert_eq!(a.partial_cmp(&b), Some(Ordering::Equal));
  }

  // ── edge cases ───────────────────────────────────────────────────

  #[test]
  fn test_vv_merge_self() {
    let mut a = VersionVector::from_iter([ID::new(1, 5)]);
    let a_clone = a.clone();
    a.merge(&a_clone);
    assert_eq!(a.get(1).copied(), Some(6));
  }

  #[test]
  fn test_vv_includes_self() {
    let a = VersionVector::from_iter([ID::new(1, 5)]);
    assert!(a.includes(&a));
  }

  #[test]
  fn test_vv_empty_includes_empty() {
    let a = VersionVector::new();
    let b = VersionVector::new();
    assert!(a.includes(&b));
    assert_eq!(a.partial_cmp(&b), Some(Ordering::Equal));
  }
}
