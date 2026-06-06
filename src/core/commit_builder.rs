use crate::common::CoralResult;
use crate::operation::Op;
use crate::types::{Counter, Lamport, ObjectIndex, OpId, PeerID, Timestamp};
use crate::version::Heads;

use super::{Commit, DocInner};

pub struct CommitBuilder<'a> {
  doc: &'a mut DocInner,
  peer_id: PeerID,
  counter: Counter,
  lamport: Lamport,
  deps: Heads,
}

impl<'a> CommitBuilder<'a> {
  pub fn new(doc: &'a mut DocInner) -> Self {
    let peer_id = doc.peer_id();
    let counter = doc.alloc_counter();
    let lamport = doc.causal_graph().calc_next_lamport();
    let deps = doc.causal_graph().heads().clone();
    Self {
      doc,
      peer_id,
      counter,
      lamport,
      deps,
    }
  }

  pub fn counter(&self) -> Counter {
    self.counter
  }

  pub fn apply(&mut self, index: ObjectIndex, op: &Op) -> CoralResult<()> {
    self.doc.state_mut(index).apply(op)
  }

  pub fn finish(self, op: Op) {
    let mut commit = Commit::new(
      OpId::new(self.peer_id, self.counter),
      self.lamport,
      0 as Timestamp,
      self.deps,
      true,
    );
    commit.push_op(op);
    self.doc.causal_graph_mut().insert(&commit);
    self.doc.push_to_history(commit);
  }
}
