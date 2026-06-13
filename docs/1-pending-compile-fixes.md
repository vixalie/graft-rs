# 遗留任务：编译错误修复清单

> 创建日期：2026-06-12
> 背景：项目 scaffolding 完成后 `cargo build` 失败（约 25 个错误 + 若干警告）

---

## 状态总览

| Crate | 状态 | 说明 |
|-------|------|------|
| `graft-core` | ❌ 编译失败 | 所有权借用、edition 2024 兼容性问题 |
| `graft-derive` | ❌ 编译失败 | API 用法错误 |
| `graft` | ⏸️ 依赖上游 | 上游修通即可编译 |

---

## 1. 已修复的问题

| # | 问题 | 文件 | 修复方式 |
|---|------|------|----------|
| 1 | `))` 多余的闭合括号 | `builder.rs:925` | 删除一个 `)` |
| 2 | `SelectExpr` 枚举定义在 `impl QueryBuilder` 内部 | `builder.rs:62` | 移至模块作用域 |
| 3 | `impl Backend { fn on_conflict }` 在 builder.rs 中 | `builder.rs:1700` | 移至 `backend.rs` 作为 trait 默认方法 |
| 4 | `HasWhere` impl 手动逐字段 `std::mem::take` | `builder.rs:1420+` | 统一改为 `std::mem::take(self)`，并添加 `Default` impl |

---

## 2. 待修复的错误

### 2.1 所有权：Join 方法中 borrow-after-move

**涉及文件**: `graft-core/src/builder.rs`

**问题**: `join()`、`left_join()`、`right_join()`、`full_join()`、`join_subquery()`、`join_cte()` 方法将 `self` 移入 `JoinAdder` 后再访问 `self.joins.len()`。

```rust
// 当前（错误）
pub fn join(mut self, table: &str, alias: &str) -> JoinAdder<Self> {
    self.joins.push(JoinClause { ... });
    JoinAdder {
        target: self,           // self 已移走
        join_idx: self.joins.len() - 1,  // ❌ 借用已移走的值
    }
}
```

**修复方法**: 在 `JoinAdder { target: self, ... }` 之前先计算 `join_idx`：

```rust
pub fn join(mut self, table: &str, alias: &str) -> JoinAdder<Self> {
    self.joins.push(JoinClause { ... });
    let join_idx = self.joins.len() - 1;  // ✅ 先计算
    JoinAdder { target: self, join_idx }
}
```

**涉及方法清单**:
- `join` (line ~260)
- `left_join` (line ~274)
- `right_join` (line ~288)
- `full_join` (line ~302)
- `join_subquery` (line ~327)
- `join_cte` (line ~341)

---

### 2.2 所有权：WhereAdder 方法中 borrow-after-move

**涉及文件**: `graft-core/src/builder.rs` (WhereAdder impl)

**问题**: `eq`、`ne`、`gt`、`gte`、`lt`、`lte`、`like`、`in_`、`in_subquery`、`is_null`、`is_not_null`、`between`、`eq_col` 等方法中调用 `self.add_cond(...)` 后访问 `self.column`。

```rust
// 当前（错误）
pub fn eq(self, val: impl Into<Param>) -> T {
    self.add_cond(WhereKind::Column {
        column: self.column,  // ❌ 在 add_cond(self) 之后使用 self.column
        op: CmpOp::Eq,
        value: Expr::Value(val.into()),
    })
}
```

**修复方法**: 在使用 `self` 之前克隆字段：

```rust
pub fn eq(self, val: impl Into<Param>) -> T {
    let column = self.column.clone();  // ✅ 提前克隆
    self.add_cond(WhereKind::Column {
        column,
        op: CmpOp::Eq,
        value: Expr::Value(val.into()),
    })
}
```

**涉及方法清单**（12 个）:
- `eq`、`ne`、`gt`、`gte`、`lt`、`lte`
- `like`
- `in_`、`in_subquery`
- `is_null`、`is_not_null`
- `between`、`eq_col`

---

### 2.3 Edition 2024：`ref` 绑定修饰符

**涉及文件**: `builder.rs` + `backend.rs`

**问题**: Rust edition 2024 不允许在匹配引用类型时显式使用 `ref`。当 match 表达式操作的是引用 `&_` 且模式使用非引用模式时，绑定的内容已隐式借用，不需要也不允许 `ref`。

```rust
// 当前（错误）— edition 2024
ConflictAction::DoUpdate { ref set, .. } => set.clone(),
                         // ^^^ ❌ 不允许显式 ref
ConflictAction::DoUpdate { ref set_excluded, .. } => {
                         // ^^^ ❌ 同上
```

**修复方法**: 删除所有 `ref` 关键字：

```rust
// ✅ edition 2024
ConflictAction::DoUpdate { set, .. } => set.clone(),
ConflictAction::DoUpdate { set_excluded, .. } => {
```

**涉及位置清单**:
- `builder.rs:736` — `ref set`
- `backend.rs:58` — `ref set_excluded`
- 可能还有其他匹配 `ConflictAction` 或引用类型的地方

---

### 2.4 RowCollector 迭代类型不匹配

**涉及文件**: `builder.rs` (RowCollector impl, ~line 1623)

**问题**: `v.into()` 中的 `v` 类型是 `&impl Into<Param>`（通过引用迭代），但 `Into<Param>` 未为 `&T` 实现。

```rust
// 当前（错误）
let params: Vec<Param> = vals.iter().map(|v| v.into()).collect();
```

**修复方法**: 使用引用转换或在迭代时解引用：

```rust
// 方案 A：手动 clone + into
let params: Vec<Param> = vals.iter().map(|v| {
    // 每个元素实现 Into<Param> + Clone
    let inner: &impl Into<Param> = v;
    // 需要 Into 用于 &T 或 clone 后 into
}).collect();

// 方案 B：修改参数签名，要求 Clone
// pub fn row(mut self, vals: &[impl Into<Param> + Clone]) -> Self {
//     let params: Vec<Param> = vals.iter().map(|v| v.clone().into()).collect();
```

---

### 2.5 WhereAdder 方法缺少 `mut self`

**涉及文件**: `builder.rs` (WhereAdder impl)

**问题**: `.raw()` 和 `add_cond()` 调用 `self.target.add_where_raw()` 和 `self.target.add_where()`（需要 `&mut self`），但 `self` 未声明为 `mut`。

```rust
// 当前（错误）
pub fn raw(self, sql: &str, params: Vec<Param>) -> T {
    self.target.add_where_raw(self.logic, sql, params)
    // ❌ self.target 不可变借用

fn add_cond(self, kind: WhereKind) -> T {
    self.target.add_where(self.logic, kind)
    // ❌ self.target 不可变借用
```

**修复方法**: 将两个方法改为 `mut self`：

```rust
pub fn raw(mut self, sql: &str, params: Vec<Param>) -> T {
fn add_cond(mut self, kind: WhereKind) -> T {
```

---

### 2.6 graft-derive 编译错误

**涉及文件**: `graft-derive/src/lib.rs:73`

**问题**: `attr.path().get_ident()` 返回 `Option<&Ident>`，但代码用 `if let Ok(ident) = ...` 当作 `Result` 处理。

```rust
// 当前（错误）
if let Ok(ident) = attr.path().get_ident() {
    // ❌ get_ident() 返回 Option，不是 Result
```

**修复方法**: 改为 `if let Some(ident) = ...`

```rust
if let Some(ident) = attr.path().get_ident() {
```

---

## 3. 警告（warnings）

### 3.1 Unused `mut`

- `builder.rs` 中 `and_where` / `or_where` 方法（GroupBuilder 和 QueryBuilder 上的）
- `OnGroupBuilder::or_on` 的 `mut self`
- 修复方法：删除 `mut` 或将方法体改为需要 `mut` 的操作

### 3.2 Unused variables

- `builder.rs:699` — 变量 `per_row` 定义了但未使用
- `backend.rs:48` — 参数 `set` 在 `on_conflict` 默认实现中未使用
- `backend.rs:74` — 参数 `idx` 在 `build_ctes` 中未使用
- 修复方法：前缀加 `_` 或实现功能

---

## 4. 修复策略

### 推荐修复顺序

1. **edition 2024 `ref` 问题**（2.3）— 影响范围最小，两处修改
2. **Join 方法 borrow-after-move**（2.1）— 6 处同名改动，模式一致
3. **WhereAdder 方法 borrow-after-move**（2.2）— 13 处改动，模式一致
4. **`raw`/`add_cond` `mut self`**（2.5）— 2 处改动
5. **RowCollector 类型问题**（2.4）— 1 处改动
6. **derive crate 编译错误**（2.6）— 1 处改动
7. **警告清理**（3）— 前缀 `_` 或删除 `mut`

### 验证方法

```bash
cargo build                    # 基础编译检查
cargo build --all-features     # 全特性编译
cargo clippy                   # lint 检查
cargo test                     # 测试验证
```

---

## 5. 可能的设计优化（非阻塞）

以下问题不阻塞编译，但值得在修复阶段一并进行：

| 问题 | 说明 |
|------|------|
| `Backend::build_ctes` 默认实现 | 当前输出了 `"..."` 占位符，需实际使用 `CteBody` |
| `InsertBuilder` 与 `QueryBuilder` 分离 | 当前 INSERT 逻辑散落在 QueryBuilder 的 `build_insert_query` 中，分离成独立类型更清晰（参考 RowCollector 模式） |
| `Having` 支持 | `having()` 方法当前退化为 `and_where()`，需独立实现 |
| `Like` 运算符 | 当前 `like()` 用 `RawExpr` 模拟，应成为真正的 `WhereKind` 变体 |
| `NOT IN` 支持 | 当前 `WhereKind::In` 的 `negated` 字段在前端无暴露方法 |

---

## 附录：完整错误信息参考

第一次 `cargo build` 输出共 **32 个错误 + 9 个警告**，分类如上。关键编译器错误代码：

- `E0382` — borrow of moved value（占最多，约 15 个）
- `E0433` — use of undeclared type `SelectExpr`（已修复）
- `E0425` — cannot find type（已修复）
- `E0596` — cannot borrow as mutable（`raw` 缺少 `mut self`）
- `E0277` — trait bound unsatisfied（RowCollector 类型问题）
- edition 2024 — `ref` binding modifier not allowed
