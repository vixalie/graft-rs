# graft (Rust)

轻量多后端动态 SQL 查询构建器，Rust 实现。

> **命名约定**：仓库 / 项目名为 `graft-rs`，crates.io 上的 crate 名为 `graft`。
>
> ⚠️ **当前处于早期开发阶段（0.1.0 alpha）**：核心骨架完整，部分细节与安全校验尚在补齐中，**暂不建议生产使用**。详见 [项目状态](#项目状态)。

## 设计哲学

- **"SQL 字符串骨架 + 参数填充"**：所有用户输入走 `Param` 枚举，SQL 字符串只含关键字、标识符、占位符
- **编译期类型安全**：`Param` 配合 `From<T>` trait —— 用户不会"忘记"参数化
- **Result 优于 panic**：所有可恢复错误经由 `BuildResult<T>`，调用方决定处理策略
- **直觉式链式调用**：`and_where("col").eq(val)` 直接对应 `WHERE col = ?` 的写 SQL 直觉
- **可选条件原生支持**：`eq_opt` / `set_opt` / `when` —— `None` 自动跳过
- **闭包即括号**：`and_group(|g| {...})` 直接对应 `AND (...)`
- **多后端编译期选**：Cargo feature flags 决定参与编译的后端，零运行时开销
- **零生产依赖**：核心 crate 不引入任何运行时依赖，可选 `chrono` 时间类型按 feature 加入

## 特性

> 图例：✅ 已实现 / 🚧 实现中或部分实现 / 🗓️ 路线图内

- ✅ CRUD 全覆盖：SELECT / INSERT / UPDATE / DELETE
- ✅ 5 种后端方言：Postgres / MySQL / MariaDB / MSSQL / SQLite
- ✅ 丰富的 WHERE 条件：`=`、`<>`、`>`、`>=`、`<`、`<=`、`LIKE`、`IN`、`BETWEEN`、`IS NULL` / `IS NOT NULL`、`EXISTS` / `NOT EXISTS`
- ✅ WHERE 分组嵌套：`and_group` / `or_group` 无限嵌套
- ✅ 条件守卫 `when(cond, |q| ...)`
- ✅ JOIN 支持：`INNER` / `LEFT` / `RIGHT` / `FULL OUTER` / `CROSS`，含参数化 ON 条件与子分组
- ✅ 子查询：FROM 子查询、JOIN 子查询、WHERE `IN` 子查询、`EXISTS`
- ✅ CTE：普通 `WITH` 与递归 `WITH RECURSIVE`
- ✅ GROUP BY / HAVING / ORDER BY / LIMIT / OFFSET（MSSQL 自动改写为 `OFFSET … FETCH NEXT`）
- ✅ INSERT 批量插入、`INSERT … SELECT`
- ✅ RETURNING（Postgres）/ OUTPUT（MSSQL）
- ✅ UPSERT：`ON CONFLICT` (Postgres / SQLite) / `ON DUPLICATE KEY` (MySQL / MariaDB)
- ✅ `#[derive(InsertRow)]` —— 派生宏自动生成 `FromRow::insert_values()`
- ✅ Param 枚举强制参数化：`bool` / `i8`~`i64` / `f32` / `f64` / `String` / `&str` / `Option<T>` / `Vec<u8>` / `chrono`
- ✅ `Executor` trait（feature-gated，async）—— 端到端执行抽象，等待驱动适配
- 🚧 部分可选条件变体（仅 `eq_opt` / `like_opt` / `in_opt` / `set_opt`）—— 其余 `*_opt` 在路线图 Phase 2
- 🚧 LIKE 安全（当前 `like()` 走 `RawExpr`，路线图 Phase 1 改为参数化）
- 🗓️ `select_ident` / `group_by_ident` / `order_by_safe` —— 智能列名引用与白名单
- 🗓️ `and_where_expr` / `or_where_expr` —— 函数表达式 WHERE
- 🗓️ `*_subquery` 子查询比较运算符（`eq_subquery` 等）
- 🗓️ `allow_unsafe_update` / `allow_unsafe_delete` —— 显式放行无 WHERE 的 UPDATE/DELETE
- 🗓️ MSSQL `MERGE` 形式的 UPSERT

完整路线图见 [`docs/roadmap.md`](docs/roadmap.md)。

## 安装

`Cargo.toml`：

```toml
[dependencies]
graft = "0.1"
```

或启用其他后端 / 派生宏：

```toml
[dependencies]
graft = { version = "0.1", default-features = false, features = ["postgresql", "mysql", "derive", "chrono"] }
```

可用 feature：

| Feature      | 说明                                              | 默认 |
|--------------|---------------------------------------------------|:----:|
| `postgresql` | Postgres 后端                                     |  ✅  |
| `mysql`      | MySQL 后端                                        |      |
| `mariadb`    | MariaDB 后端                                      |      |
| `mssql`      | MSSQL 后端                                        |      |
| `sqlite`     | SQLite 后端                                       |      |
| `chrono`     | `NaiveDateTime` / `DateTime<Utc>` 参数支持        |      |
| `derive`     | `#[derive(InsertRow)]` 派生宏                     |      |
| `executor`   | `Executor` trait（异步执行抽象，需 `async-trait`）|      |

## 快速开始

### SELECT

```rust
use graft::{QueryBuilder, QueryResult, backends::PostgresBackend};

let QueryResult { sql, params, .. } = QueryBuilder::select(&["id", "name"])
    .from("users")
    .and_where("status").eq("active")
    .and_where("age").gte(18)
    .build(&PostgresBackend)
    .unwrap();

// sql:    SELECT id, name FROM "users" WHERE "status" = $1 AND "age" >= $2
// params: [Text("active"), I32(18)]
```

### INSERT

```rust
use graft::{QueryBuilder, QueryResult, backends::MysqlBackend};

let QueryResult { sql, params, .. } = QueryBuilder::insert_into("users")
    .set("name", "alice")
    .set("age", 30i32)
    .set("dept", "eng")
    .build(&MysqlBackend)
    .unwrap();

// sql:    INSERT INTO `users` (`name`, `age`, `dept`) VALUES (?, ?, ?)
// params: [Text("alice"), I32(30), Text("eng")]
```

### UPDATE

```rust
use graft::{QueryBuilder, QueryResult, backends::MysqlBackend};

let QueryResult { sql, params, .. } = QueryBuilder::update("users")
    .update_set("name", "bob")
    .update_set("age", 25i32)
    .and_where("id").eq(42i32)
    .build(&MysqlBackend)
    .unwrap();

// sql:    UPDATE `users` SET `name` = ?, `age` = ? WHERE `id` = ?
// params: [Text("bob"), I32(25), I32(42)]
```

### DELETE

```rust
use graft::{QueryBuilder, QueryResult, backends::MysqlBackend};

let QueryResult { sql, params, .. } = QueryBuilder::delete_from("users")
    .and_where("id").eq(42i32)
    .build(&MysqlBackend)
    .unwrap();

// sql:    DELETE FROM `users` WHERE `id` = ?
// params: [I32(42)]
```

## 后端支持

| 后端     | 占位符  | 标识符引用 | RETURNING / OUTPUT | UPSERT                  |
|----------|---------|------------|--------------------|-------------------------|
| Postgres | `$1`    | `"col"`    | `RETURNING`        | `ON CONFLICT DO …`      |
| MySQL    | `?`     | `` `col` ``| 不支持             | `ON DUPLICATE KEY UPDATE` |
| MariaDB  | `?`     | `` `col` ``| 不支持             | `ON DUPLICATE KEY UPDATE` |
| MSSQL    | `@P1`   | `[col]`    | `OUTPUT INSERTED.` | 🗓️ MERGE（路线图）      |
| SQLite   | `?`     | `"col"`    | 关闭（兼容旧版本） | `ON CONFLICT DO …`      |

`build(&backend)` 时传入对应后端实例：

```rust
use graft::{QueryBuilder, backends::*};

let q = QueryBuilder::select(&["*"]).from("users");
q.clone().build(&PostgresBackend).unwrap();   // $1, $2 ...
q.clone().build(&MysqlBackend).unwrap();      // ? ?
q.clone().build(&MssqlBackend).unwrap();      // @P1 @P2
q.clone().build(&SqliteBackend).unwrap();     // ? ?
q.build(&MariaDbBackend).unwrap();            // ? ? (同 MySQL)
```

## 高级用法

### 可选条件（`None` 自动跳过）

```rust
use graft::{QueryBuilder, backends::PostgresBackend};

let name: Option<&str> = None;
let dept: Option<String> = Some("eng".into());

let result = QueryBuilder::select(&["*"]).from("users")
    .and_where("name").like_opt(name)   // None → 跳过条件
    .and_where("dept").eq_opt(dept)     // Some → 生成 = $n
    .build(&PostgresBackend)
    .unwrap();

// sql:    SELECT * FROM "users" WHERE "dept" = $1
// params: [Text("eng")]
```

UPDATE 同样支持：

```rust
QueryBuilder::update("users")
    .set_opt("name", Some("alice"))
    .set_opt("age", None::<i32>)        // 跳过
    .and_where("id").eq(1i32)
    .build(&PostgresBackend)?;
```

### WHERE 分组

```rust
use graft::{QueryBuilder, backends::MysqlBackend};

let r = QueryBuilder::select(&["*"]).from("orders")
    .and_group(|g| {
        g.or_where("status").eq("active")
         .or_where("status").eq("pending")
    })
    .and_where("amount").gt(1000i32)
    .build(&MysqlBackend)
    .unwrap();

// WHERE (`status` = ? OR `status` = ?) AND `amount` > ?
```

### 条件守卫 `when`

```rust
let with_dept = true;

QueryBuilder::select(&["*"]).from("users")
    .when(with_dept, |q| q.and_where("dept").eq("eng"))
    .build(&PostgresBackend)?;
```

### JOIN

```rust
use graft::{QueryBuilder, backends::MysqlBackend};

QueryBuilder::select(&["users.id", "o.amount"]).from("users")
    .join("orders", "o")
        .on("users.id", "o.user_id")
    .and_where("o.status").eq("paid")
    .build(&MysqlBackend)?;

// INNER JOIN `orders` AS o ON users.id = o.user_id WHERE `o.status` = ?
```

`LEFT` / `RIGHT` / `FULL` / `CROSS` 全部支持。子查询 JOIN：

```rust
let totals = QueryBuilder::select(&["user_id", "SUM(amount) AS total"])
    .from("orders")
    .group_by(&["user_id"]);

QueryBuilder::select(&["u.id", "t.total"]).from_as("users", "u")
    .join_subquery(totals, "t")
        .on("u.id", "t.user_id")
    .build(&PostgresBackend)?;
```

### CTE / 递归 CTE

```rust
use graft::{QueryBuilder, backends::PostgresBackend, UnionType};

let recent = QueryBuilder::select(&["id", "name"]).from("users")
    .and_where("created_at").gt("2025-01-01");

QueryBuilder::select(&["id", "name"]).from_cte_ref("recent")
    .with_cte("recent", recent)
    .build(&PostgresBackend)?;

// WITH recent AS (SELECT id, name FROM "users" WHERE "created_at" > $1)
// SELECT id, name FROM recent
```

递归 CTE 示例：

```rust
let anchor = QueryBuilder::select(&["id", "manager_id"]).from("employees")
    .and_where("id").eq(1i32);

let step = QueryBuilder::select(&["e.id", "e.manager_id"]).from_as("employees", "e")
    .join_cte("chain", "c").on("e.id", "c.manager_id");

QueryBuilder::select(&["*"]).from_cte_ref("chain")
    .recursive_cte("chain", &["id", "manager_id"], anchor, step, UnionType::Union)
    .build(&PostgresBackend)?;
```

### RETURNING / OUTPUT

```rust
use graft::{QueryBuilder, backends::PostgresBackend};

QueryBuilder::insert_into("users")
    .set("name", "alice")
    .set("age", 30i32)
    .returning(&["id"])
    .build(&PostgresBackend)?;

// INSERT INTO "users" ("name", "age") VALUES ($1, $2) RETURNING id
```

MSSQL 自动改写为 `OUTPUT INSERTED.id`；MySQL / MariaDB / SQLite 因后端不支持，`returning()` 不会生成 RETURNING 子句（未来计划降级为多语句 `QueryResult::multi`）。

### UPSERT

```rust
use graft::{QueryBuilder, backends::PostgresBackend};

QueryBuilder::insert_into("users")
    .columns(&["id", "name", "age"])
    .rows()
        .row(&[1i32.into(), "alice".into(), 30i32.into()])
        .row(&[2i32.into(), "bob".into(),   25i32.into()]);

// + UPSERT
QueryBuilder::insert_into("users")
    .set("id",   1i32)
    .set("name", "alice")
    .on_conflict_do_update(
        &["id"],
        vec![("name", "alice")],
        &["name"], // SET excluded.name
    )
    .build(&PostgresBackend)?;

// INSERT INTO "users" ("id", "name") VALUES ($1, $2)
// ON CONFLICT ("id") DO UPDATE SET "name" = EXCLUDED."name"
```

MySQL / MariaDB 自动改写为 `ON DUPLICATE KEY UPDATE name = VALUES(name)`。

### 批量 INSERT 与 `INSERT … SELECT`

```rust
use graft::{QueryBuilder, Param, backends::PostgresBackend};

let mut q = QueryBuilder::insert_into("users").columns(&["name", "age"]);
q.rows()
    .row(&[Param::from("alice"), Param::from(30i32)])
    .row(&[Param::from("bob"),   Param::from(25i32)]);
q.build(&PostgresBackend)?;

// INSERT FROM SELECT
let archived = QueryBuilder::select(&["name", "age"]).from("users_old");
QueryBuilder::insert_into("users")
    .columns(&["name", "age"])
    .from_select(archived)
    .build(&PostgresBackend)?;
```

### 派生宏 `InsertRow`（Rust 专属）

启用 `derive` feature 后，可让结构体自动生成插入用的 `Vec<Param>`：

```rust
use graft::{InsertRow, FromRow, Param};

#[derive(InsertRow)]
struct User {
    name: String,
    age: i32,
    dept: String,
}

let u = User { name: "alice".into(), age: 30, dept: "eng".into() };
let values: Vec<Param> = u.insert_values();
// 配合 columns(&[...]) + rows() 完成批量插入
```

字段加 `#[insert_row(skip)]` 可跳过（如自增 id）。

### 子查询比较（`IN` / `EXISTS`）

```rust
let blocked = QueryBuilder::select(&["id"]).from("blocked_users")
    .and_where("status").eq("active");

QueryBuilder::select(&["*"]).from("orders")
    .and_where("user_id").in_subquery(blocked)
    .build(&PostgresBackend)?;

// 或 EXISTS
let sub = QueryBuilder::select(&["1"]).from("orders")
    .and_where("orders.user_id").eq_col("users.id");

QueryBuilder::select(&["*"]).from("users")
    .and_exists(sub)
    .build(&PostgresBackend)?;
```

### 原始 SQL 逃生舱

```rust
use graft::Param;

QueryBuilder::select(&["*"]).from("users")
    .and_where("created_at")
        .raw(">= NOW() - INTERVAL '? days'", vec![Param::from(7i32)])
    .build(&PostgresBackend)?;
```

**安全提醒**：`raw` 永远要求"SQL 片段 + 独立 `Vec<Param>`"两段式 —— 不允许把参数直接拼到字符串里。

## Workspace 结构

```
sql-query-builder/
├── Cargo.toml                          # workspace 根
├── graft/                              # facade crate —— 用户依赖此 crate
│   └── src/lib.rs                      # re-export + 统一 feature flags
├── graft-core/                         # 核心库
│   └── src/
│       ├── lib.rs                      # 公共 API 重导出
│       ├── param.rs                    # Param 枚举 + From impls
│       ├── types.rs                    # AST 节点（WhereGroup、JoinClause、CteNode...）
│       ├── builder.rs                  # QueryBuilder + 中间态（WhereAdder、JoinAdder、GroupBuilder）
│       ├── backend.rs                  # Backend trait（方言抽象层）
│       ├── result.rs                   # QueryResult + BuildError
│       ├── exec.rs                     # Executor trait（feature-gated）
│       └── backends/                   # 各方言实现，feature-gated
│           ├── postgres.rs
│           ├── mysql.rs
│           ├── mariadb.rs
│           ├── mssql.rs
│           └── sqlite.rs
└── graft-derive/                       # proc-macro crate
    └── src/lib.rs                      # #[derive(InsertRow)]
```

## 安全防护

graft 的安全底线：

1. **所有用户值经由 `Param` 枚举与占位符**，绝不拼入 SQL 字符串
2. **`raw()` 强制两段式签名**（SQL 片段 + 独立 `Vec<Param>`），不允许混合
3. **UPDATE 无 SET** → `Err(BuildError::NoSetClauses)`
4. **空 `IN ()` 子句** → 🚧 即将返回 `Err(BuildError::EmptyInClause)`（Phase 1）
5. **UPDATE / DELETE 无 WHERE** → 🗓️ 路线图 Phase 4 将默认拒绝，提供 `allow_unsafe_*` 逃生舱
6. **MSSQL OFFSET 必须 ORDER BY** → 🗓️ 路线图 Phase 4

完整安全审计计划见 `docs/roadmap.md` 第 5 节。

## 项目状态

| 维度             | 当前状态                                                 |
|------------------|----------------------------------------------------------|
| 编译             | ✅ `cargo build --all-features` 通过（3 个 dead_code warning） |
| 功能完整度       | ~85%（核心骨架完整，部分语法糖与可选条件待补齐）          |
| 测试覆盖率       | ⚠️ 0%（路线图 Phase 6 计划新增 ~80 个用例）                |
| 已知问题         | LIKE 当前未参数化（Phase 1 修复）；MSSQL UPSERT 占位      |
| 生产可用性       | ❌ 不推荐 —— 待 Phase 1（安全修复）与 Phase 6（测试）完成 |

设计备忘：[`docs/SQLQueryBuilder-Design-Memo.md`](docs/SQLQueryBuilder-Design-Memo.md)

## 测试

```bash
cargo test --all-features
```

代码格式与静态检查：

```bash
cargo fmt --all
cargo clippy --all-features -- -D warnings
```

## 许可证

本项目以 [Apache License 2.0](LICENSE) 发布。你可以自由使用、修改和分发，但须保留原版权声明与许可证文本；修改后的文件需标注变更，且作者不承担担保责任。
