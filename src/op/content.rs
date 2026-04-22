//! Operation payloads — the "what" of each [`Op`](super::Op).
//!
//! The concrete structs (`MapOp`, `ListOp`, …) are currently opaque
//! placeholders.  They will be fleshed out in their respective phases.

use crate::rle::{HasLength, Mergable, Sliceable};

/// Discriminated union of all per-container operation payloads.
#[derive(Debug, Clone)]
pub enum OpContent {
  /// Operation on a Map container.
  Map(MapOp),

  /// Operation on a List container.
  List(ListOp),

  /// Operation on a Text container.
  Text(TextOp),

  /// Operation on a Tree container.
  Tree(TreeOp),

  /// Operation on a Counter container.
  Counter(CounterOp),
}

// ═══════════════════════════════════════════════════════════════════════════
// Placeholder operation types — will be defined in later phases.
// ═══════════════════════════════════════════════════════════════════════════

/// Placeholder for Map operations (Phase 5).
#[derive(Debug, Clone)]
pub struct MapOp;

/// Placeholder for List operations (Phase 6).
#[derive(Debug, Clone)]
pub struct ListOp;

/// Placeholder for Text operations (Phase 8).
#[derive(Debug, Clone)]
pub struct TextOp;

/// Placeholder for Tree operations (Phase 10).
#[derive(Debug, Clone)]
pub struct TreeOp;

/// Placeholder for Counter operations (Phase 3).
#[derive(Debug, Clone)]
pub struct CounterOp;

// ═══════════════════════════════════════════════════════════════════════════
// RLE trait implementations — placeholder level
// ═══════════════════════════════════════════════════════════════════════════

impl HasLength for OpContent {
  fn content_len(&self) -> usize {
    // All placeholder ops are atomic (length 1).
    1
  }
}

impl Sliceable for OpContent {
  fn slice(&self, from: usize, to: usize) -> Self {
    assert!(
      from == 0 && to == 1,
      "OpContent is atomic (placeholder) and cannot be sliced: tried [{from}, {to})"
    );
    self.clone()
  }
}

// Placeholder OpContent does not support merging (empty structs have no state
// to coalesce).  Once concrete fields are added in later phases this will be
// overridden for merge-friendly variants such as CounterOp.
impl Mergable for OpContent {}
