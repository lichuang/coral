//! Event hints for efficient diff generation.

use crate::types::CoralValue;

/// Event hints recorded during a transaction for efficient diff generation.
///
/// Hints capture the *intent* of an operation so that the event system can
/// reconstruct container-level diffs without re-scanning the entire state.
#[derive(Debug, Clone)]
pub enum EventHint {
  /// Text insertion event hint.
  InsertText {
    /// Byte/char position in the document.
    pos: usize,
    /// Length of the event in document units.
    event_len: usize,
    /// Unicode code-point length.
    unicode_len: usize,
    // TODO(Phase 9): add styles field when RichText is implemented
  },

  /// Map key set/delete event hint.
  Map {
    /// The affected key.
    key: String,
    /// The new value (`None` = deletion).
    value: Option<CoralValue>,
  },

  /// List operation event hint.
  List {
    /// Insertion/deletion position.
    pos: usize,
    // TODO(Phase 6): expand fields when List is implemented
  },

  /// Tree operation event hint.
  Tree {
    // TODO(Phase 10): expand fields when Tree is implemented
  },
}
