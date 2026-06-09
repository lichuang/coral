use crate::common::{CoralError, CoralResult};
use crate::core::Commit;
use crate::encoding::JsonSchema;
use crate::object::{CounterRef, ObjectRegistry, ObjectState};
use crate::operation::{Cmd, Op};
use crate::types::{Counter, ObjectId, ObjectIndex, ObjectType, PeerID};
use crate::version::VersionVector;
use rustc_hash::FxHashMap;

use rand::Rng;

use super::{CausalGraph, CommitBuilder, CommitStore};

/// The internal state of a collaborative document.
///
/// `DocInner` holds the actual CRDT state: the causal graph, the commit
/// history, and all container states. It is wrapped by
/// [`Document`](crate::Document) which provides the public API.
pub struct DocInner {
  peer_id: PeerID,
  next_counter: Counter,
  registry: ObjectRegistry,
  causal_graph: CausalGraph,
  states: FxHashMap<ObjectIndex, ObjectState>,
  commit_store: CommitStore,
  commit_builder: Option<CommitBuilder>,
}

impl std::fmt::Debug for DocInner {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("DocInner")
      .field("peer_id", &self.peer_id)
      .field("next_counter", &self.next_counter)
      .field("registry", &self.registry)
      .field("causal_graph", &self.causal_graph)
      .finish()
  }
}

impl Default for DocInner {
  fn default() -> Self {
    Self::new()
  }
}

impl DocInner {
  /// Creates a new `DocInner` with a randomly generated peer ID.
  pub fn new() -> Self {
    let peer_id = rand::rng().random();
    Self {
      peer_id,
      next_counter: 0,
      registry: ObjectRegistry::new(),
      causal_graph: CausalGraph::new(),
      states: FxHashMap::default(),
      commit_store: CommitStore::new(),
      commit_builder: None,
    }
  }

  pub fn peer_id(&self) -> PeerID {
    self.peer_id
  }

  pub fn causal_graph(&self) -> &CausalGraph {
    &self.causal_graph
  }

  pub fn causal_graph_mut(&mut self) -> &mut CausalGraph {
    &mut self.causal_graph
  }

  pub fn commit_store(&self) -> &CommitStore {
    &self.commit_store
  }

  pub fn iter_commits_in_range<F>(&self, start_vv: &VersionVector, end_vv: &VersionVector, f: F)
  where
    F: FnMut(&Commit),
  {
    let diff = end_vv.diff_from(start_vv);
    self.commit_store.iter_diff(&diff, f);
  }

  pub fn state(&self, index: ObjectIndex) -> Option<&ObjectState> {
    self.states.get(&index)
  }

  pub fn state_mut(&mut self, index: ObjectIndex) -> &mut ObjectState {
    self.states.entry(index).or_default()
  }

  pub fn registry(&self) -> &ObjectRegistry {
    &self.registry
  }

  fn alloc_counter(&mut self) -> Counter {
    let c = self.next_counter;
    self.next_counter += 1;
    c
  }

  fn ensure_pending(&mut self) {
    if self.commit_builder.is_none() {
      let lamport = self.causal_graph.calc_next_lamport();
      let deps = self.causal_graph.heads().clone();
      self.commit_builder = Some(CommitBuilder::new(self.peer_id, lamport, deps));
    }
  }

  pub fn push_local_op(&mut self, index: ObjectIndex, cmd: Cmd) -> CoralResult<()> {
    self.ensure_pending();
    let counter = self.alloc_counter();
    let op = Op::new(counter, index, cmd);
    self.state_mut(index).apply(&op)?;
    self.commit_builder.as_mut().unwrap().push_op(op);
    Ok(())
  }

  pub fn commit(&mut self) {
    if let Some(builder) = self.commit_builder.take()
      && let Some(commit) = builder.into_commit()
    {
      #[cfg(debug_assertions)]
      commit.assert_contiguous();
      self.causal_graph.insert(&commit);
      self.commit_store.insert(commit);
    }
  }

  pub fn get_counter(&mut self, name: &str) -> CoralResult<CounterRef<'_>> {
    if let Some(index) = self.registry.get_root(name) {
      let typ = index.typ()?;
      if typ != ObjectType::Counter {
        return Err(CoralError::TypeMismatch {
          expected: "Counter".to_string(),
          actual: typ.to_string(),
        });
      }
      return Ok(CounterRef::new(self, index));
    }

    let id = ObjectId::Root {
      name: name.to_string(),
      typ: ObjectType::Counter,
    };
    let index = self.registry.alloc_root(name.to_string(), id);
    Ok(CounterRef::new(self, index))
  }

  fn ensure_container(&mut self, name: &str, typ: ObjectType) -> CoralResult<ObjectIndex> {
    if let Some(index) = self.registry.get_root(name) {
      let existing_typ = index.typ()?;
      if existing_typ != typ {
        return Err(CoralError::TypeMismatch {
          expected: existing_typ.to_string(),
          actual: typ.to_string(),
        });
      }
      return Ok(index);
    }
    let id = ObjectId::Root {
      name: name.to_string(),
      typ,
    };
    Ok(self.registry.alloc_root(name.to_string(), id))
  }

  fn import_commit(&mut self, commit: Commit) -> CoralResult<()> {
    if commit.id.peer == self.peer_id {
      return Err(CoralError::InvalidImport("local commit".into()));
    }
    if commit.ops.is_empty() {
      return Err(CoralError::InvalidImport("empty commit".into()));
    }
    #[cfg(debug_assertions)]
    commit.assert_contiguous();

    for op in commit.ops.iter() {
      self.state_mut(op.container).apply(op)?;
    }

    self.causal_graph.insert(&commit);
    self.commit_store.insert(commit);
    Ok(())
  }

  pub fn import_json(&mut self, json: &str) -> CoralResult<()> {
    let schema: JsonSchema = serde_json::from_str(json)
      .map_err(|e| CoralError::InvalidImport(format!("json parse: {e}")))?;

    for jc in schema.commits {
      let commit =
        Commit::from_json_commit(jc, &mut |name, id| self.ensure_container(name, id.typ()))?;
      self.import_commit(commit)?;
    }

    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::Document;
  use crate::types::OpId;

  fn collect_commits(
    doc: &Document,
    start_vv: &VersionVector,
    end_vv: &VersionVector,
  ) -> Vec<Commit> {
    let mut result = Vec::new();
    doc.iter_commits_in_range(start_vv, end_vv, |c| result.push(c.clone()));
    result
  }

  #[test]
  fn test_iter_commits_in_range_full() {
    let mut doc = Document::new();

    {
      let mut counter = doc.get_counter("c").unwrap();
      counter.increment(1.0).unwrap();
      counter.increment(1.0).unwrap();
    }
    doc.commit();

    {
      let mut counter = doc.get_counter("c").unwrap();
      counter.increment(1.0).unwrap();
    }
    doc.commit();

    let start_vv = VersionVector::new();
    let end_vv = doc.causal_graph().vv().clone();
    let result = collect_commits(&doc, &start_vv, &end_vv);
    assert_eq!(result.len(), 2);
  }

  #[test]
  fn test_iter_commits_in_range_partial() {
    let mut doc = Document::new();
    let peer = doc.peer_id();

    // commit A: ops [0, 3)
    {
      let mut counter = doc.get_counter("c").unwrap();
      counter.increment(1.0).unwrap();
      counter.increment(1.0).unwrap();
      counter.increment(1.0).unwrap();
    }
    doc.commit();

    // commit B: ops [3, 5)
    {
      let mut counter = doc.get_counter("c").unwrap();
      counter.increment(1.0).unwrap();
      counter.increment(1.0).unwrap();
    }
    doc.commit();

    // iterate [0, 3) — should only get commit A
    let start_vv = VersionVector::new();
    let mut end_vv = VersionVector::new();
    end_vv.insert(peer, 3);

    let result = collect_commits(&doc, &start_vv, &end_vv);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].id, OpId::new(peer, 0));
    assert_eq!(result[0].end_counter(), 3);
  }

  #[test]
  fn test_iter_commits_in_range_empty_diff() {
    let mut doc = Document::new();
    let peer = doc.peer_id();

    {
      let mut counter = doc.get_counter("c").unwrap();
      counter.increment(1.0).unwrap();
    }
    doc.commit();

    let mut vv = VersionVector::new();
    vv.insert(peer, 1);
    let mut count = 0;
    doc.iter_commits_in_range(&vv, &vv, |_| count += 1);
    assert_eq!(count, 0);
  }

  #[test]
  fn test_iter_commits_in_range_end_equals_current() {
    let mut doc = Document::new();
    let peer = doc.peer_id();

    {
      let mut counter = doc.get_counter("c").unwrap();
      counter.increment(1.0).unwrap();
      counter.increment(1.0).unwrap();
      counter.increment(1.0).unwrap();
    }
    doc.commit();

    let mut start_vv = VersionVector::new();
    start_vv.insert(peer, 1);
    let end_vv = doc.causal_graph().vv().clone();

    let result = collect_commits(&doc, &start_vv, &end_vv);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].id, OpId::new(peer, 0));
  }
}
