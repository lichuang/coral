use super::Commit;

/// Stores the append-only history of all commits applied to the document.
///
/// `History` is a simple log of [`Commit`]s in application order.
/// During sync, peers exchange commits so that each replica can replay
/// the full history and converge to the same state.
#[derive(Debug, Clone, Default)]
pub struct History {
  commits: Vec<Commit>,
}

impl History {
  /// Creates a new empty history.
  pub fn new() -> Self {
    Self::default()
  }

  /// Appends a commit to the history.
  pub fn push(&mut self, commit: Commit) {
    self.commits.push(commit);
  }

  /// Returns the number of stored commits.
  pub fn len(&self) -> usize {
    self.commits.len()
  }

  /// Returns `true` if there are no stored commits.
  pub fn is_empty(&self) -> bool {
    self.commits.is_empty()
  }

  /// Returns an iterator over the stored commits.
  pub fn iter(&self) -> std::slice::Iter<'_, Commit> {
    self.commits.iter()
  }
}
