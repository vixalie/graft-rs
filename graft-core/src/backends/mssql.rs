use crate::backend::Backend;
use crate::param::Param;
use crate::result::{BuildError, BuildResult};
use crate::types::ConflictAction;

/// MSSQL 后端 — 使用 `@P1, @P2` 占位符和方括号引用标识符。
/// LIMIT/OFFSET 使用 `OFFSET x ROWS FETCH NEXT y ROWS ONLY` 语法。
/// RETURNING 使用 `OUTPUT INSERTED.col` 语法。
#[derive(Debug, Clone, Default)]
pub struct MssqlBackend;

impl Backend for MssqlBackend {
    fn placeholder(&self, i: usize) -> String {
        format!("@P{i}")
    }

    fn quote_ident(&self, name: &str) -> String {
        // MSSQL 方括号引用，按点分割处理多段标识符
        name.split('.')
            .map(|part| format!("[{part}]"))
            .collect::<Vec<_>>()
            .join(".")
    }

    fn limit_offset(&self, limit: Option<usize>, offset: Option<usize>) -> String {
        match (limit, offset) {
            (Some(l), Some(o)) => format!("OFFSET {o} ROWS FETCH NEXT {l} ROWS ONLY"),
            (Some(l), None) => format!("OFFSET 0 ROWS FETCH NEXT {l} ROWS ONLY"),
            (None, Some(o)) => format!("OFFSET {o} ROWS"),
            (None, None) => String::new(),
        }
    }

    fn returning(&self, columns: &[String]) -> String {
        let cols = columns
            .iter()
            .map(|c| format!("INSERTED.{c}"))
            .collect::<Vec<_>>()
            .join(", ");
        format!("OUTPUT {cols}")
    }

    fn supports_returning(&self) -> bool {
        true
    }

    fn supports_bulk_returning(&self) -> bool {
        true
    }

    fn on_conflict(
        &self,
        _columns: &[String],
        _action: &ConflictAction,
        _set: &[(String, Param)],
        _idx: &mut usize,
    ) -> BuildResult<String> {
        // MSSQL 的 UPSERT 需改写整个 INSERT 为 `MERGE` 语句，
        // 复杂度较高，Phase 1 暂不支持。返回错误比生成非法 SQL 更安全。
        Err(BuildError::UnsupportedFeature(
            "MSSQL UPSERT (use MERGE explicitly — not yet implemented)".to_string(),
        ))
    }
}
