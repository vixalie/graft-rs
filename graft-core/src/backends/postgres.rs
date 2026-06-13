use crate::backend::Backend;

/// Postgres 后端 — 使用 `$1, $2` 占位符和双引号引用标识符。
/// 默认 `Backend` 实现就是 Postgres 风格，无需额外 override。
#[derive(Debug, Clone, Default)]
pub struct PostgresBackend;

impl Backend for PostgresBackend {
    // 全部使用 trait 默认实现：
    // - placeholder: $1, $2, ...
    // - quote_ident: "col"
    // - limit_offset: LIMIT x OFFSET y
    // - returning: RETURNING col1, col2
    // - supports_returning: true
    // - supports_bulk_returning: true
    // - on_conflict: ON CONFLICT (col) DO UPDATE SET ... = EXCLUDED....
}
