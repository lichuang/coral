//! Transaction — local editing batch buffering and commit.
//!
//! A [`Transaction`] buffers multiple [`Op`]s and commits them as a single
//! [`Change`] into the [`OpLog`].  This is the unit of local editing: all ops
//! in one transaction share the same `deps`, `lamport`, and `timestamp`.

pub mod diff;
pub mod event_hint;
pub mod transaction;

pub use diff::{ContainerDiff, TxnContainerDiff, change_to_diff};
pub use event_hint::EventHint;
pub use transaction::Transaction;
