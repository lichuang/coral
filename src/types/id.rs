//! Unique operation identifier and related utilities.
//!
//! An [`ID`] combines a [`PeerID`](super::primitives::PeerID) with a
//! [`Counter`](super::primitives::Counter) to form a globally unique identifier
//! for every operation in the distributed system.

use super::primitives::{Counter, PeerID};
use std::cmp::Ordering;

/// Globally unique identifier for a single operation.
///
/// The pair `(peer, counter)` is unique across the entire distributed
/// system because each peer independently increments its own counter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ID {
  pub peer: PeerID,
  pub counter: Counter,
}

impl ID {
  /// Creates a new `ID`.
  #[inline]
  pub const fn new(peer: PeerID, counter: Counter) -> Self {
    Self { peer, counter }
  }

  /// Returns `true` if this is the root / sentinel ID (`peer == 0 && counter == 0`).
  ///
  /// The root ID is used as a virtual anchor for List RGA insertions
  /// (elements inserted at position 0 use the root as their left origin).
  #[inline]
  pub const fn is_root(&self) -> bool {
    self.peer == 0 && self.counter == 0
  }

  /// Returns a new `ID` with the counter advanced by `delta`.
  ///
  /// This is used when a single `Change` contains multiple `Op`s:
  /// the first Op uses the Change's `id`, the second uses `id.inc(1)`, and so on.
  #[inline]
  pub const fn inc(&self, delta: Counter) -> Self {
    Self {
      peer: self.peer,
      counter: self.counter + delta,
    }
  }
}

/// Deterministic ordering: first by `peer`, then by `counter`.
///
/// This ordering matches the derived `Ord` semantics in Loro, where the
/// struct field declaration order (`peer` before `counter`) determines the
/// lexicographic comparison. It is used for RGA tie-breaking and for
/// collections (e.g. `BTreeMap<ID, _>`) that require a total order.
impl PartialOrd for ID {
  fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
    Some(self.cmp(other))
  }
}

impl Ord for ID {
  fn cmp(&self, other: &Self) -> Ordering {
    match self.peer.cmp(&other.peer) {
      Ordering::Equal => self.counter.cmp(&other.counter),
      ord => ord,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_id_new() {
    let id = ID::new(42, 7);
    assert_eq!(id.peer, 42);
    assert_eq!(id.counter, 7);
  }

  #[test]
  fn test_id_is_root() {
    assert!(ID::new(0, 0).is_root());
    assert!(!ID::new(0, 1).is_root());
    assert!(!ID::new(1, 0).is_root());
  }

  #[test]
  fn test_id_inc() {
    let id = ID::new(42, 5);
    assert_eq!(id.inc(1), ID::new(42, 6));
    assert_eq!(id.inc(3), ID::new(42, 8));
    assert_eq!(id.inc(0), id);
  }

  #[test]
  fn test_id_ord_by_peer_then_counter() {
    // Peer takes precedence over counter.
    let a = ID::new(1, 100);
    let b = ID::new(2, 1);
    assert!(a < b, "peer 1 < peer 2 regardless of counter");

    // Same peer: counter breaks the tie.
    let c = ID::new(10, 5);
    let d = ID::new(10, 6);
    assert!(c < d, "same peer, smaller counter wins");

    // Equal IDs.
    let e = ID::new(7, 7);
    let f = ID::new(7, 7);
    assert_eq!(e.cmp(&f), Ordering::Equal);
  }

  #[test]
  fn test_id_hash_and_eq() {
    use std::collections::HashSet;
    let mut set = HashSet::new();
    set.insert(ID::new(1, 2));
    assert!(set.contains(&ID::new(1, 2)));
    assert!(!set.contains(&ID::new(1, 3)));
  }
}
