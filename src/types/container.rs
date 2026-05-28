use crate::common::CoralError;
use std::fmt;

/// The type of a CRDT container.
///
/// Containers are the building blocks of collaborative documents.
/// Each container type has its own conflict resolution semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
}
