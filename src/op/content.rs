//! Operation payloads — the "what" of each [`Op`](super::Op).
//!
//! The concrete structs (`MapOp`, `ListOp`, …) are currently opaque
//! placeholders.  They will be fleshed out in their respective phases.

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
