//! Operation unit — the smallest change that can be applied to a container.
//!
//! An [`Op`] targets a single container and carries a payload ([`OpContent`])
//! describing what to do.  The concrete operation types (`MapOp`, `ListOp`, …)
//! are defined in the [`content`] sub-module and re-exported here.

mod content;

pub use content::*;

use crate::types::{ContainerID, ID, Lamport};

/// A single atomic operation.
///
/// Every Op is uniquely identified by its [`ID`].  It also carries a
/// [`Lamport`] timestamp so that concurrent operations can be ordered
/// deterministically (Last-Write-Wins).
///
/// # Note
///
/// The `container` field currently stores a [`ContainerID`].  In later
/// phases this will be optimised to the compact [`ContainerIdx`](crate::arena::ContainerIdx)
/// internally, while the public API continues to expose `ContainerID`.
#[derive(Debug, Clone)]
pub struct Op {
  /// Globally unique identifier for this operation.
  pub id: ID,

  /// The target container.
  pub container: ContainerID,

  /// What to do in the target container.
  pub content: OpContent,

  /// Lamport timestamp for causal / LWW ordering.
  pub lamport: Lamport,
}

impl Op {
  /// Creates a new `Op`.
  pub fn new(id: ID, container: ContainerID, content: OpContent, lamport: Lamport) -> Self {
    Self {
      id,
      container,
      content,
      lamport,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::types::{ContainerType, ID};

  #[test]
  fn test_op_new() {
    let id = ID::new(1, 0);
    let container = ContainerID::new_root("my_map", ContainerType::Map);
    let content = OpContent::Map(MapOp);
    let op = Op::new(id, container.clone(), content, 42);
    assert_eq!(op.id, id);
    assert_eq!(op.container, container);
    assert_eq!(op.lamport, 42);
  }

  #[test]
  fn test_op_content_variants() {
    // Just make sure every variant can be constructed.
    let _ = OpContent::Map(MapOp);
    let _ = OpContent::List(ListOp);
    let _ = OpContent::Text(TextOp);
    let _ = OpContent::Tree(TreeOp);
    let _ = OpContent::Counter(CounterOp);
  }
}
