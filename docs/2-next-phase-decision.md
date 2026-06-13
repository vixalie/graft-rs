# Phase 1 决策：安全修复与质量基线

> PM 分析日期：2026-06-14
> 基于：`docs/SQLQueryBuilder-Design-Memo.md`、`docs/roadmap.md`、当前编译状态

---

## 1. 当前状态快照

### 编译与质量

| 检查项 | 状态 | 详细 |
|--------|------|------|
| `cargo build` | ✅ 通过 | 3 个 dead_code warning |
| `cargo test` | ❌ 失败 | 1 个 doc-test 失败（tuple 解构 → QueryResult） |
| `cargo clippy` | ⚠️ 5 warnings | 3 dead_code + 2 collapsible_if |
| 单元测试 | ❌ 0% | 无任何测试 |
| 文档注释 | ⚠️ 部分缺失 | 部分公开方法缺少文档注释 |

### 遗留任务清单（来自 `1-pending-compile-fixes.md`）

编译错误已全部修复（`1-pending-compile-fixes.md` 中列出的 32 个错误和 9 个警告已清零）。
当前 `cargo build` 通过，剩余可优化项上升到 roadmap Phase 1 统一处理。

---

## 2. 现存问题优先级

对照 roadmap 的 Phase 1 定义，逐一验证当前状态：

| # | 问题 | 类型 | 严重程度 | 涉及文件 | 当前状态 |
|---|------|------|---------|---------|---------|
| 1 | LIKE 值手动转义，存在 SQL 注入风险 | 🔴 安全漏洞 | P0 | `builder.rs:1222-1233` | `like()` 方法将值放入 RawExpr 并手动 `replace('\'', "''")`，未参数化 |
| 2 | `in_()` 空列表不检查 | 🟡 行为缺陷 | P1 | `builder.rs:1252-1264` | 空列表直接生成 `IN ()`（非法 SQL） |
| 3 | `in_opt()` 空列表不检查 | 🟡 行为缺陷 | P1 | `builder.rs:1267-1272` | 同上 |
| 4 | `not_in()` 不存在 API | 🟡 API 缺失 | P1 | `WhereAdder` 上缺失 | Go 版有 `not_in`，Rust 有 `WhereKind::In { negated }` 但前端未暴露 |
| 5 | Backend trait 上无 `supports_join_type()` | 🟡 功能缺失 | P1 | `backend.rs` | 错误类型已定义但从不校验 |
| 6 | MSSQL UPSERT 返回空字符串 | 🟡 功能缺失 | P1 | `backends/mssql.rs:50-60` | `on_conflict()` 返回 `String::new()`，静默生成非法 SQL |
| 7 | `build_ctes()` 在 Backend trait 上是存根 | ⚪ 代码异味 | P2 | `backend.rs:74-105` | CTE 实际实现在 `builder.rs:build_ctes_inner()`，trait 方法输出 `"..."` |
| 8 | 3 个 `logic` 字段 dead_code | ⚪ 代码质量 | P2 | `builder.rs:1521,1566,1589` | `OnAdder<T>`、`OnGroupBuilder`、`OnAdderForGroup` 的 `logic` 字段从未读取 |
| 9 | doc-test 失败 | ⚪ 文档 | P2 | `graft/src/lib.rs:29` | 用 tuple 解构 `let (sql, params)`，但 `build()` 返回 `QueryResult` |
| 10 | 2 个 collapsible_if clippy 警告 | ⚪ 代码质量 | P2 | `builder.rs:717,825` | 嵌套 `if let` 可合并 |
| 11 | `like_opt` 也有注入风险 | 🔴 安全漏洞 | P0 | `builder.rs:1244-1248` | 因为 `like_opt` 调用 `like()`，继承注入漏洞 |
| 12 | 0 测试覆盖率 | 🟡 质量风险 | P3 | — | 无任何单元/集成测试 |

### 关键发现

**LIKE 注入是真实的安全漏洞**。当前代码：
```rust
pub fn like(self, val: impl Into<Param>) -> T {
    // ...
    value: Expr::RawExpr(format!("LIKE {}", match val.into() {
        Param::Text(s) => format!("'{}'", s.replace('\'', "''")),
        // ...
    })),
};
```
- 只转义了 `'`，未处理 `%`、`_`、`\` 等 LIKE 通配符
- 即使只转义 `'`，也存在 Unicode/编码绕过风险
- 应改为参数化：`LIKE ?` / `LIKE $1`

---

## 3. 决策：Phase 1 即刻启动

### 推荐范围

**明确执行** roadmap Phase 1（安全修复 + 质量基线），包含以下 8 项工作：

### 3.1 LIKE 参数化修复（P0 安全漏洞）

**目标**：消除 LIKE 手动转义，改用占位符

| 当前行为 | 目标行为 |
|---------|---------|
| `LIKE 'alice%'`（值拼入 SQL） | `LIKE $1`（参数化，`$1` = `"alice%"`） |

**改动范围**：
- `builder.rs`：修改 `WhereAdder::like()`，移除 `RawExpr` 包装，改用 `Expr::Value` + new `Like` CmpOp 或扩展 `WhereKind`
- 建议：新增 `CmpOp::Like` 变体或新增 `WhereKind::Like` 变体

**影响面**：仅 `like()` 和 `like_opt()` 两个方法。行为一致，SQL 输出从 `LIKE 'val'` 变为 `LIKE ?`。

**置信度**: [高: 代码已定位，修复路径明确]

### 3.2 EmptyInClause 校验（P1 行为缺陷）

**目标**：`in_()` 和 `in_opt()` 在 values 为空时返回 `Err(EmptyInClause)` 而不是生成非法 SQL

**改动范围**：
- `builder.rs`：在 `WhereAdder::in_()` 中添加空值检查
- 需要将 `in_()` 的返回类型从 `T` 改为 `BuildResult<T>`
  - ⚠️ **设计决策**：这会破坏链式调用的流畅性
  - **方案 A**：`in_()` 返回 `BuildResult<T>`，调用方用 `?`
  - **方案 B**：在 `build()` 阶段统一校验，保持 fluent API 不变
  - **推荐方案 B**：所有校验集中在 `build()`，`in_()` 保持返回 `T`

**推荐方案 B** 的理由：与 Go 版设计一致（构建阶段不报错，build 时集中校验），维持链式 API 流畅性。

**置信度**: [中: 设计决策需确认，推荐 build 时校验方案]

### 3.3 UnsupportedJoinType 校验（P1 功能缺失）

**目标**：添加 `Backend::supports_join_type()` 方法，在 build 时校验

**改动**：
- `backend.rs`：添加 `fn supports_join_type(&self, jt: JoinType) -> bool` 默认方法
- `backends/sqlite.rs`：override 返回 `!matches!(jt, Right | Full)`（SQLite 3.35.0+ 才支持）
- `builder.rs`：在 `build_joins()` 中调用校验，不支持时返回 `Err(UnsupportedJoinType)`

**置信度**: [高: Go 版有完全对等的实现]

### 3.4 MSSQL UPSERT 返回错误 / 报错（P1 功能缺失）

**目标**：让 `MssqlBackend::on_conflict()` 不再静默返回空字符串

**改动**：
- `backends/mssql.rs`：当前返回 `String::new()`，改为触发 `Err(BuildError::UnsupportedFeature)`
- 完整 MERGE 实现推迟到后续版本（见 roadmap ADR-004）

**方案**：将 `on_conflict()` 返回类型改为 `Result<String, BuildError>`（或保持 `String` 但让 builder 检测空串后报错）
- 推荐**将 `on_conflict()` 改为返回 `BuildResult<String>`**

**置信度**: [高: 当前空串是明显 bug]

### 3.5 build_ctes() 存根修复（P2 代码异味）

**目标**：消除 `Backend::build_ctes()` 存根

**方案 A（推荐）**：从 `Backend` trait 中移除 `build_ctes()` 方法
- CTE 的 SQL 结构在所有后端一致（`WITH name AS (...)`）
- 实际逻辑已在 `builder.rs:build_ctes_inner()` 中完整实现
- 移除后避免误导（当前存根输出 `"..."`，与真实实现不一致）

**方案 B**：将 `build_ctes_inner()` 逻辑迁移到 `build_ctes()` trait 方法
- 增加每个 backend 的重复代码
- 需要将 `CteBody::Query/RecursiveUnion` 等类型暴露给 Backend trait

**推荐方案 A** — 移除 trait 方法，全在 Builder 内部处理。

**置信度**: [中: 需确认 MSSQL CTE 语法是否完全无差异。Go 版 CTE 也在 builder 中构建]

### 3.6 dead_code warning 修复（P2 代码质量）

**目标**：消除 3 个 `logic` 字段未读取的警告

| 位置 | 字段未读取原因 | 修复方式 |
|------|--------------|---------|
| `OnAdder<T>.logic` | `OnAdder` 方法（`eq`, `ne` 等）未读取 `logic` 字段 | 在 `eq/ne/gt/lt/gte/lte` 方法中写入 `OnCondition::EqValue` 时使用 `logic` |
| `OnGroupBuilder.logic` | 未在组展开时传递 | 在 build 时使用 logic 确定组内连接词 |
| `OnAdderForGroup.logic` | 同 OnAdder | 同上 |

**置信度**: [高: 字段已定义，只需在对应构建路径中消费]

### 3.7 Doc-test 修复（P2 文档）

**改动**：
- `graft/src/lib.rs:29`：将 `let (sql, params) = ...` 改为 `let result = ...; let sql = result.sql; let params = result.params;`

**置信度**: [高: 一行改动]

### 3.8 Collapsible_if clippy 警告修复（P2 代码质量）

**改动**：
- `builder.rs:717-722`：合并 INSERT RETURNING 的嵌套 `if let`
- `builder.rs:825-829`：合并 DELETE RETURNING 的嵌套 `if let`

**置信度**: [高: clippy 已给出具体修复建议]

---

## 4. 考虑过的替代方案

### 4.1 从 Phase 2 提前引入 *Opt 变体

**提议**：在 Phase 1 中顺便添加 `ne_opt`、`gt_opt`、`gte_opt`、`lt_opt`、`lte_opt` 到 `WhereAdder`

**赞成理由**：
- 纯机械的 pattern match（与 `eq_opt` 完全相同）
- 每方法 5 行，约 5 分钟的工作量
- 让 API 更完整

**反对理由**：
- Scope creep：Phase 1 已经有 8 项工作
- 这些是功能增强而非安全/质量修复
- 可以在 Phase 2 系统性地完成（包括子查询比较器等）

**决策**：❌ **推迟到 Phase 2**。Phase 1 保持聚焦安全+质量。如果 Phase 1 实施后有余力，可以作为"顺手做"的额外项。

### 4.2 提前开始测试编写

**提议**：在修复过程中同步为被改动的代码编写测试

**赞成理由**：
- 确保修复的正确性
- 防止回归

**反对理由**：
- Phase 6 专门留给测试
- 当前代码变动中编写测试会增加 Phase 1 工作量 2-3 倍

**决策**：✅ **采纳但限定范围**。只为 LIKE 参数化修复编写 1-2 个针对性测试（验证注入被消除），其他修复的测试推迟到 Phase 6。

---

## 5. 排除范围（YAGNI 检查）

以下事项**不纳入** Phase 1：

| 事项 | 排除理由 | 归属 |
|------|---------|------|
| 完整 MSSQL MERGE 实现 | 高复杂度，非阻塞 | 后续版本 |
| `not_in()` API 暴露 | 功能增强，`negated` 字段已可用 | Phase 2 |
| `SelectIdent` 智能引用 | 功能增强 | Phase 3 |
| `OrderBySafe` | 功能增强 | Phase 3 |
| 无 WHERE UPDATE/DELETE 校验 | 安全策略但非注入级 | Phase 4 |
| `*Opt` 变体补齐 | 功能增强 | Phase 2 |
| 子查询比较运算符 | 功能增强 + 需新 WhereKind | Phase 2 |
| 全量测试 | 应基于稳定 API 编写 | Phase 6 |

---

## 6. 实施建议

### 推荐执行顺序

```
Wave 1（安全优先）
  1. LIKE 参数化修复（P0）—— 最紧急，涉及单文件 builder.rs
  2. LIKE 修复验证测试 —— 确保无注入
  
Wave 2（行为修复）
  3. EmptyInClause build 时校验（P1）
  4. MSSQL UPSERT 改为返回错误（P1）
  5. UnsupportedJoinType 校验（P1）—— 涉及 Backend trait 变更

Wave 3（质量清理）
  6. build_ctes() 从 trait 移除（P2）
  7. 3 个 dead_code warning 修复（P2）
  8. collapsible_if 修复（P2）

Wave 4（收尾）
  9. Doc-test 修复（P2）
  10. cargo build —all-features 验证
  11. cargo clippy — -D warnings 验证
  12. cargo test 验证
```

### 验证标准

```bash
cargo build                    # 零 warning
cargo build --all-features     # 全特性编译
cargo clippy -- -D warnings    # 零 clippy 问题
cargo test                     # 全通过（包含 doc-test）
cargo fmt -- --check           # 格式合规
```

### 影响文件清单

| 文件 | 改动 |
|------|------|
| `graft-core/src/builder.rs` | LIKE 修复、EmptyInClause 校验、dead_code 修复、collapsible_if、Join 校验 |
| `graft-core/src/backend.rs` | 添加 `supports_join_type()`、移除 `build_ctes()` |
| `graft-core/src/backends/mssql.rs` | `on_conflict()` 返回错误而非空串 |
| `graft-core/src/backends/sqlite.rs` | override `supports_join_type()` |
| `graft-core/src/backends/postgres.rs` | 无变动（使用默认实现） |
| `graft-core/src/backends/mysql.rs` | 无变动 |
| `graft-core/src/backends/mariadb.rs` | 无变动 |
| `graft-core/src/types.rs` | 可能：新增 `CmpOp::Like` 或 `WhereKind::Like` |
| `graft/src/lib.rs` | doc-test 解构方式修复 |

---

## 7. 风险与缓解

| 风险 | 可能性 | 缓解 |
|------|-------|------|
| LIKE 参数化后通配符语义不兼容 | 低 | 参数化后值包含 `%` 仍然是 LIKE 的合法通配符，行为与手动转义一致 |
| EmptyInClause 校验影响现有调用方 | 低 | 当前无外部使用者，保持 `BuildResult` 返回即可 |
| build_ctes() 移除后 trait 设计不一致 | 中 | 替代方案：保留方法但删除默认实现，让每个 backend 必须有实现 |
| MSSQL UPSERT 空串改错误后破坏构建 | 中 | 这是正确的行为，错误比非法 SQL 更好 |

---

## 8. 总结

**启动 Phase 1：安全修复与质量基线**。

这是 roadmap 的既定起点，也是最紧迫的阶段——当前项目中存在 LIKE 注入的安全漏洞（P0），以及多个行为缺陷（P1），应优先解决。

Phase 1 完成后，项目将满足：
- ✅ 零编译 warning（`cargo build` 干净通过）
- ✅ 零 clippy 问题
- ✅ 所有测试通过
- ✅ LIKE 值参数化，无注入风险
- ✅ 非法 SQL 路径被阻断（空 IN、不支持 JOIN、MSSQL UPSERT 报错）
- ✅ CTE 实现与 trait 定义对齐

预计 6-8 项原子改动，按 Wave 1→4 顺序执行，可并行推进。
