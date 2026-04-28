//! Application-level causal DAG.
//!
//! [`AppDag`] maintains the causal graph of the app.
//! It's faster to answer questions like "what's the LCA version?"

use crate::core::change::Change;
use crate::core::dag::{Dag, DagNode};
use crate::rle::{HasLength, Sliceable};
use crate::types::{Counter, ID, Lamport, PeerID};
use crate::version::{Frontiers, ImVersionVector, VersionVector};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Deref;
use std::sync::{Arc, Mutex, OnceLock};

// ═══════════════════════════════════════════════════════════════════════════
// AppDagNodeInner
// ═══════════════════════════════════════════════════════════════════════════

/// Internal fields of an application-level DAG node.
///
/// Each node represents a contiguous run of operations from a single peer.
/// `has_succ` is set to `true` when another node depends on this one,
/// preventing RLE merge at the DAG level.
///
/// The `vv` field caches the version vector as an [`ImVersionVector`]
/// for cheap cloning.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct AppDagNodeInner {
  /// Peer that produced this run of operations.
  pub(crate) peer: PeerID,
  /// Starting counter (inclusive) of the run within the peer's history.
  pub(crate) cnt: Counter,
  /// Lamport timestamp of the first operation in this run.
  pub(crate) lamport: Lamport,
  /// Direct causal dependencies — the minimal set of IDs this node depends on.
  pub(crate) deps: Frontiers,
  /// Number of atomic operations in this run.
  pub(crate) len: usize,
  /// `true` when another node (from a different peer) depends on this one.
  /// Prevents RLE merge when a successor exists.
  pub(crate) has_succ: bool,
  /// Lazy cached version vector computed from this node's ancestors.
  pub(crate) vv: OnceLock<ImVersionVector>,
}

#[allow(dead_code)]
impl AppDagNodeInner {
  /// Creates a new `AppDagNodeInner`.
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
    ID::new(self.peer, self.last_counter())
  }

  /// The exclusive end [`ID`] (first ID after this node).
  #[inline]
  pub fn id_end(&self) -> ID {
    ID::new(self.peer, self.cnt + self.len as Counter)
  }

  /// The inclusive last counter.
  #[inline]
  pub fn last_counter(&self) -> Counter {
    self.end_counter() - 1
  }

  /// The exclusive end counter.
  #[inline]
  pub fn end_counter(&self) -> Counter {
    self.cnt + self.len as Counter
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
/// Wraps [`AppDagNodeInner`] via `Arc` so that cloning is O(1) and nodes can be
/// shared across iterators and caches.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct AppDagNode {
  pub(crate) inner: Arc<AppDagNodeInner>,
}

impl Deref for AppDagNode {
  type Target = AppDagNodeInner;

  fn deref(&self) -> &Self::Target {
    &self.inner
  }
}

#[allow(dead_code)]
impl AppDagNode {
  /// Wraps an existing [`AppDagNodeInner`] in an `Arc`.
  pub fn new(inner: AppDagNodeInner) -> Self {
    Self {
      inner: Arc::new(inner),
    }
  }

  /// Returns the cached version vector, computing it on first access.
  ///
  /// The VV is derived by merging the VVs of all dependency nodes and
  /// then adding this node's own ID range.
  pub fn vv<F>(&self, get_vv: F) -> &ImVersionVector
  where
    F: FnOnce(&Frontiers) -> ImVersionVector,
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
    debug_assert!(to > from, "slice requires to > from");
    let new_inner = AppDagNodeInner {
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

// ═══════════════════════════════════════════════════════════════════════════
// AppDag
// ═══════════════════════════════════════════════════════════════════════════

/// Application-level causal DAG.
///
/// Maintains a BTreeMap of [`AppDagNode`]s indexed by their start [`ID`],
/// together with the current frontier and version vector.
///
/// Application-level causal DAG.
///
/// Maintains a BTreeMap of [`AppDagNode`]s indexed by their start [`ID`],
/// together with the current frontier and version vector.
///
/// minus change-store lazy-loading and shallow-snapshot support.
#[allow(dead_code)]
#[derive(Debug)]
pub struct AppDag {
  /// It only contains nodes that are already parsed.
  map: Mutex<BTreeMap<ID, AppDagNode>>,
  /// The latest known frontiers.
  frontiers: Frontiers,
  /// The latest known version vector.
  vv: VersionVector,
  /// Ops included in the version vector but not parsed yet.
  ///
  /// # Invariants
  ///
  /// - `vv` >= `unparsed_vv`
  unparsed_vv: Mutex<VersionVector>,
  /// It's a set of points which are deps of some parsed ops.
  /// But the ops in this set are not parsed yet. When they are parsed,
  /// we need to make sure it breaks at the given point.
  unhandled_dep_points: Mutex<BTreeSet<ID>>,
  /// A temporary node representing the current local transaction while it is
  /// still being built.  `None` when no local txn is in progress.
  pending_txn_node: Option<AppDagNode>,
}

#[allow(dead_code)]
impl AppDag {
  /// Creates an empty `AppDag`.
  pub fn new() -> Self {
    Self {
      map: Mutex::new(BTreeMap::new()),
      frontiers: Frontiers::default(),
      vv: VersionVector::default(),
      unparsed_vv: Mutex::new(VersionVector::default()),
      unhandled_dep_points: Mutex::new(BTreeSet::new()),
      pending_txn_node: None,
    }
  }

  /// Current frontier — the minimal set of leaf IDs.
  pub fn frontiers(&self) -> &Frontiers {
    &self.frontiers
  }

  /// Current version vector — covers all nodes known to this DAG.
  pub fn vv(&self) -> &VersionVector {
    &self.vv
  }

  /// Returns `true` if the DAG contains no nodes.
  pub fn is_empty(&self) -> bool {
    self.vv.is_empty()
  }

  /// Returns `true` if the DAG contains a node whose counter range covers `id`.
  pub fn contains(&self, id: ID) -> bool {
    self.vv.includes_id(id)
  }

  // ── Insertion ────────────────────────────────────────────────────────────

  /// Inserts a new [`Change`] into the DAG.
  ///
  /// * `from_local` — `true` when the change originates from the local peer.
  ///   In that case the caller must have called [`start_local_txn`] first so
  ///   that `pending_txn_node` is set.
  pub fn handle_new_change<O>(&mut self, change: &Change<O>, from_local: bool)
  where
    O: crate::rle::Mergable
      + crate::rle::HasLength
      + crate::rle::HasIndex<Int = Counter>
      + std::fmt::Debug,
  {
    let len = change.content_len();
    self.update_version_on_new_change(change, from_local);

    let mut inserted = false;
    if change.deps_on_self() {
      inserted = self.with_last_mut_of_peer(change.id().peer, |last| {
        let last = last.unwrap();
        if last.has_succ {
          return false;
        }

        debug_assert_eq!(last.peer, change.id().peer);
        debug_assert_eq!(
          last.cnt + last.len as Counter,
          change.id().counter,
          "counter is not continuous"
        );
        debug_assert_eq!(
          last.lamport + last.len as Lamport,
          change.lamport(),
          "lamport is not continuous"
        );

        let inner = Arc::make_mut(&mut last.inner);
        inner.len = (change.id().counter - inner.cnt) as usize + len;
        inner.has_succ = false;
        true
      });
    }

    if !inserted {
      let node = AppDagNode::new(AppDagNodeInner {
        peer: change.id().peer,
        cnt: change.id().counter,
        lamport: change.lamport(),
        deps: change.deps().clone(),
        len,
        has_succ: false,
        vv: OnceLock::new(),
      });

      let mut map = self.map.lock().unwrap();
      map.insert(node.id_start(), node);
      self.handle_deps_break_points(change.deps().iter(), change.id().peer, Some(&mut map));
    }
  }

  /// Updates `frontiers`, `vv`, and `pending_txn_node` to reflect a new change.
  ///
  /// This is called by [`handle_new_change`] **before** the node is actually
  /// inserted into the map.  The distinction between local and remote changes
  /// matters because local changes have already updated their version state
  /// through [`update_version_on_new_local_op`].
  ///
  /// # Local changes (`from_local == true`)
  ///
  /// - `pending_txn_node` must have been set by `update_version_on_new_local_op`.
  ///   It is consumed here.
  /// - `vv` already contains the new change's range, so we only assert
  ///   continuity.
  ///
  /// # Remote changes (`from_local == false`)
  ///
  /// - `pending_txn_node` must be `None`.
  /// - The change's start counter must align with the current `vv` boundary.
  /// - We update `frontiers` (remove deps, add new last ID) and extend `vv`.
  fn update_version_on_new_change<O>(&mut self, change: &Change<O>, from_local: bool)
  where
    O: crate::rle::Mergable
      + crate::rle::HasLength
      + crate::rle::HasIndex<Int = Counter>
      + std::fmt::Debug,
  {
    if from_local {
      debug_assert!(
        self.pending_txn_node.take().is_some(),
        "pending_txn_node must be set before local change"
      );
      debug_assert_eq!(
        self.vv.get(change.id().peer).copied().unwrap_or(0),
        change.id_end().counter,
        "local change must be continuous with vv (did you forget to call update_version_on_new_local_op?)"
      );
    } else {
      let id_last = change.id_last();
      self
        .frontiers
        .update_frontiers_on_new_change(id_last, change.deps());
      debug_assert!(
        self.pending_txn_node.is_none(),
        "pending_txn_node must be None for remote change"
      );
      debug_assert_eq!(
        self.vv.get(change.id().peer).copied().unwrap_or(0),
        change.id().counter,
        "remote change must start at vv boundary"
      );
      self.vv.extend_to_include_last_id(id_last);
    }
  }

  /// Updates the DAG state for a new local operation (or batch of ops).
  ///
  /// This is called *before* `handle_new_change(..., from_local=true)` so that
  /// the version vector and frontiers already reflect the local edit.
  /// It also maintains `pending_txn_node` so consecutive local ops from the
  /// same peer can be merged into a single DAG node.
  pub fn update_version_on_new_local_op(
    &mut self,
    deps: &Frontiers,
    start_id: ID,
    start_lamport: Lamport,
    len: usize,
  ) {
    let last_id = start_id.inc(len as Counter - 1);
    self.vv.set_last(last_id);
    self.frontiers.update_frontiers_on_new_change(last_id, deps);
    match &mut self.pending_txn_node {
      Some(node) => {
        debug_assert!(
          node.peer == start_id.peer
            && node.cnt + node.len as Counter == start_id.counter
            && deps.len() == 1
            && deps.as_single().unwrap().peer == start_id.peer
        );
        let inner = Arc::make_mut(&mut node.inner);
        inner.len += len;
      }
      None => {
        let node = AppDagNode::new(AppDagNodeInner {
          peer: start_id.peer,
          cnt: start_id.counter,
          lamport: start_lamport,
          deps: deps.clone(),
          len,
          has_succ: false,
          vv: OnceLock::new(),
        });
        self.pending_txn_node = Some(node);
      }
    }
  }

  // ── Mutable access to last node of a peer ────────────────────────────────

  fn with_last_mut_of_peer<R>(
    &mut self,
    peer: PeerID,
    f: impl FnOnce(Option<&mut AppDagNode>) -> R,
  ) -> R {
    let key = ID::new(peer, Counter::MAX);
    let mut binding = self.map.lock().unwrap();
    let last = binding.range_mut(..=key).next_back().map(|(_, v)| v);
    f(last)
  }

  // ── Lookup ───────────────────────────────────────────────────────────────

  /// Looks up the node that covers the given `id`.
  ///
  /// Returns a cloned `AppDagNode` (O(1) because it is `Arc`-backed).
  pub fn get(&self, id: ID) -> Option<AppDagNode> {
    let binding = self.map.lock().unwrap();
    if let Some((_, node)) = binding.range(..=id).next_back()
      && node.contains_id(id)
    {
      return Some(node.clone());
    }

    if let Some(node) = &self.pending_txn_node
      && node.peer == id.peer
      && node.cnt <= id.counter
      && node.end_counter() > id.counter
    {
      return Some(node.clone());
    }

    None
  }

  /// Direct dependencies of a single operation.
  ///
  /// If `id` is at the start of its DAG node, returns the node's `deps`.
  /// Otherwise returns the immediately preceding ID on the same peer.
  pub fn find_deps_of_id(&self, id: ID) -> Frontiers {
    let Some(node) = self.get(id) else {
      return Frontiers::default();
    };

    let offset = id.counter - node.cnt;
    if offset == 0 {
      node.deps.clone()
    } else {
      Frontiers::from_id(ID::new(id.peer, node.cnt + offset - 1))
    }
  }

  /// Lamport timestamp of a single operation.
  pub fn get_lamport(&self, id: &ID) -> Option<Lamport> {
    self.get(*id).and_then(|node| {
      debug_assert!(id.counter >= node.cnt);
      if node.end_counter() > id.counter {
        Some(node.lamport + (id.counter - node.cnt) as Lamport)
      } else {
        None
      }
    })
  }

  /// Computes the lamport a change should use given its dependencies.
  pub fn get_change_lamport_from_deps(&self, deps: &Frontiers) -> Option<Lamport> {
    let mut lamport = 0;
    for id in deps.iter() {
      let l = self.get_lamport(&id)?;
      lamport = lamport.max(l + 1);
    }
    Some(lamport)
  }

  // ── Version vector (with lazy caching) ───────────────────────────────────

  /// Returns the version vector **at** the given operation (i.e. including it).
  pub fn get_vv(&self, id: ID) -> Option<VersionVector> {
    self.get(id).map(|node| {
      let mut vv = self.ensure_vv_for(&node);
      vv.set_last(id);
      vv.to_vv()
    })
  }

  /// Ensures that `target_node`'s VV cache is populated and returns it.
  ///
  /// The version vector of a node is the merge of the VVs of all its
  /// dependencies, plus the node's own last ID.  Because deps form a DAG,
  /// a node's VV may depend on other nodes whose VVs are not yet cached.
  /// This method uses an explicit stack to resolve those dependencies
  /// bottom-up, computing missing VVs on demand and caching them in
  /// `node.inner.vv` via the shared `Arc`.
  ///
  /// # Algorithm
  ///
  /// 1. Push the target node onto the stack.
  /// 2. Pop a node. If any of its deps has an empty VV cache, push the
  ///    current node back (to retry later) and push the uncached dep.
  /// 3. Once all deps are cached, merge their VVs and store the result.
  /// 4. Repeat until the stack is empty.
  ///
  /// The algorithm is an explicit stack traversal of the dependency DAG.
  pub(crate) fn ensure_vv_for(&self, target_node: &AppDagNode) -> ImVersionVector {
    // Fast path: the target already has a cached VV.
    if target_node.inner.vv.get().is_none() {
      // Stack of nodes whose VVs need to be computed.
      // Nodes may be pushed multiple times if their deps are not yet ready.
      let mut stack: smallvec::SmallVec<[AppDagNode; 4]> = smallvec::smallvec![target_node.clone()];
      while let Some(top_node) = stack.pop() {
        let mut ans_vv = ImVersionVector::new();

        // Collect all dependency IDs.
        let deps: Vec<ID> = top_node.deps.iter().collect();
        if deps.is_empty() {
          // Root node — nothing precedes it, so its VV is empty.
        } else {
          // First pass: ensure every dependency already has a cached VV.
          // If a dep is missing its cache, push it onto the stack and
          // defer the current node until the dep is resolved.
          let mut all_deps_processed = true;
          for &id in &deps {
            let dep_node = self.get(id).expect("deps should be in the dag");
            if dep_node.inner.vv.get().is_none() {
              // Defer the current node so we can compute the dep first.
              if all_deps_processed {
                stack.push(top_node.clone());
              }
              all_deps_processed = false;
              stack.push(dep_node);
            }
          }

          if !all_deps_processed {
            // Some deps still need their VVs computed.
            // Skip to the next stack iteration.
            continue;
          }

          // Second pass: all deps are cached — merge their VVs.
          for &id in &deps {
            let dep_node = self.get(id).expect("deps should be in the dag");
            let dep_vv = dep_node.inner.vv.get().unwrap();
            if ans_vv.is_empty() {
              ans_vv = dep_vv.clone();
            } else {
              ans_vv.extend_to_include_vv(dep_vv.iter());
            }
            // Include the dep node itself (its last ID) in the VV.
            ans_vv.set_last(dep_node.id_last());
          }
        }

        // Store the computed VV into the node's cache.
        // Because `AppDagNode` uses `Arc<AppDagNodeInner>`, clones of the
        // same node share this cache — future lookups will hit it directly.
        // `set` may fail if another thread raced, but we are single-threaded here.
        let _ = top_node.inner.vv.set(ans_vv);
      }
    }

    target_node.inner.vv.get().unwrap().clone()
  }

  // ── Frontiers <-> VV conversion ──────────────────────────────────────────

  /// Converts a set of frontiers into a version vector.
  ///
  /// Returns `None` if any frontier ID is not found in the DAG.
  pub fn frontiers_to_vv(&self, frontiers: &Frontiers) -> Option<VersionVector> {
    let mut vv: VersionVector = Default::default();
    for id in frontiers.iter() {
      let node = self.get(id)?;
      let target_vv = self.ensure_vv_for(&node);
      vv.extend_to_include_vv(target_vv.iter());
      vv.extend_to_include_last_id(id);
    }
    Some(vv)
  }

  /// Converts a version vector into the minimal frontiers representation.
  ///
  /// This is the inverse of [`frontiers_to_vv`] **up to shrinking**:
  /// some IDs that are ancestors of others are removed.
  pub fn vv_to_frontiers(&self, vv: &VersionVector) -> Frontiers {
    if vv.is_empty() {
      return Frontiers::default();
    }

    let last_ids: Frontiers = vv
      .iter()
      .filter_map(|(&peer, &cnt)| {
        if cnt == 0 {
          return None;
        }
        Some(ID::new(peer, cnt - 1))
      })
      .collect();

    self.shrink_frontiers(&last_ids)
  }

  /// Shrinks a set of last IDs to the minimal frontiers.
  ///
  /// An ID is removed if it is an ancestor of another ID in the set.
  fn shrink_frontiers(&self, last_ids: &Frontiers) -> Frontiers {
    if last_ids.len() <= 1 {
      return last_ids.clone();
    }

    let ids: Vec<ID> = last_ids.iter().collect();
    let mut result = Vec::with_capacity(ids.len());

    // For each candidate, check whether any *other* candidate's VV includes it.
    // If so, it is an ancestor and can be dropped.
    let vvs: Vec<Option<VersionVector>> = ids.iter().map(|&id| self.get_vv(id)).collect();

    for (i, &id) in ids.iter().enumerate() {
      let mut is_ancestor = false;
      for (j, other_vv) in vvs.iter().enumerate() {
        if i == j {
          continue;
        }
        if let Some(other_vv) = other_vv
          && other_vv.includes_id(id)
        {
          is_ancestor = true;
          break;
        }
      }
      if !is_ancestor {
        result.push(id);
      }
    }

    result.into()
  }

  // ── Causal comparison ────────────────────────────────────────────────────

  /// Compare the causal order of two operations.
  ///
  /// Returns `None` when they are concurrent.
  pub fn cmp_version(&self, a: ID, b: ID) -> Option<Ordering> {
    if a.peer == b.peer {
      return Some(a.counter.cmp(&b.counter));
    }

    let a_vv = self.get_vv(a)?;
    let b_vv = self.get_vv(b)?;
    a_vv.partial_cmp(&b_vv)
  }

  /// Compare this DAG's current frontiers with another frontiers set.
  ///
  /// - `Ordering::Equal` — identical frontiers.
  /// - `Ordering::Greater` — this DAG's version includes `other`.
  /// - `Ordering::Less` — `other` includes this DAG, or they are concurrent
  ///   (this is a coarse comparison used for fast paths).
  pub fn cmp_with_frontiers(&self, other: &Frontiers) -> Ordering {
    if &self.frontiers == other {
      Ordering::Equal
    } else if other.iter().all(|id| self.vv.includes_id(id)) {
      Ordering::Greater
    } else {
      Ordering::Less
    }
  }

  // ── Internal helpers ─────────────────────────────────────────────────────

  /// When a new node is inserted, ensure that every dependency points to the
  /// *end* of a DAG node.  If a dependency points to the middle of a node,
  /// split that node so the invariant holds.
  ///
  /// Also marks the targeted node(s) as having successors.
  fn handle_deps_break_points(
    &self,
    ids: impl IntoIterator<Item = ID>,
    skip_peer: PeerID,
    map_input: Option<&mut BTreeMap<ID, AppDagNode>>,
  ) {
    let mut map_guard = None;
    let map = map_input.unwrap_or_else(|| {
      map_guard = Some(self.map.lock().unwrap());
      map_guard.as_mut().unwrap()
    });
    for id in ids {
      if id.peer == skip_peer {
        continue;
      }

      let mut handled = false;
      let x = map.range_mut(..=id).next_back();
      if let Some((_, target)) = x
        && target.contains_id(id)
      {
        if target.last_counter() == id.counter {
          // Dependency points to the last ID of the node — just mark has_succ.
          let inner = Arc::make_mut(&mut target.inner);
          inner.has_succ = true;
          handled = true;
        } else {
          // Dependency points to the middle — split the node.
          let new_node = target.slice((id.counter - target.cnt) as usize + 1, target.len);
          {
            let inner = Arc::make_mut(&mut target.inner);
            inner.len -= new_node.len;
            inner.has_succ = true;
          }
          map.insert(new_node.id_start(), new_node);
          handled = true;
        }
      }

      if !handled {
        self.unhandled_dep_points.lock().unwrap().insert(id);
      }
    }
  }
}

impl Default for AppDag {
  fn default() -> Self {
    Self::new()
  }
}

impl Dag for AppDag {
  type Node = AppDagNode;

  fn get(&self, id: ID) -> Option<Self::Node> {
    self.get(id)
  }

  fn frontier(&self) -> &Frontiers {
    &self.frontiers
  }

  fn vv(&self) -> &VersionVector {
    &self.vv
  }

  fn contains(&self, id: ID) -> bool {
    self.contains(id)
  }
}

#[test]
fn test_dag_node_inner_basic() {
  let inner = AppDagNodeInner::new(1, 0, 5, Frontiers::from_id(ID::new(0, 0)), 3);
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
  let inner = AppDagNodeInner::new(1, 5, 10, Frontiers::from_id(ID::new(0, 0)), 2);
  let node = AppDagNode::new(inner);

  assert_eq!(node.id_start(), ID::new(1, 5));
  assert_eq!(node.lamport(), 10);
  assert_eq!(node.deps(), &Frontiers::from_id(ID::new(0, 0)));
  assert_eq!(node.len(), 2);
}

#[test]
fn test_app_dag_node_sliceable() {
  let inner = AppDagNodeInner::new(1, 0, 5, Frontiers::from_id(ID::new(0, 0)), 4);
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
  let inner = AppDagNodeInner::new(1, 0, 5, Frontiers::new(), 3);
  let node = AppDagNode::new(inner);

  // First access computes the VV via the closure.
  let vv = node.vv(|deps| {
    assert!(deps.is_empty());
    ImVersionVector::new()
  });
  assert_eq!(vv.get(&1).copied(), Some(3)); // exclusive end = cnt + len = 0 + 3

  // Second access returns the cached value (closure is not called again).
  let vv2 = node.vv(|_| panic!("should not be called — cached"));
  assert_eq!(vv2.get(&1).copied(), Some(3));
}

#[test]
fn test_app_dag_node_vv_with_deps() {
  let inner = AppDagNodeInner::new(2, 0, 8, Frontiers::from_id(ID::new(1, 2)), 2);
  let node = AppDagNode::new(inner);

  let vv = node.vv(|deps| {
    let mut base = ImVersionVector::new();
    for id in deps.iter() {
      base.set_last(id);
    }
    base
  });

  assert_eq!(vv.get(&1).copied(), Some(3)); // from deps: peer 1, counter 2 -> exclusive end 3
  assert_eq!(vv.get(&2).copied(), Some(2)); // from self: peer 2, cnt 0, len 2 -> exclusive end 2
}

#[test]
fn test_app_dag_node_invalidate_vv() {
  let inner = AppDagNodeInner::new(1, 0, 5, Frontiers::new(), 1);
  let mut node = AppDagNode::new(inner);

  let vv1 = node.vv(|_| ImVersionVector::new());
  assert_eq!(vv1.get(&1).copied(), Some(1));

  // Cached: closure should NOT be called again.
  let vv2 = node.vv(|_| panic!("should not be called — cached"));
  assert_eq!(vv2.get(&1).copied(), Some(1));

  node.invalidate_vv();

  // After invalidate, closure IS called again and yields the same VV.
  let vv3 = node.vv(|_| ImVersionVector::new());
  assert_eq!(vv3.get(&1).copied(), Some(1));
}

#[cfg(test)]
mod app_dag_tests {
  use super::*;
  use crate::core::change::Change;
  use crate::memory::arena::InnerArena;
  use crate::op::{Op, OpContent};
  use crate::rle::RleVec;
  use crate::types::{ContainerID, ContainerType, ID};
  use crate::version::Frontiers;

  fn make_change(
    peer: PeerID,
    counter: Counter,
    lamport: Lamport,
    deps: Frontiers,
    op_count: usize,
  ) -> Change {
    let arena = InnerArena::new();
    let container = arena.register(&ContainerID::new_root("c", ContainerType::Counter));
    let ops = RleVec::from(
      (0..op_count)
        .map(|i| Op::new(counter + i as Counter, container, OpContent::Counter(1.0)))
        .collect::<Vec<_>>(),
    );
    Change::new(ops, deps, ID::new(peer, counter), lamport, 1_700_000_000)
  }

  #[test]
  fn test_app_dag_linear_merge() {
    let mut dag = AppDag::new();

    // Peer 1 creates two consecutive changes that depend on themselves.
    let c1 = make_change(1, 0, 1, Frontiers::new(), 2);
    let c2 = make_change(1, 2, 3, Frontiers::from_id(ID::new(1, 1)), 3);

    dag.handle_new_change(&c1, false);
    dag.handle_new_change(&c2, false);

    // Both changes should be merged into a single node because they are
    // consecutive, have the same deps-on-self semantics, and no successor.
    assert_eq!(dag.map.lock().unwrap().len(), 1);
    let node = dag.get(ID::new(1, 0)).unwrap();
    assert_eq!(node.cnt, 0);
    assert_eq!(node.len, 5); // 2 + 3
    assert_eq!(node.lamport, 1);
    assert!(!node.has_succ);

    // Frontier should be the last op of the merged node.
    assert_eq!(dag.frontiers, Frontiers::from_id(ID::new(1, 4)));
    assert_eq!(dag.vv.get(1).copied(), Some(5));
  }

  #[test]
  fn test_app_dag_fork_merge() {
    let mut dag = AppDag::new();

    // Peer 1: change A@0..2 (lamport 1)
    let c1 = make_change(1, 0, 1, Frontiers::new(), 2);
    dag.handle_new_change(&c1, false);

    // Peer 2: change B@0..1, depends on A@1 (lamport 2)
    let c2 = make_change(2, 0, 2, Frontiers::from_id(ID::new(1, 1)), 1);
    dag.handle_new_change(&c2, false);

    // Peer 1: change A@2..3, depends on A@1 (lamport 3)
    // This CANNOT merge with the first node because peer 2 now depends on A@1.
    let c3 = make_change(1, 2, 3, Frontiers::from_id(ID::new(1, 1)), 1);
    dag.handle_new_change(&c3, false);

    // Now there should be 3 nodes.
    assert_eq!(dag.map.lock().unwrap().len(), 3);

    // The first node should have has_succ = true because peer 2 depends on it.
    let node1 = dag.get(ID::new(1, 0)).unwrap();
    assert_eq!(node1.len, 2);
    assert!(node1.has_succ);

    // Peer 2's node.
    let node2 = dag.get(ID::new(2, 0)).unwrap();
    assert_eq!(node2.len, 1);

    // Peer 1's second node.
    let node3 = dag.get(ID::new(1, 2)).unwrap();
    assert_eq!(node3.len, 1);

    // Frontiers should be B@0 and A@2.
    assert_eq!(dag.frontiers.len(), 2);
    assert!(dag.frontiers.contains(&ID::new(2, 0)));
    assert!(dag.frontiers.contains(&ID::new(1, 2)));
  }

  #[test]
  fn test_app_dag_get_vv() {
    let mut dag = AppDag::new();

    // Linear history: 1@0..2 -> 1@2..4
    let c1 = make_change(1, 0, 1, Frontiers::new(), 2);
    let c2 = make_change(1, 2, 3, Frontiers::from_id(ID::new(1, 1)), 2);
    dag.handle_new_change(&c1, false);
    dag.handle_new_change(&c2, false);

    let vv0 = dag.get_vv(ID::new(1, 0)).unwrap();
    assert_eq!(vv0.get(1).copied(), Some(1));

    let vv1 = dag.get_vv(ID::new(1, 1)).unwrap();
    assert_eq!(vv1.get(1).copied(), Some(2));

    let vv3 = dag.get_vv(ID::new(1, 3)).unwrap();
    assert_eq!(vv3.get(1).copied(), Some(4));

    // Concurrent branch
    let c3 = make_change(2, 0, 5, Frontiers::from_id(ID::new(1, 3)), 1);
    dag.handle_new_change(&c3, false);

    let vv_peer2 = dag.get_vv(ID::new(2, 0)).unwrap();
    assert_eq!(vv_peer2.get(1).copied(), Some(4));
    assert_eq!(vv_peer2.get(2).copied(), Some(1));
  }

  #[test]
  fn test_app_dag_frontiers_to_vv_roundtrip() {
    let mut dag = AppDag::new();

    // Build a small DAG:
    //   1@0..2  ->  2@0..1 (depends on 1@1)
    //        \
    //         ->  1@2..3 (depends on 1@1)
    let c1 = make_change(1, 0, 1, Frontiers::new(), 2);
    let c2 = make_change(2, 0, 2, Frontiers::from_id(ID::new(1, 1)), 1);
    let c3 = make_change(1, 2, 3, Frontiers::from_id(ID::new(1, 1)), 1);
    dag.handle_new_change(&c1, false);
    dag.handle_new_change(&c2, false);
    dag.handle_new_change(&c3, false);

    let vv = dag.frontiers_to_vv(&dag.frontiers).unwrap();
    let frontiers_back = dag.vv_to_frontiers(&vv);

    // After round-trip the frontiers may be shrunk, but they must represent
    // the same version.
    let vv_back = dag.frontiers_to_vv(&frontiers_back).unwrap();
    assert_eq!(vv, vv_back);
  }

  #[test]
  fn test_app_dag_cmp_version() {
    let mut dag = AppDag::new();

    let c1 = make_change(1, 0, 1, Frontiers::new(), 2);
    let c2 = make_change(2, 0, 2, Frontiers::from_id(ID::new(1, 1)), 1);
    dag.handle_new_change(&c1, false);
    dag.handle_new_change(&c2, false);

    // Same peer — compare by counter.
    assert_eq!(
      dag.cmp_version(ID::new(1, 0), ID::new(1, 1)),
      Some(Ordering::Less)
    );
    assert_eq!(
      dag.cmp_version(ID::new(1, 1), ID::new(1, 0)),
      Some(Ordering::Greater)
    );

    // 1@0 is ancestor of 2@0.
    assert_eq!(
      dag.cmp_version(ID::new(1, 0), ID::new(2, 0)),
      Some(Ordering::Less)
    );
    assert_eq!(
      dag.cmp_version(ID::new(2, 0), ID::new(1, 0)),
      Some(Ordering::Greater)
    );

    // 1@1 is ancestor of 2@0 (because 2@0 depends on 1@1).
    assert_eq!(
      dag.cmp_version(ID::new(1, 1), ID::new(2, 0)),
      Some(Ordering::Less)
    );
    assert_eq!(
      dag.cmp_version(ID::new(2, 0), ID::new(1, 1)),
      Some(Ordering::Greater)
    );
  }

  #[test]
  fn test_app_dag_find_deps_of_id() {
    let mut dag = AppDag::new();

    let c1 = make_change(1, 0, 1, Frontiers::new(), 2);
    let c2 = make_change(1, 2, 3, Frontiers::from_id(ID::new(1, 1)), 1);
    dag.handle_new_change(&c1, false);
    dag.handle_new_change(&c2, false);

    // First op of first node — deps are the node's deps (empty).
    assert_eq!(dag.find_deps_of_id(ID::new(1, 0)), Frontiers::new());

    // Second op of first node — depends on the previous op in the same node.
    assert_eq!(
      dag.find_deps_of_id(ID::new(1, 1)),
      Frontiers::from_id(ID::new(1, 0))
    );

    // First op of second node — deps are the node's deps.
    assert_eq!(
      dag.find_deps_of_id(ID::new(1, 2)),
      Frontiers::from_id(ID::new(1, 1))
    );
  }

  #[test]
  fn test_app_dag_get_lamport() {
    let mut dag = AppDag::new();

    let c1 = make_change(1, 0, 10, Frontiers::new(), 3);
    dag.handle_new_change(&c1, false);

    assert_eq!(dag.get_lamport(&ID::new(1, 0)), Some(10));
    assert_eq!(dag.get_lamport(&ID::new(1, 1)), Some(11));
    assert_eq!(dag.get_lamport(&ID::new(1, 2)), Some(12));
    assert_eq!(dag.get_lamport(&ID::new(1, 3)), None);
  }

  #[test]
  fn test_app_dag_local_txn() {
    let mut dag = AppDag::new();

    // First local op batch: peer 1, id 0..2, deps empty.
    dag.update_version_on_new_local_op(&Frontiers::new(), ID::new(1, 0), 1, 2);
    assert!(dag.pending_txn_node.is_some());
    assert_eq!(dag.vv.get(1).copied(), Some(2));

    // Commit the first local change.
    let c1 = make_change(1, 0, 1, Frontiers::new(), 2);
    dag.handle_new_change(&c1, true);
    assert!(dag.pending_txn_node.is_none());

    // Second local op batch: peer 1, id 2..3, deps-on-self.
    dag.update_version_on_new_local_op(&Frontiers::from_id(ID::new(1, 1)), ID::new(1, 2), 3, 1);

    // Commit the second local change.
    let c2 = make_change(1, 2, 3, Frontiers::from_id(ID::new(1, 1)), 1);
    dag.handle_new_change(&c2, true);

    // Both changes should be merged into a single node.
    assert_eq!(dag.map.lock().unwrap().len(), 1);
    assert_eq!(dag.get(ID::new(1, 0)).unwrap().len, 3);
  }

  // ── ensure_vv_for / get_vv tests ─────────────────────────────────────────

  #[test]
  fn test_ensure_vv_root_node() {
    let mut dag = AppDag::new();
    let c1 = make_change(1, 0, 1, Frontiers::new(), 1);
    dag.handle_new_change(&c1, false);

    let node = dag.get(ID::new(1, 0)).unwrap();
    let vv = dag.ensure_vv_for(&node);
    assert!(vv.is_empty());
  }

  #[test]
  fn test_ensure_vv_linear_history() {
    let mut dag = AppDag::new();
    // Peer 1: 0..1 (deps empty) → 1..3 (deps [1@0])
    let c1 = make_change(1, 0, 1, Frontiers::new(), 1);
    let c2 = make_change(1, 1, 2, Frontiers::from_id(ID::new(1, 0)), 2);
    dag.handle_new_change(&c1, false);
    dag.handle_new_change(&c2, false);

    // VV of peer 1 at counter 2 should include peer 1 up to counter 3.
    let vv = dag.get_vv(ID::new(1, 2)).unwrap();
    assert_eq!(vv.get(1).copied(), Some(3)); // exclusive end = 0 + 3
  }

  #[test]
  fn test_ensure_vv_fork_merge() {
    let mut dag = AppDag::new();

    // Peer 1: 0..2
    let c1 = make_change(1, 0, 1, Frontiers::new(), 2);
    dag.handle_new_change(&c1, false);

    // Peer 2: 0..1, depends on 1@1
    let c2 = make_change(2, 0, 2, Frontiers::from_id(ID::new(1, 1)), 1);
    dag.handle_new_change(&c2, false);

    // Peer 1: 2..3, depends on 1@1 (forks from same dep as peer 2)
    let c3 = make_change(1, 2, 3, Frontiers::from_id(ID::new(1, 1)), 1);
    dag.handle_new_change(&c3, false);

    // VV of peer 2's only op should include both peer 1 and peer 2.
    let vv = dag.get_vv(ID::new(2, 0)).unwrap();
    assert_eq!(vv.get(1).copied(), Some(2)); // peer 1 up to counter 2
    assert_eq!(vv.get(2).copied(), Some(1)); // peer 2 up to counter 1

    // VV of peer 1's third op.
    // c3 depends only on 1@1, not on peer 2, so its VV does NOT include peer 2.
    let vv = dag.get_vv(ID::new(1, 2)).unwrap();
    assert_eq!(vv.get(1).copied(), Some(3)); // peer 1 up to counter 3
    assert_eq!(vv.get(2).copied(), None); // peer 2 is NOT an ancestor
  }

  #[test]
  fn test_ensure_vv_caches_once() {
    let mut dag = AppDag::new();

    // Build a two-peer DAG so that ensure_vv_for must traverse deps.
    // Peer 1: 0..1 (root)
    // Peer 2: 0..1, depends on 1@0
    let c1 = make_change(1, 0, 1, Frontiers::new(), 1);
    let c2 = make_change(2, 0, 2, Frontiers::from_id(ID::new(1, 0)), 1);
    dag.handle_new_change(&c1, false);
    dag.handle_new_change(&c2, false);

    let node = dag.get(ID::new(2, 0)).unwrap();

    // Before the first call, the cache is empty.
    assert!(node.inner.vv.get().is_none());

    // First call computes and caches.
    let vv1 = dag.ensure_vv_for(&node);
    // ensure_vv_for returns the VV *without* the node itself.
    // For peer 2's node, its VV is the merged VV of its deps (peer 1 up to 1).
    assert_eq!(vv1.get(&1).copied(), Some(1));

    // After the first call, the cache is populated.
    assert!(node.inner.vv.get().is_some());

    // Second call must return the cached value (no re-computation).
    let vv2 = dag.ensure_vv_for(&node);
    assert_eq!(vv2.get(&1).copied(), Some(1));
    // Both calls return references to the same cached ImVersionVector
    // because AppDagNode shares the Arc-backed inner.
    assert!(std::ptr::eq(
      node.inner.vv.get().unwrap(),
      dag.get(ID::new(2, 0)).unwrap().inner.vv.get().unwrap()
    ));
  }
}
