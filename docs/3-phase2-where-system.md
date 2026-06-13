# Phase 2 — WHERE 系统功能补齐实施方案

> 本文档基于 `docs/roadmap.md` 和 `docs/SQLQueryBuilder-Design-Memo.md` 制定。
> 生成日期：2026-06-14
> 状态：待启动

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

### 1.1 业务背景

Phase 1（安全修复 + 质量基线）已于 2026-06-14 完成。当前项目编译零告警、10 个针对性测试通过、LIKE 注入等安全漏洞已修复。

### 1.2 问题描述

WHERE 系统是动态查询构建器的核心入口。当前 Rust 版本与 Go 版本相比，在以下方面存在功能差距：

- **`*Opt` 变体不完整**：只有 `eq_opt` 和 `like_opt`，缺失 `ne_opt` / `gt_opt` / `gte_opt` / `lt_opt` / `lte_opt`
- **子查询比较运算符缺失**：只能 `in_subquery`，不能 `eq_subquery` / `gt_subquery` 等
- **函数表达式 WHERE 缺失**：无法直接写 `WHERE UPPER(name) = ?`

### 1.3 成功标准

| 标准 | 验证方式 |
|------|---------|
| 所有 `*Opt` 变体可用 | 测试验证：每个变体在 `Some(val)` 时生成条件，`None` 时跳过 |
| 子查询比较方法可用 | 测试验证：`eq_subquery` / `neq_subquery` / `gt_subquery` / `gte_subquery` / `lt_subquery` / `lte_subquery` 生成正确 SQL |
| 函数表达式 WHERE 可用 | 测试验证：`and_where_expr("UPPER(name)")` 输出不引号包裹 |
| `cargo build` 零 warning | `cargo build` |
| `cargo clippy -- -D warnings` 通过 | `cargo clippy` |
| `cargo test` 全部通过 | 新增约 14 个测试覆盖 |

---

## 2. 当前状态分析

### 2.1 相关文件现状

#### `graft-core/src/builder.rs`

**`WhereAdder<T>`**（第 1202 行）— 中间态构造器，当前方法：

| 方法 | 状态 | 说明 |
|------|------|------|
| `eq(val)` | ✅ | 列 = 值 |
| `ne(val)` | ✅ | 列 <> 值 |
| `gt(val)` | ✅ | 列 > 值 |
| `gte(val)` | ✅ | 列 >= 值 |
| `lt(val)` | ✅ | 列 < 值 |
| `lte(val)` | ✅ | 列 <= 值 |
| `like(val)` | ✅ | 列 LIKE 值 |
| `eq_opt(val)` | ✅ | 可选 = |
| `like_opt(val)` | ✅ | 可选 LIKE |
| `in_(vals)` | ✅ | 列 IN (... ) |
| `in_opt(vals)` | ✅ | 可选 IN |
| `in_subquery(sub)` | ✅ | 列 IN (SELECT ...) |
| `is_null()` | ✅ | 列 IS NULL |
| `is_not_null()` | ✅ | 列 IS NOT NULL |
| `between(low, high)` | ✅ | 列 BETWEEN ... AND ... |
| `eq_col(col)` | ✅ | 列 = 列（关联引用） |
| `raw(sql, params)` | ✅ | 原始 SQL |

**缺失的方法**（`#[来源: docs/roadmap.md:332-370]`）：

| 方法 | 优先级 | 模式 |
|------|--------|------|
| `ne_opt(val)` | P1 | 同 `eq_opt` |
| `gt_opt(val)` | P1 | 同 `eq_opt` |
| `gte_opt(val)` | P1 | 同 `eq_opt` |
| `lt_opt(val)` | P1 | 同 `eq_opt` |
| `lte_opt(val)` | P1 | 同 `eq_opt` |
| `eq_subquery(sub)` | P1 | 列 = (SELECT ...) |
| `neq_subquery(sub)` | P2 | 列 <> (SELECT ...) |
| `gt_subquery(sub)` | P2 | 列 > (SELECT ...) |
| `gte_subquery(sub)` | P2 | 列 >= (SELECT ...) |
| `lt_subquery(sub)` | P2 | 列 < (SELECT ...) |
| `lte_subquery(sub)` | P2 | 列 <= (SELECT ...) |

**`QueryBuilder`**（第 24 行）：

当前有 `and_where(column)` / `or_where(column)` 方法。需要添加 `and_where_expr(expr)` / `or_where_expr(expr)` 用于函数表达式 WHERE。

**`GroupBuilder`**（第 1435 行）：

同样需要 `and_where_expr` / `or_where_expr` 方法。

#### `graft-core/src/types.rs`

**`WhereKind`**（第 61 行）：

```rust
pub enum WhereKind {
    Column { column: String, op: CmpOp, value: Expr },
    In { column: String, values: Vec<Vec<Expr>>, negated: bool },
    Between { column: String, low: Expr, high: Expr },
    IsNull { column: String, negated: bool },
    Exists { subquery: Box<QueryBuilder>, negated: bool },
    Group(Vec<WhereGroup>),
    Raw(String, Vec<Param>),
}
```

**关键发现**：`WhereKind::Column` 的 `value` 字段是 `Expr` 枚举，其中已包含 `Subquery(Box<QueryBuilder>)` 变体。在 `build_where_list` 中，`Expr::Subquery` 的处理逻辑为：

```rust
Expr::Subquery(sub) => {
    if let Ok((sub_sql, mut sub_params)) = sub.build_select_query(backend, idx) {
        write!(sql, "({sub_sql})").unwrap();
        params.append(&mut sub_params);
    }
}
```

这意味着 `col = (SELECT ...)` 的格式已经由现有代码支持。

#### `graft-core/src/backend.rs`

无需修改。

### 2.2 关键依赖关系

```
and_where("col").eq_subquery(sub)
  → WhereAdder<QueryBuilder>::eq_subquery(sub)
  → WhereKind::Column { column, op: CmpOp::Eq, value: Expr::Subquery(sub) }
  → QueryBuilder::add_where(LogicOp::And, kind)
  → where_list.push(WhereGroup { logic, kind })
  → build_where_list() 输出: "col = (SELECT ...)"
```

`GroupBuilder` 也实现 `HasWhere`，所有 `WhereAdder` 方法自动对其生效。

---

## 3. 调研发现

### 3.1 子查询比较可以"零成本"实现

**核心发现** [高: 代码直接证实]：

`build_where_list`（`builder.rs:938-961`）的 `WhereKind::Column` 分支中，对 `Expr::Subquery` 的输出是 `(sub_sql)`。因此：

- `eq_subquery(sub)` → `Expr::Subquery(Box::new(sub))` → `col = (SELECT ...)` ✅
- `neq_subquery(sub)` → 同上，op = `CmpOp::Ne` → `col <> (SELECT ...)` ✅
- 以此类推...

**结论**：**不需要新增 `WhereKind` 变体**。6 个 subquery 方法只需在 `WhereAdder` 上添加便捷方法，复用现有 `Column { op, value: Expr::Subquery(...) }`。

### 3.2 函数表达式 WHERE 的策略

**问题**：`build_where_list` 中对 `Column` 变体调用 `backend.quote_ident(column)`，函数表达式加引号会出错（`"UPPER(name)"` 非法）。

**方案 A（推荐）**：在 `build_where_list` 中添加 `is_simple_ident()` 判断，对非简单标识符（含 `()`, `.`, 空格等）不加引号。

**方案 B**：新增 `WhereKind::Expr` 变体，在 build 中单独处理。

**决策理由**：方案 A 更简洁（无新增变体、无新增 `WhereAdder` 字段），且已由 ADR-002 确认为推荐策略。方案 B 虽然显式但引入更多代码变化。

### 3.3 `*Opt` 变体模式

现有 `eq_opt` 的实现模式 [高: 代码 confirm]：

```rust
pub fn eq_opt(self, val: Option<impl Into<Param>>) -> T {
    match val {
        Some(v) => self.eq(v),
        None => self.target,
    }
}
```

所有缺失的 `*Opt` 变体（`ne_opt`, `gt_opt`, `gte_opt`, `lt_opt`, `lte_opt`）遵循完全相同的模式。`like_opt` 已存在。

---

## 4. 可选方案

### 方案 A：最小变更路径（推荐 ✅）

**核心策略**：
- 子查询比较：复用 `WhereKind::Column` + `Expr::Subquery`
- 函数表达式：通过 `is_simple_ident()` 自动判断是否加引号
- `*Opt` 变体：批量添加，与 `eq_opt` 一致

**优点**：
- 零 `types.rs` 变更
- 零 `WhereAdder` 结构变更
- 所有新增代码集中在 `builder.rs` 的 `WhereAdder` impl 块中
- `GroupBuilder` 自动受益

**缺点**：
- 函数表达式的 auto-detect 不是显式的（但有意名为 `and_where_expr` 提供文档线索）

### 方案 B：新增 `WhereKind::Expr` 变体

**核心策略**：
- 在 `types.rs` 新增 `WhereKind::Expr { expr: String, op: CmpOp, value: Expr }`
- 在 `WhereAdder` 新增 `is_expr: bool` 字段
- 在 `build_where_list` 添加 `Expr` 分支（不引号）

**优点**：
- 类型系统明确区分"表达式"和"列名"
- build 逻辑更显式

**缺点**：
- 需要修改 `WhereAdder` 结构（每个方法的影响）
- 新增的枚举变体在 match 中需要处理（现有代码中可能已有 exhaustive match）
- 更多变更，收益有限

### 方案 C：独立 `ExprWhereAdder` 类型

**核心策略**：
- 创建独立的 `ExprWhereAdder<T>` 中间态
- `and_where_expr()` 返回 `ExprWhereAdder`
- `ExprWhereAdder` 的方法创建 `WhereKind::Expr`

**优点**：
- 类型级别区分，编译期保证

**缺点**：
- 代码重复（`ExprWhereAdder` 方法完全复制 `WhereAdder`）
- 过度工程

---

## 5. 推荐方案

### 推荐：方案 A — 最小变更路径

**理由**：

1. **子查询比较不新增变体**：现有 `WhereKind::Column` + `Expr::Subquery` 已正确处理 `col OP (SELECT ...)`。`build_where_list` 中 `Expr::Subquery` 自动加括号，输出 `(sub_sql)`。这是已经验证的代码路径。

2. **函数表达式通过 `is_simple_ident()` 处理**：与 ADR-002 一致。函数表达式如 `UPPER(name)` 含 `()`，自动跳过引号。这是 Go 版本 `isSimpleIdent()` 的简化实现。

3. **`and_where_expr` / `or_where_expr` 作为语义别名**：与 `and_where` / `or_where` 相同的实现，仅用于 API 文档意图。也可以在 build 中统一处理。

### 具体设计

#### 5.1 `is_simple_ident()` 辅助函数

**位置**：在 `builder.rs` 中定义

```rust
/// 判断列名是否为简单标识符（仅字母数字下划线）。
/// 复杂标识符（含 `.`、`()`、空格等）不加引号。
fn is_simple_ident(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    // 不检查首字符是否为数字 — 带数字前缀的列名加引号也无害
    name.chars().all(|c| c.is_alphanumeric() || c == '_')
}
```

**置信度**：[高: 逻辑简单，与 ADR-002 一致]

#### 5.2 `build_where_list` 修改

**位置**：`builder.rs` 第 938 行

**当前代码**：
```rust
WhereKind::Column { column, op, value } => {
    write!(sql, "{} {} ", backend.quote_ident(column), op.sql()).unwrap();
    // ... match value
}
```

**修改后**：
```rust
WhereKind::Column { column, op, value } => {
    if is_simple_ident(&column) {
        write!(sql, "{} {} ", backend.quote_ident(column), op.sql()).unwrap();
    } else {
        write!(sql, "{} {} ", column, op.sql()).unwrap();
    }
    // ... match value (unchanged)
}
```

**置信度**：[高: 单点修改，不影响现有行为 — 简单标识符行为不变]

#### 5.3 `*Opt` 变体（6 个方法）

**位置**：`builder.rs` 的 `impl<T: HasWhere> WhereAdder<T>` 块

```rust
pub fn ne_opt(self, val: Option<impl Into<Param>>) -> T {
    match val {
        Some(v) => self.ne(v),
        None => self.target,
    }
}

pub fn gt_opt(self, val: Option<impl Into<Param>>) -> T { /* 同模式 */ }
pub fn gte_opt(self, val: Option<impl Into<Param>>) -> T { /* 同模式 */ }
pub fn lt_opt(self, val: Option<impl Into<Param>>) -> T { /* 同模式 */ }
pub fn lte_opt(self, val: Option<impl Into<Param>>) -> T { /* 同模式 */ }
```

**置信度**：[高: 与现有 `eq_opt` 完全相同模式]

#### 5.4 子查询比较方法（6 个方法）

**位置**：`builder.rs` 的 `impl<T: HasWhere> WhereAdder<T>` 块

```rust
pub fn eq_subquery(self, sub: QueryBuilder) -> T {
    let Self { mut target, logic, column } = self;
    let kind = WhereKind::Column {
        column,
        op: CmpOp::Eq,
        value: Expr::Subquery(Box::new(sub)),
    };
    target.add_where(logic, kind)
}

pub fn neq_subquery(self, sub: QueryBuilder) -> T {
    // 同模式，op: CmpOp::Ne
}
pub fn gt_subquery(self, sub: QueryBuilder) -> T { /* CmpOp::Gt */ }
pub fn gte_subquery(self, sub: QueryBuilder) -> T { /* CmpOp::Gte */ }
pub fn lt_subquery(self, sub: QueryBuilder) -> T { /* CmpOp::Lt */ }
pub fn lte_subquery(self, sub: QueryBuilder) -> T { /* CmpOp::Lte */ }
```

**置信度**：[高: `WhereKind::Column` + `Expr::Subquery` 已在 `build_where_list` 中正确处理]

#### 5.5 `and_where_expr` / `or_where_expr`

**位置**：`builder.rs` 的 `impl QueryBuilder` 和 `impl GroupBuilder`

```rust
// QueryBuilder
pub fn and_where_expr(self, expr: &str) -> WhereAdder<Self> {
    self.and_where(expr)  // 内部实现一致，build 时通过 is_simple_ident 自动判断
}

pub fn or_where_expr(self, expr: &str) -> WhereAdder<Self> {
    self.or_where(expr)
}

// GroupBuilder
pub fn and_where_expr(self, expr: &str) -> WhereAdder<Self> {
    self.and_where(expr)
}

pub fn or_where_expr(self, expr: &str) -> WhereAdder<Self> {
    self.or_where(expr)
}
```

**注意**：`and_where_expr` 与 `and_where` 的实现完全相同。区别仅在于方法名传达语义：「这是表达式，不是列名」。build 时通过 `is_simple_ident()` 自动处理引号。

**置信度**：[高: 纯别名方法]

#### 5.6 生成 SQL 示例

```rust
QueryBuilder::select(&["*"]).from("users")
    .and_where("name").eq_opt(Some("alice"))                    // name = $1
    .and_where("age").gt_opt(Some(18))                          // age > $2
    .and_where("dept").ne_opt(None)                             // ← 跳过
    .and_where_expr("UPPER(email)").eq("ALICE@EXAMPLE.COM")    // UPPER(email) = $3
    .and_where("salary").gt_subquery(
        QueryBuilder::select(&["AVG(salary)"]).from("employees") // salary > (SELECT AVG(salary) FROM employees)
    )
    .build(&PostgresBackend);

// SELECT * FROM users
// WHERE name = $1 AND age > $2
//   AND UPPER(email) = $3
//   AND salary > (SELECT AVG(salary) FROM employees)
```

---

## 6. 实施建议

### 6.1 实施路径

建议按以下顺序实施（从低风险到中风险）：

| 步骤 | 内容 | 风险 | 预计代码量 |
|------|------|------|-----------|
| 1 | 添加 6 个 `*Opt` 变体（`ne_opt`, `gt_opt`, `gte_opt`, `lt_opt`, `lte_opt`） | 低 | +6 个方法，~30 行 |
| 2 | 添加 6 个子查询比较方法（`eq_subquery`, `neq_subquery` 等） | 低 | +6 个方法，~48 行 |
| 3 | 添加 `is_simple_ident()` + 修改 `build_where_list` + `and_where_expr`/`or_where_expr` | 中 | ~15 行 |
| 4 | 测试覆盖 | 中 | ~14 个测试 |

### 6.2 改动文件一览

| 文件 | 改动类型 | 行数估计 |
|------|---------|---------|
| `graft-core/src/builder.rs` | 新增方法 + 辅助函数 + 测试 | ~200 行 |
| `graft-core/src/types.rs` | **无改动** | 0 |
| `graft/src/lib.rs` | **无改动** | 0 |

### 6.3 测试用例

预计新增约 14 个测试：

| # | 测试名称 | 覆盖内容 |
|---|---------|---------|
| 1 | `ne_opt_with_value_adds_condition` | `ne_opt(Some(val))` 生成 `col <> $1` |
| 2 | `ne_opt_none_skips_condition` | `ne_opt(None)` 跳过条件 |
| 3 | `gt_opt_adds_condition` | 同模式 |
| 4 | `gte_opt_adds_condition` | 同模式 |
| 5 | `lt_opt_adds_condition` | 同模式 |
| 6 | `lte_opt_adds_condition` | 同模式 |
| 7 | `eq_subquery_generates_correct_sql` | `col = (SELECT ...)` |
| 8 | `gt_subquery_generates_correct_sql` | `col > (SELECT ...)` |
| 9 | `neq_subquery_generates_correct_sql` | `col <> (SELECT ...)` |
| 10 | `where_expr_does_not_quote` | `UPPER(col) = $1` 输出无引号 |
| 11 | `where_expr_with_simple_ident_still_quotes` | 简单列名仍加引号（回归测试） |
| 12 | `like_opt_skip` | 已有（Phase 1），确保不回归 |
| 13 | `subquery_param_continuity` | 子查询参数索引连续性 |
| 14 | `and_where_expr_with_group` | 函数表达式在分组中工作 |

### 6.4 注意事项

1. **参数连续性**：子查询比较方法（`eq_subquery` 等）中的子查询参数会自动与外部连续 — `build_where_list` 的 `Expr::Subquery` 分支通过 `build_select_query(backend, idx)` 递归传递 `idx`。

2. **子查询多行问题**：`col = (SELECT ...)` 要求子查询返回单行。这是 SQL 语义限制，builder 不负责校验。用户需确保子查询是标量子查询。

3. **`is_simple_ident` 的边界**：当前实现只检查字母数字下划线。SQL 关键字如 `SELECT` 作为列名会被引号包裹（`"SELECT"`），这在 SQL 中是合法的。不维护关键字列表是 ADR-002 的明确决策。

---

## 7. 参考来源

### 代码路径

| 文件 | 关键行 | 说明 |
|------|--------|------|
| `graft-core/src/builder.rs:1202-1428` | `WhereAdder<T>` 完整实现 | 所有新方法在此添加 |
| `graft-core/src/builder.rs:924-1045` | `build_where_list()` | WHERE SQL 生成逻辑，修改 Column 变体的引号处理 |
| `graft-core/src/builder.rs:1522-1546` | `HasWhere` trait + impl | `GroupBuilder` 自动受益 |
| `graft-core/src/types.rs:52-57` | `Expr` 枚举 | 已含 `Subquery` 变体 |
| `graft-core/src/types.rs:61-90` | `WhereKind` 枚举 | 无需修改 |

### 文档引用

| 文档 | 章节 | 说明 |
|------|------|------|
| `docs/roadmap.md:328-415` | Phase 2 详细方案 | 原始需求定义 |
| `docs/roadmap.md:752-761` | ADR-002 列名智能引用策略 | 关于 `is_simple_ident` 的架构决策 |
| `docs/SQLQueryBuilder-Design-Memo.md:296-405` | WHERE 系统设计 | Expr::Subquery 的存在确认 |

### 置信度汇总

| 结论 | 置信度 | 来源 |
|------|--------|------|
| 子查询比较可复用 `WhereKind::Column` + `Expr::Subquery` | **高** | 代码直接证实（`builder.rs:938-960`） |
| `is_simple_ident()` 足够可靠 | **高** | 逻辑简单，ADR-002 推荐 |
| 新增 *Opt 对 GroupBuilder 自动生效 | **高** | `GroupBuilder: HasWhere` 实现 |
| `and_where_expr` 可与 `and_where` 实现相同 | **中** | build 时通过 is_simple_ident 区分 |
| 零 types.rs 变更 | **高** | 所有新增功能用现有枚举实现 |

---

> **与 Phase 3 的边界**：`SelectIdent` / `GroupByIdent` / `OrderBySafe` 的列名智能引用将在 Phase 3 实现。Phase 2 只在 `build_where_list` 中引入 `is_simple_ident()`，与 Phase 3 共享同一逻辑。如 Phase 3 需要独立的 `is_simple_ident` 公共函数，届时可将其提取到 `types.rs` 或新的 `expr.rs`。
