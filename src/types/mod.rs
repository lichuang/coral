//! Core types used throughout the Coral CRDT library.
//!
//! This module collects the fundamental primitives that identify peers,
//! operations, logical timestamps, and container categories.

mod container;
mod op_id;
mod primitives;
mod value;

pub use container::{ContainerIndex, ContainerType};
pub use op_id::OpId;
pub use primitives::{Counter, CounterExt, Lamport, PeerID, Timestamp};
pub use value::Value;
