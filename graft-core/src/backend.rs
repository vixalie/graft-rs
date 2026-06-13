use crate::param::Param;
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

    /// ON CONFLICT / UPSERT 子句。
    fn on_conflict(
        &self,
        columns: &[String],
        action: &ConflictAction,
        _set: &[(String, Param)],
        _idx: &mut usize,
    ) -> String {
        let cols = columns
            .iter()
            .map(|c| self.quote_ident(c))
            .collect::<Vec<_>>()
            .join(", ");
        match action {
            ConflictAction::DoNothing => format!("ON CONFLICT ({cols}) DO NOTHING"),
            ConflictAction::DoUpdate { set_excluded, .. } => {
                let updates: Vec<String> = set_excluded
                    .iter()
                    .map(|c| {
                        format!("{} = EXCLUDED.{}", self.quote_ident(c), self.quote_ident(c))
                    })
                    .collect();
                format!(
                    "ON CONFLICT ({cols}) DO UPDATE SET {}",
                    updates.join(", ")
                )
            }
        }
    }

    /// 构建 CTE 字符串。
    fn build_ctes(&self, ctes: &[CteNode], _idx: &mut usize) -> (String, Vec<Param>) {
        let mut sql = String::new();
        let params = vec![];

        if ctes.is_empty() {
            return (sql, params);
        }

        let recursive = ctes.iter().any(|c| c.recursive);
        if recursive {
            sql.push_str("WITH RECURSIVE ");
        } else {
            sql.push_str("WITH ");
        }

        for (i, cte) in ctes.iter().enumerate() {
            if i > 0 {
                sql.push_str(", ");
            }
            sql.push_str(&cte.name);
            if let Some(ref cols) = cte.columns {
                sql.push_str(&format!(" ({})", cols.join(", ")));
            }
            sql.push_str(" AS (");
            // Body building is complex; this is a placeholder
            // Real implementation would call build_select on the inner QueryBuilder
            sql.push_str("...");
            sql.push(')');
        }

        (sql, params)
    }
}
