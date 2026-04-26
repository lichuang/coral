//! Causal DAG traits.
//!
//! The DAG is built from [`DagNode`]s, each representing a single [`Change`].
//! It tracks causal dependencies (`deps`), logical timestamps (`lamport`), and
//! identity (`id_start`) so that history can be traversed, merged, and diffed.

use crate::types::{ID, Lamport};
use crate::version::{Frontiers, VersionVector};

/// A node in the causal directed acyclic graph.
///
/// Each node corresponds to one [`Change`](crate::core::change::Change) —
/// a transaction boundary.  The DAG is built from Change to Change, not
/// from individual Op to Op.
#[allow(dead_code)]
pub trait DagNode {
  /// The direct causal dependencies of this node.
  fn deps(&self) -> &Frontiers;

  /// Lamport timestamp (logical clock) of this node.
  fn lamport(&self) -> Lamport;

  /// The [`ID`] of the first atomic operation in this node.
  fn id_start(&self) -> ID;

  /// Number of atomic operations contained in this node.
  fn len(&self) -> usize;
}

/// A causal DAG storing [`DagNode`]s indexed by their start [`ID`].
#[allow(dead_code)]
pub trait Dag {
  /// Concrete node type stored in this DAG.
  type Node: DagNode;

  /// Looks up a node by the [`ID`] of its first operation.
  fn get(&self, id: ID) -> Option<&Self::Node>;

  /// Returns the current frontier — the minimal set of leaf IDs.
  fn frontier(&self) -> &Frontiers;

  /// Returns the version vector covering all nodes known to this DAG.
  fn vv(&self) -> &VersionVector;

  /// Returns `true` if this DAG contains a node whose counter range covers `id`.
  fn contains(&self, id: ID) -> bool;
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::change::Change;
  use crate::memory::arena::InnerArena;
  use crate::op::{Op, OpContent};
  use crate::rle::RleVec;
  use crate::types::{ContainerID, ContainerType, ID};
  use crate::version::Frontiers;

  #[test]
  fn test_dag_node_for_change() {
    let arena = InnerArena::new();
    let container = arena.register(&ContainerID::new_root("c", ContainerType::Counter));
    let ops = RleVec::from(vec![
      Op::new(0, container, OpContent::Counter(1.0)),
      Op::new(1, container, OpContent::Counter(2.0)),
    ]);
    let change = Change::new(
      ops,
      Frontiers::from_id(ID::new(0, 0)),
      ID::new(1, 0),
      5,
      1_700_000_000,
    );

    assert_eq!(change.id_start(), ID::new(1, 0));
    assert_eq!(change.lamport(), 5);
    assert_eq!(change.deps(), &Frontiers::from_id(ID::new(0, 0)));
    assert_eq!(change.len(), 2);
  }
}
