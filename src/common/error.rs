use thiserror::Error;

/// The error type for Coral CRDT operations.
#[derive(Error, Debug, Clone, PartialEq)]
pub enum CoralError {
  /// Unknown container type encountered during conversion.
  #[error("unknown container type: {0}")]
  UnknownContainerType(u8),
}

/// Convenience alias for `Result<T, CoralError>`.
pub type CoralResult<T> = Result<T, CoralError>;
