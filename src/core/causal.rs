use crate::rle::HasLength;
use crate::types::{Counter, Lamport, OpId, PeerID};
use crate::version::{Heads, VersionVector};
use std::collections::BTreeMap;

/// A node in the causal DAG representing a contiguous range of operations
/// from a single peer.
///
/// Multiple consecutive changes from the same peer may be merged into a
/// single `CausalNode` when they are linearly dependent (each depends only
/// on the previous one from the same peer).
#[derive(Debug, Clone, PartialEq)]
pub struct CausalNode {
  /// The peer that produced this node.
  pub peer: PeerID,

  /// Starting counter within the peer's sequence.
  pub start: Counter,

  /// Number of consecutive counters covered by this node.
  pub len: usize,

  /// Dependencies — the DAG frontier immediately before this node.
  pub deps: Heads,

  /// Lamport timestamp of the first operation in this node.
  pub lamport: Lamport,

  state: NodeState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum NodeState {
  /// Received, but dependencies are not yet satisfied; cannot be applied.
  Pending,

  /// Dependencies satisfied, waiting to be applied to the state machine.
  Ready,

  /// Already applied to the document state machine.
  Applied,

  /// Absorbed by a snapshot and can be freed from memory,
  /// but retained in the graph for version-vector calculations.
  Archived,

  /// Permanently removed (all nodes are confirmed and do not affect
  /// future operations). Used only for internal node cleanup after GC.
  Pruned,
}

impl CausalNode {
  /// Creates a new `CausalNode`.
  pub(crate) fn new(
    peer: PeerID,
    start: Counter,
    len: usize,
    deps: Heads,
    lamport: Lamport,
  ) -> Self {
    Self {
      peer,
      start,
      len,
      deps,
      lamport,
      state: NodeState::Pending,
    }
  }

  /// Returns the starting [`OpId`] of this node.
  #[inline]
  pub fn id_start(&self) -> OpId {
    OpId::new(self.peer, self.start)
  }

  /// Returns the last [`OpId`] contained in this node.
  #[inline]
  pub fn id_last(&self) -> OpId {
    debug_assert!(self.len > 0, "CausalNode len must be > 0");
    OpId::new(self.peer, self.end() - 1)
  }

  /// Returns the exclusive end counter of this node.
  #[inline]
  pub fn end(&self) -> Counter {
    self.start + self.len as Counter
  }

  /// Returns `true` if the given counter falls within this node.
  #[inline]
  pub fn contains(&self, counter: Counter) -> bool {
    self.start <= counter && counter < self.end()
  }

  /// Returns `true` if the given [`OpId`] falls within this node.
  #[inline]
  pub fn contains_id(&self, id: &OpId) -> bool {
    self.peer == id.peer && self.contains(id.counter)
  }

  /// Extends this node's length by `additional_len`.
  #[inline]
  pub fn extend(&mut self, additional_len: usize) {
    self.len += additional_len;
  }

  /// Returns the current [`NodeState`].
  #[inline]
  pub(crate) fn state(&self) -> NodeState {
    self.state
  }

  /// Sets the [`NodeState`].
  #[inline]
  #[allow(dead_code)]
  pub(crate) fn set_state(&mut self, state: NodeState) {
    self.state = state;
  }
}

/// A directed acyclic graph that tracks the causal relationships between
/// operations across all peers.
///
/// `CausalGraph` records which operations are known to exist, but it does
/// not store the operation content itself — that is the responsibility of
/// the caller (e.g. a [`Change`](super::Change) store).
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct CausalGraph {
  /// All causal nodes, keyed by their starting [`OpId`].
  ///
  /// The [`BTreeMap`] keeps nodes globally sorted by `(peer, counter)`,
  /// which enables efficient range queries (e.g. find the node that
  /// contains a given counter via `range(..=target).next_back()`).
  nodes: BTreeMap<OpId, CausalNode>,

  /// Current version vector — the latest known counter for each peer.
  vv: VersionVector,

  /// Current frontiers — the DAG heads (latest operations with no known
  /// successors).
  frontiers: Heads,
}

impl CausalGraph {
  /// Creates an empty `CausalGraph`.
  pub fn new() -> Self {
    Self::default()
  }

  /// Returns the node that contains the given [`OpId`], if any.
  pub fn get(&self, id: &OpId) -> Option<&CausalNode> {
    let (_, node) = self.nodes.range(..=id).next_back()?;
    if node.contains_id(id) {
      Some(node)
    } else {
      None
    }
  }

  /// Returns `true` if the given [`OpId`] is included in this graph.
  pub fn includes(&self, id: &OpId) -> bool {
    self.vv.includes(id)
  }

  /// Returns the current version vector.
  pub fn vv(&self) -> &VersionVector {
    &self.vv
  }

  /// Returns the current frontiers.
  pub fn frontiers(&self) -> &Heads {
    &self.frontiers
  }

  /// Returns the number of causal nodes stored in the graph.
  pub fn node_count(&self) -> usize {
    self.nodes.len()
  }

  /// Inserts a [`Change`] into the causal graph.
  ///
  /// If the change is consecutive with the last node from the same peer
  /// and linearly dependent on it, the existing node is extended in-place
  /// rather than creating a new [`CausalNode`].
  pub fn insert(&mut self, change: &super::Change) {
    let peer = change.id.peer;
    let start = change.id.counter;
    let len: usize = change.ops.iter().map(|op| op.content_len()).sum();
    let end = start + len as Counter;

    // Hot path: try to extend the last node of this peer.
    let extend_key = {
      let maybe_last = self
        .nodes
        .range(..=OpId::new(peer, Counter::MAX))
        .next_back();
      if let Some((key, node)) = maybe_last {
        if node.peer == peer
          && node.end() == start
          && node.state() == NodeState::Pending
          && change.deps.as_single() == Some(node.id_last())
          && node.lamport + node.len as Lamport == change.lamport
        {
          Some(*key)
        } else {
          None
        }
      } else {
        None
      }
    };

    if let Some(key) = extend_key {
      self.nodes.get_mut(&key).unwrap().extend(len);
      self.update_vv_and_frontiers(peer, end, &change.deps);
    } else {
      let node = CausalNode::new(peer, start, len, change.deps.clone(), change.lamport);
      self.insert_node(node);
    }
  }

  /// Inserts a standalone [`CausalNode`] into the graph.
  ///
  /// This is used when a node cannot be merged with an existing one
  /// (e.g. non-consecutive counters or non-linear dependencies).
  pub(crate) fn insert_node(&mut self, node: CausalNode) {
    let peer = node.peer;
    let end = node.end();
    let deps = node.deps.clone();
    self.nodes.insert(node.id_start(), node);
    self.update_vv_and_frontiers(peer, end, &deps);
  }

  /// Updates the version vector and frontiers after inserting ops
  /// spanning `[peer, end)` with the given dependencies.
  fn update_vv_and_frontiers(&mut self, peer: PeerID, end: Counter, deps: &Heads) {
    if self.vv.get(peer).is_none_or(|c| c < end) {
      self.vv.insert(peer, end);
    }
    for dep in deps.iter() {
      self.frontiers.remove(&dep);
    }
    let new_last = OpId::new(peer, end - 1);
    self.frontiers.push(new_last);
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::Change;
  use crate::operation::{Cmd, Op};
  use crate::types::{ContainerIndex, ContainerType, OpId};

  fn make_change(
    peer: PeerID,
    counter: Counter,
    len: usize,
    deps: Heads,
    lamport: Lamport,
    from_local: bool,
  ) -> Change {
    let id = OpId::new(peer, counter);
    let mut change = Change::new(id, lamport, 0, deps, from_local);
    for i in 0..len {
      let op = Op::new(
        counter + i as Counter,
        ContainerIndex::new(0, ContainerType::Counter),
        Cmd::IncCounter { delta: 1.0 },
      );
      change.push_op(op);
    }
    change
  }

  #[test]
  fn test_insert_hot_path_merges_consecutive_changes() {
    let mut cg = CausalGraph::new();

    // First change from peer 1.
    let a = make_change(1, 0, 1, Heads::new(), 0, true);
    cg.insert(&a);

    assert_eq!(cg.node_count(), 1);
    assert!(cg.vv().includes(&OpId::new(1, 0)));
    assert!(!cg.vv().includes(&OpId::new(1, 1)));
    assert_eq!(cg.frontiers().as_single(), Some(OpId::new(1, 0)));

    // Second change from peer 1 is consecutive and linearly dependent.
    let b = make_change(1, 1, 1, Heads::from_id(OpId::new(1, 0)), 1, true);
    cg.insert(&b);

    // Should merge into the same node.
    assert_eq!(cg.node_count(), 1);
    let node = cg.get(&OpId::new(1, 0)).unwrap();
    assert_eq!(node.len, 2);
    assert!(cg.vv().includes(&OpId::new(1, 1)));
    assert_eq!(cg.frontiers().as_single(), Some(OpId::new(1, 1)));
  }

  #[test]
  fn test_insert_cold_path_creates_new_node() {
    let mut cg = CausalGraph::new();

    let a = make_change(1, 0, 1, Heads::new(), 0, true);
    cg.insert(&a);

    // Change from a different peer cannot be merged.
    let b = make_change(2, 0, 1, Heads::from_id(OpId::new(1, 0)), 1, false);
    cg.insert(&b);

    assert_eq!(cg.node_count(), 2);
    // Peer 1 is no longer a head because peer 2 depends on it.
    assert!(!cg.frontiers().contains(&OpId::new(1, 0)));
    assert!(cg.frontiers().contains(&OpId::new(2, 0)));
    assert_eq!(cg.frontiers().len(), 1);
  }

  #[test]
  fn test_insert_node_creates_standalone_node() {
    let mut cg = CausalGraph::new();

    let node = CausalNode::new(1, 0, 3, Heads::new(), 0);
    cg.insert_node(node);

    assert_eq!(cg.node_count(), 1);
    assert!(cg.get(&OpId::new(1, 0)).is_some());
    assert!(cg.get(&OpId::new(1, 2)).is_some());
    assert!(cg.get(&OpId::new(1, 3)).is_none());
    assert_eq!(cg.vv().get_or_zero(1), 3);
    assert_eq!(cg.frontiers().as_single(), Some(OpId::new(1, 2)));
  }

  #[test]
  fn test_insert_does_not_merge_applied_node() {
    let mut cg = CausalGraph::new();

    // Insert a node and mark it as Applied.
    let mut node = CausalNode::new(1, 0, 2, Heads::new(), 0);
    node.set_state(NodeState::Applied);
    cg.insert_node(node);

    assert_eq!(cg.node_count(), 1);
    assert_eq!(
      cg.get(&OpId::new(1, 0)).unwrap().state(),
      NodeState::Applied
    );

    // Try to insert a consecutive change from the same peer.
    let change = make_change(1, 2, 1, Heads::from_id(OpId::new(1, 1)), 2, true);
    cg.insert(&change);

    // Should NOT merge; a new node should be created.
    assert_eq!(cg.node_count(), 2);
    assert_eq!(cg.get(&OpId::new(1, 0)).unwrap().len, 2); // old node unchanged
    assert_eq!(cg.get(&OpId::new(1, 2)).unwrap().len, 1); // new node
  }

  #[test]
  fn test_insert_updates_frontiers_correctly() {
    let mut cg = CausalGraph::new();

    // Peer 1 inserts two ops.
    let a = make_change(1, 0, 2, Heads::new(), 0, true);
    cg.insert(&a);
    assert_eq!(cg.frontiers().as_single(), Some(OpId::new(1, 1)));

    // Peer 2 inserts one op depending on peer 1's latest.
    let b = make_change(2, 0, 1, Heads::from_id(OpId::new(1, 1)), 2, false);
    cg.insert(&b);

    // Peer 1's head is replaced by peer 2's head.
    assert!(!cg.frontiers().contains(&OpId::new(1, 1)));
    assert!(cg.frontiers().contains(&OpId::new(2, 0)));
    assert_eq!(cg.frontiers().len(), 1);
  }
}
