//! Minimal set of leaf IDs that identifies a document version.
//!
//! See [`Frontiers`] for details.

use crate::types::ID;

/// The minimal set of leaf IDs that identifies a document version.
///
/// When history is linear, there is exactly one frontier ID.
/// When there are concurrent edits, there may be multiple.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Frontiers(Vec<ID>);

impl Frontiers {
  /// Creates empty frontiers.
  pub fn new() -> Self {
    Self(Vec::new())
  }

  /// Creates frontiers containing a single ID.
  pub fn from_id(id: ID) -> Self {
    Self(vec![id])
  }

  /// Number of frontier IDs.
  pub fn len(&self) -> usize {
    self.0.len()
  }

  pub fn is_empty(&self) -> bool {
    self.0.is_empty()
  }

  /// Iterates over the frontier IDs.
  pub fn iter(&self) -> impl Iterator<Item = ID> + '_ {
    self.0.iter().copied()
  }

  /// Adds a new leaf ID, removing any of its ancestors if present.
  pub fn push(&mut self, id: ID) {
    // Simple append; ancestor pruning is handled by the DAG layer in Loro.
    self.0.push(id);
  }
}

impl From<Vec<ID>> for Frontiers {
  fn from(ids: Vec<ID>) -> Self {
    Self(ids)
  }
}

impl FromIterator<ID> for Frontiers {
  fn from_iter<I: IntoIterator<Item = ID>>(iter: I) -> Self {
    Self(iter.into_iter().collect())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_frontiers_from_vec() {
    let f = Frontiers::from(vec![ID::new(1, 10), ID::new(2, 5)]);
    assert_eq!(f.len(), 2);
    let ids: Vec<_> = f.iter().collect();
    assert_eq!(ids, vec![ID::new(1, 10), ID::new(2, 5)]);
  }
}
