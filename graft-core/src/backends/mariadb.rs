use crate::backend::Backend;
use crate::param::Param;
use crate::result::BuildResult;
use crate::types::{ConflictAction, JoinType};

/// MariaDB 后端 — 与 MySQL 基本一致，需要时可独立调优。
#[derive(Debug, Clone, Default)]
pub struct MariaDbBackend;

impl Backend for MariaDbBackend {
    fn placeholder(&self, _i: usize) -> String {
        "?".to_string()
    }

    fn quote_ident(&self, name: &str) -> String {
        format!("`{name}`")
    }

    fn supports_returning(&self) -> bool {
        false
    }

    fn supports_bulk_returning(&self) -> bool {
        false
    }

    /// MariaDB 不支持 FULL OUTER JOIN。
    fn supports_join_type(&self, jt: JoinType) -> bool {
        !matches!(jt, JoinType::Full)
    }

    fn on_conflict(
        &self,
        _columns: &[String],
        action: &ConflictAction,
        _set: &[(String, Param)],
        _idx: &mut usize,
    ) -> BuildResult<String> {
        Ok(match action {
            ConflictAction::DoNothing => "ON DUPLICATE KEY UPDATE id = id".to_string(),
            ConflictAction::DoUpdate { set_excluded, .. } => {
                let updates: Vec<String> = set_excluded
                    .iter()
                    .map(|c| format!("`{c}` = VALUES(`{c}`)"))
                    .collect();
                format!("ON DUPLICATE KEY UPDATE {}", updates.join(", "))
            }
        })
    }
}
