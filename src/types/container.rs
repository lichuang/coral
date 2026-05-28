use crate::common::{CoralError, CoralResult};
use std::fmt;

/// The type of a CRDT container.
///
/// Containers are the building blocks of collaborative documents.
/// Each container type has its own conflict resolution semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ContainerType {
  /// A counter that supports increments and decrements (PN-Counter).
  Counter,
}

impl From<ContainerType> for u8 {
  fn from(value: ContainerType) -> Self {
    match value {
      ContainerType::Counter => 0,
    }
  }
}

impl TryFrom<u8> for ContainerType {
  type Error = CoralError;

  fn try_from(value: u8) -> Result<Self, Self::Error> {
    match value {
      0 => Ok(ContainerType::Counter),
      _ => Err(CoralError::UnknownContainerType(value)),
    }
  }
}

impl fmt::Display for ContainerType {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      ContainerType::Counter => write!(f, "Counter"),
    }
  }
}

/// A compact runtime index that references a container, embedding its type
/// into a single `u32`.
///
/// Number of bits reserved for the container type in [`ContainerIndex`].
const CONTAINER_TYPE_BITS: u32 = 4;
/// Mask for extracting the container type from a packed [`ContainerIndex`].
const CONTAINER_TYPE_MASK: u32 = (1 << CONTAINER_TYPE_BITS) - 1;

/// A compact runtime index that references a container, embedding its type
/// into a single `u32`.
///
/// Bit layout (scheme A):
/// - low [`CONTAINER_TYPE_BITS`]: container type
/// - remaining bits: container index
///
/// `ContainerIndex` is cheaper to store and compare than a full container
/// identifier, but it is only valid within the document that created it.
/// It must not be used across different replicas or serialization boundaries.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContainerIndex(u32);

impl ContainerIndex {
  /// Creates a new `ContainerIndex`.
  #[inline]
  pub fn new(index: u32, container_type: ContainerType) -> Self {
    Self((index << CONTAINER_TYPE_BITS) | (u8::from(container_type) as u32 & CONTAINER_TYPE_MASK))
  }

  /// Returns the container index.
  #[inline]
  pub const fn index(&self) -> u32 {
    self.0 >> CONTAINER_TYPE_BITS
  }

  /// Returns the container type.
  #[inline]
  pub fn container_type(&self) -> CoralResult<ContainerType> {
    ContainerType::try_from((self.0 & CONTAINER_TYPE_MASK) as u8)
  }

  /// Returns the raw packed `u32` value.
  #[inline]
  pub const fn to_u32(&self) -> u32 {
    self.0
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_container_type_to_u8() {
    assert_eq!(u8::from(ContainerType::Counter), 0);
  }

  #[test]
  fn test_container_type_try_from_u8_ok() {
    let ct: ContainerType = 0u8.try_into().unwrap();
    assert_eq!(ct, ContainerType::Counter);
  }

  #[test]
  fn test_container_type_try_from_u8_err() {
    let result: Result<ContainerType, _> = 1u8.try_into();
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), CoralError::UnknownContainerType(1));
  }

  #[test]
  fn test_container_type_roundtrip() {
    let original = ContainerType::Counter;
    let byte: u8 = original.into();
    let recovered: ContainerType = byte.try_into().unwrap();
    assert_eq!(original, recovered);
  }

  #[test]
  fn test_container_type_display() {
    assert_eq!(ContainerType::Counter.to_string(), "Counter");
  }

  #[test]
  fn test_container_index_packing() {
    let idx = ContainerIndex::new(42, ContainerType::Counter);
    assert_eq!(idx.index(), 42);
    assert_eq!(idx.container_type().unwrap(), ContainerType::Counter);

    // Verify the raw encoding: index in high bits, type in low bits.
    let raw = idx.to_u32();
    assert_eq!(raw >> CONTAINER_TYPE_BITS, 42);
    assert_eq!(raw & CONTAINER_TYPE_MASK, ContainerType::Counter as u32);
  }

  #[test]
  fn test_container_index_max_index() {
    let idx = ContainerIndex::new(0x0FFF_FFFF, ContainerType::Counter);
    assert_eq!(idx.index(), 0x0FFF_FFFF);
    assert_eq!(idx.container_type().unwrap(), ContainerType::Counter);
  }

  #[test]
  fn test_container_index_ordering() {
    let a = ContainerIndex::new(1, ContainerType::Counter);
    let b = ContainerIndex::new(2, ContainerType::Counter);
    assert!(a < b);
  }

  #[test]
  fn test_container_index_equality() {
    let a = ContainerIndex::new(7, ContainerType::Counter);
    let b = ContainerIndex::new(7, ContainerType::Counter);
    assert_eq!(a, b);
  }
}
