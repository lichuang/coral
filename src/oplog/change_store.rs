//! In-memory change store.
//!
//! [`ChangeStore`] holds all [`Change`]s indexed by their start [`ID`].
//! Changes are grouped into [`ChangesBlock`]s per peer so that consecutive
//! changes can be coalesced at the storage level.
//!
//! This is the **in-memory only** implementation.  The `Bytes` and `Both`
//! variants of [`BlockContent`] are defined for future persistence layers
//! but are not used by the current code path.

use crate::core::change::Change;
use crate::memory::arena::SharedArena;
use crate::op::Op;
use crate::rle::{HasIndex, HasLength};
use crate::types::{Counter, ID, PeerID};
use crate::version::IdSpan;
use std::collections::BTreeMap;
use std::fmt::Debug;

/// The physical representation of a block's payload.
///
/// - `Changes` — parsed `Change` objects (memory-only mode).
/// - `Bytes` — raw encoded bytes (used by persistent backends).
/// - `Both` — both forms cached simultaneously.
#[derive(Debug, Clone, PartialEq)]
pub enum BlockContent<O = Op> {
  /// Parsed changes ready for direct use.
  Changes(Vec<Change<O>>),
  /// Raw bytes that have not been deserialized yet.
  Bytes(Vec<u8>),
  /// Both forms are present; the bytes may be a stale serialization
  /// of the parsed changes.
  Both(Vec<Change<O>>, Vec<u8>),
}

/// A contiguous run of changes from a single peer.
///
/// Each block covers the counter range `[counter, counter + len)`.
/// When a new change's start counter exactly matches the block's end,
/// it is appended in place rather than creating a new block.
#[derive(Debug, Clone, PartialEq)]
pub struct ChangesBlock<O = Op> {
  /// Peer that produced all changes in this block.
  pub peer: PeerID,
  /// Starting counter (inclusive).
  pub counter: Counter,
  /// Length of the covered counter range.
  pub len: usize,
  /// Payload — in the memory-only implementation this is always `Changes`.
  pub content: BlockContent<O>,
}

#[allow(dead_code)]
impl<O: HasLength + HasIndex + crate::rle::Mergable + Debug + Clone> ChangesBlock<O> {
  /// Creates a new block containing a single change.
  pub fn from_change(change: &Change<O>) -> Self {
    Self {
      peer: change.id().peer,
      counter: change.id().counter,
      len: change.content_len(),
      content: BlockContent::Changes(vec![change.clone()]),
    }
  }

  /// The exclusive end counter.
  #[inline]
  pub fn end_counter(&self) -> Counter {
    self.counter + self.len as Counter
  }

  /// Returns `true` if `id` falls inside this block's counter range.
  pub fn contains_id(&self, id: ID) -> bool {
    id.peer == self.peer && id.counter >= self.counter && id.counter < self.end_counter()
  }

  /// Returns `true` if `change` can be appended to this block.
  pub fn can_append(&self, change: &Change<O>) -> bool {
    change.id().peer == self.peer && change.id().counter == self.end_counter()
  }

  /// Appends a change to this block.
  ///
  /// # Panics
  ///
  /// Panics if `can_append(change)` is `false`.
  pub fn append(&mut self, change: &Change<O>) {
    assert!(
      self.can_append(change),
      "cannot append non-contiguous change"
    );
    self.len += change.content_len();
    match &mut self.content {
      BlockContent::Changes(changes) => changes.push(change.clone()),
      BlockContent::Bytes(_) => {
        // In memory-only mode this branch should never be hit.
        // If it is, we degrade gracefully by switching to Changes.
        self.content = BlockContent::Changes(vec![change.clone()]);
      }
      BlockContent::Both(changes, _) => changes.push(change.clone()),
    }
  }
}

// ═══════════════════════════════════════════════════════════════════════════
// ChangeStore
// ═══════════════════════════════════════════════════════════════════════════

/// In-memory storage for all [`Change`]s.
///
/// Changes are kept in [`ChangesBlock`]s keyed by their start [`ID`].
/// Consecutive changes from the same peer are coalesced into a single block
/// to reduce allocation overhead.
#[derive(Debug, Clone)]
pub struct ChangeStore<O = Op> {
  blocks: BTreeMap<ID, ChangesBlock<O>>,
  arena: SharedArena,
}

#[allow(dead_code)]
impl<
  O: HasLength
    + HasIndex<Int = Counter>
    + crate::rle::Mergable
    + crate::rle::Sliceable
    + Debug
    + Clone,
> ChangeStore<O>
{
  /// Creates an empty `ChangeStore`.
  pub fn new(arena: SharedArena) -> Self {
    Self {
      blocks: BTreeMap::new(),
      arena,
    }
  }

  /// Reference to the underlying arena.
  pub fn arena(&self) -> &SharedArena {
    &self.arena
  }

  // ── Insertion ────────────────────────────────────────────────────────────

  /// Inserts a change into the store.
  ///
  /// If the change is contiguous with the last block from the same peer,
  /// it is appended to that block.  Otherwise a new block is created.
  ///
  /// The `is_local` flag is reserved for future persistence logic
  /// (e.g. deciding whether to flush immediately); it is ignored by the
  /// in-memory implementation.
  pub fn insert_change(&mut self, change: &Change<O>, _is_local: bool) {
    let peer = change.id().peer;
    let key = ID::new(peer, Counter::MAX);

    if let Some((_, last_block)) = self.blocks.range_mut(..=key).next_back()
      && last_block.can_append(change)
    {
      last_block.append(change);
      return;
    }

    let block = ChangesBlock::from_change(change);
    self.blocks.insert(change.id(), block);
  }

  // ── Lookup ───────────────────────────────────────────────────────────────

  /// Returns the change that contains `id`.
  ///
  /// A change "contains" `id` when `id.peer` matches and `id.counter` falls
  /// inside the change's own counter range.
  pub fn get_change(&self, id: ID) -> Option<Change<O>> {
    let block = self.find_block(id)?;
    match &block.content {
      BlockContent::Changes(changes) => {
        for change in changes {
          if change.contains_id(id) {
            return Some(change.clone());
          }
        }
        None
      }
      BlockContent::Bytes(_) => None,
      BlockContent::Both(changes, _) => {
        for change in changes {
          if change.contains_id(id) {
            return Some(change.clone());
          }
        }
        None
      }
    }
  }

  /// Iterates over all changes that intersect `id_span`.
  pub fn iter_changes(&self, id_span: IdSpan) -> impl Iterator<Item = &Change<O>> {
    let peer = id_span.peer;
    let span_start = id_span.counter.start;
    let span_end = id_span.counter.end;

    // Find the first block that *might* intersect the span.
    // A block with key < span_start may still overlap if its end > span_start.
    let start_key = ID::new(peer, span_start);
    let mut blocks: Vec<_> = Vec::new();

    // Check the block just before start_key — it might straddle span_start.
    let mut first_counter: Option<Counter> = None;
    if let Some((_, block)) = self.blocks.range(..=start_key).next_back()
      && block.peer == peer
      && block.end_counter() > span_start
    {
      first_counter = Some(block.counter);
      blocks.push(block);
    }

    // Then collect all blocks whose key lies inside [span_start, span_end).
    for (_, block) in self.blocks.range(start_key..ID::new(peer, span_end)) {
      if block.peer == peer && first_counter != Some(block.counter) {
        blocks.push(block);
      }
    }

    blocks
      .into_iter()
      .filter_map(move |block| match &block.content {
        BlockContent::Changes(changes) => Some(changes.iter()),
        BlockContent::Bytes(_) => None,
        BlockContent::Both(changes, _) => Some(changes.iter()),
      })
      .flatten()
      .filter(move |c| {
        let change_start = c.id().counter;
        let change_end = c.id().counter + c.content_len() as Counter;
        change_start < span_end && change_end > span_start
      })
  }

  /// Returns the last block for `peer`, if any.
  pub fn get_last_block_for_peer(&self, peer: PeerID) -> Option<&ChangesBlock<O>> {
    let key = ID::new(peer, Counter::MAX);
    self.blocks.range(..=key).next_back().map(|(_, b)| b)
  }

  // ── Internal helpers ─────────────────────────────────────────────────────

  fn find_block(&self, id: ID) -> Option<&ChangesBlock<O>> {
    let key = ID::new(id.peer, Counter::MAX);
    let (_, block) = self.blocks.range(..=key).next_back()?;
    if block.contains_id(id) {
      Some(block)
    } else {
      None
    }
  }
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
  use super::*;
  use crate::memory::arena::InnerArena;
  use crate::op::{Op, OpContent};
  use crate::rle::RleVec;
  use crate::types::{ContainerID, ContainerType, ID};
  use crate::version::Frontiers;

  fn make_change(
    peer: PeerID,
    counter: Counter,
    lamport: crate::types::Lamport,
    deps: Frontiers,
    op_count: usize,
  ) -> Change {
    let arena = InnerArena::new();
    let container = arena.register(&ContainerID::new_root("c", ContainerType::Counter));
    let ops = RleVec::from(
      (0..op_count)
        .map(|i| Op::new(counter + i as Counter, container, OpContent::Counter(1.0)))
        .collect::<Vec<_>>(),
    );
    Change::new(ops, deps, ID::new(peer, counter), lamport, 1_700_000_000)
  }

  fn empty_store() -> ChangeStore {
    ChangeStore::new(SharedArena::new(InnerArena::new()))
  }

  #[test]
  fn test_insert_single_change() {
    let mut store = empty_store();
    let c = make_change(1, 0, 1, Frontiers::new(), 2);
    store.insert_change(&c, true);

    assert_eq!(store.blocks.len(), 1);
    let block = store.get_last_block_for_peer(1).unwrap();
    assert_eq!(block.counter, 0);
    assert_eq!(block.len, 2);
  }

  #[test]
  fn test_insert_merge_consecutive() {
    let mut store = empty_store();
    let c1 = make_change(1, 0, 1, Frontiers::new(), 2);
    let c2 = make_change(1, 2, 3, Frontiers::from_id(ID::new(1, 1)), 3);
    store.insert_change(&c1, true);
    store.insert_change(&c2, true);

    // Same peer, continuous counter → merged into one block.
    assert_eq!(store.blocks.len(), 1);
    let block = store.get_last_block_for_peer(1).unwrap();
    assert_eq!(block.counter, 0);
    assert_eq!(block.len, 5); // 2 + 3
    assert_eq!(block.end_counter(), 5);
  }

  #[test]
  fn test_insert_no_merge_gap() {
    let mut store = empty_store();
    let c1 = make_change(1, 0, 1, Frontiers::new(), 2);
    let c2 = make_change(1, 5, 6, Frontiers::from_id(ID::new(1, 1)), 1);
    store.insert_change(&c1, true);
    store.insert_change(&c2, true);

    // Gap between 2 and 5 → two separate blocks.
    assert_eq!(store.blocks.len(), 2);
  }

  #[test]
  fn test_insert_no_merge_different_peer() {
    let mut store = empty_store();
    let c1 = make_change(1, 0, 1, Frontiers::new(), 2);
    let c2 = make_change(2, 0, 2, Frontiers::from_id(ID::new(1, 1)), 1);
    store.insert_change(&c1, true);
    store.insert_change(&c2, true);

    assert_eq!(store.blocks.len(), 2);
  }

  #[test]
  fn test_get_change_found() {
    let mut store = empty_store();
    let c = make_change(1, 0, 1, Frontiers::new(), 3);
    store.insert_change(&c, true);

    assert!(store.get_change(ID::new(1, 0)).is_some());
    assert!(store.get_change(ID::new(1, 1)).is_some());
    assert!(store.get_change(ID::new(1, 2)).is_some());
  }

  #[test]
  fn test_get_change_not_found() {
    let mut store = empty_store();
    let c = make_change(1, 0, 1, Frontiers::new(), 2);
    store.insert_change(&c, true);

    assert!(store.get_change(ID::new(1, 2)).is_none());
    assert!(store.get_change(ID::new(2, 0)).is_none());
  }

  #[test]
  fn test_iter_changes_range() {
    let mut store = empty_store();
    let c1 = make_change(1, 0, 1, Frontiers::new(), 2); // counters 0..2
    let c2 = make_change(1, 2, 3, Frontiers::from_id(ID::new(1, 1)), 3); // counters 2..5
    store.insert_change(&c1, true);
    store.insert_change(&c2, true);

    let span = IdSpan::new(1, 1, 4);
    let collected: Vec<_> = store.iter_changes(span).collect();

    // c1 covers [0,2), c2 covers [2,5).
    // Span [1,4) intersects both.
    assert_eq!(collected.len(), 2);
    assert_eq!(collected[0].id(), ID::new(1, 0));
    assert_eq!(collected[1].id(), ID::new(1, 2));
  }

  #[test]
  fn test_iter_changes_empty_range() {
    let mut store = empty_store();
    let c = make_change(1, 0, 1, Frontiers::new(), 2);
    store.insert_change(&c, true);

    let span = IdSpan::new(2, 0, 2); // different peer
    let collected: Vec<_> = store.iter_changes(span).collect();
    assert!(collected.is_empty());
  }

  #[test]
  fn test_get_last_block_for_peer() {
    let mut store = empty_store();
    assert!(store.get_last_block_for_peer(1).is_none());

    let c = make_change(1, 0, 1, Frontiers::new(), 2);
    store.insert_change(&c, true);

    let block = store.get_last_block_for_peer(1).unwrap();
    assert_eq!(block.peer, 1);
    assert_eq!(block.counter, 0);
  }

  #[test]
  fn test_block_contains_id() {
    let c = make_change(1, 0, 1, Frontiers::new(), 3);
    let block = ChangesBlock::from_change(&c);

    assert!(block.contains_id(ID::new(1, 0)));
    assert!(block.contains_id(ID::new(1, 1)));
    assert!(block.contains_id(ID::new(1, 2)));
    assert!(!block.contains_id(ID::new(1, 3)));
    assert!(!block.contains_id(ID::new(2, 0)));
  }

  #[test]
  fn test_block_can_append() {
    let c1 = make_change(1, 0, 1, Frontiers::new(), 2);
    let c2 = make_change(1, 2, 3, Frontiers::from_id(ID::new(1, 1)), 1);
    let c3 = make_change(1, 5, 6, Frontiers::from_id(ID::new(1, 1)), 1);
    let block = ChangesBlock::from_change(&c1);

    assert!(block.can_append(&c2));
    assert!(!block.can_append(&c3)); // gap

    let c_other_peer = make_change(2, 2, 3, Frontiers::new(), 1);
    assert!(!block.can_append(&c_other_peer));
  }

  #[test]
  fn test_block_append() {
    let c1 = make_change(1, 0, 1, Frontiers::new(), 2);
    let c2 = make_change(1, 2, 3, Frontiers::from_id(ID::new(1, 1)), 3);
    let mut block = ChangesBlock::from_change(&c1);
    block.append(&c2);

    assert_eq!(block.len, 5);
    assert_eq!(block.end_counter(), 5);
    match &block.content {
      BlockContent::Changes(changes) => assert_eq!(changes.len(), 2),
      _ => panic!("expected Changes variant"),
    }
  }
}
