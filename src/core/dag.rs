//! Causal DAG traits and node types.
//!
//! The DAG is built from [`DagNode`]s, each representing a single [`Change`].
//! It tracks causal dependencies (`deps`), logical timestamps (`lamport`), and
//! identity (`id_start`) so that history can be traversed, merged, and diffed.

use crate::rle::Sliceable;
use crate::types::{Counter, ID, Lamport, PeerID};
use crate::version::{Frontiers, VersionVector};
use std::ops::Deref;
use std::sync::{Arc, OnceLock};

/// A node in the causal directed acyclic graph.
///
/// Aligned with Loro's design: `DagNode` extends `Debug` and `Sliceable`
/// so that nodes can be sliced during DAG traversal (e.g. LCA lookup).
#[allow(dead_code)]
pub trait DagNode: std::fmt::Debug + Sliceable {
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
pub trait Dag: std::fmt::Debug {
  /// Concrete node type stored in this DAG.
  type Node: DagNode;

  /// Looks up a node by the [`ID`] of its first operation.
  ///
  /// Returns an owned value so that `AppDagNode` (backed by `Arc`) can be
  /// cloned cheaply without lifetime ties to the DAG container.
  fn get(&self, id: ID) -> Option<Self::Node>;

  /// Returns the current frontier — the minimal set of leaf IDs.
  fn frontier(&self) -> &Frontiers;

  /// Returns the version vector covering all nodes known to this DAG.
  fn vv(&self) -> &VersionVector;

  /// Returns `true` if this DAG contains a node whose counter range covers `id`.
  fn contains(&self, id: ID) -> bool;
}

// ═══════════════════════════════════════════════════════════════════════════
// DagNodeInner
// ═══════════════════════════════════════════════════════════════════════════

/// Internal fields of a DAG node.
///
/// Each node represents a contiguous run of operations from a single peer.
/// `has_succ` is set to `true` when another node depends on this one,
/// preventing RLE merge at the DAG level.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct DagNodeInner {
  pub peer: PeerID,
  pub cnt: Counter,
  pub lamport: Lamport,
  pub deps: Frontiers,
  pub len: usize,
  pub has_succ: bool,
  /// Lazy cached version vector computed from this node's ancestors.
  pub vv: OnceLock<VersionVector>,
}

#[allow(dead_code)]
impl DagNodeInner {
  /// Creates a new `DagNodeInner`.
  pub fn new(peer: PeerID, cnt: Counter, lamport: Lamport, deps: Frontiers, len: usize) -> Self {
    Self {
      peer,
      cnt,
      lamport,
      deps,
      len,
      has_succ: false,
      vv: OnceLock::new(),
    }
  }

  /// The [`ID`] of the first operation in this node.
  #[inline]
  pub fn id_start(&self) -> ID {
    ID::new(self.peer, self.cnt)
  }

  /// The [`ID`] of the last operation in this node.
  #[inline]
  pub fn id_last(&self) -> ID {
    ID::new(self.peer, self.cnt + self.len as Counter - 1)
  }

  /// The exclusive end [`ID`] (first ID after this node).
  #[inline]
  pub fn id_end(&self) -> ID {
    ID::new(self.peer, self.cnt + self.len as Counter)
  }

  /// Returns `true` if `id` falls inside this node's counter range.
  pub fn contains_id(&self, id: ID) -> bool {
    id.peer == self.peer && id.counter >= self.cnt && id.counter < self.id_end().counter
  }
}

// ═══════════════════════════════════════════════════════════════════════════
// AppDagNode
// ═══════════════════════════════════════════════════════════════════════════

/// A node in the application-level DAG.
///
/// Wraps [`DagNodeInner`] via `Arc` so that cloning is O(1) and nodes can be
/// shared across iterators and caches.  This matches Loro's `AppDagNode`
/// design.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct AppDagNode {
  inner: Arc<DagNodeInner>,
}

impl Deref for AppDagNode {
  type Target = DagNodeInner;

  fn deref(&self) -> &Self::Target {
    &self.inner
  }
}

#[allow(dead_code)]
impl AppDagNode {
  /// Wraps an existing [`DagNodeInner`] in an `Arc`.
  pub fn new(inner: DagNodeInner) -> Self {
    Self {
      inner: Arc::new(inner),
    }
  }

  /// Returns the cached version vector, computing it on first access.
  ///
  /// The VV is derived by merging the VVs of all dependency nodes and
  /// then adding this node's own ID range.
  pub fn vv<F>(&self, get_vv: F) -> &VersionVector
  where
    F: FnOnce(&Frontiers) -> VersionVector,
  {
    self.inner.vv.get_or_init(|| {
      let mut vv = get_vv(&self.inner.deps);
      vv.set_last(self.inner.id_last());
      vv
    })
  }

  /// Invalidates the cached version vector.
  ///
  /// Called when the DAG is modified in a way that affects ancestor
  /// relationships (e.g. out-of-order insertion).
  pub fn invalidate_vv(&mut self) {
    let inner = Arc::make_mut(&mut self.inner);
    inner.vv = OnceLock::new();
  }
}

#[allow(dead_code)]
impl Sliceable for AppDagNode {
  fn slice(&self, from: usize, to: usize) -> Self {
    let new_inner = DagNodeInner {
      peer: self.peer,
      cnt: self.cnt + from as Counter,
      lamport: self.lamport + from as Lamport,
      deps: if from == 0 {
        self.deps.clone()
      } else {
        Frontiers::from_id(ID::new(self.peer, self.cnt + from as Counter - 1))
      },
      len: to - from,
      has_succ: false,
      vv: OnceLock::new(),
    };
    Self::new(new_inner)
  }
}

#[allow(dead_code)]
impl DagNode for AppDagNode {
  fn deps(&self) -> &Frontiers {
    &self.inner.deps
  }

  fn lamport(&self) -> Lamport {
    self.inner.lamport
  }

  fn id_start(&self) -> ID {
    self.inner.id_start()
  }

  fn len(&self) -> usize {
    self.inner.len
  }
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

  #[test]
  fn test_dag_node_inner_basic() {
    let inner = DagNodeInner::new(1, 0, 5, Frontiers::from_id(ID::new(0, 0)), 3);
    assert_eq!(inner.id_start(), ID::new(1, 0));
    assert_eq!(inner.id_last(), ID::new(1, 2));
    assert_eq!(inner.id_end(), ID::new(1, 3));
    assert!(inner.contains_id(ID::new(1, 0)));
    assert!(inner.contains_id(ID::new(1, 2)));
    assert!(!inner.contains_id(ID::new(1, 3)));
    assert!(!inner.contains_id(ID::new(2, 0)));
    assert!(!inner.has_succ);
  }

  #[test]
  fn test_app_dag_node_dag_node_trait() {
    let inner = DagNodeInner::new(1, 5, 10, Frontiers::from_id(ID::new(0, 0)), 2);
    let node = AppDagNode::new(inner);

    assert_eq!(node.id_start(), ID::new(1, 5));
    assert_eq!(node.lamport(), 10);
    assert_eq!(node.deps(), &Frontiers::from_id(ID::new(0, 0)));
    assert_eq!(node.len(), 2);
  }

  #[test]
  fn test_app_dag_node_sliceable() {
    let inner = DagNodeInner::new(1, 0, 5, Frontiers::from_id(ID::new(0, 0)), 4);
    let node = AppDagNode::new(inner);

    let sliced = node.slice(1, 3);
    assert_eq!(sliced.id_start(), ID::new(1, 1));
    assert_eq!(sliced.lamport(), 6);
    assert_eq!(sliced.len(), 2);
    assert_eq!(sliced.deps(), &Frontiers::from_id(ID::new(1, 0)));

    let sliced_from_start = node.slice(0, 2);
    assert_eq!(sliced_from_start.id_start(), ID::new(1, 0));
    assert_eq!(sliced_from_start.deps(), &Frontiers::from_id(ID::new(0, 0)));
  }

  #[test]
  fn test_app_dag_node_vv_lazy() {
    let inner = DagNodeInner::new(1, 0, 5, Frontiers::new(), 3);
    let node = AppDagNode::new(inner);

    // First access computes the VV via the closure.
    let vv = node.vv(|deps| {
      assert!(deps.is_empty());
      VersionVector::new()
    });
    assert_eq!(vv.get(1).copied(), Some(3)); // exclusive end = cnt + len = 0 + 3

    // Second access returns the cached value (closure is not called again).
    let vv2 = node.vv(|_| panic!("should not be called — cached"));
    assert_eq!(vv2.get(1).copied(), Some(3));
  }

  #[test]
  fn test_app_dag_node_vv_with_deps() {
    let inner = DagNodeInner::new(2, 0, 8, Frontiers::from_id(ID::new(1, 2)), 2);
    let node = AppDagNode::new(inner);

    let vv = node.vv(|deps| {
      let mut base = VersionVector::new();
      for id in deps.iter() {
        base.set_last(id);
      }
      base
    });

    assert_eq!(vv.get(1).copied(), Some(3)); // from deps: peer 1, counter 2 -> exclusive end 3
    assert_eq!(vv.get(2).copied(), Some(2)); // from self: peer 2, cnt 0, len 2 -> exclusive end 2
  }

  #[test]
  fn test_app_dag_node_invalidate_vv() {
    let inner = DagNodeInner::new(1, 0, 5, Frontiers::new(), 1);
    let mut node = AppDagNode::new(inner);

    let vv1 = node.vv(|_| VersionVector::new());
    assert_eq!(vv1.get(1).copied(), Some(1));

    // Cached: closure should NOT be called again.
    let vv2 = node.vv(|_| panic!("should not be called — cached"));
    assert_eq!(vv2.get(1).copied(), Some(1));

    node.invalidate_vv();

    // After invalidate, closure IS called again and yields the same VV.
    let vv3 = node.vv(|_| VersionVector::new());
    assert_eq!(vv3.get(1).copied(), Some(1));
  }
}
