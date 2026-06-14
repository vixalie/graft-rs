use crate::param::Param;
use crate::result::BuildResult;
use crate::types::*;

/// 方言抽象层——每种数据库实现此 trait。
///
/// 提供默认实现（Postgres 风格），各后端 override 差异部分。
pub trait Backend {
    /// 参数占位符（1-indexed）。
    fn placeholder(&self, i: usize) -> String {
        format!("${i}")
    }

    /// 引用标识符。
    fn quote_ident(&self, name: &str) -> String {
        format!("\"{name}\"")
    }

    /// LIMIT / OFFSET 子句。
    fn limit_offset(&self, limit: Option<usize>, offset: Option<usize>) -> String {
        match (limit, offset) {
            (Some(l), Some(o)) => format!("LIMIT {l} OFFSET {o}"),
            (Some(l), None) => format!("LIMIT {l}"),
            (None, Some(o)) => format!("OFFSET {o}"),
            (None, None) => String::new(),
        }
    }

    /// RETURNING 子句。
    fn returning(&self, columns: &[String]) -> String {
        format!("RETURNING {}", columns.join(", "))
    }

    /// 支持 RETURNING 吗？
    fn supports_returning(&self) -> bool {
        true
    }

    /// 批量插入后支持多行 RETURNING 吗？
    fn supports_bulk_returning(&self) -> bool {
        true
    }

    /// 后端是否支持指定的 JOIN 类型。
    ///
    /// 默认全部支持。SQLite 3.35.0 之前不支持 RIGHT/FULL JOIN，
    /// 由 `SqliteBackend` override 拒绝。
    fn supports_join_type(&self, _jt: JoinType) -> bool {
        true
    }

    /// 后端是否支持 UPSERT（`ON CONFLICT` / `ON DUPLICATE KEY`）。
    ///
    /// MSSQL 不支持此语法（需改写为 `MERGE`），由 `MssqlBackend` override 拒绝。
    fn supports_upsert(&self) -> bool {
        true
    }

    /// ON CONFLICT / UPSERT 子句。
    ///
    /// 后端若不支持（如 MSSQL，需走 `MERGE`），返回
    /// `Err(BuildError::UnsupportedFeature(...))`，由构建器在 build 阶段向上传播。
    fn on_conflict(
        &self,
        columns: &[String],
        action: &ConflictAction,
        _set: &[(String, Param)],
        _idx: &mut usize,
    ) -> BuildResult<String> {
        let cols = columns
            .iter()
            .map(|c| self.quote_ident(c))
            .collect::<Vec<_>>()
            .join(", ");
        Ok(match action {
            ConflictAction::DoNothing => format!("ON CONFLICT ({cols}) DO NOTHING"),
            ConflictAction::DoUpdate { set_excluded, .. } => {
                let updates: Vec<String> = set_excluded
                    .iter()
                    .map(|c| format!("{} = EXCLUDED.{}", self.quote_ident(c), self.quote_ident(c)))
                    .collect();
                format!("ON CONFLICT ({cols}) DO UPDATE SET {}", updates.join(", "))
            }
        })
    }

    // 注：CTE 构建 (`WITH ... AS (...)`) 在所有后端一致，
    // 实际逻辑在 `QueryBuilder::build_ctes_inner` 中实现，
    // 不再作为 Backend trait 方法暴露——避免误导性 stub。
}
