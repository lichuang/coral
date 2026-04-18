//! Directed Acyclic Graph (causal graph) of Changes.
//!
//! The DAG stores the causal history of the document. Each [`Change`] is one
//! node. Edges run from a Change to its direct predecessors (`deps`).
//!
//! Design follows Loro's `AppDag` but is simplified for Coral:
//! - No lazy loading (all nodes are in memory).
//! - No node merging (each Change becomes exactly one node).
//! - No shallow history / snapshot trimming.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::core::change::Change;
use crate::types::{ID, Lamport};
use crate::version::{Frontiers, VersionVector};

/// The causal DAG.
///
/// Nodes are stored in a `BTreeMap` keyed by the Change's start ID so that
/// range queries (`range(..=id).next_back()`) can locate the Change that
/// covers an arbitrary counter position.
#[derive(Debug, Default)]
pub struct Dag {
  nodes: std::collections::BTreeMap<ID, Change>,
  frontiers: Frontiers,
  vv: VersionVector,
}

impl Dag {
  /// Creates an empty DAG.
  pub fn new() -> Self {
    Self::default()
  }

  /// Returns `true` if the DAG contains no nodes.
  pub fn is_empty(&self) -> bool {
    self.vv.is_empty()
  }

  // ─────────────────────────────────────────────────────────────────────────
  // Mutation
  // ─────────────────────────────────────────────────────────────────────────

  /// Adds a `Change` to the DAG.
  ///
  /// Updates `frontiers` and `vv` incrementally.
  pub fn add_change(&mut self, change: Change) {
    let last_id = change.id_last();
    self
      .frontiers
      .update_frontiers_on_new_change(last_id, &change.deps);
    self.vv.extend_to_include_last_id(last_id);
    self.nodes.insert(change.id, change);
  }

  // ─────────────────────────────────────────────────────────────────────────
  // Lookups
  // ─────────────────────────────────────────────────────────────────────────

  /// Looks up the Change that contains `id`.
  ///
  /// `id` may point to any Op inside a Change, not just the first one.
  pub fn get(&self, id: ID) -> Option<&Change> {
    let (_, change) = self.nodes.range(..=id).next_back()?;
    if change.contains_id(id) {
      Some(change)
    } else {
      None
    }
  }

  /// Returns `true` if the DAG knows about `id`.
  pub fn contains(&self, id: ID) -> bool {
    self.vv.includes_id(id)
  }

  /// Current frontiers (minimal leaf set).
  pub fn frontiers(&self) -> &Frontiers {
    &self.frontiers
  }

  /// Current version vector.
  pub fn vv(&self) -> &VersionVector {
    &self.vv
  }

  // ─────────────────────────────────────────────────────────────────────────
  // Version-vector conversions
  // ─────────────────────────────────────────────────────────────────────────

  /// Computes the full [`VersionVector`] for a set of frontiers.
  ///
  /// Walks backwards from each frontier ID, merging deps' VVs.
  pub fn frontiers_to_vv(&self, frontiers: &Frontiers) -> Option<VersionVector> {
    if frontiers.is_empty() {
      return Some(VersionVector::new());
    }

    let mut vv = VersionVector::new();
    let mut visited = FxHashSet::default();
    let mut stack: Vec<ID> = frontiers.iter().collect();

    while let Some(id) = stack.pop() {
      if !visited.insert(id) {
        continue;
      }
      let change = self.get(id)?;
      vv.try_update_last(id);
      for dep_id in change.deps.iter() {
        if !visited.contains(&dep_id) {
          stack.push(dep_id);
        }
      }
    }

    Some(vv)
  }

  /// Converts a [`VersionVector`] into the minimal [`Frontiers`].
  pub fn vv_to_frontiers(&self, vv: &VersionVector) -> Frontiers {
    let last_ids: Vec<ID> = vv
      .iter()
      .filter_map(|(peer, &counter)| {
        if counter > 0 {
          Some(ID::new(*peer, counter - 1))
        } else {
          None
        }
      })
      .collect();
    self.shrink_frontiers(&last_ids)
  }

  /// Shrinks a set of IDs to the minimal concurrent frontiers.
  ///
  /// Removes any ID that is an ancestor of another ID in the set.
  fn shrink_frontiers(&self, ids: &[ID]) -> Frontiers {
    if ids.len() <= 1 {
      return Frontiers::from(ids.to_vec());
    }

    // Sort by lamport descending (newest first).
    let mut sorted: Vec<ID> = ids.to_vec();
    sorted.sort_by(|a, b| {
      let a_node = self.get(*a).unwrap();
      let b_node = self.get(*b).unwrap();
      a_node
        .lamport
        .cmp(&b_node.lamport)
        .then_with(|| a.peer.cmp(&b.peer))
        .reverse()
    });

    let mut result = Vec::new();
    let mut covered = FxHashSet::default();

    for id in sorted {
      if covered.contains(&id) {
        continue;
      }
      result.push(id);
      self.travel_ancestors(id, &mut |change| {
        covered.insert(change.id);
      });
    }

    Frontiers::from(result)
  }

  // ─────────────────────────────────────────────────────────────────────────
  // LCA
  // ─────────────────────────────────────────────────────────────────────────

  /// Finds the Lowest Common Ancestor (LCA) of two versions.
  ///
  /// Returns the minimal set of shared ancestors that are "closest" to both
  /// versions. Algorithm follows Loro: max-heap traversal on lamport.
  pub fn find_common_ancestor(&self, a: &Frontiers, b: &Frontiers) -> Frontiers {
    if b.is_empty() || a.is_empty() {
      return Frontiers::default();
    }

    // Fast path: single node on each side, same peer.
    if let (Some(a_id), Some(b_id)) = (a.as_single(), b.as_single())
      && a_id.peer == b_id.peer
    {
      if a_id.counter < b_id.counter {
        return Frontiers::from_id(a_id);
      } else {
        return Frontiers::from_id(b_id);
      }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct HeapItem {
      id: ID,
      lamport: Lamport,
    }

    impl Ord for HeapItem {
      fn cmp(&self, other: &Self) -> Ordering {
        self
          .lamport
          .cmp(&other.lamport)
          .then_with(|| self.id.peer.cmp(&other.id.peer))
          .reverse()
      }
    }

    impl PartialOrd for HeapItem {
      fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
      }
    }

    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    enum Side {
      A,
      B,
      Shared,
    }

    let mut heap: BinaryHeap<(HeapItem, Side)> = BinaryHeap::new();
    let mut visited: FxHashMap<ID, Side> = FxHashMap::default();

    for id in a.iter() {
      if let Some(change) = self.get(id) {
        heap.push((
          HeapItem {
            id,
            lamport: change.lamport,
          },
          Side::A,
        ));
      }
    }
    for id in b.iter() {
      if let Some(change) = self.get(id) {
        heap.push((
          HeapItem {
            id,
            lamport: change.lamport,
          },
          Side::B,
        ));
      }
    }

    while let Some((item, side)) = heap.pop() {
      // If the same ID is also at the top of the heap with a different side,
      // they meet — mark as Shared.
      while let Some((other_item, other_side)) = heap.peek() {
        if item.id == other_item.id {
          let other_side = *other_side;
          heap.pop();
          if side != other_side {
            visited.insert(item.id, Side::Shared);
            break;
          }
        } else {
          break;
        }
      }

      if visited.get(&item.id) == Some(&Side::Shared) {
        continue;
      }

      let current_side = match visited.get(&item.id) {
        Some(Side::Shared) => Side::Shared,
        Some(existing) if *existing != side => {
          visited.insert(item.id, Side::Shared);
          Side::Shared
        }
        Some(existing) => *existing,
        None => {
          visited.insert(item.id, side);
          side
        }
      };

      if current_side != Side::Shared
        && let Some(change) = self.get(item.id)
      {
        for dep_id in change.deps.iter() {
          if let Some(dep_change) = self.get(dep_id) {
            heap.push((
              HeapItem {
                id: dep_id,
                lamport: dep_change.lamport,
              },
              current_side,
            ));
          }
        }
      }
    }

    let shared: Vec<ID> = visited
      .iter()
      .filter(|(_, side)| **side == Side::Shared)
      .map(|(id, _)| *id)
      .collect();

    self.shrink_frontiers(&shared)
  }

  // ─────────────────────────────────────────────────────────────────────────
  // Traversal
  // ─────────────────────────────────────────────────────────────────────────

  /// Iterate over all nodes in causal (topological) order.
  ///
  /// Yields changes in an order where every dependency comes before its
  /// dependents. Uses a max-heap on lamport — valid because the DAG
  /// invariant guarantees all deps have a smaller lamport.
  pub fn iter(&self) -> DagIterator<'_> {
    DagIterator::new(self)
  }

  /// Traverse ancestors of `id` in reverse causal order (greatest lamport first).
  fn travel_ancestors(&self, id: ID, f: &mut dyn FnMut(&Change)) {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct AncestorHeapItem {
      id: ID,
      lamport: Lamport,
    }

    impl Ord for AncestorHeapItem {
      fn cmp(&self, other: &Self) -> Ordering {
        self.lamport.cmp(&other.lamport).reverse()
      }
    }

    impl PartialOrd for AncestorHeapItem {
      fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
      }
    }

    let mut visited = FxHashSet::default();
    let mut heap = BinaryHeap::new();

    if let Some(change) = self.get(id) {
      heap.push(AncestorHeapItem {
        id: change.id,
        lamport: change.lamport,
      });
      visited.insert(change.id);
    }

    while let Some(item) = heap.pop() {
      let change = self.get(item.id).unwrap();
      f(change);
      for dep_id in change.deps.iter() {
        if let Some(dep_change) = self.get(dep_id)
          && visited.insert(dep_change.id)
        {
          heap.push(AncestorHeapItem {
            id: dep_change.id,
            lamport: dep_change.lamport,
          });
        }
      }
    }
  }

  // ─────────────────────────────────────────────────────────────────────────
  // Utility
  // ─────────────────────────────────────────────────────────────────────────

  /// Computes the version vector for a single ID.
  ///
  /// This is the set of all operations that causally precede `id`.
  pub fn get_vv(&self, id: ID) -> Option<VersionVector> {
    let mut vv = VersionVector::new();
    let mut visited = FxHashSet::default();
    let mut stack = vec![id];

    while let Some(current_id) = stack.pop() {
      if !visited.insert(current_id) {
        continue;
      }
      let change = self.get(current_id)?;
      vv.try_update_last(current_id);
      for dep_id in change.deps.iter() {
        if !visited.contains(&dep_id) {
          stack.push(dep_id);
        }
      }
    }

    Some(vv)
  }

  /// Compares the causal order of two IDs.
  ///
  /// Returns `None` if they are concurrent.
  pub fn cmp_id(&self, a: ID, b: ID) -> Option<Ordering> {
    if a.peer == b.peer {
      return Some(a.counter.cmp(&b.counter));
    }
    let a_vv = self.get_vv(a)?;
    let b_vv = self.get_vv(b)?;
    a_vv.partial_cmp(&b_vv)
  }

  /// Returns the lamport timestamp for a specific Op ID.
  pub fn get_lamport(&self, id: &ID) -> Option<Lamport> {
    let change = self.get(*id)?;
    Some(change.lamport + (id.counter - change.id.counter) as Lamport)
  }

  /// Calculates the next lamport for a change with the given deps.
  pub fn get_lamport_from_deps(&self, deps: &Frontiers) -> Option<Lamport> {
    let mut lamport = 0;
    for id in deps.iter() {
      let l = self.get_lamport(&id)?;
      lamport = lamport.max(l + 1);
    }
    Some(lamport)
  }

  /// Returns changes that are in `to` but not in `from`, in causal order.
  pub fn diff_changes(&self, from: &VersionVector, to: &VersionVector) -> Vec<&Change> {
    // `from.diff(to).forward` = spans present in `to` but not in `from`.
    let diff = from.diff(to);

    let mut changes = Vec::new();
    for (peer, span) in diff.forward.iter() {
      // We need every Change whose counter range intersects [start, end).
      // Because Changes are stored by start ID, scan all start IDs up to end-1.
      let end_id = ID::new(*peer, span.end.saturating_sub(1));
      for (_, change) in self.nodes.range(..=end_id) {
        if change.id.peer != *peer {
          continue;
        }
        let change_end = change.id_end().counter;
        if change.id.counter >= span.end || change_end <= span.start {
          continue;
        }
        changes.push(change);
      }
    }

    // Sort by lamport (causal order) and deduplicate.
    changes.sort_by(|a, b| {
      a.lamport
        .cmp(&b.lamport)
        .then_with(|| a.id.peer.cmp(&b.id.peer))
    });
    changes.dedup_by(|a, b| a.id == b.id);

    changes
  }
}

// ═══════════════════════════════════════════════════════════════════════════
// Iterator
// ═══════════════════════════════════════════════════════════════════════════

/// Causal-order iterator over all nodes in a [`Dag`].
pub struct DagIterator<'a> {
  dag: &'a Dag,
  heap: BinaryHeap<DagIterHeapItem>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DagIterHeapItem {
  id: ID,
  lamport: Lamport,
}

impl Ord for DagIterHeapItem {
  fn cmp(&self, other: &Self) -> Ordering {
    self.lamport.cmp(&other.lamport).reverse()
  }
}

impl PartialOrd for DagIterHeapItem {
  fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
    Some(self.cmp(other))
  }
}

impl<'a> DagIterator<'a> {
  fn new(dag: &'a Dag) -> Self {
    let mut heap = BinaryHeap::new();

    // Seed with the first Change of each peer.
    // We scan `nodes` (sorted by ID) rather than `vv` so that peers whose
    // history does not start at counter 0 are still included.
    let mut seen_peers = FxHashSet::default();
    for (id, change) in &dag.nodes {
      if seen_peers.insert(id.peer) {
        heap.push(DagIterHeapItem {
          id: change.id,
          lamport: change.lamport,
        });
      }
    }

    Self { dag, heap }
  }
}

impl<'a> Iterator for DagIterator<'a> {
  type Item = &'a Change;

  fn next(&mut self) -> Option<Self::Item> {
    let item = self.heap.pop()?;
    let change = self.dag.get(item.id)?;

    // Push the next Change from the same peer.
    let next_id = change.id_end();
    if self.dag.contains(next_id)
      && let Some(next_change) = self.dag.get(next_id)
    {
      self.heap.push(DagIterHeapItem {
        id: next_change.id,
        lamport: next_change.lamport,
      });
    }

    Some(change)
  }
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::arena::Arena;
  use crate::op::{CounterOp, Op, OpContent};
  use crate::types::{ContainerID, ContainerType, ID};
  use crate::version::Frontiers;

  fn dummy_op() -> Op {
    let mut arena = Arena::new();
    let container = arena.register(&ContainerID::new_root("counter", ContainerType::Counter));
    Op::new(0, container, OpContent::Counter(CounterOp))
  }

  fn change(id: ID, lamport: u32, deps: Frontiers, op_count: usize) -> Change {
    Change::new(id, lamport, 0, deps, vec![dummy_op(); op_count])
  }

  // ─────────────────────────────────────────────────────────────────────────
  // Basic structure
  // ─────────────────────────────────────────────────────────────────────────

  #[test]
  fn test_dag_empty() {
    let dag = Dag::new();
    assert!(dag.is_empty());
    assert!(dag.frontiers().is_empty());
    assert!(dag.vv().is_empty());
  }

  #[test]
  fn test_dag_add_single_change() {
    let mut dag = Dag::new();
    let c = change(ID::new(1, 0), 1, Frontiers::new(), 1);
    dag.add_change(c.clone());

    assert!(!dag.is_empty());
    assert_eq!(dag.frontiers().as_single(), Some(ID::new(1, 0)));
    assert_eq!(dag.vv().get(1), Some(&1));
    assert_eq!(dag.get(ID::new(1, 0)).map(|c| c.id), Some(ID::new(1, 0)));
  }

  #[test]
  fn test_dag_add_linear_sequence() {
    let mut dag = Dag::new();
    let c1 = change(ID::new(1, 0), 1, Frontiers::new(), 1);
    let c2 = change(ID::new(1, 1), 2, Frontiers::from_id(ID::new(1, 0)), 1);
    let c3 = change(ID::new(1, 2), 3, Frontiers::from_id(ID::new(1, 1)), 1);

    dag.add_change(c1.clone());
    dag.add_change(c2.clone());
    dag.add_change(c3.clone());

    assert_eq!(dag.frontiers().as_single(), Some(ID::new(1, 2)));
    assert_eq!(dag.vv().get(1), Some(&3));

    // Range lookup: ID(1,1) is inside change c2.
    assert_eq!(dag.get(ID::new(1, 1)).map(|c| c.id), Some(ID::new(1, 1)));
  }

  #[test]
  fn test_dag_add_multi_op_change() {
    let mut dag = Dag::new();
    let c = change(ID::new(1, 0), 1, Frontiers::new(), 3);
    dag.add_change(c.clone());

    // All three Op IDs map back to the same Change.
    assert_eq!(dag.get(ID::new(1, 0)).map(|c| c.id), Some(ID::new(1, 0)));
    assert_eq!(dag.get(ID::new(1, 1)).map(|c| c.id), Some(ID::new(1, 0)));
    assert_eq!(dag.get(ID::new(1, 2)).map(|c| c.id), Some(ID::new(1, 0)));
    assert!(dag.get(ID::new(1, 3)).is_none());
  }

  // ─────────────────────────────────────────────────────────────────────────
  // Concurrent changes
  // ─────────────────────────────────────────────────────────────────────────

  #[test]
  fn test_dag_concurrent_changes() {
    let mut dag = Dag::new();
    // Peer 1 creates change A
    let a = change(ID::new(1, 0), 1, Frontiers::new(), 1);
    dag.add_change(a.clone());

    // Peer 2 creates change B concurrently (also depends on empty).
    let b = change(ID::new(2, 0), 2, Frontiers::new(), 1);
    dag.add_change(b.clone());

    assert_eq!(dag.frontiers().len(), 2);
    let ids: Vec<ID> = dag.frontiers().iter().collect();
    assert!(ids.contains(&ID::new(1, 0)));
    assert!(ids.contains(&ID::new(2, 0)));
  }

  #[test]
  fn test_dag_merge_after_concurrent() {
    let mut dag = Dag::new();
    let a = change(ID::new(1, 0), 1, Frontiers::new(), 1);
    let b = change(ID::new(2, 0), 2, Frontiers::new(), 1);
    let m = change(
      ID::new(1, 1),
      3,
      Frontiers::from(vec![ID::new(1, 0), ID::new(2, 0)]),
      1,
    );

    dag.add_change(a.clone());
    dag.add_change(b.clone());
    dag.add_change(m.clone());

    assert_eq!(dag.frontiers().as_single(), Some(ID::new(1, 1)));
    assert!(dag.vv().includes_id(ID::new(1, 0)));
    assert!(dag.vv().includes_id(ID::new(2, 0)));
    assert!(dag.vv().includes_id(ID::new(1, 1)));
  }

  // ─────────────────────────────────────────────────────────────────────────
  // LCA
  // ─────────────────────────────────────────────────────────────────────────

  #[test]
  fn test_dag_lca_linear() {
    let mut dag = Dag::new();
    let c1 = change(ID::new(1, 0), 1, Frontiers::new(), 1);
    let c2 = change(ID::new(1, 1), 2, Frontiers::from_id(ID::new(1, 0)), 1);
    let c3 = change(ID::new(1, 2), 3, Frontiers::from_id(ID::new(1, 1)), 1);

    dag.add_change(c1.clone());
    dag.add_change(c2.clone());
    dag.add_change(c3.clone());

    let lca = dag.find_common_ancestor(
      &Frontiers::from_id(ID::new(1, 2)),
      &Frontiers::from_id(ID::new(1, 1)),
    );
    assert_eq!(lca.as_single(), Some(ID::new(1, 1)));
  }

  #[test]
  fn test_dag_lca_empty_and_version() {
    let mut dag = Dag::new();
    let c1 = change(ID::new(1, 0), 1, Frontiers::new(), 1);
    dag.add_change(c1.clone());

    let lca = dag.find_common_ancestor(&Frontiers::new(), &Frontiers::from_id(ID::new(1, 0)));
    assert!(lca.is_empty());
  }

  #[test]
  fn test_dag_lca_concurrent() {
    let mut dag = Dag::new();
    let a = change(ID::new(1, 0), 1, Frontiers::new(), 1);
    let b = change(ID::new(2, 0), 2, Frontiers::new(), 1);
    dag.add_change(a.clone());
    dag.add_change(b.clone());

    let lca = dag.find_common_ancestor(
      &Frontiers::from_id(ID::new(1, 0)),
      &Frontiers::from_id(ID::new(2, 0)),
    );
    assert!(lca.is_empty());
  }

  #[test]
  fn test_dag_lca_after_merge() {
    let mut dag = Dag::new();
    let a = change(ID::new(1, 0), 1, Frontiers::new(), 1);
    let b = change(ID::new(2, 0), 2, Frontiers::new(), 1);
    let m = change(
      ID::new(1, 1),
      3,
      Frontiers::from(vec![ID::new(1, 0), ID::new(2, 0)]),
      1,
    );

    dag.add_change(a.clone());
    dag.add_change(b.clone());
    dag.add_change(m.clone());

    // LCA of merge and peer-2's tip should be peer-2's tip itself (it's an ancestor).
    let lca = dag.find_common_ancestor(
      &Frontiers::from_id(ID::new(1, 1)),
      &Frontiers::from_id(ID::new(2, 0)),
    );
    assert_eq!(lca.as_single(), Some(ID::new(2, 0)));
  }

  // ─────────────────────────────────────────────────────────────────────────
  // Topological iteration
  // ─────────────────────────────────────────────────────────────────────────

  #[test]
  fn test_dag_iter_linear() {
    let mut dag = Dag::new();
    let c1 = change(ID::new(1, 0), 1, Frontiers::new(), 1);
    let c2 = change(ID::new(1, 1), 2, Frontiers::from_id(ID::new(1, 0)), 1);
    let c3 = change(ID::new(1, 2), 3, Frontiers::from_id(ID::new(1, 1)), 1);

    dag.add_change(c1.clone());
    dag.add_change(c2.clone());
    dag.add_change(c3.clone());

    let ids: Vec<ID> = dag.iter().map(|c| c.id).collect();
    assert_eq!(ids, vec![ID::new(1, 0), ID::new(1, 1), ID::new(1, 2)]);
  }

  #[test]
  fn test_dag_iter_concurrent() {
    let mut dag = Dag::new();
    let a = change(ID::new(1, 0), 1, Frontiers::new(), 1);
    let b = change(ID::new(2, 0), 2, Frontiers::new(), 1);
    let m = change(
      ID::new(1, 1),
      3,
      Frontiers::from(vec![ID::new(1, 0), ID::new(2, 0)]),
      1,
    );

    dag.add_change(a.clone());
    dag.add_change(b.clone());
    dag.add_change(m.clone());

    let ids: Vec<ID> = dag.iter().map(|c| c.id).collect();
    // Both a and b come before m. a and b order depends on lamport.
    assert_eq!(ids[0], ID::new(1, 0)); // lamport 1
    assert_eq!(ids[1], ID::new(2, 0)); // lamport 2
    assert_eq!(ids[2], ID::new(1, 1)); // lamport 3
  }

  // ─────────────────────────────────────────────────────────────────────────
  // VV / frontiers round-trip
  // ─────────────────────────────────────────────────────────────────────────

  #[test]
  fn test_dag_frontiers_to_vv_roundtrip() {
    let mut dag = Dag::new();
    let c1 = change(ID::new(1, 0), 1, Frontiers::new(), 1);
    let c2 = change(ID::new(2, 0), 2, Frontiers::new(), 1);
    let m = change(
      ID::new(1, 1),
      3,
      Frontiers::from(vec![ID::new(1, 0), ID::new(2, 0)]),
      1,
    );

    dag.add_change(c1.clone());
    dag.add_change(c2.clone());
    dag.add_change(m.clone());

    let vv = dag.frontiers_to_vv(dag.frontiers()).unwrap();
    assert_eq!(vv.get(1), Some(&2)); // peer 1 has counters 0..2
    assert_eq!(vv.get(2), Some(&1)); // peer 2 has counters 0..1

    let back = dag.vv_to_frontiers(&vv);
    assert_eq!(back, *dag.frontiers());
  }

  // ─────────────────────────────────────────────────────────────────────────
  // diff_changes
  // ─────────────────────────────────────────────────────────────────────────

  #[test]
  fn test_dag_diff_changes_linear() {
    let mut dag = Dag::new();
    let c1 = change(ID::new(1, 0), 1, Frontiers::new(), 1);
    let c2 = change(ID::new(1, 1), 2, Frontiers::from_id(ID::new(1, 0)), 1);
    let c3 = change(ID::new(1, 2), 3, Frontiers::from_id(ID::new(1, 1)), 1);

    dag.add_change(c1.clone());
    dag.add_change(c2.clone());
    dag.add_change(c3.clone());

    let vv1 = dag
      .frontiers_to_vv(&Frontiers::from_id(ID::new(1, 0)))
      .unwrap();
    let vv3 = dag
      .frontiers_to_vv(&Frontiers::from_id(ID::new(1, 2)))
      .unwrap();

    let diff = dag.diff_changes(&vv1, &vv3);
    assert_eq!(diff.len(), 2);
    assert_eq!(diff[0].id, ID::new(1, 1));
    assert_eq!(diff[1].id, ID::new(1, 2));
  }

  #[test]
  fn test_dag_diff_changes_concurrent() {
    let mut dag = Dag::new();
    let a = change(ID::new(1, 0), 1, Frontiers::new(), 1);
    let b = change(ID::new(2, 0), 2, Frontiers::new(), 1);
    let m = change(
      ID::new(1, 1),
      3,
      Frontiers::from(vec![ID::new(1, 0), ID::new(2, 0)]),
      1,
    );

    dag.add_change(a.clone());
    dag.add_change(b.clone());
    dag.add_change(m.clone());

    let vv_a = dag
      .frontiers_to_vv(&Frontiers::from_id(ID::new(1, 0)))
      .unwrap();
    let vv_m = dag
      .frontiers_to_vv(&Frontiers::from_id(ID::new(1, 1)))
      .unwrap();

    let diff = dag.diff_changes(&vv_a, &vv_m);
    // From a's view to m: need b and m.
    let ids: Vec<ID> = diff.iter().map(|c| c.id).collect();
    assert!(ids.contains(&ID::new(2, 0)));
    assert!(ids.contains(&ID::new(1, 1)));
  }
}
