# Coral Agent Guidelines

## Project Overview

Coral is a composable CRDT library for collaborative applications, written in
Rust. It is in early development; the focus is on establishing the core type
system before building containers, operation logs, and state machines.

## Project Structure

```
src/
├── common/
│   ├── mod.rs       # Re-exports CoralError
│   └── error.rs     # Error type backed by thiserror
├── rle/
│   ├── mod.rs       # RLE traits: HasLength, Mergable, Sliceable
│   └── rle_vec.rs   # Run-length encoded vector
├── types/
│   ├── container.rs # ContainerType enum (currently only Counter)
│   ├── mod.rs       # Types module entry point
│   ├── op_id.rs     # OpId: globally unique operation identifier
│   ├── primitives.rs# Core type aliases: PeerID, Counter, Lamport, Timestamp
│   └── value.rs     # Value enum: Null, Bool, I64, Double
└── lib.rs           # Library entry point
```

## Code Style

- Do not use long inline path references like `crate::module::Type::Variant`.
  Always import the type with `use` at the top of the file and refer to it by
  short name.
- Run `cargo fmt` after writing or modifying any Rust source file.
- Run `cargo clippy --all-features -- -D warnings` and fix all warnings before
  committing.
