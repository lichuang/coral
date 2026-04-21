# Coral - CRDT Implementation Plan (State-First Architecture)

> 本计划采用"先独立数据结构，后文档绑定"的分层策略。
> 
> 第一阶段（Phase 1-3）：每个 CRDT 是自包含的数据结构，可独立测试，不依赖 CoralDoc/OpLog/Transaction。
> 
> 第二阶段（Phase 4-6）：在独立 CRDT 之上搭建文档运行时，实现跨容器协作、事件、版本控制。

## 阶段总览

```
Phase 1: 基础类型与核心协议（已完成）
    ↓
Phase 2: 独立 CRDT 状态实现（自包含，可独立单元测试）
    ↓
Phase 3: 容器统一接口与差量（Diff）
    ↓
Phase 4: 文档运行时（Arena、OpLog、DAG、DocState、Transaction、Handler）
    ↓
Phase 5: 协作、事件与版本控制（Merge、Sync、Event、Checkout、Fork）
    ↓
Phase 6: 高级与优化（Fugue、RichText、GC、Encoding、性能）
```

---

## Phase 1: 基础类型与核心协议

> 一切依赖的起点。已完成。

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

// 后续 Phase 2 逐个定义具体 Op，先定义占位：
// MapOp, ListOp, TextOp, TreeOp, CounterOp
```

**注意**：Op 是**操作意图**的描述，独立于文档运行时。每个 CRDT 状态机只需要理解自己对应的 Op 变体。

- [x] ### 1.7 Change（变更组）

```rust
// src/change.rs

/// 一次因果边界内的操作集合。
/// 这些 Op 共享同一个起始 lamport、timestamp 和 deps。
/// Change 是 DAG 中的节点，也是传输/存储的单元。
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

## Phase 2: 独立 CRDT 状态实现

> **核心原则**：每个 CRDT 是自包含的状态机，只需要 `Op` 和 `LoroValue` 即可工作，不依赖 CoralDoc/OpLog/Arena/Transaction。
> 
> 验证方式：每个 State 完成后，写独立的单元测试验证幂等性、交换律、结合律、并发冲突。

---

- [ ] ### 2.1 CounterState（PN-Counter）

> 最简单的 CRDT，用来热身验证"独立状态机"模型。

#### 2.1.1 CounterOp

```rust
// src/op/counter_op.rs

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CounterOp {
    pub delta: i64,  // 增量，可以为负
}
```

#### 2.1.2 CounterState

```rust
// src/state/counter_state.rs

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CounterState {
    /// 当前聚合值（缓存）
    pub value: i64,
    /// 各 peer 的 delta 累加（可选，用于调试和 diff）
    pub per_peer_deltas: HashMap<PeerID, i64>,
}

impl CounterState {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// 应用一个 CounterOp（纯状态更新，无外部依赖）
    pub fn apply_op(&mut self, op: &CounterOp, peer: PeerID) {
        self.value += op.delta;
        *self.per_peer_deltas.entry(peer).or_insert(0) += op.delta;
    }
    
    pub fn get_value(&self) -> LoroValue {
        LoroValue::I64(self.value)
    }
    
    /// 直接合并另一个 CounterState（用于独立测试验证交换律）
    pub fn merge(&mut self, other: &CounterState) {
        for (peer, delta) in &other.per_peer_deltas {
            *self.per_peer_deltas.entry(*peer).or_insert(0) += delta;
        }
        self.value += other.value; // 注意：实际应重新聚合，这里简化
    }
}
```

**独立测试要求**：
- [ ] 单操作：`apply_op(+3)` 后 `value == 3`
- [ ] 幂等性：同一 `Op` apply 两次，值不变（由上层去重，但 State 应保证 `+=` 的幂等性需配合 Op 去重）
- [ ] 交换律：`A.merge(B) == B.merge(A)`
- [ ] 结合律：`(A.merge(B)).merge(C) == A.merge(B.merge(C))`
- [ ] 并发冲突：A 加 3，B 减 2，合并后为 +1

---

- [ ] ### 2.2 LWW-Register

> 单值 CRDT，LWW-Map 和许多高级结构的基础单元。

#### 2.2.1 状态结构

```rust
// src/state/lww_register.rs

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LWWRegister<T> {
    pub value: Option<T>,     // None 表示已删除
    pub lamport: Lamport,
    pub peer: PeerID,         // tie-break
}

impl<T: Clone> LWWRegister<T> {
    pub fn new() -> Self {
        Self { value: None, lamport: 0, peer: 0 }
    }
    
    /// 合并另一个 Register（LWW 语义）
    pub fn merge(&mut self, other: &LWWRegister<T>) {
        match self.lamport.cmp(&other.lamport) {
            Ordering::Less    => { *self = other.clone(); }
            Ordering::Greater => {}
            Ordering::Equal   => {
                if self.peer < other.peer {
                    *self = other.clone();
                }
            }
        }
    }
    
    /// 用新的 (value, lamport, peer) 更新
    pub fn update(&mut self, value: Option<T>, lamport: Lamport, peer: PeerID) {
        let other = LWWRegister { value, lamport, peer };
        self.merge(&other);
    }
}
```

**独立测试要求**：
- [ ] Lamport 大的覆盖小的
- [ ] Lamport 相同时 peer 大的赢
- [ ] `None` 参与 LWW 比较（删除可以赢过写入）

---

- [ ] ### 2.3 MapState

> 基于 LWW-Register 的键值对。每个 key 背后是一个 Register。

#### 2.3.1 MapOp

```rust
// src/op/map_op.rs

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MapOp {
    Insert { key: String, value: LoroValue },
    Delete { key: String },
}
```

#### 2.3.2 MapState

```rust
// src/state/map_state.rs

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapState {
    /// 每个 key 一个 LWW-Register。
    /// 保留 tombstone（value == None）以支持并发删除后插入的 LWW 比较。
    registers: IndexMap<String, LWWRegister<LoroValue>>,
}

impl MapState {
    pub fn new() -> Self {
        Self { registers: IndexMap::new() }
    }
    
    pub fn apply_op(&mut self, op: &MapOp, lamport: Lamport, peer: PeerID) {
        match op {
            MapOp::Insert { key, value } => {
                let reg = self.registers.entry(key.clone())
                    .or_insert_with(|| LWWRegister::new());
                reg.update(Some(value.clone()), lamport, peer);
            }
            MapOp::Delete { key } => {
                let reg = self.registers.entry(key.clone())
                    .or_insert_with(|| LWWRegister::new());
                reg.update(None, lamport, peer);
            }
        }
    }
    
    pub fn get(&self, key: &str) -> Option<&LoroValue> {
        self.registers.get(key)?.value.as_ref()
    }
    
    pub fn get_value(&self) -> LoroValue {
        LoroValue::Map(
            self.registers.iter()
                .filter_map(|(k, r)| r.value.as_ref().map(|v| (k.clone(), v.clone())))
                .collect()
        )
    }
    
    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.registers.keys()
    }
    
    /// 合并另一个 MapState（用于独立测试）
    pub fn merge(&mut self, other: &MapState) {
        for (key, other_reg) in &other.registers {
            let reg = self.registers.entry(key.clone())
                .or_insert_with(|| LWWRegister::new());
            reg.merge(other_reg);
            // 清理：如果合并后 value 为 None 且没有并发写入，可以保留 tombstone
        }
    }
}
```

**要点**：
- `get_value` 只返回 value != None 的键，已删除的 key 对用户不可见
- 但内部保留 tombstone，因为并发 Insert 同一 key 需要 LWW 比较
- `IndexMap` 保证 key 遍历顺序是插入顺序

**独立测试要求**：
- [ ] 插入/读取/删除基本功能
- [ ] 并发写入同一 key：LWW 胜出
- [ ] 并发删除与写入：LWW 决定最终可见性
- [ ] 交换律/结合律：两个 MapState 的 merge

---

- [ ] ### 2.4 ListState（RGA）

> **难度跳升点** — 并发有序集合。

#### 2.4.1 ListOp

```rust
// src/op/list_op.rs

/// 内部真正存储/传输的操作（基于 ID 引用，保证并发正确性）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListOp {
    Insert {
        id: ID,           // 新元素的唯一 ID
        after: ID,        // 插入到哪个 ID 之后
        value: LoroValue,
    },
    Delete {
        id: ID,           // 要删除的元素 ID
    },
}
```

**为什么用 ID 而非 pos**：并发编辑时 pos 会漂移。ID 引用保证位置语义的稳定性。

#### 2.4.2 ListState（核心数据结构）

```rust
// src/state/list_state.rs

/// RGA 列表元素
#[derive(Debug, Clone)]
pub struct ListElement {
    pub id: ID,
    pub value: LoroValue,
    pub left_origin: ID,      // 插入时的左邻居
    pub deleted: bool,        // tombstone
}

/// List CRDT 状态（纯数据结构）
#[derive(Debug, Clone)]
pub struct ListState {
    /// 所有元素（包括已删除的），按 ID 索引以便快速查找
    elements: HashMap<ID, ListElement>,
    /// 当前可见元素的文档顺序。
    /// 每次 apply_insert 后重新计算插入位置，维护此 Vec。
    visible_order: Vec<ID>,
}

impl ListState {
    pub fn new() -> Self {
        Self {
            elements: HashMap::new(),
            visible_order: Vec::new(),
        }
    }
    
    pub fn apply_op(&mut self, op: &ListOp, lamport: Lamport) {
        match op {
            ListOp::Insert { id, after, value } => {
                if self.elements.contains_key(id) {
                    return; // 幂等：已存在则忽略
                }
                let elem = ListElement {
                    id: *id,
                    value: value.clone(),
                    left_origin: *after,
                    deleted: false,
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
            // 虚拟根节点，插入到最前面
            None
        } else {
            self.visible_order.iter().position(|&id| id == left_origin)
        };
        
        let insert_pos = match left_pos {
            None => 0, // left_origin 不在可见列表中（可能是根或已删除），插到开头
            Some(pos) => {
                // 从 pos+1 开始向后遍历，找到所有 left_origin == 当前插入左邻居的元素
                // 这些元素与 new_id 是并发插入到同一位置的
                let mut target_pos = pos + 1;
                for (i, &id) in self.visible_order.iter().enumerate().skip(pos + 1) {
                    let elem = self.elements.get(&id).unwrap();
                    if elem.left_origin != left_origin {
                        // 已经离开这个并发组
                        break;
                    }
                    // 并发冲突：按 (lamport, peer) 排序
                    let existing_lamport = self.estimate_lamport(id); // 实际应从 Op 记录
                    if (lamport, new_id.peer) < (existing_lamport, id.peer) {
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
    
    pub fn len(&self) -> usize {
        self.visible_order.len()
    }
    
    pub fn get(&self, pos: usize) -> Option<&LoroValue> {
        let id = self.visible_order.get(pos)?;
        self.elements.get(id).map(|e| &e.value)
    }
    
    pub fn get_value(&self) -> LoroValue {
        LoroValue::List(
            self.visible_order.iter()
                .map(|id| self.elements.get(id).unwrap().value.clone())
                .collect()
        )
    }
    
    /// 合并另一个 ListState（用于独立测试）
    pub fn merge(&mut self, other: &ListState) {
        // 合并所有 elements（已存在的保留，不存在的插入）
        for (id, elem) in &other.elements {
            if !self.elements.contains_key(id) {
                self.elements.insert(*id, elem.clone());
            }
        }
        // 重建 visible_order（基于 RGA 排序规则）
        self.rebuild_visible_order();
    }
    
    fn rebuild_visible_order(&mut self) {
        // 收集所有非删除元素的 ID，按 RGA 规则排序
        // 简化实现：基于 left_origin 和 (lamport, peer) 做拓扑排序
        self.visible_order.clear();
        // ... 具体重建逻辑
    }
}
```

**要点**：
- **不用双向链表**：`next/prev` 指针在批量导入时维护困难。用 `HashMap<ID, Element>` + `Vec<ID>` 更可靠。
- `visible_order` 在每次 insert 时增量更新，在 merge 后全量重建。
- `lamport` 的获取：ListElement 需要存储创建时的 lamport（用于并发排序）。上面伪代码中 `estimate_lamport` 只是占位，实际应在 `ListElement` 中增加 `lamport` 字段。

**修正**：`ListElement` 应补充 `lamport`：
```rust
pub struct ListElement {
    pub id: ID,
    pub value: LoroValue,
    pub left_origin: ID,
    pub lamport: Lamport,  // 创建时的 lamport，用于并发排序
    pub deleted: bool,
}
```

**独立测试要求**：
- [ ] 顺序插入/删除
- [ ] 并发插入到同一位置：按 (lamport, peer) 排序
- [ ] 并发删除与插入：删除标记 tombstone，不影响其他元素顺序
- [ ] 交换律/结合律：两个 ListState merge 后顺序一致

---

- [ ] ### 2.5 MovableListState

> 在 List 基础上增加 move 操作。

#### 2.5.1 扩展的 ListOp

```rust
pub enum ListOp {
    Insert { id: ID, after: ID, value: LoroValue },
    Delete { id: ID },
    Move { id: ID, after: ID },  // 把 id 移到 after 之后
}
```

#### 2.5.2 扩展的 ListElement

```rust
pub struct ListElement {
    pub id: ID,
    pub value: LoroValue,
    pub left_origin: ID,
    pub lamport: Lamport,
    pub deleted: bool,
    // MovableList 新增：
    pub after: ID,              // 当前被指定的后继（由最后一次 winning move 决定）
    pub move_lamport: Lamport,  // 最后一次 move 的时间戳
    pub move_peer: PeerID,      // tie-break
}
```

#### 2.5.3 Move 应用逻辑

```rust
impl ListState {
    pub fn apply_move(&mut self, id: ID, after: ID, lamport: Lamport, peer: PeerID) {
        let elem = match self.elements.get_mut(&id) {
            Some(e) => e,
            None => return,
        };
        
        // LWW 比较：新的 move 赢才更新
        if lamport > elem.move_lamport
           || (lamport == elem.move_lamport && peer > elem.move_peer) {
            elem.after = after;
            elem.move_lamport = lamport;
            elem.move_peer = peer;
            self.rebuild_visible_order(); // move 改变全局顺序，需要重建
        }
    }
}
```

**文档顺序重建（MovableList）**：
每次 move 后，基于所有元素的 `after` 引用 + LWW 信息，重新构建全序：
1. 按 `after` 链组织成森林
2. 每个 `after` 下的并发元素按 `(move_lamport, move_peer)` 排序
3. DFS 遍历得到文档顺序

> 注意：这是简化实现。Loro 实际使用 fractional_index + BTree，性能更好。初期先保证正确性。

---

- [ ] ### 2.6 TextState（List-based 简化版）

> 先复用 ListState 的逻辑验证架构，后续再升级为 Fugue。

#### 2.6.1 TextOp

```rust
// src/op/text_op.rs

pub enum TextOp {
    Insert { pos: usize, text: String },   // 用户侧（Handler 层转换）
    Delete { pos: usize, len: usize },     // 用户侧
}

/// 内部存储：每个字符是一个 ListElement
/// TextState 本质是 ListState<char> 的包装
```

#### 2.6.2 TextState

```rust
// src/state/text_state.rs

pub struct TextState {
    list: ListState,  // 底层复用 ListState
}

impl TextState {
    pub fn new() -> Self {
        Self { list: ListState::new() }
    }
    
    /// 内部方法：将 pos 转换为 after ID（基于当前 visible_order）
    fn pos_to_after_id(&self, pos: usize) -> Option<ID> { ... }
    
    /// 内部方法：将 user-level Insert 转换为 ListOp 序列
    pub fn insert(&mut self, pos: usize, text: &str, id_gen: &mut dyn IdGenerator) {
        let after = if pos == 0 { ID::root() } else {
            self.pos_to_after_id(pos - 1).unwrap_or(ID::root())
        };
        for ch in text.chars() {
            let id = id_gen.next_id();
            self.list.apply_op(&ListOp::Insert { id, after, value: LoroValue::String(ch.to_string()) }, 0);
            // lamport 由外部传入，独立测试时可简化
        }
    }
    
    pub fn to_string(&self) -> String {
        self.list.visible_order.iter()
            .map(|id| self.list.elements.get(id).unwrap().value.as_str().unwrap())
            .collect()
    }
}
```

**要点**：
- 阶段 A 每个字符一个 Op，内存和性能都很差，但**能快速验证 Text Handler API 和事件输出**
- `id_gen` 是 ID 分配器抽象。独立测试时用本地递增分配器；文档绑定时用 Doc 的全局分配器。

---

- [ ] ### 2.7 TreeState

> 最复杂的独立 CRDT。

#### 2.7.1 TreeOp

```rust
// src/op/tree_op.rs

pub enum TreeOp {
    Create { target: TreeID, parent: TreeParentID },
    Move { target: TreeID, parent: TreeParentID },
    Delete { target: TreeID },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeParentID {
    Root,
    Node(TreeID),
    Deleted,
}

pub type TreeID = ID;  // Tree 节点 ID 就是 Op 的 ID
```

#### 2.7.2 TreeState

```rust
// src/state/tree_state.rs

pub struct TreeNode {
    pub id: TreeID,
    pub parent: TreeParentID,
    pub move_lamport: Lamport,
    pub move_peer: PeerID,
    pub deleted: bool,
    // 注意：兄弟节点排序初期用 Vec，后期升级为 FractionalIndex
    pub index: usize,  // 在兄弟中的位置（简化版，非并发安全）
}

pub struct TreeState {
    nodes: HashMap<TreeID, TreeNode>,
}

impl TreeState {
    pub fn apply_create(&mut self, target: TreeID, parent: TreeParentID, lamport: Lamport, peer: PeerID) {
        if self.nodes.contains_key(&target) { return; }
        self.nodes.insert(target, TreeNode {
            id: target,
            parent,
            move_lamport: lamport,
            move_peer: peer,
            deleted: false,
            index: 0,
        });
    }
    
    pub fn apply_move(&mut self, target: TreeID, new_parent: TreeParentID, lamport: Lamport, peer: PeerID) {
        // 1. 循环检测
        if self.is_descendant(&new_parent, &target) { return; }
        
        // 2. LWW 比较
        let node = self.nodes.get_mut(&target).unwrap();
        if lamport > node.move_lamport
           || (lamport == node.move_lamport && peer > node.move_peer) {
            node.parent = new_parent;
            node.move_lamport = lamport;
            node.move_peer = peer;
        }
    }
    
    pub fn apply_delete(&mut self, target: TreeID) {
        if let Some(node) = self.nodes.get_mut(&target) {
            node.deleted = true;
        }
    }
    
    fn is_descendant(&self, parent: &TreeParentID, target: &TreeID) -> bool {
        // 检查 parent 是否是 target 的后代（避免循环）
        // ...
        false
    }
    
    pub fn children(&self, parent: TreeParentID) -> Vec<&TreeNode> {
        // 返回指定 parent 下的所有非删除子节点
        self.nodes.values()
            .filter(|n| n.parent == parent && !n.deleted)
            .collect()
    }
}
```

**要点**：
- 初期不做 `FractionalIndex`（兄弟排序），用 `Vec` 顺序 + `index` 字段。
- 循环检测是 Tree 的核心安全机制。
- metadata Map（每个 Tree 节点绑定一个 Map）是**文档运行时**的概念，独立 State 阶段不做。

---

## Phase 3: 容器统一接口与差量

> 为所有独立 CRDT 状态定义统一接口，使它们能被 DocState 统一管理。

---

- [ ] ### 3.1 ContainerState trait

```rust
// src/container_state.rs

/// 所有 CRDT 容器的统一接口。
/// 实现此 trait 的对象可以被 DocState 存储和管理。
pub trait ContainerState: Debug + Send + Sync {
    /// 应用一个 Op（增量更新）
    fn apply_op(&mut self, op: &Op) -> Result<()>;
    
    /// 应用一个 Diff（批量重建或同步）
    fn apply_diff(&mut self, diff: &Diff) -> Result<()>;
    
    /// 导出当前状态的 Diff（用于编码传输、事件输出、快照）
    fn to_diff(&self) -> Diff;
    
    /// 获取当前状态的值
    fn get_value(&self) -> LoroValue;
    
    /// 克隆自身（用于 fork / 快照）
    fn fork(&self) -> Box<dyn ContainerState>;
    
    /// 获取容器类型
    fn container_type(&self) -> ContainerType;
}
```

**要点**：
- `apply_op` 的参数是 `&Op`（含 container / lamport / id 等完整信息），各实现提取自己关心的部分
- `fork` 返回 `Box<dyn>` 因为不同容器类型大小不同

---

- [ ] ### 3.2 Diff 类型定义

```rust
// src/diff.rs

#[derive(Debug, Clone)]
pub enum Diff {
    Map(MapDiff),
    List(ListDiff),
    Text(TextDiff),
    Tree(TreeDiff),
    Counter(CounterDiff),
}

#[derive(Debug, Clone)]
pub struct CounterDiff {
    pub deltas: HashMap<PeerID, i64>,
}

#[derive(Debug, Clone)]
pub struct MapDiff {
    pub updated: IndexMap<String, Option<LoroValue>>,  // Some = insert/update, None = delete
}

#[derive(Debug, Clone)]
pub struct ListDiff {
    pub inserts: Vec<(usize, Vec<LoroValue>)>,  // (pos, values)
    pub deletes: Vec<(usize, usize)>,            // (pos, len)
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
    pub id: TreeID,
    pub parent: TreeParentID,
    pub deleted: bool,
}
```

---

- [ ] ### 3.3 各 State 的 Diff 实现

为每个 Phase 2 实现的 State 补全：
- `apply_diff`：从 Diff 批量重建状态（用于 checkout / snapshot 导入）
- `to_diff`：从当前状态生成 Diff（用于 export / 事件通知）

**实现顺序**：
- [ ] 3.3.1 CounterState Diff
- [ ] 3.3.2 MapState Diff
- [ ] 3.3.3 ListState Diff
- [ ] 3.3.4 MovableListState Diff
- [ ] 3.3.5 TextState Diff
- [ ] 3.3.6 TreeState Diff

---

- [ ] ### 3.4 容器状态工厂

```rust
// src/container_state.rs

pub fn create_container_state(kind: ContainerType) -> Box<dyn ContainerState> {
    match kind {
        ContainerType::Counter => Box::new(CounterState::new()),
        ContainerType::Map => Box::new(MapState::new()),
        ContainerType::List => Box::new(ListState::new()),
        ContainerType::MovableList => Box::new(ListState::new()), // MovableList 复用 ListState
        ContainerType::Text => Box::new(TextState::new()),
        ContainerType::Tree => Box::new(TreeState::new()),
    }
}
```

---

## Phase 4: 文档运行时

> 在独立 CRDT 之上搭建文档级基础设施：统一 ID 分配、因果追踪、事务、Handler。

---

- [ ] ### 4.1 Arena & ContainerIdx（内存优化层）

```rust
// src/arena.rs

/// 内部紧凑表示，替代臃肿的 ContainerID。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ContainerIdx(u32);

/// 管理 ContainerID ↔ ContainerIdx 的双向映射。
pub struct Arena {
    id_to_idx: HashMap<ContainerID, ContainerIdx>,
    idx_to_id: Vec<ContainerID>,
    parent: Vec<Option<ContainerIdx>>,  // 记录每个容器的父容器
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
- `ContainerIdx` 只有 4 字节，内部全部用 `ContainerIdx`
- 序列化/跨进程传输时才用 `ContainerID`
- `parent` 关系用于事件传播时计算容器路径

---

- [ ] ### 4.2 DAG（有向无环图 / 因果图）

```rust
// src/dag.rs

pub struct Dag<ID> {
    nodes: Vec<DagNode<ID>>,
}

struct DagNode<ID> {
    pub id: ID,
    pub deps: Vec<ID>,
    pub children: Vec<ID>,
}

impl Dag<ID> {
    pub fn iter(&self) -> impl Iterator<Item = &DagNode<ID>>;
    pub fn frontiers_to_vv(&self, frontiers: &Frontiers) -> VersionVector;
    pub fn find_common_ancestor(&self, a: &Frontiers, b: &Frontiers) -> Frontiers;
    pub fn diff_changes(&self, from: &VersionVector, to: &VersionVector) -> Vec<&Change>;
}
```

---

- [ ] ### 4.3 OpLog（操作日志）

```rust
// src/oplog.rs

/// 所有历史变更的存储，与 DocState 分离。
pub struct OpLog {
    changes: Vec<Change>,
    dag: Dag<ID>,
    vv: VersionVector,
    frontiers: Frontiers,
    pending: Vec<Change>,  // 依赖未满足的 Change
}

impl OpLog {
    pub fn import_change(&mut self, change: Change) -> Result<()>;
    pub fn export_changes(&self, from: &VersionVector) -> Vec<&Change>;
    pub fn vv(&self) -> &VersionVector;
    pub fn frontiers(&self) -> &Frontiers;
    pub fn get_changes_between(&self, from: &Frontiers, to: &Frontiers) -> Vec<&Change>;
}
```

**要点**：
- `import_change` 检查 deps 是否满足，不满足则放入 `pending`
- 已存在的 Change 要幂等跳过（用 ID 去重）

---

- [ ] ### 4.4 DocState（所有容器状态的集合）

```rust
// src/doc_state.rs

/// 当前文档的所有容器状态的集合。
pub struct DocState {
    states: HashMap<ContainerIdx, Box<dyn ContainerState>>,
    arena: Arena,
}

impl DocState {
    /// 应用一个 Op（增量更新）
    pub fn apply_op(&mut self, op: &Op) -> Result<()> {
        let idx = self.arena.get_idx(&op.container).ok_or(...)?;
        let state = self.get_or_create(idx, op.container.container_type());
        state.apply_op(op)
    }
    
    /// 应用一个 Diff（批量重建）
    pub fn apply_diff(&mut self, idx: ContainerIdx, diff: &Diff) -> Result<()>;
    
    /// 获取或创建容器状态
    pub fn get_or_create(&mut self, idx: ContainerIdx, kind: ContainerType) -> &mut dyn ContainerState;
    
    /// 获取整个文档的值
    pub fn get_value(&self) -> LoroValue;
}
```

---

- [ ] ### 4.5 CoralDoc 顶层结构

```rust
// src/doc.rs

/// 用户直接操作的文档对象。
pub struct CoralDoc {
    oplog: OpLog,
    state: DocState,
    arena: Arena,
    peer_id: PeerID,
    counter: Counter,        // 本地 ID 分配器
    pending_ops: Vec<Op>,    // 当前事务中累积的 Op
    in_txn: bool,
}

impl CoralDoc {
    pub fn new() -> Self;
    pub fn with_peer_id(peer_id: PeerID) -> Self;
    
    pub fn get_value(&self) -> LoroValue;
    pub fn frontiers(&self) -> &Frontiers;
    pub fn vv(&self) -> &VersionVector;
    
    /// 导入远程变更
    pub fn import(&mut self, changes: &[Change]) -> Result<()>;
    /// 导出自某版本以来的本地变更
    pub fn export(&self, from: &VersionVector) -> Vec<Change>;
    
    /// 事务控制
    pub fn start_txn(&mut self);
    pub fn commit_txn(&mut self) -> Result<()>;
    pub fn abort_txn(&mut self);
    
    /// 内部：生成下一个 Op ID
    pub(crate) fn next_id(&mut self) -> ID;
    /// 内部：提交单个 Op（由 Handler 调用）
    pub(crate) fn submit_op(&mut self, container: ContainerIdx, content: OpContent);
}
```

---

- [ ] ### 4.6 Transaction（本地事务提交）

```rust
// src/txn.rs

pub struct Transaction<'a> {
    doc: &'a mut CoralDoc,
}

impl<'a> Transaction<'a> {
    pub fn new(doc: &'a mut CoralDoc) -> Self;
    pub fn commit(self) -> Result<()>;
    pub fn abort(self);
}
```

**提交逻辑**：
1. `lamport = max(所有已知 Change 的 lamport) + 1`
2. `deps = 当前 frontiers`
3. 构造 `Change { id, lamport, timestamp, deps, ops }`
4. 写入 OpLog
5. 逐个 apply 到 DocState
6. 清空 `pending_ops`

---

- [ ] ### 4.7 Handler（用户 API 层）

Handler 是用户直接接触的面相对象 API，内部委托给 CoralDoc。

#### 4.7.1 CounterHandler

```rust
pub struct CounterHandler<'a> {
    doc: &'a mut CoralDoc,
    container_id: ContainerIdx,
}

impl<'a> CounterHandler<'a> {
    pub fn increment(&mut self, delta: i64);
    pub fn get_value(&self) -> i64;
}
```

#### 4.7.2 MapHandler

```rust
pub struct MapHandler<'a> { ... }

impl<'a> MapHandler<'a> {
    pub fn insert(&mut self, key: &str, value: impl Into<LoroValue>);
    pub fn delete(&mut self, key: &str);
    pub fn get(&self, key: &str) -> Option<LoroValue>;
    pub fn get_container<T: ContainerHandler>(&mut self, key: &str) -> T; // 返回子容器 Handler
    pub fn keys(&self) -> Vec<String>;
}
```

**注意**：`get_container` 返回类型需要根据运行时 ContainerType 确定，建议先用显式方法（`get_list`, `get_map`）替代泛型。

#### 4.7.3 ListHandler

```rust
pub struct ListHandler<'a> { ... }

impl<'a> ListHandler<'a> {
    pub fn insert(&mut self, pos: usize, value: impl Into<LoroValue>);
    pub fn delete(&mut self, pos: usize, len: usize);
    pub fn get(&self, pos: usize) -> Option<LoroValue>;
    pub fn len(&self) -> usize;
    pub fn get_container<T: ContainerHandler>(&mut self, pos: usize) -> T;
}
```

#### 4.7.4 TextHandler

```rust
pub struct TextHandler<'a> { ... }

impl<'a> TextHandler<'a> {
    pub fn insert(&mut self, pos: usize, text: &str);
    pub fn delete(&mut self, pos: usize, len: usize);
    pub fn to_string(&self) -> String;
    pub fn len(&self) -> usize;        // Unicode 字符数
    pub fn len_utf16(&self) -> usize;  // WASM/前端需要
}
```

#### 4.7.5 TreeHandler

```rust
pub struct TreeHandler<'a> { ... }

impl<'a> TreeHandler<'a> {
    pub fn create(&mut self, parent: Option<TreeID>) -> TreeID;
    pub fn mov(&mut self, target: TreeID, new_parent: Option<TreeID>);
    pub fn delete(&mut self, target: TreeID);
    pub fn children(&self, parent: Option<TreeID>) -> Vec<TreeID>;
    pub fn parent(&self, target: TreeID) -> Option<TreeID>;
    pub fn get_meta(&mut self, target: TreeID) -> MapHandler<'_>;
}
```

---

## Phase 5: 协作、事件与版本控制

> 文档运行时的核心能力：多文档同步、状态变更通知、时间旅行。

---

- [ ] ### 5.1 Merge & Sync（文档合并）

```rust
impl CoralDoc {
    /// 导出自 `from` 版本以来本地产生的所有 Change
    pub fn export(&self, from: &VersionVector) -> Vec<Change> {
        self.oplog.export_changes(from).into_iter().cloned().collect()
    }
    
    /// 导入远程 Change
    pub fn import(&mut self, changes: &[Change]) -> Result<()> {
        for change in changes {
            self.oplog.import_change(change.clone())?;
            for op in &change.ops {
                self.state.apply_op(op)?;
            }
        }
        // 触发事件（见 5.2）
        self.emit_events(changes);
        Ok(())
    }
    
    /// 将 other 的变更合并到 self
    pub fn merge(&mut self, other: &CoralDoc) -> Result<()> {
        let changes = other.export(self.vv());
        self.import(&changes)
    }
}
```

**核心测试**：
```rust
#[test]
fn test_merge_commutative() {
    let mut a = CoralDoc::new();
    let mut b = CoralDoc::new();
    a.get_map("root").insert("key", "A");
    b.get_map("root").insert("key", "B");
    
    let changes_a = a.export(b.vv());
    let changes_b = b.export(a.vv());
    a.import(&changes_b).unwrap();
    b.import(&changes_a).unwrap();
    
    assert_eq!(a.get_value(), b.get_value());
    assert_eq!(a.frontiers(), b.frontiers());
}
```

---

- [ ] ### 5.2 Event / Subscription 系统

> **原 plan 缺失的关键模块**。状态变更时必须通知外部（UI、索引、副作用）。

```rust
// src/event.rs

/// 文档级别的事件
#[derive(Debug, Clone)]
pub struct DocEvent {
    pub path: Vec<PathSegment>,  // 如 ["root", "map", "list", 3]
    pub diff: Diff,              // 状态变化的具体内容
}

pub enum PathSegment {
    Key(String),
    Index(usize),
}

/// 事件订阅回调
pub type EventCallback = Box<dyn Fn(&[DocEvent])>;

impl CoralDoc {
    pub fn subscribe(&mut self, callback: EventCallback) -> SubscriptionId;
    pub fn unsubscribe(&mut self, id: SubscriptionId);
    
    /// import / commit / apply_diff 后调用
    fn emit_events(&mut self, changes: &[Change]) {
        let events = self.compute_events(changes);
        for cb in &self.subscribers {
            cb(&events);
        }
    }
    
    /// 计算变更产生的事件（核心算法）
    fn compute_events(&self, changes: &[Change]) -> Vec<DocEvent> {
        // 1. 遍历 changes 中的所有 Op
        // 2. 用 Arena 的 parent 关系构建路径
        // 3. 用 ContainerState::to_diff 生成 diff
        // 4. 合并同一容器的多个 diff
        // ...
    }
}
```

**要点**：
- 事件在 **Doc 层级**计算，因为需要路径信息和跨容器协调
- `compute_events` 是性能敏感点：后期需要优化（如增量 diff、缓存）
- 时间旅行（checkout）不产生事件，因为不是"变更"而是"替换"

---

- [ ] ### 5.3 Checkout & Time Travel

```rust
impl CoralDoc {
    /// 将 DocState 回滚/前进到指定的 Frontiers 版本
    pub fn checkout(&mut self, target: &Frontiers) -> Result<()> {
        let current = self.oplog.frontiers().clone();
        
        // 简单实现：丢弃当前 state，从空重建
        self.state = DocState::new(self.arena.clone());
        
        let changes = self.oplog.get_changes_between(&Frontiers::default(), target);
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

**优化路径**：
- 初期：每次 checkout 全量重建（慢但正确）
- 后期：增量 checkout（向前/向后 apply diff）、LRU 状态缓存

---

- [ ] ### 5.4 Fork（分支）

```rust
impl CoralDoc {
    pub fn fork(&self) -> CoralDoc {
        CoralDoc {
            oplog: self.oplog.clone(),
            state: self.state.fork(),
            arena: self.arena.clone(),
            peer_id: random_peer_id(),
            pending_ops: vec![],
            in_txn: false,
            // subscribers 不复制（fork 后的事件独立订阅）
            subscribers: vec![],
        }
    }
}
```

---

## Phase 6: 高级与优化

> 在核心功能稳定后，逐步替换和增强。

---

- [ ] ### 6.1 Text 升级 Fugue

> 用 Fugue 算法替换 List-based 简化版。

#### 6.1.1 FugueSpan

```rust
pub struct FugueSpan {
    pub id: ID,
    pub text: String,
    pub deleted: bool,
    pub left_origin: Option<ID>,
    pub right_origin: Option<ID>,
}
```

#### 6.1.2 Fugue 并发排序

```
并发插入同一位置时：
  1. 先按 left_origin 的 (lamport, peer) 排序
  2. 相同 left_origin 时，按自身的 (lamport, peer) 排序
  3. 保证结果是确定性的
```

#### 6.1.3 Rope / BTree 存储

用 `generic-btree` 或自定义 BTree 替换 `HashMap + Vec`，实现 O(log n) 的索引和插入。

---

- [ ] ### 6.2 Rich Text（样式）

> 在 Fugue Text 基础上增加 Mark/Unmark。

```rust
pub enum TextOp {
    Insert { pos: usize, text: String },
    Delete { pos: usize, len: usize },
    Mark { start_id: ID, end_id: ID, key: String, value: LoroValue, lamport: Lamport },
    Unmark { start_id: ID, end_id: ID, key: String, lamport: Lamport },
}
```

样式存储：每种 key 对应一个 `RangeMap<ID, StyleValue>`，冲突用 LWW 解决。

输出 Quill Delta 格式：
```rust
pub fn to_delta(&self) -> Vec<TextDelta>;
```

---

- [ ] ### 6.3 GC & Tombstone 清理

> 长期运行后，List/Text/Map 会积累大量 tombstone，需要清理。

```rust
impl CoralDoc {
    /// 清理所有 peers 都确认接收到的 tombstone
    pub fn gc(&mut self, horizon: &VersionVector) {
        // 1. 遍历所有 ContainerState
        // 2. 对每个 State 调用 reset_remove(horizon)
        // 3. 从 OpLog 中删除 horizon 之前的 Change（可选，影响 checkout 能力）
    }
}
```

各 State 需要实现 `ResetRemove`：
```rust
pub trait ResetRemove {
    /// 删除所有被 `horizon` 覆盖的元数据（tombstone、旧版本信息等）
    fn reset_remove(&mut self, horizon: &VersionVector);
}
```

**权衡**：
- GC 后无法 checkout 到 GC 点之前的版本（除非保留快照）
- 可选策略：定期 snapshot + 清理旧 OpLog

---

- [ ] ### 6.4 编码与传输

```rust
pub trait Encode {
    fn encode(&self) -> Vec<u8>;
    fn decode(data: &[u8]) -> Result<Self>;
}

impl Encode for Change { ... }
impl Encode for Snapshot { ... }
```

初期用 `serde + postcard/bincode`，后期考虑列式编码（参考 Loro 的 `serde_columnar`）。

---

- [ ] ### 6.5 性能优化

| 优化项 | 当前实现 | 目标 |
|--------|----------|------|
| List 索引 | `Vec<ID>` O(n) | Rope / BTree O(log n) |
| Text 存储 | List-based / FugueSpan Vec | generic-btree Rope |
| MovableList 排序 | 全量重建 O(n log n) | FractionalIndex + BTree |
| Tree 兄弟排序 | `Vec` O(n) | FractionalIndex + BTree |
| Event 计算 | 全量 diff | 增量 diff + 缓存 |
| State fork | 深克隆 | `im` 不可变数据结构共享 |

---

## 数据结构修正备忘

| 位置 | 原计划 | 修正 |
|------|--------|------|
| ListState.elements | `BTreeMap<ID, ListElement>` + 双向链表 | `HashMap<ID, ListElement>` + `Vec<ID>` 维护可见顺序 |
| List 顺序 | ID 字典序 | 由 `left_origin` + `(lamport, peer)` 排序决定 |
| Counter 类型 | G-Counter | PN-Counter（可正负） |
| Text 实现 | 直接 Fugue | 先做 List-based 简化版验证架构，再替换 |
| Tree | 仅节点结构 | 补充每个节点的隐式 metadata Map（Phase 4.7.5） |
| Op.container | ContainerID | 内部用 ContainerIdx，API 层用 ContainerID |
| MovableList | 双向链表维护 | `Vec<ID>` 全量重建（初期），后期 FractionalIndex |

---

## 阶段验收标准

> 每个 Phase 完成后，必须跑通对应的验收测试才能进入下一阶段。
> 测试代码应尽量用 `#[test]` 写在对应模块的 `mod tests` 中。

---

### Phase 1 验收：基础类型

**判定标准**：全部编译通过，基础运算正确。

**必备用例**：
- ID 排序：同 peer 比 counter，不同 peer 仍按 counter 优先
- VersionVector 包含关系：a >= b 时 a 包含 b 的所有变更
- VersionVector 合并：取各 peer 最大值
- Frontiers 与 VV 转换：给定 DAG 能从 Frontiers 计算完整 VV，反之亦然
- Span 区间运算：包含、交集、合并连续区间

---

### Phase 2.1 验收：CounterState

**判定标准**：`cargo test counter` 全部通过。

**必备用例**：
- 单操作正确性：apply +5 再 apply -2，value 为 3
- 幂等性：同一 Op（相同 ID）apply 两次，结果不变（需配合上层去重或自身去重）
- 交换律：A 先加 3、B 先减 2，A.merge(B) 与 B.merge(A) 结果相同
- 结合律：(A.merge(B)).merge(C) 与 A.merge(B.merge(C)) 结果相同
- 并发合并：A 加 3、B 减 2，合并后值为 1

---

### Phase 2.2 验收：LWWRegister

**判定标准**：LWW 语义和 tie-break 正确。

**必备用例**：
- Lamport 大的覆盖小的：先写 lamport=5，再写 lamport=10，最终值为后者
- Tie-break：lamport 相同时 peer ID 大的胜出
- 删除参与 LWW：写入后删除（更高 lamport），最终值为 None
- 合并交换律：两个 Register merge 结果与顺序无关
- 重复写入同一 marker：相同 lamport+peer 写入相同值应无冲突；写入不同值需处理或拒绝

---

### Phase 2.3 验收：MapState

**判定标准**：Map 操作 + 并发 key 冲突 + merge 律正确。

**必备用例**：
- 插入/读取/删除：insert key 后 get 到值，delete 后 get 为 None
- 遍历顺序：IndexMap 保证插入序
- 并发写入同一 key：不同 peer 同时写同一 key，LWW（lamport/peer）决定胜出
- 并发删除与写入：删除和写入并发，LWW 决定最终可见性
- 合并交换律：两个 MapState 各自有不同 key，merge 后包含所有 key
- 合并结合律：三个 MapState merge 顺序不影响结果
- Tombstone 保留：删除后内部仍保留 Register，支持后续并发插入的 LWW 比较

---

### Phase 2.4 验收：ListState（RGA）

**判定标准**：顺序操作、并发插入排序、merge 一致性。

**必备用例**：
- 顺序插入：A→B→C 插入后，遍历顺序为 A,B,C
- 中间插入：在 A 后插入 B，在 B 后插入 C，顺序仍为 A,B,C
- 删除：删除 B 后，遍历为 A,C；len 为 2
- Tombstone：删除后元素仍在 elements 中，但不在 visible_order 中
- 并发插入同一位置：A 和 B 同时插在 root 后，按 (lamport, peer) 确定顺序；a.merge(b) 与 b.merge(a) 结果必须一致
- 并发删除与插入：删除 A 的同时在 A 后插入 B，B 应正常可见
- 合并交换律：两个 ListState merge 后元素集合和顺序一致
- 合并结合律：三个 ListState merge 顺序不影响结果

---

### Phase 2.5-2.7 验收：MovableList、Text、Tree

**判定标准**：

**MovableList**：
- Move 后元素位置正确
- 并发 move 同一元素：LWW（lamport/peer）决定最终位置
- Move 后删除：被移动的元素删除后不再可见
- 合并交换律/结合律

**Text（简化版）**：
- 复用 ListState 的测试逻辑（每个字符是一个元素）
- insert(pos, "hello") 后 to_string 正确
- delete(pos, len) 后 to_string 正确
- 中文字符/emoji 的 Unicode 处理正确（长度按码点计）

**Tree**：
- Create：创建 root 节点、创建子节点
- Move：移动节点到另一 parent 下
- Delete：删除节点后 children 遍历不再包含
- 循环检测：把祖先节点移到后代下应被拒绝
- 并发 move：LWW 决定最终 parent
- Children 遍历：返回指定 parent 下的所有非删除子节点
- 合并交换律：两个 TreeState merge 后结构一致

---

### Phase 3 验收：ContainerState 统一接口

**判定标准**：所有 Phase 2 的 State 都能被统一接口管理，Diff 往返一致。

**必备用例**：
- 工厂函数：`create_container_state(Counter)` 返回 CounterState 实例
- 类型识别：`container_type()` 返回正确的 ContainerType
- Diff 往返：State A → to_diff → 新建 State B → apply_diff → A.get_value() == B.get_value()
- 动态分发：`HashMap<u32, Box<dyn ContainerState>>` 中存取不同 State，统一调用 apply_op
- Fork 克隆：`Box<dyn ContainerState>.fork()` 产生独立副本，修改互不影响
- Diff 一致性：同一 State 两次调用 to_diff 结果相同

---

### Phase 4 验收：文档运行时

**判定标准**：端到端 `CoralDoc` 能创建、编辑、读取。

**必备用例**：
- Arena：ContainerID 注册后能通过 Idx 反查，parent 关系正确
- OpLog 导入导出：导入 Change 后 VV 和 Frontiers 正确；导出的 Change 能被另一 OpLog 导入
- OpLog 乱序处理：Change B（依赖 A）先到达时进入 pending，A 到达后级联应用 B
- DocState apply_op：Op 被路由到正确的 ContainerState
- Counter E2E：通过 Handler increment → commit → get_value 正确
- Map E2E：insert/delete/get 通过 Handler 工作
- List E2E：insert/delete/len 通过 Handler 工作
- 嵌套容器：Map 中创建子 List，子 List push 元素后整体 get_value 结构正确
- Transaction commit：多 Op 事务提交后全部生效
- Transaction abort：abort 后所有 Op 未生效，State 回滚到事务前
- 自动提交：未显式开启事务时，单次操作自动 commit

---

### Phase 5 验收：协作、事件与版本控制

**判定标准**：Merge 一致性、Event 触发、Checkout/Fork 正确。

**必备用例**：
- Export：导出自某 VV 以来的 Change 列表完整且无冗余
- Import：导入远程 Change 后 State 正确更新
- Merge 交换律：A import B，B import A，最终 value 和 frontiers 一致
- Merge 结合律：A import B 再 import C，与 A import (B+C) 结果一致
- 幂等性：重复 import 同一批 Change，State 不变
- 事件触发：本地 commit 后触发事件；import 远程变更后触发事件
- 事件内容：事件包含正确的 path 和 diff
- 事件去重：同一变更不重复触发事件
- Checkout：回滚到历史版本后 get_value 正确
- Checkout 前进：从旧版本 checkout 到新版本后 State 正确
- Fork：fork 后的文档独立编辑不影响原文档
- Fork 后 Merge：fork 的修改 merge 回原文档后正确合并

---

### Phase 6 验收：高级与优化

**判定标准**：升级替换后原有测试仍通过，新增功能可用。

**必备用例**：
- Fugue 兼容性：用 Fugue 替换 List-based Text 后，Phase 2-5 的所有 Text 测试仍通过
- Fugue 并发插入：长文本块并发插入同一位置，顺序正确且一致
- GC 状态保持：GC 前后 get_value() 结果相同
- GC 后 Tombstone 减少：elements 数量减少，内存下降
- Rich Text Delta：insert + mark 后 to_delta 输出 Quill Delta 格式正确
- Rich Text 样式冲突：同一区间并发 mark/unmark，LWW 决定最终样式
- 编码往返：Change/Snapshot encode → decode 后与原值相等
- 性能基准：大规模文档（10k+ Op）的 apply、merge、get_value 在可接受时间内完成
```

---

## 测试策略

### 分层测试

| 层级 | 测试对象 | 验证目标 |
|------|----------|----------|
| 单元测试 | 单个 State（Counter/Map/List/...） | 幂等性、交换律、结合律、并发冲突 |
| 集成测试 | DocState + Arena + ContainerState | 多容器 apply、fork、to_diff |
| 端到端测试 | CoralDoc + Handler | commit、import、merge、checkout |
| 属性测试 | 随机操作序列 | proptest 验证不变量 |
| 模糊测试 | 多 peer 并发随机编辑 | cargo fuzz |

### 每个 CRDT 的基础测试清单

- [ ] 单操作正确性
- [ ] 幂等性（同一 Op apply 两次）
- [ ] 交换律（A.merge(B) == B.merge(A)）
- [ ] 结合律（(A^B)^C == A^(B^C)）
- [ ] 并发冲突（两个 peer 同时操作同一位置）

### 跨阶段集成测试

```rust
fn fuzz_two_peers() {
    let mut peer_a = CoralDoc::new();
    let mut peer_b = CoralDoc::new();
    
    for _ in 0..100 {
        random_op(&mut peer_a);
        random_op(&mut peer_b);
    }
    
    let to_b = peer_a.export(peer_b.vv());
    let to_a = peer_b.export(peer_a.vv());
    peer_a.import(&to_a).unwrap();
    peer_b.import(&to_b).unwrap();
    
    assert_eq!(peer_a.get_value(), peer_b.get_value());
    assert_eq!(peer_a.frontiers(), peer_b.frontiers());
}
```

### 关键不变量检查清单

- [ ] OpLog 的 DAG 始终无环
- [ ] VersionVector 单调性：import 新 Change 后只增不减
- [ ] Frontiers 确定性：相同 Frontiers 对应相同 State
- [ ] Tombstone 不泄漏：get_value 过滤已删除元素
- [ ] Tree 无环：任何时刻 parent 关系无循环
- [ ] 事件一致性：apply 后产生的事件与 to_diff 结果一致

---

## 依赖库建议

| 库 | 用途 | 阶段 |
|----|------|------|
| `indexmap` | `LoroValue::Map` 保序存储 | Phase 1 |
| `serde` + `serde_json` | 序列化/JSON 输出 | Phase 1 |
| `thiserror` | 错误类型定义 | Phase 4 |
| `proptest` | 随机属性测试 | Phase 2+ |
| `fractional_index` | Tree/MovableList 兄弟排序 | Phase 6 |
| `im` | 不可变数据结构（fork 共享） | Phase 6 |
| `generic-btree` | Rope / 高性能序列存储 | Phase 6 |

---

## 统计

| 指标 | 数量 |
|------|------|
| 已完成 | 10 |
| 未完成 | 约 45 |
| 总数 | 约 55 |
| 完成百分比 | ~18% |
