# Rust SQLQueryBuilder 设计备忘

> 多后端动态 SQL 查询构建器，支持 Postgres、MySQL、MariaDB、MSSQL、SQLite
> 设计日期：2026-06-11

---

## 目录

1. [设计哲学](#1-设计哲学)
2. [整体架构](#2-整体架构)
3. [参数系统（Param）](#3-参数系统param)
4. [Backend Trait](#4-backend-trait)
5. [查询节点（AST 层）](#5-查询节点ast-层)
6. [WHERE 系统](#6-where-系统)
7. [JOIN](#7-join)
8. [INSERT](#8-insert)
9. [UPSERT / ON CONFLICT](#9-upsert--on-conflict)
10. [BULK INSERT](#10-bulk-insert)
11. [UPDATE](#11-update)
12. [DELETE](#12-delete)
13. [子查询](#13-子查询)
14. [CTE（WITH 子句）](#14-ctewith-子句)
15. [执行器（Executor）](#15-执行器executor)
16. [后端差异对照表](#16-后端差异对照表)
17. [安全底线](#17-安全底线)
18. [Feature 条件编译](#18-feature-条件编译)
19. [Roadmap / 未定事项](#19-roadmap--未定事项)

---

## 1. 设计哲学

### 核心理念

- **Builder 层只构建查询意图（AST-like 结构）和收集参数，格式化交给 Backend trait**
- 所有用户输入必须经过 `Param` 枚举，绝不拼进 SQL 字符串
- `build()` 之后返回的 `QueryResult` 包含 `(String, Vec<Param>)`，SQL 字符串里只有关键字、标识符、占位符
- **安全底线不破**：任何 `raw` 方法都只接受 SQL 片段 + 独立 `Vec<Param>`，不允许混入参数值

### 价值主张

| 维度 | 目标 |
|------|------|
| 安全 | 编译期强制参数化，杜绝 SQL 注入 |
| 动态 | `eq_opt` / `set_opt` / `when` 原生支持可选条件 |
| 多后端 | Backend trait 封装方言差异，build 时选择 |
| 可读 | Fluent API 与 SQL 逻辑结构一一对应 |

### 与现有方案定位

| 方案 | 动态查询 | 多后端 | 参数安全 | 学习成本 |
|------|---------|-------|---------|---------|
| **本设计** | ✅ 原生（eq_opt, when） | ✅ feature 编译期选 | ✅ 强制 | 中 |
| `sea-query` | ⚠️ 可以但不顺手 | ✅ 最多 | ✅ 强类型 | 高 |
| `sqlx::QueryBuilder` | ✅ push_bind 自由度高 | ✅ compile-time | ✅ 强类型 | 中低 |
| `diesel` | ❌ 不适合动态 | ✅ | ✅ | 高 |
| 手写字符串 | ✅ 自由但危险 | ❌ 硬编码 | ❌ 容易忘 | 低 |

定位：介于 `sqlx::QueryBuilder`（灵活但 raw）和 `sea-query`（完备但沉重）之间。

---

## 2. 整体架构

```
┌──────────────────────────────┐
│         QueryBuilder         │  ← 核心，与后端无关
│  AST 层 (SelectNode / CteNode)  │
│  参数收集 (Vec<Param>)       │
└───────────┬──────────────────┘
            │ .build::<Backend>()
            ▼
┌──────────────────────────────┐
│        Backend trait         │  ← 每个后端实现
│  build_select()              │
│  placeholder(index)          │
│  quote_ident(name)           │
│  limit_offset()              │
│  returning()                 │
│  on_conflict()               │
└───────────┬──────────────────┘
            │
 ┌──────────┼──────────┬───────────┐
 ▼          ▼          ▼           ▼
Pgsql     Mysql     Mssql      Sqlite
Backend   Backend   Backend    Backend
($1, $2)  (?, ?)    (@P1)      (?)
```

### 编译期多后端

```toml
# Cargo.toml
[features]
default = ["postgresql"]
postgresql = ["chrono"]
mysql      = ["chrono"]
mariadb    = ["chrono"]
mssql      = []
sqlite     = []
```

运行时选择 Backend：

```rust
let q = match db_kind {
    "postgres" => builder.clone().build(&PostgresBackend),
    "mysql"    => builder.clone().build(&MysqlBackend),
    "mssql"    => builder.clone().build(&MssqlBackend),
    "sqlite"   => builder.clone().build(&SqliteBackend),
};
```

---

## 3. 参数系统（Param）

### Param 枚举

覆盖常见 SQL 类型。From trait 用于零成本转换。

```rust
#[derive(Debug, Clone)]
pub enum Param {
    Null,
    Bool(bool),
    I8(i8), I16(i16), I32(i32), I64(i64),
    F32(f32), F64(f64),
    Text(String),
    Bytes(Vec<u8>),
    #[cfg(feature = "chrono")]
    DateTime(chrono::NaiveDateTime),
    #[cfg(feature = "chrono")]
    DateTimeTz(chrono::DateTime<chrono::Utc>),
}

impl From<&str> for Param { fn from(s: &str) -> Self { Param::Text(s.to_owned()) } }
impl From<String> for Param { ... }
impl From<i32> for Param { ... }
// ...更多 From 实现
```

### 核心约束

build 返回的 `QueryResult` 包含：

```rust
pub struct QueryResult {
    pub statements: Vec<(String, Vec<Param>)>,  // 支持多语句（如 MySQL RETURNING 降级）
    pub sql: String,                            // 单语句快捷字段
    pub params: Vec<Param>,
}
```

**SQL 字符串里没有用户输入**──只有关键字、标识符、占位符。

---

## 4. Backend Trait

```rust
pub trait Backend {
    /// 占位符（1-indexed）
    fn placeholder(&self, i: usize) -> String {
        format!("${}", i)  // Postgres 默认
        // MySQL:     "?"
        // SQLite:    "?"
        // MSSQL:     "@P{}"
    }

    /// 引用标识符
    fn quote_ident(&self, name: &str) -> String {
        format!("\"{}\"", name)  // Postgres/SQLite
        // MySQL:     "`{}`"
        // MSSQL:     "[{}]"
    }

    /// LIMIT / OFFSET
    fn limit_offset(&self, limit: Option<usize>, offset: Option<usize>) -> String {
        // Postgres/MySQL/SQLite: "LIMIT x OFFSET y"
        // MSSQL: "OFFSET x ROWS FETCH NEXT y ROWS ONLY"
    }

    /// RETURNING 子句
    fn returning(&self, columns: &[String]) -> String {
        format!("RETURNING {}", columns.join(", "))  // Postgres 默认
        // MSSQL: "OUTPUT INSERTED.col1, INSERTED.col2"
    }

    /// 支持 RETURNING 吗？
    fn supports_returning(&self) -> bool { true }  // MySQL/SQLite 返回 false

    /// 批量插入后支持 RETURNING 多行吗？
    fn supports_bulk_returning(&self) -> bool { true }  // MySQL/SQLite 返回 false

    /// ON CONFLICT
    fn on_conflict(&self, columns: &[String], action: &ConflictAction,
                   set: &[(String, Param)]) -> String {
        // Postgres: "ON CONFLICT (col) DO UPDATE SET ..."
        // MySQL:    "ON DUPLICATE KEY UPDATE ..."
        // MSSQL:    MERGE 整个语句
    }

    /// 构建 CTE
    fn build_ctes(&self, ctes: &[CteNode], param_offset: &mut usize) -> (String, Vec<Param>);

    /// 构建 SELECT（递归入口）
    fn build_select(&self, node: &SelectNode, param_offset: &mut usize) -> (String, Vec<Param>);

    // ...更多构建方法
}
```

### 后端差异策略

Backend trait 给出合理默认值（Postgres），各后端 override 差异部分。MSSQL 的 `MERGE` 等根本不同的语法，用单独的 `build_merge()` 方法或让 `on_conflict()` 返回完整 MERGE 语句。

---

## 5. 查询节点（AST 层）

Builder 内部用结构体表示查询意图，`build()` 时遍历生成 SQL。

### QueryBuilder

```rust
pub struct QueryBuilder {
    mode: QueryMode,                        // Select, Insert, Update, Delete
    // SELECT
    columns: Vec<SelectExpr>,               // 列或标量子查询
    from: Vec<TableRef>,                    // 表或派生表
    joins: Vec<JoinClause>,
    where_list: Vec<WhereGroup>,
    group_by: Vec<String>,
    having: Vec<WhereGroup>,
    order_by: Vec<(String, SortDir)>,
    limit: Option<usize>,
    offset: Option<usize>,

    // INSERT
    insert_table: Option<String>,
    insert_columns: Vec<String>,
    insert_values: Vec<Vec<Param>>,         // 多行
    insert_from_select: Option<Box<QueryBuilder>>,
    insert_returning: Option<Vec<String>>,
    insert_conflict: Option<ConflictClause>,

    // UPDATE
    update_table: Option<String>,
    set_list: Vec<SetClause>,

    // DELETE
    delete_table: Option<String>,
    delete_returning: Option<Vec<String>>,

    // CTE
    ctes: Vec<CteNode>,
}
```

### 值传递链式调用

Builder 的方法用**值传递（consume-and-return）**，避免 borrow checker 干扰：

```rust
pub fn and_where(self, column: &str) -> WhereAdder<Self> { ... }
pub fn or_where(self, column: &str) -> WhereAdder<Self> { ... }
```

`WhereAdder<T>` 持有 builder 的所有权，方法消耗自己返回 builder：

```rust
pub struct WhereAdder<T> {
    target: T,
    column: String,
    logic: LogicOp,
}

impl<T: HasWhere> WhereAdder<T> {
    pub fn eq(self, val: impl Into<Param>) -> T { ... }
    pub fn in_(self, vals: impl IntoIterator<Item: impl Into<Param>>) -> T { ... }
    pub fn like(self, val: impl Into<Param>) -> T { ... }
    pub fn eq_opt(self, val: Option<impl Into<Param>>) -> T { ... }  // ❤️ 动态查询 MVP
    pub fn in_opt(self, vals: Option<impl IntoIterator<Item: impl Into<Param>>>) -> T { ... }
    pub fn is_null(self) -> T { ... }
    pub fn between(self, low: impl Into<Param>, high: impl Into<Param>) -> T { ... }
    pub fn eq_col(self, col: &str) -> T { ... }       // 列 = 列（关联子查询）
    pub fn eq_raw(self, expr: &str) -> T { ... }       // 列 = 表达式
    pub fn raw(self, sql: &str, params: Vec<Param>) -> T { ... }  // 逃生舱
}
```

---

## 6. WHERE 系统

### 核心结构

```rust
pub struct WhereGroup {
    logic: LogicOp,       // AND / OR
    kind: WhereKind,
}

pub enum WhereKind {
    /// 列级条件
    Column {
        column: String,
        op: CmpOp,
        value: Expr,      // Value(Param) / Column(String) / RawExpr(String)
    },
    /// IN
    In {
        column: String,
        values: Vec<Expr>,
        negated: bool,
    },
    /// BETWEEN
    Between {
        column: String,
        low: Expr,
        high: Expr,
    },
    /// IS NULL / IS NOT NULL
    IsNull { column: String, negated: bool },
    /// EXISTS 子查询
    Exists(Box<QueryBuilder>, negated: bool),
    /// 子分组（括号嵌套）
    Group(Vec<WhereGroup>),
    /// 原始 SQL 片段（逃生舱）
    Raw(String, Vec<Param>),
}

pub enum Expr {
    Value(Param),
    Column(String),       // 列引用，不加引号，用于关联子查询
    Subquery(Box<QueryBuilder>),
    RawExpr(String),      // 原始表达式
}
```

### 可选条件

```rust
// ❤️ 核心动态 API：Option 自动跳过
pub fn eq_opt(self, val: Option<impl Into<Param>>) -> T {
    match val {
        Some(v) => self.eq(v),
        None    => self.target,   // 什么都不加
    }
}
```

### 条件分组

```rust
QueryBuilder::select(&["*"]).from("users")
    .and_group(|g| {
        g.or_where("status").eq("active")
         .or_where("status").eq("pending")
    })
    .and_group(|g| {
        g.or_where("dept").eq("eng")
         .or_where("role").in_(["dev", "qa", "ops"])
    })
    .build(&MssqlBackend);

// WHERE (status = @P1 OR status = @P2)
//   AND (dept = @P3 OR role IN (@P4, @P5, @P6))
```

`and_group` = `AND (`，`or_group` = `OR (`。组内每个条件独立标记自己的 `LogicOp`，相邻条件之间插入 `AND`/`OR`。`GroupBuilder` 拥有和 `QueryBuilder` 相同的 where 方法，支持递归嵌套：

```rust
.or_group(|g1| {
    g1.and_where("a").eq(1)
      .or_group(|g2| {
          g2.and_where("b").eq(2)
            .and_group(|g3| {
                g3.or_where("c").in_([3, 4, 5])
                   .or_where("d").eq(6)
            })
      })
})
```

### when 条件守卫

```rust
// 用闭包和条件守卫
pub fn when(self, cond: bool, f: impl FnOnce(Self) -> Self) -> Self {
    if cond { f(self) } else { self }
}

// 使用
QueryBuilder::select(&["*"]).from("users")
    .and_where("name").like_opt(name)
    .when(!tags.is_empty(), |q| {
        q.and_where("tag").in_(tags.iter().copied())
    })
    .when(min_age.is_some(), |q| {
        q.and_where("age").gte(min_age.unwrap())
    })
```

---
## 7. JOIN

### 数据结构

```rust
#[derive(Clone)]
pub enum JoinType {
    Inner,
    Left,
    Right,
    Full,
    Cross,
}

#[derive(Clone)]
pub struct JoinClause {
    join_type: JoinType,
    table: TableRef,
    alias: Option<String>,
    conditions: Vec<OnCondition>,
}

pub enum TableRef {
    Table(String),
    TableAs(String, String),                      // 表 + 别名
    Subquery(Box<QueryBuilder>, String),           // 子查询 + 别名
    CteRef(String, Option<String>),                // CTE 引用 + 可选别名
}

pub enum OnCondition {
    /// col1 = col2（关联列引用，最常用的形式）
    Eq { left: String, right: String },
    /// col = value（参数化条件）
    EqValue { column: String, op: CmpOp, value: Param },
    /// AND / OR 子分组
    Group { logic: LogicOp, conditions: Vec<OnCondition> },
    /// 原始 SQL 片段（逃生舱）
    Raw(String, Vec<Param>),
}
```

### Fluent API

基础 JOIN（占所有 JOIN 的 ~85%）：

```rust
QueryBuilder::select(&["id", "name", "d.name AS dept_name"])
    .from("users")
    .join("departments", "d")                      // INNER JOIN departments AS d
        .on("users.dept_id", "d.id")               //   ON users.dept_id = d.id
    .left_join("orders", "o")                      // LEFT JOIN orders AS o
        .on("users.id", "o.user_id")                //   ON users.id = o.user_id
    .build(&MssqlBackend);

// SELECT id, name, d.name AS dept_name
// FROM users
// INNER JOIN departments AS d ON users.dept_id = d.id
// LEFT JOIN orders AS o ON users.id = o.user_id
```

JOIN 类型：

```rust
.join("table", "t")       // → INNER JOIN
.left_join("table", "t")  // → LEFT JOIN
.right_join("table", "t") // → RIGHT JOIN
.full_join("table", "t")  // → FULL OUTER JOIN
.cross_join("table")      // → CROSS JOIN（无 ON）
```

多条件 ON（参数化）：

```rust
.join("orders", "o")
    .on("users.id", "o.user_id")
    .and_on("o.status").eq("active")
    .and_on("o.amount").gte(1000)
// → INNER JOIN orders AS o
//   ON users.id = o.user_id AND o.status = @P1 AND o.amount >= @P2
// 参数: ["active", 1000]
```

子查询 JOIN：

```rust
let recent = QueryBuilder::select(&["user_id", "MAX(amount) AS top_amt"])
    .from("orders")
    .group_by("user_id");

QueryBuilder::select(&["u.name", "r.top_amt"])
    .from("users", "u")
    .join_subquery(recent, "r")
        .on("u.id", "r.user_id")
    .build(&PgBackend);

// SELECT u.name, r.top_amt
// FROM users AS u
// INNER JOIN (SELECT user_id, MAX(amount) AS top_amt FROM orders GROUP BY user_id) AS r
//   ON u.id = r.user_id
```

CTE JOIN：

```rust
QueryBuilder::select(&["e.id", "e.name", "t.max_sal"])
    .from("employees", "e")
    .join_cte("top_salaries", "t")
        .on("e.dept_id", "t.dept_id")
    .build(&MssqlBackend);

// SELECT e.id, e.name, t.max_sal
// FROM employees AS e
// INNER JOIN top_salaries AS t ON e.dept_id = t.dept_id
```

ON 条件分组：

```rust
.join("orders", "o")
    .on("users.id", "o.user_id")
    .and_group(|og| {
        og.or_on("o.status").eq("pending")
          .or_on("o.status").eq("active")
    })
// → INNER JOIN orders AS o
//   ON users.id = o.user_id
//   AND (o.status = @P1 OR o.status = @P2)
```

### JoinAdder / OnAdder 中间态

```rust
pub fn join<T: HasJoins>(mut self, table: &str, alias: &str) -> JoinAdder<T> {
    self.joins.push(JoinClause { join_type: JoinType::Inner, table: table.into(), alias: Some(alias.into()), conditions: vec![] });
    JoinAdder { target: self, join_idx: self.joins.len() - 1 }
}

pub struct JoinAdder<T> {
    target: T,
    join_idx: usize,
}

impl<T: HasJoins> JoinAdder<T> {
    /// 主 ON 条件：left_col = right_col
    pub fn on(mut self, left: &str, right: &str) -> T {
        self.target.add_join_cond(self.join_idx, OnCondition::Eq {
            left: left.into(), right: right.into(),
        });
        self.target
    }

    /// AND 附加条件（列 = 值）
    pub fn and_on(self, column: &str) -> OnAdder<T> { ... }
    pub fn or_on(self, column: &str) -> OnAdder<T> { ... }

    /// AND / OR 子分组
    pub fn and_group(self, f: impl FnOnce(OnGroupBuilder) -> OnGroupBuilder) -> T { ... }
    pub fn or_group(self, f: impl FnOnce(OnGroupBuilder) -> OnGroupBuilder) -> T { ... }
}

pub struct OnAdder<T> {
    target: T,
    join_idx: usize,
    column: String,
    logic: LogicOp,
}

impl<T: HasJoins> OnAdder<T> {
    pub fn eq(self, val: impl Into<Param>) -> T { ... }
    pub fn ne(self, val: impl Into<Param>) -> T { ... }
    pub fn gt(self, val: impl Into<Param>) -> T { ... }
    pub fn gte(self, val: impl Into<Param>) -> T { ... }
    pub fn lt(self, val: impl Into<Param>) -> T { ... }
    pub fn lte(self, val: impl Into<Param>) -> T { ... }
}
```

### build 中的 JOIN 处理

```rust
fn build_joins<B: Backend>(&self, joins: &[JoinClause], backend: &B,
    idx: &mut usize, params: &mut Vec<Param>) -> String
{
    let mut sql = String::new();
    for join in joins {
        write!(sql, " {} {} {}",
            join.join_type.sql(),
            join.table.sql(backend, idx, params),
            join.alias_str()).unwrap();

        if !join.conditions.is_empty() {
            sql.push_str(" ON ");
            for (i, cond) in join.conditions.iter().enumerate() {
                if i > 0 {
                    sql.push_str(match cond.logic() {
                        LogicOp::And => " AND ",
                        LogicOp::Or  => " OR ",
                    });
                }
                match cond {
                    OnCondition::Eq { left, right } => {
                        write!(sql, "{} = {}",
                            backend.quote_ident(left),
                            backend.quote_ident(right));
                    }
                    OnCondition::EqValue { column, op, value } => {
                        *idx += 1;
                        write!(sql, "{} {} {}",
                            backend.quote_ident(column), op.sql(), backend.placeholder(*idx));
                        params.push(value.clone());
                    }
                    OnCondition::Group { conditions: sub, .. } => {
                        sql.push('(');
                        // 递归子条件
                        sql.push(')');
                    }
                    OnCondition::Raw(expr, extra) => {
                        sql.push_str(expr);
                        params.extend(extra.iter().cloned());
                        *idx += extra.len();
                    }
                }
            }
        }
    }
    sql
}
```

**参数索引连续性**：ON 条件中的参数化值（`EqValue`）和 WHERE / 子查询共享同一个 `idx`，展开后连续。

| 条件类型 | 参数行为 | 示例 |
|---------|---------|------|
| `Eq { left, right }` | 无参数，纯列引用 | `users.id = o.user_id` |
| `EqValue { value }` | 1 个参数，idx++ | `o.status = @P1` |
| `Raw(sql, extra)` | extra 展开 | `o.date >= DATEADD(day, @P1, GETDATE())` |
| `Group` | 递归累加 | `o.status = @P2 OR o.status = @P3` |

### 后端差异

| 特性 | Postgres | MySQL | MSSQL | SQLite |
|------|----------|-------|-------|--------|
| INNER JOIN | ✅ 标准 | ✅ | ✅ | ✅ |
| LEFT JOIN | ✅ | ✅ | ✅ | ✅ |
| RIGHT JOIN | ✅ | ✅ | ✅ | ⚠️ 3.35.0+ |
| FULL JOIN | ✅ | ❌ | ✅ | ⚠️ 3.35.0+ |
| CROSS JOIN | ✅ | ✅ | ✅ | ✅ |
| 子查询 JOIN | ✅ | ✅ | ✅ | ✅ |
| CTE JOIN | ✅ | ✅ | ✅ | ✅ |
| ON 条件参数化 | ✅ | ✅ | ✅ | ✅ |
| NATURAL JOIN | ⚠️ | ✅ | ❌ | ✅ |

**差异处理策略**：Builder 不做自动降级。调用不支持的 JOIN 类型时 build 返回 `Err(UnsupportedJoinType)`。

---

## 8. INSERT

### 单行

```rust
// 使用
let (sql, params) = QueryBuilder::insert_into("users")
    .set("name", "alice")
    .set("age", 30)
    .set("dept", "eng")
    .build(&MssqlBackend);

// output: INSERT INTO users (name, age, dept) VALUES (@P1, @P2, @P3)
// params: ["alice", 30, "eng"]
```

等同于 `INSERT ... VALUES (...)`，**所有后端输出形态一样**，区别只在占位符风格。

### 获取自增 ID / 插入后返回值

这是后端差异最大的地方，用 `returning()` 统一 API：

```rust
let q = QueryBuilder::insert_into("users")
    .set("name", "alice")
    .returning("id");
```

各后端输出：

```sql
-- Postgres: INSERT INTO users (name, age) VALUES ($1, $2) RETURNING id
-- MSSQL:    INSERT INTO users (name, age) OUTPUT INSERTED.id VALUES (@P1, @P2)
-- MySQL:    INSERT INTO users (name, age) VALUES (?, ?); SELECT LAST_INSERT_ID()
-- SQLite:   INSERT INTO users (name, age) VALUES (?, ?); SELECT last_insert_rowid()
```

`Backend::supports_returning()` 控制行为：

- `true`（Postgres/MSSQL）：直接在 INSERT 语句内嵌入 `RETURNING`/`OUTPUT INSERTED.`
- `false`（MySQL/SQLite）：`build()` 返回包含两条语句的 `QueryResult.statements`

### INSERT FROM SELECT

```rust
let sub = QueryBuilder::select(&["id", "name"])
    .from("temp_users")
    .and_where("batch").eq(batch_id);

QueryBuilder::insert_into("users")
    .columns(&["id", "name"])
    .from_select(sub)
    .build(&PostgresBackend);

// INSERT INTO users (id, name) SELECT id, name FROM temp_users WHERE batch = $1
// 子查询的 params 合并到外层
```

---

## 9. UPSERT / ON CONFLICT

```rust
// DO NOTHING
QueryBuilder::insert_into("users")
    .set("id", 1).set("name", "alice")
    .on_conflict(&["id"], ConflictAction::DoNothing)
    .build(&PgBackend);

// DO UPDATE
.on_conflict(&["id"], ConflictAction::DoUpdate {
    set: vec![("name", "alice".into())],
    set_excluded: &["name", "age"],  // 引用 EXCLUDED
})
```

各后端输出：

```sql
-- Postgres: ON CONFLICT (id) DO UPDATE SET name = EXCLUDED.name, age = EXCLUDED.age
-- MySQL:    ON DUPLICATE KEY UPDATE name = VALUES(name), age = VALUES(age)
-- SQLite:   ON CONFLICT(id) DO UPDATE SET name = excluded.name
-- MSSQL:    整个语句转为 MERGE（Backend::build_merge()）
```

MSSQL 最特殊——`ON CONFLICT` 不存在，必须用 `MERGE`。`MssqlBackend` 在检测到 `conflict` 字段时，调用 `build_merge()` 生成完整的 `MERGE ... WHEN MATCHED THEN UPDATE ... WHEN NOT MATCHED THEN INSERT ...` 语句。

---

## 10. BULK INSERT

### API

```rust
let (sql, params) = QueryBuilder::insert_into("users")
    .columns(&["name", "age", "dept"])
    .rows()
        .row(&["alice", 30, "eng"])
        .row(&["bob", 25, "sales"])
        .row(&["carol", 28, "eng"])
    .build(&MssqlBackend);

// INSERT INTO users (name, age, dept)
// VALUES (@P1, @P2, @P3), (@P4, @P5, @P6), (@P7, @P8, @P9)
// params: ["alice", 30, "eng", "bob", 25, "sales", "carol", 28, "eng"]
```

### RowCollector 设计

```rust
pub struct RowCollector<'a> {
    builder: &'a mut InsertBuilder,
}

impl<'a> RowCollector<'a> {
    pub fn row(mut self, vals: &[impl Into<Param>]) -> Self {
        let params: Vec<Param> = vals.iter().map(|v| v.clone().into()).collect();
        if !self.builder.values.is_empty() {
            assert_eq!(params.len(), self.builder.columns.len(),
                       "Bulk insert: row has {} values, expected {}",
                       params.len(), self.builder.columns.len());
        }
        self.builder.values.push(params);
        self
    }
}
```

### 批量 RETURNING

- **Postgres**: `INSERT ... VALUES (...), (...) RETURNING id` → 多行结果集
- **MSSQL**: `INSERT ... OUTPUT INSERTED.id VALUES (@P1, @P2), (@P3, @P4)` → 多行结果集
- **MySQL/SQLite**: `supports_bulk_returning() = false`，降级为 N 条单行 INSERT + `LAST_INSERT_ID()`

```rust
// MySQLBackend 降级后:
// statements: [
//   ("INSERT INTO users (name, age) VALUES (?, ?)", ["alice", 30]),
//   ("SELECT LAST_INSERT_ID()", []),
//   ("INSERT INTO users (name, age) VALUES (?, ?)", ["bob", 25]),
//   ("SELECT LAST_INSERT_ID()", []),
// ]
```

### 可选 FromRow trait

```rust
pub trait FromRow {
    fn insert_values(&self) -> Vec<Param>;
}

// 派生宏（可选 feature）
#[derive(InsertRow)]
struct User { name: String, age: i32, dept: String }

// 使用
let mut collector = builder.rows();
for user in &users { collector = collector.add(user); }
```

---

## 11. UPDATE

### 基础

```rust
QueryBuilder::update("users")
    .set("name", "bob")
    .set("age", 25)
    .and_where("id").eq(42)
    .build(&MssqlBackend);

// UPDATE users SET name = @P1, age = @P2 WHERE id = @P3
```

### 动态 SET（❤️ 核心）

```rust
fn update_user(id: i32, name: Option<&str>, age: Option<i32>) -> (String, Vec<Param>) {
    QueryBuilder::update("users")
        .set_opt("name", name)     // None → 跳过
        .set_opt("age", age)
        .and_where("id").eq(id)
        .build(&MssqlBackend)
}
```

**安全校验**：`build()` 时 `set_list` 为空则返回 `Err(UpdateError::NoSetClauses)`。

### 关联子查询

```rust
let sub = QueryBuilder::select(&["name"])
    .from("dept")
    .and_where_raw("dept.id = users.dept_id", vec![]);

QueryBuilder::update("users")
    .set_subquery("dept_name", sub)
    .and_where("dept_id").eq(1)
    .build(&PgBackend);

// UPDATE users SET dept_name = (SELECT name FROM dept WHERE dept.id = users.dept_id)
// WHERE dept_id = $1
```

---

## 12. DELETE

```rust
QueryBuilder::delete_from("users")
    .and_where("id").eq(42)
    .build(&MssqlBackend);

// DELETE FROM users WHERE id = @P1
```

支持 RETURNING（Postgres/MSSQL）：

```rust
QueryBuilder::delete_from("users")
    .returning("id")
    .and_where("age").lt(18)
    .build(&PostgresBackend);

// DELETE FROM users WHERE age < $1 RETURNING id
```

---

## 13. 子查询

### 参数连续性

**核心约束**：子查询和主查询共享同一套参数空间。子查询的 `Param` 展开到外层，索引连续排布。`build()` 递归时 `param_offset` 作为可变引用传递。

```rust
fn build_select(&self, node: &SelectNode, param_offset: &mut usize) -> (String, Vec<Param>)
```

索引顺序 = SQL 中的出现顺序（CTE → 主查询 → 子查询，后序遍历），由参数展开逻辑自然保证。

### 子查询种类

| 位置 | Fluent API | SQL |
|------|-----------|-----|
| WHERE IN | `.and_where("col").in_subquery(sub)` | `col IN (SELECT ...)` |
| EXISTS | `.and_exists(sub)` | `EXISTS (SELECT ...)` |
| FROM 派生表 | `.from_subquery(sub, "t")` | `FROM (SELECT ...) AS t` |
| JOIN 派生表 | `.join_subquery(sub, "t").on(...)` | `JOIN (SELECT ...) AS t ON ...` |
| SELECT 列 | `.select_subquery(sub, "n")` | `SELECT (SELECT ...) AS n` |

### 关联子查询 / 列引用

```rust
// 关联引用：列 = 列
.and_where("orders.user_id").eq_col("users.id")
// → WHERE orders.user_id = users.id

// EXISTS 关联
.and_exists(
    QueryBuilder::select_raw("1").from("orders")
        .and_where("orders.user_id").eq_col("users.id")
)
```

`Expr` 枚举区分参数值和列名：

```rust
pub enum Expr {
    Value(Param),          // 参数化值
    Column(String),        // 列引用（不加引号）
    Subquery(Box<QueryBuilder>),
    RawExpr(String),       // 原始表达式
}
```

### 深度嵌套示例（参数连续性检验）

```rust
let inner_inner = QueryBuilder::select(&["id"])
    .from("blacklist").and_where("reason").eq("fraud");

let inner = QueryBuilder::select(&["user_id"])
    .from("orders").and_where("amount").gt(1000)
    .and_where("user_id").in_subquery(inner_inner);

let (sql, params) = QueryBuilder::select(&["id", "name"]).from("users")
    .and_where("id").in_subquery(inner)
    .and_where("status").eq("active")
    .build(&PostgresBackend);

// params: ["active", 1000, "fraud"]
//          ^- $1    ^- $2   ^- $3
//
// SELECT id, name FROM users
// WHERE id IN (
//   SELECT user_id FROM orders
//   WHERE amount > $2 AND user_id IN (
//     SELECT id FROM blacklist WHERE reason = $3
//   )
// ) AND status = $1
```

后序遍历：最深层子查询先分配参数索引。

### 原始 SQL 逃生舱

```rust
.in_subquery_raw("SELECT id FROM dbo.fn_GetSubordinates(@P1)", vec![Param::I32(id)])
```

参数仍然独立传递，不拼进 SQL 字符串。

---

## 14. CTE（WITH 子句）

### 核心数据结构

```rust
pub struct CteNode {
    name: String,
    columns: Option<Vec<String>>,     // WITH name (col1, col2)
    recursive: bool,
    body: CteBody,
}

pub enum CteBody {
    /// 单次查询：AS (SELECT ...)
    Query(Box<QueryBuilder>),
    /// 递归：AS (anchor UNION ALL recursive)
    RecursiveUnion {
        anchor: Box<QueryBuilder>,
        recursive: Box<QueryBuilder>,
        union_type: UnionType,
    },
}
```

### Fluent API

```rust
// 简单 CTE
let active = QueryBuilder::select(&["id", "name"])
    .from("users").and_where("status").eq("active");

QueryBuilder::select(&["name"])
    .with_cte("active_users", active)        // ← WITH 在先
    .from_cte_ref("active_users")
    .and_where("id").gt(100)
    .build(&PgBackend);

// WITH active_users AS (SELECT id, name FROM users WHERE status = $1)
// SELECT name FROM active_users WHERE id > $2
```

```rust
// 可选列名
.from_cte_with_columns(builder, "stats", &["max_age", "min_age"])
// → WITH stats (max_age, min_age) AS (SELECT ...)
```

```rust
// 递归 CTE
QueryBuilder::recursive_cte("org_tree", &["id", "parent_id", "name", "level"])
    .as_union(
        // 锚点
        QueryBuilder::select(&["id", "parent_id", "name", "1 AS level"])
            .from("employees").and_where("id").eq(1),
        // 递归成员
        QueryBuilder::select(&["e.id", "e.parent_id", "e.name", "org_tree.level + 1"])
            .from("employees", "e")
            .and_join_cte("org_tree").on("e.parent_id").eq_col("org_tree.id"),
    )
    .select(&["id", "name", "level"])
    .from_cte_ref("org_tree")
    .build(&PgBackend);

// WITH RECURSIVE org_tree (...) AS (
//   SELECT ... WHERE id = $1
//   UNION ALL
//   SELECT e.... FROM employees e JOIN org_tree ON e.parent_id = org_tree.id
// )
// SELECT ... FROM org_tree
// params: [1]
```

### 参数连续性

CTE 在 `build()` 中**优先展开**，然后才构建主查询：

```rust
fn build_select(&self, node: &SelectNode, param_offset: &mut usize) -> (String, Vec<Param>) {
    // 1. WITH 子句（参数优先）
    if !node.ctes.is_empty() {
        let (cte_sql, cte_params) = self.build_ctes(&node.ctes, param_offset);
        all_params.extend(cte_params);
    }
    // 2. 主查询（参数继续）
    let (rest_sql, rest_params) = self.build_select_body(&node, param_offset);
    all_params.extend(rest_params);
}
```

### 多 CTE

```rust
QueryBuilder::select(&["name", "salary"])
    .with_cte("eng_dept", cte1)
    .with_cte("eng_employees", cte2)
    .from_cte_ref("eng_employees")
    .build(&PgBackend);

// WITH
//   eng_dept AS (...),
//   eng_employees AS (SELECT ... WHERE dept_id IN (SELECT id FROM eng_dept))
// SELECT name, salary FROM eng_employees
```

CTE 按定义顺序求值，后续 CTE 可引用前面定义的 CTE。

### CTE + UPDATE / DELETE

```rust
QueryBuilder::update("employees")
    .with_cte("dept_bonus", cte)   // WITH ... AS (...)
    .set_raw("salary = salary * 1.1")
    .and_exists(
        QueryBuilder::select_raw("1").from_cte_ref("dept_bonus")
            .and_where("dept_bonus.id").raw_col("employees.id"),
    )
    .build(&MssqlBackend);
```

---

## 15. 执行器（Executor）

让 builder 不局限于 SQL 生成，而是提供可执行的 end-to-end 接口：

```rust
#[async_trait]
pub trait Executor {
    async fn execute(&mut self, sql: &str, params: &[Param]) -> Result<u64, Error>;
    async fn query_row(&mut self, sql: &str, params: &[Param]) -> Result<Vec<Param>, Error>;
    async fn query(&mut self, sql: &str, params: &[Param]) -> Result<Vec<Vec<Param>>, Error>;
}
```

为各驱动实现：

```rust
#[async_trait]
impl Executor for tiberius::Client<impl AsyncRead + AsyncWrite + Unpin + Send> {
    async fn execute(&mut self, sql: &str, params: &[Param]) -> Result<u64, Error> {
        let mut q = Query::new(sql);
        for p in params { q.bind(p); }
        let rows = q.execute(self).await?;
        Ok(rows.total())
    }
    // ...
}
```

Builder 上的快捷方法：

```rust
impl InsertBuilder {
    pub async fn execute(self, executor: &mut impl Executor) -> Result<Vec<Row>, Error> {
        let result = self.build(&self.backend);
        for (sql, params) in result.statements {
            if self.returning.is_some() {
                let batch = executor.query(&sql, &params).await?;
                rows.extend(batch);
            } else {
                executor.execute(&sql, &params).await?;
            }
        }
        Ok(rows)
    }
}
```

---

## 16. 后端差异对照表

### 占位符 / 引用

| 特性 | Postgres | MySQL | MariaDB | MSSQL | SQLite |
|------|----------|-------|---------|-------|--------|
| 占位符 | `$1` | `?` | `?` | `@P1` | `?` |
| 标识符引号 | `"col"` | `` `col` `` | `` `col` `` | `[col]` | `"col"` |

### DML 差异

| 特性 | Postgres | MySQL | MariaDB | MSSQL | SQLite |
|------|----------|-------|---------|-------|--------|
| 分页 | `LIMIT x OFFSET y` | 同上 | 同上 | `OFFSET x ROWS FETCH NEXT y ROWS ONLY` | 同 Postgres |
| 自增 ID | `RETURNING id` | `LAST_INSERT_ID()` | 同 MySQL | `OUTPUT INSERTED.id` | `last_insert_rowid()` |
| 批量 RETURNING | ✅ 单语句多行 | ❌ 逐条 | ❌ 逐条 | ✅ 单语句多行 | ❌ 逐条 |
| UPSERT | `ON CONFLICT DO` | `ON DUPLICATE KEY` | 同 MySQL | `MERGE` | `ON CONFLICT(...) DO` |
| 布尔字面量 | `TRUE`/`FALSE` | `1`/`0` | `1`/`0` | `1`/`0` | `1`/`0` |
| 递归 CTE | `WITH RECURSIVE` | `WITH RECURSIVE` (8.0+) | `WITH RECURSIVE` | `WITH`（隐式递归） | `WITH RECURSIVE` |

### 子查询差异

| 场景 | Postgres | MSSQL | MySQL | SQLite |
|------|----------|-------|-------|--------|
| WHERE IN (SELECT) | ✅ | ✅ | ✅ | ✅ |
| FROM (SELECT) | ✅ | ✅ | ✅ | ✅ |
| 子查询有 LIMIT | ✅ | ⚠️ OFFSET...FETCH 允许 | ✅ | ✅ |
| 标量子查询 | ✅ | ✅ | ⚠️ 性能差 | ✅ |

---

## 17. 安全底线

### 硬规则

1. **Builder API 上不提供直接拼接值的接口**：`where_raw("col = ?", val)` 必须配合独立 `Vec<Param>`
2. **IN () 空子句**：`build()` 时检查 `values.is_empty()` → 返回 `Err(BuildError::EmptyInClause)`
3. **UPDATE 无 SET**：`set_list` 为空 → 返回 `Err(UpdateError::NoSetClauses)`
4. **列名白名单**：`ORDER BY` 的列名不接受任意字符串，必须从白名单中选取
5. **Null 安全**：用 `Option<T>` 表达数据库 NULL，`None` → `Param::Null`

### 安全分层

```
用户输入
    │
    ▼
Param::from(...)              ← From trait 强制转换
    │
    ▼
QueryBuilder.and_where(...)   ← 只接受 Param，不接受 &str 做值
    │
    ▼
WhereKind::Column {            ← 列名与值分离
    column: "name",           ← 列名是 SQL 字符串（白名单）
    value: Expr::Value(Param) ← 值参数化
}
    │
    ▼
Backend::placeholder(i)       ← 按后端生成占位符
    │
    ▼
(Vec<Param>, String)          ← 交给驱动执行
```

---

## 18. Feature 条件编译

```toml
# Cargo.toml
[features]
default = ["postgresql"]

# 数据库后端
postgresql = ["dep:chrono"]
mysql      = ["dep:chrono"]
mariadb    = ["dep:chrono"]
mssql      = []
sqlite     = []

# 可选增强
chrono          = ["dep:chrono"]              # 时间类型支持
derive          = ["dep:graft_derive"]  # FromRow 派生宏
```
---

## 19. Roadmap / 未定事项

- [ ] **MSSQL MERGE**：`on_conflict` 的完整实现。是把整个 INSERT 重写为 MERGE，还是用户必须显式调用 `.merge_into()`？
- [ ] **子查询 LIMIT 在 MSSQL**：2012+ 的 `OFFSET...FETCH` 在子查询中是否完全兼容？是否需自动包裹派生表？
- [ ] **CTE IN UPDATE/DELETE**：`with_cte()` 在 INSERT/UPDATE/DELETE builder 上的 API 统一性
- [ ] **批量 RETURNING 降级**：MySQL/SQLite 的逐条插入 + LAST_INSERT_ID 可能导致非原子性，是否应在事务内执行？
- [ ] **Enum 类型**：是否支持自定义 `#[sql_enum]` 派生枚举？
- [ ] **类型映射**：`Param` 到各驱动原生类型（tiberius: `ColumnData`、sqlx: `Encode`）的映射层
- [ ] **编译期 SQL 校验**：是否可选将 build 结果喂给 sqlx 做编译期校验？
- [ ] **Schema 感知**：从数据库 schema 生成类型安全的 Builder 包装器

---

> **核心原则回顾**：
> - 所有用户输入通过 `Param`，SQL 字符串只含关键字、标识符、占位符
> - Builder 是 AST 层，方言差异由 `Backend trait` 封装
> - 参数索引在 `build()` 递归中自然连续，不做额外假设
> - 安全底线：`raw` 方法也要求参数独立传递
> - 动态能力：`eq_opt` / `set_opt` / `when` 是核心 API
