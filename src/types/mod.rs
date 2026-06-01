//! Core types used throughout the Coral CRDT library.
//!
//! This module collects the fundamental primitives that identify peers,
//! operations, logical timestamps, and object categories.

mod object;
mod op_id;
mod primitives;
mod value;

pub use object::{ObjectId, ObjectIndex, ObjectType};
pub use op_id::OpId;
pub use primitives::{Counter, CounterExt, Lamport, PeerID, Timestamp};
pub use value::Value;
