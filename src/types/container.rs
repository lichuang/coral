//! Container identifiers and types.
//!
//! A [`ContainerType`] categorizes the kind of CRDT a container holds.
//! [`ContainerID`] (defined later in Phase 1.4) uniquely identifies a
//! specific container instance.

/// The kind of CRDT stored inside a container.
///
/// Each variant maps to a distinct CRDT algorithm and user-facing API.
/// The discriminant is a single byte (`#[repr(u8)]`) so that the type can
/// be encoded compactly in wire formats and snapshots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ContainerType {
  /// A key-value map using Last-Write-Wins registers.
  Map = 0,

  /// An ordered sequence using the RGA (Replicated Growable Array) algorithm.
  List = 1,

  /// A collaborative text buffer using the Fugue algorithm.
  Text = 2,

  /// An ordered sequence where elements can be moved (MovableList CRDT).
  MovableList = 3,

  /// A hierarchical tree structure where nodes can be reparented.
  Tree = 4,

  /// A distributed counter supporting positive and negative increments.
  Counter = 5,
}

impl ContainerType {
  /// All known container types, in discriminant order.
  pub const ALL_TYPES: [ContainerType; 6] = [
    ContainerType::Map,
    ContainerType::List,
    ContainerType::Text,
    ContainerType::MovableList,
    ContainerType::Tree,
    ContainerType::Counter,
  ];

  /// Returns the single-byte discriminant.
  #[inline]
  pub const fn to_u8(self) -> u8 {
    self as u8
  }

  /// Parses a `ContainerType` from its single-byte discriminant.
  ///
  /// Returns `None` if the byte does not correspond to a known variant.
  #[inline]
  pub const fn try_from_u8(v: u8) -> Option<Self> {
    match v {
      0 => Some(ContainerType::Map),
      1 => Some(ContainerType::List),
      2 => Some(ContainerType::Text),
      3 => Some(ContainerType::MovableList),
      4 => Some(ContainerType::Tree),
      5 => Some(ContainerType::Counter),
      _ => None,
    }
  }
}

impl std::fmt::Display for ContainerType {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      ContainerType::Map => write!(f, "Map"),
      ContainerType::List => write!(f, "List"),
      ContainerType::Text => write!(f, "Text"),
      ContainerType::MovableList => write!(f, "MovableList"),
      ContainerType::Tree => write!(f, "Tree"),
      ContainerType::Counter => write!(f, "Counter"),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_container_type_roundtrip() {
    for &ty in &ContainerType::ALL_TYPES {
      let byte = ty.to_u8();
      let parsed = ContainerType::try_from_u8(byte).unwrap();
      assert_eq!(ty, parsed, "roundtrip failed for {:?}", ty);
    }
  }

  #[test]
  fn test_container_type_invalid_byte() {
    assert!(ContainerType::try_from_u8(6).is_none());
    assert!(ContainerType::try_from_u8(255).is_none());
  }

  #[test]
  fn test_container_type_display() {
    assert_eq!(ContainerType::Map.to_string(), "Map");
    assert_eq!(ContainerType::Counter.to_string(), "Counter");
  }

  #[test]
  fn test_container_type_discriminants() {
    assert_eq!(ContainerType::Map as u8, 0);
    assert_eq!(ContainerType::List as u8, 1);
    assert_eq!(ContainerType::Text as u8, 2);
    assert_eq!(ContainerType::MovableList as u8, 3);
    assert_eq!(ContainerType::Tree as u8, 4);
    assert_eq!(ContainerType::Counter as u8, 5);
  }
}
