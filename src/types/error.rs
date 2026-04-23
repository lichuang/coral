//! Error types for the Coral CRDT library.
//!
//! [`CoralError`] is the unified error enum used across all public and
//! internal APIs.  [`CoralResult<T>`] is a convenience alias for
//! `Result<T, CoralError>`.

use crate::types::{ContainerID, ContainerType, ID};
use thiserror::Error;

/// Unified error type for Coral operations.
#[derive(Error, Debug, Clone, PartialEq)]
pub enum CoralError {
  /// The requested container does not exist in the arena.
  #[error("Container not found: {0}")]
  ContainerNotFound(ContainerID),

  /// A positional index is out of range or otherwise invalid.
  #[error("Invalid position: {0}")]
  InvalidPosition(usize),

  /// The actual container type does not match the expected type.
  #[error("Type mismatch: expected {expected}, got {got}")]
  TypeMismatch {
    expected: ContainerType,
    got: ContainerType,
  },

  /// A change depends on an ID that is not yet present in the DAG.
  #[error("DAG invariant violated: missing dependency {0:?}")]
  MissingDependency(ID),

  /// An index exceeds the bounds of a sequence or buffer.
  #[error("Index out of bound")]
  OutOfBound,

  /// Failure while decoding an external representation (e.g. snapshot, update).
  #[error("Decode error: {0}")]
  DecodeError(String),

  /// Failure to acquire an internal lock (e.g. `Mutex` poisoned).
  #[error("Lock error")]
  LockError,
}

/// Convenience alias for `Result<T, CoralError>`.
pub type CoralResult<T> = Result<T, CoralError>;
