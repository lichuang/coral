use thiserror::Error;

/// The error type for Coral CRDT operations.
#[derive(Error, Debug, Clone, PartialEq)]
pub enum CoralError {
  /// Unknown object type encountered during conversion.
  #[error("unknown object type: {0}")]
  UnknownObjectType(u8),
}

/// Convenience alias for `Result<T, CoralError>`.
pub type CoralResult<T> = Result<T, CoralError>;
