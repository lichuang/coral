//! Operation payloads — the "what" of each [`Op`](super::Op).
//!
//! Two flavours are provided:
//!
//! * [`OpContent`] (a.k.a. `InnerContent`) — arena-resolved, stored inside
//!   committed [`Op`]s.
//! * [`RawOpContent<'a>`] — borrowed/transport representation used before
//!   arena allocation.

use crate::container::list::{InnerListOp, ListOp};
use crate::container::map::MapSet;
use crate::container::tree::TreeOp;
use crate::memory::SharedArena;
use crate::rle::{HasLength, Mergable, Sliceable};
use std::sync::Arc;

// ═══════════════════════════════════════════════════════════════════════════
// InnerContent — arena-resolved, stored in Op
// ═══════════════════════════════════════════════════════════════════════════

/// Discriminated union of all per-container operation payloads (resolved).
///
/// This is the form stored inside committed [`Op`]s.  List/Text values have
/// already been copied into the arena, so `Insert` carries a [`SliceRange`]
/// and `InsertText` carries a [`BytesSlice`](crate::memory::str_arena::BytesSlice).
#[derive(Debug, Clone)]
pub enum OpContent {
  /// Operation on a List or Text container.
  List(InnerListOp),

  /// Operation on a Map container.
  Map(MapSet),

  /// Operation on a Tree container.
  Tree(Arc<TreeOp>),

  /// Counter CRDT operation (arithmetic delta).
  Counter(f64),
}

impl HasLength for OpContent {
  fn content_len(&self) -> usize {
    match self {
      OpContent::List(op) => op.content_len(),
      OpContent::Map(_) | OpContent::Tree(_) | OpContent::Counter(_) => 1,
    }
  }
}

impl Sliceable for OpContent {
  fn slice(&self, from: usize, to: usize) -> Self {
    match self {
      OpContent::List(op) => OpContent::List(op.slice(from, to)),
      _ => {
        assert!(
          from == 0 && to == 1,
          "OpContent::slice: Map/Tree/Counter are atomic"
        );
        self.clone()
      }
    }
  }
}

impl Mergable for OpContent {
  fn is_mergable(&self, other: &Self, conf: &()) -> bool {
    match (self, other) {
      (OpContent::List(a), OpContent::List(b)) => a.is_mergable(b, conf),
      _ => false,
    }
  }

  fn merge(&mut self, other: &Self, conf: &()) {
    match (self, other) {
      (OpContent::List(a), OpContent::List(b)) => a.merge(b, conf),
      _ => unreachable!(),
    }
  }
}

// ═══════════════════════════════════════════════════════════════════════════
// RawOpContent — transport / serialization form
// ═══════════════════════════════════════════════════════════════════════════

/// Transport representation of an operation payload.
///
/// The `'a` lifetime allows `ListOp::Insert` to borrow data before it is
/// copied into the arena.  All other variants are fully owned.
#[derive(Debug, Clone, PartialEq)]
pub enum RawOpContent<'a> {
  /// List or Text operation (borrows data).
  List(ListOp<'a>),

  /// Map operation.
  Map(MapSet),

  /// Tree operation.
  Tree(Arc<TreeOp>),

  /// Counter delta.
  Counter(f64),
}

impl HasLength for RawOpContent<'_> {
  fn content_len(&self) -> usize {
    match self {
      RawOpContent::List(op) => op.content_len(),
      RawOpContent::Map(_) | RawOpContent::Tree(_) | RawOpContent::Counter(_) => 1,
    }
  }
}

impl Sliceable for RawOpContent<'_> {
  fn slice(&self, from: usize, to: usize) -> Self {
    match self {
      RawOpContent::List(op) => RawOpContent::List(op.slice(from, to)),
      _ => {
        assert!(
          from == 0 && to == 1,
          "RawOpContent::slice: Map/Tree/Counter are atomic"
        );
        self.clone()
      }
    }
  }
}

impl Mergable for RawOpContent<'_> {
  fn is_mergable(&self, other: &Self, conf: &()) -> bool {
    match (self, other) {
      (RawOpContent::List(a), RawOpContent::List(b)) => a.is_mergable(b, conf),
      _ => false,
    }
  }

  fn merge(&mut self, other: &Self, conf: &()) {
    assert!(self.is_mergable(other, conf));
    match (self, other) {
      (RawOpContent::List(a), RawOpContent::List(b)) => a.merge(b, conf),
      _ => unreachable!(),
    }
  }
}

impl<'a> RawOpContent<'a> {
  /// Convert this raw transport content into arena-resolved [`OpContent`].
  ///
  /// # Panics
  ///
  /// Panics with `todo!()` for [`RawOpContent::List`] variants until
  /// List/Text arena allocation is implemented in Phase 6.
  pub fn to_op_content(self, _arena: &SharedArena) -> OpContent {
    match self {
      RawOpContent::Counter(v) => OpContent::Counter(v),
      RawOpContent::Map(set) => OpContent::Map(set),
      RawOpContent::Tree(op) => OpContent::Tree(op),
      RawOpContent::List(_list_op) => {
        // TODO(Phase 6): List/Text arena allocation (InnerListOp conversion)
        todo!("List op arena allocation not yet implemented")
      }
    }
  }
}
