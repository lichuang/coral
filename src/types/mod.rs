//! Core types used throughout the Coral CRDT library.
//!
//! This module collects the fundamental primitives that identify peers,
//! operations, logical timestamps, and container categories.

mod container;
mod op_id;
mod primitives;

pub use container::ContainerType;
pub use op_id::OpId;
pub use primitives::{Counter, Lamport, PeerID, Timestamp};
