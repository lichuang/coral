# Loro CRDT 完整复刻实现路线图

> 本文件记录从 0 开始完整复刻 Loro CRDT 核心代码的详细步骤。
> 每一步都必须先标记为 `- [ ]` 未完成状态，实现完成后再改为 `- [x]`。
> 实现顺序严格遵循自底向上的依赖关系，不可随意跳过。

---

## Phase 0: 项目搭建与工具链配置

**目标**: 建立可编译的 Rust 项目骨架，配置必要的开发工具和依赖。

- [ ] **0.1 创建项目结构**
  - [ ] 0.1.1 初始化 Cargo 项目 `coral`，设置 `crate-type = ["lib"]`
  - [ ] 0.1.2 创建目录结构：`src/core/`、`src/types/`、`src/op/`、`src/state/`、`src/handler/`、`src/version/`、`tests/`
  - [ ] 0.1.3 配置 `Cargo.toml` 基础依赖：`serde`、`thiserror`、`indexmap`、`rustc-hash`
  - [ ] 0.1.4 配置开发依赖：`proptest`
  - [ ] 0.1.5 配置 `rustfmt.toml` 和 `rust-toolchain.toml`

- [ ] **0.2 配置 CI 质量检查脚本**
  - [ ] 0.2.1 确保能运行 `cargo fmt --check`
  - [ ] 0.2.2 确保能运行 `cargo clippy -- -D warnings`
  - [ ] 0.2.3 确保能运行 `cargo test`

- [ ] **0.3 验证空项目编译通过**
  - [ ] 0.3.1 `cargo build` 成功
  - [ ] 0.3.2 `cargo test` 成功（空测试集）

---

## Phase 1: 基础类型系统

**目标**: 实现所有无业务逻辑的纯数据类型，这是整个系统的地基。
**验收标准**: 所有类型可正确创建、比较、序列化/反序列化。

- [ ] **1.1 ID 类型族**
  - [ ] 1.1.1 定义 `PeerID = u64`、`Counter = i32`、`Lamport = u32`
  - [ ] 1.1.2 定义 `ID { peer: PeerID, counter: Counter }`
  - [ ] 1.1.3 为 `ID` 实现 `Debug`、`Display`（格式 `{counter}@{peer}`）、`PartialOrd`、`Ord`
  - [ ] 1.1.4 为 `ID` 实现 `TryFrom<&str>` 和 `to_bytes` / `from_bytes`
  - [ ] 1.1.5 定义 `IdLp { lamport: Lamport, peer: PeerID }`
  - [ ] 1.1.6 定义 `IdFull { peer: PeerID, lamport: Lamport, counter: Counter }`
  - [ ] 1.1.7 实现 `IdFull::id()` → `ID` 和 `IdFull::idlp()` → `IdLp`
  - [ ] 1.1.8 实现 `ID` 的辅助方法：`new`、`to_span(len)`、`inc`、`is_connected_id`

- [ ] **1.2 Span 类型**
  - [ ] 1.2.1 定义 `CounterSpan { start: Counter, end: Counter }`，支持反向表示（`start > end`）
  - [ ] 1.2.2 为 `CounterSpan` 实现 `HasLength`、`Sliceable`、`Mergable`
  - [ ] 1.2.3 定义 `IdSpan { peer: PeerID, counter: CounterSpan }`
  - [ ] 1.2.4 为 `IdSpan` 实现 `HasLength`、`Sliceable`、`Mergable`
  - [ ] 1.2.5 实现 `IdSpan` 的方法：`contains`、`is_reversed`、`normalize_`、`get_intersection`
  - [ ] 1.2.6 定义 `LamportSpan { start: Lamport, end: Lamport }`

- [ ] **1.3 ContainerID 与 ContainerType**
  - [ ] 1.3.1 定义枚举 `ContainerID::Root { name, container_type }` 和 `ContainerID::Normal { peer, counter, container_type }`
  - [ ] 1.3.2 定义枚举 `ContainerType { Text, Map, List, MovableList, Tree, Counter, Unknown(u8) }`
  - [ ] 1.3.3 为 `ContainerType` 实现 `to_u8` / `try_from_u8`
  - [ ] 1.3.4 为 `ContainerType` 实现 `default_value()`
  - [ ] 1.3.5 为 `ContainerID` 实现 `Display`（格式 `cid:root-name:Map` 或 `cid:10@255:Map`）
  - [ ] 1.3.6 为 `ContainerID` 实现 `TryFrom<&str>`
  - [ ] 1.3.7 实现 `ContainerID::new_normal(id, container_type)` 和 `ContainerID::new_root(name, container_type)`
  - [ ] 1.3.8 实现 `ContainerID::container_type()` 和 `ContainerID::name()`
  - [ ] 1.3.9 实现 `ContainerID::encode` / `to_bytes` / `from_bytes`

- [ ] **1.4 TreeID**
  - [ ] 1.4.1 定义 `TreeID { peer: PeerID, counter: Counter }`
  - [ ] 1.4.2 定义常量 `DELETED_TREE_ROOT: TreeID`
  - [ ] 1.4.3 实现 `TreeID::new`、`TreeID::from_id`、`TreeID::id`、`TreeID::delete_root`、`TreeID::is_deleted_root`
  - [ ] 1.4.4 实现 `TreeID::associated_meta_container()` → `ContainerID`（类型为 Map）

- [ ] **1.5 LoroValue**
  - [ ] 1.5.1 定义 `LoroValue` 枚举：Null, Bool, Double, I64, Binary, String, List, Map, Container(ContainerID)
  - [ ] 1.5.2 定义包装类型：`LoroBinaryValue(Arc<Vec<u8>>)`、`LoroStringValue(Arc<String>)`、`LoroListValue(Arc<Vec<LoroValue>>)`、`LoroMapValue(Arc<FxHashMap<String, LoroValue>>)`
  - [ ] 1.5.3 为所有包装类型实现 `Clone`、`Deref`、`AsRef`、`FromIterator`
  - [ ] 1.5.4 为 `LoroValue` 实现 `Hash`、`Eq`、`PartialEq`
  - [ ] 1.5.5 实现 `LoroValue` 的 `Serialize` / `Deserialize`（human-readable vs binary 两种模式）
  - [ ] 1.5.6 实现 `LoroValue::get_by_key`、`get_by_index`、`is_empty_collection`、`get_depth`
  - [ ] 1.5.7 实现 `Index<&str>` 和 `Index<usize>` for `LoroValue`
  - [ ] 1.5.8 实现 `loro_value!` 宏（类似 `serde_json::json!`）

- [ ] **1.6 InternalString**
  - [ ] 1.6.1 定义 `InternalString`（可用 `Arc<str>` 或小型字符串优化版）
  - [ ] 1.6.2 实现 `Clone`、`Debug`、`Hash`、`PartialEq`、`Eq`、`AsRef<str>`、`Deref<Target=str>`、`From<&str>`

- [ ] **1.7 错误类型**
  - [ ] 1.7.1 使用 `thiserror` 定义 `CoralError` 枚举
  - [ ] 1.7.2 包含至少以下变体：`ContainerNotFound`、`InvalidPosition`、`TypeMismatch`、`MissingDependency(ID)`、`OutOfBound`、`DecodeError`、`LockError`
  - [ ] 1.7.3 定义类型别名 `CoralResult<T> = Result<T, CoralError>`

- [ ] **1.8 Phase 1 测试**
  - [ ] 1.8.1 测试 `ID` 的创建、比较、序列化、反序列化
  - [ ] 1.8.2 测试 `ContainerID` 的字符串转换和字节编码
  - [ ] 1.8.3 测试 `CounterSpan` 的合并、切片、交集
  - [ ] 1.8.4 测试 `IdSpan` 的合并、切片、交集
  - [ ] 1.8.5 测试 `LoroValue` 的嵌套、深度计算、索引访问
  - [ ] 1.8.6 运行 `cargo fmt --check`、`cargo clippy -- -D warnings`、`cargo test`，全部通过

---

## Phase 2: RLE（Run-Length Encoding）基础

**目标**: 实现 RLE 向量，为 Change 中的 ops 压缩存储提供基础。
**验收标准**: `RleVec<T>` 能正确 push、合并、切片、按原子索引访问。

- [ ] **2.1 RLE Trait 定义**
  - [ ] 2.1.1 定义 `HasLength { fn atom_len(&self) -> usize; fn rle_len(&self) -> usize; }`
  - [ ] 2.1.2 定义 `Sliceable { fn slice(&self, from: usize, to: usize) -> Self; }`
  - [ ] 2.1.3 定义 `Mergable { fn is_mergable(&self, other: &Self) -> bool; fn merge(&mut self, other: &Self); }`
  - [ ] 2.1.4 定义 `RlePush { fn push(&mut self, value: Self::Item); }`
  - [ ] 2.1.5 定义辅助结构 `Slice<'a, T>` 和 `SearchResult<'a, T, I>`

- [ ] **2.2 RleVec 实现**
  - [ ] 2.2.1 定义 `RleVec<T: HasLength + Mergable>`，内部用 `Vec<T>` 存储
  - [ ] 2.2.2 实现 `push(value)`：尝试与最后一个元素合并，否则追加
  - [ ] 2.2.3 实现 `len()` → 原子元素总数
  - [ ] 2.2.4 实现 `get(index)` → 按原子索引查找元素（二分搜索）
  - [ ] 2.2.5 实现 `slice(from, to)` → 返回新的 `RleVec`（可能分割边界元素）
  - [ ] 2.2.6 实现 `iter()` 和 `SliceIterator`

- [ ] **2.3 RleVec 为常用类型实现 Trait**
  - [ ] 2.3.1 为 `CounterSpan` 实现 `HasLength`、`Sliceable`、`Mergable`
  - [ ] 2.3.2 为 `IdSpan` 实现 `HasLength`、`Sliceable`、`Mergable`

- [ ] **2.4 Phase 2 测试**
  - [ ] 2.4.1 测试 `RleVec<CounterSpan>` 的合并行为（`[0..5] + [5..10] → [0..10]`）
  - [ ] 2.4.2 测试 `RleVec<CounterSpan>` 的切片（切割中间元素）
  - [ ] 2.4.3 测试 `RleVec<CounterSpan>` 的原子索引查找
  - [ ] 2.4.4 测试 `RleVec<IdSpan>` 的合并和切片
  - [ ] 2.4.5 运行 fmt、clippy、test，全部通过

---

## Phase 3: VersionVector 与 Frontiers

**目标**: 实现版本向量和前沿集合，这是 CRDT 合并与 checkout 的基础。
**验收标准**: VV 和 Frontiers 能正确 diff、merge、转换。

- [ ] **3.1 VersionVector**
  - [ ] 3.1.1 定义 `VersionVector(FxHashMap<PeerID, Counter>)`
  - [ ] 3.1.2 实现 `get(peer) -> Counter`、`set(peer, counter)`
  - [ ] 3.1.3 实现 `partial_cmp(other)` → 因果序（`Less`/`Equal`/`Greater`/`None`）
  - [ ] 3.1.4 实现 `merge(other)` → 逐 peer 取最大值
  - [ ] 3.1.5 实现 `diff(other)` → `VersionVectorDiff { retreat, forward }`
  - [ ] 3.1.6 实现 `sub_iter(other)` → 迭代 self 有但 other 没有的 `IdSpan`
  - [ ] 3.1.7 实现 `get_frontiers()` → 从 VV 生成 `Frontiers`（取每个 peer 的最后一个 ID）
  - [ ] 3.1.8 实现 `encode` / `decode`（使用 `postcard` 或自定义）

- [ ] **3.2 Frontiers**
  - [ ] 3.2.1 定义 `Frontiers` 枚举：`None`、`ID(ID)`、`Map(Arc<FxHashMap<PeerID, Counter>>)`
  - [ ] 3.2.2 实现 `push(id)`：添加/更新 ID（同 peer 取最大 counter，多 peer 时提升为 Map）
  - [ ] 3.2.3 实现 `remove(id)` / `retain(f)`：支持过滤和自动降级（Map→ID→None）
  - [ ] 3.2.4 实现 `update_frontiers_on_new_change(id, deps)`：核心 DAG 前沿更新（移除 deps，添加新 ID）
  - [ ] 3.2.5 实现 `merge_with_greater(other)`：合并另一个 frontiers，逐 peer 取最大
  - [ ] 3.2.6 实现 `as_single()` → `Option<ID>`
  - [ ] 3.2.7 实现 `encode` / `decode`

- [ ] **3.3 版本范围**
  - [ ] 3.3.1 定义 `VersionRange(FxHashMap<PeerID, (Counter, Counter)>)`（闭区间）
  - [ ] 3.3.2 定义 `IdSpanVector = FxHashMap<PeerID, CounterSpan>`

- [ ] **3.4 Phase 3 测试**
  - [ ] 3.4.1 测试 `VersionVector::partial_cmp`（线性历史、并发历史、相等）
  - [ ] 3.4.2 测试 `VersionVector::merge` 的交换律和结合律
  - [ ] 3.4.3 测试 `VersionVector::diff` 的对称性
  - [ ] 3.4.4 测试 `Frontiers::push` 的自动升级/降级
  - [ ] 3.4.5 测试 `Frontiers::update_frontiers_on_new_change`
  - [ ] 3.4.6 测试 VV ↔ Frontiers 双向转换的一致性
  - [ ] 3.4.7 运行 fmt、clippy、test，全部通过

---

## Phase 4: DAG（因果图）

**目标**: 实现有向无环图，支持变更的因果依赖、LCA 查找和拓扑遍历。
**验收标准**: 能正确构建 DAG、检测环、找 LCA、遍历祖先。

- [ ] **4.1 DAG Node 与 Trait**
  - [ ] 4.1.1 定义 `DagNode` trait：`fn deps(&self) -> &Frontiers`、`fn lamport(&self) -> Lamport`、`fn id_start(&self) -> ID`、`fn len(&self) -> usize`
  - [ ] 4.1.2 定义 `Dag` trait：`fn get(&self, id: ID) -> Option<&Node>`、`fn frontier(&self) -> &Frontiers`、`fn vv(&self) -> &VersionVector`、`fn contains(&self, id: ID) -> bool`

- [ ] **4.2 DAG 节点实现**
  - [ ] 4.2.1 定义 `DagNodeInner`：`peer`、`cnt`（起始 counter）、`lamport`、`deps: Frontiers`、`len: usize`、`has_succ: bool`
  - [ ] 4.2.2 定义 `AppDagNode`：包装 `DagNodeInner`，支持 lazy VV 计算（`OnceCell<VersionVector>`）

- [ ] **4.3 AppDag 实现**
  - [ ] 4.3.1 定义 `AppDag`：`map: BTreeMap<ID, AppDagNode>`、`frontiers: Frontiers`、`vv: VersionVector`
  - [ ] 4.3.2 实现 `handle_new_change(change, from_local)`：插入新节点，尝试与前一个 peer 的节点合并
  - [ ] 4.3.3 实现合并条件判断：连续 counter、相同 deps、无后继（`has_succ`）
  - [ ] 4.3.4 实现 `update_version_on_new_local_op`：维护 `pending_txn_node` 和 vv/frontiers
  - [ ] 4.3.5 实现 `get_version_vector(node_id)`：递归遍历 deps 计算 VV（带缓存）
  - [ ] 4.3.6 实现 `frontiers_to_vv(frontiers)` 和 `vv_to_frontiers(vv)`

- [ ] **4.4 LCA 算法**
  - [ ] 4.4.1 实现 `find_common_ancestor(a_id, b_id)`：使用优先队列（按 lamport 降序）从两个前沿同时回溯
  - [ ] 4.4.2 返回 LCA 的 `Frontiers` 和 `DiffMode`
  - [ ] 4.4.3 实现优化路径：单 peer 线性历史直接比较 counter
  - [ ] 4.4.4 实现 `remove_included_frontiers(vv, deps)`：从 VV 中移除被 deps 祖先包含的条目

- [ ] **4.5 DAG 遍历**
  - [ ] 4.5.1 实现 `travel_ancestors(id, f)`：从指定 ID 反向遍历祖先（lamport 降序）
  - [ ] 4.5.2 实现 `iter_causal()`：按因果序遍历所有节点
  - [ ] 4.5.3 实现 `iter()`：按 lamport 序遍历

- [ ] **4.6 Phase 4 测试**
  - [ ] 4.6.1 测试线性历史的 DAG 构建和 vv 计算
  - [ ] 4.6.2 测试分叉/合并历史的 DAG 构建
  - [ ] 4.6.3 测试 LCA：两个分支的共同祖先
  - [ ] 4.6.4 测试 LCA：其中一个分支是另一个的祖先
  - [ ] 4.6.5 测试 `frontiers_to_vv` 和 `vv_to_frontiers` 的互逆性
  - [ ] 4.6.6 测试 `travel_ancestors` 的遍历顺序正确性
  - [ ] 4.6.7 运行 fmt、clippy、test，全部通过

---

## Phase 5: Arena（容器索引系统）

**目标**: 实现 ContainerID 到紧凑 ContainerIdx 的映射，管理容器父子关系。
**验收标准**: Arena 能正确分配索引、双向查找、管理父子关系、分配值和字符串。

- [ ] **5.1 ContainerIdx**
  - [ ] 5.1.1 定义 `ContainerIdx(u32)`：高 5 位编码 `ContainerType`，低 27 位编码索引
  - [ ] 5.1.2 实现 `ContainerIdx::new(idx, container_type)`
  - [ ] 5.1.3 实现 `ContainerIdx::container_type()` 和 `ContainerIdx::to_u32()`

- [ ] **5.2 Arena 核心**
  - [ ] 5.2.1 定义 `Arena`：
    - `container_idx_to_id: Vec<ContainerID>`
    - `container_id_to_idx: FxHashMap<ContainerID, ContainerIdx>`
    - `depth: Vec<Option<NonZeroU16>>`
    - `parents: FxHashMap<ContainerIdx, Option<ContainerIdx>>`
  - [ ] 5.2.2 实现 `register_container(id) -> ContainerIdx`：幂等分配
  - [ ] 5.2.3 实现 `id_to_idx(id)` 和 `idx_to_id(idx)`
  - [ ] 5.2.4 实现 `set_parent(child, parent)` 和 `get_parent(child)`
  - [ ] 5.2.5 实现 `get_path_to_root(child)` → `Vec<ContainerIdx>`

- [ ] **5.3 值与字符串 Arena**
  - [ ] 5.3.1 定义 `values: Vec<LoroValue>`，实现 `alloc_value(value) -> usize`
  - [ ] 5.3.2 定义字符串存储（可用 `Vec<String>` 或 `StringArena`），实现 `alloc_str(str) -> StrAllocResult`
  - [ ] 5.3.3 实现 `get_value(idx)` 和 `get_str(result)`

- [ ] **5.4 SharedArena**
  - [ ] 5.4.1 定义 `SharedArena(Arc<Mutex<Arena>>)` 或内部可变性包装
  - [ ] 5.4.2 实现 `fork()`：深拷贝 Arena 状态（用于文档 fork）

- [ ] **5.5 Phase 5 测试**
  - [ ] 5.5.1 测试 `ContainerIdx` 的位编码/解码
  - [ ] 5.5.2 测试 Arena 的 ID↔Idx 双向映射
  - [ ] 5.5.3 测试父子关系设置和查询
  - [ ] 5.5.4 测试路径计算（从子到根）
  - [ ] 5.5.5 测试值和字符串的分配与读取
  - [ ] 5.5.6 测试 `Arena::fork()` 的独立性
  - [ ] 5.5.7 运行 fmt、clippy、test，全部通过

---

## Phase 6: Change 与 Op 定义

**目标**: 定义变更（事务边界）和操作（原子修改）的数据结构。
**验收标准**: Change 和 Op 能正确序列化、切片、实现 DagNode trait。

- [ ] **6.1 Op 核心**
  - [ ] 6.1.1 定义 `Op { counter: Counter, container: ContainerIdx, content: InnerContent }`
  - [ ] 6.1.2 为 `Op` 实现 `HasLength`（`atom_len = content_len`）

- [ ] **6.2 OpContent**
  - [ ] 6.2.1 定义 `InnerContent` 枚举：
    - `List(InnerListOp)`
    - `Map(MapSet)`
    - `Tree(Arc<TreeOp>)`
    - `Future(FutureInnerContent)`
  - [ ] 6.2.2 定义 `RawOpContent<'a>` 枚举（序列化/传输用）：
    - `Map(MapSet)`
    - `List(ListOp<'a>)`
    - `Tree(Arc<TreeOp>)`
    - `Counter(f64)`
    - `Unknown { prop, value }`
  - [ ] 6.2.3 定义 `MapSet { key: InternalString, value: Option<LoroValue> }`（`None` = 逻辑删除）
  - [ ] 6.2.4 定义 `ListOp<'a>`：
    - `Insert { slice: ListSlice<'a>, pos: usize }`
    - `Delete(DeleteSpanWithId)`
    - `Move { from: u32, to: u32, elem_id: IdLp }`
    - `Set { elem_id: IdLp, value: LoroValue }`
    - `StyleStart { start, end, key, info, value }`
    - `StyleEnd`
  - [ ] 6.2.5 定义 `InnerListOp`（arena 解析后的版本）：
    - `Insert { slice: SliceRange, pos: usize }`
    - `InsertText { slice: BytesSlice, unicode_start, unicode_len, pos }`
    - `Delete(DeleteSpanWithId)`
    - `Move { from, to, elem_id }`
    - `Set { elem_id, value }`
    - `StyleStart`、`StyleEnd`
  - [ ] 6.2.6 定义 `ListSlice<'a>`：`RawData(Cow<[LoroValue]>)` / `RawStr { str, unicode_len }`
  - [ ] 6.2.7 定义 `DeleteSpanWithId`：支持双向删除跨度（`pos + signed_len`）
  - [ ] 6.2.8 定义 `TreeOp`：`Create { target, parent, position }`、`Move { target, parent, position }`、`Delete { target }`
  - [ ] 6.2.9 定义 `FutureInnerContent`：扩展点（如 Counter）

- [ ] **6.3 RawOp 与 RichOp**
  - [ ] 6.3.1 定义 `RawOp<'a> { id: ID, lamport: Lamport, container: ContainerIdx, content: RawOpContent<'a> }`
  - [ ] 6.3.2 定义 `RichOp<'a> { op: &'a Op, peer: PeerID, lamport: Lamport, timestamp: Timestamp, start: usize, end: usize }`

- [ ] **6.4 Change**
  - [ ] 6.4.1 定义 `Change<O = Op> { id: ID, lamport: Lamport, deps: Frontiers, timestamp: Timestamp, ops: RleVec<[O; 1]> }`
  - [ ] 6.4.2 为 `Change` 实现 `DagNode`、`HasId`、`HasCounter`、`HasLamport`、`HasLength`、`Sliceable`
  - [ ] 6.4.3 实现 `Change::ops()` / `deps()` / `peer()` / `lamport()` / `timestamp()` / `id()`
  - [ ] 6.4.4 实现 `Change::slice(from, to)`：按原子 op 偏移分割 Change（包括分割 ops）
  - [ ] 6.4.5 实现 `Change::can_merge_right(other, merge_interval)`：判断两个 Change 是否可合并

- [ ] **6.5 Phase 6 测试**
  - [ ] 6.5.1 测试 `Change::slice` 正确分割 ops
  - [ ] 6.5.2 测试 `Change::can_merge_right`（时间间隔内可合并，超时不可合并）
  - [ ] 6.5.3 测试 `DeleteSpanWithId` 的双向表示和合并
  - [ ] 6.5.4 测试 `MapSet` 的序列化/反序列化
  - [ ] 6.5.5 测试 `TreeOp` 的序列化/反序列化
  - [ ] 6.5.6 运行 fmt、clippy、test，全部通过

---

## Phase 7: OpLog（操作日志核心）

**目标**: 实现历史存储中心，整合 DAG、ChangeStore、PendingChanges。
**验收标准**: 能本地插入变更、导入远程变更、处理乱序依赖、迭代 ops。

- [ ] **7.1 ChangeStore（内存版）**
  - [ ] 7.1.1 定义 `ChangeStore`：
    - `inner: ChangeStoreInner`（`BTreeMap<ID, ChangesBlock>`）
    - `arena: SharedArena`
  - [ ] 7.1.2 定义 `ChangesBlock`：包含 peer 的 counter 范围，内容可为 `Changes(Vec<Change>)`、`Bytes(Vec<u8>)` 或 `Both`
  - [ ] 7.1.3 实现 `insert_change(change, is_local)`：尝试合并到最后一个 block
  - [ ] 7.1.4 实现 `get_change(id)` → `Option<Change>`
  - [ ] 7.1.5 实现 `iter_changes(id_span)` → 迭代器
  - [ ] 7.1.6 实现 `get_last_dag_nodes_for_peer(peer)` → 供 AppDag 惰性加载

- [ ] **7.2 PendingChanges**
  - [ ] 7.2.1 定义 `PendingChanges`：存储依赖未满足的变更队列
  - [ ] 7.2.2 实现 `push(change)`：尝试应用，失败则入队
  - [ ] 7.2.3 实现 `try_apply_pending(oplog)`：每次新变更导入后尝试应用挂起的变更

- [ ] **7.3 OpLog 组装**
  - [ ] 7.3.1 定义 `OpLog { dag: AppDag, arena: SharedArena, change_store: ChangeStore, pending_changes: PendingChanges }`
  - [ ] 7.3.2 实现 `OpLog::new()`
  - [ ] 7.3.3 实现 `insert_new_change(change, from_local)`：唯一入口，更新 DAG、ChangeStore、arena 父子链接
  - [ ] 7.3.4 实现 `import_local_change(change)`：导入本地事务产生的变更
  - [ ] 7.3.5 实现 `import_remote_change(change)`：导入远程变更（处理 pending）
  - [ ] 7.3.6 实现 `next_id(peer)` → 该 peer 的下一个可用 counter
  - [ ] 7.3.7 实现 `vv()` → 当前 VV，`frontiers()` → 当前 Frontiers
  - [ ] 7.3.8 实现 `iter_ops(id_span)` → 迭代 `RichOp`
  - [ ] 7.3.9 实现 `get_change_at(id)` → `Option<Change>`

- [ ] **7.4 Phase 7 测试**
  - [ ] 7.4.1 测试线性历史插入：10 个 Change 后 VV 和 Frontiers 正确
  - [ ] 7.4.2 测试分叉历史：peer A 和 peer B 各插入，merge 后 Frontiers 包含两个 ID
  - [ ] 7.4.3 测试乱序导入：先导入依赖方的变更，再导入被依赖方的变更
  - [ ] 7.4.4 测试重复导入：同一 Change 导入两次，状态不变
  - [ ] 7.4.5 测试 `iter_ops` 的正确性（counter 连续性、RichOp 字段完整）
  - [ ] 7.4.6 运行 fmt、clippy、test，全部通过

---

## Phase 8: 事务系统（Transaction）

**目标**: 实现本地编辑的事务缓冲、提交和事件生成。
**验收标准**: 用户能开启事务、应用多个 op、提交为一个 Change，并触发事件。

- [ ] **8.1 Transaction 结构**
  - [ ] 8.1.1 定义 `Transaction { peer, start_counter, next_counter, start_lamport, next_lamport, frontiers, local_ops, arena, finished }`
  - [ ] 8.1.2 定义 `EventHint` 枚举：
    - `InsertText { pos, event_len, unicode_len, styles }`
    - `Map { key, value }`
    - `Tree { .. }`
    - `List { .. }`

- [ ] **8.2 事务生命周期**
  - [ ] 8.2.1 实现 `Transaction::new(doc, origin)`：锁定 OpLog 和 DocState，分配 counter/lamport
  - [ ] 8.2.2 实现 `apply_local_op(container, content, event_hint, doc)`：
    - 将 `RawOpContent` 转为 `Op`（通过 Arena 分配 slice/字符串）
    - 应用 op 到 `DocState`
    - 更新 DAG 版本追踪
    - 记录 EventHint
  - [ ] 8.2.3 实现 `commit()` / `_commit()`：
    - 构建 `Change`
    - 导入 `OpLog`
    - 提交 `DocState`
    - 运行 `on_commit` 回调

- [ ] **8.3 事件生成**
  - [ ] 8.3.1 实现 `change_to_diff(change, event_hints)`：利用 EventHint 快速生成 `TxnContainerDiff`
  - [ ] 8.3.2 定义 `TxnContainerDiff`：容器级别的差异表示

- [ ] **8.4 Phase 8 测试**
  - [ ] 8.4.1 测试空事务提交（不产生 Change）
  - [ ] 8.4.2 测试单 op 事务提交（counter、lamport、deps 正确）
  - [ ] 8.4.3 测试多 op 事务提交（ops 的 counter 连续，Change 的 len 正确）
  - [ ] 8.4.4 测试事务中应用 op 后 DocState 立即更新
  - [ ] 8.4.5 测试两个连续事务的 deps 关系（第二个 deps 指向第一个）
  - [ ] 8.4.6 运行 fmt、clippy、test，全部通过

---

## Phase 9: Counter CRDT

**目标**: 实现最简单的 CRDT——PN-Counter。
**验收标准**: 并发增量操作可正确合并，结果是代数和。

- [ ] **9.1 CounterState**
  - [ ] 9.1.1 定义 `CounterState { idx: ContainerIdx, value: f64 }`
  - [ ] 9.1.2 实现 `ContainerState` trait for `CounterState`：
    - `apply_local_op`：接收 `RawOpContent::Counter(diff)`，累加到 `value`
    - `apply_diff_and_convert`：接收 `InternalDiff::Counter(diff)`，累加并返回外部 diff
    - `to_diff`：返回 `InternalDiff::Counter(self.value)`（从空状态恢复）
    - `get_value`：返回 `LoroValue::Double(self.value)`
    - `fork`：克隆
  - [ ] 9.1.3 `is_state_empty()` 返回 `false`

- [ ] **9.2 Counter 操作定义**
  - [ ] 9.2.1 在 `RawOpContent` 中添加 `Counter(f64)`
  - [ ] 9.2.2 在 `InnerContent` 中添加 `Future(FutureInnerContent::Counter(f64))` 或直接添加 `Counter(f64)`

- [ ] **9.3 CounterHandler**
  - [ ] 9.3.1 定义 `CounterHandler`（Attached/Detached 双态）
  - [ ] 9.3.2 实现 `increment(n: f64)`、`decrement(n: f64)`
  - [ ] 9.3.3 实现 `get_value() -> f64`

- [ ] **9.4 Phase 9 测试**
  - [ ] 9.4.1 测试单文档 Counter 累加：`0 → +3 → +(-2) → 1`
  - [ ] 9.4.2 测试 idempotency：同一 op 应用两次，结果不变
  - [ ] 9.4.3 测试并发合并：A +3，B -2，合并后 = +1
  - [ ] 9.4.4 测试三文档并发合并：A +1，B +2，C -3，最终结果 = 0
  - [ ] 9.4.5 测试 `to_diff` 和 `apply_diff` 互为逆操作
  - [ ] 9.4.6 运行 fmt、clippy、test，全部通过

---

## Phase 10: Map CRDT（LWW Register）

**目标**: 实现基于 LWW（Last-Write-Wins）寄存器的 Map。
**验收标准**: 并发插入/删除同一 key 时，高 `(lamport, peer)` 者胜；删除是逻辑墓碑。

- [ ] **10.1 MapValue（LWW Register）**
  - [ ] 10.1.1 定义 `MapValue { lamp: Lamport, peer: PeerID, value: Option<LoroValue> }`
  - [ ] 10.1.2 实现 LWW 比较：`Ord for MapValue` 按 `(lamport, peer)` 排序
  - [ ] 10.1.3 `value: None` 表示逻辑删除（tombstone）

- [ ] **10.2 MapState**
  - [ ] 10.2.1 定义 `MapState { idx: ContainerIdx, map: BTreeMap<InternalString, MapValue>, child_containers: FxHashMap<ContainerID, InternalString>, size: usize }`
  - [ ] 10.2.2 实现 `apply_local_op`：
    - 接收 `MapSet { key, value }`
    - 比较新旧 `MapValue` 的 `(lamport, peer)`
    - 新值胜则更新，记录被删除的子容器到 `ApplyLocalOpReturn`
    - 更新 `size`（仅当 `value.is_some()`）
  - [ ] 10.2.3 实现 `apply_diff`：
    - 接收 `MapDelta`
    - `Checkout`/`Linear` 模式：强制应用
    - `Import` 模式：LWW 比较，只有更新才应用
  - [ ] 10.2.4 实现 `to_diff`：遍历 `map`，生成 `MapDelta`
  - [ ] 10.2.5 实现 `get_value`：过滤 `value.is_some()` 的条目，构建 `LoroValue::Map`
  - [ ] 10.2.6 实现 `fork`：深拷贝

- [ ] **10.3 MapDelta**
  - [ ] 10.3.1 定义 `MapDelta { updated: FxHashMap<InternalString, Option<MapValue>> }`
  - [ ] 10.3.2 实现 `MapDelta::compose(other)`：合并两个 delta，LWW 比较冲突键

- [ ] **10.4 MapHandler**
  - [ ] 10.4.1 定义 `MapHandler`（Attached/Detached）
  - [ ] 10.4.2 实现 `insert(key, value)`、`delete(key)`
  - [ ] 10.4.3 实现 `get(key) -> Option<LoroValue>`
  - [ ] 10.4.4 实现 `insert_container(key, handler)`：插入嵌套容器，在 Arena 中建立父子关系

- [ ] **10.5 Phase 10 测试**
  - [ ] 10.5.1 测试 Map 单操作：insert、get、delete
  - [ ] 10.5.2 测试 Map idempotency：同一 op 应用两次
  - [ ] 10.5.3 测试并发插入同一 key：A insert("k", "A")，B insert("k", "B")，高 lamport 胜
  - [ ] 10.5.4 测试并发 insert + delete：A insert("k", "v")，B delete("k")，高 lamport 决定结果
  - [ ] 10.5.5 测试 tombstone：delete 后再并发 insert，新 insert 参与 LWW 比较
  - [ ] 10.5.6 测试嵌套容器：Map 中 insert List，Arena 父子关系正确
  - [ ] 10.5.7 测试 `to_diff` / `apply_diff` 互为逆操作
  - [ ] 10.5.8 测试双文档 100 次随机编辑后 merge，结果一致
  - [ ] 10.5.9 运行 fmt、clippy、test，全部通过

---

## Phase 11: List CRDT（RGA）

**目标**: 实现基于 RGA（Replicated Growable Array）的 List。
**验收标准**: 并发插入顺序确定（按 ID 全序），删除为逻辑墓碑，pos→ID 转换正确。

- [ ] **11.1 ListElement**
  - [ ] 11.1.1 定义 `ListElement { value: LoroValue, id: IdFull, deleted: bool }`

- [ ] **11.2 ListState**
  - [ ] 11.2.1 定义 `ListState { idx: ContainerIdx, list: Vec<ListElement>, child_container_to_index: FxHashMap<ContainerID, usize> }`
  - [ ] 11.2.2 实现 RGA 插入排序：新元素按 `(lamport, peer)` 全序插入到正确位置
  - [ ] 11.2.3 实现 `apply_local_op`：
    - `Insert`：找到插入位置（基于 `pos` 参数和 RGA 排序），插入 `ListElement`
    - `Delete`：将对应范围元素标记 `deleted = true`
  - [ ] 11.2.4 实现 `apply_diff`：
    - 处理 `ListRaw` delta（Retain/Insert/Delete）
    - Insert 时按 RGA 规则排序插入
    - Delete 时标记 tombstone
  - [ ] 11.2.5 实现 `to_diff`：遍历所有元素，生成 `ListRaw` delta
  - [ ] 11.2.6 实现 `get_value`：跳过 `deleted = true`，收集可见元素
  - [ ] 11.2.7 实现 `len()`（可见长度）和 `get(pos)`（按可见位置查找）
  - [ ] 11.2.8 实现 `fork`：深拷贝

- [ ] **11.3 ListDiff**
  - [ ] 11.3.1 定义 `ListRawDelta`：基于 `Delta<SliceWithId>` 的序列差异
  - [ ] 11.3.2 定义 `SliceWithId { value: LoroValue, id: IdFull }`

- [ ] **11.4 ListHandler**
  - [ ] 11.4.1 定义 `ListHandler`（Attached/Detached）
  - [ ] 11.4.2 实现 `insert(pos, value)`：将 pos 转换为插入锚点 ID，生成 RGA 插入 op
  - [ ] 11.4.3 实现 `delete(pos, len)`
  - [ ] 11.4.4 实现 `get(index)`、`len()`、`push(value)`、`pop()`
  - [ ] 11.4.5 实现 `insert_container(pos, handler)`：插入嵌套容器

- [ ] **11.5 Phase 11 测试**
  - [ ] 11.5.1 测试 List 单操作：insert、delete、get、len
  - [ ] 11.5.2 测试 List idempotency
  - [ ] 11.5.3 测试并发插入同一位置：A 在位置 0 插入 "A"，B 在位置 0 插入 "B"，RGA 全序确定最终顺序
  - [ ] 11.5.4 测试并发 insert + delete：A insert("x")，B delete("x")，tombstone 保留锚点
  - [ ] 11.5.5 测试删除后并发 insert：删除位置 0 后，C 在位置 0 插入，新元素正确锚定
  - [ ] 11.5.6 测试双文档 100 次随机编辑后 merge，结果一致（frontiers 和 get_value 都一致）
  - [ ] 11.5.7 测试 pos → ID → pos  round-trip 正确性
  - [ ] 11.5.8 测试 `to_diff` / `apply_diff` 互为逆操作
  - [ ] 11.5.9 运行 fmt、clippy、test，全部通过

---

## Phase 12: MovableList CRDT

**目标**: 实现支持元素移动和更新的 List。
**验收标准**: Move 和 Set 操作的 LWW 冲突可正确解决，双向指针一致。

- [ ] **12.1 MovableList 内部结构**
  - [ ] 12.1.1 定义 `ListItem { pointed_by: Option<CompactIdLp>, id: IdFull }`
  - [ ] 12.1.2 定义 `Element { value: LoroValue, value_id: IdLp, pos: IdLp }`
  - [ ] 12.1.3 定义 `InnerState`：
    - `list: Vec<ListItem>`（按 op 顺序，包含 dead items）
    - `elements: FxHashMap<CompactIdLp, Element>`
    - `id_to_list_index: FxHashMap<IdLp, usize>`
    - `child_container_to_elem: FxHashMap<ContainerID, CompactIdLp>`

- [ ] **12.2 MovableListState**
  - [ ] 12.2.1 实现 `apply_local_op`：
    - `Insert`：在 `list` 末尾添加 `ListItem`，创建 `Element` 指向它
    - `Delete`：遍历 list，将对应 op-index 范围的 `ListItem.pointed_by` 设为 `None`，删除对应 `Element`
    - `Move`：在目标位置插入新 `ListItem`，更新对应 `Element.pos`，旧 `ListItem.pointed_by` 设为 `None`
    - `Set`：LWW 比较 `value_id`，更新 `Element.value`
  - [ ] 12.2.2 实现 `apply_diff`：
    - 先应用 list 位置变化（Insert/Delete in op-index space）
    - 再应用元素更新（value + pos 的 LWW 比较）
    - 维护双向一致性
  - [ ] 12.2.3 实现 `to_diff`：生成 `MovableListInnerDelta`
  - [ ] 12.2.4 实现 `get_value`：仅收集 `pointed_by.is_some()` 的 ListItem 对应的 Element value
  - [ ] 12.2.5 实现两套索引转换：`op_index_to_user_index` 和 `user_index_to_op_index`

- [ ] **12.3 MovableListInnerDelta**
  - [ ] 12.3.1 定义 `MovableListInnerDelta { list: Delta<Vec<IdFull>>, elements: FxHashMap<CompactIdLp, ElementDelta> }`
  - [ ] 12.3.2 定义 `ElementDelta { pos: Option<IdLp>, value: LoroValue, value_updated: bool, value_id: Option<IdLp> }`

- [ ] **12.4 MovableListHandler**
  - [ ] 12.4.1 定义 `MovableListHandler`（Attached/Detached）
  - [ ] 12.4.2 实现 `insert(pos, value)`、`delete(pos, len)`、`set(pos, value)`、`mov(from, to)`
  - [ ] 12.4.3 实现 `get(index)`、`len()`

- [ ] **12.5 Phase 12 测试**
  - [ ] 12.5.1 测试 Insert + Get + Len
  - [ ] 12.5.2 测试 Delete 后可见长度减少
  - [ ] 12.5.3 测试 Move：移动元素位置
  - [ ] 12.5.4 测试 Set：更新元素值（LWW）
  - [ ] 12.5.5 测试并发 Move：A move(0→2)，B move(0→1)，高 lamport 胜
  - [ ] 12.5.6 测试并发 Set：A set(0, "A")，B set(0, "B")，高 lamport 胜
  - [ ] 12.5.7 测试 Move + Set 并发：不影响彼此
  - [ ] 12.5.8 测试双向一致性：`ListItem.pointed_by` 和 `Element.pos` 始终匹配
  - [ ] 12.5.9 测试 `to_diff` / `apply_diff` 互为逆操作
  - [ ] 12.5.10 测试双文档随机操作后 merge，结果一致
  - [ ] 12.5.11 运行 fmt、clippy、test，全部通过

---

## Phase 13: Text / Richtext CRDT

**目标**: 实现文本 CRDT，初期可用简化 List-based 版本，后期替换为 Fugue。
**验收标准**: 文本插入/删除/样式标记正确，并发编辑一致性。

- [ ] **13.1 简化 TextState（基于 ListState）**
  - [ ] 13.1.1 定义 `TextState { idx: ContainerIdx, state: ListState }`（元素为字符或字符串块）
  - [ ] 13.1.2 实现 `insert_text(pos, text)`：将 pos 转为 unicode 索引，插入字符串
  - [ ] 13.1.3 实现 `delete_text(pos, len)`：unicode 索引范围的删除
  - [ ] 13.1.4 实现 `to_string()` → `String`
  - [ ] 13.1.5 实现 `get_value()` → `LoroValue::String`

- [ ] **13.2 Fugue 准备（可选延后）**
  - [ ] 13.2.1 定义 `RichtextStateChunk` 枚举：`Text(String)`、`StyleAnchor { type: AnchorType, id: IdFull }`
  - [ ] 13.2.2 定义 `AnchorType::Start` / `End`
  - [ ] 13.2.3 定义 `StyleOp { lamport, peer, key, value, info }`
  - [ ] 13.2.4 定义 `TextStyleInfoFlag`：编码 expand 行为（Before/After/Both/None）
  - [ ] 13.2.5 定义 `RichtextState`：使用 `BTree` 或自定义结构存储 chunks
  - [ ] 13.2.6 实现 Entity Index：unicode index + anchor index
  - [ ] 13.2.7 实现 `insert_text` / `delete` / `mark` / `unmark`

- [ ] **13.3 Richtext Diff**
  - [ ] 13.3.1 定义 `TextDelta`：`DeltaRope<TextChunk, StyleMeta>`
  - [ ] 13.3.2 定义 `StyleMeta`：`FxHashMap<InternalString, StyleMetaItem>`
  - [ ] 13.3.3 实现 `StyleMetaItem` 的 LWW 合并

- [ ] **13.4 TextHandler**
  - [ ] 13.4.1 定义 `TextHandler`（Attached/Detached）
  - [ ] 13.4.2 实现 `insert(pos, text)`（unicode 位置）
  - [ ] 13.4.3 实现 `delete(pos, len)`
  - [ ] 13.4.4 实现 `mark(start, end, key, value)` / `unmark(start, end, key)`（富文本）
  - [ ] 13.4.5 实现 `to_string()`、`to_delta()`
  - [ ] 13.4.6 实现 `update(text, options)`：使用 Myers diff 算法计算最小操作集

- [ ] **13.5 Phase 13 测试**
  - [ ] 13.5.1 测试文本插入和删除
  - [ ] 13.5.2 测试并发插入同一位置：字符顺序由 RGA/Fugue 全序确定
  - [ ] 13.5.3 测试并发删除同一范围：idempotency
  - [ ] 13.5.4 测试富文本 mark/unmark
  - [ ] 13.5.5 测试 `TextHandler::update` 的 diff 行为
  - [ ] 13.5.6 测试 `to_diff` / `apply_diff` 互为逆操作
  - [ ] 13.5.7 测试双文档随机编辑后 merge，文本一致
  - [ ] 13.5.8 运行 fmt、clippy、test，全部通过

---

## Phase 14: Tree CRDT（Movable Tree + FractionalIndex）

**目标**: 实现可移动树，支持无环检测和 FractionalIndex 子节点排序。
**验收标准**: 树的创建/移动/删除正确，无环，子节点顺序稳定。

- [ ] **14.1 FractionalIndex**
  - [ ] 14.1.1 定义 `FractionalIndex(Arc<Vec<u8>>)`
  - [ ] 14.1.2 实现 `FractionalIndex::new_between(a, b)`：在两个索引之间生成新索引
  - [.1.3 实现 `FractionalIndex::new_after(a)` 和 `new_before(b)`
  - [ ] 14.1.4 实现 `FractionalIndex::generate_n_evenly(lower, upper, n)`：均匀生成 n 个索引
  - [ ] 14.1.5 terminator 字节值为 128，使用 0~255 空间

- [ ] **14.2 TreeState**
  - [ ] 14.2.1 定义 `TreeStateNode { parent: TreeParentId, position: Option<FractionalIndex>, last_move_op: IdFull }`
  - [ ] 14.2.2 定义 `TreeParentId`：`Root`、`Node(TreeID)`、`Deleted`
  - [ ] 14.2.3 定义 `TreeState { idx, trees: FxHashMap<TreeID, TreeStateNode>, children: FxHashMap<TreeParentId, Vec<TreeID>>, fractional_index_config, peer_id }`
  - [ ] 14.2.4 实现 `apply_local_op`：
    - `Create`：创建 `TreeStateNode`，parent 为 Root 或指定节点
    - `Move`：调用 `mov(target, parent, id, position, true)`，检查无环
    - `Delete`：parent 设为 `Deleted`
  - [ ] 14.2.5 实现无环检测 `is_ancestor_of(ancestor, descendant)`：从 descendant 向上遍历 parent
  - [ ] 14.2.6 实现 `apply_diff`：
    - `Create` / `Move` / `Delete` / `MoveInDelete` / `UnCreate`
    - `Import` 模式：LWW 比较 `last_move_op`
    - `Checkout`/`Linear` 模式：直接应用
  - [ ] 14.2.7 实现 `get_value`：递归构建 `TreeNodeWithChildren` 结构
  - [ ] 14.2.8 实现子节点排序：按 `FractionalIndex` 排序，碰撞时重新分配
  - [ ] 14.2.9 实现 `fork`：深拷贝

- [ ] **14.3 TreeDiff**
  - [ ] 14.3.1 定义 `TreeExternalDiff`：`Create { parent, index, position }`、`Move { parent, index, position, old_parent, old_index }`、`Delete { old_parent, old_index }`
  - [ ] 14.3.2 定义 `TreeInternalDiff`：`Create`、`UnCreate`、`Move`、`Delete`、`MoveInDelete`
  - [ ] 14.3.3 定义 `TreeDeltaItem { target, action: TreeInternalDiff, last_effective_move_op_id: IdFull }`

- [ ] **14.4 TreeHandler**
  - [ ] 14.4.1 定义 `TreeHandler`（Attached/Detached）
  - [ ] 14.4.2 实现 `create(parent) -> TreeID`、`create_at(parent, index)`
  - [ ] 14.4.3 实现 `mov(target, parent)`、`mov_to(target, parent, index)`、`mov_after`、`mov_before`
  - [ ] 14.4.4 实现 `delete(target)`
  - [ ] 14.4.5 实现 `get_meta(target) -> MapHandler`：每个节点关联一个 Map 容器
  - [ ] 14.4.6 实现 `children(parent)`、`parent(target)`
  - [ ] 14.4.7 实现 `enable_fractional_index(jitter)` / `disable_fractional_index()`

- [ ] **14.5 Phase 14 测试**
  - [ ] 14.5.1 测试 Tree 创建：根节点、子节点
  - [ ] 14.5.2 测试 Tree 移动：mov 改变 parent
  - [ ] 14.5.3 测试无环检测：尝试将父节点移到自己的子树下，应失败/拒绝
  - [ ] 14.5.4 测试 Tree 删除：删除后 `is_node_deleted` 返回 true
  - [ ] 14.5.5 测试 FractionalIndex：生成、排序、between 不碰撞
  - [ ] 14.5.6 测试并发 Create：两个 peer 各自创建节点，merge 后都存在
  - [ ] 14.5.7 测试并发 Move：A move X→P1，B move X→P2，高 lamport 胜
  - [ ] 14.5.8 测试 Move + Delete 并发
  - [ ] 14.5.9 测试 metadata Map：创建节点时自动创建关联 Map，删除时一起删除
  - [ ] 14.5.10 测试 `to_diff` / `apply_diff` 互为逆操作
  - [ ] 14.5.11 测试双文档随机操作后 merge，树结构一致
  - [ ] 14.5.12 运行 fmt、clippy、test，全部通过

---

## Phase 15: DocState（容器状态分发与管理）

**目标**: 实现文档状态中心，统一管理所有容器状态的生命周期。
**验收标准**: DocState 能正确分发 op/diff 到具体容器，管理懒加载和事件。

- [ ] **15.1 ContainerState Trait 完整定义**
  - [ ] 15.1.1 定义 `ContainerState` trait（如果前面 phases 中已定义则完善）：
    - `apply_local_op(raw_op, op) -> ApplyLocalOpReturn`
    - `apply_diff_and_convert(diff, ctx) -> Diff`
    - `to_diff(doc) -> Diff`
    - `get_value() -> LoroValue`
    - `get_value_by_idx(idx) -> LoroValue`
    - `fork(config) -> Self`
    - `is_state_empty() -> bool`
  - [ ] 15.1.2 定义 `ApplyLocalOpReturn { deleted_containers: Vec<ContainerIdx> }`

- [ ] **15.2 State 枚举**
  - [ ] 15.2.1 定义 `State` 枚举：
    - `List(ListState)`、`MovableList(MovableListState)`、`Map(MapState)`、`Richtext(RichtextState)`、`Tree(TreeState)`、`Counter(CounterState)`、`Unknown(UnknownState)`
  - [ ] 15.2.2 为 `State` 实现 `ContainerState`：按变体分发

- [ ] **15.3 ContainerStore**
  - [ ] 15.3.1 定义 `ContainerStore { map: FxHashMap<ContainerIdx, State> }`
  - [ ] 15.3.2 实现 `get_or_create(idx, container_type)`：懒加载，首次访问时创建默认状态
  - [ ] 15.3.3 实现 `get(idx)`、`get_mut(idx)`

- [ ] **15.4 DocState**
  - [ ] 15.4.1 定义 `DocState { peer, frontiers, store: ContainerStore, arena, in_txn, changed_idx_in_txn, event_recorder }`
  - [ ] 15.4.2 实现 `apply_local_op(raw_op, op)`：
    - 通过 `op.container` 找到对应 `State`
    - 调用 `ContainerState::apply_local_op`
    - 记录 `changed_idx_in_txn`
    - 处理被删除的子容器
  - [ ] 15.4.3 实现 `apply_diff(diff, diff_mode)`：
    - 遍历 `InternalDocDiff`，对每个容器调用 `apply_diff_and_convert`
    - 处理容器复活（`bring_back = true`）：被删除的容器重新出现，发送全量事件
  - [ ] 15.4.4 实现 `start_txn()` / `commit_txn()` / `abort_txn()`
  - [ ] 15.4.5 实现 `get_value()`（浅层）和 `get_deep_value()`（递归解析子容器）
  - [ ] 15.4.6 实现 `get_path_to_container(idx)`：利用 Arena 的 parent 链计算 root-to-leaf 路径
  - [ ] 15.4.7 实现 `init_with_states_and_version(frontiers, states)`：从快照初始化

- [ ] **15.5 Phase 15 测试**
  - [ ] 15.5.1 测试懒加载：访问不存在的容器时自动创建默认状态
  - [ ] 15.5.2 测试多容器事务：一次事务修改 Map 和 List 两个容器
  - [ ] 15.5.3 测试嵌套容器：Map 中插入 List，List 中插入 Text，get_deep_value 递归正确
  - [ ] 15.5.4 测试容器删除后复活：delete Map key（值为 List），然后 checkout 回之前版本，List 重新出现
  - [ ] 15.5.5 测试 `get_path_to_container` 路径正确
  - [ ] 15.5.6 运行 fmt、clippy、test，全部通过

---

## Phase 16: Diff 计算系统

**目标**: 实现两个版本之间的差异计算，用于 import 时的状态更新和 checkout。
**验收标准**: 任意两个版本之间的 diff 能正确转换为状态更新。

- [ ] **16.1 DiffCalculator 框架**
  - [ ] 16.1.1 定义 `DiffCalculator { calculators: FxHashMap<ContainerIdx, (Option<depth>, ContainerDiffCalculator)> }`
  - [ ] 16.1.2 定义 `DiffCalcVersionInfo { from_vv, to_vv, from_frontiers, to_frontiers }`
  - [ ] 16.1.3 定义 `DiffCalculatorTrait`：
    - `start_tracking(from, to)`
    - `apply_change(change)`
    - `calculate_diff()`
    - `finish_this_round()`

- [ ] **16.2 按容器的 DiffCalculator**
  - [ ] 16.2.1 实现 `MapDiffCalculator`：
    - `Linear`/`ImportGreaterUpdates`：累积 changed 映射
    - `Checkout`/`Import`：使用 HistoryCache（或遍历完整历史）
  - [ ] 16.2.2 实现 `ListDiffCalculator`：基于位置追踪的插入/删除差异
  - [ ] 16.2.3 实现 `RichtextDiffCalculator`：`Crdt` 模式（完整追踪）vs `Linear` 模式（直接构建 DeltaRope）
  - [ ] 16.2.4 实现 `TreeDiffCalculator`：`TreeCacheForDiff` + 祖先检测 + LWW move
  - [ ] 16.2.5 实现 `MovableListDiffCalculator`：组合 List + 元素级追踪
  - [ ] 16.2.6 实现 `CounterDiffCalculator`：`BTreeMap<ID, f64>`，diff 时求和
  - [ ] 16.2.7 定义 `ContainerDiffCalculator` 枚举：按容器类型分发

- [ ] **16.3 文本 Diff 算法（Myers）**
  - [ ] 16.3.1 实现 Myers diff 算法（或集成 `similar` crate）
  - [ ] 16.3.2 实现 `UpdateOptions { timeout_ms, use_refined_diff }`
  - [ ] 16.3.3 实现 `diff(a, b) -> Vec<DiffOp>`（Insert/Delete/Retain）

- [ ] **16.4 Phase 16 测试**
  - [ ] 16.4.1 测试线性历史的 diff：A 编辑 10 次，从 VV0 diff 到 VV10，结果等于最终状态
  - [ ] 16.4.2 测试分叉历史的 diff：A 和 B 各编辑，从共同祖先 diff 到合并后版本
  - [ ] 16.4.3 测试 checkout diff：从最新版本 checkout 到历史版本，diff 正确应用
  - [ ] 16.4.4 测试 Map 的并发 diff：两个版本修改同一 key，diff 包含 LWW 结果
  - [ ] 16.4.5 测试 Tree diff：move 操作在 diff 中正确表示为 Move（而非 Delete+Create）
  - [ ] 16.4.6 测试 Text diff（Myers）：两个字符串的 diff 是最小编辑集
  - [ ] 16.4.7 运行 fmt、clippy、test，全部通过

---

## Phase 17: CoralDoc（文档顶层）与 Handler

**目标**: 实现用户入口 `CoralDoc`，整合 OpLog、DocState、Transaction、DiffCalculator。
**验收标准**: 用户能通过 Handler API 完成完整 CRUD 操作，事务自动提交，事件正确触发。

- [ ] **17.1 CoralDocInner**
  - [ ] 17.1.1 定义 `CoralDocInner { oplog, state, arena, config, txn, auto_commit, detached, observer, diff_calculator }`
  - [ ] 17.1.2 实现 `CoralDocInner::new()`：创建 OpLog、DocState、Arena，建立共享
  - [ ] 17.1.3 实现 `txn()` / `txn_with_origin()`：获取或创建 Transaction
  - [ ] 17.1.4 实现 `import(bytes)` / `import_with(bytes, origin)`：
    - 解码变更
    - 当 attached 时：计算 diff → 应用至 DocState
    - 当 detached 时：仅追加到 OpLog
  - [ ] 17.1.5 实现 `export(mode)`：按 ExportMode 编码（Updates / Snapshot）
  - [ ] 17.1.6 实现 `checkout(frontiers)`：
    - 进入 detached 模式
    - 用 DiffCalculator 计算当前→目标的 diff
    - 应用 diff 到 DocState
  - [ ] 17.1.7 实现 `checkout_to_latest()` / `attach()` / `detach()`
  - [ ] 17.1.8 实现 `merge(other)`：导入 other 的所有更新
  - [ ] 17.1.9 实现 `fork()`：深拷贝 OpLog + DocState + Arena，分配新 peer_id
  - [ ] 17.1.10 实现 `get_value()` / `get_deep_value()`
  - [ ] 17.1.11 实现 `set_peer_id()` / `peer_id()`
  - [ ] 17.1.12 实现 `commit()` / `commit_with(options)` / `implicit_commit_then_stop()`

- [ ] **17.2 Handler 双态模式**
  - [ ] 17.2.1 定义 `MaybeDetached<T>`：`Detached(Arc<Mutex<DetachedInner<T>>>)` / `Attached(BasicHandler)`
  - [ ] 17.2.2 定义 `BasicHandler { id: ContainerID, container_idx: ContainerIdx, doc: CoralDoc }`
  - [ ] 17.2.3 定义 `HandlerTrait`：
    - `is_attached()`、`attached_handler()`、`get_value()`、`get_deep_value()`、`kind()`
    - `to_handler()`、`from_handler()`、`doc()`、`attach()`
  - [ ] 17.2.4 实现 `BasicHandler::with_txn(f)`：获取当前 auto-commit transaction 并执行
  - [ ] 17.2.5 实现 `BasicHandler::with_state(f)`：读取 DocState

- [ ] **17.3 各容器 Handler**
  - [ ] 17.3.1 实现 `TextHandler`：委托给 `TextState` 的操作
  - [ ] 17.3.2 实现 `MapHandler`：委托给 `MapState` 的操作
  - [ ] 17.3.3 实现 `ListHandler`：委托给 `ListState` 的操作
  - [ ] 17.3.4 实现 `MovableListHandler`：委托给 `MovableListState` 的操作
  - [ ] 17.3.5 实现 `TreeHandler`：委托给 `TreeState` 的操作
  - [ ] 17.3.6 实现 `CounterHandler`：委托给 `CounterState` 的操作
  - [ ] 17.3.7 定义 `Handler` 枚举：所有 Handler 的联合体
  - [ ] 17.3.8 为 `Handler` 实现 `HandlerTrait`

- [ ] **17.4 CoralDoc 公共 API**
  - [ ] 17.4.1 定义 `CoralDoc { inner: Arc<CoralDocInner> }`
  - [ ] 17.4.2 实现 `CoralDoc::new()`、`default()`、`clone()`（引用克隆）
  - [ ] 17.4.3 实现 `get_text(id)`、`get_map(id)`、`get_list(id)`、`get_movable_list(id)`、`get_tree(id)`、`get_counter(id)`
  - [ ] 17.4.4 实现 `get_container(id) -> Option<Container>`
  - [ ] 17.4.5 实现 `get_value()`、`get_deep_value()`
  - [ ] 17.4.6 实现 `import(bytes)`、`export(mode)`
  - [ ] 17.4.7 实现 `checkout(frontiers)`、`attach()`、`detach()`
  - [ ] 17.4.8 实现 `commit()`、`set_next_commit_message()`、`set_next_commit_origin()`

- [ ] **17.5 Phase 17 测试**
  - [ ] 17.5.1 测试 `CoralDoc::new()` 后 get_value 返回空文档
  - [ ] 17.5.2 测试通过 Handler 插入数据后 get_value 正确
  - [ ] 17.5.3 测试 auto-commit：每个操作后自动提交，变更出现在 OpLog
  - [ ] 17.5.4 测试手动 commit：多个操作合并为一个 Change
  - [ ] 17.5.5 测试 detached 模式：import 不更新 state，checkout 后 state 变化
  - [ ] 17.5.6 测试 fork：新文档与原文档独立编辑
  - [ ] 17.5.7 测试嵌套容器操作：Map → List → Text 的完整链路
  - [ ] 17.5.8 运行 fmt、clippy、test，全部通过

---

## Phase 18: 编码与序列化

**目标**: 实现数据的导入导出，支持 JSON Schema 和二进制快照。
**验收标准**: 文档能正确 export/import，跨文档传输后状态一致。

- [ ] **18.1 JSON Schema**
  - [ ] 18.1.1 定义 `JsonSchema { schema_version, start_version, peers, changes }`
  - [ ] 18.1.2 定义 `JsonChange { id, timestamp, deps, lamport, msg, ops }`
  - [ ] 18.1.3 定义 `JsonOp { content, container, counter }`
  - [ ] 18.1.4 定义 `JsonOpContent`：untagged enum 覆盖 List/MovableList/Map/Text/Tree/Future
  - [ ] 18.1.5 实现 `export_json_updates(start_vv, end_vv)`：
    - 遍历 OpLog 中指定范围的 Change
    - 转换为 JsonChange
    - Peer 压缩（用 ValueRegister 映射 peer→小整数）
  - [ ] 18.1.6 实现 `import_json_updates(json)`：
    - 解析 JsonSchema
    - 转换回 `Vec<Change>`
    - 导入 OpLog

- [ ] **18.2 二进制编码（Fast Snapshot）**
  - [ ] 18.2.1 定义 `ExportMode` 枚举：
    - `Snapshot`：完整历史+状态
    - `Updates { from }`：增量更新
    - `UpdatesInRange { spans }`：指定范围
    - `StateOnly(frontiers)`：最小历史+状态
  - [ ] 18.2.2 实现编码头部：magic bytes + checksum + mode
  - [ ] 18.2.3 实现 `encode_snapshot()`：
    - 编码 OpLog（ChangeStore 导出）
    - 编码 State（DocState 导出）
    - 组合为 Snapshot 结构
  - [ ] 18.2.4 实现 `decode_snapshot(bytes)`：解析并重建 CoralDoc
  - [ ] 18.2.5 实现 `encode_updates(from_vv)`：导出指定 VV 之后的所有 Change
  - [ ] 18.2.6 实现 `decode_updates(bytes)`：解析并导入

- [ ] **18.3 Phase 18 测试**
  - [ ] 18.3.1 测试 JSON export/import：A 导出 JSON，B 导入，状态一致
  - [ ] 18.3.2 测试 Snapshot export/import：完整快照后重建，状态一致
  - [ ] 18.3.3 测试 Updates export/import：A 导出 updates，B 导入，B 的 state 与 A 一致
  - [ ] 18.3.4 测试增量同步：A 编辑 10 次，导出 updates，B 已有前 5 次，导入后 5 次
  - [ ] 18.3.5 测试乱序 import：updates 分片任意顺序导入，最终状态一致
  - [ ] 18.3.6 测试重复 import：同一 updates 导入两次，状态不变
  - [ ] 18.3.7 运行 fmt、clippy、test，全部通过

---

## Phase 19: Checkout 与时间旅行

**目标**: 实现 DocState 回滚到任意历史版本。
**验收标准**: checkout 到任意 frontiers 后，get_value 等于该版本的状态。

- [ ] **19.1 Checkout 核心逻辑**
  - [ ] 19.1.1 实现 `checkout(frontiers)`：
    - 计算 `state_vv` → `target_vv`
    - 用 DiffCalculator 计算 diff
    - 应用 diff 到 DocState
    - 更新 `state.frontiers`
    - 进入 detached 模式
  - [ ] 19.1.2 实现 `checkout_to_latest()`：diff 从当前到 oplog_frontiers，应用后 attach
  - [ ] 19.1.3 实现 `attach()`：将 state 同步到 oplog 最新版本
  - [ ] 19.1.4 实现 `detach()`：冻结 state，后续 import 只进 OpLog

- [ ] **19.2 Detached 编辑（可选高级功能）**
  - [ ] 19.2.1 实现 `set_detached_editing(true)`
  - [ ] 19.2.2 实现 detached 模式下的 Transaction：使用不同 peer_id
  - [ ] 19.2.3 确保 detached 编辑的变更导入到 attached 文档时正确合并

- [ ] **19.3 Phase 19 测试**
  - [ ] 19.3.1 测试线性 checkout：编辑 5 次，checkout 到第 3 次，state 正确
  - [ ] 19.3.2 测试 checkout 后 attach：checkout 到历史版本，再 attach 回最新
  - [ ] 19.3.3 测试分叉 checkout：A 和 B 并发编辑，checkout 到 A 的版本、B 的版本、合并后的版本
  - [ ] 19.3.4 测试 checkout 后编辑（detached editing）：在历史版本上编辑，再 attach
  - [ ] 19.3.5 测试 fork_at：从指定 frontiers fork，新文档只包含之前的历史
  - [ ] 19.3.6 运行 fmt、clippy、test，全部通过

---

## Phase 20: Merge 与 Sync

**目标**: 实现两个文档的合并，处理乱序、重复、缺失依赖。
**验收标准**: 任意两个文档合并后，frontiers 和 get_value 一致。

- [ ] **20.1 Import 与 Pending Queue**
  - [ ] 20.1.1 实现 `import(bytes)`：
    - 解析 blob 头部，确定编码模式
    - 解码为 `Vec<Change>`
    - 逐个导入：检查 deps 是否满足，满足则插入 OpLog，否则加入 PendingChanges
    - 每次插入后尝试 apply pending
  - [ ] 20.1.2 实现 `ImportStatus { success: VersionRange, pending: Option<VersionRange> }`
  - [ ] 20.1.3 确保重复导入的 Change 被幂等处理（通过 VV 判断已存在）

- [ ] **20.2 Batch Import**
  - [ ] 20.2.1 实现 `import_batch(bytes_list)`：
    - 排序或多次尝试，直到所有数据导入或确定缺失
    - 返回最终 ImportStatus

- [ ] **20.3 Merge 语义验证**
  - [ ] 20.3.1 测试两文档独立编辑后 merge：A 编辑 100 次，B 编辑 100 次，A.import(B), B.import(A)，状态一致
  - [ ] 20.3.2 测试并发冲突：所有容器类型的并发冲突场景（参考各 Phase 的并发测试）
  - [ ] 20.3.3 测试乱序导入：将 updates 拆分为 10 片，随机顺序导入
  - [ ] 20.3.4 测试缺失依赖：导入依赖未满足的变更，pending 后补全依赖再导入
  - [ ] 20.3.5 测试三方 merge：A、B、C 各自编辑，循环导入后状态一致

- [ ] **20.4 Phase 20 测试**
  - [ ] 20.4.1 运行所有 merge 场景测试
  - [ ] 20.4.2 运行 fmt、clippy、test，全部通过

---

## Phase 21: 事件系统（Event / Subscription）

**目标**: 实现用户可订阅的 diff 事件系统。
**验收标准**: 订阅者能在事务提交后收到正确的 ContainerDiff 序列。

- [ ] **21.1 事件类型定义**
  - [ ] 21.1.1 定义 `DiffEvent { triggered_by: EventTriggerKind, origin, current_target, events: Vec<ContainerDiff> }`
  - [ ] 21.1.2 定义 `EventTriggerKind { Local, Import, Checkout }`
  - [ ] 21.1.3 定义 `ContainerDiff { target, path, idx, is_unknown, diff }`
  - [ ] 21.1.4 定义 `Diff` 枚举（外部 diff）：
    - `List(Vec<ListDiffItem>)`
    - `Text(Vec<TextDelta>)`
    - `Map(MapDelta)`
    - `Tree(TreeDiff)`
    - `Counter(f64)`
  - [ ] 21.1.5 定义 `ListDiffItem`：`Insert { insert, is_move }`、`Delete { delete }`、`Retain { retain }`
  - [ ] 21.1.6 定义 `TextDelta`：`Insert { insert, attributes }`、`Delete { delete }`、`Retain { retain, attributes }`

- [ ] **21.2 内部事件记录**
  - [ ] 21.2.1 定义 `EventRecorder`：在事务期间记录所有 `InternalContainerDiff`
  - [ ] 21.2.2 定义 `InternalContainerDiff { idx, bring_back, diff: DiffVariant, diff_mode }`
  - [ ] 21.2.3 实现 `EventRecorder::push_diff(diff)`
  - [ ] 21.2.4 实现 `EventRecorder::into_external_diffs()`：将内部 diff 转换为外部 diff

- [ ] **21.3 订阅系统**
  - [ ] 21.3.1 定义 `Subscriber` 类型：`Arc<dyn Fn(DiffEvent) + Send + Sync>`
  - [ ] 21.3.2 定义 `Subscription`：支持 `unsubscribe()`
  - [ ] 21.3.3 实现 `subscribe(container_id, callback)`：注册容器级订阅
  - [ ] 21.3.4 实现 `subscribe_root(callback)`：注册文档级订阅（所有变更）
  - [ ] 21.3.5 实现 `subscribe_local_update(callback)`：本地变更提交时触发
  - [ ] 21.3.6 事务提交后：遍历所有订阅者，过滤相关 diff，构造 `DiffEvent` 调用

- [ ] **21.4 Phase 21 测试**
  - [ ] 21.4.1 测试单容器订阅：修改 Text，订阅者收到 TextDelta
  - [ ] 21.4.2 测试多容器订阅：一次事务修改 Map 和 List，root 订阅者收到两个 ContainerDiff
  - [ ] 21.4.3 测试事件触发时机：commit 后才触发，而不是 apply op 时
  - [ ] 21.4.4 测试 import 事件：导入远程变更，触发 Import 类型事件
  - [ ] 21.4.5 测试 checkout 事件：checkout 到历史版本，触发 Checkout 类型事件
  - [ ] 21.4.6 测试 unsubscribe：取消订阅后不再收到事件
  - [ ] 21.4.7 测试 bring_back（容器复活）事件：包含完整状态 diff
  - [ ] 21.4.8 运行 fmt、clippy、test，全部通过

---

## Phase 22: UndoManager

**目标**: 实现撤销/重做管理器。
**验收标准**: undo/redo 能正确回退/恢复变更，支持分组。

- [ ] **22.1 UndoManager 结构**
  - [ ] 22.1.1 定义 `UndoManager { doc, undo_stack, redo_stack, max_undo_steps, merge_interval, group_started }`
  - [ ] 22.1.2 定义 `UndoItem { diff: InternalDocDiff, meta: UndoItemMeta }`
  - [ ] 22.1.3 定义 `UndoItemMeta`：自定义元数据

- [ ] **22.2 Undo / Redo 逻辑**
  - [ ] 22.2.1 实现 `record_change(diff)`：每次本地 commit 后将 diff 压入 undo_stack
  - [ ] 22.2.2 实现 `undo()`：
    - 弹出 undo_stack 顶部的 diff
    - 应用反向 diff（invert）到 DocState
    - 压入 redo_stack
  - [ ] 22.2.3 实现 `redo()`：
    - 弹出 redo_stack 顶部的 diff
    - 应用正向 diff
    - 压入 undo_stack
  - [ ] 22.2.4 实现 `can_undo()` / `can_redo()`
  - [ ] 22.2.5 实现 `clear_redo()`：新编辑时清空 redo_stack

- [ ] **22.3 高级功能**
  - [ ] 22.3.1 实现 `group_start()` / `group_end()`：将多个变更合并为一个 undo 单元
  - [ ] 22.3.2 实现 `set_merge_interval(ms)`：短时间内的连续变更自动合并
  - [ ] 22.3.3 实现 `exclude_origin(prefix)`：特定 origin 的变更不记录到 undo
  - [ ] 22.3.4 实现 `on_push` / `on_pop` 回调（可选）

- [ ] **22.4 Phase 22 测试**
  - [ ] 22.4.1 测试简单 undo：insert "abc"，undo 后文本为空
  - [ ] 22.4.2 测试 undo + redo：undo 后再 redo，文本恢复 "abc"
  - [ ] 22.4.3 测试多步 undo：3 次插入，undo 2 次，状态正确
  - [ ] 22.4.4 测试 undo 后新编辑清空 redo：undo 后插入 "x"，redo 不可用
  - [ ] 22.4.5 测试分组 undo：group_start/end 包围 3 次插入，一次 undo 回退全部
  - [ ] 22.4.6 测试 merge_interval：1 秒内 5 次输入，undo 一次回退全部
  - [ ] 22.4.7 测试跨容器 undo：一次事务修改 Map 和 Text，undo 同时回退两者
  - [ ] 22.4.8 运行 fmt、clippy、test，全部通过

---

## Phase 23: 属性测试与压力测试

**目标**: 用随机操作序列验证 CRDT 的核心不变量。
**验收标准**: 大量随机测试下，所有 CRDT 性质始终成立。

- [ ] **23.1 基础属性测试**
  - [ ] 23.1.1 为 Counter 编写 proptest：随机 increment/decrement 序列，验证最终值 = 所有增量之和
  - [ ] 23.1.2 为 Map 编写 proptest：随机 insert/delete，验证最终状态 = 按 LWW 过滤后的状态
  - [ ] 23.1.3 为 List 编写 proptest：随机 insert/delete，验证最终顺序 = RGA 全序
  - [ ] 23.1.4 为 Tree 编写 proptest：随机 create/move/delete，验证无环、parent 关系一致

- [ ] **23.2 双文档同步测试**
  - [ ] 23.2.1 编写 `merge_sync` 测试：两个文档执行随机操作（各 100~1000 次），然后互相导入
  - [ ] 23.2.2 验证不变量：`doc_a.get_value() == doc_b.get_value()`
  - [ ] 23.2.3 验证不变量：`doc_a.oplog_frontiers() == doc_b.oplog_frontiers()`
  - [ ] 23.2.4 编写 `out_of_order_import` 测试：将操作日志随机打乱分片导入

- [ ] **23.3 Checkout 与时间旅行测试**
  - [ ] 23.3.1 编写 `checkout_random` 测试：随机编辑后随机 checkout 到历史版本
  - [ ] 23.3.2 验证：checkout 后的 state 等于从头重放到该版本的 state
  - [ ] 23.3.3 编写 `fork_and_merge` 测试：fork 后独立编辑，再 merge 回原文档

- [ ] **23.4 全容器混合测试**
  - [ ] 23.4.1 编写 `mixed_containers` 测试：同一文档中随机操作所有容器类型
  - [ ] 23.4.2 验证嵌套结构不变量：Map 中的 List 中的 Text 编辑后，深值正确

- [ ] **23.5 Phase 23 验收**
  - [ ] 23.5.1 运行全部 proptest，默认参数（100 cases）全部通过
  - [ ] 23.5.2 加大压力测试至 10000 cases，无失败
  - [ ] 23.5.3 运行 fmt、clippy、test，全部通过

---

## Phase 24: 性能优化与完善（持续）

**目标**: 替换初期简化实现，提升性能。
**注意**: 这些优化不改变外部行为，只改变内部实现。

- [ ] **24.1 数据结构优化**
  - [ ] 24.1.1 ListState：将 `Vec` 替换为 `BTree`（generic_btree 或类似）
  - [ ] 24.1.2 MapState：将 `BTreeMap` 评估是否替换为 `FxHashMap`（若无需有序遍历）
  - [ ] 24.1.3 TextState：将简化 List 版本替换为完整 Fugue 实现

- [ ] **24.2 编码优化**
  - [ ] 24.2.1 实现 Fast Snapshot 的增量编码（delta encoding）
  - [ ] 24.2.2 实现 ChangeStore 的块压缩（可选 LZ4）
  - [ ] 24.2.3 评估是否引入 KV-Store 持久化

- [ ] **24.3 缓存优化**
  - [ ] 24.3.1 实现 `HistoryCache`：加速 checkout 的 VV 查找
  - [ ] 24.3.2 实现 `DiffCalculator` 的 `Persist` 模式：避免重复计算
  - [ ] 24.3.3 为 TreeState 的 `children` 缓存评估 BTree 升级策略

- [ ] **24.4 文档与示例**
  - [ ] 24.4.1 为所有 `pub` 类型和函数编写 `///` doc comments
  - [ ] 24.4.2 编写 README.md：快速开始、架构说明、示例代码
  - [ ] 24.4.3 编写 examples/ 目录：基本 CRUD、协作同步、checkout 演示

---

## 附：实现原则检查清单

每个 Phase 完成后，必须运行以下检查：

- [ ] `cargo fmt --check` 通过
- [ ] `cargo clippy -- -D warnings` 通过（零警告）
- [ ] `cargo test` 全部通过
- [ ] 新代码遵循现有命名约定
- [ ] 每个 `pub` 类型/函数有 `///` doc comment
- [ ] 复杂算法有内联注释

每个 CRDT 容器（Counter/Map/List/MovableList/Text/Tree）实现完成后，额外检查：

- [ ] `to_diff` 和 `apply_diff` 互为逆操作（空状态应用 to_diff 结果 = 原状态）
- [ ] 同一 op 应用两次结果不变（idempotency）
- [ ] 两个文档执行相同操作集（不同顺序）后 frontiers 和 get_value 一致（commutativity）
- [ ] 至少 2 个并发冲突场景的测试用例
- [ ] Handler API 的 pos/key → ID 转换正确
