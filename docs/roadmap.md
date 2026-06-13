# sql-query-builder (graft) 开发规划

> 基于 Go 版本 (graft) 的功能对等分析，结合 Rust 语言特性，制定阶段性开发路线
> 生成日期：2026-06-13
> 最近更新：2026-06-14 — **Phase 1（安全修复 + 质量基线）已完成** ✅

---

## 目录

1. [当前状态总览](#1-当前状态总览)
2. [功能差异矩阵](#2-功能差异矩阵)
3. [阶段路线图](#3-阶段路线图)
4. [各阶段详细方案](#4-各阶段详细方案)
5. [安全与质量基线](#5-安全与质量基线)
6. [验证标准](#6-验证标准)

---

## 1. 当前状态总览

### 1.1 项目现状

| 维度 | Rust (当前) | Go (对标) |
|------|------------|-----------|
| 编译状态 | ✅ `cargo build` 零 warning，`cargo clippy -- -D warnings` 通过 | ✅ 编译通过 |
| 测试覆盖率 | **起步** — 10 单元测试 + 2 doc-test（Phase 1 针对性测试）；Phase 6 将补全 | ✅ 115+ 测试用例 |
| 功能完整度 | ~85%（核心骨架完整，缺细节和校验） | ✅ 100% |
| 外部依赖 | 0 生产依赖（仅 chrono/async-trait 可选） | 0 外部依赖 |
| 后端数量 | 5/5（Postgres/MySQL/MariaDB/MSSQL/SQLite） | 5/5 |

### 1.2 Rust 已实现的核心能力

| 分类 | 状态 |
|------|------|
| SELECT / INSERT / UPDATE / DELETE | ✅ 完整 |
| WHERE 条件（=, <>, >, >=, <, <=, LIKE, IN, BETWEEN, IS NULL, EXISTS） | ✅ 完整 |
| WHERE 可选条件（eq_opt, like_opt, in_opt, set_opt） | ✅ 基础（缺 ne/gt/gte/lt/lte 的 *Opt 变体 — Phase 2） |
| WHERE 分组嵌套（and_group, or_group） | ✅ 完整 |
| 条件守卫 `when()` | ✅ 完整 |
| JOIN（INNER/LEFT/RIGHT/FULL/CROSS + ON 条件/分组/子查询/CTE） | ✅ 完整 |
| JOIN 能力校验（`supports_join_type`） | ✅ Phase 1 — SQLite 拒绝 RIGHT/FULL |
| GROUP BY / HAVING | ✅ 实现 |
| ORDER BY / LIMIT / OFFSET | ✅ 完整（含 MSSQL OFFSET...FETCH） |
| CTE（WITH / WITH RECURSIVE） | ✅ 完整 |
| 子查询（FROM/JOIN/WHERE IN/EXISTS） | ✅ 完整 |
| INSERT 批量 rows() | ✅ 完整 |
| INSERT FROM SELECT | ✅ 完整 |
| RETURNING（Postgres/MSSQL） | ✅ 实现 |
| ON CONFLICT / UPSERT（Postgres/MySQL/SQLite） | ✅ 实现 |
| MSSQL UPSERT | ⚠️ 显式返回 `UnsupportedFeature`（完整 MERGE 推迟） |
| 参数系统 Param 枚举 | ✅ 完整 |
| 多后端方言（5 个） | ✅ 全部有结构体实现 |
| Backend trait | ✅ 完整接口（含 `supports_join_type`） |
| Param 类型强制转换（From trait） | ✅ 完整 |
| 构建期校验（EmptyInClause / UnsupportedJoinType / UnsupportedFeature / NoSetClauses） | ✅ Phase 1 完成 |
| proc-macro `#[derive(InsertRow)]` | ✅ 实现 |
| chrono 时间类型支持 | ✅ feature-gated |

### 1.3 Rust 当前存在的问题

> **Phase 1（2026-06-14）已修复**：P0（LIKE 注入）、P1（EmptyInClause / UnsupportedJoinType / MSSQL UPSERT）、P2（build_ctes 存根 / dead_code / Doc-test）。

| # | 问题 | 严重程度 | 说明 | 状态 |
|---|------|---------|------|------|
| ~~P0~~ | ~~LIKE 手动转义非占位符~~ | 🔴 ~~安全漏洞~~ | ~~`Like` 条件在 SQL 中用单引号括值并手动转义，存在 SQL 注入风险~~ | ✅ **Phase 1 完成** — 改为 `CmpOp::Like` + `Expr::Value` 参数化 |
| ~~P1~~ | ~~EmptyInClause 未触发~~ | 🟡 ~~行为缺陷~~ | ~~`BuildError::EmptyInClause` 已定义但 `in_()`/`in_opt()` 不检查空列表~~ | ✅ **Phase 1 完成** — `validate_where_list` 递归校验 build 时返回 `Err` |
| ~~P1~~ | ~~UnsupportedJoinType 未校验~~ | 🟡 ~~行为缺陷~~ | ~~错误类型已定义但不校验，不支持的 JOIN 直接生成 SQL~~ | ✅ **Phase 1 完成** — `supports_join_type()` 默认实现 + SQLite override |
| ~~P1~~ | ~~MSSQL UPSERT 为空~~ | 🟡 ~~功能缺失~~ | ~~`MssqlBackend::on_conflict()` 返回空字符串~~ | ✅ **Phase 1 完成** — `on_conflict` 改为 `BuildResult<String>`，MSSQL 返回 `UnsupportedFeature` |
| ~~P2~~ | ~~build_ctes() 在 Backend trait 上是存根~~ | 🟡 ~~不一致~~ | ~~输出 `"..."`，实际逻辑在 builder 内联实现~~ | ✅ **Phase 1 完成** — 从 `Backend` trait 移除 stub |
| ~~P2~~ | ~~dead_code warning（3 个 logic 字段）~~ | ⚪ ~~代码质量~~ | ~~`OnAdder<T>`、`OnGroupBuilder`、`OnAdderForGroup` 的 `logic` 字段未使用~~ | ✅ **Phase 1 完成** — 直接移除未使用字段 |
| ~~P2~~ | ~~Doc-test 解构模式错误~~ | ⚪ ~~文档~~ | ~~build() 返回 QueryResult 而非 tuple~~ | ✅ **Phase 1 完成** — `graft/src/lib.rs` + `graft-core/src/builder.rs` 两处 doc-test |
| P3 | **测试覆盖率偏低** | 🟡 质量风险 | Phase 1 引入 10 个针对性测试（LIKE / EmptyIn / UPSERT / JOIN 校验），Phase 6 将补全 | 🟡 **待续**（Phase 6） |
| P3 | **部分公开方法缺文档注释** | ⚪ 文档 | 不符合项目规范 | ⏳ **待续** |

---

## 2. 功能差异矩阵

以下逐项比对 Go 版本已有而 Rust 版本缺失或需增强的功能。

### 2.1 列名智能引用

| 特性 | Go | Rust (当前) | 优先级 |
|------|----|------------|--------|
| `SelectIdent()` — 智能列名引用 | ✅ | ❌ 缺失 | **P1** |
| `GroupByIdent()` — 智能 GROUP BY 引用 | ✅ | ❌ 缺失 | **P2** |
| `OrderBySafe()` — 白名单校验 ORDER BY | ✅ | ❌ 缺失 | **P1** |

**设计意图**：`Ident` 系列方法根据列名是否含 `.`、`()`、是否为 SQL 关键字，智能决定加引号与否。Go 版在 `op.go` 中用 `isSimpleIdent()` 和 `sqlKeywords` map 实现。

### 2.2 WHERE 运算符补充

| 特性 | Go | Rust (当前) | 优先级 |
|------|----|------------|--------|
| `eq_opt` | ✅ | ✅ | 已有 |
| `neq_opt` / `ne_opt` | ✅ | ❌ 缺失 | **P1** |
| `gt_opt` / `gte_opt` / `lt_opt` / `lte_opt` | ✅ | ❌ 缺失 | **P1** |
| `like_opt` | ✅ | ❌ 缺失 | **P1** |
| **函数表达式 WHERE** | ✅ | ❌ 缺失 | **P2** |

**设计意图**：Go 版本为每个比较运算符都提供了 `*Opt` 变体，全量覆盖。Rust 当前只有 `eq_opt` 和 `like_opt`（in_opt 已存在）。

### 2.3 子查询比较运算符

| 特性 | Go | Rust (当前) | 优先级 |
|------|----|------------|--------|
| `in_subquery` | ✅ | ✅ | 已有 |
| `eq_subquery` | ✅ | ❌ 缺失 | **P1** |
| `neq_subquery` / `gt_subquery` / `gte_subquery` / `lt_subquery` / `lte_subquery` | ✅ | ❌ 缺失 | **P2** |
| `exists` / `not_exists` | ✅ | ✅ | 已有 |

### 2.4 安全/校验

| 特性 | Go | Rust (当前) | 优先级 |
|------|----|------------|--------|
| UPDATE 无 SET 时错误 | ✅ | ✅ (BuildError::NoSetClauses) | 已有 |
| 空 `IN` 子句错误 | ✅ | ✅ Phase 1 — `BuildError::EmptyInClause` build 时校验 | 已有 |
| 不支持的 JOIN 类型错误 | ✅ | ✅ Phase 1 — `BuildError::UnsupportedJoinType` build 时校验 | 已有 |
| UPDATE 无 WHERE 时拒绝 | ✅ (panic) | ❌ 未实现 | **P1**（Phase 4） |
| DELETE 无 WHERE 时拒绝 | ✅ (panic) | ❌ 未实现 | **P1**（Phase 4） |
| `AllowUnsafeUpdate()` / `AllowUnsafeDelete()` | ✅ | ❌ 缺失 | **P1**（Phase 4） |
| MSSQL OFFSET 无 ORDER BY 时拒绝 | ✅ (panic) | ❌ 未实现 | **P2**（Phase 4） |
| SQLite 不支持 RIGHT/FULL JOIN 时拒绝 | ✅ (panic) | ✅ Phase 1 — `SqliteBackend::supports_join_type()` 拒绝 | 已有 |

### 2.5 Backend 接口增强

| 特性 | Go | Rust (当前) | 优先级 |
|------|----|------------|--------|
| `SupportsJoinType()` | ✅ | ✅ Phase 1 — `Backend::supports_join_type()` | 已有 |
| `SupportsUpsertSyntax()` 或类似能力查询 | ✅ | ❌ 缺失 | **P2** |

### 2.6 MariaDB 方言差异

| 特性 | Go | Rust (当前) | 优先级 |
|------|----|------------|--------|
| MariaDB `ON CONFLICT DO NOTHING` (10.11+) | ✅ | ❌ 缺失（与 MySQL 相同） | **P2** |

### 2.7 便捷/语法糖方法

| 特性 | Go | Rust (当前) | 优先级 |
|------|----|------------|--------|
| `FromAs(table, alias)` | ✅ | ❌ 缺失（只能 `.from(table).as_(alias)`） | **P2** |
| `AndWhereExpr()` / `OrWhereExpr()` — 函数表达式 | ✅ | ❌ 缺失 | **P2** |
| `Having()` 独立 builder | ✅ | ⚠️ 复用 WhereAdder | **P3**（当前方案可用） |
| `SelectSubquery()` | ✅ | ❌ 缺失 | **P2** |

### 2.8 Rust 独有特性（Go 没有，应保留）

| 特性 | 说明 | 建议 |
|------|------|------|
| `Param` 枚举 + `From` trait | 编译期类型安全 | ✅ **保留并强化** |
| `BuildError` / `BuildResult<T>` | 优雅错误处理（Go 用 panic） | ✅ **保留** — Rust 更惯用 |
| `#[derive(InsertRow)]` proc-macro | 派生宏生成 FromRow 实现 | ✅ **保留** |
| `Executor` trait (async) | 端到端数据库执行抽象 | ⚠️ **保留但推迟驱动实现** |
| `chrono` 时间类型 | feature-gated 时间支持 | ✅ **保留** |

---

## 3. 阶段路线图

```
Phase 1: 安全修复 + 质量基线
Phase 2: WHERE 系统功能补齐
Phase 3: SELECT/ORDER/GROUP 补齐 + Backend 增强
Phase 4: 安全策略实现
Phase 5: 剩余语法糖 + MariaDB 方言
Phase 6: 综合测试
```

### 3.1 路线图总览

| 阶段 | 聚焦 | 产出 | 预计复杂度 | 风险 | 状态 |
|------|------|------|-----------|------|------|
| **Phase 1** | LIKE 注入修复、doc-test、dead_code、EmptyInClause、UnsupportedJoinType、build_ctes 对齐 | 编译零告警、无安全漏洞 | 中 | 低 | ✅ **完成（2026-06-14）** |
| **Phase 2** | `neq_opt`, `gt_opt`.., `like_opt`, `eq_subquery`, `and_where_expr`, `having` 独立 | WHERE 系统 100% 对齐 Go | 中高 | 中（需新增 WhereKind 变体） | ⏳ **下一阶段** |
| **Phase 3** | `SelectIdent`, `GroupByIdent`, `OrderBySafe`, `FromAs`, `SupportsJoinType`, MariaDB MSSQL UPSERT | SELECT/ORDER/GROUP 对齐 | 中高 | 中（列引用智能判断较复杂） | ⏳ 待启动 |
| **Phase 4** | 无 WHERE UPDATE/DELETE 拒绝、`AllowUnsafe*`、MSSQL ORDER BY 校验 | 安全策略对齐 | 低 | 低 | ⏳ 待启动 |
| **Phase 5** | `SelectSubquery`, `AndWhereExpr` 函数表达式, MariaDB 方言 | 剩余语法糖 + 方言 | 低 | 低 | ⏳ 待启动 |
| **Phase 6** | 全量测试覆盖 | 与 Go 版测试等价覆盖 | 高 | 低 | ⏳ 待启动 |

---

## 4. 各阶段详细方案

### Phase 1 — 安全修复 + 质量基线 ✅ **已完成（2026-06-14）**

**目标**：修复所有 P0/P1 问题和编译警告，拉平质量基线。

**最终验收**：
- `cargo build` 零 warning
- `cargo build --all-features` 成功
- `cargo clippy --all-features --all-targets -- -D warnings` 通过
- `cargo test --all-features` 10 单元测试 + 2 doc-test 全部通过
- 10 个针对性测试覆盖（LIKE 参数化 / EmptyInClause / MSSQL UPSERT / SQLite JOIN 校验）

#### 1.1 修复 LIKE 手动转义 → 改用占位符 ✅

**当前代码（有安全漏洞）**：
```rust
// builder.rs 中 build_where() 的 WhereKind::Column + CmpOp::Like 分支
CmpOp::Like => {
    let escaped = value.replace('\'', "''");
    sql.push_str(&format!("'{}'", escaped));  // ❌ 手动转义
}
```

**修复方案**：
```rust
// 将 Like 改为用占位符
CmpOp::Like => {
    params.push(value.clone());
    *idx += 1;
    sql.push_str(&backend.placeholder(*idx));
}
```

**影响**：LIKE 值也参数化，消除注入风险。Go 版同样用参数化（`col LIKE ?`）。

**实际完成**：在 `types.rs` 新增 `CmpOp::Like` 变体，`WhereAdder::like()` 改用 `Expr::Value` 承载值，`CmpOp::sql()` 返回 `"LIKE"`。`build_where_list` 的现有 `"{} {} "` 格式化自动产出 `column LIKE $1`。

**测试**：`like_uses_parameterized_placeholder`（验证 payload 不进 SQL）+ `like_opt_none_skips_condition`。

**置信度**: [高: 直接代码定位，修改范围明确]

#### 1.2 EmptyInClause 校验 ✅

**当前**：`in_()` 和 `in_opt()` 中 `values` 为空时直接生成 `IN ()`（语法错误）。

**修复**：在 `in_()`, `in_opt()`, `not_in()` 中检查 values 是否为空，为空时 `return Err(BuildError::EmptyInClause)`。

**实际完成**：在 `builder.rs` 新增 `validate_where_list(&self, groups: &[WhereGroup]) -> BuildResult<()>`，递归检查 `WhereKind::In { values, .. }` 是否为空组（覆盖 `and_group` 嵌套）。`build_select_query` / `build_update_query` / `build_delete_query` 入口处调用。

**决策**：按推荐方案 B（build 时校验），保持 fluent API 返回 `T` 不变。

**测试**：`in_clause_with_empty_iter_returns_error` + `in_opt_none_skips_condition` + `nested_empty_in_clause_returns_error`。

**置信度**: [高: 错误类型已定义，只需在 builder 方法中添加检查]

#### 1.3 UnsupportedJoinType 校验 ✅

**当前**：所有 JOIN 类型直接生成 SQL。

**修复**：在 build_joins 中调用 `backend.supports_join_type()`，不支持时返回 `Err(BuildError::UnsupportedJoinType)`。

**注意**：需要先在 `Backend trait` 中添加 `fn supports_join_type(&self, jt: JoinType) -> bool` 方法。

**实际完成**：`Backend` trait 新增 `supports_join_type` 默认返回 `true`；`SqliteBackend` override 拒绝 `Right`/`Full`；`builder.rs` 新增 `validate_joins()` 辅助方法。

**测试**：`sqlite_rejects_right_join` + `sqlite_rejects_full_join` + `sqlite_accepts_inner_join`（正例）。

**置信度**: [高: Go 版已有完整实现可参考]

#### 1.4 修复 build_ctes() 存根 ✅

**当前**：`Backend` trait 的 `build_ctes()` 输出 `"..."` 占位符，实际逻辑在 builder 内联实现。

**修复方案**：两种选择——
- **方案 A（推荐）**：从 `Backend` trait 中移除 `build_ctes()` 方法，所有 CTE build 逻辑集中在 `QueryBuilder` 内部（因为 CTE 是 AST 层的格式化，内部实现与后端无关）。
- **方案 B**：将 builder 内联的 `build_ctes_inner()` 逻辑迁移到每个 backend 的 `build_ctes()` 实现。

**实际完成**：采用**方案 A** — `Backend::build_ctes` stub 已从 trait 中移除。CTE 逻辑统一在 `QueryBuilder::build_ctes_inner` 内联实现（与 Go 版对齐）。

**置信度**: [中: 需确认 MSSQL 的 CTE 语法是否真的无差异。Go 版 CTE 完全在 builder 中构建，不经过 backend]

#### 1.5 修复 dead_code warnings ✅

3 个 `logic` 字段在中间态构造器中未使用：

| 位置 | 修复方式 |
|------|---------|
| `OnAdder<T>.logic` | 在 `OnAdder` 的 eq/ne/gt 等方法中读取并写入 ON condition |
| `OnGroupBuilder.logic` | 类似修复 |
| `OnAdderForGroup.logic` | 类似修复 |

**实际完成**：经审查，3 个 `logic` 字段确实在 build 路径中无消费者（build code 仅从 `OnCondition::Group { logic, .. }` 读取 logic，OnAdder/OnGroupBuilder 持有的 logic 没有被任何方法读取）。决定**直接移除**这 3 个字段（最小变更路径）：

- `OnAdder<T>`：删除 `logic: LogicOp` 字段
- `OnGroupBuilder`：删除 `logic: LogicOp` 字段 + `new(logic)` 改为 `new()`
- `OnAdderForGroup`：删除 `logic: LogicOp` 字段
- `JoinAdder::and_on` / `or_on` 构造 `OnAdder` 时不再填 `logic`
- `JoinAdder::and_group` / `or_group` 显式传 `LogicOp::And/Or` 给 `OnCondition::Group`，不依赖 `OnGroupBuilder` 内部字段

**置信度**: [高: 字段已定义，只需在对应方法中消费]

#### 1.6 修复 Doc-test ✅

`graft/src/lib.rs:26` 的 doc-test 中使用 `let (sql, params) = ...` 解构 tuple，但 `build()` 返回 `QueryResult`。

**修复**：改为 `let result = ...; let sql = result.sql; let params = result.params;` 或 `let QueryResult { sql, params, .. } = ...;`。

**实际完成**：
- `graft/src/lib.rs:26` 改用 `result.sql` / `result.params` 字段访问
- 额外修复 `graft-core/src/builder.rs:104` 缺失 `use graft_core::QueryBuilder;` 的 doc-test

**置信度**: [高: 代码清晰可见]

#### 1.7 MSSQL UPSERT 占位修复 ✅

**当前**：`MssqlBackend::on_conflict()` 返回空字符串。

**修复方案**：MSSQL 的 UPSERT 需要将整个 INSERT 改写为 `MERGE` 语句。这是一个结构性变更——需要让 `build_insert()` 在检测到 ON CONFLICT 时，对 MSSQL 后端调用 `build_merge()` 替代。

**Phase 1 最低要求**：使 `on_conflict()` 返回 `Err(BuildError::UnsupportedFeature)` 而不是静默失败。完整 MERGE 实现可推迟。

**实际完成**：将 `Backend::on_conflict` 签名从 `-> String` 改为 `-> BuildResult<String>`，所有 backend 同步更新。`MssqlBackend::on_conflict` 返回 `Err(BuildError::UnsupportedFeature(...))`。Postgres/MySQL/MariaDB/SQLite 路径不受影响。

**测试**：`mssql_upsert_returns_unsupported_error`（验证 MSSQL 路径报错）+ `postgres_upsert_do_nothing_still_works`（验证 Postgres 路径未回归）。

**置信度**: [高: 当前返回空字符串显然是 bug，至少应报错]

---

### Phase 1 附记：clippy collapsible_if 修复（额外收益）

`cargo clippy` 报告 2 处嵌套 `if let` 可合并（`builder.rs:717` INSERT RETURNING + `builder.rs:825` DELETE RETURNING）。利用 edition 2024 的 let-chains 合并为：

```rust
if let Some(ref returning_cols) = self.insert_returning
    && backend.supports_returning()
{
    write!(sql, " {}", backend.returning(returning_cols)).unwrap();
}
```

---

### Phase 2 — WHERE 系统功能补齐

**目标**：WHERE 系统与 Go 版本 100% 对齐。

#### 2.1 补充 *Opt 变体

**新增方法**（在 `WhereAdder<T>` 上）：

```rust
impl<T: HasWhere> WhereAdder<T> {
    pub fn ne_opt(self, val: Option<impl Into<Param>>) -> T { ... }
    pub fn gt_opt(self, val: Option<impl Into<Param>>) -> T { ... }
    pub fn gte_opt(self, val: Option<impl Into<Param>>) -> T { ... }
    pub fn lt_opt(self, val: Option<impl Into<Param>>) -> T { ... }
    pub fn lte_opt(self, val: Option<impl Into<Param>>) -> T { ... }
    pub fn like_opt(self, val: Option<impl Into<Param>>) -> T { ... }
}
```

**实现模式**（与现有 `eq_opt` 一致）：

```rust
pub fn ne_opt(self, val: Option<impl Into<Param>>) -> T {
    match val {
        Some(v) => self.ne(v),
        None => self.target,
    }
}
```

**置信度**: [高: 模式完全相同，批量添加即可]

#### 2.2 子查询比较运算符

**新增方法**：

```rust
impl<T: HasWhere> WhereAdder<T> {
    pub fn eq_subquery(self, sub: QueryBuilder) -> T { ... }
    pub fn neq_subquery(self, sub: QueryBuilder) -> T { ... }
    pub fn gt_subquery(self, sub: QueryBuilder) -> T { ... }
    pub fn gte_subquery(self, sub: QueryBuilder) -> T { ... }
    pub fn lt_subquery(self, sub: QueryBuilder) -> T { ... }
    pub fn lte_subquery(self, sub: QueryBuilder) -> T { ... }
}
```

**需要新增 WhereKind 变体**：在 `types.rs` 的 `WhereKind` 中添加：
```rust
pub enum WhereKind {
    // ... 现有变体 ...
    /// col op (SELECT ...)
    SubqueryCompare {
        column: String,
        op: CmpOp,
        subquery: Box<QueryBuilder>,
    },
}
```

**置信度**: [中: `whereKindSubquery` 在 Go 版已实现，可参考移植。需要注意子查询的参数连续性处理]

#### 2.3 函数表达式 WHERE

**目的**：支持 `WHERE UPPER(name) = ?` 或 `WHERE DATE(created_at) = ?` 这类左值不是裸列名的条件。

**新增方法**：
```rust
// 在 QueryBuilder 上
pub fn and_where_expr(self, expr: &str) -> WhereAdder<Self> { ... }
pub fn or_where_expr(self, expr: &str) -> WhereAdder<Self> { ... }
```

**实现**：新增 `WhereKind` 变体或标记字段指示列名是表达式而非标识符。build 时对表达式列名不加引号。

**置信度**: [中: Go 版通过 `AndWhereExpr()` / `OrWhereExpr()` 实现，列名标记为 expr role]

#### 2.4 Having 独立 Builder

**当前**：HAVING 复用 `WhereAdder`（通过 `group_by` + `having` 字段）。

**Go 版本**：有独立的 `Having` 方法返回 `HavingAdder`（但实际与 WhereAdder 结构相同）。

**建议**：保留当前方案——Rust 的 `Having` 方法复用 `WhereAdder` 更简洁。Go 版实际上也是类似的复用模式，只是结构体命名不同。这不是关键差异。

**置信度**: [高: 当前方案可行，不需改]
**决策**: ⏭️ **跳过**（非必要差异）

---

### Phase 3 — SELECT/ORDER/GROUP 补齐 + Backend 增强

**目标**：SELECT、ORDER BY、GROUP BY 的智能引用与安全特性对齐。

#### 3.1 SelectIdent — 智能列名引用

**设计意图**：根据列名是否含特殊字符、是否为 SQL 关键字，自动决定是否加引号。

**实现方案**：
1. 新建 `expr.rs` 或扩展 `types.rs`：添加 `is_simple_ident(name: &str) -> bool` 函数
   - 简单标识符：纯字母数字下划线开头，非 SQL 关键字 → 不加引号
   - 复杂标识符（含 `.`, `()`, 关键字）→ 加引号
2. 在 `QueryBuilder` 上添加 `select_ident(columns: &[&str]) -> Self`
3. build 时根据列名类型选择性调用 `backend.quote_ident()`

**关键字列表**：从 Go 版的 `sqlKeywords` map 迁移（约 150+ 个 SQL 关键字）。

**置信度**: [中: 逻辑清晰但需要维护关键字列表。Rust 版可按需简化，仅识别含特殊字符的列名]

#### 3.2 OrderBySafe — 白名单校验

**设计意图**：防御 ORDER BY 注入。只允许在白名单中的列名排序。

**API**：
```rust
pub fn order_by_safe(self, column: &str, dir: SortDir, whitelist: &[&str]) -> BuildResult<Self> {
    if !whitelist.contains(&column) {
        return Err(BuildError::UnsafeColumn(column.to_string()));
    }
    self.order_by(column, dir)
}
```

**Go 版实现**：panic 而非返回 Result。Rust 应利用类型系统返回 `Result`。

**置信度**: [高: 实现直接]

#### 3.3 GroupByIdent — 智能 GROUP BY 引用

与 `SelectIdent` 类似，对 GROUP BY 的列名智能引用。

**API**：
```rust
pub fn group_by_ident(self, columns: &[&str]) -> Self
```

**复用 `is_simple_ident()` 逻辑**。

**置信度**: [高: 与 SelectIdent 共享核心逻辑]

#### 3.4 FromAs — 带别名的 FROM

**当前**：`.from("users").as_("u")` 两步。
**目标**：`.from_as("users", "u")` 一步。

```rust
pub fn from_as(self, table: &str, alias: &str) -> Self {
    self.from.push(TableRef::TableAs(table.to_string(), alias.to_string()));
    self
}
```

**置信度**: [高: 纯粹的便捷方法]

#### 3.5 Backend trait 添加能力查询方法

**新增方法**：

```rust
pub trait Backend {
    /// 查询后端是否支持指定 JOIN 类型
    fn supports_join_type(&self, jt: JoinType) -> bool {
        match jt {
            JoinType::Inner | JoinType::Left | JoinType::Cross => true,
            JoinType::Right | JoinType::Full => true,
        }
    }

    /// 查询后端是否支持指定 UPSERT 语法
    fn supports_upsert(&self) -> bool { true }
}
```

**各后端 override**：
| 后端 | supports_join_type | supports_upsert |
|------|-------------------|-----------------|
| Postgres | 全支持 | ✅ |
| MySQL | 全支持（除 FULL） | ✅ |
| MariaDB | 全支持（除 FULL） | ✅ |
| MSSQL | 全支持 | ❌（走 MERGE） |
| SQLite | ❌ RIGHT/FULL (3.35.0+) | ✅ |

**置信度**: [高: Go 版有完全对应的 `SupportsJoinType()` 和 `SupportsUpsertSyntax()`]

---

### Phase 4 — 安全策略实现

**目标**：UPDATE/DELETE 安全校验与 Go 版对齐。

#### 4.1 无 WHERE 的 UPDATE/DELETE 拒绝

**规则**：
- `UPDATE` 构建时若 `where_list.is_empty()` 返回 `Err(BuildError::UnsafeUpdateWithoutWhere)`
- `DELETE` 同理返回 `Err(BuildError::UnsafeDeleteWithoutWhere)`

**逃生舱**：
```rust
pub fn allow_unsafe_update(self) -> Self {
    self.flags.allow_unsafe_update = true;
    self
}
pub fn allow_unsafe_delete(self) -> Self {
    self.flags.allow_unsafe_delete = true;
    self
}
```

**Go 版设计**：默认 panic，可放行。Rust 使用 `Result` 而非 panic，更符合 Rust 惯用做法。

**置信度**: [高: 在 build() 方法中检查 flag + where_list.is_empty()]

#### 4.2 MSSQL OFFSET 要求 ORDER BY

**规则**：MSSQL 的 `OFFSET...FETCH` 必须配合 `ORDER BY`。缺少时拒绝构建。

```rust
// MssqlBackend 的 limit_offset() 或 builder 的 build 阶段校验
// 可以在 build 阶段检查：若为 MSSQL 且 offset/limit 有值但 order_by 为空
```

**置信度**: [中: Go 版在 MssqlBackend 中通过 `resolveBackend()` 做校验。Rust 需要决定在 build 中校验还是在 backend 中]

#### 4.3 SQLite JOIN 限制校验

**规则**：SQLite 3.35.0 之前不支持 RIGHT/FULL JOIN。当前实现应拒绝这两种 JOIN。

```rust
// SqliteBackend 的 supports_join_type()
fn supports_join_type(&self, jt: JoinType) -> bool {
    !matches!(jt, JoinType::Right | JoinType::Full)
}
```

**置信度**: [高: Go 版已实现，与 Phase 3.5 的 supports_join_type 联动]

---

### Phase 5 — 剩余语法糖 + MariaDB 方言

**目标**：补齐剩余边缘功能。

#### 5.1 SelectSubquery

**当前**：没有便捷的标量子查询选择方法。
**新增**：

```rust
pub fn select_subquery(self, sub: QueryBuilder, alias: &str) -> Self
```

**生成**：`SELECT (SELECT ...) AS alias` 作为输出列。

**置信度**: [高: Go 版 `SelectSubquery()` 已实现]

#### 5.2 AndWhereExpr 函数表达式

与 Phase 2.3 的 `and_where_expr` / `or_where_expr` 相同，放在此处作为完整实现确认。

#### 5.3 MariaDB 独立 UPSERT 支持

**当前**：`MariaDbBackend` 完全继承 `MysqlBackend`，只支持 `ON DUPLICATE KEY UPDATE`。

**需要**：MariaDB 10.11+ 支持 `ON CONFLICT DO NOTHING` 标准语法。但 `ON CONFLICT DO UPDATE` 仍需要用 `ON DUPLICATE KEY UPDATE`。

**实现**：在 `MariaDbBackend` 中 override `on_conflict()`，对 `ConflictAction::DoNothing` 使用 Postgres 语法，对 `DoUpdate` 使用 MySQL 语法。

**置信度**: [高: Go 版 `mariaDBBackend` 已实现]

---

### Phase 6 — 综合测试

**目标**：达到与 Go 版等价的测试覆盖率。

#### 6.1 测试策略

| 测试层级 | 范围 | 方式 |
|---------|------|------|
| 单元测试 | 每个公开方法的基础功能 | `#[cfg(test)] mod tests {}` 内联 |
| 集成测试 | SELECT/INSERT/UPDATE/DELETE 全流程 | `tests/` 目录 |
| 方言测试 | 每个后端输出 SQL 验证 | 参数化测试 |
| 安全测试 | 无 WHERE/空 IN 等边界 | 构建时 `Result` 校验 |
| 参数连续性测试 | 子查询/CTE 参数索引验证 | 多级嵌套测试 |

#### 6.2 测试用例优先级

| 优先级 | 覆盖内容 | 估计用例数 |
|--------|---------|-----------|
| P0 | 基础 SELECT/INSERT/UPDATE/DELETE + 各后端 | 10 |
| P0 | 安全校验（无 SET、无 WHERE、空 IN） | 8 |
| P0 | LIKE 参数化（验证无注入） | 2 |
| P1 | 所有 *Opt 可选条件 | 14 |
| P1 | JOIN 所有类型 + ON 条件 | 8 |
| P1 | CTE / 递归 CTE | 4 |
| P1 | 子查询（IN_SUBQUERY/EXISTS/EQ_SUBQUERY） | 6 |
| P1 | MSSQL OFFSET/FETCH + SQLite JOIN 限制 | 4 |
| P2 | GROUP BY / HAVING / ORDER BY | 6 |
| P2 | 批量 INSERT / INSERT FROM SELECT | 4 |
| P2 | ON CONFLICT / UPSERT（各后端） | 6 |
| P2 | 列名引用 SelectIdent / GroupByIdent | 6 |
| P2 | 参数连续性（嵌套子查询） | 3 |

**总计**：约 80 测试用例（Go 版本 115+ 个，Rust 需要覆盖核心差异）

#### 6.3 测试最佳实践

- 使用 `build()` 的 `Result` 输出验证 SQL 字符串和参数列表
- 对每个后端运行关键测试（Postgres/MySQL/MSSQL/SQLite）
- 错误路径使用 `assert!(result.is_err())`
- SQL 字符串比较不依赖空白敏感性
- 参数值比较使用 `Param` 的 `Debug` 或手动 match

**置信度**: [高: Go 版测试已提供了完整的测试模式参考]

---

## 5. 安全与质量基线

### 5.1 Rust 项目的安全优势（应坚持）

| 策略 | Rust 实现 | 说明 |
|------|----------|------|
| 参数化 | `Param` 枚举 + `From trait` | 编译期强制，不会遗漏 |
| 错误处理 | `BuildResult<T>` | 非 panic 模式，调用方必须处理 |
| 类型安全 | consume-and-return 模式 | 中途状态不能误用 |
| 可选条件 | `Option<T>` 参数 | 明确表达意图 |

### 5.2 需要纠正的安全问题

| 问题 | 影响 | 修复阶段 | 状态 |
|------|------|---------|------|
| ~~LIKE 手动转义~~ | ~~可被 SQL 注入~~ | ~~**Phase 1**（立即）~~ | ✅ **已完成**（2026-06-14） |
| ~~空 IN 子句~~ | ~~生成非法 SQL~~ | ~~**Phase 1**~~ | ✅ **已完成**（2026-06-14） |
| 无 WHERE 的 UPDATE/DELETE | 误操作全表更新/删除 | **Phase 4** | ⏳ 待启动 |
| ~~MSSQL UPSERT 返回空字符串~~ | ~~静默生成非法 SQL~~ | ~~**Phase 1**（阶段修复）~~ | ✅ **已完成**（2026-06-14） |
| ~~SQLite 不支持 RIGHT/FULL JOIN 静默生成 SQL~~ | ~~执行时数据库报错（非构建期阻断）~~ | （**Phase 1** 顺带） | ✅ **已完成**（2026-06-14） |

### 5.3 编译质量基线

| 检查项 | 目标 | 验证命令 |
|--------|------|---------|
| 零 warning | `cargo build` 无 warning | `cargo build 2>&1 | grep -i warning \| wc -l` |
| 零 dead_code | 同上 | `cargo check` |
| 文档测试通过 | `cargo test` 全部通过 | `cargo test` |
| Clippy 无 error | `cargo clippy` | `cargo clippy -- -D warnings` |
| 格式化 | `cargo fmt --check` | `cargo fmt -- --check` |

---

## 6. 验证标准

### 6.1 阶段验证

| 阶段 | 验证标准 | 状态 |
|------|---------|------|
| **Phase 1** | `cargo build` 零 warning，`cargo test` 通过，LIKE 不再手动转义，EmptyInClause 触发，MSSQL UPSERT 报错而非空串 | ✅ **已通过**（2026-06-14） |
| **Phase 2** | 所有 *Opt 变体可用，子查询比较方法可用，函数表达式 WHERE 可用 | ⏳ 待启动 |
| **Phase 3** | `select_ident`/`order_by_safe`/`group_by_ident` 可用，`supports_join_type` 在 SQLite 拒绝 RIGHT/FULL JOIN | ⏳ 待启动（`supports_join_type` 已就位，SQLite override 已生效） |
| **Phase 4** | 无 WHERE 的 UPDATE/DELETE 返回 Err，`allow_unsafe_*` 放行，MSSQL ORDER BY 校验生效 | ⏳ 待启动 |
| **Phase 5** | `select_subquery` 可用，MariaDB ON CONFLICT DO NOTHING 输出正确 | ⏳ 待启动 |
| **Phase 6** | 测试覆盖所有公开 API，覆盖率 >= 80%（关键路径），参数连续性验证通过 | ⏳ 待启动 |

### 6.2 最终验收

```bash
cargo build              # 编译成功
cargo test               # 所有测试通过
cargo clippy -- -D warnings  # 无 clippy 问题
cargo fmt -- --check     # 格式正确
```

**Phase 1 验收快照（2026-06-14）**：
- `cargo build` → 零 warning ✅
- `cargo build --all-features` → 成功 ✅
- `cargo clippy --all-features --all-targets -- -D warnings` → 零 issue ✅
- `cargo test --all-features` → 10 单元测试 + 2 doc-test 全部通过 ✅
- `cargo fmt --check` → `graft-derive` 有预先存在的格式差异（非 Phase 1 范围）⚠️

### 6.3 Go 版对等验收

用 Go 版测试用例覆盖的模式作为基准，编写等价的 Rust 测试：

| Go 测试 | 对应 Rust 测试 | 注意 |
|---------|---------------|------|
| `TestSelectWhere*` (4 后端) | 参数化测试，对 5 后端运行 | 增加 SQLite |
| `TestEqOptSkipZero` | 用 `None` 测试 *Opt 跳过 | Rust 用 Option |
| `TestInEmptySkip` | `EmptyInClause` 返回 Err | Rust 返回 Result |
| `TestUpdateSetOpt` | 全部跳过时 NoSetClauses | 与 Go 行为一致 |
| `TestJoinWithParamCondition` | ON 条件参数化 | 验证占位符 |
| `TestParameterContinuity` | 嵌套子查询参数索引 | 关键测试 |
| `TestRecursiveCTE` | WITH RECURSIVE | 验证各后端 |
| `TestMssqlLimitRequiresOrderBy` | MSSQL 校验 | 返回 Err |
| `TestSqliteRightJoinPanic` | SQLite 校验 | Rust 返回 Err |

---

## 附录 A：各文件涉及改动概览

| 文件 | Phase 1 | Phase 2 | Phase 3 | Phase 4 | Phase 5 | Phase 6 |
|------|---------|---------|---------|---------|---------|---------|
| `graft-core/src/builder.rs` | ✅ LIKE 修复、EmptyInClause、dead_code、collapsible_if、validate_joins、10 个测试 | *Opt 变体、子查询比较、函数表达式 | SelectIdent、OrderBySafe、GroupByIdent、FromAs | 无 WHERE 校验、AllowUnsafe* | SelectSubquery | — |
| `graft-core/src/types.rs` | ✅ CmpOp::Like 新增 | SubqueryCompare WhereKind | — | UnsafeUpdate/Delete 错误变体 | — | — |
| `graft-core/src/backend.rs` | ✅ build_ctes 移除、supports_join_type 新增、on_conflict 返回 BuildResult | — | SupportsJoinType | — | — | — |
| `graft-core/src/backends/*.rs` | ✅ MSSQL on_conflict 报错、SQLite supports_join_type override、各后端 on_conflict 签名同步 | — | supports_join_type override | SQLite/MSSQL 校验 | MariaDB upsert | — |
| `graft-core/src/result.rs` | — | — | — | 新增 UnsafeUpdate/Delete 错误 | — | — |
| `graft-core/src/lib.rs` | — | — | — | — | — | — |
| `graft/src/lib.rs` | ✅ doc-test 修复 | — | — | — | — | — |
| `graft-core/src/*.rs` | — | — | — | — | — | 测试添加 |

---

## 附录 B：架构决策备忘

### ADR-001：Go 的 panic vs Rust 的 Result

**背景**：Go 版在方法构建阶段（`Build()` 前）使用 panic 报告编程错误（无 SET、无 WHERE、不支持的 JOIN 等）。

**决策**：Rust 版本统一使用 `BuildResult<T>` 返回错误。原因：
1. panic 在 Rust 中更"重"——可能触发 unwinding 或 abort
2. Rust 生态惯用 `Result` 处理可恢复错误
3. 调用方可以通过 `?` 或 `.unwrap()` 灵活选择处理方式

**例外**：Go 版一些纯内部的前置检查（如空别名检查）可以保持 `panic`/`expect` 风格。

### ADR-002：列名智能引用策略

**背景**：Go 版在 `op.go` 中用 `isSimpleIdent()` + `sqlKeywords` map 实现智能引用。

**方案 A（推荐）**：实现简化的 `is_simple_ident()` 函数，仅当列名包含 `.`、`(`、`)`、空格或空字符串时才加引号。不维持完整 SQL 关键字列表（过度工程）。

**方案 B**：移植 Go 版的完整关键字列表。

**决策**：采用**方案 A**。关键字列表在 SQL 标准演进中会不断增长，维护成本高。Rust 版本的 `SelectIdent` 和 `GroupByIdent` 只在必要时加引号，其余时候由用户负责。

### ADR-003：HAVING Builder 独立

**背景**：Go 版有独立的 `Having` 方法。Rust 当前复用 `WhereAdder`。

**决策**：保持当前复用方案。HAVING 的语义结构与 WHERE 完全相同（条件组），复用减少代码重复。`Having` 只是对 `.group_by()` 后的 `.and_where()` 的语义别名。

### ADR-004：MSSQL MERGE 实现策略

**背景**：MSSQL 的 UPSERT 需要将 INSERT 改写为 MERGE 语句。

**方案 A**：在 MSSQL 的 `on_conflict()` 中生成完整 MERGE 语句（改写整个 INSERT）。
**方案 B**：在 `MssqlBackend` 中 `on_conflict()` 返回 `Err(UnsupportedFeature)`，推迟完整实现。

**决策**：Phase 1 采用**方案 B**（返回错误），后续版本再实现方案 A。MERGE 语句的构建涉及 INSERT 结构的完全重写，复杂度较高，不应阻塞其他修复。

---

> **📊 统计摘要**
>
> | 阶段 | 计划 | 实际 | 状态 |
> |------|------|------|------|
> | **Phase 1** | 8 个文件，6 个核心改动 | 4 个核心文件改动（`builder.rs` / `backend.rs` / `types.rs` / 4 个 backends / `graft/src/lib.rs`），10 个针对性测试 | ✅ **完成（2026-06-14）** |
> | Phase 2（WHERE 补齐） | 约 2 个文件，~15 个方法添加 | — | ⏳ 待启动 |
> | Phase 3（SELECT/GROUP/ORDER+Backend） | 约 3 个文件，~10 个方法添加 | — | ⏳ 待启动（`supports_join_type` 已就位） |
> | Phase 4（安全策略） | 约 2 个文件，4 个校验规则 | — | ⏳ 待启动 |
> | Phase 5（语法糖） | 约 1 个文件，2-3 个方法 | — | ⏳ 待启动 |
> | Phase 6（测试） | ~80 个测试用例 | — | ⏳ 待启动（Phase 1 已引入 10 个） |
>
> **Phase 1 实际改动文件**：
> - `graft-core/src/types.rs`（CmpOp::Like 新增）
> - `graft-core/src/builder.rs`（LIKE 修复 / validate_where_list / validate_joins / dead_code 清理 / collapsible_if / 10 个测试）
> - `graft-core/src/backend.rs`（build_ctes 移除 / supports_join_type / on_conflict 改返回 BuildResult）
> - `graft-core/src/backends/mssql.rs`（on_conflict 返回 UnsupportedFeature）
> - `graft-core/src/backends/mysql.rs`、`mariadb.rs`、`sqlite.rs`（on_conflict 签名同步 + SQLite supports_join_type override）
> - `graft/src/lib.rs`（doc-test 解构修复）
