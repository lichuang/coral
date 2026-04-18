//! Compact container handle.
//!
//! [`ContainerIdx`] is a 4-byte handle that replaces the much larger
//! [`ContainerID`](crate::types::ContainerID) inside the CRDT engine.

use crate::types::ContainerType;

/// A compact 32-bit handle for a container.
///
/// Layout: top 5 bits store the [`ContainerType`], remaining 27 bits store
/// the index into an arena's `idx_to_id` table.
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Hash)]
pub struct ContainerIdx(u32);

impl ContainerIdx {
  /// Mask for the type bits (top 5).
  pub(crate) const TYPE_MASK: u32 = 0b11111 << 27;
  /// Mask for the index bits (low 27).
  pub(crate) const INDEX_MASK: u32 = !Self::TYPE_MASK;

  /// Creates a new `ContainerIdx` from an arena index and container type.
  #[inline]
  pub fn from_index_and_type(index: u32, container_type: ContainerType) -> Self {
    let prefix = if let ContainerType::Unknown(k) = container_type {
      (0b10000 | (k as u32 & 0b1111)) << 27
    } else {
      (container_type.to_u8() as u32) << 27
    };
    Self(prefix | (index & Self::INDEX_MASK))
  }

  /// Returns the container type encoded in this handle.
  #[inline]
  pub fn get_type(self) -> ContainerType {
    let type_value = (self.0 & Self::TYPE_MASK) >> 27;
    if self.is_unknown() {
      ContainerType::Unknown((type_value & 0b1111) as u8)
    } else {
      ContainerType::try_from_u8(type_value as u8).expect("invalid container type in ContainerIdx")
    }
  }

  /// Returns the arena table index.
  #[inline]
  pub fn to_index(self) -> u32 {
    self.0 & Self::INDEX_MASK
  }

  /// Returns `true` if the encoded type is [`ContainerType::Unknown`].
  #[inline]
  pub fn is_unknown(self) -> bool {
    self.0 >> 31 == 1
  }
}

impl std::fmt::Debug for ContainerIdx {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "ContainerIdx({} {})", self.get_type(), self.to_index())
  }
}

impl std::fmt::Display for ContainerIdx {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "ContainerIdx({} {})", self.get_type(), self.to_index())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_container_idx_layout() {
    let idx = ContainerIdx::from_index_and_type(42, ContainerType::Map);
    assert_eq!(idx.to_index(), 42);
    assert_eq!(idx.get_type(), ContainerType::Map);
  }
}
