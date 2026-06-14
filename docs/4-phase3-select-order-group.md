# Phase 3 — SELECT/ORDER/GROUP 补齐 + Backend 增强

> 编制日期：2026-06-14
> 编制人员：Proposal Agent
> 前置依赖：✅ Phase 1（安全基线）✅ Phase 2（WHERE 系统补齐）
> 置信度：[高: 代码已大部分就位，新增内容模式清晰]

---

## 目录

1. [背景与目标](#1-背景与目标)
2. [当前状态分析](#2-当前状态分析)
3. [调研发现](#3-调研发现)
4. [可选方案与决策](#4-可选方案与决策)
5. [实现方案](#5-实现方案)
6. [测试计划](#6-测试计划)
7. [实施路线图](#7-实施路线图)
8. [参考来源](#8-参考来源)

---

## 1. 背景与目标

### 1.1 问题陈述

Phase 3 的目标是补齐 SELECT、ORDER BY、GROUP BY 的**智能列名引用**与**安全特性**，以及 Backend trait 剩余的能力查询方法。

这是 roadmap 中定位为"中高"复杂度的阶段，因为列名智能引用涉及较为细致的逻辑判断。

### 1.2 成功标准

| # | 标准 | 验证方式 |
|---|------|---------|
| 1 | `select_ident()` 根据列名是否含特殊字符智能决定是否加引号 | 测试验证含 `.` 的不加引号，简单的加引号 |
| 2 | `group_by_ident()` 对分组列名做智能引用 | 测试验证输出 `GROUP BY "col"` 而非 `GROUP BY col` |
| 3 | `order_by_safe()` 实现白名单校验，越界列名返回 `Err` | 测试验证白名单外列名返回 `BuildError::UnsafeColumn` |
| 4 | `Backend::supports_upsert()` 可用，MSSQL 返回 `false` | 测试验证各后端返回值正确 |
| 5 | MySQL/MariaDB 拒绝 FULL JOIN | 测试验证 FULL JOIN 返回 `UnsupportedJoinType` |
| 6 | 现有 23 个测试 + 新增 17 个测试全通过 | `cargo test --all-features` |
| 7 | 无 clippy warning | `cargo clippy --all-features --all-targets -- -D warnings` |

### 1.3 前置状态

| 检查项 | 状态 | 置信度 |
|--------|------|--------|
| `cargo build --all-features` | ✅ 通过 | [高] |
| `cargo test --all-features` | ✅ 23 测试通过 | [高] |
| `cargo clippy --all-features --all-targets -- -D warnings` | ✅ 通过 | [高] |
| Phase 2 已完成（WHERE *Opt / 子查询比较 / 函数表达式） | ✅ 确认 | [高] |
| `is_simple_ident()` 辅助函数 | ✅ 已就位（builder.rs:938） | [高] |
| `from_as()` 方法 | ✅ 已实现（builder.rs:158） | [高] |
| `supports_join_type()` + SQLite override | ✅ 已就位（Phase 1） | [高] |

---

## 2. 当前状态分析

### 2.1 SELECT 列引用现状

当前 `select()` 方法将列名存储为 `SelectExpr::Column(String)`，build 时**原样输出**，不加任何引号：

```rust
// builder.rs:608
SelectExpr::Column(c) => sql.push_str(c),
```

这意味着 `select(&["name"])` 输出 `SELECT name` 而非 `SELECT "name"`。WHERE 条件中的列名已经通过 `is_simple_ident()` 做了智能引用（Phase 2 引入），但 SELECT 列尚未对齐。

### 2.2 GROUP BY 现状

```rust
// builder.rs:641-643
if !self.group_by.is_empty() {
    sql.push_str(" GROUP BY ");
    sql.push_str(&self.group_by.join(", "));
}
```

同样无引号处理。

### 2.3 ORDER BY 现状

```rust
// builder.rs:653-659
if !self.order_by.is_empty() {
    sql.push_str(" ORDER BY ");
    for (i, (col, dir)) in self.order_by.iter().enumerate() {
        if i > 0 { sql.push_str(", "); }
        write!(sql, "{} {}", backend.quote_ident(col), dir.sql()).unwrap();
    }
}
```

ORDER BY 已经对列名做了引用（`backend.quote_ident(col)`），但**缺少白名单校验**。

### 2.4 Backend 能力查询

| 方法 | 状态 | 备注 |
|------|------|------|
| `supports_returning()` | ✅ 已实现 | 各后端均有 override |
| `supports_bulk_returning()` | ✅ 已实现 | 各后端均有 override |
| `supports_join_type()` | ✅ 已实现 | SQLite override 已就位 |
| `supports_upsert()` | ❌ 缺失 | 需新增 |

### 2.5 文件涉及范围评估

| 文件 | 改动类型 | 预期影响 |
|------|---------|---------|
| `graft-core/src/builder.rs` | 新增 + 修改 | `SelectExpr` 新变体、`select_ident()`、`group_by_ident()` + flag、`order_by_safe()`、build 逻辑适配 |
| `graft-core/src/types.rs` | 无修改 | — |
| `graft-core/src/result.rs` | 修改 | 新增 `BuildError::UnsafeColumn` 变体 |
| `graft-core/src/backend.rs` | 修改 | 新增 `supports_upsert()` 默认方法 |
| `graft-core/src/backends/mysql.rs` | 修改 | 新增 `supports_join_type()` override 拒绝 FULL |
| `graft-core/src/backends/mariadb.rs` | 修改 | 新增 `supports_join_type()` override 拒绝 FULL |
| `graft-core/src/backends/mssql.rs` | 修改 | 新增 `supports_upsert()` override 返回 false |
| `graft-core/src/backends/postgres.rs` | 无需修改 | 使用默认实现 |
| `graft-core/src/backends/sqlite.rs` | 无需修改 | 已 override |

---

## 3. 调研发现

### 3.1 现有基础设施

Phase 2 已经做了大量铺垫工作：

**`is_simple_ident()` 函数**（builder.rs:938-940）：
```rust
fn is_simple_ident(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_')
}
```

该函数在 WHERE 条件构建中已投入使用——简单列名加引号，复杂表达式（如 `UPPER(name)`）维持原样。

**ADR-002 决策**：采用简化策略——仅当列名包含 `.`、`(`、`)`、空格或空字符串时不加引号，否则加引号。不维护完整 SQL 关键字列表。

### 3.2 Go 版参考

Go 版 `SelectIdent()` 在 `op.go` 中用 `wrapIdentIfNeeded()` 实现智能引用，包含约 150 个 SQL 关键字的 map。Rust 版按 ADR-002 采用简化策略。

Go 版 `OrderBySafe()` 在白名单外时 panic，Rust 版按 ADR-001 返回 `BuildResult`。

### 3.3 各后端 FULL JOIN 支持情况

| 后端 | FULL JOIN | 当前行为 | 目标行为 |
|------|-----------|---------|---------|
| Postgres | ✅ 支持 | 默认允许 | 保持不变 |
| MySQL | ❌ 不支持 | 默认允许（静默生成非法 SQL） | 返回 `UnsupportedJoinType` |
| MariaDB | ❌ 不支持 | 默认允许（同 MySQL） | 返回 `UnsupportedJoinType` |
| MSSQL | ✅ 支持 | 默认允许 | 保持不变 |
| SQLite | ❌ 3.35.0 前 | 拒绝 ✅（Phase 1） | 保持不变 |

---

## 4. 可选方案与决策

### 4.1 SelectIdent — 实现策略

#### 方案 A（推荐）：新增 `SelectExpr::Ident` 变体
在 `SelectExpr` 枚举中新增 `Ident(String)` 变体，`select_ident()` 用此变体存储列名，build 时做智能引用。

**优点**：
- 与现有 `select()` 行为完全正交，不相互影响
- 允许未来混用（虽不常见但语法正确）

**缺点**：
- 需要新增枚举变体并在 match 中处理
- 枚举变体数增加

**置信度**: [高: 模式清晰，build_where_list 已有同类逻辑]

#### 方案 B：复用 `Column` + 全局标志位
添加一个 `select_ident: bool` 标志，`select_ident()` 设置标志，build 时根据标志决定是否对所有 SELECT 列做智能引用。

**优点**：
- 不新增枚举变体

**缺点**：
- 不能混用 `select()` 和 `select_ident()`——设置标志后所有列都受影响
- 不易扩展

**决策**：采用**方案 A**。枚举变体是 Rust 表达"OR"关系的自然方式，新增 `Ident` 变体语义清晰。

### 4.2 GroupByIdent — 实现策略

#### 方案 A（推荐）：添加 `group_by_ident: bool` 标志
在 `QueryBuilder` 中添加 `group_by_ident: bool` 字段，默认 `false`。`group_by_ident()` 设置标志为 `true`。build 时检查标志，对每列做智能引用。

**优点**：
- 最小化存储变化（不新增 Vec 字段）
- 模式简单

**缺点**：
- 不能混用 `group_by()` 和 `group_by_ident()`
- 额外的 bool 字段

**置信度**: [高: 简单 bool 标志，build 时分支判断]

#### 方案 B：新的 `Vec<GroupByExpr>` 枚举
创建新的枚举类型来区分"裸列名"和"智能列名"。

**优点**：
- 更灵活

**缺点**：
- 过度工程——GROUP BY 列几乎总是列引用，罕见表达式场景

**决策**：**方案 A**。符合 YAGNI 原则。

### 4.3 OrderBySafe — 实现策略

#### 方案 A（推荐）：返回 `BuildResult<Self>`
```rust
pub fn order_by_safe(self, column: &str, dir: SortDir, whitelist: &[&str]) -> BuildResult<Self>
```

白名单校验失败返回 `Err(BuildError::UnsafeColumn(...))`。

**优点**：
- 遵循 ADR-001（Rust 用 Result 而非 panic）
- 调用方灵活处理（`?` 或 `match`）

**缺点**：
- 中断 fluent chain（需要 `let qb = qb.order_by_safe(...)?;`）

**置信度**: [高: 模式明确]

#### 方案 B：panic（与 Go 一致）
```rust
pub fn order_by_safe(self, column: &str, dir: SortDir, whitelist: &[&str]) -> Self
```

**缺点**：
- 违反 ADR-001
- panic 在 Rust 中不合适

**决策**：**方案 A**。Rust 应使用 `Result`。

### 4.4 supports_upsert — 实现策略

直接添加默认方法，MSSQL override 返回 `false`。

**置信度**: [高: 模式与 `supports_join_type` 完全相同]

### 4.5 MySQL/MariaDB FULL JOIN 拒绝

直接在 `MysqlBackend` 和 `MariaDbBackend` 中 override `supports_join_type()`。

**置信度**: [高: SQLite 已有完全相同的模式]

---

## 5. 实现方案

### 5.1 新增错误类型

**文件**：`graft-core/src/result.rs`

在 `BuildError` 枚举中添加：

```rust
#[derive(Debug, Clone)]
pub enum BuildError {
    EmptyInClause,
    NoSetClauses,
    UnsupportedJoinType(String),
    UnsupportedFeature(String),
    ModeMismatch(String),
    /// ORDER BY 或类似上下文中使用了不在白名单内的列名
    UnsafeColumn(String),  // ← 新增
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // ... 现有变体 ...
            BuildError::UnsafeColumn(col) => {
                write!(f, "column not in whitelist: {col}")
            }
        }
    }
}
```

### 5.2 SelectIdent

#### 5.2.1 新增枚举变体

**文件**：`graft-core/src/builder.rs`

```rust
pub enum SelectExpr {
    Column(String),
    /// 智能引用列名——简单标识符加引号，复杂表达式（含 `.`、`()` 等）不加引号。
    Ident(String),      // ← 新增
    Subquery(Box<QueryBuilder>, String),
    Raw(String),
}
```

#### 5.2.2 新增方法

```rust
impl QueryBuilder {
    /// 智能列名 SELECT。
    ///
    /// 简单标识符（仅字母数字下划线）将被后端引用，
    /// 含 `.`、`()`、空格等的表达式则不加引号。
    ///
    /// **注意**：带 `AS` 别名的列名作为整体处理，不会分别引用两侧。
    /// 例如 `select_ident(&["name AS user_name"])` 因含空格整体原样输出。
    /// 如需分别引用，请用 `select(&["\"name\" AS \"user_name\""])` 自行处理。
    ///
    /// ```rust
    /// use graft_core::QueryBuilder;
    /// let qb = QueryBuilder::select_ident(&["users.name", "age", "UPPER(email) AS email_upper"])
    ///     .from("users");
    /// # let _ = qb;
    /// ```
    pub fn select_ident(mut self, columns: &[&str]) -> Self {
        self.columns = columns
            .iter()
            .map(|c| SelectExpr::Ident(c.to_string()))
            .collect();
        self
    }
}
```

#### 5.2.3 Build 适配

在 `build_select_query` 中，`SelectExpr::Ident` 分支：

```rust
match col {
    SelectExpr::Column(c) => sql.push_str(c),
    SelectExpr::Ident(c) => {
        // 智能引用：简单列名 → quote_ident，否则原样输出
        if Self::is_simple_ident(c) {
            write!(sql, "{}", backend.quote_ident(c)).unwrap();
        } else {
            sql.push_str(c);
        }
    }
    SelectExpr::Subquery(sub, alias) => { /* 不变 */ }
    SelectExpr::Raw(r) => sql.push_str(r),
}
```

### 5.3 GroupByIdent

#### 5.3.1 新增字段

**文件**：`graft-core/src/builder.rs`

```rust
pub struct QueryBuilder {
    // ... 现有字段 ...
    pub(crate) group_by: Vec<String>,
    pub(crate) group_by_ident: bool,  // ← 新增，默认 false
    // ...
}
```

在 `fn new()` 中初始化为 `false`。

#### 5.3.2 新增方法

```rust
impl QueryBuilder {
    /// 智能 GROUP BY。
    ///
    /// 对列名做智能引用（与 `select_ident` 规则一致）。
    pub fn group_by_ident(mut self, columns: &[&str]) -> Self {
        self.group_by = columns.iter().map(|c| c.to_string()).collect();
        self.group_by_ident = true;
        self
    }
}
```

#### 5.3.3 修复：`group_by()` 需重置标志

**Bug**：链式调用 `group_by_ident().group_by()` 时，`group_by_ident` 标志残留为 `true`，导致后续的 `group_by()` 列也被智能引用。

**修复**：在现有的 `group_by()` 方法中重置标志：

```rust
pub fn group_by(mut self, columns: &[&str]) -> Self {
    self.group_by = columns.iter().map(|c| c.to_string()).collect();
    self.group_by_ident = false;  // ← 重置标志，确保行为由最后一次调用决定
    self
}
```

**影响**：这是对已有方法的微小修改，不影响现有行为（`group_by()` 原本未设置该标志，`false` 是默认值）。新增的 `group_by_ident()` 保持不变。

**置信度**: [高: 纯防御性编程，不影响任何现有测试]

#### 5.3.4 Build 适配

```rust
// 6. GROUP BY
if !self.group_by.is_empty() {
    sql.push_str(" GROUP BY ");
    if self.group_by_ident {
        let quoted: Vec<String> = self.group_by.iter().map(|c| {
            if Self::is_simple_ident(c) {
                backend.quote_ident(c)
            } else {
                c.clone()
            }
        }).collect();
        sql.push_str(&quoted.join(", "));
    } else {
        sql.push_str(&self.group_by.join(", "));
    }
}
```

### 5.4 OrderBySafe

#### 5.4.1 新增方法

```rust
impl QueryBuilder {
    /// 白名单校验的 ORDER BY。
    ///
    /// 仅当 `column` 在 `whitelist` 中时才允许排序。
    /// 否则返回 `Err(BuildError::UnsafeColumn)`。
    ///
    /// # 用法
    ///
    /// **在返回 `Result` 的函数中**（推荐，使用 `?` 运算符）：
    ///
    /// ```rust
    /// # use graft_core::{QueryBuilder, SortDir, BuildResult, QueryResult};
    /// # fn example() -> BuildResult<QueryResult> {
    /// let result = QueryBuilder::select(&["id"]).from("users")
    ///     .and_where("status").eq("active")
    ///     .order_by_safe("name", SortDir::Asc, &["name", "id", "email"])?
    ///     .build(&graft_core::backends::postgres::PostgresBackend)?;
    /// # Ok(result)
    /// # }
    /// ```
    ///
    /// **在非 Result 上下文中**（使用 `.unwrap()`，信任白名单）：
    ///
    /// ```rust
    /// use graft_core::{QueryBuilder, SortDir};
    /// let qb = QueryBuilder::select(&["id"]).from("users")
    ///     .order_by_safe("name", SortDir::Asc, &["name", "id", "email"])
    ///     .unwrap();
    /// # let _ = qb;
    /// ```
    ///
    /// **与 `when()` 守卫配合**：
    ///
    /// ```rust
    /// # use graft_core::{QueryBuilder, SortDir, BuildResult, QueryResult};
    /// # fn example(sort_col: Option<&str>) -> BuildResult<QueryResult> {
    /// let qb = QueryBuilder::select(&["id"]).from("users");
    /// let qb = if let Some(col) = sort_col {
    ///     qb.order_by_safe(col, SortDir::Asc, &["name", "id"])?
    /// } else {
    ///     qb
    /// };
    /// let result = qb.build(&graft_core::backends::postgres::PostgresBackend)?;
    /// # Ok(result)
    /// # }
    /// ```
    pub fn order_by_safe(self, column: &str, dir: SortDir, whitelist: &[&str]) -> BuildResult<Self> {
        if !whitelist.contains(&column) {
            return Err(BuildError::UnsafeColumn(column.to_string()));
        }
        Ok(self.order_by(column, dir))
    }
}
```

### 5.5 Backend::supports_upsert

#### 5.5.1 Trait 新增方法

**文件**：`graft-core/src/backend.rs`

```rust
pub trait Backend {
    // ... 现有方法 ...

    /// 后端是否支持 UPSERT（ON CONFLICT / ON DUPLICATE KEY）。
    ///
    /// MSSQL 不支持此语法（需使用 MERGE），返回 `false`。
    fn supports_upsert(&self) -> bool {
        true
    }
}
```

#### 5.5.2 MSSQL override

**文件**：`graft-core/src/backends/mssql.rs`

```rust
impl Backend for MssqlBackend {
    // ... 现有 override ...

    fn supports_upsert(&self) -> bool {
        false   // MSSQL 使用 MERGE，不支持 ON CONFLICT 语法
    }
}
```

### 5.6 MySQL/MariaDB FULL JOIN 拒绝

#### 5.6.1 MySQL

**文件**：`graft-core/src/backends/mysql.rs`

```rust
use crate::types::JoinType;  // 可能需要添加

impl Backend for MysqlBackend {
    // ... 现有 override ...

    fn supports_join_type(&self, jt: JoinType) -> bool {
        !matches!(jt, JoinType::Full)  // MySQL 不支持 FULL OUTER JOIN
    }
}
```

#### 5.6.2 MariaDB

**文件**：`graft-core/src/backends/mariadb.rs`

```rust
use crate::types::JoinType;  // 可能需要添加

impl Backend for MariaDbBackend {
    // ... 现有 override ...

    fn supports_join_type(&self, jt: JoinType) -> bool {
        !matches!(jt, JoinType::Full)  // MariaDB 不支持 FULL OUTER JOIN
    }
}
```

### 5.7 public API re-export 确认

**文件**：`graft-core/src/lib.rs`

无需修改——`SelectExpr` 通过 `pub use types::*` 已导出，新增变体自动暴露。

`BuildError::UnsafeColumn` 通过 `pub use result::{BuildError, BuildResult, QueryResult}` 已导出。

---

## 6. 测试计划

### 6.1 新增测试用例

| # | 测试 | 模块 | 覆盖 |
|---|------|------|------|
| 1 | `select_ident_quotes_simple_column` | builder | 简单列名被 `quote_ident` 包裹 |
| 2 | `select_ident_does_not_quote_complex` | builder | `table.col` / `UPPER(x)` 不加引号 |
| 3 | `select_ident_mixed_with_select` | builder | 确认 `select()` 不受影响（回归） |
| 4 | `group_by_ident_quotes_columns` | builder | GROUP BY 列被引用 |
| 5 | `group_by_ident_does_not_quote_expression` | builder | 函数表达式在 GROUP BY 中不加引号 |
| 6 | `group_by_legacy_unchanged` | builder | 原有 `group_by()` 行为不变 |
| 7 | `order_by_safe_allows_whitelisted` | builder | 白名单内列名正常排序 |
| 8 | `order_by_safe_rejects_unlisted` | builder | 白名单外列名返回 `UnsafeColumn` 错误 |
| 9 | `supports_upsert_mssql_false` | backend | MSSQL 返回 false |
| 10 | `supports_upsert_pg_true` | backend | Postgres 返回 true |
| 11 | `mysql_rejects_full_join` | backend | MySQL 拒绝 FULL JOIN |
| 12 | `mariadb_rejects_full_join` | backend | MariaDB 拒绝 FULL JOIN |
| 13 | `mysql_accepts_inner_join` | backend | MySQL 接受 INNER JOIN（正例） |
| 14 | `select_ident_empty_column` | builder | 空列名不 panic，原样输出 |
| 15 | `group_by_ident_then_group_by_resets_flag` | builder | 链式 `group_by_ident().group_by()` 后 flag 被重置 |
| 16 | `order_by_safe_dotted_column` | builder | 点号列名 `users.name` 白名单匹配工作 |
| 17 | `select_ident_with_as_alias_raw` | builder | `name AS alias` 因含空格原样输出不引用 |

### 6.2 测试设计要点

- 所有新增测试使用 `#[cfg(feature = "postgresql")]`（或对应后端 feature）
- `order_by_safe` 测试验证 `BuildResult::Err` 而非 panic
- `select_ident` 测试验证 SQL 字符串包含 `"name"` 而非 `name`
- 回归测试确保现有 `select()` 行为不受影响

---

## 7. 实施路线图

### Wave 1 — 基础设施（低风险）

| 步骤 | 文件 | 变更 | 验证 |
|------|------|------|------|
| 1.1 | `result.rs` | 新增 `UnsafeColumn` 错误变体 | `cargo build` |
| 1.2 | `backend.rs` | 新增 `supports_upsert()` 默认方法 | `cargo build` |
| 1.3 | `mssql.rs` | override `supports_upsert()` = false | `cargo test --features mssql` |
| 1.4 | `mysql.rs` | override `supports_join_type()` 拒绝 FULL | `cargo test --features mysql` |
| 1.5 | `mariadb.rs` | override `supports_join_type()` 拒绝 FULL | `cargo test --features mariadb` |

### Wave 2 — 核心功能（中风险）

| 步骤 | 文件 | 变更 | 验证 |
|------|------|------|------|
| 2.1 | `builder.rs` | `SelectExpr::Ident` 枚举变体 | `cargo build` |
| 2.2 | `builder.rs` | `select_ident()` 方法 + build 适配 | 单元测试 |
| 2.3 | `builder.rs` | `group_by_ident` 字段 + `group_by_ident()` 方法 + build 适配 | 单元测试 |
| 2.4 | `builder.rs` | `order_by_safe()` 方法 | 单元测试 |

### Wave 3 — 测试 + 最终验证

| 步骤 | 内容 | 验证 |
|------|------|------|
| 3.1 | 编写 17 个测试用例（13 核心 + 4 边界） | `cargo test --all-features` |
| 3.2 | `cargo clippy --all-features --all-targets -- -D warnings` | 零 issue |
| 3.3 | `cargo fmt -- --check` | 格式正确 |
| 3.4 | 回归验证——23 个旧测试 + 17 个新测试均通过 | 40 个测试全绿 |

---

## 8. 参考来源

### 代码位置

| 符号 | 文件:行 | 说明 |
|------|---------|------|
| `SelectExpr` | `graft-core/src/builder.rs:61` | 需新增 `Ident` 变体 |
| `is_simple_ident()` | `graft-core/src/builder.rs:938` | 核心辅助函数，已就位 |
| `build_select_query()` | `graft-core/src/builder.rs:582` | 需修改 SELECT 列处理 |
| `group_by` build | `graft-core/src/builder.rs:641` | 需修改 GROUP BY 处理 |
| `order_by` build | `graft-core/src/builder.rs:653` | 无需改动 |
| `BuildError` | `graft-core/src/result.rs:44` | 需新增 `UnsafeColumn` |
| `Backend::supports_join_type` | `graft-core/src/backend.rs:48` | 参考模式 |
| `SqliteBackend` | `graft-core/src/backends/sqlite.rs:31` | FULL JOIN 拒绝参考实现 |

### 文档引用

| 文档 | 章节 | 内容 |
|------|------|------|
| `roadmap.md` | Phase 3 | SELECT/ORDER/GROUP 补齐 + Backend 增强 |
| `roadmap.md` | ADR-001 | panic vs Result 决策 |
| `roadmap.md` | ADR-002 | 列名智能引用策略 |
| `SQLQueryBuilder-Design-Memo.md` | §4 Backend Trait | `supports_*` 方法模式 |

---

## 附录 A：改动文件清单与估算

| 文件 | 改动类型 | 新增/修改行数估计 | 风险 |
|------|---------|-------------------|------|
| `graft-core/src/result.rs` | 修改 + 新增 | +4 行（错误变体 + Display） | 低 |
| `graft-core/src/backend.rs` | 修改 | +5 行（`supports_upsert`） | 低 |
| `graft-core/src/builder.rs` | 修改 + 新增 | +50 行（SelectExpr 变体 + 3 个方法 + build 适配） | 中 |
| `graft-core/src/backends/mysql.rs` | 修改 | +5 行（`supports_join_type`） | 低 |
| `graft-core/src/backends/mariadb.rs` | 修改 | +5 行（`supports_join_type`） | 低 |
| `graft-core/src/backends/mssql.rs` | 修改 | +4 行（`supports_upsert`） | 低 |
| **总计** | 6 个文件 | ~73 行 | 中低 |

## 附录 B：Adversarial Check 记录

### 自审 #1 — 假设审视

| 假设 | 验证 | 风险 |
|------|------|------|
| `SelectExpr::Ident` 新增不会破坏现有 match | builder.rs 仅 1 处 match `SelectExpr`，已检查所有分支 | 低 |
| `group_by_ident` bool 不影响 `group_by()` 行为 | ⚠️ **Arch Review 发现 Bug**：`group_by()` 需重置 flag 防止残留 | **已修复** ✅ |
| `order_by_safe` 返回 `BuildResult` 不破坏现有 API | 新增方法，无侵入 | 低 |
| `supports_upsert` 默认 true 不会影响 MSSQL | MSSQL 有 override | 低 |

### 自审 #2 — 否决复核

| 否决项 | 原因 | 重新评估 |
|--------|------|---------|
| 方案 B（`SelectExpr::Column` + 标志位） | 无法混用 | 维持方案 A——枚举变体更干净 |
| 方案 B（`GroupByExpr` 枚举） | 过度工程 | 维持 bool 标志 |
| `order_by_safe` panic | 违反 ADR-001 | 维持 `BuildResult` |
| 完整的 `not_in()` 方法 | 不在 Phase 3 范围 | 推迟至 Phase 5 |

### 自审 #3 — YAGNI 检查

| 拟加入功能 | YAGNI 判断 | 处置 |
|-----------|-----------|------|
| `SelectExpr::Ident` | ✅ 必要——明确区分"裸列"和"智能列" | 保留 |
| `group_by_ident` bool | ✅ 最小设计 | 保留（已修复 flag 残留 bug） |
| `supports_upsert` 未来使用 | ⚠️ 当前暂未在 build 中消费，但 Backend trait 是为框架性设计 | 保留——API 一致性 |
| MySQL FULL JOIN 拒绝 | ✅ build 时校验是安全要求，不应静默生成非法 SQL | 保留 |

### 附录 C：Architecture Review 发现与修复

| # | 类型 | 问题 | 修复方式 | 状态 |
|---|------|------|---------|------|
| 1 | 🔴 Bug | `group_by()` 不重置 `group_by_ident` 标志，链式调用残留 | `group_by()` 中显式设为 `false` | ✅ 已修复 |
| 2 | 🟡 文档 | `select_ident` 的 `AS 别名` 行为不直观（整体不做解析） | 文档注释补充说明 + 示例 | ✅ 已修复 |
| 3 | 🟡 API | `order_by_safe` 返回 Result 中断 fluent chain，`when()` 守卫不易配合 | 文档补充 3 种用法模式（`?` / `unwrap` / `if let`） | ✅ 已修复 |
| 4 | 🟢 测试 | 缺少空列名、flag 残留、点号白名单、AS 别名 4 个边界测试 | 补充到测试计划 | ✅ 已修复 |
