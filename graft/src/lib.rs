//! # graft
//!
//! 多后端动态 SQL 查询构建器。
//!
//! ## 特性
//!
//! - **安全**：编译期强制参数化，杜绝 SQL 注入
//! - **动态**：`eq_opt` / `set_opt` / `when` 原生支持可选条件
//! - **多后端**：Backend trait 封装方言差异，build 时选择
//! - **可读**：Fluent API 与 SQL 逻辑结构一一对应
//!
//! ## Feature flags
//!
//! | Feature | 说明 | 默认 |
//! |---------|------|------|
//! | `postgresql` | Postgres 后端 | ✅ |
//! | `mysql` | MySQL 后端 | |
//! | `mariadb` | MariaDB 后端 | |
//! | `mssql` | MSSQL 后端 | |
//! | `sqlite` | SQLite 后端 | |
//! | `chrono` | 时间类型支持 (NaiveDateTime, DateTime<Utc>) | |
//! | `derive` | InsertRow 派生宏 | |
//!
//! ## 示例
//!
//! ```rust
//! use graft::*;
//!
//! let result = QueryBuilder::select(&["id", "name"])
//!     .from("users")
//!     .and_where("status").eq("active")
//!     .and_where("age").gte(18)
//!     .build(&backends::postgres::PostgresBackend)
//!     .unwrap();
//! let sql = result.sql;
//! let params = result.params;
//!
//! // SELECT "id", "name" FROM "users" WHERE "status" = $1 AND "age" >= $2
//! // params: ["active", 18]
//! ```

// ── 公共 API ──

#[doc(inline)]
pub use graft_core::*;

/// 后端实现。
pub mod backends {
    #[cfg(feature = "mariadb")]
    pub use graft_core::backends::mariadb;
    #[cfg(feature = "mssql")]
    pub use graft_core::backends::mssql;
    #[cfg(feature = "mysql")]
    pub use graft_core::backends::mysql;
    #[cfg(feature = "postgresql")]
    pub use graft_core::backends::postgres;
    #[cfg(feature = "sqlite")]
    pub use graft_core::backends::sqlite;

    // Re-export backend types at the backends module level
    #[cfg(feature = "mariadb")]
    pub use mariadb::MariaDbBackend;
    #[cfg(feature = "mssql")]
    pub use mssql::MssqlBackend;
    #[cfg(feature = "mysql")]
    pub use mysql::MysqlBackend;
    #[cfg(feature = "postgresql")]
    pub use postgres::PostgresBackend;
    #[cfg(feature = "sqlite")]
    pub use sqlite::SqliteBackend;
}

/// 派生宏。
#[cfg(feature = "derive")]
pub use graft_derive::*;

/// FromRow trait —— 将结构体转换为插入值。
pub trait FromRow {
    fn insert_values(&self) -> Vec<Param>;
}
