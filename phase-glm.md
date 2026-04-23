# Coral - 实现计划（对齐 Loro 架构）

> 本文档基于对 Loro 源码（`/Users/codedump/source/rs/loro/crates/`）的完整分析。
> 每个 `[x]` 项已在 `src/` 中验证存在并通过测试；每个 `[ ]` 项尚未实现。
> `[ ]` 项包含完整的 Rust 类型签名和实现指南，可直接作为编码依据。

---

## 阶段总览

```
Phase 1: 基础类型与 RLE 基础设施    ← 大部分完成，Frontiers 需重构
    ↓
Phase 2: 独立 CRDT 状态实现          ← 全部未开始
    ↓
Phase 3: ContainerState trait 与 Diff ← 全部未开始
    ↓
Phase 4: 文档运行时                  ← 全部未开始
    ↓
Phase 5: 协作、事件与版本控制         ← 全部未开始
    ↓
Phase 6: 高级与优化                  ← 全部未开始
```

---

## Phase 1: 基础类型与 RLE 基础设施

### 1.1 类型别名

- [x] `PeerID = u64`
- [x] `Counter = i32`
- [x] `Lamport = u32`
- [x] `Timestamp = i64`

文件：`src/types/primitives.rs`

```rust
pub type PeerID = u64;
pub type Counter = i32;   // 操作计数器，每个 peer 从 0 单调递增
pub type Lamport = u32;   // Lamport 时间戳，LWW 比较用
pub type Timestamp = i64; // 物理时间戳（秒）
```

---

### 1.2 操作 ID

- [x] `ID` 结构体
- [x] `new()`, `is_root()`, `inc()`
- [x] `Ord` 实现（先比 peer，再比 counter）
- [x] `Hash` 实现
- [x] 单元测试

文件：`src/types/id.rs`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ID {
    pub peer: PeerID,
    pub counter: Counter,
}
// Ord: 先 peer 后 counter，保证全局确定性排序
```

> **对齐确认**：Loro 的 `ID` 字段名和排序规则与此一致。

---

### 1.3 ContainerType 枚举

- [x] `ContainerType` 枚举（Map=0, List=1, Text=2, Tree=3, MovableList=4, Counter=5, Unknown）
- [x] `#[repr(u8)]`
- [x] `to_u8()` / `try_from_u8()`
- [x] `Display` / `FromStr`
- [x] `Unknown(u8)` 前向兼容变体
- [x] 单元测试

文件：`src/types/container.rs`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum ContainerType {
    Map = 0, List = 1, Text = 2, Tree = 3,
    MovableList = 4, Counter = 5, Unknown(u8),
}
```

> **对齐确认**：Loro 有相同的 `Unknown` 变体用于前向兼容。

---

### 1.4 ContainerID

- [x] `ContainerID` 枚举（Root / Normal）
- [x] `new_root()`, `new_normal()`
- [x] `container_type()`
- [x] `Display` / `FromStr` 往返
- [x] 单元测试

文件：`src/types/container.rs`

```rust
pub enum ContainerID {
    Root { name: String, container_type: ContainerType },
    Normal { peer: PeerID, counter: Counter, container_type: ContainerType },
}
```

---

### 1.5 CoralValue（JSON 值）

- [x] `CoralValue` 枚举
- [x] Arc-backed `CoralStringValue`, `CoralListValue`, `CoralMapValue`, `CoralBinaryValue`
- [x] `to_json()` / `from_json()`
- [x] `Hash` 实现（f64 用 `to_bits()`）
- [x] `Eq` 手动实现
- [x] 单元测试

文件：`src/types/value.rs`

```rust
pub enum CoralValue {
    Null, Bool(bool), I64(i64), Double(f64),
    Binary(CoralBinaryValue), String(CoralStringValue),
    List(CoralListValue), Map(CoralMapValue),
    Container(ContainerID),
}
```

> **NOTE**：当前 `CoralMapValue` 底层用 `FxHashMap`（无序）。Loro 的 Map 遍历是有序的（`IndexMap`）。后续如果需要保序语义，需替换为 `IndexMap`。

---

### 1.6 ContainerIdx（紧凑容器句柄）

- [x] `ContainerIdx(u32)` 结构体
- [x] top-5-bits 存储 ContainerType，low-27-bits 存储索引
- [x] `from_index_and_type()`, `get_type()`, `to_index()`, `is_unknown()`
- [x] 单元测试

文件：`src/core/container.rs`

```rust
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Hash)]
pub struct ContainerIdx(u32);
```

> **对齐确认**：Loro 的 `ContainerIdx` 使用相同的类型编码方案。

---

### 1.7 Arena（ContainerID ↔ ContainerIdx 映射）

- [x] `Arena` 结构体
- [x] `register()`, `get_id()`, `get_idx()`
- [x] `set_parent()`, `get_parent()`
- [x] 单元测试

文件：`src/core/arena.rs`

```rust
pub struct Arena {
    id_to_idx: FxHashMap<ContainerID, ContainerIdx>,
    idx_to_id: Vec<ContainerID>,
    parents: Vec<Option<ContainerIdx>>,
}
```

> **Phase 4 注意**：Loro 中 `SharedArena` 通过 `Arc` 在 OpLog 和 DocState 之间共享。Coral 为单线程，但 OpLog 和 DocState 必须引用同一个 Arena 实例，不能各自持有副本。Phase 4 设计时需确保这一点。

---

### 1.8 Op（紧凑存储形式）

- [x] `Op` 结构体
- [x] `OpContent` 枚举（占位符变体）
- [x] `OpWithId` 辅助结构
- [x] RLE traits（`HasLength`, `Sliceable`, `Mergable`, `HasIndex`）已为 `Op` 实现
- [x] 单元测试

文件：`src/op/mod.rs`, `src/op/content.rs`

```rust
pub struct Op {
    pub counter: Counter,
    pub container: ContainerIdx,
    pub content: OpContent,
}

pub enum OpContent {
    Map(MapOp), List(ListOp), Text(TextOp),
    Tree(TreeOp), Counter(CounterOp),
}
```

> **占位符状态**：`MapOp`, `ListOp`, `TextOp`, `TreeOp`, `CounterOp` 目前是空结构体。Phase 2 将逐个替换为完整定义。

---

### 1.9 RawOp（运行时完整形式）

- [ ] `RawOp` 结构体定义
- [ ] `RawOpContent` 枚举定义

```rust
// src/op/raw.rs（新文件）

/// 完整运行时形式的操作。
/// 从 Change + Op + Arena 动态构建，包含 apply_local_op 所需的全部因果上下文。
#[derive(Debug, Clone)]
pub struct RawOp {
    pub id: ID,
    pub lamport: Lamport,
    pub container: ContainerIdx,
    pub content: RawOpContent,
}

/// 完整形式的操作内容。
/// 对于 Counter，直接是 f64；对于复杂类型，包含额外运行时数据。
#[derive(Debug, Clone)]
pub enum RawOpContent {
    Map(MapOp),
    List(ListOp),
    Text(TextOp),
    Tree(TreeOp),
    Counter(f64),  // Loro 中 CounterOp 的运行时形式就是 f64
}
```

> **为什么需要 RawOp**：`ContainerState::apply_local_op` 需要 lamport（LWW 比较）和完整 ID（去重）。这些信息在 `Op` 层面不存在（peer/lamport 在 Change 级别）。Phase 3 定义 trait 时需要此类型，但实际构建发生在 Phase 4 的 DocState 中。

---

### 1.10 Change（变更组）

- [x] `Change<O = Op>` 结构体
- [x] `ops: RleVec<[O; 1]>`（非 `Vec<Op>`）
- [x] `new()`, `peer()`, `lamport()`, `timestamp()`, `id()`, `deps()`, `len()`, `id_last()`, `id_end()`
- [x] `can_merge_right()`
- [x] RLE traits：`HasLength`, `HasIndex`, `Mergable`（永远 false）, `Sliceable`（完整实现，含二分搜索优化）
- [x] 单元测试

文件：`src/core/change.rs`

```rust
pub struct Change<O = Op> {
    pub(crate) id: ID,
    pub(crate) lamport: Lamport,
    pub(crate) deps: Frontiers,
    pub(crate) timestamp: Timestamp,
    pub(crate) commit_msg: Option<Arc<str>>,
    pub(crate) ops: RleVec<[O; 1]>,
}
```

> **对齐确认**：Loro 的 `Change` 也使用 `RleVec<[O; 1]>` 存储 ops。`Change::slice` 是 checkout/time-travel 的核心操作。

---

### 1.11 RLE 核心 Traits

- [x] `HasLength`（`content_len` + `atom_len`）
- [x] `Sliceable`
- [x] `Mergable<Cfg = ()>`
- [x] `HasIndex`（`get_start_index` + `get_end_index`）
- [x] `GlobalIndex` trait bound
- [x] `RlePush<T>` trait + impl for Vec / SmallVec
- [x] `Slice<T>`, `SearchResult`, `SliceIterator`
- [x] `RleCollection<T>` trait

文件：`src/rle/mod.rs`

```rust
pub trait HasLength {
    fn content_len(&self) -> usize;
    fn atom_len(&self) -> usize { self.content_len() }
}
pub trait Sliceable {
    fn slice(&self, from: usize, to: usize) -> Self;
}
pub trait Mergable<Cfg = ()> {
    fn is_mergable(&self, other: &Self, conf: &Cfg) -> bool { false }
    fn merge(&mut self, other: &Self, conf: &Cfg) { unreachable!() }
}
pub trait HasIndex: HasLength {
    type Int: GlobalIndex;
    fn get_start_index(&self) -> Self::Int;
}
```

---

### 1.12 RleVec

- [x] `RleVec<A: Array>` 结构体
- [x] `push()` 自动合并
- [x] `slice_by_index()` 基于索引切片
- [x] `search_atom_index()` 二分搜索
- [x] `iter_by_index()`, `slice_iter()`
- [x] `From<Vec>`, `From<&[T]>`, `FromIterator`
- [x] `Mergable` / `Sliceable` for `RleVec` 自身
- [x] 单元测试

文件：`src/rle/rle_vec.rs`

> **对齐确认**：与 Loro 的 `RleVec` API 一致。

---

### 1.13 Span 类型

- [x] `CounterSpan { start, end }` + RLE traits
- [x] `IdSpan { peer, counter: CounterSpan }` + RLE traits
- [x] 单元测试

文件：`src/version/span.rs`

```rust
pub struct CounterSpan { pub start: Counter, pub end: Counter }
pub struct IdSpan { pub peer: PeerID, pub counter: CounterSpan }
```

---

### 1.14 VersionVector

- [x] `VersionVector(FxHashMap<PeerID, Counter>)` newtype 结构体
- [x] `new()`, `set_last()`, `get()`, `get_last()`, `set_end()`
- [x] `merge()`, `includes()`, `includes_id()`
- [x] `diff()`, `diff_iter()`, `sub_iter()`, `sub_vec()`, `distance_between()`, `intersection()`
- [x] `extend_to_include()`, `shrink_to_exclude()`, `forward()`, `retreat()`
- [x] `get_frontiers()`
- [x] `PartialOrd` 实现
- [x] `ImVersionVector(im::HashMap)` 不可变版本向量
- [x] VV ↔ ImVV 转换
- [x] 单元测试

文件：`src/version/mod.rs`

```rust
pub struct VersionVector(FxHashMap<PeerID, Counter>);
pub struct ImVersionVector(im::HashMap<PeerID, Counter, rustc_hash::FxBuildHasher>);
```

> **对齐确认**：Loro 的 `VersionVector` 也是 `FxHashMap` newtype。`ImVersionVector` 对应 Loro 的 `ImVersionVector`，使用 `im` crate 实现结构共享。

---

### 1.15 VersionVectorDiff

- [x] `IdSpanVector` 类型别名
- [x] `VersionVectorDiff { retreat, forward }` 结构体
- [x] `merge_left()`, `merge_right()`, `subtract_start_left/right()`
- [x] `get_id_spans_left()`, `get_id_spans_right()`
- [x] 单元测试

文件：`src/version/diff.rs`

```rust
pub type IdSpanVector = FxHashMap<PeerID, CounterSpan>;
pub struct VersionVectorDiff {
    pub retreat: IdSpanVector,
    pub forward: IdSpanVector,
}
```

---

### 1.16 Frontiers

- [x] `Frontiers(Vec<ID>)` 当前实现（可用但需优化）
- [x] `new()`, `from_id()`, `push()`, `as_single()`, `contains()`, `iter()`
- [x] `update_frontiers_on_new_change()`
- [ ] **重构为 Loro 风格三态枚举**（性能优化，非阻塞）

文件：`src/version/frontiers.rs`

当前：
```rust
pub struct Frontiers(Vec<ID>);
```

目标（对齐 Loro）：
```rust
/// Loro 的 Frontiers 是三态枚举：
/// - None: 空文档
/// - ID: 线性历史（最常见，零分配）
/// - Map: 并发编辑（多前沿）
pub enum Frontiers {
    None,
    ID(ID),
    Map(InternalMap),
}

/// InternalMap = Arc<FxHashMap<PeerID, Counter>>
/// 支持廉价的 clone（Arc 共享）
pub struct InternalMap(Arc<FxHashMap<PeerID, Counter>>);
```

> **为什么重构**：线性历史是绝大多数场景。三态枚举在单 ID 情况下零堆分配，`Vec<ID>` 则至少一次。Loro 的 `shrink_frontiers` 会在并发合并后将 Map 缩回 ID/None。此重构优先级为中，可在 Phase 4 之前或期间完成。

---

### 1.17 RLE traits 为已有类型实现

- [x] `HasLength`, `Sliceable`, `Mergable`, `HasIndex` for `CounterSpan`
- [x] `HasLength`, `Sliceable`, `Mergable`, `HasIndex` for `IdSpan`
- [x] `HasLength`, `Sliceable`, `Mergable` for `OpContent`（占位符：原子 len=1，不可合并）
- [x] `HasLength`, `Sliceable`, `Mergable`, `HasIndex` for `Op`
- [x] `HasLength`, `HasIndex`, `Mergable`, `Sliceable` for `Change`
- [x] `Sliceable`, `Mergable`, `HasLength`, `HasIndex` for `Range<T>`
- [x] `Sliceable` for `SmallVec`

---

### Phase 1 验收标准

- [x] `cargo test` 全部通过
- [x] `cargo fmt --check` 通过
- [x] `cargo clippy -- -D warnings` 通过
- [x] ID 排序确定性：先 peer 后 counter
- [x] VersionVector 包含/合并/diff 正确
- [x] CounterSpan / IdSpan 切片/合并正确
- [x] RleVec push 自动合并、slice 正确
- [x] Change::slice 正确调整 id/deps/lamport

---

## Phase 2: 独立 CRDT 状态实现

> **核心原则**：每个 CRDT 是自包含的状态机。只需 `Op` 和 `CoralValue` 即可工作。
> **不依赖** CoralDoc / OpLog / Arena / Transaction。
> **验证方式**：独立单元测试验证幂等性、交换律、结合律、并发冲突。

> **Phase 2 → Phase 3 衔接策略**：Phase 2 的 State 先实现 `apply_local_op(&mut self, raw_op: &RawOp)` 和 `get_value(&mut self) -> CoralValue`（这是 ContainerState trait 的核心子集）。Phase 3 只需补全 `to_diff` / `apply_diff` 等方法，无需重签名。

---

### 2.1 错误类型

- [ ] `CoralError` 错误枚举

```rust
// src/error.rs（新文件）

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

> **依赖**：需要在 `Cargo.toml` 添加 `thiserror`。

---

### 2.2 CounterOp

- [ ] `CounterOp` 完整定义（替换占位符）

```rust
// src/op/counter_op.rs（替换 content.rs 中的占位符）

#[derive(Debug, Clone, PartialEq)]
pub struct CounterOp {
    pub delta: f64,  // Loro 中 Counter 使用 f64，非 i64
}
```

> **为什么 f64**：Loro 的 Counter 支持 `f64` 增量（如 +0.5）。`Diff::Counter` 在 Loro 中就是 `f64`。使用 i64 会导致后续 API 不兼容。

---

### 2.3 CounterState（PN-Counter）

- [ ] `CounterState` 结构体
- [ ] `apply_op()` 方法
- [ ] `get_value()` 方法
- [ ] `merge()` 方法（独立测试用）
- [ ] 单元测试

```rust
// src/state/counter_state.rs（新文件）

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CounterState {
    /// 当前聚合值
    value: f64,
    /// 各 peer 的 delta 累加（调试和 diff 用）
    per_peer: FxHashMap<PeerID, f64>,
}

impl CounterState {
    pub fn new() -> Self { Self::default() }

    pub fn apply_op(&mut self, op: &CounterOp, peer: PeerID) {
        self.value += op.delta;
        *self.per_peer.entry(peer).or_insert(0.0) += op.delta;
    }

    pub fn get_value(&self) -> CoralValue {
        CoralValue::Double(self.value)
    }

    /// 独立测试用：合并另一个 CounterState
    pub fn merge(&mut self, other: &Self) {
        for (&peer, &delta) in &other.per_peer {
            if let Some(my_delta) = self.per_peer.get_mut(&peer) {
                let diff = delta - *my_delta;
                self.value += diff;
                *my_delta = delta;
            } else {
                self.value += delta;
                self.per_peer.insert(peer, delta);
            }
        }
    }
}
```

**测试要求**：
- [ ] 单操作：`apply(+3.0)` 后 `value == 3.0`
- [ ] 负增量：`apply(-2.0)` 后 `value == -2.0`
- [ ] 浮点增量：`apply(+0.5)` 后 `value == 0.5`
- [ ] 交换律：`A.merge(B) == B.merge(A)`
- [ ] 结合律：`(A.merge(B)).merge(C) == A.merge(B.merge(C))`
- [ ] 并发冲突：A 加 3.0，B 减 2.0，合并后为 +1.0

---

### 2.4 LWW-Register

- [ ] `LWWRegister<T>` 结构体
- [ ] `update()` / `merge()` 方法
- [ ] 单元测试

```rust
// src/state/lww_register.rs（新文件）

#[derive(Debug, Clone, PartialEq)]
pub struct LWWRegister<T> {
    pub value: Option<T>,   // None = 已删除
    pub lamport: Lamport,
    pub peer: PeerID,       // tie-break
}

impl<T: Clone> LWWRegister<T> {
    pub fn new() -> Self {
        Self { value: None, lamport: 0, peer: 0 }
    }

    /// LWW 合并：lamport 大的覆盖小的；相同时 peer 大的赢
    pub fn merge(&mut self, other: &Self) {
        match self.lamport.cmp(&other.lamport) {
            std::cmp::Ordering::Less    => { *self = other.clone(); }
            std::cmp::Ordering::Greater => {}
            std::cmp::Ordering::Equal   => {
                if self.peer < other.peer {
                    *self = other.clone();
                }
            }
        }
    }

    pub fn update(&mut self, value: Option<T>, lamport: Lamport, peer: PeerID) {
        let other = LWWRegister { value, lamport, peer };
        self.merge(&other);
    }
}
```

**测试要求**：
- [ ] Lamport 大的覆盖小的
- [ ] Lamport 相同时 peer 大的赢
- [ ] `None`（删除）参与 LWW 比较
- [ ] 合并交换律

---

### 2.5 MapOp

- [ ] `MapOp` 完整定义（替换占位符）

```rust
// src/op/map_op.rs

#[derive(Debug, Clone, PartialEq)]
pub enum MapOp {
    Insert { key: String, value: CoralValue },
    Delete { key: String },
}
```

---

### 2.6 MapState

- [ ] `MapState` 结构体
- [ ] `apply_op()`, `get()`, `get_value()`, `keys()`, `merge()`
- [ ] 单元测试

```rust
// src/state/map_state.rs（新文件）

#[derive(Debug, Clone)]
pub struct MapState {
    /// 每个 key 一个 LWW-Register。
    /// 保留 tombstone（value == None）以支持并发删除后插入的 LWW 比较。
    registers: FxHashMap<String, LWWRegister<CoralValue>>,
}

impl MapState {
    pub fn new() -> Self { Self { registers: FxHashMap::default() } }

    pub fn apply_op(&mut self, op: &MapOp, lamport: Lamport, peer: PeerID) {
        match op {
            MapOp::Insert { key, value } => {
                let reg = self.registers.entry(key.clone())
                    .or_insert_with(LWWRegister::new);
                reg.update(Some(value.clone()), lamport, peer);
            }
            MapOp::Delete { key } => {
                let reg = self.registers.entry(key.clone())
                    .or_insert_with(LWWRegister::new);
                reg.update(None, lamport, peer);
            }
        }
    }

    pub fn get(&self, key: &str) -> Option<&CoralValue> {
        self.registers.get(key)?.value.as_ref()
    }

    pub fn get_value(&self) -> CoralValue {
        let map: FxHashMap<String, CoralValue> = self.registers.iter()
            .filter_map(|(k, r)| r.value.as_ref().map(|v| (k.clone(), v.clone())))
            .collect();
        CoralValue::Map(map.into())
    }

    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.registers.keys()
    }

    pub fn merge(&mut self, other: &Self) {
        for (key, other_reg) in &other.registers {
            let reg = self.registers.entry(key.clone())
                .or_insert_with(LWWRegister::new);
            reg.merge(other_reg);
        }
    }
}
```

**测试要求**：
- [ ] 插入/读取/删除基本功能
- [ ] 并发写入同一 key：LWW 胜出
- [ ] 并发删除与写入：LWW 决定最终可见性
- [ ] 合并交换律/结合律
- [ ] Tombstone 保留：删除后并发插入同一 key，LWW 正确

---

### 2.7 ListOp

- [ ] `ListOp` 完整定义（替换占位符）

```rust
// src/op/list_op.rs

/// 内部操作（基于 ID 引用，保证并发正确性）
#[derive(Debug, Clone, PartialEq)]
pub enum ListOp {
    Insert {
        id: ID,            // 新元素的唯一 ID
        after: ID,         // 插入到哪个 ID 之后（left_origin）
        value: CoralValue,
    },
    Delete {
        id: ID,            // 要删除的元素 ID
        // 注意：Loro 的 Delete 不含 tombstone 信息，
        // tombstone 由 ListState 内部管理（elem.deleted = true）
    },
}
```

> **为什么用 ID 而非 pos**：并发编辑时 pos 会漂移。ID 引用保证位置语义的稳定性。

---

### 2.8 ListState（RGA）

- [ ] `ListElement` 结构体
- [ ] `ListState` 结构体
- [ ] `apply_op()`, `len()`, `get()`, `get_value()`, `merge()`
- [ ] RGA 插入排序算法
- [ ] 单元测试

```rust
// src/state/list_state.rs（新文件）

#[derive(Debug, Clone)]
pub struct ListElement {
    pub id: ID,
    pub value: CoralValue,
    pub left_origin: ID,    // 插入时的左邻居
    pub lamport: Lamport,   // 创建时的 lamport，用于并发排序
    pub deleted: bool,      // tombstone
}

#[derive(Debug, Clone, Default)]
pub struct ListState {
    /// 所有元素（包括已删除的），按 ID 索引
    elements: FxHashMap<ID, ListElement>,
    /// 当前可见元素的文档顺序
    visible_order: Vec<ID>,
}

impl ListState {
    pub fn new() -> Self { Self::default() }

    pub fn apply_op(&mut self, op: &ListOp, lamport: Lamport) {
        match op {
            ListOp::Insert { id, after, value } => {
                if self.elements.contains_key(id) { return; } // 幂等
                let elem = ListElement {
                    id: *id, value: value.clone(),
                    left_origin: *after, lamport, deleted: false,
                };
                self.elements.insert(*id, elem);
                self.insert_to_visible_order(*id, *after, lamport);
            }
            ListOp::Delete { id } => {
                if let Some(elem) = self.elements.get_mut(id) {
                    elem.deleted = true;
                    self.visible_order.retain(|&x| x != *id);
                }
            }
        }
    }

    /// RGA 插入算法：找到 left_origin 之后正确的插入位置
    fn insert_to_visible_order(&mut self, new_id: ID, left_origin: ID, lamport: Lamport) {
        // 找到 left_origin 在 visible_order 中的位置
        let left_pos = if left_origin.is_root() {
            None
        } else {
            self.visible_order.iter().position(|&id| id == left_origin)
        };

        let insert_pos = match left_pos {
            None => 0,
            Some(pos) => {
                let mut target_pos = pos + 1;
                for (i, &id) in self.visible_order.iter().enumerate().skip(pos + 1) {
                    let elem = self.elements.get(&id).unwrap();
                    // 离开并发组就停止
                    if elem.left_origin != left_origin { break; }
                    // 并发冲突：按 (lamport, peer) 排序
                    if (lamport, new_id.peer) < (elem.lamport, id.peer) {
                        target_pos = i;
                        break;
                    }
                    target_pos = i + 1;
                }
                target_pos
            }
        };
        self.visible_order.insert(insert_pos, new_id);
    }

    pub fn len(&self) -> usize { self.visible_order.len() }
    pub fn get(&self, pos: usize) -> Option<&CoralValue> { /* ... */ }
    pub fn get_value(&self) -> CoralValue { /* ... */ }

    /// 合并：收集所有元素后重建 visible_order
    pub fn merge(&mut self, other: &Self) { /* ... */ }
}
```

**测试要求**：
- [ ] 顺序插入/删除
- [ ] 并发插入到同一位置：按 (lamport, peer) 排序
- [ ] 并发删除与插入：tombstone 不影响其他元素
- [ ] 合并交换律/结合律

---

### 2.9 MovableListOp

- [ ] 扩展 `ListOp` 添加 `Move` 变体

```rust
pub enum ListOp {
    Insert { id: ID, after: ID, value: CoralValue },
    Delete { id: ID },
    Move { id: ID, after: ID },  // 把 id 移到 after 之后
}
```

---

### 2.10 MovableListState

- [ ] `MovableListState` 结构体（独立于 ListState）
- [ ] Move 的 LWW 逻辑
- [ ] `rebuild_visible_order()`
- [ ] 单元测试

```rust
// src/state/movable_list_state.rs（新文件）

#[derive(Debug, Clone)]
pub struct MovableListElement {
    pub id: ID,
    pub value: CoralValue,
    pub left_origin: ID,
    pub lamport: Lamport,
    pub deleted: bool,
    // Move 相关字段：
    pub after: ID,              // 当前被指定的后继（最后一次 winning move）
    pub move_lamport: Lamport,  // 最后一次 move 的 lamport
    pub move_peer: PeerID,      // tie-break
}
```

> **注意**：MovableListState 有自己的状态实现，不能复用 ListState。Move 操作的 LWW 逻辑与普通 List 的 Insert/Delete 完全不同。这是 Loro 的设计。

**测试要求**：
- [ ] Move 后元素位置正确
- [ ] 并发 move 同一元素：LWW 决定最终位置
- [ ] Move 后删除不再可见
- [ ] 合并交换律/结合律

---

### 2.11 TextOp

- [ ] `TextOp` 完整定义（替换占位符）

```rust
// src/op/text_op.rs

/// 内部存储用 TextOp（基于 ID 引用）
pub enum TextOp {
    Insert { id: ID, after: ID, text: String },  // 每个字符或连续字符串
    Delete { id: ID },
}
```

> **NOTE**：这是简化版 TextOp。Phase 6 Fugue 升级时会替换内部实现，但 Handler API（insert/delete/toString）不变。

---

### 2.12 TextState（List-based 简化版）

- [ ] `TextState` 结构体（包装 ListState）
- [ ] `insert()`, `delete()`, `to_string()`, `len()`
- [ ] 单元测试

```rust
// src/state/text_state.rs（新文件）

pub struct TextState {
    list: ListState,
}

impl TextState {
    pub fn new() -> Self { Self { list: ListState::new() } }
    pub fn to_string(&self) -> String { /* ... */ }
    pub fn len(&self) -> usize { self.list.len() }
}
```

**测试要求**：
- [ ] insert + delete + to_string 正确
- [ ] Unicode（中文、emoji）处理正确
- [ ] 并发编辑 merge 后字符串一致

---

### 2.13 TreeOp

- [ ] `TreeOp` 完整定义
- [ ] `TreeParentID` 枚举

```rust
// src/op/tree_op.rs

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeParentID {
    Root,
    Node(ID),
    Deleted,
}

pub enum TreeOp {
    Create { target: ID, parent: TreeParentID },
    Move { target: ID, parent: TreeParentID },
    Delete { target: ID },
}
```

---

### 2.14 TreeState

- [ ] `TreeNode` 结构体
- [ ] `TreeState` 结构体
- [ ] `apply_create()`, `apply_move()`, `apply_delete()`
- [ ] 循环检测
- [ ] `children()`, `parent()`
- [ ] 单元测试

```rust
// src/state/tree_state.rs（新文件）

#[derive(Debug, Clone)]
pub struct TreeNode {
    pub id: ID,
    pub parent: TreeParentID,
    pub move_lamport: Lamport,
    pub move_peer: PeerID,
    pub deleted: bool,
}

#[derive(Debug, Clone, Default)]
pub struct TreeState {
    nodes: FxHashMap<ID, TreeNode>,
}
```

**测试要求**：
- [ ] Create / Move / Delete 基本功能
- [ ] 循环检测：祖先不能移到后代下
- [ ] 并发 move：LWW 决定最终 parent
- [ ] 合并交换律

> **NOTE**：metadata Map（每个 Tree 节点绑定一个 Map 容器）是 Phase 4 文档运行时的概念。Phase 2 的 TreeState 不涉及。

---

### Phase 2 验收标准

- [ ] `cargo test` 全部通过
- [ ] `cargo clippy -- -D warnings` 通过
- [ ] 每个 CRDT 独立可测试，无外部依赖
- [ ] CounterState：f64 增量正确
- [ ] LWWRegister：LWW 语义正确
- [ ] MapState：并发 key 冲突 LWW 胜出
- [ ] ListState：RGA 并发插入顺序正确
- [ ] MovableListState：Move LWW 正确
- [ ] TextState：to_string 正确
- [ ] TreeState：循环检测 + LWW Move 正确

---

## Phase 3: ContainerState trait 与 Diff

> 为所有独立 CRDT 状态定义统一接口，使它们能被 DocState 统一管理。
> 对齐 Loro 的 `ContainerState` trait（定义在 `loro-internal/src/state.rs`）。

---

### 3.1 ContainerState trait

- [ ] trait 定义
- [ ] `ApplyLocalOpReturn` 结构体
- [ ] `DiffApplyContext` 结构体

```rust
// src/container_state.rs（新文件）

/// 所有 CRDT 容器的统一接口
pub trait ContainerState: Debug + Send + Sync {
    fn container_idx(&self) -> ContainerIdx;
    fn is_state_empty(&self) -> bool;

    /// 应用一个本地 Op（增量更新）
    fn apply_local_op(
        &mut self,
        raw_op: &RawOp,
        op: &Op,
    ) -> Result<ApplyLocalOpReturn, CoralError>;

    /// 应用一个 Diff 并返回产生的 Diff（用于事件通知）
    fn apply_diff_and_convert(
        &mut self,
        diff: InternalDiff,
        ctx: DiffApplyContext,
    ) -> Diff;

    /// 应用一个 Diff（批量重建或同步）
    fn apply_diff(&mut self, diff: InternalDiff, ctx: DiffApplyContext);

    /// 导出当前状态的 Diff
    fn to_diff(&mut self) -> Diff;

    /// 获取当前值
    fn get_value(&mut self) -> CoralValue;

    /// 获取子容器索引（Tree / Map 嵌套用）
    fn get_child_index(&self, id: &ContainerID) -> Option<ContainerIdx>;
    fn get_child_containers(&self) -> Vec<ContainerID>;
    fn contains_child(&self, id: &ContainerID) -> bool;

    /// 克隆自身（fork / 快照用）
    fn fork(&self) -> Self;

    /// 获取容器类型
    fn container_type(&self) -> ContainerType;
}
```

> **对齐 Loro**：Loro 的 `ContainerState` trait 有相同的方法签名。`apply_local_op` 接收 `&RawOp` + `&Op`，其中 `raw_op` 提供完整因果上下文。

---

### 3.2 InternalDiff vs Diff（双层 Diff）

- [ ] `InternalDiff` 枚举（内部传输用）
- [ ] `Diff` 枚举（用户/事件用）

```rust
// src/diff.rs（新文件）

/// 内部 Diff 类型，用于 Change 应用时的批量操作
#[derive(Debug, Clone)]
#[non_exhaustive]
pub(crate) enum InternalDiff {
    Counter(f64),
    Map(MapDiff),
    List(ListDiff),
    MovableList(MovableListDiff),
    Text(TextDiff),
    Tree(TreeDiff),
    Unknown,
}

/// 用户/事件面向的 Diff 类型
#[derive(Debug, Clone)]
pub enum Diff {
    Counter(f64),       // Loro: Diff::Counter(f64)，即当前值
    Map(MapDiff),
    List(ListDiff),
    Text(TextDiff),
    Tree(TreeDiff),
    Unknown,
}
```

> **为什么双层**：Loro 区分 `InternalDiff`（内部批量操作）和 `Diff`（用户可见的事件）。`apply_diff_and_convert` 接收 InternalDiff、返回 Diff，用于事件系统。

---

### 3.3 具体 Diff 类型

- [ ] `CounterDiff`
- [ ] `MapDiff`
- [ ] `ListDiff`
- [ ] `MovableListDiff`
- [ ] `TextDiff`
- [ ] `TreeDiff`

```rust
#[derive(Debug, Clone)]
pub struct MapDiff {
    pub updated: FxHashMap<String, Option<CoralValue>>,  // Some=insert, None=delete
}

#[derive(Debug, Clone)]
pub struct ListDiff {
    // 初期用简化的 pos-based 表示，后续可升级为 ID-based
    pub inserts: Vec<(usize, Vec<CoralValue>)>,
    pub deletes: Vec<(usize, usize)>,
}

#[derive(Debug, Clone)]
pub struct TextDiff {
    pub inserts: Vec<(usize, String)>,
    pub deletes: Vec<(usize, usize)>,
}

#[derive(Debug, Clone)]
pub struct TreeDiff {
    pub nodes: Vec<TreeNodeDiff>,
}

pub struct TreeNodeDiff {
    pub id: ID,
    pub parent: TreeParentID,
    pub deleted: bool,
}
```

---

### 3.4 各 State 的 ContainerState impl

- [ ] `impl ContainerState for CounterState`
- [ ] `impl ContainerState for MapState`
- [ ] `impl ContainerState for ListState`
- [ ] `impl ContainerState for MovableListState`
- [ ] `impl ContainerState for TextState`
- [ ] `impl ContainerState for TreeState`
- [ ] `impl ContainerState for UnknownState`

每个 impl 必须实现：
- `apply_local_op`：从 RawOp/Op 提取对应类型的操作并应用
- `apply_diff_and_convert`：接收 InternalDiff，应用并返回 Diff
- `apply_diff`：接收 InternalDiff，静默应用
- `to_diff`：将当前状态导出为 Diff
- `get_value`：返回当前值
- `fork`：深克隆

---

### 3.5 容器状态工厂

- [ ] `create_container_state()` 函数

```rust
pub fn create_container_state(kind: ContainerType) -> Box<dyn ContainerState> {
    match kind {
        ContainerType::Counter => Box::new(CounterState::new()),
        ContainerType::Map => Box::new(MapState::new()),
        ContainerType::List => Box::new(ListState::new()),
        ContainerType::MovableList => Box::new(MovableListState::new()),  // 独立状态！
        ContainerType::Text => Box::new(TextState::new()),
        ContainerType::Tree => Box::new(TreeState::new()),
        ContainerType::Unknown(_) => Box::new(UnknownState::new()),
    }
}
```

> **修正**：旧计划中 MovableList 复用 ListState 是错误的。MovableListState 有独立的 Move LWW 逻辑。

---

### 3.6 Diff 往返测试

- [ ] State A → `to_diff()` → 新建 State B → `apply_diff()` → `A.get_value() == B.get_value()`

每个 CRDT 都必须验证。

---

### Phase 3 验收标准

- [ ] `cargo test` 全部通过
- [ ] 所有 Phase 2 的 State 都实现了 ContainerState trait
- [ ] 工厂函数能根据 ContainerType 创建正确的 State
- [ ] 每个 CRDT 的 Diff 往返测试通过
- [ ] `UnknownState` 存根可用（前向兼容）

---

## Phase 4: 文档运行时

> 在独立 CRDT 之上搭建文档级基础设施。
> **核心组件**：共享 Arena、AppDag（BTreeMap）、OpLog + ChangeStore、DocState、Transaction、Handler。

---

### 4.1 共享 Arena

- [ ] 确保 OpLog 和 DocState 引用同一个 Arena 实例

```rust
// 方案 A（推荐）：CoralDoc 持有 Arena，传引用给 OpLog/DocState
pub struct CoralDoc {
    arena: Arena,           // 唯一持有者
    oplog: OpLog,           // &arena
    state: DocState,        // &arena
    // ...
}

// 方案 B：Rc<RefCell<Arena>> 共享
```

> **对齐 Loro**：Loro 使用 `Arc<SharedArena>` + `Mutex` 共享。Coral 单线程，只需确保同一个实例。

---

### 4.2 AppDag（有向无环图 / 因果图）

- [ ] `AppDag` 结构体
- [ ] `AppDagNode` / `AppDagNodeInner` 结构体
- [ ] `Dag` trait
- [ ] `DagUtils` trait（find_common_ancestor, iter_causal 等）
- [ ] `frontiers_to_vv()` / `vv_to_frontiers()`
- [ ] `shrink_frontiers()`

```rust
// src/dag.rs（新文件）

/// DAG 的核心 trait
pub(crate) trait Dag {
    type Node;
    fn get(&self, id: ID) -> Option<Self::Node>;
    fn frontier(&self) -> &Frontiers;
    fn vv(&self) -> &VersionVector;
    fn contains(&self, id: ID) -> bool;
}

/// 具体实现
pub struct AppDag {
    map: BTreeMap<ID, AppDagNode>,   // 注意：BTreeMap，不是 Vec！
    frontiers: Frontiers,
    vv: VersionVector,
    change_store: ChangeStore,       // Change 存储后端
    // ...
}

pub struct AppDagNode {
    inner: Arc<AppDagNodeInner>,
}

struct AppDagNodeInner {
    pub peer: PeerID,
    pub cnt: Counter,
    pub lamport: Lamport,
    pub deps: Frontiers,
    pub vv: OnceCell<ImVersionVector>,
    pub has_succ: bool,
    pub len: usize,
}
```

> **对齐 Loro**：Loro 使用 `BTreeMap<ID, AppDagNode>` 存储 DAG 节点，提供 O(log n) 的 ID 查找。Vec 不适合此场景。

---

### 4.3 OpLog（操作日志）

- [ ] `OpLog` 结构体
- [ ] `import_change()` / `export_changes()`
- [ ] `vv()` / `frontiers()`
- [ ] `iter_changes()` / `get_changes_between()`

```rust
// src/oplog.rs（新文件）

pub struct OpLog {
    dag: AppDag,
    arena: Arena,                        // 共享引用
    changes: Vec<Change>,                // 简化版：后续升级为 ChangeStore
    pending: PendingChanges,             // 依赖未满足的 Change
    history_cache: ContainerHistoryCache,
}
```

---

### 4.4 PendingChanges

- [ ] `PendingChanges` 结构体
- [ ] `try_apply_pending()`

```rust
// src/pending_changes.rs（新文件）

pub struct PendingChanges {
    // 按 peer + counter 索引，方便查找依赖
    changes: FxHashMap<PeerID, BTreeMap<Counter, Vec<Change>>>,
}
```

> **对齐 Loro**：Loro 的 PendingChanges 使用 `FxHashMap<PeerID, BTreeMap<Counter, Vec<PendingChange>>>`。Coral 初期可简化为 `Vec<Change>`，但索引结构性能更好。

---

### 4.5 DocState（所有容器状态的集合）

- [ ] `DocState` 结构体
- [ ] `apply_op()` / `apply_diff()`
- [ ] `get_or_create()` 懒创建容器状态
- [ ] `get_value()`

```rust
// src/doc_state.rs（新文件）

pub struct DocState {
    states: FxHashMap<ContainerIdx, Box<dyn ContainerState>>,
    arena: Arena,  // 共享引用
}

impl DocState {
    /// 懒创建：首次访问时才创建容器状态
    pub fn get_or_create(&mut self, idx: ContainerIdx, kind: ContainerType)
        -> &mut dyn ContainerState
    { /* ... */ }
}
```

> **对齐 Loro**：Loro 的 `DocState` 使用 `ContainerStore`，其中的 `get_or_create_mut` / `get_or_create_imm` 实现懒创建。

---

### 4.6 Transaction

- [ ] `Transaction` 结构体
- [ ] `commit()` / `abort()`
- [ ] `Drop` 自动 commit
- [ ] `next_id()` / `id_span()`
- [ ] `on_commit` 回调

```rust
// src/txn.rs（新文件）

pub struct Transaction<'a> {
    doc: &'a mut CoralDocInner,
    local_ops: RleVec<[Op; 1]>,    // 追踪本次事务的所有操作
    on_commit: Option<OnCommitFn>,  // commit 回调
    origin: String,                 // 事务来源标记
}

impl<'a> Transaction<'a> {
    pub fn commit(self) -> Result<(), CoralError> { /* ... */ }
    pub fn abort(self) { /* 丢弃 local_ops */ }
    pub fn next_id(&self) -> ID { /* ... */ }
}

impl<'a> Drop for Transaction<'a> {
    fn drop(&mut self) {
        // 自动 commit，防止丢失未提交的操作
        if !self.local_ops.is_empty() {
            let _ = self.commit_internal();
        }
    }
}
```

> **对齐 Loro**：Loro 的 Transaction 有 `local_ops: RleVec`、`on_commit` 回调、`Drop` 自动 commit。Coral 的旧计划只包装了 `&mut CoralDoc`，无法独立追踪操作。

---

### 4.7 CoralDoc（顶层入口）

- [ ] `CoralDoc` 结构体
- [ ] `new()` / `with_peer_id()`
- [ ] `get_value()` / `frontiers()` / `vv()`
- [ ] `import()` / `export()`
- [ ] 事务控制：`start_txn()` / `commit_txn()` / `abort_txn()`

```rust
// src/doc.rs（新文件）

pub struct CoralDoc {
    inner: CoralDocInner,
}

struct CoralDocInner {
    arena: Arena,
    oplog: OpLog,
    state: DocState,
    peer_id: PeerID,
    counter: Counter,           // 本地 ID 分配器
    auto_commit: bool,
    txn: Option<Transaction<'static>>,  // 活跃事务
}
```

---

### 4.8 Handler（用户 API 层）

- [ ] `CounterHandler`
- [ ] `MapHandler`
- [ ] `ListHandler`
- [ ] `TextHandler`
- [ ] `TreeHandler`

```rust
// Handler 使用 with_txn 闭包模式（对齐 Loro）

pub struct CounterHandler<'a> {
    doc: &'a mut CoralDocInner,
    container: ContainerIdx,
}

impl CounterHandler<'_> {
    pub fn increment(&mut self, delta: f64) {
        self.doc.with_txn(|txn| {
            let id = txn.next_id();
            let op = Op::new(id.counter, self.container, OpContent::Counter(CounterOp { delta }));
            txn.local_ops.push(op);
            Ok(())
        }).unwrap();
    }
    pub fn get_value(&self) -> f64 { /* ... */ }
}

pub struct MapHandler<'a> { /* 类似结构 */ }
pub struct ListHandler<'a> { /* 类似结构 */ }
pub struct TextHandler<'a> { /* 类似结构 */ }

pub struct TreeHandler<'a> {
    doc: &'a mut CoralDocInner,
    container: ContainerIdx,
}

impl TreeHandler<'_> {
    pub fn create(&mut self, parent: TreeParentID) -> ID { /* ... */ }
    pub fn mov(&mut self, target: ID, new_parent: TreeParentID) { /* ... */ }
    pub fn delete(&mut self, target: ID) { /* ... */ }
    pub fn children(&self, parent: TreeParentID) -> Vec<ID> { /* ... */ }
    pub fn get_meta(&mut self, target: ID) -> MapHandler<'_> { /* ... */ }
}
```

> **对齐 Loro**：Loro 的 Handler 持有 `LoroDoc`（Arc 引用），通过 `with_txn` 闭包获取 `&mut Transaction`。Coral 单线程简化为 `&mut CoralDocInner`。

---

### Phase 4 验收标准

- [ ] `CoralDoc::new()` 创建成功
- [ ] Counter E2E：handler increment → commit → get_value 正确
- [ ] Map E2E：insert/delete/get 通过 handler 工作
- [ ] List E2E：insert/delete/len 通过 handler 工作
- [ ] 嵌套容器：Map 中创建子 List，整体 get_value 结构正确
- [ ] Transaction commit：多 Op 事务提交后全部生效
- [ ] Transaction abort：abort 后 Op 未生效
- [ ] OpLog 导入导出：VV 和 Frontiers 正确
- [ ] OpLog pending：乱序 Change 正确缓存和应用

---

## Phase 5: 协作、事件与版本控制

---

### 5.1 Merge & Sync

- [ ] `CoralDoc::export()`
- [ ] `CoralDoc::import()`
- [ ] `CoralDoc::merge()`
- [ ] Merge 交换律/幂等性测试

```rust
impl CoralDoc {
    pub fn export(&self, from: &VersionVector) -> Vec<Change>;
    pub fn import(&mut self, changes: &[Change]) -> Result<()>;
    pub fn merge(&mut self, other: &CoralDoc) -> Result<()>;
}
```

**核心测试**：
```rust
#[test]
fn test_merge_commutative() {
    let mut a = CoralDoc::new();
    let mut b = CoralDoc::new();
    // 各自编辑...
    let changes_a = a.export(b.vv());
    let changes_b = b.export(a.vv());
    a.import(&changes_b).unwrap();
    b.import(&changes_a).unwrap();
    assert_eq!(a.get_value(), b.get_value());
    assert_eq!(a.frontiers(), b.frontiers());
}
```

---

### 5.2 Event / Subscription 系统

- [ ] `DocEvent` 结构体
- [ ] `PathSegment` 枚举
- [ ] `subscribe()` / `unsubscribe()`
- [ ] `compute_events()` 事件计算

```rust
pub struct DocEvent {
    pub path: Vec<PathSegment>,
    pub diff: Diff,
}

pub enum PathSegment {
    Key(String),
    Index(usize),
}

impl CoralDoc {
    pub fn subscribe(&mut self, callback: Box<dyn Fn(&[DocEvent])>) -> SubscriptionId;
    pub fn unsubscribe(&mut self, id: SubscriptionId);
}
```

---

### 5.3 Checkout & Time Travel

- [ ] `checkout()` 方法（初期：全量重建）

```rust
impl CoralDoc {
    pub fn checkout(&mut self, target: &Frontiers) -> Result<()> {
        // 初期实现：丢弃当前 state，从空重建到 target
        self.state = DocState::new(self.arena.clone());
        let changes = self.oplog.get_changes_between(&Frontiers::new(), target);
        for change in changes {
            for op in &change.ops {
                self.state.apply_op(op)?;
            }
        }
        Ok(())
    }
}
```

---

### 5.4 Fork

- [ ] `fork()` 方法

```rust
impl CoralDoc {
    pub fn fork(&self) -> CoralDoc {
        CoralDoc {
            inner: CoralDocInner {
                arena: self.inner.arena.clone(),
                oplog: self.inner.oplog.clone(),
                state: self.inner.state.fork(),
                peer_id: random_peer_id(),
                // subscribers 不复制
                ..
            },
        }
    }
}
```

---

### Phase 5 验收标准

- [ ] Merge 交换律：A import B == B import A
- [ ] Merge 结合律
- [ ] Import 幂等性
- [ ] 事件在 commit/import 后正确触发
- [ ] Checkout 到历史版本后 get_value 正确
- [ ] Fork 后独立编辑互不影响
- [ ] Fork 后 merge 回原档正确

---

## Phase 6: 高级与优化

---

### 6.1 Text 升级 Fugue

- [ ] `FugueSpan` 结构体
- [ ] Fugue 并发排序算法
- [ ] 替换 TextState 内部实现
- [ ] Handler API 不变验证

```rust
pub struct FugueSpan {
    pub id: ID,
    pub text: String,
    pub deleted: bool,
    pub left_origin: Option<ID>,
    pub right_origin: Option<ID>,
}
```

> Phase 2-5 的所有 Text 测试必须在 Fugue 替换后仍然通过。

---

### 6.2 RichText（样式）

- [ ] `Mark` / `Unmark` 操作
- [ ] `RangeMap<ID, StyleValue>` 样式存储
- [ ] LWW 样式冲突解决
- [ ] `to_delta()` Quill Delta 输出

---

### 6.3 GC & Tombstone 清理

- [ ] `ResetRemove` trait
- [ ] `gc()` 方法

```rust
pub trait ResetRemove {
    fn reset_remove(&mut self, horizon: &VersionVector);
}

impl CoralDoc {
    pub fn gc(&mut self, horizon: &VersionVector) { /* ... */ }
}
```

> GC 后无法 checkout 到 GC 点之前的版本。

---

### 6.4 编码与传输

- [ ] `Encode` trait
- [ ] `impl Encode for Change`
- [ ] `impl Encode for Snapshot`

```rust
pub trait Encode {
    fn encode(&self) -> Vec<u8>;
    fn decode(data: &[u8]) -> Result<Self> where Self: Sized;
}
```

---

### 6.5 性能优化

| 优化项 | 当前 | 目标 |
|--------|------|------|
| List 索引 | Vec<ID> O(n) | Rope/BTree O(log n) |
| Text 存储 | List-based | Fugue + generic-btree |
| MovableList 排序 | 全量重建 | FractionalIndex + BTree |
| Event 计算 | 全量 diff | 增量 diff + 缓存 |
| ContainerState 分发 | `Box<dyn>` | `enum_dispatch` |

---

## Phase 6 验收标准

- [ ] Fugue 替换后所有 Phase 2-5 Text 测试通过
- [ ] RichText delta 输出正确
- [ ] GC 前后 get_value 不变
- [ ] Encode 往返正确
- [ ] `cargo test` 全部通过

---

## 依赖库

| 库 | 用途 | Phase |
|----|------|-------|
| `im` | ImVersionVector 结构共享 | Phase 1（已添加） |
| `rustc-hash` | FxHashMap 高性能哈希 | Phase 1（已添加） |
| `serde` + `serde_json` | JSON 序列化 | Phase 1（已添加） |
| `smallvec` | RleVec 底层存储 | Phase 1（已添加） |
| `num` | 数值 trait | Phase 1（已添加） |
| `thiserror` | 错误类型 | Phase 2（需添加） |
| `indexmap` | Map 保序存储（可选） | Phase 2 |
| `proptest` | 属性测试 | Phase 2+（dev-dep） |

---

## 与 Loro 对齐检查清单

> 以下差异已在本文档中修正。

| 项目 | 旧计划（错误） | 本文档（正确） |
|------|---------------|---------------|
| Counter 值类型 | `i64` | `f64` |
| Frontiers | `Vec<ID>` | 目标：三态枚举 None/ID/Map |
| Change.ops | `Vec<Op>` | `RleVec<[Op; 1]>`（已正确实现） |
| VersionVector | `type` 别名 | `struct` newtype（已正确实现） |
| DAG 存储 | `Vec<Node>` | `BTreeMap<ID, Node>` |
| Arena 共享 | 各自持有 | 同一实例引用 |
| MovableList 工厂 | 复用 ListState | 独立 MovableListState |
| Diff 类型 | 单层 | 双层 InternalDiff / Diff |
| Transaction | 薄包装 | 含 local_ops + on_commit + Drop |
| ID 排序 | counter 优先 | peer 优先（已正确实现） |
| ContainerType::Unknown | 无 | 有（已正确实现） |

---

## 统计

| 指标 | 数量 |
|------|------|
| Phase 1 已完成 | ~40 |
| Phase 1 待完成 | ~2（RawOp, Frontiers 重构） |
| Phase 2-6 待完成 | ~60 |
| 完成百分比 | ~40%（Phase 1 基本完成） |
