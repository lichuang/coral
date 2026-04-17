//! Core types used throughout the Coral CRDT library.
//!
//! This module collects the fundamental primitives that identify peers,
//! operations, logical timestamps, and container categories.

mod container;
mod container_id;
mod id;
mod primitives;

pub use container::ContainerType;
pub use container_id::ContainerID;
pub use id::ID;
pub use primitives::{Counter, Lamport, PeerID};
