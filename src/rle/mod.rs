//! Run-Length Encoding (RLE) infrastructure.
//!
//! This module provides the core traits that enable `Change`s and `Op`s to be
//! merged, sliced, and stored compactly — the same primitives that Loro uses
//! inside its `RleVec` and causal-graph iterators.
//!
//! # Design note
//!
//! The traits are intentionally minimal.  `HasLength` + `Sliceable` +
//! `Mergable` are sufficient to build an `RleVec` later (1.10.2), but for now
//! we only define the traits so that `Op`, `Change`, `CounterSpan` and
//! `IdSpan` can start implementing them.

// ---------------------------------------------------------------------------
// HasLength
// ---------------------------------------------------------------------------

/// A type that has a measurable length in "atoms" (individual operations).
///
/// In RLE terms an element may represent a *run* of many atomic operations.
/// `content_len` is the semantic length (e.g. 3 inserted characters) while
/// `atom_len` is the number of indivisible operations (usually the same, but
/// can differ for compressed representations).
pub trait HasLength {
  /// Semantic length of the content.
  fn content_len(&self) -> usize;

  /// Number of atomic operations represented by this element.
  ///
  /// Defaults to `content_len`; override when the two differ.
  fn atom_len(&self) -> usize {
    self.content_len()
  }
}

// ---------------------------------------------------------------------------
// HasIndex
// ---------------------------------------------------------------------------

/// A type that has a starting index, used to locate an element inside a
/// sequence (e.g. the starting `Counter` of a `Change`).
pub trait HasIndex {
  /// The integer type used for the index (usually `Counter` / `i32`).
  type Int;

  /// Returns the start index.
  fn get_start_index(&self) -> Self::Int;
}

// ---------------------------------------------------------------------------
// Sliceable
// ---------------------------------------------------------------------------

/// A type that can be sliced by atom indices.
///
/// # Contract
///
/// `slice(from, to)` requires `from < to` and `to <= self.atom_len()`.
/// The returned value must satisfy `result.atom_len() == to - from`.
pub trait Sliceable {
  /// Returns a new instance containing only atoms `[from, to)`.
  fn slice(&self, from: usize, to: usize) -> Self;
}

// ---------------------------------------------------------------------------
// Mergable
// ---------------------------------------------------------------------------

/// A type whose adjacent instances can be merged into one.
///
/// This is the foundation of run-length encoding: two consecutive `Op`s that
/// touch the same container and have compatible content can be stored as a
/// single wider run.
pub trait Mergable {
  /// Extra context needed to decide whether two elements are mergeable.
  ///
  /// Use `()` when no external configuration is required.
  type Config;

  /// Returns `true` if `self` and `other` can be merged.
  fn is_mergable(&self, other: &Self, conf: &Self::Config) -> bool;

  /// Merges `other` into `self`.
  ///
  /// # Panics
  ///
  /// May panic if `is_mergable` would return `false`; callers must check
  /// first.
  fn merge(&mut self, other: &Self, conf: &Self::Config);
}
