//! Run-length encoding (RLE) utilities for compact storage of mergeable
//! sequences.
//!
//! This module provides traits and data structures that enable adjacent,
//! compatible items to be merged into a single representative entry. The
//! primary use case is compressing sequences of operations or values that
//! appear in long runs, reducing memory footprint without losing logical
//! information.
//!
//! # Core traits
//!
//! - [`HasLength`] – reports the logical length of an item.
//! - [`Mergeable`] – defines when two adjacent items can be combined.
//!
//! # Core types
//!
//! - [`RleVec<T>`] – a vector that automatically merges incoming elements
//!   when the above traits indicate it is safe to do so.

pub mod rle_traits;
mod rle_vec;

pub use rle_traits::{HasLength, Mergeable};
pub use rle_vec::RleVec;
