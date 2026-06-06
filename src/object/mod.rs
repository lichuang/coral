pub mod registry;
pub mod state;

pub use registry::ObjectRegistry;
pub use state::ObjectState;

use crate::common::CoralResult;
use crate::core::{CommitBuilder, DocInner};
use crate::operation::{Cmd, Op};
use crate::types::ObjectIndex;

/// Type markers for [`ObjectRef`].
///
/// These are zero-sized types used only at compile time to distinguish
/// the kind of object a reference points to.
pub mod marker {
  /// Marker for a counter object.
  pub struct Counter;
}

/// A typed reference to a CRDT object within a document.
///
/// `ObjectRef` is the unified handle type for all container operations.
/// The generic parameter `T` distinguishes the object kind at compile time,
/// allowing each kind to expose its own methods via dedicated `impl` blocks.
#[allow(dead_code)]
pub struct ObjectRef<'a, T> {
  doc: &'a mut DocInner,
  index: ObjectIndex,
  _marker: std::marker::PhantomData<T>,
}

impl<'a, T> ObjectRef<'a, T> {
  /// Creates a new `ObjectRef`.
  #[allow(dead_code)]
  pub(crate) fn new(doc: &'a mut DocInner, index: ObjectIndex) -> Self {
    Self {
      doc,
      index,
      _marker: std::marker::PhantomData,
    }
  }

  /// Returns the [`ObjectIndex`] of the referenced object.
  pub fn index(&self) -> ObjectIndex {
    self.index
  }
}

/// Alias for [`ObjectRef`] pointing to a counter.
pub type CounterRef<'a> = ObjectRef<'a, marker::Counter>;

impl ObjectRef<'_, marker::Counter> {
  /// Returns the current value of the counter.
  pub fn value(&self) -> f64 {
    self
      .doc
      .state(self.index)
      .and_then(|s| s.as_counter())
      .map(|s| s.value())
      .unwrap_or(0.0)
  }

  /// Increments the counter by `delta`.
  ///
  /// Creates a new [`Op`], wraps it in a [`Commit`](super::Commit), inserts
  /// the commit into the causal graph, appends it to the history, and applies
  /// the delta to the counter state.
  pub fn increment(&mut self, delta: f64) -> CoralResult<()> {
    let mut builder = CommitBuilder::new(self.doc);
    let op = Op::new(builder.counter(), self.index, Cmd::IncCounter { delta });
    builder.apply(self.index, &op)?;
    builder.finish(op);
    Ok(())
  }
}

#[cfg(test)]
mod increment_tests {
  use crate::Document;
  use crate::types::OpId;

  #[test]
  fn test_counter_increment_and_value() {
    let mut doc = Document::new();
    let mut counter = doc.get_counter("hits").unwrap();

    assert_eq!(counter.value(), 0.0);

    counter.increment(1.0).unwrap();
    assert_eq!(counter.value(), 1.0);

    counter.increment(2.5).unwrap();
    assert_eq!(counter.value(), 3.5);

    counter.increment(-0.5).unwrap();
    assert_eq!(counter.value(), 3.0);
  }

  #[test]
  fn test_counter_causal_graph_single_peer() {
    let mut doc = Document::new();
    let peer_id = doc.peer_id();
    let mut counter = doc.get_counter("score").unwrap();

    counter.increment(10.0).unwrap();
    counter.increment(20.0).unwrap();

    let cg = doc.causal_graph();
    assert_eq!(cg.node_count(), 1);

    let vv = cg.vv();
    assert_eq!(vv.get(peer_id), Some(2));

    let heads = cg.heads();
    assert_eq!(heads.as_single(), Some(OpId::new(peer_id, 1)));
  }

  #[test]
  fn test_counter_history_records_commits() {
    let mut doc = Document::new();
    let mut counter = doc.get_counter("views").unwrap();

    counter.increment(1.0).unwrap();
    counter.increment(1.0).unwrap();
    counter.increment(1.0).unwrap();

    assert_eq!(doc.history().len(), 3);
  }

  #[test]
  fn test_two_counters_independent() {
    let mut doc = Document::new();

    {
      let mut a = doc.get_counter("a").unwrap();
      a.increment(5.0).unwrap();
    }
    {
      let mut b = doc.get_counter("b").unwrap();
      b.increment(10.0).unwrap();
    }

    let a = doc.get_counter("a").unwrap();
    assert_eq!(a.value(), 5.0);

    let b = doc.get_counter("b").unwrap();
    assert_eq!(b.value(), 10.0);
  }

  #[test]
  fn test_counter_increment_zero() {
    let mut doc = Document::new();
    let mut counter = doc.get_counter("x").unwrap();

    counter.increment(0.0).unwrap();
    assert_eq!(counter.value(), 0.0);

    counter.increment(5.0).unwrap();
    counter.increment(0.0).unwrap();
    assert_eq!(counter.value(), 5.0);
  }

  #[test]
  fn test_counter_negative_delta() {
    let mut doc = Document::new();
    let mut counter = doc.get_counter("balance").unwrap();

    counter.increment(100.0).unwrap();
    counter.increment(-30.0).unwrap();
    counter.increment(-70.0).unwrap();
    assert_eq!(counter.value(), 0.0);
  }

  #[test]
  fn test_counter_large_delta() {
    let mut doc = Document::new();
    let mut counter = doc.get_counter("big").unwrap();

    counter.increment(1e15).unwrap();
    counter.increment(1.0).unwrap();
    assert_eq!(counter.value(), 1e15 + 1.0);
  }

  #[test]
  fn test_counter_increment_after_reacquire() {
    let mut doc = Document::new();

    {
      let mut counter = doc.get_counter("session").unwrap();
      counter.increment(1.0).unwrap();
      counter.increment(2.0).unwrap();
    }

    {
      let mut counter = doc.get_counter("session").unwrap();
      counter.increment(3.0).unwrap();
    }

    let counter = doc.get_counter("session").unwrap();
    assert_eq!(counter.value(), 6.0);
  }

  #[test]
  fn test_counter_commit_op_id_sequence() {
    let mut doc = Document::new();
    let peer_id = doc.peer_id();
    let mut counter = doc.get_counter("seq").unwrap();

    counter.increment(1.0).unwrap();
    counter.increment(2.0).unwrap();
    counter.increment(3.0).unwrap();

    let history = doc.history();
    let commits = history.iter().collect::<Vec<_>>();
    assert_eq!(commits.len(), 3);

    assert_eq!(commits[0].id, OpId::new(peer_id, 0));
    assert_eq!(commits[1].id, OpId::new(peer_id, 1));
    assert_eq!(commits[2].id, OpId::new(peer_id, 2));
  }

  #[test]
  fn test_counter_lamport_advances() {
    let mut doc = Document::new();
    let mut counter = doc.get_counter("tick").unwrap();

    counter.increment(1.0).unwrap();
    counter.increment(1.0).unwrap();

    let history = doc.history();
    let commits: Vec<_> = history.iter().collect();
    assert_eq!(commits[0].lamport, 0);
    assert_eq!(commits[1].lamport, 1);
  }

  #[test]
  fn test_counter_heads_chain() {
    let mut doc = Document::new();
    let peer_id = doc.peer_id();

    {
      let mut counter = doc.get_counter("chain").unwrap();
      counter.increment(1.0).unwrap();
    }
    assert_eq!(
      doc.causal_graph().heads().as_single(),
      Some(OpId::new(peer_id, 0))
    );

    {
      let mut counter = doc.get_counter("chain").unwrap();
      counter.increment(1.0).unwrap();
    }
    assert_eq!(
      doc.causal_graph().heads().as_single(),
      Some(OpId::new(peer_id, 1))
    );
  }
}
