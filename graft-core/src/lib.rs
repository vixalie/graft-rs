//! # graft-core
//!
//! 多后端动态 SQL 查询构建器的核心类型和 Backend trait。
//! 所有用户输入通过 `Param` 枚举，杜绝 SQL 注入。

pub mod param;
pub mod types;
pub mod builder;
pub mod backend;
pub mod result;

#[cfg(feature = "executor")]
pub mod exec;

// 后端实现 —— 按 feature gate 编译
pub mod backends;

// ── 常用类型的 re-export ──

pub use param::Param;
pub use types::*;
pub use builder::{QueryBuilder, WhereAdder, GroupBuilder, JoinAdder, OnAdder, HasWhere, HasJoins};
pub use backend::Backend;
pub use result::{QueryResult, BuildError, BuildResult};

#[cfg(feature = "executor")]
pub use exec::Executor;
