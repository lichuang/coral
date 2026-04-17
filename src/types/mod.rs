//! Core types used throughout the Coral CRDT library.
//!
//! This module collects the fundamental primitives that identify peers,
//! operations, logical timestamps, and container categories.

mod container;
mod id;
mod primitives;

pub use container::ContainerType;
pub use id::ID;
pub use primitives::{Counter, Lamport, PeerID};
