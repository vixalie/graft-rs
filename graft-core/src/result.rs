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
    /// ORDER BY 或类似上下文中使用了不在白名单内的列名
    UnsafeColumn(String),
    /// UPDATE 没有 WHERE 条件（默认拒绝，除非 allow_unsafe_update）
    UnsafeUpdateWithoutWhere,
    /// DELETE 没有 WHERE 条件（默认拒绝，除非 allow_unsafe_delete）
    UnsafeDeleteWithoutWhere,
    /// OFFSET/FETCH 需要 ORDER BY（MSSQL 语法要求）
    OrderByRequired,
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildError::EmptyInClause => write!(f, "IN clause must not be empty"),
            BuildError::NoSetClauses => write!(f, "UPDATE requires at least one SET clause"),
            BuildError::UnsupportedJoinType(t) => {
                write!(f, "backend does not support JOIN type: {t}")
            }
            BuildError::UnsupportedFeature(feat) => write!(f, "backend does not support: {feat}"),
            BuildError::ModeMismatch(m) => {
                write!(f, "operation not valid for current query mode: {m}")
            }
            BuildError::UnsafeColumn(col) => write!(f, "column not in whitelist: {col}"),
            BuildError::UnsafeUpdateWithoutWhere => write!(
                f,
                "UPDATE without WHERE is not allowed; use allow_unsafe_update() to bypass"
            ),
            BuildError::UnsafeDeleteWithoutWhere => write!(
                f,
                "DELETE without WHERE is not allowed; use allow_unsafe_delete() to bypass"
            ),
            BuildError::OrderByRequired => write!(
                f,
                "OFFSET/FETCH requires ORDER BY; add .order_by(column, dir) before .limit()/.offset()"
            ),
        }
    }
}

impl std::error::Error for BuildError {}

pub type BuildResult<T> = Result<T, BuildError>;
