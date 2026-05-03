//! Transaction-level diff generation.
//!
//! [`change_to_diff`] converts a committed [`Change`] plus its
//! [`EventHint`](super::EventHint)s into a vector of per-container
//! [`TxnContainerDiff`]s.

use crate::container::map::MapSet;
use crate::core::change::Change;
use crate::core::container::ContainerIdx;
use crate::op::OpContent;
use crate::types::CoralValue;

use super::EventHint;

/// A container-level diff produced by a single transaction.
#[derive(Debug, Clone, PartialEq)]
pub struct TxnContainerDiff {
  /// The container that was modified.
  pub container: ContainerIdx,
  /// The concrete diff for that container.
  pub diff: ContainerDiff,
}

/// Per-container diff variants.
///
/// # Phase note
///
/// This is a skeleton aligned with Loro's diff design.  It will be
/// enriched (e.g. `ListDiff` gaining insert/delete spans, `TextDiff`
/// gaining style information) as container states land in Phases 9–14.
#[derive(Debug, Clone, PartialEq)]
pub enum ContainerDiff {
  /// Counter delta (arithmetic change).
  Counter(f64),
  /// Map key set or logical deletion.
  Map {
    key: String,
    value: Option<CoralValue>,
  },
  /// List insert or delete at the given position.
  List { pos: usize },
  /// Text insert with position and length metadata.
  Text {
    pos: usize,
    event_len: usize,
    unicode_len: usize,
  },
  /// Tree operation (placeholder until Phase 10).
  Tree,
}

/// Convert a [`Change`] and its [`EventHint`]s into a vector of
/// [`TxnContainerDiff`]s.
///
/// Counter diffs are derived directly from the [`OpContent`]; all other
/// variants rely on the corresponding [`EventHint`].  If an op has no
/// matching hint the function falls back to the raw op content where
/// possible (currently only [`MapSet`]).
///
/// # Panics
///
/// Never panics in the current skeleton implementation.
pub fn change_to_diff(change: &Change, event_hints: &[EventHint]) -> Vec<TxnContainerDiff> {
  let mut diffs = Vec::new();
  let mut hint_iter = event_hints.iter();

  for op in change.ops().iter() {
    let diff = match &op.content {
      OpContent::Counter(delta) => Some(ContainerDiff::Counter(*delta)),

      OpContent::Map(MapSet { key, value }) => {
        if let Some(EventHint::Map {
          key: hint_key,
          value: hint_value,
        }) = hint_iter.next()
        {
          Some(ContainerDiff::Map {
            key: hint_key.clone(),
            value: hint_value.clone(),
          })
        } else {
          Some(ContainerDiff::Map {
            key: key.clone(),
            value: value.clone(),
          })
        }
      }

      OpContent::List(_) => {
        if let Some(EventHint::List { pos }) = hint_iter.next() {
          Some(ContainerDiff::List { pos: *pos })
        } else {
          None
        }
      }

      OpContent::Tree(_) => {
        if let Some(EventHint::Tree {}) = hint_iter.next() {
          Some(ContainerDiff::Tree)
        } else {
          None
        }
      }
    };

    if let Some(diff) = diff {
      diffs.push(TxnContainerDiff {
        container: op.container,
        diff,
      });
    }
  }

  diffs
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::container::ContainerIdx;
  use crate::doc::CoralDoc;
  use crate::memory::arena::InnerArena;
  use crate::memory::arena::SharedArena;
  use crate::txn::Transaction;
  use crate::types::{ContainerID, ContainerType};

  fn make_counter_container() -> (CoralDoc, ContainerIdx) {
    let doc = CoralDoc::new(1);
    let c = doc
      .arena
      .register(&ContainerID::new_root("c", ContainerType::Counter));
    (doc, c)
  }

  fn make_map_container() -> (CoralDoc, ContainerIdx) {
    let doc = CoralDoc::new(1);
    let m = doc
      .arena
      .register(&ContainerID::new_root("m", ContainerType::Map));
    (doc, m)
  }

  #[test]
  fn test_change_to_diff_counter() {
    let (mut doc, c) = make_counter_container();
    let mut txn = Transaction::new(&mut doc, None);
    txn.add_op(c, OpContent::Counter(3.5));
    let change = txn.commit_with_timestamp(&mut doc, 1).unwrap();

    let diffs = change_to_diff(&change, &[]);
    assert_eq!(diffs.len(), 1);
    assert_eq!(diffs[0].container, c);
    assert_eq!(diffs[0].diff, ContainerDiff::Counter(3.5));
  }

  #[test]
  fn test_change_to_diff_map_with_hint() {
    let (mut doc, m) = make_map_container();
    let mut txn = Transaction::new(&mut doc, None);
    txn.apply_local_op(
      &mut doc,
      m,
      crate::op::RawOpContent::Map(MapSet {
        key: "k1".into(),
        value: Some(CoralValue::from(42i32)),
      }),
      Some(EventHint::Map {
        key: "k1".to_string(),
        value: Some(CoralValue::from(42i32)),
      }),
    );
    let hints = txn.event_hints.clone();
    let change = txn.commit_with_timestamp(&mut doc, 1).unwrap();

    let diffs = change_to_diff(&change, &hints);
    assert_eq!(diffs.len(), 1);
    assert_eq!(diffs[0].container, m);
    assert_eq!(
      diffs[0].diff,
      ContainerDiff::Map {
        key: "k1".to_string(),
        value: Some(CoralValue::from(42i32)),
      }
    );
  }

  #[test]
  fn test_change_to_diff_map_without_hint_falls_back() {
    let (mut doc, m) = make_map_container();
    let txn = Transaction::new(&mut doc, None);
    let mut txn = txn;
    txn.add_op(
      m,
      OpContent::Map(MapSet {
        key: "fallback".into(),
        value: Some(CoralValue::from("v")),
      }),
    );
    let change = txn.commit_with_timestamp(&mut doc, 1).unwrap();

    let diffs = change_to_diff(&change, &[]);
    assert_eq!(diffs.len(), 1);
    assert_eq!(
      diffs[0].diff,
      ContainerDiff::Map {
        key: "fallback".to_string(),
        value: Some(CoralValue::from("v")),
      }
    );
  }

  #[test]
  fn test_change_to_diff_mixed_counter_and_map() {
    let mut doc = CoralDoc::new(1);
    let c = doc
      .arena
      .register(&ContainerID::new_root("c", ContainerType::Counter));
    let m = doc
      .arena
      .register(&ContainerID::new_root("m", ContainerType::Map));

    let mut txn = Transaction::new(&mut doc, None);
    txn.add_op(c, OpContent::Counter(1.0));
    txn.apply_local_op(
      &mut doc,
      m,
      crate::op::RawOpContent::Map(MapSet {
        key: "k".into(),
        value: Some(CoralValue::from(true)),
      }),
      Some(EventHint::Map {
        key: "k".to_string(),
        value: Some(CoralValue::from(true)),
      }),
    );
    txn.add_op(c, OpContent::Counter(2.0));
    let hints = txn.event_hints.clone();
    let change = txn.commit_with_timestamp(&mut doc, 1).unwrap();

    let diffs = change_to_diff(&change, &hints);
    assert_eq!(diffs.len(), 3);

    // Counter op 1 (no hint consumed)
    assert_eq!(diffs[0].container, c);
    assert_eq!(diffs[0].diff, ContainerDiff::Counter(1.0));

    // Map op (consumes the first hint)
    assert_eq!(diffs[1].container, m);
    assert_eq!(
      diffs[1].diff,
      ContainerDiff::Map {
        key: "k".to_string(),
        value: Some(CoralValue::from(true)),
      }
    );

    // Counter op 2 (no hint consumed)
    assert_eq!(diffs[2].container, c);
    assert_eq!(diffs[2].diff, ContainerDiff::Counter(2.0));
  }

  #[test]
  fn test_change_to_diff_empty_change() {
    let _arena = SharedArena::new(InnerArena::new());
    let change = Change::new(
      crate::rle::RleVec::new(),
      crate::version::Frontiers::new(),
      crate::types::ID::new(1, 0),
      0,
      0,
    );
    let diffs = change_to_diff(&change, &[]);
    assert!(diffs.is_empty());
  }

  #[test]
  fn test_change_to_diff_text_hint() {
    let mut doc = CoralDoc::new(1);
    let _t = doc
      .arena
      .register(&ContainerID::new_root("t", ContainerType::Text));

    // Text ops are not yet fully supported (List op arena allocation is todo),
    // but we can still test the EventHint → diff path directly.
    let hints = vec![EventHint::InsertText {
      pos: 5,
      event_len: 3,
      unicode_len: 3,
    }];

    // Build a dummy change with a single Counter op so change_to_diff has
    // something to iterate over.  The Text hint will be skipped because
    // the op is Counter (Counter consumes no hints).
    let mut txn = Transaction::new(&mut doc, None);
    let c = doc
      .arena
      .register(&ContainerID::new_root("c", ContainerType::Counter));
    txn.add_op(c, OpContent::Counter(1.0));
    let change = txn.commit_with_timestamp(&mut doc, 1).unwrap();

    // Counter op doesn't consume hints, so the Text hint is ignored.
    let diffs = change_to_diff(&change, &hints);
    assert_eq!(diffs.len(), 1);
    assert_eq!(diffs[0].diff, ContainerDiff::Counter(1.0));
  }
}
