use crate::backend::Backend;
use crate::param::Param;
use crate::types::ConflictAction;

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

    fn on_conflict(
        &self,
        _columns: &[String],
        action: &ConflictAction,
        set: &[(String, Param)],
        idx: &mut usize,
    ) -> String {
        match action {
            ConflictAction::DoNothing => "ON DUPLICATE KEY UPDATE id = id".to_string(),
            ConflictAction::DoUpdate { set_excluded, .. } => {
                let updates: Vec<String> = set_excluded
                    .iter()
                    .map(|c| format!("`{c}` = VALUES(`{c}`)"))
                    .collect();
                format!("ON DUPLICATE KEY UPDATE {}", updates.join(", "))
            }
            _ => String::new(),
        }
    }
}
