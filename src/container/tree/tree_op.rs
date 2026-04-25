//! Tree operations — create, move, delete nodes.

use crate::types::ID;

// TODO(phase 10): replace with real FractionalIndex from src/fractional_index.rs
pub type FractionalIndex = Vec<u8>;

/// Operations on a Tree container.
#[derive(Debug, Clone, PartialEq)]
pub enum TreeOp {
  /// Create a new tree node.
  Create {
    target: TreeID,
    parent: Option<TreeID>,
    position: FractionalIndex,
  },
  /// Move an existing node to a new parent.
  Move {
    target: TreeID,
    parent: Option<TreeID>,
    position: FractionalIndex,
  },
  /// Delete a node (logically moves it to a deleted root).
  Delete { target: TreeID },
}

/// Identifier for a tree node.
///
/// A `TreeID` is simply an [`ID`] because each tree node is created by
/// a unique operation.
pub type TreeID = ID;
