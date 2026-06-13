use crate::param::Param;

// ============================================================
// WHERE 系统
// ============================================================

/// 逻辑运算符——相邻条件之间的连接词。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LogicOp {
    And,
    Or,
}

impl LogicOp {
    pub fn sql(&self) -> &'static str {
        match self {
            LogicOp::And => "AND",
            LogicOp::Or => "OR",
        }
    }
}

/// 比较运算符
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CmpOp {
    Eq,
    Ne,
    Gt,
    Gte,
    Lt,
    Lte,
    /// SQL `LIKE` 运算符——值仍由 `Expr::Value` 承载以保证参数化。
    Like,
}

impl CmpOp {
    pub fn sql(&self) -> &'static str {
        match self {
            CmpOp::Eq => "=",
            CmpOp::Ne => "<>",
            CmpOp::Gt => ">",
            CmpOp::Gte => ">=",
            CmpOp::Lt => "<",
            CmpOp::Lte => "<=",
            CmpOp::Like => "LIKE",
        }
    }
}

/// 表达式——列比较的右侧。
#[derive(Debug, Clone)]
pub enum Expr {
    Value(Param),
    Column(String),
    Subquery(Box<crate::builder::QueryBuilder>),
    RawExpr(String),
}

/// WHERE 条件种类。
#[derive(Debug, Clone)]
pub enum WhereKind {
    Column {
        column: String,
        op: CmpOp,
        value: Expr,
    },
    In {
        column: String,
        values: Vec<Vec<Expr>>,
        // values 外层 Vec 表示一组 OR 表达式：
        // vec![vec![Expr::Value(v1)], vec![Expr::Value(v2)]]
        // 展开为 (col = v1 OR col = v2)
        negated: bool,
    },
    Between {
        column: String,
        low: Expr,
        high: Expr,
    },
    IsNull {
        column: String,
        negated: bool,
    },
    Exists {
        subquery: Box<crate::builder::QueryBuilder>,
        negated: bool,
    },
    Group(Vec<WhereGroup>),
    Raw(String, Vec<Param>),
}

/// 一个 WHERE 条件节点。
#[derive(Debug, Clone)]
pub struct WhereGroup {
    pub logic: LogicOp,
    pub kind: WhereKind,
}

impl WhereGroup {
    pub fn new(logic: LogicOp, kind: WhereKind) -> Self {
        Self { logic, kind }
    }
}

// ============================================================
// JOIN 系统
// ============================================================

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum JoinType {
    Inner,
    Left,
    Right,
    Full,
    Cross,
}

impl JoinType {
    pub fn sql(&self) -> &'static str {
        match self {
            JoinType::Inner => "INNER JOIN",
            JoinType::Left => "LEFT JOIN",
            JoinType::Right => "RIGHT JOIN",
            JoinType::Full => "FULL OUTER JOIN",
            JoinType::Cross => "CROSS JOIN",
        }
    }
}

/// 表引用。
#[derive(Debug, Clone)]
pub enum TableRef {
    Table(String),
    TableAs(String, String),
    Subquery(Box<crate::builder::QueryBuilder>, String),
    CteRef(String, Option<String>),
}

impl TableRef {
    pub fn alias(&self) -> Option<&str> {
        match self {
            TableRef::TableAs(_, a) => Some(a.as_str()),
            TableRef::Subquery(_, a) => Some(a.as_str()),
            TableRef::CteRef(_, a) => a.as_deref(),
            _ => None,
        }
    }
}

impl From<&str> for TableRef {
    fn from(s: &str) -> Self {
        TableRef::Table(s.to_owned())
    }
}

/// ON 条件。
#[derive(Debug, Clone)]
pub enum OnCondition {
    Eq {
        left: String,
        right: String,
    },
    EqValue {
        column: String,
        op: CmpOp,
        value: Param,
    },
    Group {
        logic: LogicOp,
        conditions: Vec<OnCondition>,
    },
    Raw(String, Vec<Param>),
}

impl OnCondition {
    pub fn logic(&self) -> Option<LogicOp> {
        match self {
            OnCondition::Group { logic, .. } => Some(*logic),
            _ => None,
        }
    }
}

/// JOIN 子句。
#[derive(Debug, Clone)]
pub struct JoinClause {
    pub join_type: JoinType,
    pub table: TableRef,
    pub alias: Option<String>,
    pub conditions: Vec<OnCondition>,
}

impl JoinClause {
    pub fn new(join_type: JoinType, table: impl Into<TableRef>) -> Self {
        Self {
            join_type,
            table: table.into(),
            alias: None,
            conditions: vec![],
        }
    }

    pub fn alias_str(&self) -> String {
        self.alias
            .as_ref()
            .map(|a| format!(" AS {a}"))
            .unwrap_or_default()
    }
}

// ============================================================
// 排序
// ============================================================

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SortDir {
    Asc,
    Desc,
}

impl SortDir {
    pub fn sql(&self) -> &'static str {
        match self {
            SortDir::Asc => "ASC",
            SortDir::Desc => "DESC",
        }
    }
}

// ============================================================
// CTE
// ============================================================

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnionType {
    UnionAll,
    Union,
}

/// CTE 体。
#[derive(Debug, Clone)]
pub enum CteBody {
    Query(Box<crate::builder::QueryBuilder>),
    RecursiveUnion {
        anchor: Box<crate::builder::QueryBuilder>,
        recursive: Box<crate::builder::QueryBuilder>,
        union_type: UnionType,
    },
}

/// CTE 节点。
#[derive(Debug, Clone)]
pub struct CteNode {
    pub name: String,
    pub columns: Option<Vec<String>>,
    pub recursive: bool,
    pub body: CteBody,
}

impl CteNode {
    pub fn new(name: impl Into<String>, body: CteBody) -> Self {
        Self {
            name: name.into(),
            columns: None,
            recursive: false,
            body,
        }
    }
}

// ============================================================
// 冲突处理（UPSERT）
// ============================================================

#[derive(Debug, Clone)]
pub enum ConflictAction {
    DoNothing,
    DoUpdate {
        set: Vec<(String, Param)>,
        set_excluded: Vec<String>,
    },
}

#[derive(Debug, Clone)]
pub struct ConflictClause {
    pub columns: Vec<String>,
    pub action: ConflictAction,
}

impl ConflictClause {
    pub fn new(columns: Vec<String>, action: ConflictAction) -> Self {
        Self { columns, action }
    }
}

// ============================================================
// SET 子句（UPDATE）
// ============================================================

#[derive(Debug, Clone)]
pub enum SetValue {
    Param(Param),
    Subquery(Box<crate::builder::QueryBuilder>),
    Raw(String),
}

#[derive(Debug, Clone)]
pub struct SetClause {
    pub column: String,
    pub value: SetValue,
}

impl SetClause {
    pub fn new(column: impl Into<String>, value: SetValue) -> Self {
        Self {
            column: column.into(),
            value,
        }
    }
}
