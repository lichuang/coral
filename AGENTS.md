# Coral - Agent Guide

## Project Overview

**Coral** is a Rust implementation of CRDTs (Conflict-free Replicated Data Types). The goal is to provide a complete, embeddable local-first data structure without relying on external runtimes (such as Yjs/Loro's WASM bindings or specific transport layers).

This project is a **complete re-implementation of Loro** in Rust. The goal is to achieve full structural and behavioral parity with Loro's CRDT core — including all container types, algorithms, encoding formats, and advanced features. Development proceeds in phases for manageability, but no feature is permanently excluded; every Loro capability is on the roadmap to be ported.

### Alignment with Loro

Every feature that **is already implemented** in Coral must stay **structurally and behaviorally aligned** with Loro's counterpart (types, traits, field names, method signatures, and core semantics). Partial implementation per phase is acceptable—e.g., a Phase may only introduce the Counter CRDT while List remains a stub—but whatever exists must match Loro's design so that future porting of algorithms (checkout, diff, encoding) can be done with minimal friction.

### Core Design Principles

1. **OpLog and DocState separation**: OpLog stores all history (DAG); DocState stores the current state. DocState can be replayed and rebuilt from any version in the OpLog.
2. **Transaction boundaries**: Each user editing session is encapsulated as a `Transaction` → `Change` (containing multiple `Op`s), rather than submitting Op-by-Op.
3. **Compact internal indexing**: The API layer exposes `ContainerID`, while internally everything uses `ContainerIdx` (a 4-byte compact index) to reduce memory overhead.
4. **Lazy loading**: Container states are created only when first accessed.
5. **Determinism**: Any two nodes with identical Frontiers must have identical states (assuming apply order is guaranteed by DAG topological ordering).

---

## Architecture Overview

```
┌──────────────────────────────────────────────────────┐
│                    User API Layer                      │
│  CounterHandler / MapHandler / ListHandler / ...     │
├──────────────────────────────────────────────────────┤
│                   CoralDoc (Top Level)                 │
│  peer_id │ pending_ops │ arena │ oplog │ state        │
├──────────┴─────────────────────┼────────┴─────────────┤
│           OpLog                │       DocState       │
│  DAG(Change) │ vv │ frontiers │  ContainerState Set  │
├────────────────────────────────┴──────────────────────┤
│              Arena (ContainerID ↔ ContainerIdx)        │
├──────────────────────────────────────────────────────┤
│         Basic Types (ID / Span / Value / ...)          │
└──────────────────────────────────────────────────────┘
```

### Module Responsibilities

| Module | File/Directory | Responsibility |
|--------|----------------|----------------|
| Basic Types | `src/types.rs`, `src/id.rs`, `src/span.rs`, `src/value.rs`, `src/container_id.rs` | Pure data structures, no business logic |
| Change | `src/change.rs` | `Change`: an atomic unit of a transaction commit |
| Version | `src/version.rs` | `VersionVector`, `Frontiers` |
| Causal Graph | `src/dag.rs` | `Dag<ID>`, topological ordering, LCA, diff_changes |
| Operation Log | `src/oplog.rs` | `OpLog`, history storage, import/export, pending queue |
| Memory Management | `src/arena.rs` | `Arena`, `ContainerID` ↔ `ContainerIdx` mapping, parent-child relationships |
| Document | `src/doc.rs` | `CoralDoc`, user entry point, transaction lifecycle management |
| Transaction | `src/txn.rs` | `Transaction`, local editing batch buffering and commit |
| State Bus | `src/state.rs` | `DocState`, collection and dispatch of all container states |
| Container Trait | `src/container_state.rs` | `ContainerState` trait, `Diff` enum |
| Operation Definitions | `src/op.rs`, `src/op/*.rs` | `Op`, `OpContent`, specific operation types for each container |
| Container States | `src/state/*.rs` | State implementations for each CRDT (Counter/Map/List/Text/Tree) |
| Register | `src/state/lww_register.rs` | `LWWRegister<T>`, foundation for Map and deletion semantics |
| Fractional Index | `src/fractional_index.rs` | `FractionalIndex`, sort key for Tree and MovableList |
| Handlers | `src/handler/*.rs` | User-friendly APIs, pos → ID conversion, automatic transaction management |

---

## Coding Standards

### 1. Type Visibility

- **Public (`pub`)**: Only `CoralDoc`, each `Handler`, and basic types (`ID`, `ContainerID`, `LoroValue`, `ContainerType`)
- **Crate-internal (`pub(crate)`)**: `OpLog`, `DocState`, `Arena`, `ContainerState` trait, `Change`
- **Module-internal (default)**: Specific state implementation details, internal helper functions

### 2. Error Handling

- Use `thiserror` to define a unified `CoralError`
- Internal logic (e.g., `apply_op`) should `panic!` on type mismatch (treat as invariant violation, not recoverable)
- User input (e.g., invalid pos, non-existent key) returns `Result<T, CoralError>`

```rust
#[derive(thiserror::Error, Debug)]
pub enum CoralError {
    #[error("Container not found: {0}")]
    ContainerNotFound(ContainerID),
    #[error("Invalid position: {0}")]
    InvalidPosition(usize),
    #[error("Type mismatch: expected {expected}, got {got}")]
    TypeMismatch { expected: ContainerType, got: ContainerType },
    #[error("DAG invariant violated: missing dependency {0:?}")]
    MissingDependency(ID),
}
```

### 3. Naming Conventions

| Type | Naming |
|------|--------|
| User API structs | `CoralDoc`, `CounterHandler`, `MapHandler` |
| Internal core structs | `OpLog`, `DocState`, `Arena`, `Dag` |
| Traits | `ContainerState` (noun, no suffix) |
| Concrete states | `CounterState`, `MapState`, `ListState` |
| Operation types | `CounterOp`, `MapOp`, `ListOp` (or `ListOpInternal`) |
| Diff types | `CounterDiff`, `MapDiff`, `ListDiff` |
| Compact index | `ContainerIdx` (not `ContainerIndex`) |
| ID type | `ID` (all uppercase, consistent with Loro, despite Rust conventions) |

### 4. Documentation Comments

- Every `pub` type and function must have a `///` doc comment
- Complex algorithms (RGA insert sorting, Fugue, Tree LWW Move) must have inline comments explaining the core logic
- Use `// TODO:` for known debt, `// NOTE:` for design decisions that are easy to trip over

### 5. Avoid Premature Abstraction

- **Do not** introduce complex generic parameters or macro generation for each CRDT; hand-write first, then extract commonalities
- **Defer** encoding/serialization logic to later phases; prioritize algorithmic correctness first, but design interfaces so serialization can plug in seamlessly
- **Allow** temporary `unimplemented!()` or `todo!()`, but they must be marked with phase plans

---

## Relationship with Loro and Key Differences

### Preserved Designs

- OpLog / DocState separation
- `ContainerIdx` compact indexing + `Arena`
- `Change` as transaction boundary, with `deps` + `lamport` + `timestamp`
- DAG causal graph + `Frontiers` + `VersionVector`
- LWW-Register as the foundation for Map and deletion semantics
- Algorithm choices: RGA List, Fugue Text, Movable Tree

### Intentional Divergences from Loro

Some Loro design choices are deliberately **not** preserved in Coral because they add complexity without benefit for a full re-implementation:

| Loro Design | Coral Approach | Rationale |
|-------------|----------------|-----------|
| `FutureInnerContent` enum (`Counter` + `Unknown`) | `Counter(f64)` is a first-class variant of `OpContent` | Coral controls all source code; there is no need for a forward-compatible catch-all. New container types are added directly to `OpContent` / `RawOpContent` rather than shoe-horned into a `Future` wrapper. |
| `ContainerType::Unknown(u8)` | `ContainerType` only contains known types; `try_from_u8` returns `None` for unrecognized bytes | Same rationale — Coral is a full re-implementation, not a client that needs to gracefully ignore unknown container types from a newer Loro version. |

### Phased Implementation Plan

The following Loro features are **not yet implemented** in Coral. The table below lists what is *missing*; every item must eventually be brought to parity with Loro's actual design.

> ⚠️ **Do not treat the left column as design guidance.** Phrases like "placeholder" or "not started" mean the feature is absent — not that it is permanently excluded.

| Loro Feature | What's Missing in Coral (TODO) | Loro's Actual Design (Target) |
|--------------|--------------------------------|-------------------------------|
| Binary encoding (fast_snapshot, shallow_snapshot) | Only trait interface exists; encoding logic not implemented | Full fast / shallow snapshot encoding |
| JSON Schema encoding | Only serde_json export exists; compact schema not implemented | Compact JSON Schema with peer compression |
| KV-Store persistence | No persistence layer; everything is in-memory only | Pluggable KV-store backend |
| WASM FFI / JS API | Not implemented | Full wasm-bindgen API surface |
| Awareness (cursor/selection collaboration) | Not implemented | Complete awareness protocol |
| UndoManager | Not implemented | Full UndoManager with grouping |
| generic-btree (high-performance Rope/BTree) | Using temporary HashMap + linked list; btree not implemented | generic-btree or equivalent |
| Multi-threading (Arc<Mutex>, etc.) | Arc/Mutex wrappers not yet added | Full Arc/Mutex + loom testing |
| Event subscription system (Observer/Subscription) | Subscription system not implemented; only manual diff available | Full subscription / observer system |

### ChangeStore: In-Memory Only (For Now)

Coral's `ChangeStore` (Phase 7) is deliberately implemented as an **in-memory-only** structure. The reasons are:

1. **Algorithmic correctness first** — persistence introduces I/O, async boundaries, and error-recovery complexity that distracts from getting the CRDT core right.
2. **Interface compatibility** — the `ChangeStore` API (`insert_change`, `get_change`, `iter_changes`) is designed so that a future KV-store backend can be swapped in without touching `OpLog` or `DocState`.
3. **Shallow-snapshot readiness** — the `ChangesBlock` design (holding either parsed `Changes`, raw `Bytes`, or `Both`) is kept structurally aligned with Loro so that lazy loading and shallow snapshots can be added later.

A pluggable persistence layer (IndexedDB, RocksDB, etc.) is on the long-term roadmap but **explicitly deferred** until all CRDT containers and the event system are stable.

**Every design decision must keep the door open for the full Loro feature to land later with minimal refactoring.**

---

## Development Principles and Checklist

### Phase Progress Tracking

`phase.md` is the single source of truth for project progress. Every time you complete a task from `phase.md`:

- [ ] **Update `phase.md`** immediately: change the checkbox from `- [ ]` to `- [x]` for the completed item.
- [ ] Do not batch these updates. Mark the task as done in the same turn where the implementation is finished.
- [ ] If a task is partially completed or blocked, leave it unchecked and add a `<!-- NOTE: ... -->` comment below it explaining the blocker.
- [ ] **Use only `[ ]` and `[x]` for task status**. Do not use textual markers like "已完成", "DONE", "pending", or "in progress" anywhere in `phase.md`. Checkboxes are the single source of truth for completion state.
- [ ] **Update the statistics table at the end of `phase.md`** whenever a phase's completion state changes significantly. Count all checkbox lines (`- [x]` and `- [ ]`) in the phase, then update the "已完成 / 未完成 / 完成率" columns and the "合计" row accordingly.

### When Implementing a New CRDT, You Must Check

- [ ] All methods of the `ContainerState` trait are implemented
- [ ] `to_diff` and `apply_diff` are inverse operations: applying `to_diff()` output to an empty state should yield the original state
- [ ] Applying the same `Op` twice produces the same result (idempotency)
- [ ] Two documents executing the same set of operations (in different orders) end up with consistent `frontiers` and `get_value()` (commutativity)
- [ ] Concurrent conflict scenarios have deterministic outcomes (list at least 2 concurrent cases in tests)
- [ ] Handler APIs correctly convert pos/key to internal ID references

### Things You Must NOT Do

- **Do NOT directly modify another container's state inside `ContainerState::apply_local_op`**. Cross-container effects (e.g., Tree deletion invalidating child containers) should be coordinated at the `DocState` or `CoralDoc` layer.
- **Do NOT expose `VersionVector` or `Frontiers` inside state implementations**. Causal logic belongs only to `OpLog`/`CoralDoc`.
- **Do NOT assume operations arrive in counter order**. Remote Changes may arrive out of order or duplicated; the import layer handles deduplication and pending queue management.
- **Do NOT use `BTreeMap<ID, _>` iteration order as document order**. ID lexicographic order ≠ RGA/Fugue document order.

---

## Frequently Asked Questions (FAQ for Agent)

### Q: Is `Counter`'s concurrent semantics LWW or arithmetic merge?

A: Arithmetic merge (PN-Counter). If A increments by +3 and B increments by -2, the merged result is +1. Internally, just accumulate `delta`; idempotency is guaranteed by OpLog deduplication.

### Q: Is `Map` deletion physical or logical?

A: Logical deletion (LWW tombstone). Internally retains `LWWRegister::value = None`, filtered out during `get_value()`. This is because subsequent concurrent inserts to the same key need to participate in LWW comparison.

### Q: Where do `List` elements go after `delete`?

A: Marked as `deleted = true` (tombstone), remaining in `HashMap<ID, ListElement>`. Document order traversal (`len()`, `get(pos)`) skips them. Tombstones ensure that concurrent inserts after a deleted element still have an anchor point.

### Q: When is a `Tree` node's metadata Map created?

A: Automatically created during `TreeHandler::create()`, using `ContainerID::new_normal(tree_id.into_id(), ContainerType::Map)` as the associated container ID. TreeState internally maintains `meta_map: HashMap<TreeID, ContainerIdx>`.

### Q: Text is initially a List-based simplified version; how to seamlessly replace it with Fugue later?

A: `TextHandler` API (`insert`/`delete`/`to_string`) remains unchanged. `TextState` internal structure is replaced from a `ListState` wrapper to `FugueState`. As long as the `ContainerState` trait implementation remains unchanged, upper layers are unaware. Before replacement, ensure sufficient property-based tests lock down the behavior.

### Q: When does `FractionalIndex` need to expand in length?

A: When `between(a, b)` finds no available byte value between `a` and `b` (e.g., `a = [0x00, 0xFF]`, `b = [0x01]`), append an intermediate byte (e.g., `[0x00, 0xFF, 0x80]`). For initial implementation, do not consider space reclamation; append-only is sufficient.

---

## Testing Conventions

### Unit Test Placement

```
src/
  state/
    counter_state.rs      # Tests at bottom of file in mod tests { ... }
    map_state.rs
    ...
```

### Integration Test Placement

```
tests/
  crdt_properties.rs     # proptest: random operation sequences, check invariants
  merge_sync.rs          # Two-document merge, concurrent conflicts
  checkout.rs            # Version rollback and time travel
```

### Required Test Templates (using Map as an example)

```rust
#[test]
fn map_single_op() { ... }

#[test]
fn map_idempotent() { ... }

#[test]
fn map_concurrent_insert() {
    // peer A insert("k", "A")
    // peer B insert("k", "B")
    // merged result determined by LWW
}

#[test]
fn map_concurrent_insert_delete() {
    // peer A insert("k", "v")
    // peer B delete("k")
    // merged result determined by LWW (higher lamport wins)
}

#[test]
fn map_merge_commutative() {
    // A edits 100 times, B edits 100 times
    // A.import(B.export()); B.import(A.export());
    // assert_eq!(A.get_value(), B.get_value());
}
```

---

## Toolchain

- **Rust Version**: stable latest (no nightly required)
- **Suggested `Cargo.toml` dependencies**:
  - `indexmap`: ordered Map
  - `serde` + `serde_json`: JSON serialization
  - `thiserror`: error types
  - `proptest` (dev-dependency): property-based testing
- **Formatting**: `rustfmt` default configuration
- **Lint**: `cargo clippy -- -D warnings`

---

## Code Quality Checks

Every time you write or modify code, you **must** run the following checks before considering the task complete. Do not skip them.

### Required Commands

```bash
# 1. Formatting Check — verify code follows rustfmt style
cargo fmt --check

# 2. Clippy Check — catch lints, warnings, and common mistakes
cargo clippy -- -D warnings

# 3. Test Check — run all existing tests to prevent regressions
cargo test
```

### Rules

- [ ] **Formatting Check must pass**. If `cargo fmt --check` fails, run `cargo fmt` to auto-fix and review the diff.
- [ ] **Clippy Check must pass with zero warnings**. The `-D warnings` flag treats all warnings as errors. Fix or explicitly allow with a documented reason.
- [ ] **All tests must pass**. If a test fails, fix the code or the test. Do not delete or disable existing tests to make them pass.
- [ ] **Run all three in sequence** — not just the one you think is relevant. A formatting change can break tests, and a logic fix can introduce new clippy warnings.
- [ ] **Check both `lib.rs` and integration tests in `tests/`**. If you added a new test file, make sure it is picked up by `cargo test`.

### When Checks Can Be Deferred

Never. Even for trivial one-line changes, run all three. The only exception is when you are explicitly told to skip them in a debug/ exploratory session — but you must run them before marking the task as done in `phase.md`.

---

## Phase Guidance Quick Reference

If you are asked to work on a specific phase, first confirm its prerequisites:

| Phase | Prerequisites | Validation After Completion |
|-------|---------------|----------------------------|
| Phase 1 (Basic Types) | None | Compiles; types can be correctly created and compared |
| Phase 2 (Infrastructure) | Phase 1 | CoralDoc can be created/committed/got value; OpLog DAG is acyclic |
| Phase 3 (Counter) | Phase 2 | OpLog → State full pipeline works; two Counters merge correctly |
| Phase 4-5 (Register/Map) | Phase 3 | LWW conflicts have deterministic results; Map nested sub-containers have correct Arena parent-child relationships |
| Phase 6 (List) | Phase 5 | RGA concurrent insert order is deterministic; pos → ID conversion is correct; tombstone handling is correct |
| Phase 7 (MovableList) | Phase 6 | Move LWW conflicts are deterministic; document order rebuild is correct |
| Phase 8 (Text) | Phase 7 | Simplified version verifies first; later Fugue replacement preserves behavior |
| Phase 9 (RichText) | Phase 8 | Style LWW conflicts are deterministic; mark/unmark produces correct to_delta output |
| Phase 10 (Tree) | Phase 9 | Cycle detection is correct; metadata Map created/deleted with node; FractionalIndex sorting is correct |
| Phase 11 (Merge/Sync) | Phase 10 | Two random documents merge consistently; out-of-order import + pending queue works |
| Phase 12 (Checkout) | Phase 11 | Checkout to any historical version and rollback preserves state; fork → independent edit → merge works |
