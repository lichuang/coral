//! Version vectors and frontiers.
//!
//! [`VersionVector`] tracks how many operations from each peer are known.
//! [`Frontiers`] is the minimal set of leaf IDs that identifies a version.

mod diff;
mod frontiers;
mod span;

pub use diff::{IdSpanVector, VersionVectorDiff};
pub use frontiers::Frontiers;
pub use span::{CounterSpan, IdSpan};

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

  /// Convert to an immutable (structurally-shared) version vector.
  pub fn to_im_vv(&self) -> ImVersionVector {
    ImVersionVector(self.0.iter().map(|(&k, &v)| (k, v)).collect())
  }

  /// Convert from an immutable version vector.
  pub fn from_im_vv(im_vv: &ImVersionVector) -> Self {
    VersionVector(im_vv.0.iter().map(|(&k, &v)| (k, v)).collect())
  }

  /// Compute the difference between two version vectors.
  ///
  /// - `retreat`: spans in `self` but not in `rhs`.
  /// - `forward`: spans in `rhs` but not in `self`.
  pub fn diff(&self, rhs: &Self) -> VersionVectorDiff {
    let mut ans: VersionVectorDiff = Default::default();
    for (client_id, &counter) in self.iter() {
      if let Some(&rhs_counter) = rhs.get(*client_id) {
        match counter.cmp(&rhs_counter) {
          Ordering::Less => {
            ans.forward.insert(
              *client_id,
              CounterSpan {
                start: counter,
                end: rhs_counter,
              },
            );
          }
          Ordering::Greater => {
            ans.retreat.insert(
              *client_id,
              CounterSpan {
                start: rhs_counter,
                end: counter,
              },
            );
          }
          Ordering::Equal => {}
        }
      } else {
        ans.retreat.insert(
          *client_id,
          CounterSpan {
            start: 0,
            end: counter,
          },
        );
      }
    }
    for (client_id, &rhs_counter) in rhs.iter() {
      if !self.contains_key(client_id) {
        ans.forward.insert(
          *client_id,
          CounterSpan {
            start: 0,
            end: rhs_counter,
          },
        );
      }
    }
    ans
  }

  /// Returns two iterators covering the difference between two version vectors.
  ///
  /// - First iterator: spans in `self` but not in `rhs`.
  /// - Second iterator: spans in `rhs` but not in `self`.
  pub fn diff_iter<'a>(
    &'a self,
    rhs: &'a Self,
  ) -> (
    impl Iterator<Item = IdSpan> + 'a,
    impl Iterator<Item = IdSpan> + 'a,
  ) {
    (self.sub_iter(rhs), rhs.sub_iter(self))
  }

  /// Returns spans that are in `self` but not in `rhs`.
  pub fn sub_iter<'a>(&'a self, rhs: &'a Self) -> impl Iterator<Item = IdSpan> + 'a {
    self.iter().filter_map(move |(peer, &counter)| {
      if let Some(&rhs_counter) = rhs.get(*peer) {
        if counter > rhs_counter {
          Some(IdSpan {
            peer: *peer,
            counter: CounterSpan {
              start: rhs_counter,
              end: counter,
            },
          })
        } else {
          None
        }
      } else if counter > 0 {
        Some(IdSpan {
          peer: *peer,
          counter: CounterSpan {
            start: 0,
            end: counter,
          },
        })
      } else {
        None
      }
    })
  }

  /// Iterate over all spans that differ between `self` and `other` (both directions).
  pub fn iter_between<'a>(&'a self, other: &'a Self) -> impl Iterator<Item = IdSpan> + 'a {
    self.sub_iter(other).chain(other.sub_iter(self))
  }

  /// Returns the difference as an [`IdSpanVector`].
  pub fn sub_vec(&self, rhs: &Self) -> IdSpanVector {
    self.sub_iter(rhs).map(|x| (x.peer, x.counter)).collect()
  }

  /// Returns the total number of operations that differ between the two vectors.
  pub fn distance_between(&self, other: &Self) -> usize {
    let mut ans: i32 = 0;
    for (client_id, &counter) in self.iter() {
      if let Some(&other_counter) = other.get(*client_id) {
        ans += (counter - other_counter).abs();
      } else if counter > 0 {
        ans += counter;
      }
    }
    for (client_id, &counter) in other.iter() {
      if !self.contains_key(client_id) {
        ans += counter;
      }
    }
    ans as usize
  }

  /// Extend this vector to include the given span.
  pub fn extend_to_include(&mut self, span: IdSpan) {
    if let Some(counter) = self.get_mut(&span.peer) {
      if *counter < span.counter.end {
        *counter = span.counter.end;
      }
    } else {
      self.insert(span.peer, span.counter.end);
    }
  }

  /// Shrink this vector to exclude the given span.
  pub fn shrink_to_exclude(&mut self, span: IdSpan) {
    if span.counter.start == 0 {
      self.remove(&span.peer);
      return;
    }
    if let Some(counter) = self.get_mut(&span.peer)
      && *counter > span.counter.start
    {
      *counter = span.counter.start;
    }
  }

  /// Apply a set of forward spans.
  pub fn forward(&mut self, spans: &IdSpanVector) {
    for (&peer, &span) in spans.iter() {
      self.extend_to_include(IdSpan {
        peer,
        counter: span,
      });
    }
  }

  /// Apply a set of retreat spans.
  pub fn retreat(&mut self, spans: &IdSpanVector) {
    for (&peer, &span) in spans.iter() {
      self.shrink_to_exclude(IdSpan {
        peer,
        counter: span,
      });
    }
  }

  /// Returns the intersection of two version vectors.
  ///
  /// For each peer, the smaller counter is kept (as long as it is non-zero).
  pub fn intersection(&self, other: &Self) -> Self {
    let mut ans = Self::new();
    for (client_id, &counter) in self.iter() {
      if let Some(&other_counter) = other.get(*client_id) {
        if counter < other_counter {
          if counter != 0 {
            ans.insert(*client_id, counter);
          }
        } else if other_counter != 0 {
          ans.insert(*client_id, other_counter);
        }
      }
    }
    ans
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

/// Immutable version vector backed by a structurally-shared hash map.
///
/// The "immutable" here refers to the *underlying data structure* (a
/// persistent / functional hash map), not that this Rust type is read-only.
/// `im::HashMap` uses structural sharing: cloning is O(1) and old handles
/// remain valid after modification.  The `&mut self` methods on this type
/// create a *new* version internally while reusing unmodified nodes from
/// the old one — callers holding a clone still see the original data.
///
/// This makes `ImVersionVector` ideal for cases where a version vector is
/// copied and modified frequently (e.g. tracking the current version of a
/// document state).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImVersionVector(im::HashMap<PeerID, Counter, rustc_hash::FxBuildHasher>);

impl ImVersionVector {
  pub fn new() -> Self {
    Self(Default::default())
  }

  pub fn clear(&mut self) {
    self.0.clear()
  }

  pub fn get(&self, key: &PeerID) -> Option<&Counter> {
    self.0.get(key)
  }

  pub fn get_mut(&mut self, key: &PeerID) -> Option<&mut Counter> {
    self.0.get_mut(key)
  }

  pub fn insert(&mut self, k: PeerID, v: Counter) {
    self.0.insert(k, v);
  }

  pub fn is_empty(&self) -> bool {
    self.0.is_empty()
  }

  pub fn iter(&self) -> im::hashmap::Iter<'_, PeerID, Counter> {
    self.0.iter()
  }

  pub fn remove(&mut self, k: &PeerID) -> Option<Counter> {
    self.0.remove(k)
  }

  pub fn len(&self) -> usize {
    self.0.len()
  }

  pub fn contains_key(&self, k: &PeerID) -> bool {
    self.0.contains_key(k)
  }

  pub fn to_vv(&self) -> VersionVector {
    VersionVector(self.0.iter().map(|(&k, &v)| (k, v)).collect())
  }

  pub fn from_vv(vv: &VersionVector) -> Self {
    ImVersionVector(vv.0.iter().map(|(&k, &v)| (k, v)).collect())
  }

  pub fn extend_to_include_vv<'a>(&mut self, vv: impl Iterator<Item = (&'a PeerID, &'a Counter)>) {
    for (&client_id, &counter) in vv {
      if let Some(my_counter) = self.0.get_mut(&client_id) {
        if *my_counter < counter {
          *my_counter = counter;
        }
      } else {
        self.0.insert(client_id, counter);
      }
    }
  }

  #[inline]
  pub fn merge(&mut self, other: &Self) {
    self.extend_to_include_vv(other.0.iter());
  }

  #[inline]
  pub fn merge_vv(&mut self, other: &VersionVector) {
    self.extend_to_include_vv(other.0.iter());
  }

  #[inline]
  pub fn set_last(&mut self, id: ID) {
    self.0.insert(id.peer, id.counter + 1);
  }

  pub fn extend_to_include_last_id(&mut self, id: ID) {
    if let Some(counter) = self.0.get_mut(&id.peer) {
      if *counter <= id.counter {
        *counter = id.counter + 1;
      }
    } else {
      self.set_last(id)
    }
  }

  pub fn includes_id(&self, x: ID) -> bool {
    if let Some(end) = self.get(&x.peer)
      && *end > x.counter
    {
      return true;
    }
    false
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

  // ── ImVersionVector ──────────────────────────────────────────────

  #[test]
  fn test_im_vv_new_is_empty() {
    let im = ImVersionVector::new();
    assert!(im.is_empty());
    assert_eq!(im.len(), 0);
  }

  #[test]
  fn test_im_vv_default_is_empty() {
    let im = ImVersionVector::default();
    assert!(im.is_empty());
  }

  #[test]
  fn test_im_vv_insert_and_get() {
    let mut im = ImVersionVector::new();
    im.insert(1, 5);
    assert_eq!(im.get(&1).copied(), Some(5));
    assert!(im.get(&2).is_none());
    assert!(im.contains_key(&1));
  }

  #[test]
  fn test_im_vv_remove() {
    let mut im = ImVersionVector::new();
    im.insert(1, 5);
    assert_eq!(im.remove(&1), Some(5));
    assert!(im.is_empty());
  }

  #[test]
  fn test_im_vv_set_last() {
    let mut im = ImVersionVector::new();
    im.set_last(ID::new(1, 10));
    assert_eq!(im.get(&1).copied(), Some(11));
  }

  #[test]
  fn test_im_vv_extend_to_include_last_id() {
    let mut im = ImVersionVector::new();
    im.set_last(ID::new(1, 5));
    im.extend_to_include_last_id(ID::new(1, 8));
    assert_eq!(im.get(&1).copied(), Some(9));
    // already covered, no-op
    im.extend_to_include_last_id(ID::new(1, 3));
    assert_eq!(im.get(&1).copied(), Some(9));
  }

  #[test]
  fn test_im_vv_merge_takes_max() {
    let mut a = ImVersionVector::new();
    a.insert(1, 3);
    a.insert(2, 4);
    let mut b = ImVersionVector::new();
    b.insert(1, 6);
    b.insert(3, 2);
    a.merge(&b);
    assert_eq!(a.get(&1).copied(), Some(6));
    assert_eq!(a.get(&2).copied(), Some(4));
    assert_eq!(a.get(&3).copied(), Some(2));
  }

  #[test]
  fn test_im_vv_merge_vv() {
    let mut im = ImVersionVector::new();
    im.insert(1, 3);
    let vv = VersionVector::from_iter([ID::new(1, 5), ID::new(2, 3)]);
    im.merge_vv(&vv);
    assert_eq!(im.get(&1).copied(), Some(6));
    assert_eq!(im.get(&2).copied(), Some(4));
  }

  #[test]
  fn test_im_vv_includes_id() {
    let mut im = ImVersionVector::new();
    im.set_last(ID::new(1, 5));
    assert!(im.includes_id(ID::new(1, 4)));
    assert!(im.includes_id(ID::new(1, 5)));
    assert!(!im.includes_id(ID::new(1, 6)));
    assert!(!im.includes_id(ID::new(2, 0)));
  }

  #[test]
  fn test_im_vv_clone_is_cheap() {
    let mut a = ImVersionVector::new();
    a.insert(1, 5);
    a.insert(2, 7);
    let b = a.clone();
    // structural sharing: b should reflect a's data even before modification
    assert_eq!(b.get(&1).copied(), Some(5));
    assert_eq!(b.get(&2).copied(), Some(7));
  }

  #[test]
  fn test_vv_to_im_vv_roundtrip() {
    let vv = VersionVector::from_iter([ID::new(1, 5), ID::new(2, 3)]);
    let im = vv.to_im_vv();
    assert_eq!(im.get(&1).copied(), Some(6));
    assert_eq!(im.get(&2).copied(), Some(4));
    let vv2 = VersionVector::from_im_vv(&im);
    assert_eq!(vv, vv2);
  }

  #[test]
  fn test_im_vv_from_vv_roundtrip() {
    let vv = VersionVector::from_iter([ID::new(1, 5)]);
    let im = ImVersionVector::from_vv(&vv);
    assert_eq!(im.get(&1).copied(), Some(6));
    let vv2 = im.to_vv();
    assert_eq!(vv, vv2);
  }

  #[test]
  fn test_im_vv_clear() {
    let mut im = ImVersionVector::new();
    im.insert(1, 5);
    im.clear();
    assert!(im.is_empty());
  }

  #[test]
  fn test_im_vv_iter() {
    let mut im = ImVersionVector::new();
    im.insert(1, 5);
    im.insert(2, 7);
    let mut pairs: Vec<_> = im.iter().map(|(&k, &v)| (k, v)).collect();
    pairs.sort();
    assert_eq!(pairs, vec![(1, 5), (2, 7)]);
  }

  // ── diff ─────────────────────────────────────────────────────────

  #[test]
  fn test_vv_diff_basic() {
    let a = VersionVector::from_iter([ID::new(1, 5), ID::new(2, 3)]);
    let b = VersionVector::from_iter([ID::new(1, 7), ID::new(3, 2)]);
    let d = a.diff(&b);
    // a has peer 2@0..4, b doesn't -> retreat
    assert_eq!(d.retreat.get(&2).copied(), Some(CounterSpan::new(0, 4)));
    // a has peer 1@0..6, b has 1@0..8 -> forward for extra 6..8
    assert_eq!(d.forward.get(&1).copied(), Some(CounterSpan::new(6, 8)));
    // b has peer 3@0..3, a doesn't -> forward
    assert_eq!(d.forward.get(&3).copied(), Some(CounterSpan::new(0, 3)));
  }

  #[test]
  fn test_vv_diff_equal() {
    let a = VersionVector::from_iter([ID::new(1, 5)]);
    let b = VersionVector::from_iter([ID::new(1, 5)]);
    let d = a.diff(&b);
    assert!(d.retreat.is_empty());
    assert!(d.forward.is_empty());
  }

  #[test]
  fn test_vv_diff_empty() {
    let a = VersionVector::new();
    let b = VersionVector::from_iter([ID::new(1, 5)]);
    let d = a.diff(&b);
    assert!(d.retreat.is_empty());
    assert_eq!(d.forward.get(&1).copied(), Some(CounterSpan::new(0, 6)));
  }

  #[test]
  fn test_vv_diff_iter() {
    let a = VersionVector::from_iter([ID::new(1, 5)]);
    let b = VersionVector::from_iter([ID::new(1, 3), ID::new(2, 2)]);
    let (left, right) = a.diff_iter(&b);
    let left: Vec<_> = left.collect();
    let right: Vec<_> = right.collect();
    // a - b: peer 1, 4..6
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].peer, 1);
    assert_eq!(left[0].counter, CounterSpan::new(4, 6));
    // b - a: peer 2, 0..3
    assert_eq!(right.len(), 1);
    assert_eq!(right[0].peer, 2);
    assert_eq!(right[0].counter, CounterSpan::new(0, 3));
  }

  #[test]
  fn test_vv_sub_iter() {
    let a = VersionVector::from_iter([ID::new(1, 5), ID::new(2, 3)]);
    let b = VersionVector::from_iter([ID::new(1, 2)]);
    let spans: Vec<_> = a.sub_iter(&b).collect();
    assert_eq!(spans.len(), 2);
    // peer 1: 3..6
    assert!(
      spans
        .iter()
        .any(|s| s.peer == 1 && s.counter == CounterSpan::new(3, 6))
    );
    // peer 2: 0..4
    assert!(
      spans
        .iter()
        .any(|s| s.peer == 2 && s.counter == CounterSpan::new(0, 4))
    );
  }

  #[test]
  fn test_vv_iter_between() {
    let a = VersionVector::from_iter([ID::new(1, 5)]);
    let b = VersionVector::from_iter([ID::new(2, 3)]);
    let spans: Vec<_> = a.iter_between(&b).collect();
    assert_eq!(spans.len(), 2);
    assert!(spans.iter().any(|s| s.peer == 1));
    assert!(spans.iter().any(|s| s.peer == 2));
  }

  #[test]
  fn test_vv_distance_between() {
    let a = VersionVector::from_iter([ID::new(1, 5), ID::new(2, 3)]);
    let b = VersionVector::from_iter([ID::new(1, 2), ID::new(3, 4)]);
    // peer1: |6 - 3| = 3
    // peer2: a has 4, b has 0 -> 4
    // peer3: a has 0, b has 5 -> 5
    assert_eq!(a.distance_between(&b), 12);
  }

  #[test]
  fn test_vv_intersection() {
    let a = VersionVector::from_iter([ID::new(1, 5), ID::new(2, 3)]);
    let b = VersionVector::from_iter([ID::new(1, 7), ID::new(2, 2)]);
    let c = a.intersection(&b);
    assert_eq!(c.get(1).copied(), Some(6)); // min(6, 8)
    assert_eq!(c.get(2).copied(), Some(3)); // min(4, 3)
  }

  #[test]
  fn test_vv_intersection_zero_skipped() {
    let a = VersionVector::from_iter([ID::new(1, 5)]);
    let mut b = VersionVector::new();
    b.insert(1, 0); // zero entry should be skipped
    let c = a.intersection(&b);
    assert!(c.is_empty());
  }

  #[test]
  fn test_vv_extend_to_include() {
    let mut vv = VersionVector::from_iter([ID::new(1, 5)]);
    vv.extend_to_include(IdSpan::new(1, 3, 10));
    assert_eq!(vv.get(1).copied(), Some(10));
    vv.extend_to_include(IdSpan::new(2, 0, 3));
    assert_eq!(vv.get(2).copied(), Some(3));
  }

  #[test]
  fn test_vv_shrink_to_exclude() {
    let mut vv = VersionVector::from_iter([ID::new(1, 5)]);
    vv.shrink_to_exclude(IdSpan::new(1, 3, 10));
    assert_eq!(vv.get(1).copied(), Some(3));
  }

  #[test]
  fn test_vv_shrink_to_exclude_removes_zero() {
    let mut vv = VersionVector::from_iter([ID::new(1, 5)]);
    vv.shrink_to_exclude(IdSpan::new(1, 0, 3));
    // min = 0, so remove the peer entirely
    assert!(vv.get(1).is_none());
  }

  #[test]
  fn test_vv_forward_and_retreat() {
    let mut vv = VersionVector::new();
    let mut spans = IdSpanVector::default();
    spans.insert(1, CounterSpan::new(0, 6));
    spans.insert(2, CounterSpan::new(0, 4));
    vv.forward(&spans);
    assert_eq!(vv.get(1).copied(), Some(6));
    assert_eq!(vv.get(2).copied(), Some(4));

    // retreat from counter 3..6 down to 3 (shrink to exclude 3..6)
    let mut retreat = IdSpanVector::default();
    retreat.insert(1, CounterSpan::new(3, 6));
    vv.retreat(&retreat);
    assert_eq!(vv.get(1).copied(), Some(3));
  }
}
