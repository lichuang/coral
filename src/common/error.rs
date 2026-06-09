use thiserror::Error;

/// The error type for Coral CRDT operations.
#[derive(Error, Debug, Clone, PartialEq)]
pub enum CoralError {
  /// Unknown object type encountered during conversion.
  #[error("unknown object type: {0}")]
  UnknownObjectType(u8),

  /// Requested object does not exist.
  #[error("object not found: {0}")]
  NotFound(String),

  /// Object type does not match the expected type.
  #[error("type mismatch: expected {expected}, found {actual}")]
  TypeMismatch { expected: String, actual: String },

  /// An operation was applied to a container that cannot handle it.
  #[error("invalid operation for container: {0}")]
  InvalidOperation(String),

  /// JSON serialization or deserialization error.
  #[error("JSON error: {0}")]
  Json(String),

  /// Import validation failed.
  #[error("invalid import: {0}")]
  InvalidImport(String),

  #[error("invalid export: {0}")]
  InvalidExport(String),
}

impl From<serde_json::Error> for CoralError {
  fn from(e: serde_json::Error) -> Self {
    CoralError::Json(e.to_string())
  }
}

/// Convenience alias for `Result<T, CoralError>`.
pub type CoralResult<T> = Result<T, CoralError>;
