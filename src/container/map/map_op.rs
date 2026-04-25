//! Map operation — set or delete a key.

use crate::types::CoralValue;

/// Set a key in a Map container.
///
/// `value = None` represents a logical deletion (tombstone).
/// The actual LWW resolution happens at the state layer.
#[derive(Debug, Clone, PartialEq)]
pub struct MapSet {
  pub key: String,
  pub value: Option<CoralValue>,
}
