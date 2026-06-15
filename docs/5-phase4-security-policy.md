# Phase 4 — 安全策略实现

> **基于 roadmap.md Phase 4 定义 + 代码当前状态调研**
> 生成日期：2026-06-15
> 状态：📋 计划待实施

---

## 目录

1. [背景与目标](#1-背景与目标)
2. [当前状态分析](#2-当前状态分析)
3. [调研发现](#3-调研发现)
4. [可选方案](#4-可选方案)
5. [推荐方案](#5-推荐方案)
6. [实施建议](#6-实施建议)
7. [参考来源](#7-参考来源)

---

## 1. 背景与目标

### 问题描述

与 Go 版相比，Rust 版本缺少以下安全策略：

| # | 安全策略 | Go | Rust (当前) | 影响 |
|---|---------|----|------------|------|
| 1 | 无 WHERE 的 UPDATE/DELETE 拒绝 | ✅ panic | ❌ 无校验 | 误操作全表更新/删除 |
| 2 | `AllowUnsafeUpdate()` / `AllowUnsafeDelete()` 逃生舱 | ✅ | ❌ 缺失 | 用户无法显式放行 |
| 3 | MSSQL OFFSET 要求 ORDER BY | ✅ panic | ❌ 无校验 | 生成非法 SQL |
| 4 | SQLite RIGHT/FULL JOIN 拒绝 | ✅ | ✅ Phase 1 已实现 | — |

### 目标

- UPDATE/DELETE 默认防御全表操作，同时提供逃生舱
- MSSQL `OFFSET...FETCH` 语法强制要求 ORDER BY
- 与 Go 版安全策略 100% 对齐（但使用 Rust 惯用的 `Result` 而非 panic）

### 成功标准

1. ✅ 无 WHERE 的 `UPDATE`/`DELETE` 在 `build()` 时返回 `Err`
2. ✅ `.allow_unsafe_update()` / `.allow_unsafe_delete()` 可显式放行
3. ✅ MSSQL 的 `OFFSET`/`FETCH` 无 `ORDER BY` 时返回 `Err`
4. ✅ 非 MSSQL 后端不受影响（Postgres/MySQL/SQLite 允许无 ORDER BY 的分页）
5. ✅ 新增单元测试覆盖以上所有场景
6. ✅ `cargo build` + `cargo test` + `cargo clippy` 通过

---

## 2. 当前状态分析

### 2.1 代码调研结果

#### `result.rs` — BuildError 枚举

```rust
pub enum BuildError {
    EmptyInClause,
    NoSetClauses,
    UnsupportedJoinType(String),
    UnsupportedFeature(String),
    ModeMismatch(String),
    UnsafeColumn(String),
}
```

**现状**：已有 6 个变体，缺少安全策略相关的错误类型。

**置信度**: [高: 直接代码查看]

#### `builder.rs` — QueryBuilder 结构体

```rust
pub struct QueryBuilder {
    pub(crate) mode: QueryMode,
    pub(crate) columns: Vec<SelectExpr>,
    pub(crate) from: Vec<TableRef>,
    pub(crate) where_list: Vec<WhereGroup>,
    pub(crate) group_by: Vec<String>,
    // ... 更多字段
    pub(crate) order_by: Vec<(String, SortDir)>,
    pub(crate) limit: Option<usize>,
    pub(crate) offset: Option<usize>,
    pub(crate) update_table: Option<String>,
    pub(crate) set_list: Vec<SetClause>,
    pub(crate) delete_table: Option<String>,
    // ... 更多字段
}
```

**现状**：无 `allow_unsafe_update` / `allow_unsafe_delete` 标志位。

`build_update_query()` 当前仅校验 `set_list.is_empty()`：
```rust
fn build_update_query<B: Backend>(...) -> BuildResult<(String, Vec<Param>)> {
    if self.set_list.is_empty() {
        return Err(BuildError::NoSetClauses);
    }
    self.validate_where_list(&self.where_list)?;
    // ... 构建 SQL（WHERE 部分仅当非空时输出，但不校验）
}
```

`build_delete_query()` 当前仅校验 `where_list` 为空值问题：
```rust
fn build_delete_query<B: Backend>(...) -> BuildResult<(String, Vec<Param>)> {
    self.validate_where_list(&self.where_list)?;
    // ... 构建 SQL（WHERE 部分仅当非空时输出，但不校验）
}
```

**置信度**: [高: 直接代码查看]

#### `build_select_query()` — LIMIT/OFFSET 无 ORDER BY 校验

```rust
// 构建 ORDER BY
if !self.order_by.is_empty() { ... }

// 构建 LIMIT / OFFSET
let lo = backend.limit_offset(self.limit, self.offset);
```

**现状**：当前 MSSQL 的 `limit_offset()` 已正确输出 `OFFSET x ROWS FETCH NEXT y ROWS ONLY` 语法，但不校验 ORDER BY 是否存在。

**置信度**: [高: 直接代码查看]

#### `backend.rs` — Backend trait

```rust
pub trait Backend {
    fn placeholder(&self, i: usize) -> String;
    fn quote_ident(&self, name: &str) -> String;
    fn limit_offset(&self, limit: Option<usize>, offset: Option<usize>) -> String;
    fn returning(&self, columns: &[String]) -> String;
    fn supports_returning(&self) -> bool;
    fn supports_bulk_returning(&self) -> bool;
    fn supports_join_type(&self, _jt: JoinType) -> bool;
    fn supports_upsert(&self) -> bool;
    fn on_conflict(...) -> BuildResult<String>;
}
```

**现状**：暂无不支持 ORDER BY 的能力查询方法。

**置信度**: [高: 直接代码查看]

#### SQLite JOIN 校验

已在 Phase 1 完成——`SqliteBackend::supports_join_type()` 返回 `false` 对 `Right`/`Full`。无需额外工作。 ✅

**置信度**: [高: Phase 1 交付物验证]

### 2.2 已完成项

| 项目 | 状态 | 说明 |
|------|------|------|
| SQLite JOIN 限制 | ✅ Phase 1 | `SqliteBackend::supports_join_type()` 已拒绝 RIGHT/FULL JOIN |

### 2.3 缺失项

| 项目 | 优先级 | 复杂度 | 涉及文件 |
|------|--------|--------|---------|
| 无 WHERE UPDATE/DELETE 拒绝 | P1 | 低 | `result.rs`, `builder.rs` |
| `allow_unsafe_update/delete` 逃生舱 | P1 | 低 | `builder.rs` |
| MSSQL ORDER BY + OFFSET 校验 | P1 | 低 | `result.rs`, `backend.rs`, `mssql.rs`, `builder.rs` |

---

## 3. 调研发现

### 3.1 Go 版参考

Go 版用 panic 而非 Result，但语义等价：

```go
// Go 版：buildUpdateQuery 末尾校验
func (b *Builder) buildUpdateQuery() string {
    // ...
    if len(b.whereList) == 0 && !b.flags.allowUnsafeUpdate {
        panic("UPDATE without WHERE is not allowed; use AllowUnsafeUpdate() to bypass")
    }
    // ...
}

// Go 版：AllowUnsafeUpdate
func (b *Builder) AllowUnsafeUpdate() *Builder {
    b.flags.allowUnsafeUpdate = true
    return b
}
```

**Rust 差异**：使用 `BuildResult<T>` 返回错误而非 panic。调用方可通过 `?` 或 `.unwrap()` 灵活处理。

### 3.2 MSSQL 语法约束

MSSQL 的 `OFFSET...FETCH` 子句必须与 `ORDER BY` 子句一起使用（SQL Server 2012+ 语法约束）：

```sql
-- ✅ 合法
SELECT * FROM users ORDER BY name OFFSET 10 ROWS FETCH NEXT 5 ROWS ONLY;

-- ❌ 非法（MSSQL 报错）
SELECT * FROM users OFFSET 10 ROWS FETCH NEXT 5 ROWS ONLY;
-- "A TOP or FETCH number of rows specified must be greater than 0."
-- "OFFSET 必须配合 ORDER BY 使用"
```

Postgres/MySQL/SQLite 均不要求 ORDER BY。

**参考**：[MSSQL Documentation — ORDER BY Clause](https://learn.microsoft.com/en-us/sql/t-sql/queries/select-order-by-clause-transact-sql)

### 3.3 架构决策记录对照

| ADR | 内容 | Phase 4 相关性 |
|-----|------|---------------|
| ADR-001 | Go 版 panic → Rust 使用 Result | ✅ 直接适用——安全策略错误通过 `BuildResult` 返回 |
| ADR-002 | 列名智能引用 | 不相关 |
| ADR-004 | MSSQL MERGE 暂缓 | 不相关 |

### 3.4 前置依赖

- Phase 4 不依赖 Phase 2/3/5 的任何内容
- 可独立于其他阶段实施
- 与 Phase 1 的 `validate_where_list` / `validate_joins` 模式一致

---

## 4. 可选方案

### 4.1 无 WHERE UPDATE/DELETE 校验

#### 方案 A（推荐）：build 时校验 + flags

在 `build_update_query` / `build_delete_query` 中检查 `where_list.is_empty()`，并根据 `allow_unsafe_*` 标志决定是否放行。

**优点**：
- 与 Phase 1 的 `validate_where_list` / `validate_joins` 模式一致
- 使用 Result 而非 panic，Rust 惯用
- 构造期 API 保持 `.and_where()` 返回 `Self` 不变

**缺点**：
- 需要新增 QueryBuilder 字段（2 个 bool）

#### 方案 B：构造期立即拒绝

在 `and_where()` 等方法的调用路径上检查——但要求 State 模式或类型级区分。

**优点**：编译期拦截
**缺点**：
- 复杂度高（需要区分"已 safe 的 UPDATE"和"未 safe 的 UPDATE"类型状态）
- fluent API 链式调用中间状态复杂
- 超出"简洁优先"原则

**决策**：采用方案 A。与现有 build 时校验模式一致，改动最小。

#### 方案 C（否决）：不接受任何 bypass

没有逃生舱，必须始终有 WHERE。

**否决原因**：与 Go 版语义不一致，且有些合法场景需要全表操作（如 `TRUNCATE` 替代品、批量置默认值等）。

---

### 4.2 MSSQL OFFSET 需 ORDER BY 校验

#### 方案 A（推荐）：Backend trait 新增能力查询方法

在 `Backend` trait 上添加 `requires_order_by_for_offset() -> bool`，默认返回 `false`，`MssqlBackend` override 返回 `true`。

在 `build_select_query` 中校验：

```rust
if backend.requires_order_by_for_offset() 
    && (self.limit.is_some() || self.offset.is_some()) 
    && self.order_by.is_empty() 
{
    return Err(BuildError::OrderByRequired);
}
```

**优点**：
- 与 `supports_join_type()` / `supports_upsert()` 模式一致
- 任何未来有此约束的后端可直接 override（如某些旧版 SQL Server）
- 校验逻辑集中在 build 方法中

**缺点**：
- 新增 Backend trait 方法（但 `requires_*` 已经是既有模式）

#### 方案 B（否决）：直接硬编码到 `build_select_query`

不新增 Backend 方法，而是在 `build_select_query` 中直接判断当前后端是否为 MSSQL。

**否决原因**：`build_select_query` 的泛型参数 `B: Backend` 无法在方法内部判断具体类型。即使能用 `TypeId` 做到，也破坏了 Backend trait 的抽象层。

#### 示例

```rust
// ❌ 无法在泛型方法中判断具体类型：
fn build_select_query<B: Backend>(...) -> BuildResult<...> {
    // 无法写 if B == MssqlBackend { ... }
}
```

---

## 5. 推荐方案

### 5.1 总体设计

遵循与 Phase 1 一致的`build 时校验 + Result 返回`模式。每个安全策略校验都在 build 方法内部完成，不改变 fluent API 的链式调用模式。

### 5.2 实现路径

#### Step 1 — `result.rs`：新增 3 个错误变体

```rust
pub enum BuildError {
    // ... 现有变体 ...
    
    /// UPDATE 没有 WHERE 条件（默认拒绝，除非 allow_unsafe_update）
    UnsafeUpdateWithoutWhere,
    
    /// DELETE 没有 WHERE 条件（默认拒绝，除非 allow_unsafe_delete）
    UnsafeDeleteWithoutWhere,
    
    /// OFFSET/FETCH 需要 ORDER BY（MSSQL 语法要求）
    OrderByRequired,
}
```

Display 实现：

```rust
BuildError::UnsafeUpdateWithoutWhere => write!(f, "UPDATE without WHERE is not allowed; use allow_unsafe_update() to bypass"),
BuildError::UnsafeDeleteWithoutWhere => write!(f, "DELETE without WHERE is not allowed; use allow_unsafe_delete() to bypass"),
BuildError::OrderByRequired => write!(f, "OFFSET/FETCH requires ORDER BY; add .order_by(column, dir) before .limit()/.offset()"),
```

#### Step 2 — `backend.rs`：新增能力查询方法

```rust
pub trait Backend {
    // ... 现有方法 ...

    /// OFFSET/FETCH 是否需要 ORDER BY？
    /// MSSQL 需要（语法强制），其他后端不需要。
    fn requires_order_by_for_offset(&self) -> bool {
        false
    }
}
```

#### Step 3 — `backends/mssql.rs`：Override

```rust
impl Backend for MssqlBackend {
    // ... 现有 override ...

    fn requires_order_by_for_offset(&self) -> bool {
        true
    }
}
```

#### Step 4 — `builder.rs`：QueryBuilder 添加字段和方法

**新增字段**：

```rust
pub struct QueryBuilder {
    // ... 现有字段 ...

    // ── 安全策略 ──
    pub(crate) allow_unsafe_update: bool,
    pub(crate) allow_unsafe_delete: bool,
}
```

**构造器更新**（`fn new()`）：

```rust
Self {
    // ... 现有初始化 ...
    allow_unsafe_update: false,
    allow_unsafe_delete: false,
}
```

**新增方法**：

```rust
/// 放行无 WHERE 的 UPDATE（默认拒绝）。
pub fn allow_unsafe_update(mut self) -> Self {
    self.allow_unsafe_update = true;
    self
}

/// 放行无 WHERE 的 DELETE（默认拒绝）。
pub fn allow_unsafe_delete(mut self) -> Self {
    self.allow_unsafe_delete = true;
    self
}
```

#### Step 5 — `builder.rs`：build 方法校验

**`build_update_query`** 中新增 WHERE 校验：

```rust
fn build_update_query<B: Backend>(...) -> BuildResult<(String, Vec<Param>)> {
    if self.set_list.is_empty() {
        return Err(BuildError::NoSetClauses);
    }
    
    // ✨ Phase 4 新增：无 WHERE 拒绝
    if self.where_list.is_empty() && !self.allow_unsafe_update {
        return Err(BuildError::UnsafeUpdateWithoutWhere);
    }
    
    self.validate_where_list(&self.where_list)?;
    // ... 继续构建 ...
}
```

**`build_delete_query`** 中新增 WHERE 校验（校验顺序与 `build_update_query` 保持一致：先做安全校验，再提取 table）：

```rust
fn build_delete_query<B: Backend>(...) -> BuildResult<(String, Vec<Param>)> {
    // ✨ Phase 4 新增：无 WHERE 拒绝（优先于 table 校验，与 build_update_query 一致）
    if self.where_list.is_empty() && !self.allow_unsafe_delete {
        return Err(BuildError::UnsafeDeleteWithoutWhere);
    }
    
    self.validate_where_list(&self.where_list)?;
    let table = self.delete_table.as_deref()
        .ok_or_else(|| BuildError::ModeMismatch(...))?;
    // ... 继续构建 ...
}
```

**`build_select_query`** 中新增 MSSQL ORDER BY 校验：

```rust
fn build_select_query<B: Backend>(...) -> BuildResult<(String, Vec<Param>)> {
    // ... 现有校验 ...
    
    // ✨ Phase 4 新增：MSSQL OFFSET/FETCH 需要 ORDER BY
    if backend.requires_order_by_for_offset()
        && (self.limit.is_some() || self.offset.is_some())
        && self.order_by.is_empty()
    {
        return Err(BuildError::OrderByRequired);
    }
    
    // ... 继续构建 ...
}
```

> **注意**：校验位置应在构建 ORDER BY 和 LIMIT/OFFSET 之前（或在开头，与其他校验一起）。推荐放在函数开头，与 `validate_where_list` / `validate_joins` 放在一起，便于阅读：

```rust
fn build_select_query<B: Backend>(...) -> BuildResult<(String, Vec<Param>)> {
    self.validate_where_list(&self.where_list)?;
    self.validate_where_list(&self.having)?;
    self.validate_joins(backend)?;
    
    // Phase 4 — MSSQL ORDER BY 校验
    if backend.requires_order_by_for_offset()
        && (self.limit.is_some() || self.offset.is_some())
        && self.order_by.is_empty()
    {
        return Err(BuildError::OrderByRequired);
    }
    
    // ... 构建 SQL ...
}
```

#### Step 6 — 测试

在 `builder.rs` 的 `mod tests` 中添加以下测试：

**无 WHERE UPDATE/DELETE 校验**：

```rust
/// 验证 UPDATE 无 WHERE 返回 UnsafeUpdateWithoutWhere。
#[cfg(feature = "postgresql")]
#[test]
fn update_without_where_returns_error() {
    let result = QueryBuilder::update("users")
        .update_set("name", "bob")
        .build(&PostgresBackend);

    assert!(matches!(result, Err(BuildError::UnsafeUpdateWithoutWhere)));
}

/// 验证 UPDATE 无 WHERE + allow_unsafe_update 放行。
#[cfg(feature = "postgresql")]
#[test]
fn update_without_where_with_allow_unsafe_succeeds() {
    let result = QueryBuilder::update("users")
        .update_set("name", "bob")
        .allow_unsafe_update()
        .build(&PostgresBackend);

    assert!(result.is_ok());
    assert!(result.unwrap().sql.contains("UPDATE"));
}

/// 验证 DELETE 无 WHERE 返回 UnsafeDeleteWithoutWhere。
#[cfg(feature = "postgresql")]
#[test]
fn delete_without_where_returns_error() {
    let result = QueryBuilder::delete_from("users")
        .build(&PostgresBackend);

    assert!(matches!(result, Err(BuildError::UnsafeDeleteWithoutWhere)));
}

/// 验证 DELETE 无 WHERE + allow_unsafe_delete 放行。
#[cfg(feature = "postgresql")]
#[test]
fn delete_without_where_with_allow_unsafe_succeeds() {
    let result = QueryBuilder::delete_from("users")
        .allow_unsafe_delete()
        .build(&PostgresBackend);

    assert!(result.is_ok());
    assert!(result.unwrap().sql.contains("DELETE"));
}

/// 验证 UPDATE 有 WHERE 正常（回归测试）。
#[cfg(feature = "postgresql")]
#[test]
fn update_with_where_succeeds() {
    let result = QueryBuilder::update("users")
        .update_set("name", "bob")
        .and_where("id").eq(1)
        .build(&PostgresBackend);

    assert!(result.is_ok());
    assert!(result.unwrap().sql.contains("WHERE"));
}

/// 验证 DELETE 有 WHERE 正常（回归测试）。
#[cfg(feature = "postgresql")]
#[test]
fn delete_with_where_succeeds() {
    let result = QueryBuilder::delete_from("users")
        .and_where("id").eq(1)
        .build(&PostgresBackend);

    assert!(result.is_ok());
    assert!(result.unwrap().sql.contains("WHERE"));
}
```

**CTE + UPDATE/DELETE 场景**（YAGNI 自审确认需要覆盖）：

```rust
/// 验证 CTE + 无 WHERE UPDATE + allow_unsafe_update 放行。
#[cfg(feature = "postgresql")]
#[test]
fn cte_update_without_where_with_allow_unsafe_succeeds() {
    let cte = QueryBuilder::select(&["id"]).from("archived_users");
    let result = QueryBuilder::update("users")
        .with_cte("archived", cte)
        .update_set("status", "archived")
        .allow_unsafe_update()
        .build(&PostgresBackend);

    assert!(result.is_ok());
    assert!(result.unwrap().sql.contains("WITH"));
}

/// 验证 CTE + 无 WHERE DELETE + allow_unsafe_delete 放行。
#[cfg(feature = "postgresql")]
#[test]
fn cte_delete_without_where_with_allow_unsafe_succeeds() {
    let cte = QueryBuilder::select(&["id"]).from("expired_sessions");
    let result = QueryBuilder::delete_from("sessions")
        .with_cte("expired", cte)
        .allow_unsafe_delete()
        .build(&PostgresBackend);

    assert!(result.is_ok());
    assert!(result.unwrap().sql.contains("WITH"));
}

/// 验证 CTE + 有 WHERE UPDATE 正常（不触发安全校验）。
#[cfg(feature = "postgresql")]
#[test]
fn cte_update_with_where_succeeds() {
    let cte = QueryBuilder::select(&["id"]).from("temp_users")
        .and_where("batch").eq(1);
    let result = QueryBuilder::update("users")
        .with_cte("tmp", cte)
        .update_set("status", "active")
        .and_where("id").in_subquery(
            QueryBuilder::select(&["id"]).from_cte_ref("tmp")
        )
        .build(&PostgresBackend);

    assert!(result.is_ok());
    assert!(result.unwrap().sql.contains("WITH"));
    assert!(result.unwrap().sql.contains("WHERE"));
}
```

**MSSQL ORDER BY 校验**：

```rust
/// 验证 MSSQL OFFSET 无 ORDER BY 返回 OrderByRequired。
#[cfg(feature = "mssql")]
#[test]
fn mssql_offset_without_order_by_returns_error() {
    use crate::backends::mssql::MssqlBackend;

    let result = QueryBuilder::select(&["id"])
        .from("users")
        .limit(10)
        .build(&MssqlBackend);

    assert!(matches!(result, Err(BuildError::OrderByRequired)));
}

/// 验证 MSSQL OFFSET 有 ORDER BY 正常。
#[cfg(feature = "mssql")]
#[test]
fn mssql_offset_with_order_by_succeeds() {
    use crate::backends::mssql::MssqlBackend;

    let result = QueryBuilder::select(&["id"])
        .from("users")
        .order_by("id", SortDir::Asc)
        .limit(10)
        .build(&MssqlBackend);

    assert!(result.is_ok());
}

/// 验证 MSSQL 无 LIMIT/OFFSET 时不需要 ORDER BY。
#[cfg(feature = "mssql")]
#[test]
fn mssql_without_offset_does_not_require_order_by() {
    use crate::backends::mssql::MssqlBackend;

    let result = QueryBuilder::select(&["id"])
        .from("users")
        .build(&MssqlBackend);

    assert!(result.is_ok());
}

/// 验证 Postgres OFFSET 无需 ORDER BY（回归测试）。
#[cfg(feature = "postgresql")]
#[test]
fn postgres_offset_without_order_by_succeeds() {
    let result = QueryBuilder::select(&["id"])
        .from("users")
        .limit(10)
        .offset(5)
        .build(&PostgresBackend);

    assert!(result.is_ok());
    assert!(result.unwrap().sql.contains("LIMIT"));
}
```

### 5.3 置信度

| 项目 | 置信度 | 依据 |
|------|--------|------|
| 无 WHERE UPDATE/DELETE 校验 | [高] | 模式与 `NoSetClauses` 完全一致 |
| `allow_unsafe_*` 逃生舱 | [高] | 简单 flag 模式，与 Go 版一致 |
| MSSQL ORDER BY 校验 | [中] | 通过能力查询方法在 `build_select_query` 中校验，架构清晰；需确认 `build_select_query` 中校验位置不影响 SSELECT 流程 |
| SQLite JOIN（已完成） | [高] | Phase 1 已实现并测试 |

### 5.4 YAGNI 检查（自审第三轮）

| "以防万一"的成分 | 判断 | 理由 |
|----------------|------|------|
| 是否要为 OFFSET 单独建错误类型？ | 不用 | `OrderByRequired` 一个变体足够，不需要细分后端的错误消息 |
| 是否需要一个统一的 `UnsafeOperation` 错误？ | 不用 | `UnsafeUpdateWithoutWhere` / `UnsafeDeleteWithoutWhere` 语义清晰，各自独立更有助于调试 |
| 是否需要 `QueryBuilderFlags` 结构体？ | **否** | 两个 bool 字段直接放在 QueryBuilder 上即可。未来若 flags 增多再提取 |
| 是否需要为 allowed_unsafe 加 CTE + UPDATE/DELETE 场景测试？ | **需要** | CTE + UPDATE/DELETE 也是合法场景，测试应包括 `with_cte().allow_unsafe_update().build()` |
| 是否需要支持 `allow_unsafe_delete` 与 MSSQL ORDER BY 联动？ | 不需要 | 这是两个独立的安全策略，不互相影响 |

---

## 6. 实施建议

### 6.1 实施顺序

| 步骤 | 内容 | 文件 | 预估改行数 |
|------|------|------|-----------|
| 1 | 新增错误变体 + Display | `result.rs` | ~15 行 |
| 2 | 新增 Backend 能力查询方法 | `backend.rs` | ~5 行 |
| 3 | MSSQL override | `mssql.rs` | ~4 行 |
| 4 | QueryBuilder 新增字段 + 方法 | `builder.rs` | ~20 行 |
| 5 | build 方法校验逻辑 | `builder.rs` | ~15 行 |
| 6 | 测试用例 | `builder.rs` | ~180 行 |
| **总计** | | | **~240 行** |

### 6.2 阶段划分

建议分 2 个 wave 实施：

**Wave 1**：无 WHERE UPDATE/DELETE + `allow_unsafe_*`
- Steps 1, 4, 5 (UPDATE/DELETE 部分), 6 (相应测试)
- 可独立验证

**Wave 2**：MSSQL ORDER BY 校验
- Steps 2, 3, 5 (SELECT 部分), 6 (相应测试)
- 依赖 Step 1（需要 `OrderByRequired` 错误变体）

### 6.3 依赖关系

```
Step 1 (result.rs) ← 所有后续步骤依赖
    ├── Step 4, 5 (builder.rs UPDATE/DELETE 部分) ← Wave 1
    │
    └── Step 2 (backend.rs) → Step 3 (mssql.rs) → Step 5 (builder.rs SELECT 部分) ← Wave 2
                        ↓
                    Step 6 (测试)
```

### 6.4 注意事项

1. **MSSQL 校验位置**：`requires_order_by_for_offset` 的检查应放在 `build_select_query` 函数的**开头**与 `validate_where_list`/`validate_joins` 一起，而不是在构建 ORDER BY/LIMIT 代码段之前。这样校验逻辑集中，便于维护。

2. **CTE + UPDATE/DELETE 场景**：`build_update_query` 和 `build_delete_query` 暂时不支持 CTE + WHERE 校验，因为 CTE 不产生 WHERE 条件。但如果有 `with_cte()` 但没有 WHERE，仍然应被拒绝。这是正确的行为。

3. **`allow_unsafe_update` 与 `set_opt` 交互**：如果 `set_opt` 导致所有 SET 子句为空，`NoSetClauses` 会先于 `UnsafeUpdateWithoutWhere` 触发。这是合理的行为——语义错误优先于安全策略。

### 6.5 验证命令

```bash
# 完整编译
cargo build --all-features

# 运行所有测试
cargo test --all-features

# Clippy
cargo clippy --all-features --all-targets -- -D warnings

# 仅运行 Phase 4 新增测试
cargo test --all-features -- update_without_where
cargo test --all-features -- delete_without_where
cargo test --all-features -- mssql_offset
```

---

## 7. 参考来源

### 代码路径

| 文件 | 行数 | 作用 |
|------|------|------|
| `graft-core/src/result.rs` | 73 | BuildError 定义 |
| `graft-core/src/builder.rs` | 2853 | QueryBuilder + build 方法 + 测试 |
| `graft-core/src/backend.rs` | 90 | Backend trait 定义 |
| `graft-core/src/backends/mssql.rs` | 69 | MSSQL backend 实现 |
| `graft-core/src/backends/sqlite.rs` | 58 | SQLite backend（确认 JOIN 校验已就位） |

### 文档参考

- `docs/roadmap.md` — Phase 4 定义（§4）
- `docs/SQLQueryBuilder-Design-Memo.md` — 设计备忘（§17 安全底线）
- `docs/4-phase3-select-order-group.md` — Phase 3 设计方案（供 `supports_*` 模式参考）

### Go 版参考

- Go 版 `AllowUnsafeUpdate()` / `AllowUnsafeDelete()` 实现
- MSSQL `ORDER BY` 约束：[Microsoft Docs](https://learn.microsoft.com/en-us/sql/t-sql/queries/select-order-by-clause-transact-sql)

---

## 附录：文件改动预览

### `graft-core/src/result.rs`

```diff
 pub enum BuildError {
     EmptyInClause,
     NoSetClauses,
     UnsupportedJoinType(String),
     UnsupportedFeature(String),
     ModeMismatch(String),
     UnsafeColumn(String),
+    /// UPDATE 没有 WHERE 条件
+    UnsafeUpdateWithoutWhere,
+    /// DELETE 没有 WHERE 条件
+    UnsafeDeleteWithoutWhere,
+    /// OFFSET/FETCH 需要 ORDER BY
+    OrderByRequired,
 }
```

### `graft-core/src/backend.rs`

```diff
 pub trait Backend {
     // ... 现有方法 ...
+    
+    /// OFFSET/FETCH 是否需要 ORDER BY？
+    fn requires_order_by_for_offset(&self) -> bool { false }
 }
```

### `graft-core/src/backends/mssql.rs`

```diff
 impl Backend for MssqlBackend {
     // ... 现有 override ...
+    
+    fn requires_order_by_for_offset(&self) -> bool { true }
 }
```

### `graft-core/src/builder.rs`

```diff
 pub struct QueryBuilder {
     // ... 现有字段 ...
+    pub(crate) allow_unsafe_update: bool,
+    pub(crate) allow_unsafe_delete: bool,
 }
```

```diff
 impl QueryBuilder {
     fn new(mode: QueryMode) -> Self {
         Self {
             // ... 现有初始化 ...
+            allow_unsafe_update: false,
+            allow_unsafe_delete: false,
         }
     }
+    
+    pub fn allow_unsafe_update(mut self) -> Self { ... }
+    pub fn allow_unsafe_delete(mut self) -> Self { ... }
 }
```

```diff
 // build_update_query():
+    if self.where_list.is_empty() && !self.allow_unsafe_update {
+        return Err(BuildError::UnsafeUpdateWithoutWhere);
+    }

 // build_delete_query():
+    if self.where_list.is_empty() && !self.allow_unsafe_delete {
+        return Err(BuildError::UnsafeDeleteWithoutWhere);
+    }

 // build_select_query():
+    if backend.requires_order_by_for_offset()
+        && (self.limit.is_some() || self.offset.is_some())
+        && self.order_by.is_empty()
+    {
+        return Err(BuildError::OrderByRequired);
+    }
```

---

> **Phase 4 统计摘要**
>
> | 指标 | 值 |
> |------|----|
> | 涉及文件 | 4 个（result.rs, backend.rs, mssql.rs, builder.rs） |
> | 新增错误变体 | 3 个（`UnsafeUpdateWithoutWhere`, `UnsafeDeleteWithoutWhere`, `OrderByRequired`） |
> | 新增 Backend 方法 | 1 个（`requires_order_by_for_offset()`） |
> | 新增 QueryBuilder 字段 | 2 个（`allow_unsafe_update`, `allow_unsafe_delete`） |
> | 新增 public API 方法 | 2 个（`allow_unsafe_update()`, `allow_unsafe_delete()`） |
> | 新增测试用例 | 14 个（5 UPDATE/DELETE + 3 CTE + 4 MSSQL + 2 回归） |
> | 预估代码改动 | ~240 行 |
> | 前序依赖 | Phase 1（基础校验模式已确立）；不依赖 Phase 2/3 |
