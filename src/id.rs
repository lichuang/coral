//! Unique operation identifier and related utilities.
//!
//! An [`ID`] combines a [`PeerID`](crate::types::PeerID) with a
//! [`Counter`](crate::types::Counter) to form a globally unique identifier
//! for every operation in the distributed system.

use crate::types::{Counter, PeerID};
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
  /// This is used when a single [`Change`](crate::change::Change) contains
  /// multiple [`Op`](crate::op::Op)s: the first Op uses the Change's `id`,
  /// the second uses `id.inc(1)`, and so on.
  #[inline]
  pub const fn inc(&self, delta: Counter) -> Self {
    Self {
      peer: self.peer,
      counter: self.counter + delta,
    }
  }
}

/// Deterministic ordering: first by `counter`, then by `peer`.
///
/// This ordering is critical for concurrent insert resolution in RGA
/// (Replicated Growable Array) where ties on Lamport timestamp must be
/// broken deterministically across all peers.
impl PartialOrd for ID {
  fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
    Some(self.cmp(other))
  }
}

impl Ord for ID {
  fn cmp(&self, other: &Self) -> Ordering {
    match self.counter.cmp(&other.counter) {
      Ordering::Equal => self.peer.cmp(&other.peer),
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
  fn test_id_ord_by_counter_then_peer() {
    // Counter takes precedence over peer.
    let a = ID::new(100, 1);
    let b = ID::new(1, 2);
    assert!(a < b, "counter 1 < counter 2 regardless of peer");

    // Same counter: peer breaks the tie.
    let c = ID::new(10, 5);
    let d = ID::new(20, 5);
    assert!(c < d, "same counter, smaller peer wins");

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
