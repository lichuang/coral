//! Memory management — arena, compact indexing, and shared state.

pub mod arena;
pub mod str_arena;

pub use arena::SharedArena;
