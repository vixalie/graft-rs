# AGENTS.md

本文件定义本仓库中 Agent 的行为约束与项目特定规范。

目标：减少错误假设、过度工程、无关改动和不可验证的交付。

---

## 0) 总则

- 以用户请求为最高优先级。
- 在不违反用户要求的前提下，遵循本文件规则。
- 倾向谨慎而非速度；琐碎任务可适当简化流程。

---

## 1) 核心原则（Karpathy 风格）

### 1.1 编码前思考

**不要假设，不要隐藏困惑，明确呈现权衡。**

- 不确定时先提问，不猜测。
- 存在歧义时列出解释选项，不要静默选一种。
- 若有更简单方案，要明确提出。
- 发现信息不足时立即暂停并澄清。

### 1.2 简洁优先

**用最少必要改动解决问题，不做投机性扩展。**

- 不添加请求之外的功能。
- 不为一次性逻辑引入额外抽象。
- 不添加未被要求的灵活性/可配置性。
- 不为不可能场景增加复杂处理。
- 若实现明显可简化，应主动简化。

### 1.3 精准修改

**只改必须改的，且只清理自己引入的问题。**

- 不顺手改相邻代码、注释、格式。
- 不重构与当前任务无关的模块。
- 优先匹配现有代码风格与约定。
- 发现无关死代码：可提示，不主动删除。
- 若你的改动引入了未使用代码，必须清理。

### 1.4 目标驱动执行

**先定义"完成标准"，再循环验证直到达标。**

- "加校验" → "通过测试验证已有实现的正确性"。
- "修 bug" → "编写测试复现问题，验证修复"。
- "重构" → "确保重构后测试仍然通过"。

多步骤任务建议格式：

1. [步骤] → 验证：[检查项]
2. [步骤] → 验证：[检查项]
3. [步骤] → 验证：[检查项]

---

## 2) 项目概述

**sql-query-builder** — 多后端动态 SQL 查询构建器。

定位：介于 `sqlx::QueryBuilder`（灵活但 raw）和 `sea-query`（完备但沉重）之间。提供编译期强制参数化（杜绝 SQL 注入）、Fluent API 与 SQL 逻辑结构一一对应、可选条件原生支持、多后端方言差异封装。

### 设计哲学

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

---

## 3) Workspace 架构

```
sql-query-builder/
├── Cargo.toml                          # workspace 根
├── graft-core/                         # 核心库：Param、Backend trait、QueryBuilder、AST 节点
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                      # 模块声明 + 公共 API 重导出
│       ├── param.rs                    # Param 枚举 + From impls
│       ├── types.rs                    # AST 节点：WhereGroup、JoinClause、CteNode 等
│       ├── builder.rs                  # QueryBuilder + 中间态构造器（WhereAdder、JoinAdder 等）
│       ├── backend.rs                  # Backend trait（方言抽象层）
│       ├── result.rs                   # QueryResult + BuildError
│       ├── exec.rs                     # Executor trait（feature-gated）
│       └── backends/
│           ├── mod.rs                  # feature-gated 模块声明
│           ├── postgres.rs             # PostgresBackend
│           ├── mysql.rs                # MysqlBackend
│           ├── mariadb.rs              # MariaDbBackend
│           ├── mssql.rs                # MssqlBackend
│           └── sqlite.rs               # SqliteBackend
├── graft-derive/                       # proc-macro crate：#[derive(InsertRow)]
│   ├── Cargo.toml
│   └── src/lib.rs
└── graft/                              # facade crate：re-export + 统一 feature flags
    ├── Cargo.toml
    └── src/lib.rs
```

### Feature flags

| Feature | 说明 | 默认 |
|---------|------|------|
| `postgresql` | Postgres 后端 | ✅ |
| `mysql` | MySQL 后端 | |
| `mariadb` | MariaDB 后端 | |
| `mssql` | MSSQL 后端 | |
| `sqlite` | SQLite 后端 | |
| `chrono` | 时间类型支持 | |
| `derive` | InsertRow 派生宏 | |
| `executor` | Executor trait（async） | |

---

## 4) Rust 编码规范

### 通用规范

- **包管理器**: `cargo`
- **代码格式**: `cargo fmt` + `cargo clippy`
- **edition**: 2024

### 命名约定
- 变量/函数: `snake_case`
- 类型/结构体/枚举: `PascalCase`
- 常量: `SCREAMING_SNAKE_CASE`
- 模块/文件: `snake_case`
- 文件/目录名（文件系统）: `kebab-case`
- Trait 名: 动词性（`Backend`、`Executor`、`HasWhere`、`HasJoins`）

### 模块组织

- **2018+ 版风格**：使用 `foo.rs` 作为模块根，子模块放在 `foo/` 目录下
- **扁平优先**：如果模块没有子模块，直接用单个 `foo.rs` 文件
- **需要子模块时才用目录**：只有当模块需要组织多个子模块时才创建 `foo/` 目录
- **避免 `mod.rs`**：不使用 `foo/mod.rs` 风格，统一使用 `foo.rs` 作为模块根
- **公共 API 重导出**：在模块根中使用 `pub use` 重新导出子模块的公共类型

### 错误处理

- 使用 `BuildError`（定义在 `result.rs`）作为公开 API 的错误类型
- 使用 `BuildResult<T>` 作为别名
- 避免 `unwrap()`（初始化 / 测试 / 确定不会失败的场景除外）
- 使用 `?` 传播错误
- 错误消息对用户友好、对开发者有调试信息

### 所有权与借用

- Builder 方法使用 **值传递（consume-and-return）**，避免 borrow checker 干扰
- 中间态构造器（`WhereAdder`、`JoinAdder`）持有目标的所有权，完成操作后返回
- `HasWhere` / `HasJoins` trait 通过 `&mut self` + `std::mem::take` 实现所有权转交
- 注意 edition 2024 的借用语义变化：匹配引用类型时不再使用 `ref` 关键字

### 测试

- 使用内联测试（`#[cfg(test)] mod tests {}`）
- 核心业务逻辑需测试（关键算法、边界条件、错误处理）
- 简单逻辑不需要测试（枚举字面值、Getter、无分支的简单转换）
- 不主动补测试（除非用户明确要求）

### 文档

- 所有公开 API 必须有文档注释（`///`）
- 顶层模块使用 `//!` 模块级文档

### 依赖管理

- 使用语义化版本
- 添加依赖时检查 feature 配置
- 依赖升级策略：
  - **安全补丁**：立即升级
  - **次要版本**：评估后升级
  - **主要版本**：谨慎升级
  - **验证**：升级后运行 `cargo build` + `cargo test`

---

## 5) 代码风格与实现约定

### Backend trait 设计

- **提供 Postgres 默认实现**，各后端 override 差异部分
- 方法签名尽量使用简单类型（`&str`、`usize`），避免在 Backend trait 中引入复杂 AST 类型
- `on_conflict()` 提供默认 Postgres 实现，MySQL/MSSQL/SQLite 各自 override
- 后端能力用 `supports_*()` 方法表达，而非 trait 层级分离

### QueryBuilder 设计

- 单一 `QueryBuilder` 结构体通过 `QueryMode` 枚举区分 SELECT/INSERT/UPDATE/DELETE
- 所有字段为 `pub(crate)`，不允许外部直接修改
- 链式调用方法使用 `self`（consume）-> `WhereAdder<T>`（中间态）-> `T`（返回）模式
- 可选条件：`eq_opt`、`set_opt`、`when` 是核心 API

### 参数安全

- **SQL 字符串里没有用户输入**——只有关键字、标识符、占位符
- 所有用户输入通过 `From trait` 转换为 `Param` 枚举
- `raw` 逃生舱也要求参数独立传递，不拼进 SQL
- `IN ()` 空子句 → `Err(BuildError::EmptyInClause)`
- UPDATE 无 SET → `Err(BuildError::NoSetClauses)`

### 参数连续性

- 子查询和主查询共享同一套参数空间
- 参数索引顺序 = SQL 中的出现顺序（CTE → 主查询 → 子查询，后序遍历）
- `param_offset: &mut usize` 作为可变引用在 build 递归中传递

---

## 6) Git Commit 规范

- 使用 Conventional Commits 格式：`<type>(<scope>): <description>`
- **描述使用中文**，祈使句、现在时态、句尾无句号
- 类型：
  - `feat` - 新功能
  - `fix` - Bug 修复
  - `enhance` - 增强现有功能
  - `docs` - 文档更新
  - `style` - 代码格式
  - `refactor` - 重构
  - `test` - 测试相关
  - `chore` - 构建/工具/配置
- **Scope 推导规则**：
  - 从被提交的文件路径中推导出一个最相关的 scope
  - 一个 commit 只写一个主要 scope，不要罗列多个
  - 根据 crate 名确定 scope：`core`、`derive`、`facade`（顶层 `graft`）
  - 跨 crate 改动用 `workspace` 或省略 scope
  - 示例：
    - `src/builder.rs` 改动 → `feat(core): 添加 JOIN 子查询支持`
    - `graft-derive/src/lib.rs` → `feat(derive): 添加 InsertRow 派生宏`
    - `graft/Cargo.toml` → `chore(facade): 添加 executor feature`

---

## 7) 当前状态

- **初始化阶段**：项目 scaffolding 已完成，需修复编译错误
- **下一阶段**：修复所有权借用问题、edition 2024 兼容性、通过 `cargo build`
- **设计文档**：`/Users/midnite/References/AI-Suggest/SQLQueryBuilder-Design-Memo.md`

---

**这些指南生效的标志：**
- diff 中不必要的改动更少
- 因过度复杂而导致的重写更少
- 澄清问题在实现之前提出
- 干净、精简的 PR
