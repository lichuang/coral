use crate::common::{CoralError, CoralResult};
use crate::types::{Counter, ObjectId, ObjectType, PeerID};
use rustc_hash::FxHashMap;

use rand::Rng;

use super::{CausalGraph, Commit, History};
use crate::object::{CounterRef, CounterState, ObjectRegistry};

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
  counter_states: FxHashMap<crate::types::ObjectIndex, CounterState>,
  history: History,
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
      counter_states: FxHashMap::default(),
      history: History::new(),
    }
  }

  /// Returns the peer ID of this document.
  pub fn peer_id(&self) -> PeerID {
    self.peer_id
  }

  /// Returns a reference to the causal graph.
  pub fn causal_graph(&self) -> &CausalGraph {
    &self.causal_graph
  }

  /// Returns a mutable reference to the causal graph.
  pub fn causal_graph_mut(&mut self) -> &mut CausalGraph {
    &mut self.causal_graph
  }

  /// Returns a reference to the commit history.
  pub fn history(&self) -> &History {
    &self.history
  }

  /// Appends a commit to the history.
  pub fn push_to_history(&mut self, commit: Commit) {
    self.history.push(commit);
  }

  /// Returns a reference to the counter state for the given container, if any.
  pub fn counter_state(&self, index: crate::types::ObjectIndex) -> Option<&CounterState> {
    self.counter_states.get(&index)
  }

  /// Returns a mutable reference to the counter state for the given container,
  /// creating one if it does not exist.
  pub fn counter_state_mut(&mut self, index: crate::types::ObjectIndex) -> &mut CounterState {
    self.counter_states.entry(index).or_default()
  }

  /// Returns a reference to the object registry.
  pub fn registry(&self) -> &ObjectRegistry {
    &self.registry
  }

  /// Allocates and returns the next counter for this peer, incrementing
  /// the internal counter in the process.
  pub fn alloc_counter(&mut self) -> Counter {
    let c = self.next_counter;
    self.next_counter += 1;
    c
  }

  /// Returns a reference to the counter object with the given name.
  ///
  /// If the object does not yet exist, a new entry is allocated in the
  /// registry. If the name is already used by a different type, returns
  /// a type mismatch error.
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
