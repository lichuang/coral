//! Container-specific operation types.
//!
//! Each subdirectory holds the op payloads for a single container type
//! (List, Map, Tree, …).  The top-level [`op`](crate::op) module assembles
//! these into [`OpContent`] and [`RawOpContent`].

pub mod list;
pub mod map;
pub mod tree;
