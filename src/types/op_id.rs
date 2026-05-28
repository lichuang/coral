use super::{Counter, PeerID};
use std::cmp::Ordering;
use std::fmt;

/// Globally unique identifier for an operation.
///
/// An `OpId` combines a [`PeerID`] and a monotonically increasing [`Counter`]
/// to uniquely identify every operation across the distributed system.
///
/// # Ordering
///
/// `OpId` is ordered first by `peer`, then by `counter`. This provides a
/// deterministic total order, though it does **not** reflect causality.
/// For causal ordering, use [`Lamport`](super::Lamport) timestamps.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct OpId {
  pub peer: PeerID,
  pub counter: Counter,
}

impl OpId {
  /// Creates a new `OpId`.
  #[inline]
  pub const fn new(peer: PeerID, counter: Counter) -> Self {
    Self { peer, counter }
  }

  /// Returns a new `OpId` with the counter incremented by `offset`.
  #[inline]
  pub const fn inc(&self, offset: Counter) -> Self {
    Self {
      peer: self.peer,
      counter: self.counter + offset,
    }
  }
}

impl PartialOrd for OpId {
  #[inline]
  fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
    Some(self.cmp(other))
  }
}

impl Ord for OpId {
  /// Total order: first by `peer`, then by `counter`.
  ///
  /// This ordering is deterministic but **does not reflect causality**.
  /// Two concurrent operations from different peers may have any relative
  /// order here; use Lamport timestamps for causal comparison.
  #[inline]
  fn cmp(&self, other: &Self) -> Ordering {
    match self.peer.cmp(&other.peer) {
      Ordering::Equal => self.counter.cmp(&other.counter),
      ord => ord,
    }
  }
}

impl fmt::Debug for OpId {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}@{}", self.counter, self.peer)
  }
}

impl fmt::Display for OpId {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}@{}", self.counter, self.peer)
  }
}

impl TryFrom<&str> for OpId {
  type Error = String;

  fn try_from(value: &str) -> Result<Self, Self::Error> {
    if value.split('@').count() != 2 {
      return Err("Invalid OpId format".into());
    }
    let mut iter = value.split('@');
    let counter = iter
      .next()
      .unwrap()
      .parse::<Counter>()
      .map_err(|_| "Invalid OpId format: counter".to_string())?;
    let peer = iter
      .next()
      .unwrap()
      .parse::<PeerID>()
      .map_err(|_| "Invalid OpId format: peer".to_string())?;
    Ok(OpId::new(peer, counter))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_op_id_ordering() {
    let a = OpId::new(1, 10);
    let b = OpId::new(1, 20);
    let c = OpId::new(2, 5);

    assert!(a < b);
    assert!(b < c);
    assert!(a < c);
  }

  #[test]
  fn test_op_id_inc() {
    let id = OpId::new(42, 100);
    assert_eq!(id.inc(5), OpId::new(42, 105));
  }

  #[test]
  fn test_op_id_try_from_str_ok() {
    let id: OpId = "10@42".try_into().unwrap();
    assert_eq!(id.counter, 10);
    assert_eq!(id.peer, 42);

    let id: OpId = "-1@0".try_into().unwrap();
    assert_eq!(id.counter, -1);
    assert_eq!(id.peer, 0);

    let id: OpId = "0@18446744073709551615".try_into().unwrap();
    assert_eq!(id.counter, 0);
    assert_eq!(id.peer, u64::MAX);
  }

  #[test]
  fn test_op_id_try_from_str_err() {
    // missing @
    let result: Result<OpId, _> = "42".try_into();
    assert!(result.is_err());

    // empty string
    let result: Result<OpId, _> = "".try_into();
    assert!(result.is_err());

    // multiple @
    let result: Result<OpId, _> = "10@42@extra".try_into();
    assert!(result.is_err());

    // invalid counter
    let result: Result<OpId, _> = "abc@42".try_into();
    assert!(result.is_err());

    // invalid peer
    let result: Result<OpId, _> = "10@abc".try_into();
    assert!(result.is_err());

    // counter overflow (i32)
    let result: Result<OpId, _> = "2147483648@42".try_into();
    assert!(result.is_err());
  }
}
