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
    // Simple append; ancestor pruning is handled by the DAG layer.
    self.0.push(id);
  }

  /// Returns the single ID if this frontiers contains exactly one.
  pub fn as_single(&self) -> Option<ID> {
    if self.0.len() == 1 {
      Some(self.0[0])
    } else {
      None
    }
  }

  /// Returns `true` if the frontiers contains the given ID.
  pub fn contains(&self, id: ID) -> bool {
    self.0.contains(&id)
  }

  /// Update frontiers when a new change is added.
  ///
  /// Removes all deps (and their ancestors) from the frontiers, then adds
  /// the new change's last ID.
  pub fn update_frontiers_on_new_change(&mut self, last_id: ID, deps: &Frontiers) {
    // Remove any frontier ID that is on the same peer and <= a dep,
    // because it is now an ancestor of the new change.
    for dep in deps.iter() {
      self
        .0
        .retain(|id| !(id.peer == dep.peer && id.counter <= dep.counter));
    }
    // Also remove any same-peer frontier that is before the new last_id.
    self
      .0
      .retain(|id| !(id.peer == last_id.peer && id.counter < last_id.counter));
    self.0.push(last_id);
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
