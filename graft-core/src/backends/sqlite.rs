use crate::backend::Backend;
use crate::param::Param;
use crate::result::BuildResult;
use crate::types::{ConflictAction, JoinType};

/// SQLite 后端 — 使用 `?` 占位符和双引号引用标识符。
/// 不支持 RETURNING（需要 `last_insert_rowid()` 降级）。
/// 3.35.0+ 支持 RIGHT/FULL JOIN，当前按不支持处理。
#[derive(Debug, Clone, Default)]
pub struct SqliteBackend;

impl Backend for SqliteBackend {
    fn placeholder(&self, _i: usize) -> String {
        "?".to_string()
    }

    fn quote_ident(&self, name: &str) -> String {
        format!("\"{name}\"")
    }

    fn supports_returning(&self) -> bool {
        // SQLite 3.35.0+ 支持 RETURNING，但为了兼容旧版本默认关闭
        false
    }

    fn supports_bulk_returning(&self) -> bool {
        false
    }

    /// SQLite 3.35.0 之前不支持 RIGHT / FULL JOIN，当前按不支持处理。
    fn supports_join_type(&self, jt: JoinType) -> bool {
        !matches!(jt, JoinType::Right | JoinType::Full)
    }

    fn on_conflict(
        &self,
        columns: &[String],
        action: &ConflictAction,
        _set: &[(String, Param)],
        _idx: &mut usize,
    ) -> BuildResult<String> {
        let cols = columns
            .iter()
            .map(|c| format!("\"{c}\""))
            .collect::<Vec<_>>()
            .join(", ");
        Ok(match action {
            ConflictAction::DoNothing => format!("ON CONFLICT ({cols}) DO NOTHING"),
            ConflictAction::DoUpdate { set_excluded, .. } => {
                let updates: Vec<String> = set_excluded
                    .iter()
                    .map(|c| format!("\"{c}\" = excluded.\"{c}\""))
                    .collect();
                format!("ON CONFLICT ({cols}) DO UPDATE SET {}", updates.join(", "))
            }
        })
    }
}
