# Coral - CRDT Implementation Plan

## 阶段总览

```
Phase 1: 基础类型（纯数据结构，无逻辑）
    ↓
Phase 2: 核心基础设施（OpLog、DAG、Arena、Transaction、ContainerState Trait）
    ↓
Phase 3: Counter（最简单的 CRDT，验证 OpLog → State 全链路）
Phase 4: LWW-Register（单值 CRDT，Map 的基础）
Phase 5: LWW-Map（基于 Register 的键值对）
Phase 6: List（RGA，并发有序集合）
Phase 7: MovableList（在 List 基础上增加 Move）
Phase 8: Text（先做基于 List 的简化版，再替换为 Fugue）
Phase 9: Rich Text（文本 + 样式）
Phase 10: Tree（可移动树 + Metadata Map）
    ↓
Phase 11: Merge & Sync（两文档合并、差量同步）
Phase 12: Checkout & Time Travel（版本回滚、Fork、时间旅行）
```

---

## Phase 1: 基础类型

> 一切依赖的起点，纯数据结构，无逻辑。

- [x] ### 1.1 类型别名

```rust
// src/types.rs

pub type PeerID = u64;
pub type Counter = i32;   // 操作计数器，从 0 单调递增
pub type Lamport = u32;   // Lamport 时间戳
```

- `PeerID`: 用 `u64`，可用随机生成或雪花算法
- `Counter`: 每个 peer 独立递增，与 PeerID 组合唯一标识一个操作
- `Lamport`: 用于 LWW 比较（因果排序）

- [x] ### 1.2 操作 ID

```rust
// src/id.rs

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ID {
    pub peer: PeerID,
    pub counter: Counter,
}

impl ID {
    pub fn new(peer: PeerID, counter: Counter) -> Self;
    pub fn is_root(&self) -> bool;      // peer == 0 && counter == 0
    pub fn inc(&self, delta: Counter) -> ID; // 用于批量操作中递增
}
```

**要点**：
- 实现 `Ord` — 先比 counter，再比 peer，保证全局确定性排序
- 实现 `Hash` — 用于 HashMap/HashSet 查找
- `inc()` — 一个 Change 包含多个 Op，每个 Op 的 ID 递增

- [x] ### 1.3 ContainerType 枚举

```rust
// src/container_id.rs

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ContainerType {
    Map = 0,
    List = 1,
    Text = 2,
    MovableList = 3,
    Tree = 4,
    Counter = 5,
}
```

**要点**：
- `#[repr(u8)]` — 编码/解码时用单字节表示
- 后续编码传输时 `to_u8()` / `from_u8()` 互转

- [x] ### 1.4 ContainerID

```rust
// src/container_id.rs

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ContainerID {
    Root { name: String, container_type: ContainerType },
    Normal { peer: PeerID, counter: Counter, container_type: ContainerType },
}
```

**两种容器的区别**：
- **Root**: 用户显式创建的顶层容器，如 `"text"`、`"my_map"`
- **Normal**: 由 List/Map 等容器内部自动创建的子容器，用创建它的 Op 的 ID 标识

**方法**：
```rust
impl ContainerID {
    pub fn new_root(name: &str, kind: ContainerType) -> Self;
    pub fn new_normal(id: ID, kind: ContainerType) -> Self;
    pub fn container_type(&self) -> ContainerType;
    pub fn to_bytes(&self) -> Vec<u8>;
    pub fn from_bytes(data: &[u8]) -> Result<Self>;
    pub fn to_string(&self) -> String;       // 可读表示，如 "root:text:Map"
    pub fn from_string(s: &str) -> Result<Self>;
}
```

- [x] ### 1.5 CoralValue（JSON 值）

```rust
// src/value.rs

#[derive(Debug, Clone, PartialEq)]
pub enum LoroValue {
    Null,
    Bool(bool),
    I64(i64),
    F64(f64),
    String(String),
    List(Vec<LoroValue>),
    Map(FxHashMap<String, LoroValue>),  // 用 IndexMap 保序
    Container(ContainerID),            // 引用子容器
}
```

**要点**：
- 用 `IndexMap` 而非 `HashMap` — Map 的 key 需要保插入序
- `Container` 变体 — 表示"这个位置是一个子容器"（如 Map 中某个 value 是一个 List）
- `PartialEq` 但不实现 `Eq` — f64 的 NaN 问题
- 提供 `to_json()` / `from_json()` 与 serde_json 互转

- [x] ### 1.6 Op（操作单元）

```rust
// src/op.rs

#[derive(Debug, Clone)]
pub struct Op {
    pub id: ID,           // 这个操作的唯一 ID
    pub container: ContainerID, // 目标容器
    pub content: OpContent,     // 操作内容
    pub lamport: Lamport,       // Lamport 时间戳（LWW 用）
}

#[derive(Debug, Clone)]
pub enum OpContent {
    Map(MapOp),
    List(ListOp),
    Text(TextOp),
    Tree(TreeOp),
    Counter(CounterOp),
}

// 第三阶段再逐个定义具体 Op，先定义占位：
// MapOp, ListOp, TextOp, TreeOp, CounterOp
```

**注意**：在内部实现中，Op 的 `container` 字段后期会优化为 `ContainerIdx`（见 Phase 2.1），但 API 层仍暴露 `ContainerID`。

- [x] ### 1.7 Change（变更组 / 事务）

```rust
// src/change.rs

/// 用户一次提交（Transaction）产生一个 Change，包含多个 Op。
/// 这些 Op 共享同一个起始 lamport、timestamp 和 deps。
pub struct Change {
    pub id: ID,               // 第一个 Op 的 ID（peer + counter）
    pub lamport: Lamport,     // 起始 lamport
    pub timestamp: i64,       // 物理时间戳（毫秒）
    pub deps: Frontiers,      // 直接前驱版本（因果依赖）
    pub ops: Vec<Op>,         // 本次提交的所有操作
}
```

**要点**：
- CRDT 的因果追踪在 **Change 级别**进行，而非单个 Op 级别
- `deps` 说明这个 Change 依赖于哪些前置 Change（DAG 的边）
- 一个 Change 内的多个 Op 的 ID 是连续的：`id.counter`, `id.counter+1`, ...
- 后续 `OpLog` 按 Change 存储历史，而非按 Op

- [x] ### 1.8 VersionVector & Frontiers

```rust
// src/version.rs

/// 版本向量：记录每个 peer 已见到的最大 counter。
/// 用于判断 "A 是否包含 B 的所有变更"。
pub type VersionVector = HashMap<PeerID, Counter>;

/// DAG 的当前叶子节点集合。
/// 当历史是线性时，Frontiers 只有一个 ID；并发编辑时会有多个。
pub struct Frontiers(pub Vec<ID>);

impl VersionVector {
    /// 判断 self 是否包含 other 的所有变更（self >= other）
    pub fn includes(&self, other: &VersionVector) -> bool;
    
    /// 合并另一个 VersionVector（取每个 peer 的最大值）
    pub fn merge(&mut self, other: &VersionVector);
    
    /// 从 Frontiers + DAG 计算完整的 VersionVector
    pub fn from_frontiers(dag: &Dag<ID>, frontiers: &Frontiers) -> Self;
}
```

**要点**：
- `VersionVector` 是**集合包含关系**的紧凑表示：若 vv_a >= vv_b，则 A 包含了 B 的所有变更
- `Frontiers` 是**版本标识**：两个文档若 Frontiers 相同，则状态一定相同（假设确定性 apply）
- Diff（计算状态差异）、Merge、Checkout 都依赖这两者

- [x] ### 1.9 Span（区间类型）

```rust
// src/span.rs

/// 一个 peer 的连续 counter 区间 [start, end)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CounterSpan {
    pub start: Counter,
    pub end: Counter,
}

/// 全局唯一的 ID 区间
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdSpan {
    pub peer: PeerID,
    pub start: Counter,
    pub end: Counter,
}
```

**要点**：
- 用于批量操作、区间查询、编码压缩（连续的 Op 可以合并传输）
- `IdSpan` 是 `Change` 中多个 `Op` 的 ID 范围表示

---

## Phase 2: 核心基础设施

> 连接"操作"与"状态"的桥梁。没有这些，CRDT 只是单机数据结构。

- [x] ### 2.1 Arena & ContainerIdx（内存优化层）

```rust
// src/arena.rs

/// 内部紧凑表示，替代臃肿的 ContainerID。
/// top 4 bits 存 ContainerType，其余存自增索引。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ContainerIdx(u32);

/// 管理 ContainerID ↔ ContainerIdx 的双向映射。
pub struct Arena {
    id_to_idx: HashMap<ContainerID, ContainerIdx>,
    idx_to_id: Vec<ContainerID>,
    // 记录每个容器的父容器（用于构建树形结构、计算路径）
    parent: Vec<Option<ContainerIdx>>,
}

impl Arena {
    pub fn register(&mut self, id: &ContainerID) -> ContainerIdx;
    pub fn get_id(&self, idx: ContainerIdx) -> Option<&ContainerID>;
    pub fn get_idx(&self, id: &ContainerID) -> Option<ContainerIdx>;
    pub fn set_parent(&mut self, child: ContainerIdx, parent: Option<ContainerIdx>);
    pub fn get_parent(&self, child: ContainerIdx) -> Option<ContainerIdx>;
}
```

**要点**：
- `ContainerIdx` 只有 4 字节，`ContainerID` 可能几十字节（含 String），内部全部用 `ContainerIdx`
- 序列化/反序列化、跨进程传输时才用 `ContainerID`
- `parent` 关系用于事件传播时计算容器路径（如 `root.map.list[3]`）
- Root 容器没有父节点；Normal 容器在创建时注册其父容器

- [ ] ### 2.2 DAG（有向无环图 / 因果图）

```rust
// src/dag.rs

/// 管理 Change 之间的因果关系。
/// 每个 Change 是一个节点，deps 指向它的父节点。
pub struct Dag<ID> {
    nodes: Vec<DagNode<ID>>,
    // 可能还需要按 peer 索引，加速查询
}

struct DagNode<ID> {
    pub id: ID,
    pub deps: Vec<ID>,        // 直接前驱
    pub children: Vec<ID>,    // 直接后继（反向索引，加速遍历）
}

impl Dag<ID> {
    /// 按拓扑序遍历所有节点
    pub fn iter(&self) -> impl Iterator<Item = &DagNode<ID>>;
    
    /// 从 Frontiers 回溯到某版本的 VersionVector
    pub fn frontiers_to_vv(&self, frontiers: &Frontiers) -> VersionVector;
    
    /// 查找两个版本的共同祖先（LCA）
    pub fn find_common_ancestor(&self, a: &Frontiers, b: &Frontiers) -> Frontiers;
    
    /// 获取从 from 到 to 需要应用的 Change 列表
    pub fn diff_changes(&self, from: &VersionVector, to: &VersionVector) -> Vec<&Change>;
}
```

**要点**：
- DAG 是 CRDT 的"时间线"，所有历史变更按因果关系组织
- `frontiers_to_vv` 是核心算法：从叶子节点反向遍历累加各 peer 的最大 counter
- LCA（最近公共祖先）用于 Merge 时找到分叉点

- [ ] ### 2.3 OpLog（操作日志）

```rust
// src/oplog.rs

/// 所有历史变更的存储，与 DocState 分离。
/// DocState 可以从 OpLog 的任意版本"重放"构建出来。
pub struct OpLog {
    changes: Vec<Change>,
    dag: Dag<ID>,
    vv: VersionVector,
    frontiers: Frontiers,
}

impl OpLog {
    /// 导入一个远程 Change（来自其他 peer）
    pub fn import_change(&mut self, change: Change) -> Result<()>;
    
    /// 导出本地自某版本以来的所有 Change
    pub fn export_changes(&self, from: &VersionVector) -> Vec<&Change>;
    
    /// 获取当前版本向量
    pub fn vv(&self) -> &VersionVector;
    
    /// 获取当前 Frontiers
    pub fn frontiers(&self) -> &Frontiers;
    
    /// 从 empty 重放到指定版本，返回所需的所有 Change
    pub fn get_changes_to(&self, target: &Frontiers) -> Vec<&Change>;
}
```

**要点**：
- **OpLog 与 DocState 分离**是 Loro 最核心的架构决策
- 支持 `checkout`：DocState 可以回滚到旧版本，再前进到新版本
- 支持 `fork`：从当前状态克隆一份独立的 OpLog + State
- `import_change` 需要检查：Change 的 deps 是否都已满足（否则暂存到 pending queue）

- [ ] ### 2.4 CoralDoc 顶层结构

```rust
// src/doc.rs

/// 用户直接操作的文档对象。
/// 由 OpLog（历史）和 DocState（当前状态）组成，共享同一个 Arena。
pub struct CoralDoc {
    oplog: OpLog,
    state: DocState,
    arena: Arena,
    peer_id: PeerID,
    // 本地编辑的批量缓冲（Transaction 未提交前暂存于此）
    pending_ops: Vec<Op>,
}

impl CoralDoc {
    pub fn new() -> Self;
    pub fn with_peer_id(peer_id: PeerID) -> Self;
    
    /// 获取当前文档值的快照
    pub fn get_value(&self) -> LoroValue;
    
    /// 获取当前版本 Frontiers
    pub fn frontiers(&self) -> &Frontiers;
    
    /// 获取当前版本向量
    pub fn vv(&self) -> &VersionVector;
    
    /// 导入远程变更（Merge 入口）
    pub fn import(&mut self, changes: &[Change]) -> Result<()>;
    
    /// 导出自某版本以来的本地变更
    pub fn export(&self, from: &VersionVector) -> Vec<Change>;
    
    /// 开始一个本地事务
    pub fn start_txn(&mut self);
    
    /// 提交本地事务，生成一个 Change 写入 OpLog，并应用到 State
    pub fn commit_txn(&mut self) -> Result<()>;
    
    /// 丢弃当前事务
    pub fn abort_txn(&mut self);
    
    /// 内部方法：生成下一个 Op 的 ID（递增本地 counter）
    pub(crate) fn next_id(&mut self) -> ID;
    
    /// 内部方法：提交单个 Op（由 Handler 调用）
    pub(crate) fn submit_op(&mut self, container: ContainerIdx, content: OpContent);
}
```

**要点**：
- `peer_id` 每个文档独立，创建时随机生成
- `pending_ops` 在事务期间累积，commit 时批量生成 Change
- Handler（CounterHandler / MapHandler 等）持有 `&mut CoralDoc`，但操作都委托给 Doc
- 自动提交模式：用户也可以不设显式事务，每次操作自动 commit

- [ ] ### 2.5 Transaction（本地事务提交）

```rust
// src/txn.rs

/// 事务是用户一次编辑的边界。
/// 事务内的多个 Op 原子地成为一个 Change。
pub struct Transaction<'a> {
    doc: &'a mut CoralDoc,
    ops: Vec<Op>,
}

impl<'a> Transaction<'a> {
    pub fn new(doc: &'a mut CoralDoc) -> Self;
    
    /// 向事务中添加一个 Op（由 Handler 调用）
    pub(crate) fn push_op(&mut self, container: ContainerIdx, content: OpContent);
    
    /// 提交事务：
    /// 1. 计算 lamport = max(本地已知 lamport) + 1
    /// 2. 构造 Change { id, lamport, timestamp, deps, ops }
    /// 3. deps = 当前 frontiers
    /// 4. 写入 OpLog
    /// 5. 逐个 apply 到 DocState
    pub fn commit(self) -> Result<()>;
    
    pub fn abort(self);
}
```

**要点**：
- Lamport 的维护：每次 commit 时，`lamport = max(所有已知 Change 的 lamport) + 1`
- deps 设置为 commit 时的当前 frontiers（即这个 Change 知道的所有历史）
- Counter 也需要被事务包裹：用户调用 `counter.increment(1)` 时，实际上是生成 Op 并提交

- [ ] ### 2.6 DocState（所有容器状态的集合）

```rust
// src/state.rs

/// 当前文档的所有容器状态的集合。
/// 与 OpLog 分离，可以从 OpLog 重放构建。
pub struct DocState {
    states: HashMap<ContainerIdx, Box<dyn ContainerState>>,
    arena: Arena,
}

impl DocState {
    /// 应用一个 Op（增量更新，由本地 commit 或远程 import 触发）
    pub fn apply_op(&mut self, op: &Op) -> Result<()>;
    
    /// 应用一个 Diff（用于 checkout / import snapshot 时批量重建）
    pub fn apply_diff(&mut self, diff: &DocDiff) -> Result<()>;
    
    /// 获取某个容器的状态（如果不存在则创建 default）
    pub fn get_or_create(&mut self, idx: ContainerIdx, kind: ContainerType) -> &mut dyn ContainerState;
    
    /// 获取整个文档的值
    pub fn get_value(&self) -> LoroValue;
}
```

**要点**：
- `states` 用懒加载：只有被操作过的容器才会创建对应的 State 对象
- 每个容器状态独立实现 `ContainerState` trait
- DocState 不直接参与因果/版本逻辑，只负责"给定一个 Op/Diff，更新本地状态"

- [ ] ### 2.7 ContainerState Trait & Diff 类型

```rust
// src/container_state.rs

/// 所有 CRDT 容器的统一接口。
pub trait ContainerState: Debug + Send + Sync {
    /// 该容器的 ContainerIdx
    fn container_idx(&self) -> ContainerIdx;
    
    /// 应用一个本地生成的 Op（增量更新）
    fn apply_local_op(&mut self, op: &Op) -> Result<()>;
    
    /// 应用一个 Diff（用于 checkout / import / 远程同步时批量重建）
    fn apply_diff(&mut self, diff: &Diff);
    
    /// 从 empty 状态到当前状态的 diff（用于编码传输、事件输出）
    fn to_diff(&self) -> Diff;
    
    /// 获取当前状态的值
    fn get_value(&self) -> LoroValue;
    
    /// 克隆自身（用于 fork / 快照）
    fn fork(&self) -> Box<dyn ContainerState>;
}

// --- Diff 类型 ---

#[derive(Debug, Clone)]
pub enum Diff {
    Map(MapDiff),
    List(ListDiff),
    Text(TextDiff),
    Tree(TreeDiff),
    Counter(CounterDiff),
}

// 具体的 Diff 结构在实现对应容器时再定义。
```

**要点**：
- `apply_local_op` vs `apply_diff`：前者是增量（Op 级别），后者是批量（Diff 级别）
- `to_diff` 用于：
  1. 编码传输（把当前状态编码为 Diff 发送给对方）
  2. 事件输出（用户订阅变更时收到 Diff）
  3. 快照（从空文档 apply_diff 即可还原）
- 为什么需要 Diff：Op 是"编辑意图"（如"在第 3 位插入 a"），Diff 是"状态变化"（如"添加了 [a,b,c]"）。当从旧版本 checkout 到新版本时，你可能需要 Diff 而非重放所有 Op。

---

## Phase 3: Counter (PN-Counter)

> 最简单的 CRDT，用来热身验证 OpLog → Transaction → State 的全链路。

- [ ] ### 3.1 CounterOp

```rust
// src/op/counter_op.rs

#[derive(Debug, Clone)]
pub struct CounterOp {
    pub delta: i64,  // 增量，可以为负
}
```

**注意**：这里命名是 **PN-Counter**（可正负），不是 G-Counter（只能加）。
Loro 的 Counter 是 **op-based**：每个 delta 作为一个 Op，合并时按 peer 聚合所有 delta。

- [ ] ### 3.2 CounterState

```rust
// src/state/counter_state.rs

#[derive(Debug, Clone)]
pub struct CounterState {
    container_id: ContainerIdx,
    value: i64,  // 当前值（聚合后的缓存）
}

impl ContainerState for CounterState {
    fn apply_local_op(&mut self, op: &Op) {
        if let OpContent::Counter(counter_op) = &op.content {
            self.value += counter_op.delta;
        }
    }
    
    fn get_value(&self) -> LoroValue {
        LoroValue::I64(self.value)
    }
    
    // ... apply_diff / to_diff / fork
}
```

**要点**：
- `apply` 就是简单的 `self.value += delta`
- CRDT 的幂等性由上层 OpLog 保证（每个 Op 只 apply 一次）
- Counter 的并发语义是**最终一致性**：A 加 3，B 减 2，合并后是 +1（不是 LWW，是算术合并）

- [ ] ### 3.3 Counter Handler（用户 API）

```rust
// src/handler/counter.rs

pub struct CounterHandler<'a> {
    doc: &'a mut CoralDoc,
    container_id: ContainerIdx,
}

impl<'a> CounterHandler<'a> {
    pub fn increment(&mut self, delta: i64) {
        // 生成 Op，提交给当前 Transaction
        self.doc.submit_op(self.container_id, OpContent::Counter(CounterOp { delta }));
    }
    
    pub fn get_value(&self) -> i64 {
        // 从 DocState 读取当前值
        let state = self.doc.state.get(self.container_id).unwrap();
        state.get_value().as_i64().unwrap()
    }
}
```

**要点**：
- `increment()` 生成一个 Op，通过 `CoralDoc::submit_op` 进入当前 Transaction
- 用户通过 handler 操作，不直接接触 CounterState
- 如果不在显式事务中，`submit_op` 可以自动开启并提交一个单 Op 事务

---

## Phase 4: LWW-Register

> 单值 CRDT，LWW-Map 的基础。

- [ ] ### 4.1 内部结构

```rust
// src/state/lww_register.rs

#[derive(Debug, Clone)]
pub struct LWWRegister<T> {
    pub value: Option<T>,     // None 表示已删除
    pub lamport: Lamport,
    pub peer: PeerID,         // 用于 lamport 相同时的 tie-break
}
```

- [ ] ### 4.2 合并逻辑

```rust
impl<T: Clone> LWWRegister<T> {
    pub fn merge(&mut self, other: &LWWRegister<T>) {
        match self.lamport.cmp(&other.lamport) {
            Ordering::Less    => { *self = other.clone(); }
            Ordering::Greater => {}
            Ordering::Equal   => {
                // tie-break: peerID 大的赢
                if self.peer < other.peer {
                    *self = other.clone();
                }
            }
        }
    }
    
    /// 用新的 Op 更新 Register
    pub fn update(&mut self, value: Option<T>, lamport: Lamport, peer: PeerID) {
        let other = LWWRegister { value, lamport, peer };
        self.merge(&other);
    }
}
```

**要点**：
- Lamport 相同时必须用确定性的 tie-break（peerID），否则不同节点结果不一致
- `None` 代表"已删除"，删除也是一种值，需要参与 LWW 比较
- 为什么用 lamport 而不是 timestamp：物理时钟不可靠，Lamport 逻辑时钟保证因果关系

---

## Phase 5: LWW-Map

> 基于 Register 的键值对，最实用的 CRDT 之一。

- [ ] ### 5.1 MapOp

```rust
// src/op/map_op.rs

#[derive(Debug, Clone)]
pub enum MapOp {
    Insert { key: String, value: LoroValue },
    Delete { key: String },
}
```

- [ ] ### 5.2 MapState

```rust
// src/state/map_state.rs

#[derive(Debug, Clone)]
pub struct MapState {
    container_id: ContainerIdx,
    registers: IndexMap<String, LWWRegister<LoroValue>>,  // 每个 key 一个 LWW-Register
}

impl ContainerState for MapState {
    fn apply_local_op(&mut self, op: &Op) {
        match &op.content {
            OpContent::Map(MapOp::Insert { key, value }) => {
                let reg = self.registers.entry(key.clone())
                    .or_insert_with(|| LWWRegister::new_none());
                reg.update(Some(value.clone()), op.lamport, op.id.peer);
            }
            OpContent::Map(MapOp::Delete { key }) => {
                let reg = self.registers.entry(key.clone())
                    .or_insert_with(|| LWWRegister::new_none());
                reg.update(None, op.lamport, op.id.peer);
            }
            _ => panic!("wrong op type"),
        }
    }

    fn get_value(&self) -> LoroValue {
        // 遍历 registers，过滤掉 value == None 的，构建 Map
        LoroValue::Map(self.registers.iter()
            .filter_map(|(k, r)| r.value.as_ref().map(|v| (k.clone(), v.clone())))
            .collect())
    }
}
```

**要点**：
- `get_value` 只返回 value != None 的键，已删除的 key 对用户不可见
- 但内部 `registers` 保留 tombstone，因为：
  1. 后续并发 Insert 同一 key 需要知道之前的 lamport 以做 LWW 比较
  2. Diff 输出时可能需要表达 "key 被删除" 的事件
- `IndexMap` 保证 key 的遍历顺序是插入顺序

- [ ] ### 5.3 Map Handler

```rust
// src/handler/map.rs

pub struct MapHandler<'a> { ... }

impl<'a> MapHandler<'a> {
    pub fn insert(&mut self, key: &str, value: impl Into<LoroValue>);
    pub fn delete(&mut self, key: &str);
    pub fn get(&self, key: &str) -> Option<LoroValue>;
    pub fn get_container<T: ContainerHandler>(&self, key: &str) -> T;
    pub fn keys(&self) -> Vec<String>;
    pub fn to_json(&self) -> String;
}
```

**要点**：
- `get_container` — Map 的 value 可以是子容器（嵌套），这是组合式 CRDT 的关键
- 当 `insert` 一个 `LoroValue::Container(child_id)` 时，需要在 Arena 中注册父子关系
- 当 `get_container` 时，如果 key 不存在或不是 Container，需要自动创建（lazy init）

---

## Phase 6: List (RGA)

> **难度跳升点** — 并发有序集合。注意：List 的顺序不是 ID 的字典序！

- [ ] ### 6.1 ListOp

```rust
// src/op/list_op.rs

/// 用户侧的操作（基于 pos，友好但不适合并发）
pub enum ListOp {
    Insert { pos: usize, value: LoroValue },
    Delete { pos: usize, len: usize },
}

/// 内部真正存储/传输的操作（基于 ID 引用，保证并发正确性）
pub enum ListOpInternal {
    Insert {
        after: ID,       // 插入到哪个 ID 之后（不是 pos！）
        value: LoroValue,
    },
    Delete {
        target: ID,      // 删除哪个 ID 的元素
        len: usize,      // 批量删除长度
    },
}
```

**为什么内部不能用 pos**：
并发编辑时 pos 会漂移。A 在第 3 位插入，B 同时删除第 1 位，A 的"第 3 位"在 B 的视角已经不是原来的元素了。

**ID 引用策略**：
每个元素有唯一的 `ID`（即创建它的 `Op.id`）。插入时指定"插在哪个已有元素的后面"，这样无论并发怎么编辑，插入的相对位置是稳定的。

- [ ] ### 6.2 ListState（核心数据结构修正）

```rust
// src/state/list_state.rs

/// ❌ 错误：BTreeMap<ID, ...> 的遍历顺序是 ID 字典序，不是文档顺序！
/// elements: BTreeMap<ID, ListElement>,  // 不要这样用

/// ✅ 正确：用 HashMap 存储所有元素 + 双向链表维护文档顺序
#[derive(Debug, Clone)]
pub struct ListState {
    container_id: ContainerIdx,
    // 存储所有元素（包括已删除的 tombstone），按 ID 索引以便快速查找
    elements: HashMap<ID, ListElement>,
    // 文档顺序：双向链表
    head: ID,  // 虚拟头节点（ID::root()）
}

#[derive(Debug, Clone)]
struct ListElement {
    id: ID,
    value: LoroValue,
    left_origin: ID,      // 插入时的左邻居（用于重建并发顺序）
    deleted: bool,        // tombstone，RGA 不真正删除
    next: Option<ID>,     // 文档顺序的下一个
    prev: Option<ID>,     // 文档顺序的上一个
}
```

**RGA 文档顺序重建算法**：

当插入一个元素时，它的位置由 `left_origin` 决定。但如果有**多个并发插入**到同一个 `left_origin` 之后，需要确定性的排序规则：

```
排序规则（关键！）：
1. 找到 left_origin 在链表中的位置
2. 从 left_origin.next 开始向后遍历，收集所有"也是插在 left_origin 之后"的元素
3. 这些并发插入的元素按以下规则排序：
   a. lamport 小的在前
   b. lamport 相同，peer 小的在前
4. 新元素插入到这些并发元素中的正确位置
```

更精确地说（来自 RGA 论文）：
```
findInsertPosition(new_elem):
    left = new_elem.left_origin
    // 从 left 之后开始找
    current = left.next
    while current is not None:
        // current 也是 left 的直接后继候选
        if current.left_origin == left:
            // 并发冲突：按 (lamport, peer) 排序
            if (new_elem.lamport, new_elem.peer) < (current.lamport, current.peer):
                return current.prev  // 插入到 current 之前
        current = current.next
    return last_element  // 插到最后
```

- [ ] ### 6.3 List Handler

```rust
// src/handler/list.rs

pub struct ListHandler<'a> { ... }

impl<'a> ListHandler<'a> {
    pub fn insert(&mut self, pos: usize, value: impl Into<LoroValue>);
    pub fn delete(&mut self, pos: usize, len: usize);
    pub fn get(&self, pos: usize) -> Option<LoroValue>;
    pub fn len(&self) -> usize;  // 当前可见元素数（不含 tombstone）
    pub fn push(&mut self, value: impl Into<LoroValue>);
    pub fn get_container<T: ContainerHandler>(&self, pos: usize) -> T;
}
```

**Handler 的 `insert` 需要 pos → ID 转换**：
```rust
fn insert(&mut self, pos: usize, value: impl Into<LoroValue>) {
    let after_id = if pos == 0 {
        ID::root()  // 虚拟根节点
    } else {
        // 遍历链表找到第 pos-1 个可见元素的 ID
        self.state.nth_visible_id(pos - 1)
    };
    // 生成 Op: Insert { after: after_id, value }
    self.doc.submit_op(self.container_id, OpContent::List(
        ListOpInternal::Insert { after: after_id, value: value.into() }
    ));
}
```

**性能考虑**：
- `len()` 和 `get(pos)` 需要遍历链表跳过 tombstone，是 O(n)
- 后期可以维护一个 `Vec<ID>` 缓存可见元素的顺序，在每次 apply_op 后 dirty-rebuild
- 真正的 Loro 用 generic-btree（Rope/BTree）做这一步，性能是 O(log n)

---

## Phase 7: MovableList

> 在 List 基础上增加 move 操作。

- [ ] ### 7.1 新增操作

```rust
pub enum ListOpInternal {
    Insert { after: ID, value: LoroValue },
    Delete { target: ID, len: usize },
    Move { target: ID, after: ID },  // 把 target 移到 after 之后
}
```

- [ ] ### 7.2 Move 的并发冲突解决

**核心难点**：
- peer A 把 item X 移到位置 3（after = id_3）
- peer B 同时把 item X 移到位置 7（after = id_7）
- 最终只能在一个位置 → 需要 LWW 策略

**Loro 的策略**：为每个元素维护一个 `move_lamport`，move 操作基于 LWW 覆盖：

```rust
struct ListElement {
    id: ID,
    value: LoroValue,
    left_origin: ID,
    deleted: bool,
    // MovableList 新增：
    after: ID,              // 当前被指定的后继（由最后一次 winning move 决定）
    move_lamport: Lamport,  // 最后一次 move 的时间戳
    move_peer: PeerID,      // tie-break
}
```

```rust
fn apply_move(&mut self, op: &Op, target: ID, after: ID) {
    let elem = self.elements.get_mut(&target).unwrap();
    // LWW 比较：新的 move 赢才更新
    if op.lamport > elem.move_lamport
       || (op.lamport == elem.move_lamport && op.id.peer > elem.move_peer) {
        elem.after = after;
        elem.move_lamport = op.lamport;
        elem.move_peer = op.id.peer;
        self.order_dirty = true;  // 标记文档顺序需要重建
    }
}
```

**文档顺序重建**：
MovableList 的文档顺序不能简单地用双向链表维护，因为 move 会改变元素的位置。每次 move 后，需要基于所有元素的 `after` 引用 + LWW 信息，重新构建全序。

简单实现：每次 move 后，按以下规则排序所有元素：
1. 按 `after` 链组织成森林
2. 每个 `after` 下的并发元素按 `(move_lamport, move_peer)` 排序
3. DFS 遍历得到文档顺序

> Loro 的实际实现更复杂，用了 fractional_index + BTree。初期可以先做简单版本。

---

## Phase 8: Text

> 本质是 List 的特化版本，但算法不同。建议**先做简化版，再升级**。

- [ ] ### 8.1 两阶段实现策略

**阶段 A（先实现，验证架构）**：基于 List 的 Text
- 每个字符是一个 `ListElement`
- 直接复用 ListState 的逻辑
- `insert(pos, "hello")` = 5 个 List insert 操作
- 缺点：每个字符一个 Op，内存爆炸；O(n) 索引
- 优点：快速验证 Text Handler API 和事件输出

**阶段 B（后替换）**：Fugue 算法
- 用 span（连续字符块）作为操作单元
- 用 rope/btree 存储，O(log n) 索引
- Fugue 特有的左右锚点并发排序

- [ ] ### 8.2 TextOp

```rust
pub enum TextOp {
    Insert { pos: usize, text: String },
    Delete { pos: usize, len: usize },
}
```

内部同样需要从 pos 转换为 ID 引用。阶段 A 复用 List 的 `after` 机制；阶段 B 引入 Fugue 的 `left_origin` + `right_origin`。

- [ ] ### 8.3 FugueSpan（阶段 B 的数据结构）

```rust
// src/container/text/fugue_span.rs

#[derive(Debug, Clone)]
pub struct FugueSpan {
    pub id: ID,
    pub text: String,          // 连续字符块
    pub deleted: bool,
    // Fugue 特有：左右锚点
    pub left_origin: Option<ID>,   // 插入时的左邻居
    pub right_origin: Option<ID>,  // 插入时的右邻居
}
```

**Fugue 并发插入排序**：
```
并发插入同一位置时：
  1. 先按 left_origin 的 (lamport, peer) 排序
  2. 相同 left_origin 时，按自身的 (lamport, peer) 排序
  3. 保证结果是确定性的
```

**注意**：Fugue 算法的完整实现涉及大量边界情况（删除后的并发插入、左锚点失效回退、span 合并与分裂等）。不要在一开始就追求完美实现，先用 List-based 版本跑通上层。

- [ ] ### 8.4 Text Handler

```rust
// src/handler/text.rs

pub struct TextHandler<'a> { ... }

impl<'a> TextHandler<'a> {
    pub fn insert(&mut self, pos: usize, text: &str);
    pub fn delete(&mut self, pos: usize, len: usize);
    pub fn to_string(&self) -> String;
    pub fn len(&self) -> usize;  // Unicode 字符数（非字节数）
    pub fn len_utf16(&self) -> usize;  // WASM/前端需要 UTF-16 长度
}
```

---

## Phase 9: Rich Text

> 在 Text 基础上增加样式（Mark / Unmark）。

- [ ] ### 9.1 样式操作

```rust
pub enum TextOp {
    Insert { pos: usize, text: String },
    Delete { pos: usize, len: usize },
    Mark { 
        start: usize, 
        end: usize, 
        key: String, 
        value: LoroValue,
        info: MarkInfo,  // 包含 lamport、peer 等 LWW 信息
    },
    Unmark { 
        start: usize, 
        end: usize, 
        key: String,
        info: MarkInfo,
    },
}
```

- [ ] ### 9.2 样式存储

样式本质上是一个 **RangeMap**：每种样式 key 对应一个区间树。

```rust
struct RichTextState {
    text: TextState,                          // 底层文本（Fugue）
    // 每种样式 key 一个范围映射
    styles: HashMap<String, Vec<StyleRange>>,  
}

struct StyleRange {
    start_id: ID,       // 样式开始位置的元素 ID
    end_id: ID,         // 样式结束位置的元素 ID
    value: Option<LoroValue>,  // Some = 设置样式, None = 清除样式
    lamport: Lamport,
    peer: PeerID,
}
```

**样式冲突解决**：
同一文本区间的同一 key 的样式设置是 **LWW**（和 Map 一样）。`lamport` 大的覆盖小的。

**为什么用 ID 而非 pos 表示范围**：
和 List 一样，pos 会漂移。用 ID 表示范围才能保证并发编辑时样式附着在正确的文本上。

- [ ] ### 9.3 RichText Handler

```rust
impl<'a> TextHandler<'a> {
    // 基础文本方法已有，新增：
    pub fn mark(&mut self, start: usize, end: usize, key: &str, value: impl Into<LoroValue>);
    pub fn unmark(&mut self, start: usize, end: usize, key: &str);
    pub fn get_marks(&self, pos: usize) -> IndexMap<String, LoroValue>;
    
    /// 输出 Quill Delta 格式：
    /// [{insert: "hello", attributes: {bold: true}}, ...]
    pub fn to_delta(&self) -> Vec<TextDelta>;
}
```

---

## Phase 10: Tree（可移动树）

> 最复杂的 CRDT。

- [ ] ### 10.1 TreeOp

```rust
// src/op/tree_op.rs

pub enum TreeOp {
    Create { 
        target: TreeID, 
        parent: TreeParentID, 
        position: FractionalIndex,
    },
    Move { 
        target: TreeID, 
        parent: TreeParentID, 
        position: FractionalIndex,
    },
    Delete { 
        target: TreeID,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeParentID {
    Root,
    Node(TreeID),
    /// 被删除的节点统一挂到 DELETED_ROOT 下
    Deleted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TreeID {
    pub peer: PeerID,
    pub counter: Counter,
}
```

- [ ] ### 10.2 TreeState

```rust
// src/state/tree_state.rs

struct TreeState {
    container_id: ContainerIdx,
    nodes: HashMap<TreeID, TreeNode>,
    // parent -> children（有序）
    children_map: HashMap<TreeParentID, Vec<TreeID>>,
}

struct TreeNode {
    id: TreeID,
    parent: TreeParentID,
    position: FractionalIndex,  // 在兄弟节点中的排序位置
    move_lamport: Lamport,      // 最后一次 move 的时间（LWW）
    move_peer: PeerID,
    deleted: bool,
}
```

- [ ] ### 10.3 并发 Move 的冲突解决

**核心问题**：
- A 把节点 X 移到节点 P1 下
- B 同时把节点 X 移到节点 P2 下
- 必须：无环 + 确定性结果

**策略**：
1. **LWW**：最后写入胜出，同 Counter/MovableList
2. **循环检测**：move 前检查 `new_parent` 是否是 `target` 的后代，是则拒绝（保持当前 parent）
3. **Fractional Index**：兄弟排序用分数索引，不依赖绝对位置

```rust
fn apply_move(&mut self, op: &Op, target: TreeID, new_parent: TreeParentID, position: FractionalIndex) {
    // 1. 检查循环
    if self.is_descendant(&new_parent, &target) { return; }

    // 2. LWW 比较
    let node = self.nodes.get_mut(&target).unwrap();
    if op.lamport > node.move_lamport
       || (op.lamport == node.move_lamport && op.id.peer > node.move_peer) {
        // 3. 从旧 parent 的 children 中移除
        if let Some(old_children) = self.children_map.get_mut(&node.parent) {
            old_children.retain(|id| id != &target);
        }
        // 4. 更新节点
        node.parent = new_parent;
        node.position = position;
        node.move_lamport = op.lamport;
        node.move_peer = op.id.peer;
        // 5. 插入到新 parent 的 children 中
        self.children_map.entry(new_parent).or_default().push(target);
        // 6. 按 FractionalIndex 重排 children
        self.sort_children(new_parent);
    }
}
```

- [ ] ### 10.4 FractionalIndex

```rust
// src/fractional_index.rs

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct FractionalIndex(Vec<u8>);  // 内部是字节串

impl FractionalIndex {
    pub fn default() -> Self;                          // 中间值
    pub fn between(a: &Self, b: &Self) -> Self;       // 两个 key 之间生成新 key
    pub fn before(&self) -> Self;                     // 在 self 之前
    pub fn after(&self) -> Self;                      // 在 self 之后
}
```

**算法**：基于字节串中间值生成。如 `"a"` 和 `"b"` 之间生成 `"aV"`（取中间字节值）。
需要处理边界情况：当两个 key 之间没有空间时，需要扩展长度（如 `"a\xFF"` 和 `"b"` 之间生成 `"a\xFF\x80"`）。

- [ ] ### 10.5 Tree 的 Metadata Map

在 Loro 中，**每个 Tree 节点都隐式绑定一个 Map 容器作为其 metadata**：

```rust
impl TreeHandler<'_> {
    /// 创建节点时，自动为其创建一个 Map 容器作为 metadata
    pub fn create(&mut self, parent: Option<TreeID>) -> TreeID {
        let node_id = self.generate_tree_id();
        let meta_container_id = ContainerID::new_normal(node_id.into_id(), ContainerType::Map);
        // 注册到 arena，建立 TreeID -> Map Container 的关联
        ...
    }
    
    /// 获取节点的 metadata MapHandler
    pub fn get_meta(&mut self, target: TreeID) -> MapHandler<'_> {
        let meta_cid = target.associated_meta_container();
        MapHandler::new(self.doc, meta_cid)
    }
}
```

在 `TreeState` 中需要记录：
```rust
struct TreeState {
    nodes: HashMap<TreeID, TreeNode>,
    children_map: HashMap<TreeParentID, Vec<TreeID>>,
    // 每个 TreeID 对应一个 metadata Map 的 ContainerIdx
    meta_map: HashMap<TreeID, ContainerIdx>,
}
```

当 Tree 节点被删除时，其 metadata Map 容器也被标记为不可达（但内部状态保留， revived 时恢复）。

- [ ] ### 10.6 Tree Handler

```rust
// src/handler/tree.rs

pub struct TreeHandler<'a> { ... }

impl<'a> TreeHandler<'a> {
    pub fn create(&mut self, parent: Option<TreeID>) -> TreeID;
    pub fn mov(&mut self, target: TreeID, new_parent: Option<TreeID>);
    pub fn delete(&mut self, target: TreeID);
    pub fn children(&self, parent: Option<TreeID>) -> Vec<TreeID>;
    pub fn parent(&self, target: TreeID) -> Option<TreeID>;
    pub fn is_deleted(&self, target: TreeID) -> bool;
    pub fn root_nodes(&self) -> Vec<TreeID>;
    pub fn get_meta(&mut self, target: TreeID) -> MapHandler<'_>;
    pub fn foreach(&self, f: impl Fn(TreeID, /*depth*/ usize));
}
```

---

## Phase 11: Merge & Sync（文档合并与同步）

> 两个 CoralDoc 之间的数据交换。

- [ ] ### 11.1 导出变更

```rust
impl CoralDoc {
    /// 导出自 `from` 版本以来本地产生的所有 Change
    pub fn export(&self, from: &VersionVector) -> Vec<Change> {
        self.oplog.export_changes(from)
    }
    
    /// 导出快照：当前状态的完整表示（用于新节点首次同步）
    pub fn export_snapshot(&self) -> Snapshot {
        // 编码所有 ContainerState 的 to_diff
    }
}
```

- [ ] ### 11.2 导入变更

```rust
impl CoralDoc {
    /// 导入远程 Change，应用到 OpLog 和 State
    pub fn import(&mut self, changes: &[Change]) -> Result<()> {
        for change in changes {
            // 1. 检查 Change 的 deps 是否已满足
            // 2. 检查 Change 是否已存在（去重）
            // 3. 写入 OpLog
            // 4. 逐个 apply Op 到 DocState
            // 5. 更新 frontiers / vv
        }
    }
}
```

**要点**：
- 导入时需要处理 **乱序到达**：Change B 先到达，但它的 dep A 还没到达 → B 进入 pending queue
- 当 A 到达后，需要检查 pending queue，级联应用所有就绪的 Change
- 已存在的 Change 要幂等跳过（用 ID 去重）

- [ ] ### 11.3 两文档合并

```rust
/// 将 other 的变更合并到 self 中
pub fn merge(&mut self, other: &CoralDoc) -> Result<()> {
    let changes = other.export(self.vv());
    self.import(&changes)
}
```

**核心测试**：
```rust
#[test]
fn test_merge_commutative() {
    let mut a = CoralDoc::new();
    let mut b = CoralDoc::new();
    
    // A 编辑
    a.get_map("root").insert("key", "A");
    // B 编辑
    b.get_map("root").insert("key", "B");
    
    // 互相导入
    let changes_a = a.export(b.vv());
    let changes_b = b.export(a.vv());
    a.import(&changes_b);
    b.import(&changes_a);
    
    // 最终状态必须一致
    assert_eq!(a.get_value(), b.get_value());
}
```

---

## Phase 12: Checkout & Time Travel（版本回滚与分支）

> OpLog 与 DocState 分离的核心收益。

- [ ] ### 12.1 Checkout 到指定版本

```rust
impl CoralDoc {
    /// 将 DocState 回滚/前进到指定的 Frontiers 版本
    pub fn checkout(&mut self, target: &Frontiers) -> Result<()> {
        let current = self.oplog.frontiers().clone();
        
        // 1. 找到 current 和 target 的 LCA
        let lca = self.oplog.dag().find_common_ancestor(&current, target);
        
        // 2. 从 current 回滚到 LCA（逆向 apply diff）
        //    或更简单的做法：丢弃当前 state，从空重建
        self.state = DocState::new();
        
        // 3. 从 LCA 前进到 target（正向 apply 所有 Change）
        let changes = self.oplog.get_changes_between(&lca, target);
        for change in changes {
            for op in &change.ops {
                self.state.apply_op(op)?;
            }
        }
        
        self.oplog.set_frontiers(target.clone());
        Ok(())
    }
}
```

**简单实现 vs 优化实现**：
- **简单**：每次 checkout 都丢弃当前 state，从空重放所有到 target 的 Change。慢但正确。
- **优化**：维护 state 缓存（LRU），checkout 到最近访问过的版本时直接复用。或者做增量 diff（向前/向后应用 diff）。

- [ ] ### 12.2 Fork（分支）

```rust
impl CoralDoc {
    /// 从当前状态克隆一个独立的文档
    pub fn fork(&self) -> CoralDoc {
        CoralDoc {
            oplog: self.oplog.clone(),
            state: self.state.fork(),
            arena: self.arena.clone(),
            peer_id: random_peer_id(),
            pending_ops: vec![],
        }
    }
}
```

Fork 后的文档有独立的 `peer_id`，可以继续独立编辑，之后再 `merge` 回来。

---

## 数据结构修正备忘

| 位置 | 原计划 | 修正 |
|------|--------|------|
| ListState.elements | `BTreeMap<ID, ListElement>` | `HashMap<ID, ListElement>` + 双向链表维护顺序 |
| List 顺序 | ID 字典序 | 由 `left_origin` + `(lamport, peer)` 排序决定 |
| Counter 类型 | G-Counter | PN-Counter（可正负） |
| Text 实现 | 直接 Fugue | 先做 List-based 简化版验证架构，再替换 |
| Tree | 仅节点结构 | 补充每个节点的隐式 metadata Map |
| Op.container | ContainerID | 内部用 ContainerIdx，API 层用 ContainerID |

---

## 测试策略

> 建议每完成一个阶段就写测试，不要攒到最后。

### 每个 CRDT 的基础测试

| 测试 | 目的 |
|------|------|
| 单操作正确性 | insert/get/delete/move 基本功能 |
| 幂等性 | 同一个 Op apply 两次结果不变 |
| 交换律 | A→B 和 B→A 合并结果相同 |
| 结合律 | (A→B)→C 和 A→(B→C) 结果相同 |
| 并发冲突 | 两个 peer 同时操作同一位置/key/元素 |

### 跨阶段集成测试

```rust
/// 经典 CRDT 一致性测试模式
fn fuzz_two_peers() {
    let mut peer_a = CoralDoc::new();
    let mut peer_b = CoralDoc::new();
    
    // 各执行 N 个随机操作
    for _ in 0..100 {
        random_op(&mut peer_a);
        random_op(&mut peer_b);
    }
    
    // 互相导入
    let to_b = peer_a.export(peer_b.vv());
    let to_a = peer_b.export(peer_a.vv());
    peer_a.import(&to_a);
    peer_b.import(&to_b);
    
    // 状态、版本、frontiers 必须完全一致
    assert_eq!(peer_a.get_value(), peer_b.get_value());
    assert_eq!(peer_a.frontiers(), peer_b.frontiers());
}
```

### 推荐测试工具

| 工具 | 用途 |
|------|------|
| `#[test]` | 单元测试、回归测试 |
| `proptest` | 基于属性的随机测试（如随机操作序列后检查不变量） |
| `cargo fuzz` | 模糊测试（多 peer 并发随机编辑） |
| `loom` | 并发模型检测（如果引入多线程） |

### 关键不变量检查清单

- [ ] OpLog 的 DAG 始终无环
- [ ] VersionVector 的单调性：import 新 Change 后只增不减
- [ ] Frontiers 的确定性：相同 Frontiers 必然对应相同 State（给定确定性 apply）
- [ ] Tombstone 不泄漏：List/Text 的已删除元素对用户不可见（get_value 过滤）
- [ ] Tree 无环：任何时刻 Tree 的 parent 关系无循环
- [ ] 嵌套容器一致性：Map 中插入子容器后，Arena 的 parent 关系正确

---

## 依赖库建议

| 库 | 用途 | 阶段 |
|----|------|------|
| `indexmap` | `LoroValue::Map` 保序存储 | Phase 1 |
| `serde` + `serde_json` | 序列化/JSON 输出 | Phase 1 |
| `thiserror` | 错误类型定义 | Phase 2 |
| `proptest` | 随机属性测试 | Phase 3+ |
| `im` | 不可变数据结构（fork 时共享 state） | Phase 12 |

---

## 编码与传输（后期可选）

本计划先聚焦核心 CRDT 算法，编码/传输可以后期再做。但设计时预留接口：

```rust
// 预留接口，Phase 12 之后再实现

trait Encode {
    fn encode(&self) -> Vec<u8>;
    fn decode(data: &[u8]) -> Result<Self>;
}

impl Encode for Change { ... }
impl Encode for Snapshot { ... }
```

Loro 使用 `serde_columnar` 做列式压缩编码，初期可以用 `serde + postcard/bincode` 做简单二进制编码。

---

## 统计

| 指标 | 数量 |
|------|------|
| 已完成 | 10 |
| 未完成 | 43 |
| 总数 | 53 |
| 完成百分比 | 18.9% |
