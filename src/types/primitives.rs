//! Core type aliases used throughout the Coral CRDT library.
//!
//! These are the fundamental primitives that identify peers, operations,
//! and logical timestamps.

/// Unique identifier for each peer/replica in the distributed system.
///
/// A `PeerID` should be globally unique. It can be randomly generated
/// (e.g., via `rand::random()`) or derived from a deterministic source
/// such as a snowflake ID.
pub type PeerID = u64;

/// Monotonically increasing counter for operations within a single peer.
///
/// Each peer maintains its own `Counter`, starting from 0 and incrementing
/// by 1 for each atomic operation. Combined with `PeerID`, it forms a
/// globally unique [`ID`](super::id::ID) for every operation.
pub type Counter = i32;

/// Lamport logical timestamp for causal ordering and Last-Write-Wins resolution.
///
/// When two operations are concurrent (neither happened-before the other),
/// the one with the higher `Lamport` value is considered "later".
/// If `Lamport` values are equal, `PeerID` is used as a deterministic tie-breaker.
pub type Lamport = u32;

/// Physical timestamp in seconds since the Unix epoch.
///
/// Used to record wall-clock time for each [`Change`](crate::core::change::Change).
/// Note: this is stored in **seconds**, not milliseconds.
pub type Timestamp = i64;
