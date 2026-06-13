//! # graft-core
//!
//! 多后端动态 SQL 查询构建器的核心类型和 Backend trait。
//! 所有用户输入通过 `Param` 枚举，杜绝 SQL 注入。

pub mod backend;
pub mod builder;
pub mod param;
pub mod result;
pub mod types;

#[cfg(feature = "executor")]
pub mod exec;

// 后端实现 —— 按 feature gate 编译
pub mod backends;

// ── 常用类型的 re-export ──

pub use backend::Backend;
pub use builder::{GroupBuilder, HasJoins, HasWhere, JoinAdder, OnAdder, QueryBuilder, WhereAdder};
pub use param::Param;
pub use result::{BuildError, BuildResult, QueryResult};
pub use types::*;

#[cfg(feature = "executor")]
pub use exec::Executor;
