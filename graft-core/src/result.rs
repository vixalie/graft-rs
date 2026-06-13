use crate::param::Param;

/// build 的产出。
#[derive(Debug, Clone)]
pub struct QueryResult {
    /// 多语句支持（如 MySQL RETURNING 降级为两条语句）
    pub statements: Vec<(String, Vec<Param>)>,
    /// 单语句快捷字段
    pub sql: String,
    /// 参数列表
    pub params: Vec<Param>,
}

impl QueryResult {
    pub fn single(sql: String, params: Vec<Param>) -> Self {
        Self {
            statements: vec![(sql.clone(), params.clone())],
            sql,
            params,
        }
    }

    pub fn multi(statements: Vec<(String, Vec<Param>)>) -> Self {
        let sql = statements
            .iter()
            .map(|(s, _)| s.as_str())
            .collect::<Vec<_>>()
            .join(";\n");
        let params = statements
            .iter()
            .flat_map(|(_, p)| p.iter().cloned())
            .collect();
        Self {
            statements,
            sql,
            params,
        }
    }
}

// === 错误类型 ===

#[derive(Debug, Clone)]
pub enum BuildError {
    EmptyInClause,
    NoSetClauses,
    UnsupportedJoinType(String),
    UnsupportedFeature(String),
    ModeMismatch(String),
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildError::EmptyInClause => write!(f, "IN clause must not be empty"),
            BuildError::NoSetClauses => write!(f, "UPDATE requires at least one SET clause"),
            BuildError::UnsupportedJoinType(t) => write!(f, "backend does not support JOIN type: {t}"),
            BuildError::UnsupportedFeature(feat) => write!(f, "backend does not support: {feat}"),
            BuildError::ModeMismatch(m) => write!(f, "operation not valid for current query mode: {m}"),
        }
    }
}

impl std::error::Error for BuildError {}

pub type BuildResult<T> = Result<T, BuildError>;
