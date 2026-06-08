use crate::common::{CoralError, CoralResult};
use crate::object::{CounterRef, ObjectRegistry, ObjectState};
use crate::operation::{Cmd, Op};
use crate::types::{Counter, ObjectId, ObjectIndex, ObjectType, PeerID};
use rustc_hash::FxHashMap;

use rand::Rng;

use super::{CausalGraph, CommitBuilder, History};

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
  history: History,
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
      history: History::new(),
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

  pub fn history(&self) -> &History {
    &self.history
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
      self.causal_graph.insert(&commit);
      self.history.push(commit);
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
}
