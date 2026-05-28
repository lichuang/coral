use thiserror::Error;

/// The error type for Coral CRDT operations.
#[derive(Error, Debug, Clone, PartialEq)]
pub enum CoralError {
  /// Unknown container type encountered during conversion.
  #[error("unknown container type: {0}")]
  UnknownContainerType(u8),
}
