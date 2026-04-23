//! Core types used throughout the Coral CRDT library.
//!
//! This module collects the fundamental primitives that identify peers,
//! operations, logical timestamps, and container categories.

mod container;
mod error;
mod id;
mod primitives;
mod value;

pub use container::{ContainerID, ContainerType};
pub use error::{CoralError, CoralResult};
pub use id::ID;
pub use primitives::{Counter, Lamport, PeerID, Timestamp};
pub use value::CoralValue;
