use crate::common::{CoralError, CoralResult};
use crate::types::OpId;
use std::fmt;

/// Unique identifier for a CRDT object (container).
///
/// - [`Root`] objects are user-named top-level containers (e.g. `"text"`,
///   `"map"`).
/// - [`Node`] objects are created at runtime and identified by the operation
///   that created them.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ObjectId {
  Root { name: String, typ: ObjectType },
  Node { op: OpId, typ: ObjectType },
}

impl ObjectId {
  pub fn name(&self) -> &str {
    match self {
      ObjectId::Root { name, .. } => name,
      ObjectId::Node { .. } => "",
    }
  }

  pub fn typ(&self) -> ObjectType {
    match self {
      ObjectId::Root { typ, .. } => *typ,
      ObjectId::Node { typ, .. } => *typ,
    }
  }
}

/// The type of a CRDT object (container).
///
/// Objects are the building blocks of collaborative documents.
/// Each object type has its own conflict resolution semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectType {
  /// A counter that supports increments and decrements (PN-Counter).
  Counter,
}

impl From<ObjectType> for u8 {
  fn from(value: ObjectType) -> Self {
    match value {
      ObjectType::Counter => 0,
    }
  }
}

impl TryFrom<u8> for ObjectType {
  type Error = CoralError;

  fn try_from(value: u8) -> Result<Self, Self::Error> {
    match value {
      0 => Ok(ObjectType::Counter),
      _ => Err(CoralError::UnknownObjectType(value)),
    }
  }
}

impl fmt::Display for ObjectType {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      ObjectType::Counter => write!(f, "Counter"),
    }
  }
}

/// Number of bits reserved for the object type in [`ObjectIndex`].
const OBJECT_TYPE_BITS: u32 = 4;
/// Mask for extracting the object type from a packed [`ObjectIndex`].
const OBJECT_TYPE_MASK: u32 = (1 << OBJECT_TYPE_BITS) - 1;

/// A compact runtime index that references a container, embedding its type
/// into a single `u32`.
///
/// Bit layout (scheme A):
/// - low [`OBJECT_TYPE_BITS`]: object type
/// - remaining bits: container index
///
/// `ObjectIndex` is cheaper to store and compare than a full container
/// identifier, but it is only valid within the document that created it.
/// It must not be used across different replicas or serialization boundaries.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjectIndex(u32);

impl ObjectIndex {
  /// Creates a new `ObjectIndex`.
  #[inline]
  pub fn new(index: u32, typ: ObjectType) -> Self {
    Self((index << OBJECT_TYPE_BITS) | (u8::from(typ) as u32 & OBJECT_TYPE_MASK))
  }

  /// Returns the container index.
  #[inline]
  pub const fn index(&self) -> u32 {
    self.0 >> OBJECT_TYPE_BITS
  }

  /// Returns the object type.
  #[inline]
  pub fn typ(&self) -> CoralResult<ObjectType> {
    ObjectType::try_from((self.0 & OBJECT_TYPE_MASK) as u8)
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
  fn test_object_id_root() {
    let id = ObjectId::Root {
      name: "text".into(),
      typ: ObjectType::Counter,
    };
    assert!(matches!(id, ObjectId::Root { .. }));
  }

  #[test]
  fn test_object_id_node() {
    let id = ObjectId::Node {
      op: OpId::new(42, 100),
      typ: ObjectType::Counter,
    };
    assert_eq!(
      id,
      ObjectId::Node {
        op: OpId::new(42, 100),
        typ: ObjectType::Counter,
      }
    );
  }

  #[test]
  fn test_object_type_to_u8() {
    assert_eq!(u8::from(ObjectType::Counter), 0);
  }

  #[test]
  fn test_object_type_try_from_u8_ok() {
    let ct: ObjectType = 0u8.try_into().unwrap();
    assert_eq!(ct, ObjectType::Counter);
  }

  #[test]
  fn test_object_type_try_from_u8_err() {
    let result: Result<ObjectType, _> = 1u8.try_into();
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), CoralError::UnknownObjectType(1));
  }

  #[test]
  fn test_object_type_roundtrip() {
    let original = ObjectType::Counter;
    let byte: u8 = original.into();
    let recovered: ObjectType = byte.try_into().unwrap();
    assert_eq!(original, recovered);
  }

  #[test]
  fn test_object_type_display() {
    assert_eq!(ObjectType::Counter.to_string(), "Counter");
  }

  #[test]
  fn test_container_index_packing() {
    let idx = ObjectIndex::new(42, ObjectType::Counter);
    assert_eq!(idx.index(), 42);
    assert_eq!(idx.typ().unwrap(), ObjectType::Counter);

    let raw = idx.to_u32();
    assert_eq!(raw >> OBJECT_TYPE_BITS, 42);
    assert_eq!(raw & OBJECT_TYPE_MASK, ObjectType::Counter as u32);
  }

  #[test]
  fn test_container_index_max_index() {
    let idx = ObjectIndex::new(0x0FFF_FFFF, ObjectType::Counter);
    assert_eq!(idx.index(), 0x0FFF_FFFF);
    assert_eq!(idx.typ().unwrap(), ObjectType::Counter);
  }

  #[test]
  fn test_container_index_ordering() {
    let a = ObjectIndex::new(1, ObjectType::Counter);
    let b = ObjectIndex::new(2, ObjectType::Counter);
    assert!(a < b);
  }

  #[test]
  fn test_container_index_equality() {
    let a = ObjectIndex::new(7, ObjectType::Counter);
    let b = ObjectIndex::new(7, ObjectType::Counter);
    assert_eq!(a, b);
  }
}
