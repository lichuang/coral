use crate::types::{Counter, PeerID};

/// A contiguous range of operations from a single peer.
///
/// `start` is inclusive, `end` is exclusive: the span covers counters
/// `[start, end)`. An empty span has `start == end`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IdSpan {
  pub peer: PeerID,
  pub start: Counter,
  pub end: Counter,
}

impl IdSpan {
  pub fn new(peer: PeerID, start: Counter, end: Counter) -> Self {
    Self { peer, start, end }
  }

  pub fn is_empty(&self) -> bool {
    self.start >= self.end
  }

  pub fn len(&self) -> Counter {
    self.end - self.start
  }
}

/// The diff between two version vectors, expressed as a set of [`IdSpan`]s.
///
/// Each span represents operations present in `self_vv` but not in `other_vv`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionVectorDiff {
  pub spans: Vec<IdSpan>,
}

impl VersionVectorDiff {
  pub fn new(spans: Vec<IdSpan>) -> Self {
    Self { spans }
  }

  pub fn is_empty(&self) -> bool {
    self.spans.is_empty()
  }

  pub fn iter(&self) -> impl Iterator<Item = &IdSpan> {
    self.spans.iter()
  }
}
