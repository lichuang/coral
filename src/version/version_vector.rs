use crate::types::{Counter, OpId, PeerID};
use rustc_hash::FxHashMap;

/// A version vector tracks the latest observed counter for each peer.
///
/// It is used to determine causal ordering and to detect whether a given
/// operation has already been applied.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct VersionVector(FxHashMap<PeerID, Counter>);

impl VersionVector {
  /// Creates an empty version vector.
  pub fn new() -> Self {
    Self(FxHashMap::default())
  }

  /// Returns the counter for the given peer, if known.
  pub fn get(&self, peer: PeerID) -> Option<Counter> {
    self.0.get(&peer).copied()
  }

  /// Returns the counter for the given peer, or `0` if not present.
  pub fn get_or_zero(&self, peer: PeerID) -> Counter {
    self.get(peer).unwrap_or(0)
  }

  /// Sets the counter for the given peer.
  pub fn insert(&mut self, peer: PeerID, counter: Counter) {
    self.0.insert(peer, counter);
  }

  /// Merges another version vector into this one, taking the maximum
  /// counter for each peer.
  pub fn merge(&mut self, other: &Self) {
    for (&peer, &counter) in other.0.iter() {
      let entry = self.0.entry(peer).or_insert(counter);
      if *entry < counter {
        *entry = counter;
      }
    }
  }

  /// Returns `true` if this version vector includes the given operation.
  ///
  /// An operation is considered included if its counter is strictly less
  /// than the tracked counter for its peer.
  pub fn includes(&self, op_id: &OpId) -> bool {
    self.get_or_zero(op_id.peer) > op_id.counter
  }

  /// Returns `true` if there are no tracked peers.
  pub fn is_empty(&self) -> bool {
    self.0.is_empty()
  }

  /// Returns the number of tracked peers.
  pub fn len(&self) -> usize {
    self.0.len()
  }

  /// Returns an iterator over (peer, counter) pairs.
  pub fn iter(&self) -> impl Iterator<Item = (&PeerID, &Counter)> {
    self.0.iter()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_vv_basic() {
    let mut vv = VersionVector::new();
    assert!(vv.is_empty());

    vv.insert(1, 10);
    assert_eq!(vv.get(1), Some(10));
    assert_eq!(vv.get_or_zero(1), 10);
    assert_eq!(vv.get_or_zero(99), 0);
    assert_eq!(vv.len(), 1);
  }

  #[test]
  fn test_vv_merge() {
    let mut a = VersionVector::new();
    a.insert(1, 5);
    a.insert(2, 3);

    let mut b = VersionVector::new();
    b.insert(1, 8);
    b.insert(3, 7);

    a.merge(&b);
    assert_eq!(a.get(1), Some(8));
    assert_eq!(a.get(2), Some(3));
    assert_eq!(a.get(3), Some(7));
  }

  #[test]
  fn test_vv_includes() {
    let mut vv = VersionVector::new();
    vv.insert(1, 5);

    assert!(vv.includes(&OpId::new(1, 0)));
    assert!(vv.includes(&OpId::new(1, 4)));
    assert!(!vv.includes(&OpId::new(1, 5)));
    assert!(!vv.includes(&OpId::new(1, 6)));
    assert!(!vv.includes(&OpId::new(2, 0)));
  }
}
