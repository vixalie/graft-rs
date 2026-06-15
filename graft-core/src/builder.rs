use crate::Backend;
use crate::param::Param;
use crate::result::{BuildError, BuildResult, QueryResult};
use crate::types::*;

// ============================================================
// QueryBuilder — 核心查询构建器
// ============================================================

/// 查询模式。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum QueryMode {
    Select,
    Insert,
    Update,
    Delete,
}

/// 动态 SQL 查询构建器。
///
/// 内部以 AST-like 结构存储查询意图，`build()` 时由 `Backend` trait
/// 生成最终 SQL。所有用户输入通过 `Param` 枚举，绝不拼入 SQL 字符串。
#[derive(Debug, Clone)]
pub struct QueryBuilder {
    // ── 模式 ──
    pub(crate) mode: QueryMode,

    // ── SELECT ──
    pub(crate) columns: Vec<SelectExpr>,
    pub(crate) from: Vec<TableRef>,
    pub(crate) joins: Vec<JoinClause>,
    pub(crate) where_list: Vec<WhereGroup>,
    pub(crate) group_by: Vec<String>,
    pub(crate) group_by_ident: bool,
    pub(crate) having: Vec<WhereGroup>,
    pub(crate) order_by: Vec<(String, SortDir)>,
    pub(crate) limit: Option<usize>,
    pub(crate) offset: Option<usize>,

    // ── INSERT ──
    pub(crate) insert_table: Option<String>,
    pub(crate) insert_columns: Vec<String>,
    pub(crate) insert_values: Vec<Vec<Param>>,
    pub(crate) insert_from_select: Option<Box<QueryBuilder>>,
    pub(crate) insert_returning: Option<Vec<String>>,
    pub(crate) insert_conflict: Option<ConflictClause>,

    // ── UPDATE ──
    pub(crate) update_table: Option<String>,
    pub(crate) set_list: Vec<SetClause>,

    // ── DELETE ──
    pub(crate) delete_table: Option<String>,
    pub(crate) delete_returning: Option<Vec<String>>,

    // ── CTE ──
    pub(crate) ctes: Vec<CteNode>,

    // ── 安全策略 ──
    pub(crate) allow_unsafe_update: bool,
    pub(crate) allow_unsafe_delete: bool,
}

/// SELECT 列表达式 —— 支持 `"col"` 或 `"expr AS alias"`。
#[derive(Debug, Clone)]
pub enum SelectExpr {
    Column(String),
    /// 智能引用列名——简单标识符加引号，复杂表达式（含 `.`、`()` 等）不加引号。
    Ident(String),
    Subquery(Box<QueryBuilder>, String), // subquery, alias
    Raw(String),
}

// ═══════════════════════════════════════════
// QueryBuilder impl
// ═══════════════════════════════════════════

impl QueryBuilder {
    // ═══════════════════════════════════════════
    // 构造器
    // ═══════════════════════════════════════════

    fn new(mode: QueryMode) -> Self {
        Self {
            mode,
            columns: vec![],
            from: vec![],
            joins: vec![],
            where_list: vec![],
            group_by: vec![],
            group_by_ident: false,
            having: vec![],
            order_by: vec![],
            limit: None,
            offset: None,
            insert_table: None,
            insert_columns: vec![],
            insert_values: vec![],
            insert_from_select: None,
            insert_returning: None,
            insert_conflict: None,
            update_table: None,
            set_list: vec![],
            delete_table: None,
            delete_returning: None,
            ctes: vec![],
            allow_unsafe_update: false,
            allow_unsafe_delete: false,
        }
    }

    /// 创建 SELECT 查询。
    ///
    /// ```rust
    /// use graft_core::QueryBuilder;
    /// let qb = QueryBuilder::select(&["id", "name"]).from("users");
    /// # let _ = qb;
    /// ```
    pub fn select(columns: &[&str]) -> Self {
        let mut b = Self::new(QueryMode::Select);
        b.columns = columns
            .iter()
            .map(|c| SelectExpr::Column(c.to_string()))
            .collect();
        b
    }

    /// 创建 SELECT raw 表达式（`SELECT 1`）。
    pub fn select_raw(expr: &str) -> Self {
        let mut b = Self::new(QueryMode::Select);
        b.columns.push(SelectExpr::Raw(expr.to_string()));
        b
    }

    /// 智能列名 SELECT。
    ///
    /// 简单标识符（仅字母数字下划线）将被后端引用，
    /// 含 `.`、`()`、空格等的表达式则不加引号。
    ///
    /// **注意**：带 `AS` 别名的列名作为整体处理，不会分别引用两侧。
    /// 例如 `select_ident(&["name AS user_name"])` 因含空格整体原样输出。
    /// 如需分别引用，请用 `select(&["\"name\" AS \"user_name\""])` 自行处理。
    ///
    /// ```rust
    /// use graft_core::QueryBuilder;
    /// let qb = QueryBuilder::default()
    ///     .select_ident(&["users.name", "age", "UPPER(email) AS email_upper"])
    ///     .from("users");
    /// # let _ = qb;
    /// ```
    pub fn select_ident(mut self, columns: &[&str]) -> Self {
        self.columns = columns
            .iter()
            .map(|c| SelectExpr::Ident(c.to_string()))
            .collect();
        self
    }

    /// 创建 INSERT 查询。
    pub fn insert_into(table: &str) -> Self {
        let mut b = Self::new(QueryMode::Insert);
        b.insert_table = Some(table.to_string());
        b
    }

    /// 创建 UPDATE 查询。
    pub fn update(table: &str) -> Self {
        let mut b = Self::new(QueryMode::Update);
        b.update_table = Some(table.to_string());
        b
    }

    /// 创建 DELETE 查询。
    pub fn delete_from(table: &str) -> Self {
        let mut b = Self::new(QueryMode::Delete);
        b.delete_table = Some(table.to_string());
        b
    }

    // ═══════════════════════════════════════════
    // FROM / SELECT 子句
    // ═══════════════════════════════════════════

    /// 设置 FROM 子句。
    pub fn from(mut self, table: &str) -> Self {
        self.from.push(TableRef::Table(table.to_string()));
        self
    }

    /// 带别名的 FROM。
    /// `from("users", "u")` → `FROM users AS u`
    pub fn from_as(mut self, table: &str, alias: &str) -> Self {
        self.from
            .push(TableRef::TableAs(table.to_string(), alias.to_string()));
        self
    }

    /// 派生表：`FROM (SELECT ...) AS alias`
    pub fn from_subquery(mut self, subquery: QueryBuilder, alias: &str) -> Self {
        self.from
            .push(TableRef::Subquery(Box::new(subquery), alias.to_string()));
        self
    }

    /// CTE 引用：`FROM cte_name`
    pub fn from_cte_ref(mut self, cte: &str) -> Self {
        self.from.push(TableRef::CteRef(cte.to_string(), None));
        self
    }

    /// CTE 引用带别名。
    pub fn from_cte_ref_as(mut self, cte: &str, alias: &str) -> Self {
        self.from
            .push(TableRef::CteRef(cte.to_string(), Some(alias.to_string())));
        self
    }

    /// 添加列选择（标量子查询）。
    pub fn select_subquery(mut self, subquery: QueryBuilder, alias: &str) -> Self {
        self.columns
            .push(SelectExpr::Subquery(Box::new(subquery), alias.to_string()));
        self
    }

    // ═══════════════════════════════════════════
    // WHERE
    // ═══════════════════════════════════════════

    /// 添加 AND WHERE 条件。
    pub fn and_where(self, column: &str) -> WhereAdder<Self> {
        let column = column.to_string();
        WhereAdder {
            target: self,
            column,
            logic: LogicOp::And,
        }
    }

    /// 添加 OR WHERE 条件。
    pub fn or_where(self, column: &str) -> WhereAdder<Self> {
        let column = column.to_string();
        WhereAdder {
            target: self,
            column,
            logic: LogicOp::Or,
        }
    }

    /// 添加 AND 函数表达式 WHERE 条件（如 `UPPER(name)`）。
    /// build 时表达式不被引号包裹（与 ADR-002 一致）。
    pub fn and_where_expr(self, expr: &str) -> WhereAdder<Self> {
        self.and_where(expr)
    }

    /// 添加 OR 函数表达式 WHERE 条件。
    pub fn or_where_expr(self, expr: &str) -> WhereAdder<Self> {
        self.or_where(expr)
    }

    /// 添加 AND EXISTS 子查询。
    pub fn and_exists(mut self, subquery: QueryBuilder) -> Self {
        self.where_list.push(WhereGroup::new(
            LogicOp::And,
            WhereKind::Exists {
                subquery: Box::new(subquery),
                negated: false,
            },
        ));
        self
    }

    /// 添加 NOT EXISTS 子查询。
    pub fn and_not_exists(mut self, subquery: QueryBuilder) -> Self {
        self.where_list.push(WhereGroup::new(
            LogicOp::And,
            WhereKind::Exists {
                subquery: Box::new(subquery),
                negated: true,
            },
        ));
        self
    }

    /// 添加 AND 条件分组 `AND (...)`。
    pub fn and_group(mut self, f: impl FnOnce(GroupBuilder) -> GroupBuilder) -> Self {
        let group = GroupBuilder::new(LogicOp::And);
        let group = f(group);
        self.where_list.push(WhereGroup::new(
            LogicOp::And,
            WhereKind::Group(group.groups),
        ));
        self
    }

    /// 添加 OR 条件分组 `OR (...)`。
    pub fn or_group(mut self, f: impl FnOnce(GroupBuilder) -> GroupBuilder) -> Self {
        let group = GroupBuilder::new(LogicOp::Or);
        let group = f(group);
        self.where_list
            .push(WhereGroup::new(LogicOp::Or, WhereKind::Group(group.groups)));
        self
    }

    /// 条件守卫——仅在 `cond` 为 true 时执行闭包。
    pub fn when(self, cond: bool, f: impl FnOnce(Self) -> Self) -> Self {
        if cond { f(self) } else { self }
    }

    // ═══════════════════════════════════════════
    // JOIN
    // ═══════════════════════════════════════════

    /// INNER JOIN table AS alias。
    pub fn join(mut self, table: &str, alias: &str) -> JoinAdder<Self> {
        self.joins.push(JoinClause {
            join_type: JoinType::Inner,
            table: TableRef::Table(table.to_string()),
            alias: Some(alias.to_string()),
            conditions: vec![],
        });
        let join_idx = self.joins.len() - 1;
        JoinAdder {
            target: self,
            join_idx,
        }
    }

    /// LEFT JOIN table AS alias。
    pub fn left_join(mut self, table: &str, alias: &str) -> JoinAdder<Self> {
        self.joins.push(JoinClause {
            join_type: JoinType::Left,
            table: TableRef::Table(table.to_string()),
            alias: Some(alias.to_string()),
            conditions: vec![],
        });
        let join_idx = self.joins.len() - 1;
        JoinAdder {
            target: self,
            join_idx,
        }
    }

    /// RIGHT JOIN table AS alias。
    pub fn right_join(mut self, table: &str, alias: &str) -> JoinAdder<Self> {
        self.joins.push(JoinClause {
            join_type: JoinType::Right,
            table: TableRef::Table(table.to_string()),
            alias: Some(alias.to_string()),
            conditions: vec![],
        });
        let join_idx = self.joins.len() - 1;
        JoinAdder {
            target: self,
            join_idx,
        }
    }

    /// FULL OUTER JOIN table AS alias。
    pub fn full_join(mut self, table: &str, alias: &str) -> JoinAdder<Self> {
        self.joins.push(JoinClause {
            join_type: JoinType::Full,
            table: TableRef::Table(table.to_string()),
            alias: Some(alias.to_string()),
            conditions: vec![],
        });
        let join_idx = self.joins.len() - 1;
        JoinAdder {
            target: self,
            join_idx,
        }
    }

    /// CROSS JOIN (无 ON 条件)。
    pub fn cross_join(mut self, table: &str) -> Self {
        self.joins.push(JoinClause {
            join_type: JoinType::Cross,
            table: TableRef::Table(table.to_string()),
            alias: None,
            conditions: vec![],
        });
        self
    }

    /// 子查询 JOIN：`INNER JOIN (SELECT ...) AS alias`
    pub fn join_subquery(mut self, subquery: QueryBuilder, alias: &str) -> JoinAdder<Self> {
        self.joins.push(JoinClause {
            join_type: JoinType::Inner,
            table: TableRef::Subquery(Box::new(subquery), alias.to_string()),
            alias: Some(alias.to_string()),
            conditions: vec![],
        });
        let join_idx = self.joins.len() - 1;
        JoinAdder {
            target: self,
            join_idx,
        }
    }

    /// CTE JOIN：`INNER JOIN cte_name AS alias`
    pub fn join_cte(mut self, cte: &str, alias: &str) -> JoinAdder<Self> {
        self.joins.push(JoinClause {
            join_type: JoinType::Inner,
            table: TableRef::CteRef(cte.to_string(), Some(alias.to_string())),
            alias: Some(alias.to_string()),
            conditions: vec![],
        });
        let join_idx = self.joins.len() - 1;
        JoinAdder {
            target: self,
            join_idx,
        }
    }

    // ═══════════════════════════════════════════
    // GROUP BY / HAVING / ORDER BY / LIMIT
    // ═══════════════════════════════════════════

    pub fn group_by(mut self, columns: &[&str]) -> Self {
        self.group_by = columns.iter().map(|c| c.to_string()).collect();
        self.group_by_ident = false; // 重置标志，确保行为由最后一次调用决定
        self
    }

    /// 智能 GROUP BY。
    ///
    /// 对列名做智能引用（与 `select_ident` 规则一致）。
    pub fn group_by_ident(mut self, columns: &[&str]) -> Self {
        self.group_by = columns.iter().map(|c| c.to_string()).collect();
        self.group_by_ident = true;
        self
    }

    pub fn having(self, _column: &str) -> WhereAdder<Self> {
        // simplified: append having with AND logic
        self.and_where(_column)
    }

    pub fn order_by(mut self, column: &str, dir: SortDir) -> Self {
        self.order_by.push((column.to_string(), dir));
        self
    }

    /// 白名单校验的 ORDER BY。
    ///
    /// 仅当 `column` 在 `whitelist` 中时才允许排序。
    /// 否则返回 `Err(BuildError::UnsafeColumn)`。
    ///
    /// # 用法
    ///
    /// **在返回 `Result` 的函数中**（推荐，使用 `?` 运算符）：
    ///
    /// ```rust
    /// # use graft_core::{QueryBuilder, SortDir, BuildResult, QueryResult};
    /// # fn example() -> BuildResult<QueryResult> {
    /// let result = QueryBuilder::select(&["id"]).from("users")
    ///     .and_where("status").eq("active")
    ///     .order_by_safe("name", SortDir::Asc, &["name", "id", "email"])?
    ///     .build(&graft_core::backends::postgres::PostgresBackend)?;
    /// # Ok(result)
    /// # }
    /// ```
    ///
    /// **在非 Result 上下文中**（使用 `.unwrap()`，信任白名单）：
    ///
    /// ```rust
    /// use graft_core::{QueryBuilder, SortDir};
    /// let qb = QueryBuilder::select(&["id"]).from("users")
    ///     .order_by_safe("name", SortDir::Asc, &["name", "id", "email"])
    ///     .unwrap();
    /// # let _ = qb;
    /// ```
    ///
    /// **与 `when()` 守卫配合**（用 `if let` 解包）：
    ///
    /// ```rust
    /// # use graft_core::{QueryBuilder, SortDir, BuildResult, QueryResult};
    /// # fn example(sort_col: Option<&str>) -> BuildResult<QueryResult> {
    /// let qb = QueryBuilder::select(&["id"]).from("users");
    /// let qb = if let Some(col) = sort_col {
    ///     qb.order_by_safe(col, SortDir::Asc, &["name", "id"])?
    /// } else {
    ///     qb
    /// };
    /// let result = qb.build(&graft_core::backends::postgres::PostgresBackend)?;
    /// # Ok(result)
    /// # }
    /// ```
    pub fn order_by_safe(
        self,
        column: &str,
        dir: SortDir,
        whitelist: &[&str],
    ) -> BuildResult<Self> {
        if !whitelist.contains(&column) {
            return Err(BuildError::UnsafeColumn(column.to_string()));
        }
        Ok(self.order_by(column, dir))
    }

    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    pub fn offset(mut self, offset: usize) -> Self {
        self.offset = Some(offset);
        self
    }

    // ═══════════════════════════════════════════
    // INSERT
    // ═══════════════════════════════════════════

    /// INSERT 设置列值。
    pub fn set(mut self, column: &str, value: impl Into<Param>) -> Self {
        self.insert_columns.push(column.to_string());
        self.insert_values.push(vec![value.into()]);
        self
    }

    /// INSERT 设置列名（配合 `rows()` 批量插入）。
    pub fn columns(mut self, cols: &[&str]) -> Self {
        self.insert_columns = cols.iter().map(|c| c.to_string()).collect();
        self
    }

    /// INSERT 批量插入。
    pub fn rows(&mut self) -> RowCollector<'_> {
        RowCollector { builder: self }
    }

    /// INSERT FROM SELECT。
    pub fn from_select(mut self, subquery: QueryBuilder) -> Self {
        self.insert_from_select = Some(Box::new(subquery));
        self
    }

    /// INSERT RETURNING。
    pub fn returning(mut self, columns: &[&str]) -> Self {
        self.insert_returning = Some(columns.iter().map(|c| c.to_string()).collect());
        self
    }

    /// DELETE RETURNING。
    pub fn delete_returning(mut self, columns: &[&str]) -> Self {
        self.delete_returning = Some(columns.iter().map(|c| c.to_string()).collect());
        self
    }

    /// UPSERT ON CONFLICT DO NOTHING。
    pub fn on_conflict_do_nothing(mut self, columns: &[&str]) -> Self {
        self.insert_conflict = Some(ConflictClause::new(
            columns.iter().map(|c| c.to_string()).collect(),
            ConflictAction::DoNothing,
        ));
        self
    }

    /// UPSERT ON CONFLICT DO UPDATE。
    pub fn on_conflict_do_update(
        mut self,
        columns: &[&str],
        set: Vec<(&str, impl Into<Param>)>,
        set_excluded: &[&str],
    ) -> Self {
        self.insert_conflict = Some(ConflictClause::new(
            columns.iter().map(|c| c.to_string()).collect(),
            ConflictAction::DoUpdate {
                set: set
                    .into_iter()
                    .map(|(c, v)| (c.to_string(), v.into()))
                    .collect(),
                set_excluded: set_excluded.iter().map(|c| c.to_string()).collect(),
            },
        ));
        self
    }

    // ═══════════════════════════════════════════
    // UPDATE
    // ═══════════════════════════════════════════

    /// UPDATE SET。
    pub fn update_set(mut self, column: &str, value: impl Into<Param>) -> Self {
        self.set_list
            .push(SetClause::new(column, SetValue::Param(value.into())));
        self
    }

    /// 可选 SET：`None` 时跳过。
    pub fn set_opt(self, column: &str, value: Option<impl Into<Param>>) -> Self {
        match value {
            Some(v) => self.update_set(column, v),
            None => self,
        }
    }

    /// SET 子查询。
    pub fn set_subquery(mut self, column: &str, subquery: QueryBuilder) -> Self {
        self.set_list.push(SetClause::new(
            column,
            SetValue::Subquery(Box::new(subquery)),
        ));
        self
    }

    /// 放行无 WHERE 的 UPDATE（默认拒绝）。
    ///
    /// 默认情况下 `UPDATE` 必须有 WHERE 条件，否则返回 `UnsafeUpdateWithoutWhere`。
    /// 此方法显式放行，将所有行纳入更新范围。
    pub fn allow_unsafe_update(mut self) -> Self {
        self.allow_unsafe_update = true;
        self
    }

    /// 放行无 WHERE 的 DELETE（默认拒绝）。
    ///
    /// 默认情况下 `DELETE` 必须有 WHERE 条件，否则返回 `UnsafeDeleteWithoutWhere`。
    /// 此方法显式放行，删除表中所有行。
    pub fn allow_unsafe_delete(mut self) -> Self {
        self.allow_unsafe_delete = true;
        self
    }

    // ═══════════════════════════════════════════
    // CTE
    // ═══════════════════════════════════════════

    /// 带 CTE：`WITH name AS (subquery)`。
    pub fn with_cte(mut self, name: &str, subquery: QueryBuilder) -> Self {
        self.ctes
            .push(CteNode::new(name, CteBody::Query(Box::new(subquery))));
        self
    }

    /// 带列名的 CTE：`WITH name (col1, col2) AS (...)`。
    pub fn with_cte_columns(
        mut self,
        name: &str,
        columns: &[&str],
        subquery: QueryBuilder,
    ) -> Self {
        let mut node = CteNode::new(name, CteBody::Query(Box::new(subquery)));
        node.columns = Some(columns.iter().map(|c| c.to_string()).collect());
        self.ctes.push(node);
        self
    }

    /// 递归 CTE。
    pub fn recursive_cte(
        mut self,
        name: &str,
        columns: &[&str],
        anchor: QueryBuilder,
        recursive: QueryBuilder,
        union_type: UnionType,
    ) -> Self {
        let mut node = CteNode::new(
            name,
            CteBody::RecursiveUnion {
                anchor: Box::new(anchor),
                recursive: Box::new(recursive),
                union_type,
            },
        );
        node.recursive = true;
        node.columns = Some(columns.iter().map(|c| c.to_string()).collect());
        self.ctes.push(node);
        self
    }

    // ═══════════════════════════════════════════
    // BUILD
    // ═══════════════════════════════════════════

    /// 构建 SQL。
    pub fn build<B: Backend>(&self, backend: &B) -> BuildResult<QueryResult> {
        let mut idx = 1usize;
        match self.mode {
            QueryMode::Select => {
                let (sql, params) = self.build_select_query(backend, &mut idx)?;
                Ok(QueryResult::single(sql, params))
            }
            QueryMode::Insert => {
                let (sql, params) = self.build_insert_query(backend, &mut idx)?;
                Ok(QueryResult::single(sql, params))
            }
            QueryMode::Update => {
                let (sql, params) = self.build_update_query(backend, &mut idx)?;
                Ok(QueryResult::single(sql, params))
            }
            QueryMode::Delete => {
                let (sql, params) = self.build_delete_query(backend, &mut idx)?;
                Ok(QueryResult::single(sql, params))
            }
        }
    }

    // ── 内部 build 方法 ──

    fn build_select_query<B: Backend>(
        &self,
        backend: &B,
        idx: &mut usize,
    ) -> BuildResult<(String, Vec<Param>)> {
        use std::fmt::Write;
        self.validate_where_list(&self.where_list)?;
        self.validate_where_list(&self.having)?;
        self.validate_joins(backend)?;

        if backend.requires_order_by_for_offset()
            && (self.limit.is_some() || self.offset.is_some())
            && self.order_by.is_empty()
        {
            return Err(BuildError::OrderByRequired);
        }
        let mut sql = String::new();
        let mut all_params = vec![];

        // 1. CTE
        if !self.ctes.is_empty() {
            let (cte_sql, cte_params) = self.build_ctes_inner(backend, idx);
            sql.push_str(&cte_sql);
            all_params.extend(cte_params);
        }

        // 2. SELECT
        sql.push_str("SELECT ");
        for (i, col) in self.columns.iter().enumerate() {
            if i > 0 {
                sql.push_str(", ");
            }
            match col {
                SelectExpr::Column(c) => sql.push_str(c),
                SelectExpr::Ident(c) => {
                    // 智能引用：简单列名 → quote_ident，否则原样输出
                    if Self::is_simple_ident(c) {
                        write!(sql, "{}", backend.quote_ident(c)).unwrap();
                    } else {
                        sql.push_str(c);
                    }
                }
                SelectExpr::Subquery(sub, alias) => {
                    let (sub_sql, sub_params) = sub.build_select_query(backend, idx)?;
                    write!(sql, "({sub_sql}) AS {alias}").unwrap();
                    all_params.extend(sub_params);
                }
                SelectExpr::Raw(r) => sql.push_str(r),
            }
        }

        // 3. FROM
        if !self.from.is_empty() {
            sql.push_str(" FROM ");
            for (i, table) in self.from.iter().enumerate() {
                if i > 0 {
                    sql.push_str(", ");
                }
                self.build_table_ref(table, backend, idx, &mut sql, &mut all_params);
            }
        }

        // 4. JOIN
        if !self.joins.is_empty() {
            self.build_joins_inner(backend, idx, &mut sql, &mut all_params);
        }

        // 5. WHERE
        if !self.where_list.is_empty() {
            sql.push_str(" WHERE ");
            self.build_where_list(&self.where_list, backend, idx, &mut sql, &mut all_params);
        }

        // 6. GROUP BY
        if !self.group_by.is_empty() {
            sql.push_str(" GROUP BY ");
            if self.group_by_ident {
                let quoted: Vec<String> = self
                    .group_by
                    .iter()
                    .map(|c| {
                        if Self::is_simple_ident(c) {
                            backend.quote_ident(c)
                        } else {
                            c.clone()
                        }
                    })
                    .collect();
                sql.push_str(&quoted.join(", "));
            } else {
                sql.push_str(&self.group_by.join(", "));
            }
        }

        // 7. HAVING
        if !self.having.is_empty() {
            sql.push_str(" HAVING ");
            self.build_where_list(&self.having, backend, idx, &mut sql, &mut all_params);
        }

        // 8. ORDER BY
        if !self.order_by.is_empty() {
            sql.push_str(" ORDER BY ");
            for (i, (col, dir)) in self.order_by.iter().enumerate() {
                if i > 0 {
                    sql.push_str(", ");
                }
                write!(sql, "{} {}", backend.quote_ident(col), dir.sql()).unwrap();
            }
        }

        // 9. LIMIT / OFFSET
        let lo = backend.limit_offset(self.limit, self.offset);
        if !lo.is_empty() {
            sql.push(' ');
            sql.push_str(&lo);
        }

        Ok((sql, all_params))
    }

    fn build_insert_query<B: Backend>(
        &self,
        backend: &B,
        idx: &mut usize,
    ) -> BuildResult<(String, Vec<Param>)> {
        use std::fmt::Write;
        let table = self
            .insert_table
            .as_deref()
            .ok_or_else(|| BuildError::ModeMismatch("INSERT requires a table".to_string()))?;

        // INSERT FROM SELECT
        if let Some(ref sub) = self.insert_from_select {
            let (sub_sql, sub_params) = sub.build_select_query(backend, idx)?;
            let cols = if self.insert_columns.is_empty() {
                String::new()
            } else {
                format!(
                    " ({})",
                    self.insert_columns
                        .iter()
                        .map(|c| backend.quote_ident(c))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            };
            let sql = format!(
                "INSERT INTO {table}{cols}\n{sub_sql}",
                table = backend.quote_ident(table),
            );
            return Ok((sql, sub_params));
        }

        // INSERT VALUES
        let cols = if self.insert_columns.is_empty() {
            String::new()
        } else {
            format!(
                " ({})",
                self.insert_columns
                    .iter()
                    .map(|c| backend.quote_ident(c))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };

        // Build VALUES clause
        let mut all_params = vec![];
        let mut values_parts = vec![];

        // If insert_values is populated, use it
        if !self.insert_values.is_empty() {
            let _per_row = if self.insert_values.len() == 1 {
                // single row — use columns length
                self.insert_columns.len()
            } else {
                self.insert_values[0].len()
            };

            for row in &self.insert_values {
                let mut placeholders = vec![];
                for val in row {
                    placeholders.push(backend.placeholder(*idx));
                    all_params.push(val.clone());
                    *idx += 1;
                }
                values_parts.push(format!("({})", placeholders.join(", ")));
            }
        } else {
            // fallback: no values yet
            values_parts.push("()".to_string());
        }

        let values_str = values_parts.join(", ");

        // RETURNING
        let mut sql = format!("INSERT INTO {table}{cols} VALUES {values_str}");

        if let Some(ref returning_cols) = self.insert_returning
            && backend.supports_returning()
        {
            write!(sql, " {}", backend.returning(returning_cols)).unwrap();
        }
        // If backend doesn't support RETURNING, add separate statement (handled at executor level)

        // ON CONFLICT
        if let Some(ref conflict) = self.insert_conflict {
            let set_for_conflict: Vec<(String, Param)> = match &conflict.action {
                ConflictAction::DoNothing => vec![],
                ConflictAction::DoUpdate { set, .. } => set.clone(),
            };
            let conflict_sql =
                backend.on_conflict(&conflict.columns, &conflict.action, &set_for_conflict, idx)?;
            write!(sql, " {conflict_sql}").unwrap();
        }

        Ok((sql, all_params))
    }

    fn build_update_query<B: Backend>(
        &self,
        backend: &B,
        idx: &mut usize,
    ) -> BuildResult<(String, Vec<Param>)> {
        use std::fmt::Write;
        if self.set_list.is_empty() {
            return Err(BuildError::NoSetClauses);
        }

        if self.where_list.is_empty() && !self.allow_unsafe_update {
            return Err(BuildError::UnsafeUpdateWithoutWhere);
        }

        self.validate_where_list(&self.where_list)?;

        let table = self
            .update_table
            .as_deref()
            .ok_or_else(|| BuildError::ModeMismatch("UPDATE requires a table".to_string()))?;

        let mut sql = String::new();
        let mut all_params = vec![];

        // CTE
        if !self.ctes.is_empty() {
            let (cte_sql, cte_params) = self.build_ctes_inner(backend, idx);
            sql.push_str(&cte_sql);
            all_params.extend(cte_params);
        }

        write!(sql, "UPDATE {}", backend.quote_ident(table)).unwrap();

        // SET
        sql.push_str(" SET ");
        for (i, set) in self.set_list.iter().enumerate() {
            if i > 0 {
                sql.push_str(", ");
            }
            write!(sql, "{} = ", backend.quote_ident(&set.column)).unwrap();
            match &set.value {
                SetValue::Param(p) => {
                    write!(sql, "{}", backend.placeholder(*idx)).unwrap();
                    all_params.push(p.clone());
                    *idx += 1;
                }
                SetValue::Subquery(sub) => {
                    let (sub_sql, sub_params) = sub.build_select_query(backend, idx)?;
                    write!(sql, "({sub_sql})").unwrap();
                    all_params.extend(sub_params);
                }
                SetValue::Raw(expr) => {
                    sql.push_str(expr);
                }
            }
        }

        // WHERE
        if !self.where_list.is_empty() {
            sql.push_str(" WHERE ");
            self.build_where_list(&self.where_list, backend, idx, &mut sql, &mut all_params);
        }

        Ok((sql, all_params))
    }

    fn build_delete_query<B: Backend>(
        &self,
        backend: &B,
        idx: &mut usize,
    ) -> BuildResult<(String, Vec<Param>)> {
        use std::fmt::Write;
        if self.where_list.is_empty() && !self.allow_unsafe_delete {
            return Err(BuildError::UnsafeDeleteWithoutWhere);
        }

        self.validate_where_list(&self.where_list)?;

        let table = self
            .delete_table
            .as_deref()
            .ok_or_else(|| BuildError::ModeMismatch("DELETE requires a table".to_string()))?;

        let mut sql = String::new();
        let mut all_params = vec![];

        // CTE
        if !self.ctes.is_empty() {
            let (cte_sql, cte_params) = self.build_ctes_inner(backend, idx);
            sql.push_str(&cte_sql);
            all_params.extend(cte_params);
        }

        write!(sql, "DELETE FROM {}", backend.quote_ident(table)).unwrap();

        // WHERE
        if !self.where_list.is_empty() {
            sql.push_str(" WHERE ");
            self.build_where_list(&self.where_list, backend, idx, &mut sql, &mut all_params);
        }

        // RETURNING
        if let Some(ref returning_cols) = self.delete_returning
            && backend.supports_returning()
        {
            write!(sql, " {}", backend.returning(returning_cols)).unwrap();
        }

        Ok((sql, all_params))
    }

    // ── 辅助 build 方法 ──

    fn build_table_ref<B: Backend>(
        &self,
        table: &TableRef,
        backend: &B,
        idx: &mut usize,
        sql: &mut String,
        params: &mut Vec<Param>,
    ) {
        use std::fmt::Write;
        match table {
            TableRef::Table(name) => {
                write!(sql, "{}", backend.quote_ident(name)).unwrap();
            }
            TableRef::TableAs(name, alias) => {
                write!(sql, "{} AS {alias}", backend.quote_ident(name)).unwrap();
            }
            TableRef::Subquery(sub, alias) => {
                // Build subquery in place
                if let Ok((sub_sql, mut sub_params)) = sub.build_select_query(backend, idx) {
                    write!(sql, "({sub_sql}) AS {alias}").unwrap();
                    params.append(&mut sub_params);
                }
            }
            TableRef::CteRef(name, alias) => {
                if let Some(a) = alias {
                    write!(sql, "{name} AS {a}").unwrap();
                } else {
                    sql.push_str(name);
                }
            }
        }
    }

    /// 递归校验 WHERE 条件组，提前阻断非法 SQL。
    ///
    /// 当前规则：
    /// - `IN` 子句至少含一个非空表达式
    fn validate_where_list(&self, groups: &[WhereGroup]) -> BuildResult<()> {
        for group in groups {
            match &group.kind {
                WhereKind::In { values, .. } => {
                    if values.is_empty() || values.iter().all(|v| v.is_empty()) {
                        return Err(BuildError::EmptyInClause);
                    }
                }
                WhereKind::Group(inner) => {
                    self.validate_where_list(inner)?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// 校验所有 JOIN 是否被当前 backend 支持。
    fn validate_joins<B: Backend>(&self, backend: &B) -> BuildResult<()> {
        for join in &self.joins {
            if !backend.supports_join_type(join.join_type) {
                return Err(BuildError::UnsupportedJoinType(
                    join.join_type.sql().to_string(),
                ));
            }
        }
        Ok(())
    }

    /// 判断列名是否为简单标识符（仅字母数字下划线）。
    /// 函数表达式（如 `UPPER(name)`）含 `()`/`.`/空格等，识别为非简单标识符不加引号。
    /// 与 ADR-002 列名智能引用策略一致。
    fn is_simple_ident(name: &str) -> bool {
        !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_')
    }

    fn build_where_list<B: Backend>(
        &self,
        groups: &[WhereGroup],
        backend: &B,
        idx: &mut usize,
        sql: &mut String,
        params: &mut Vec<Param>,
    ) {
        use std::fmt::Write;
        for (i, group) in groups.iter().enumerate() {
            if i > 0 {
                write!(sql, " {} ", group.logic.sql()).unwrap();
            }
            match &group.kind {
                WhereKind::Column { column, op, value } => {
                    if Self::is_simple_ident(column) {
                        write!(sql, "{} {} ", backend.quote_ident(column), op.sql()).unwrap();
                    } else {
                        write!(sql, "{} {} ", column, op.sql()).unwrap();
                    }
                    match value {
                        Expr::Value(p) => {
                            write!(sql, "{}", backend.placeholder(*idx)).unwrap();
                            params.push(p.clone());
                            *idx += 1;
                        }
                        Expr::Column(col) => {
                            sql.push_str(col); // column ref, not quoted
                        }
                        Expr::Subquery(sub) => {
                            if let Ok((sub_sql, mut sub_params)) =
                                sub.build_select_query(backend, idx)
                            {
                                write!(sql, "({sub_sql})").unwrap();
                                params.append(&mut sub_params);
                            }
                        }
                        Expr::RawExpr(expr) => {
                            sql.push_str(expr);
                        }
                    }
                }
                WhereKind::In {
                    column,
                    values,
                    negated,
                } => {
                    let not = if *negated { " NOT" } else { "" };
                    write!(sql, "{}{} IN (", backend.quote_ident(column), not).unwrap();
                    let flat_values: Vec<&Expr> = values.iter().flatten().collect();
                    for (j, expr) in flat_values.iter().enumerate() {
                        if j > 0 {
                            sql.push_str(", ");
                        }
                        match expr {
                            Expr::Value(p) => {
                                write!(sql, "{}", backend.placeholder(*idx)).unwrap();
                                params.push(p.clone());
                                *idx += 1;
                            }
                            Expr::Subquery(sub) => {
                                if let Ok((sub_sql, mut sub_params)) =
                                    sub.build_select_query(backend, idx)
                                {
                                    write!(sql, "{sub_sql}").unwrap();
                                    params.append(&mut sub_params);
                                }
                            }
                            _ => {}
                        }
                    }
                    sql.push(')');
                }
                WhereKind::Between { column, low, high } => {
                    write!(
                        sql,
                        "{} BETWEEN {} AND {}",
                        backend.quote_ident(column),
                        match low {
                            Expr::Value(p) => {
                                let ph = backend.placeholder(*idx);
                                params.push(p.clone());
                                *idx += 1;
                                ph
                            }
                            _ => String::new(),
                        },
                        match high {
                            Expr::Value(p) => {
                                let ph = backend.placeholder(*idx);
                                params.push(p.clone());
                                *idx += 1;
                                ph
                            }
                            _ => String::new(),
                        }
                    )
                    .unwrap();
                }
                WhereKind::IsNull { column, negated } => {
                    let not = if *negated { " NOT" } else { "" };
                    write!(sql, "{}{} IS NULL", backend.quote_ident(column), not).unwrap();
                }
                WhereKind::Exists { subquery, negated } => {
                    let not = if *negated { " NOT" } else { "" };
                    write!(sql, "{not} EXISTS (").unwrap();
                    if let Ok((sub_sql, mut sub_params)) = subquery.build_select_query(backend, idx)
                    {
                        sql.push_str(&sub_sql);
                        params.append(&mut sub_params);
                    }
                    sql.push(')');
                }
                WhereKind::Group(groups) => {
                    sql.push('(');
                    self.build_where_list(groups, backend, idx, sql, params);
                    sql.push(')');
                }
                WhereKind::Raw(expr, extra) => {
                    sql.push_str(expr);
                    params.extend(extra.iter().cloned());
                    *idx += extra.len();
                }
            }
        }
    }

    fn build_joins_inner<B: Backend>(
        &self,
        backend: &B,
        idx: &mut usize,
        sql: &mut String,
        params: &mut Vec<Param>,
    ) {
        use std::fmt::Write;
        for join in &self.joins {
            let alias_str = join.alias_str();
            write!(
                sql,
                " {} {} {}",
                join.join_type.sql(),
                match &join.table {
                    TableRef::Subquery(sub, _) => {
                        if let Ok((sub_sql, _)) = sub.build_select_query(backend, idx) {
                            format!("({sub_sql})")
                        } else {
                            String::new()
                        }
                    }
                    other => {
                        let mut buf = String::new();
                        self.build_table_ref(other, backend, idx, &mut buf, params);
                        buf
                    }
                },
                alias_str,
            )
            .unwrap();

            if !join.conditions.is_empty() {
                sql.push_str(" ON ");
                self.build_on_conditions(&join.conditions, backend, idx, sql, params);
            }
        }
    }

    fn build_on_conditions<B: Backend>(
        &self,
        conditions: &[OnCondition],
        backend: &B,
        idx: &mut usize,
        sql: &mut String,
        params: &mut Vec<Param>,
    ) {
        use std::fmt::Write;
        for (i, cond) in conditions.iter().enumerate() {
            if i > 0 {
                match cond.logic() {
                    Some(LogicOp::And) => sql.push_str(" AND "),
                    Some(LogicOp::Or) => sql.push_str(" OR "),
                    None => sql.push_str(" AND "),
                }
            }
            match cond {
                OnCondition::Eq { left, right } => {
                    write!(
                        sql,
                        "{} = {}",
                        backend.quote_ident(left),
                        backend.quote_ident(right)
                    )
                    .unwrap();
                }
                OnCondition::EqValue { column, op, value } => {
                    write!(
                        sql,
                        "{} {} {}",
                        backend.quote_ident(column),
                        op.sql(),
                        backend.placeholder(*idx)
                    )
                    .unwrap();
                    params.push(value.clone());
                    *idx += 1;
                }
                OnCondition::Group {
                    conditions: sub, ..
                } => {
                    sql.push('(');
                    self.build_on_conditions(sub, backend, idx, sql, params);
                    sql.push(')');
                }
                OnCondition::Raw(expr, extra) => {
                    sql.push_str(expr);
                    params.extend(extra.iter().cloned());
                    *idx += extra.len();
                }
            }
        }
    }

    fn build_ctes_inner<B: Backend>(&self, backend: &B, idx: &mut usize) -> (String, Vec<Param>) {
        use std::fmt::Write;
        let mut sql = String::new();
        let mut all_params = vec![];

        let recursive = self.ctes.iter().any(|c| c.recursive);
        if recursive {
            sql.push_str("WITH RECURSIVE ");
        } else {
            sql.push_str("WITH ");
        }

        for (i, cte) in self.ctes.iter().enumerate() {
            if i > 0 {
                sql.push_str(", ");
            }

            write!(sql, "{}", cte.name).unwrap();
            if let Some(ref cols) = cte.columns {
                write!(sql, " ({})", cols.join(", ")).unwrap();
            }
            sql.push_str(" AS (");

            match &cte.body {
                CteBody::Query(sub) => {
                    if let Ok((sub_sql, mut sub_params)) = sub.build_select_query(backend, idx) {
                        sql.push_str(&sub_sql);
                        all_params.append(&mut sub_params);
                    }
                }
                CteBody::RecursiveUnion {
                    anchor,
                    recursive,
                    union_type,
                } => {
                    if let Ok((a_sql, mut a_params)) = anchor.build_select_query(backend, idx) {
                        sql.push_str(&a_sql);
                        all_params.append(&mut a_params);
                    }
                    write!(sql, " {} ", union_type.sql()).unwrap();
                    if let Ok((r_sql, mut r_params)) = recursive.build_select_query(backend, idx) {
                        sql.push_str(&r_sql);
                        all_params.append(&mut r_params);
                    }
                }
            }

            sql.push(')');
        }

        sql.push(' ');
        (sql, all_params)
    }
}

// ============================================================
// WhereAdder — WHERE 条件构造器中间态
// ============================================================

/// `and_where("col")` 或 `or_where("col")` 返回的中间态。
/// 调用 `.eq(val)` 等完成条件并返回主 builder。
pub struct WhereAdder<T> {
    target: T,
    column: String,
    logic: LogicOp,
}

impl<T: HasWhere> WhereAdder<T> {
    pub fn eq(self, val: impl Into<Param>) -> T {
        let Self {
            mut target,
            logic,
            column,
        } = self;
        let kind = WhereKind::Column {
            column,
            op: CmpOp::Eq,
            value: Expr::Value(val.into()),
        };
        target.add_where(logic, kind)
    }

    pub fn ne(self, val: impl Into<Param>) -> T {
        let Self {
            mut target,
            logic,
            column,
        } = self;
        let kind = WhereKind::Column {
            column,
            op: CmpOp::Ne,
            value: Expr::Value(val.into()),
        };
        target.add_where(logic, kind)
    }

    pub fn gt(self, val: impl Into<Param>) -> T {
        let Self {
            mut target,
            logic,
            column,
        } = self;
        let kind = WhereKind::Column {
            column,
            op: CmpOp::Gt,
            value: Expr::Value(val.into()),
        };
        target.add_where(logic, kind)
    }

    pub fn gte(self, val: impl Into<Param>) -> T {
        let Self {
            mut target,
            logic,
            column,
        } = self;
        let kind = WhereKind::Column {
            column,
            op: CmpOp::Gte,
            value: Expr::Value(val.into()),
        };
        target.add_where(logic, kind)
    }

    pub fn lt(self, val: impl Into<Param>) -> T {
        let Self {
            mut target,
            logic,
            column,
        } = self;
        let kind = WhereKind::Column {
            column,
            op: CmpOp::Lt,
            value: Expr::Value(val.into()),
        };
        target.add_where(logic, kind)
    }

    pub fn lte(self, val: impl Into<Param>) -> T {
        let Self {
            mut target,
            logic,
            column,
        } = self;
        let kind = WhereKind::Column {
            column,
            op: CmpOp::Lte,
            value: Expr::Value(val.into()),
        };
        target.add_where(logic, kind)
    }

    /// LIKE 条件。值始终参数化，杜绝 SQL 注入。
    pub fn like(self, val: impl Into<Param>) -> T {
        let Self {
            mut target,
            logic,
            column,
        } = self;
        let kind = WhereKind::Column {
            column,
            op: CmpOp::Like,
            value: Expr::Value(val.into()),
        };
        target.add_where(logic, kind)
    }

    /// 可选 eq：`None` 时跳过条件。
    pub fn eq_opt(self, val: Option<impl Into<Param>>) -> T {
        match val {
            Some(v) => self.eq(v),
            None => self.target,
        }
    }

    /// LIKE 可选。
    pub fn like_opt(self, val: Option<impl Into<Param>>) -> T {
        match val {
            Some(v) => self.like(v),
            None => self.target,
        }
    }

    /// 可选 ne：`None` 时跳过条件。
    pub fn ne_opt(self, val: Option<impl Into<Param>>) -> T {
        match val {
            Some(v) => self.ne(v),
            None => self.target,
        }
    }

    /// 可选 gt：`None` 时跳过条件。
    pub fn gt_opt(self, val: Option<impl Into<Param>>) -> T {
        match val {
            Some(v) => self.gt(v),
            None => self.target,
        }
    }

    /// 可选 gte：`None` 时跳过条件。
    pub fn gte_opt(self, val: Option<impl Into<Param>>) -> T {
        match val {
            Some(v) => self.gte(v),
            None => self.target,
        }
    }

    /// 可选 lt：`None` 时跳过条件。
    pub fn lt_opt(self, val: Option<impl Into<Param>>) -> T {
        match val {
            Some(v) => self.lt(v),
            None => self.target,
        }
    }

    /// 可选 lte：`None` 时跳过条件。
    pub fn lte_opt(self, val: Option<impl Into<Param>>) -> T {
        match val {
            Some(v) => self.lte(v),
            None => self.target,
        }
    }

    /// IN 条件。
    pub fn in_(self, vals: impl IntoIterator<Item: Into<Param>>) -> T {
        let Self {
            mut target,
            logic,
            column,
        } = self;
        let expr_vals: Vec<Vec<Expr>> = vals
            .into_iter()
            .map(|v| vec![Expr::Value(v.into())])
            .collect();
        let kind = WhereKind::In {
            column,
            values: expr_vals,
            negated: false,
        };
        target.add_where(logic, kind)
    }

    /// IN 可选。
    pub fn in_opt(self, vals: Option<impl IntoIterator<Item: Into<Param>>>) -> T {
        match vals {
            Some(v) => self.in_(v),
            None => self.target,
        }
    }

    /// IN 子查询。
    pub fn in_subquery(self, subquery: QueryBuilder) -> T {
        let Self {
            mut target,
            logic,
            column,
        } = self;
        let kind = WhereKind::In {
            column,
            values: vec![vec![Expr::Subquery(Box::new(subquery))]],
            negated: false,
        };
        target.add_where(logic, kind)
    }

    /// 列 = (子查询)。子查询必须返回单行。
    pub fn eq_subquery(self, sub: QueryBuilder) -> T {
        let Self {
            mut target,
            logic,
            column,
        } = self;
        let kind = WhereKind::Column {
            column,
            op: CmpOp::Eq,
            value: Expr::Subquery(Box::new(sub)),
        };
        target.add_where(logic, kind)
    }

    /// 列 <> (子查询)。
    pub fn neq_subquery(self, sub: QueryBuilder) -> T {
        let Self {
            mut target,
            logic,
            column,
        } = self;
        let kind = WhereKind::Column {
            column,
            op: CmpOp::Ne,
            value: Expr::Subquery(Box::new(sub)),
        };
        target.add_where(logic, kind)
    }

    /// 列 > (子查询)。
    pub fn gt_subquery(self, sub: QueryBuilder) -> T {
        let Self {
            mut target,
            logic,
            column,
        } = self;
        let kind = WhereKind::Column {
            column,
            op: CmpOp::Gt,
            value: Expr::Subquery(Box::new(sub)),
        };
        target.add_where(logic, kind)
    }

    /// 列 >= (子查询)。
    pub fn gte_subquery(self, sub: QueryBuilder) -> T {
        let Self {
            mut target,
            logic,
            column,
        } = self;
        let kind = WhereKind::Column {
            column,
            op: CmpOp::Gte,
            value: Expr::Subquery(Box::new(sub)),
        };
        target.add_where(logic, kind)
    }

    /// 列 < (子查询)。
    pub fn lt_subquery(self, sub: QueryBuilder) -> T {
        let Self {
            mut target,
            logic,
            column,
        } = self;
        let kind = WhereKind::Column {
            column,
            op: CmpOp::Lt,
            value: Expr::Subquery(Box::new(sub)),
        };
        target.add_where(logic, kind)
    }

    /// 列 <= (子查询)。
    pub fn lte_subquery(self, sub: QueryBuilder) -> T {
        let Self {
            mut target,
            logic,
            column,
        } = self;
        let kind = WhereKind::Column {
            column,
            op: CmpOp::Lte,
            value: Expr::Subquery(Box::new(sub)),
        };
        target.add_where(logic, kind)
    }

    /// IS NULL。
    pub fn is_null(self) -> T {
        let Self {
            mut target,
            logic,
            column,
        } = self;
        let kind = WhereKind::IsNull {
            column,
            negated: false,
        };
        target.add_where(logic, kind)
    }

    /// IS NOT NULL。
    pub fn is_not_null(self) -> T {
        let Self {
            mut target,
            logic,
            column,
        } = self;
        let kind = WhereKind::IsNull {
            column,
            negated: true,
        };
        target.add_where(logic, kind)
    }

    /// BETWEEN。
    pub fn between(self, low: impl Into<Param>, high: impl Into<Param>) -> T {
        let Self {
            mut target,
            logic,
            column,
        } = self;
        let kind = WhereKind::Between {
            column,
            low: Expr::Value(low.into()),
            high: Expr::Value(high.into()),
        };
        target.add_where(logic, kind)
    }

    /// 列 = 列（关联引用）。
    pub fn eq_col(self, col: &str) -> T {
        let Self {
            mut target,
            logic,
            column,
        } = self;
        let kind = WhereKind::Column {
            column,
            op: CmpOp::Eq,
            value: Expr::Column(col.to_string()),
        };
        target.add_where(logic, kind)
    }

    /// 原始 SQL 逃生舱。
    pub fn raw(mut self, sql: &str, params: Vec<Param>) -> T {
        self.target.add_where_raw(self.logic, sql, params)
    }
}

// ============================================================
// GroupBuilder — 条件分组构造器
// ============================================================

/// `and_group(|g| { g.or_where(...)... })` 中的闭包参数。
pub struct GroupBuilder {
    groups: Vec<WhereGroup>,
    logic: LogicOp,
}

impl GroupBuilder {
    pub(crate) fn new(logic: LogicOp) -> Self {
        Self {
            groups: vec![],
            logic,
        }
    }

    /// AND 条件。
    pub fn and_where(self, column: &str) -> WhereAdder<Self> {
        WhereAdder {
            target: self,
            column: column.to_string(),
            logic: LogicOp::And,
        }
    }

    /// OR 条件。
    pub fn or_where(self, column: &str) -> WhereAdder<Self> {
        WhereAdder {
            target: self,
            column: column.to_string(),
            logic: LogicOp::Or,
        }
    }

    /// AND 函数表达式条件（如 `UPPER(name)`）。
    pub fn and_where_expr(self, expr: &str) -> WhereAdder<Self> {
        self.and_where(expr)
    }

    /// OR 函数表达式条件。
    pub fn or_where_expr(self, expr: &str) -> WhereAdder<Self> {
        self.or_where(expr)
    }

    /// OR EXISTS。
    pub fn or_exists(mut self, subquery: QueryBuilder) -> Self {
        self.groups.push(WhereGroup::new(
            self.logic,
            WhereKind::Exists {
                subquery: Box::new(subquery),
                negated: false,
            },
        ));
        self
    }

    /// 子分组。
    pub fn and_group(mut self, f: impl FnOnce(GroupBuilder) -> GroupBuilder) -> Self {
        let inner = GroupBuilder::new(LogicOp::And);
        let inner = f(inner);
        self.groups.push(WhereGroup::new(
            LogicOp::And,
            WhereKind::Group(inner.groups),
        ));
        self
    }

    /// 子分组（OR）。
    pub fn or_group(mut self, f: impl FnOnce(GroupBuilder) -> GroupBuilder) -> Self {
        let inner = GroupBuilder::new(LogicOp::Or);
        let inner = f(inner);
        self.groups
            .push(WhereGroup::new(LogicOp::Or, WhereKind::Group(inner.groups)));
        self
    }
}

impl HasWhere for GroupBuilder {
    fn add_where(&mut self, _logic: LogicOp, kind: WhereKind) -> Self {
        self.groups.push(WhereGroup::new(self.logic, kind));
        std::mem::take(self)
    }

    fn add_where_raw(&mut self, _logic: LogicOp, sql: &str, params: Vec<Param>) -> Self {
        self.groups.push(WhereGroup::new(
            self.logic,
            WhereKind::Raw(sql.to_string(), params),
        ));
        std::mem::take(self)
    }

    fn where_mut(&mut self) -> &mut Vec<WhereGroup> {
        &mut self.groups
    }
}

// ============================================================
// HasWhere trait
// ============================================================

/// 类型可拥有 WHERE 条件（QueryBuilder / GroupBuilder 共用）。
pub trait HasWhere: Sized {
    fn add_where(&mut self, logic: LogicOp, kind: WhereKind) -> Self;
    fn add_where_raw(&mut self, logic: LogicOp, sql: &str, params: Vec<Param>) -> Self;
    fn where_mut(&mut self) -> &mut Vec<WhereGroup>;
}

impl HasWhere for QueryBuilder {
    fn add_where(&mut self, logic: LogicOp, kind: WhereKind) -> Self {
        self.where_list.push(WhereGroup::new(logic, kind));
        std::mem::take(self)
    }

    fn add_where_raw(&mut self, _logic: LogicOp, sql: &str, params: Vec<Param>) -> Self {
        self.where_list.push(WhereGroup::new(
            _logic,
            WhereKind::Raw(sql.to_string(), params),
        ));
        std::mem::take(self)
    }

    fn where_mut(&mut self) -> &mut Vec<WhereGroup> {
        &mut self.where_list
    }
}

// ============================================================
// JoinAdder — JOIN 条件构造器中间态
// ============================================================

/// `join("table", "t")` 或 `left_join("table", "t")` 返回的中间态。
/// 调用 `.on(left, right)` 完成并返回主 builder。
pub struct JoinAdder<T> {
    target: T,
    join_idx: usize,
}

impl<T: HasJoins> JoinAdder<T> {
    /// 主 ON 条件：`left_col = right_col`。
    pub fn on(mut self, left: &str, right: &str) -> T {
        self.target.add_join_cond(
            self.join_idx,
            OnCondition::Eq {
                left: left.to_string(),
                right: right.to_string(),
            },
        );
        self.target
    }

    /// AND 附加条件（列 = 值）。
    pub fn and_on(self, column: &str) -> OnAdder<T> {
        OnAdder {
            target: self.target,
            join_idx: self.join_idx,
            column: column.to_string(),
        }
    }

    /// OR 附加条件。
    pub fn or_on(self, column: &str) -> OnAdder<T> {
        OnAdder {
            target: self.target,
            join_idx: self.join_idx,
            column: column.to_string(),
        }
    }

    /// AND 子分组。
    pub fn and_group(mut self, f: impl FnOnce(OnGroupBuilder) -> OnGroupBuilder) -> T {
        let group = f(OnGroupBuilder::new());
        self.target.add_join_cond(
            self.join_idx,
            OnCondition::Group {
                logic: LogicOp::And,
                conditions: group.conditions,
            },
        );
        self.target
    }

    /// OR 子分组。
    pub fn or_group(mut self, f: impl FnOnce(OnGroupBuilder) -> OnGroupBuilder) -> T {
        let group = f(OnGroupBuilder::new());
        self.target.add_join_cond(
            self.join_idx,
            OnCondition::Group {
                logic: LogicOp::Or,
                conditions: group.conditions,
            },
        );
        self.target
    }
}

// ============================================================
// OnAdder — ON 条件构造器中间态
// ============================================================

/// `.and_on("col")` 或 `.or_on("col")` 返回的中间态。
pub struct OnAdder<T> {
    target: T,
    join_idx: usize,
    column: String,
}

impl<T: HasJoins> OnAdder<T> {
    pub fn eq(self, val: impl Into<Param>) -> T {
        self.add_on_cond(CmpOp::Eq, val.into())
    }

    pub fn ne(self, val: impl Into<Param>) -> T {
        self.add_on_cond(CmpOp::Ne, val.into())
    }

    pub fn gt(self, val: impl Into<Param>) -> T {
        self.add_on_cond(CmpOp::Gt, val.into())
    }

    pub fn gte(self, val: impl Into<Param>) -> T {
        self.add_on_cond(CmpOp::Gte, val.into())
    }

    pub fn lt(self, val: impl Into<Param>) -> T {
        self.add_on_cond(CmpOp::Lt, val.into())
    }

    pub fn lte(self, val: impl Into<Param>) -> T {
        self.add_on_cond(CmpOp::Lte, val.into())
    }

    fn add_on_cond(mut self, op: CmpOp, value: Param) -> T {
        self.target.add_join_cond(
            self.join_idx,
            OnCondition::EqValue {
                column: self.column.clone(),
                op,
                value,
            },
        );
        self.target
    }
}

// ============================================================
// OnGroupBuilder — ON 条件分组构造器
// ============================================================

pub struct OnGroupBuilder {
    pub(crate) conditions: Vec<OnCondition>,
}

impl OnGroupBuilder {
    pub fn new() -> Self {
        Self { conditions: vec![] }
    }

    pub fn or_on(self, column: &str) -> OnAdderForGroup {
        OnAdderForGroup {
            target: self,
            column: column.to_string(),
        }
    }
}

impl Default for OnGroupBuilder {
    fn default() -> Self {
        Self::new()
    }
}

pub struct OnAdderForGroup {
    target: OnGroupBuilder,
    column: String,
}

impl OnAdderForGroup {
    pub fn eq(mut self, val: impl Into<Param>) -> OnGroupBuilder {
        self.target.conditions.push(OnCondition::EqValue {
            column: self.column,
            op: CmpOp::Eq,
            value: val.into(),
        });
        self.target
    }
}

// ============================================================
// HasJoins trait
// ============================================================

pub trait HasJoins: Sized {
    fn add_join_cond(&mut self, join_idx: usize, condition: OnCondition);
}

impl HasJoins for QueryBuilder {
    fn add_join_cond(&mut self, join_idx: usize, condition: OnCondition) {
        if join_idx < self.joins.len() {
            self.joins[join_idx].conditions.push(condition);
        }
    }
}

// ============================================================
// RowCollector — 批量插入收集器
// ============================================================

pub struct RowCollector<'a> {
    builder: &'a mut QueryBuilder,
}

impl<'a> RowCollector<'a> {
    pub fn row(self, vals: &[impl Into<Param> + Clone]) -> Self {
        let params: Vec<Param> = vals.iter().map(|v| v.clone().into()).collect();
        if !self.builder.insert_values.is_empty() {
            let expected = self.builder.insert_columns.len();
            assert_eq!(
                params.len(),
                expected,
                "Bulk insert: row has {} values, expected {}",
                params.len(),
                expected
            );
        }
        self.builder.insert_values.push(params);
        self
    }
}

// ============================================================
// UnionType::sql()
// ============================================================

impl UnionType {
    pub fn sql(&self) -> &'static str {
        match self {
            UnionType::UnionAll => "UNION ALL",
            UnionType::Union => "UNION",
        }
    }
}

// ============================================================
// Default impl for QueryBuilder
// ============================================================

impl Default for QueryBuilder {
    fn default() -> Self {
        Self::new(QueryMode::Select)
    }
}

impl Default for GroupBuilder {
    fn default() -> Self {
        Self::new(LogicOp::And)
    }
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    //! Phase 1 修复验证测试。
    //!
    //! 后续阶段的测试将基于稳定 API 编写，参见 `roadmap.md` Phase 6。
    use super::*;

    #[cfg(feature = "postgresql")]
    use crate::backends::postgres::PostgresBackend;

    /// 验证 LIKE 条件将值参数化（不再拼入 SQL 字符串）。
    #[cfg(feature = "postgresql")]
    #[test]
    fn like_uses_parameterized_placeholder() {
        // 含 SQL 注入尝试的 LIKE 值
        let payload = "alice%' OR '1'='1";
        let result = QueryBuilder::select(&["id"])
            .from("users")
            .and_where("name")
            .like(payload)
            .build(&PostgresBackend)
            .unwrap();

        // SQL 字符串里只含占位符，不含 payload
        assert!(
            !result.sql.contains(payload),
            "LIKE payload leaked into SQL: {}",
            result.sql
        );
        assert!(
            result.sql.contains("$1"),
            "expected $1 placeholder: {}",
            result.sql
        );
        assert!(
            result.sql.contains("LIKE"),
            "expected LIKE keyword: {}",
            result.sql
        );

        // 参数列表里包含原值
        assert_eq!(result.params, vec![Param::Text(payload.to_string())]);
    }

    /// 验证 like_opt(None) 跳过条件，值不影响参数。
    #[cfg(feature = "postgresql")]
    #[test]
    fn like_opt_none_skips_condition() {
        let result = QueryBuilder::select(&["id"])
            .from("users")
            .and_where("name")
            .like_opt(Option::<&str>::None)
            .build(&PostgresBackend)
            .unwrap();

        assert!(
            !result.sql.contains("LIKE"),
            "like_opt(None) should not emit LIKE"
        );
        assert!(result.params.is_empty());
    }

    /// 验证空 `in_([])` 在 build 时返回 `EmptyInClause`。
    #[cfg(feature = "postgresql")]
    #[test]
    fn in_clause_with_empty_iter_returns_error() {
        let result = QueryBuilder::select(&["id"])
            .from("users")
            .and_where("id")
            .in_(std::iter::empty::<i32>())
            .build(&PostgresBackend);

        assert!(matches!(result, Err(BuildError::EmptyInClause)));
    }

    /// 验证 `in_opt(None)` 不触发空 IN（条件被跳过）。
    #[cfg(feature = "postgresql")]
    #[test]
    fn in_opt_none_skips_condition() {
        let result = QueryBuilder::select(&["id"])
            .from("users")
            .and_where("id")
            .in_opt(Option::<Vec<i32>>::None)
            .build(&PostgresBackend)
            .unwrap();

        assert!(
            !result.sql.contains("IN"),
            "in_opt(None) should not emit IN"
        );
        assert!(result.params.is_empty());
    }

    /// 验证嵌套在 `and_group` 内的空 IN 也能被递归捕获。
    #[cfg(feature = "postgresql")]
    #[test]
    fn nested_empty_in_clause_returns_error() {
        let result = QueryBuilder::select(&["id"])
            .from("users")
            .and_group(|g| g.and_where("id").in_(std::iter::empty::<i32>()))
            .build(&PostgresBackend);

        assert!(matches!(result, Err(BuildError::EmptyInClause)));
    }

    /// 验证 MSSQL UPSERT 在 build 时返回 `UnsupportedFeature`（不静默生成非法 SQL）。
    #[cfg(feature = "mssql")]
    #[test]
    fn mssql_upsert_returns_unsupported_error() {
        use crate::backends::mssql::MssqlBackend;

        let result = QueryBuilder::insert_into("users")
            .set("id", 1)
            .set("name", "alice")
            .on_conflict_do_nothing(&["id"])
            .build(&MssqlBackend);

        match result {
            Err(BuildError::UnsupportedFeature(msg)) => {
                assert!(msg.contains("MSSQL"), "unexpected message: {msg}");
            }
            other => panic!("expected UnsupportedFeature, got: {other:?}"),
        }
    }

    /// 验证 Postgres UPSERT (DO NOTHING) 仍正常工作。
    #[cfg(feature = "postgresql")]
    #[test]
    fn postgres_upsert_do_nothing_still_works() {
        let result = QueryBuilder::insert_into("users")
            .set("id", 1)
            .set("name", "alice")
            .on_conflict_do_nothing(&["id"])
            .build(&PostgresBackend)
            .unwrap();

        assert!(result.sql.contains("ON CONFLICT (\"id\") DO NOTHING"));
    }

    /// 验证 SQLite 拒绝 RIGHT JOIN。
    #[cfg(feature = "sqlite")]
    #[test]
    fn sqlite_rejects_right_join() {
        use crate::backends::sqlite::SqliteBackend;

        let result = QueryBuilder::select(&["a.id"])
            .from("a")
            .right_join("b", "b")
            .on("a.id", "b.a_id")
            .build(&SqliteBackend);

        match result {
            Err(BuildError::UnsupportedJoinType(t)) => {
                assert!(t.contains("RIGHT"), "unexpected join type: {t}");
            }
            other => panic!("expected UnsupportedJoinType, got: {other:?}"),
        }
    }

    /// 验证 SQLite 拒绝 FULL JOIN。
    #[cfg(feature = "sqlite")]
    #[test]
    fn sqlite_rejects_full_join() {
        use crate::backends::sqlite::SqliteBackend;

        let result = QueryBuilder::select(&["a.id"])
            .from("a")
            .full_join("b", "b")
            .on("a.id", "b.a_id")
            .build(&SqliteBackend);

        assert!(matches!(result, Err(BuildError::UnsupportedJoinType(_))));
    }

    /// 验证 SQLite 接受 INNER JOIN（默认应支持）。
    #[cfg(feature = "sqlite")]
    #[test]
    fn sqlite_accepts_inner_join() {
        use crate::backends::sqlite::SqliteBackend;

        let result = QueryBuilder::select(&["a.id"])
            .from("a")
            .join("b", "b")
            .on("a.id", "b.a_id")
            .build(&SqliteBackend)
            .unwrap();

        assert!(result.sql.contains("INNER JOIN"));
    }

    // ============================================================
    // Phase 2 — *Opt 变体
    // ============================================================

    /// ne_opt(Some(val)) 生成 `<> $1` 条件。
    #[cfg(feature = "postgresql")]
    #[test]
    fn ne_opt_with_value_adds_condition() {
        let result = QueryBuilder::select(&["id"])
            .from("users")
            .and_where("status")
            .ne_opt(Some("banned"))
            .build(&PostgresBackend)
            .unwrap();

        assert!(result.sql.contains("<>"));
        assert!(result.sql.contains("$1"));
        assert_eq!(result.params, vec![Param::Text("banned".to_string())]);
    }

    /// ne_opt(None) 跳过条件。
    #[cfg(feature = "postgresql")]
    #[test]
    fn ne_opt_none_skips_condition() {
        let result = QueryBuilder::select(&["id"])
            .from("users")
            .and_where("status")
            .ne_opt(Option::<&str>::None)
            .build(&PostgresBackend)
            .unwrap();

        assert!(!result.sql.contains("<>"));
        assert!(result.params.is_empty());
    }

    /// gt_opt 正常工作。
    #[cfg(feature = "postgresql")]
    #[test]
    fn gt_opt_adds_condition() {
        let result = QueryBuilder::select(&["id"])
            .from("users")
            .and_where("age")
            .gt_opt(Some(18))
            .build(&PostgresBackend)
            .unwrap();

        assert!(result.sql.contains("> $1"));
        assert_eq!(result.params, vec![Param::I32(18)]);
    }

    /// gte_opt 正常工作。
    #[cfg(feature = "postgresql")]
    #[test]
    fn gte_opt_adds_condition() {
        let result = QueryBuilder::select(&["id"])
            .from("users")
            .and_where("age")
            .gte_opt(Some(18))
            .build(&PostgresBackend)
            .unwrap();

        assert!(result.sql.contains(">= $1"));
        assert_eq!(result.params, vec![Param::I32(18)]);
    }

    /// lt_opt 正常工作。
    #[cfg(feature = "postgresql")]
    #[test]
    fn lt_opt_adds_condition() {
        let result = QueryBuilder::select(&["id"])
            .from("users")
            .and_where("age")
            .lt_opt(Some(65))
            .build(&PostgresBackend)
            .unwrap();

        assert!(result.sql.contains("< $1"));
        assert_eq!(result.params, vec![Param::I32(65)]);
    }

    /// lte_opt 正常工作。
    #[cfg(feature = "postgresql")]
    #[test]
    fn lte_opt_adds_condition() {
        let result = QueryBuilder::select(&["id"])
            .from("users")
            .and_where("age")
            .lte_opt(Some(65))
            .build(&PostgresBackend)
            .unwrap();

        assert!(result.sql.contains("<= $1"));
        assert_eq!(result.params, vec![Param::I32(65)]);
    }

    // ============================================================
    // Phase 2 — 子查询比较方法
    // ============================================================

    /// eq_subquery 生成 `col = (SELECT ...)`。
    #[cfg(feature = "postgresql")]
    #[test]
    fn eq_subquery_generates_correct_sql() {
        let sub = QueryBuilder::select(&["AVG(salary)"]).from("employees");
        let result = QueryBuilder::select(&["name"])
            .from("users")
            .and_where("salary")
            .eq_subquery(sub)
            .build(&PostgresBackend)
            .unwrap();

        assert!(
            result
                .sql
                .contains("= (SELECT AVG(salary) FROM \"employees\")"),
            "expected subquery equality, got: {}",
            result.sql
        );
    }

    /// gt_subquery 生成 `col > (SELECT ...)`。
    #[cfg(feature = "postgresql")]
    #[test]
    fn gt_subquery_generates_correct_sql() {
        let sub = QueryBuilder::select(&["AVG(salary)"]).from("employees");
        let result = QueryBuilder::select(&["name"])
            .from("users")
            .and_where("salary")
            .gt_subquery(sub)
            .build(&PostgresBackend)
            .unwrap();

        assert!(
            result
                .sql
                .contains("> (SELECT AVG(salary) FROM \"employees\")"),
            "expected subquery gt, got: {}",
            result.sql
        );
    }

    /// neq_subquery 生成 `col <> (SELECT ...)`。
    #[cfg(feature = "postgresql")]
    #[test]
    fn neq_subquery_generates_correct_sql() {
        let sub = QueryBuilder::select(&["1"]).from("banned_users");
        let result = QueryBuilder::select(&["name"])
            .from("users")
            .and_where("id")
            .neq_subquery(sub)
            .build(&PostgresBackend)
            .unwrap();

        assert!(
            result.sql.contains("<> (SELECT 1 FROM \"banned_users\")"),
            "expected subquery neq, got: {}",
            result.sql
        );
    }

    /// 子查询参数索引与外部连续。
    #[cfg(feature = "postgresql")]
    #[test]
    fn subquery_param_continuity() {
        let sub = QueryBuilder::select(&["id"])
            .from("blacklist")
            .and_where("reason")
            .eq("fraud");
        let result = QueryBuilder::select(&["id", "name"])
            .from("users")
            .and_where("status")
            .eq("active")
            .and_where("id")
            .in_subquery(sub)
            .build(&PostgresBackend)
            .unwrap();

        // "active" 是 $1, "fraud" 是 $2（后序遍历：子查询参数先分配）
        assert_eq!(
            result.params,
            vec![
                Param::Text("active".to_string()),
                Param::Text("fraud".to_string())
            ]
        );
        assert!(result.sql.contains("$1"));
        assert!(result.sql.contains("$2"));
    }

    // ============================================================
    // Phase 2 — 函数表达式 WHERE
    // ============================================================

    /// 函数表达式 `UPPER(col)` 不加引号。
    #[cfg(feature = "postgresql")]
    #[test]
    fn where_expr_does_not_quote() {
        let result = QueryBuilder::select(&["id"])
            .from("users")
            .and_where_expr("UPPER(email)")
            .eq("ALICE@EXAMPLE.COM")
            .build(&PostgresBackend)
            .unwrap();

        assert!(
            result.sql.contains("UPPER(email) = $1"),
            "expected unquoted expression, got: {}",
            result.sql
        );
        assert!(
            !result.sql.contains("\"UPPER\""),
            "expression should not be quoted: {}",
            result.sql
        );
    }

    /// 简单列名仍被加引号（回归测试，确保 is_simple_ident 不会过度宽松）。
    #[cfg(feature = "postgresql")]
    #[test]
    fn where_expr_with_simple_ident_still_quotes() {
        let result = QueryBuilder::select(&["id"])
            .from("users")
            .and_where("name")
            .eq("alice")
            .build(&PostgresBackend)
            .unwrap();

        assert!(
            result.sql.contains("\"name\""),
            "simple identifier should be quoted, got: {}",
            result.sql
        );
    }

    /// 函数表达式在 GroupBuilder 中也能工作。
    /// 使用 `or_group` 验证 OR 语义：`or_where_expr` 改为 `and_where_expr`，
    /// 因为组内条件连接符由组的 logic 决定（`and_group` = 全部 AND，`or_group` = 全部 OR）。
    #[cfg(feature = "postgresql")]
    #[test]
    fn and_where_expr_with_group() {
        let result = QueryBuilder::select(&["id"])
            .from("users")
            .or_group(|g| {
                g.and_where_expr("LOWER(name)")
                    .eq("alice")
                    .and_where_expr("UPPER(email)")
                    .eq("ALICE@EXAMPLE.COM")
            })
            .build(&PostgresBackend)
            .unwrap();

        assert!(
            result.sql.contains("LOWER(name) = $1"),
            "expected LOWER in group, got: {}",
            result.sql
        );
        assert!(
            result.sql.contains("UPPER(email) = $2"),
            "expected UPPER in group, got: {}",
            result.sql
        );
        assert!(
            result.sql.contains(" OR "),
            "expected OR in group, got: {}",
            result.sql
        );
    }

    // ============================================================
    // Phase 3 — SelectIdent
    // ============================================================

    /// 简单列名被 `quote_ident` 包裹。
    #[cfg(feature = "postgresql")]
    #[test]
    fn select_ident_quotes_simple_column() {
        let result = QueryBuilder::default()
            .select_ident(&["id", "name"])
            .from("users")
            .build(&PostgresBackend)
            .unwrap();

        assert!(
            result.sql.contains("\"id\""),
            "expected quoted id, got: {}",
            result.sql
        );
        assert!(
            result.sql.contains("\"name\""),
            "expected quoted name, got: {}",
            result.sql
        );
    }

    /// 含 `.` 或 `()` 的复杂表达式不加引号。
    #[cfg(feature = "postgresql")]
    #[test]
    fn select_ident_does_not_quote_complex() {
        let result = QueryBuilder::default()
            .select_ident(&["users.name", "COUNT(*)"])
            .from("users")
            .build(&PostgresBackend)
            .unwrap();

        assert!(
            result.sql.contains("users.name"),
            "expected unquoted dotted column, got: {}",
            result.sql
        );
        assert!(
            result.sql.contains("COUNT(*)"),
            "expected unquoted function call, got: {}",
            result.sql
        );
        assert!(
            !result.sql.contains("\"users.name\""),
            "dotted column should not be quoted: {}",
            result.sql
        );
    }

    /// 原有 `select()` 行为不受影响（回归测试）。
    #[cfg(feature = "postgresql")]
    #[test]
    fn select_ident_mixed_with_select_unchanged() {
        let result = QueryBuilder::select(&["id", "name"])
            .from("users")
            .build(&PostgresBackend)
            .unwrap();

        // 原始 select() 不加引号（保持向后兼容）
        assert!(
            result.sql.contains("SELECT id, name"),
            "select() should output raw columns, got: {}",
            result.sql
        );
    }

    /// 空列名不 panic，原样输出。
    #[cfg(feature = "postgresql")]
    #[test]
    fn select_ident_empty_column() {
        let result = QueryBuilder::default()
            .select_ident(&["", "name"])
            .from("users")
            .build(&PostgresBackend)
            .unwrap();

        // 空字符串 is_simple_ident 返回 false，原样输出
        assert!(
            result.sql.contains("\"name\""),
            "expected quoted name, got: {}",
            result.sql
        );
    }

    /// 带 AS 别名的列名因含空格整体原样输出不引用。
    #[cfg(feature = "postgresql")]
    #[test]
    fn select_ident_with_as_alias_raw() {
        let result = QueryBuilder::default()
            .select_ident(&["name AS user_name"])
            .from("users")
            .build(&PostgresBackend)
            .unwrap();

        assert!(
            result.sql.contains("name AS user_name"),
            "expected raw 'AS' expression, got: {}",
            result.sql
        );
        assert!(
            !result.sql.contains("\"name AS user_name\""),
            "AS alias should not be quoted as a whole: {}",
            result.sql
        );
    }

    // ============================================================
    // Phase 3 — GroupByIdent
    // ============================================================

    /// GROUP BY 列被引用。
    #[cfg(feature = "postgresql")]
    #[test]
    fn group_by_ident_quotes_columns() {
        let result = QueryBuilder::select(&["dept"])
            .from("users")
            .group_by_ident(&["dept", "role"])
            .build(&PostgresBackend)
            .unwrap();

        assert!(
            result.sql.contains("GROUP BY \"dept\", \"role\""),
            "expected quoted GROUP BY, got: {}",
            result.sql
        );
    }

    /// 函数表达式在 GROUP BY 中不加引号。
    #[cfg(feature = "postgresql")]
    #[test]
    fn group_by_ident_does_not_quote_expression() {
        let result = QueryBuilder::select(&["dept"])
            .from("users")
            .group_by_ident(&["dept", "DATE(created_at)"])
            .build(&PostgresBackend)
            .unwrap();

        assert!(
            result.sql.contains("\"dept\""),
            "expected quoted dept, got: {}",
            result.sql
        );
        assert!(
            result.sql.contains("DATE(created_at)"),
            "expected unquoted function expr, got: {}",
            result.sql
        );
        assert!(
            !result.sql.contains("\"DATE(created_at)\""),
            "function expr should not be quoted: {}",
            result.sql
        );
    }

    /// 原有 `group_by()` 行为不变（回归测试）。
    #[cfg(feature = "postgresql")]
    #[test]
    fn group_by_legacy_unchanged() {
        let result = QueryBuilder::select(&["dept"])
            .from("users")
            .group_by(&["dept", "role"])
            .build(&PostgresBackend)
            .unwrap();

        // 原始 group_by() 不加引号
        assert!(
            result.sql.contains("GROUP BY dept, role"),
            "legacy group_by() should output raw, got: {}",
            result.sql
        );
    }

    /// 链式 `group_by_ident().group_by()` 后 flag 被重置。
    #[cfg(feature = "postgresql")]
    #[test]
    fn group_by_ident_then_group_by_resets_flag() {
        let result = QueryBuilder::select(&["dept"])
            .from("users")
            .group_by_ident(&["dept"]) // 先用 ident，flag=true
            .group_by(&["role"]) // 再用普通 group_by，flag=false
            .build(&PostgresBackend)
            .unwrap();

        // 应使用 group_by() 的行为：列名不加引号
        assert!(
            result.sql.contains("GROUP BY role"),
            "group_by() after group_by_ident() should reset flag, got: {}",
            result.sql
        );
        assert!(
            !result.sql.contains("\"role\""),
            "group_by() output should not be quoted, got: {}",
            result.sql
        );
    }

    // ============================================================
    // Phase 3 — OrderBySafe
    // ============================================================

    /// 白名单内列名正常排序。
    #[cfg(feature = "postgresql")]
    #[test]
    fn order_by_safe_allows_whitelisted() {
        let result = QueryBuilder::select(&["id"])
            .from("users")
            .order_by_safe("name", SortDir::Asc, &["name", "id", "email"])
            .unwrap()
            .build(&PostgresBackend)
            .unwrap();

        assert!(
            result.sql.contains("ORDER BY \"name\" ASC"),
            "expected safe order by, got: {}",
            result.sql
        );
    }

    /// 白名单外列名返回 UnsafeColumn 错误。
    #[cfg(feature = "postgresql")]
    #[test]
    fn order_by_safe_rejects_unlisted() {
        let result = QueryBuilder::select(&["id"]).from("users").order_by_safe(
            "password",
            SortDir::Asc,
            &["name", "id", "email"],
        );

        match result {
            Err(BuildError::UnsafeColumn(col)) => {
                assert_eq!(col, "password", "unexpected column in error: {col}");
            }
            other => panic!("expected UnsafeColumn, got: {other:?}"),
        }
    }

    /// 点号列名 `users.name` 白名单匹配工作。
    #[cfg(feature = "postgresql")]
    #[test]
    fn order_by_safe_dotted_column() {
        let result = QueryBuilder::select(&["id"])
            .from("users")
            .order_by_safe("users.name", SortDir::Desc, &["users.name", "id"])
            .unwrap()
            .build(&PostgresBackend)
            .unwrap();

        assert!(
            result.sql.contains("ORDER BY \"users.name\" DESC"),
            "expected dotted column sort, got: {}",
            result.sql
        );
    }

    // ============================================================
    // Phase 3 — Backend::supports_upsert
    // ============================================================

    /// MSSQL `supports_upsert` 返回 false。
    #[cfg(feature = "mssql")]
    #[test]
    fn supports_upsert_mssql_false() {
        use crate::backends::mssql::MssqlBackend;
        assert!(!MssqlBackend.supports_upsert());
    }

    /// Postgres `supports_upsert` 返回 true。
    #[cfg(feature = "postgresql")]
    #[test]
    fn supports_upsert_pg_true() {
        assert!(PostgresBackend.supports_upsert());
    }

    // ============================================================
    // Phase 3 — MySQL/MariaDB FULL JOIN 拒绝
    // ============================================================

    /// MySQL 拒绝 FULL JOIN。
    #[cfg(feature = "mysql")]
    #[test]
    fn mysql_rejects_full_join() {
        use crate::backends::mysql::MysqlBackend;

        let result = QueryBuilder::select(&["a.id"])
            .from("a")
            .full_join("b", "b")
            .on("a.id", "b.a_id")
            .build(&MysqlBackend);

        match result {
            Err(BuildError::UnsupportedJoinType(t)) => {
                assert!(t.contains("FULL"), "unexpected join type: {t}");
            }
            other => panic!("expected UnsupportedJoinType, got: {other:?}"),
        }
    }

    /// MariaDB 拒绝 FULL JOIN。
    #[cfg(feature = "mariadb")]
    #[test]
    fn mariadb_rejects_full_join() {
        use crate::backends::mariadb::MariaDbBackend;

        let result = QueryBuilder::select(&["a.id"])
            .from("a")
            .full_join("b", "b")
            .on("a.id", "b.a_id")
            .build(&MariaDbBackend);

        assert!(matches!(result, Err(BuildError::UnsupportedJoinType(_))));
    }

    /// MySQL 接受 INNER JOIN（正例）。
    #[cfg(feature = "mysql")]
    #[test]
    fn mysql_accepts_inner_join() {
        use crate::backends::mysql::MysqlBackend;

        let result = QueryBuilder::select(&["a.id"])
            .from("a")
            .join("b", "b")
            .on("a.id", "b.a_id")
            .build(&MysqlBackend)
            .unwrap();

        assert!(result.sql.contains("INNER JOIN"));
    }

    // ============================================================
    // Phase 4 — 无 WHERE UPDATE/DELETE 安全校验
    // ============================================================

    /// 验证 UPDATE 无 WHERE 返回 UnsafeUpdateWithoutWhere。
    #[cfg(feature = "postgresql")]
    #[test]
    fn update_without_where_returns_error() {
        let result = QueryBuilder::update("users")
            .update_set("name", "bob")
            .build(&PostgresBackend);

        assert!(matches!(result, Err(BuildError::UnsafeUpdateWithoutWhere)));
    }

    /// 验证 UPDATE 无 WHERE + allow_unsafe_update 放行。
    #[cfg(feature = "postgresql")]
    #[test]
    fn update_without_where_with_allow_unsafe_succeeds() {
        let result = QueryBuilder::update("users")
            .update_set("name", "bob")
            .allow_unsafe_update()
            .build(&PostgresBackend);

        let qr = result.expect("allow_unsafe_update should bypass safety check");
        assert!(qr.sql.contains("UPDATE"));
        assert!(!qr.sql.contains("WHERE"));
    }

    /// 验证 DELETE 无 WHERE 返回 UnsafeDeleteWithoutWhere。
    #[cfg(feature = "postgresql")]
    #[test]
    fn delete_without_where_returns_error() {
        let result = QueryBuilder::delete_from("users").build(&PostgresBackend);

        assert!(matches!(result, Err(BuildError::UnsafeDeleteWithoutWhere)));
    }

    /// 验证 DELETE 无 WHERE + allow_unsafe_delete 放行。
    #[cfg(feature = "postgresql")]
    #[test]
    fn delete_without_where_with_allow_unsafe_succeeds() {
        let result = QueryBuilder::delete_from("users")
            .allow_unsafe_delete()
            .build(&PostgresBackend);

        let qr = result.expect("allow_unsafe_delete should bypass safety check");
        assert!(qr.sql.contains("DELETE"));
        assert!(!qr.sql.contains("WHERE"));
    }

    /// 验证 UPDATE 有 WHERE 正常（回归测试）。
    #[cfg(feature = "postgresql")]
    #[test]
    fn update_with_where_succeeds() {
        let result = QueryBuilder::update("users")
            .update_set("name", "bob")
            .and_where("id")
            .eq(1)
            .build(&PostgresBackend);

        assert!(result.is_ok());
        assert!(result.unwrap().sql.contains("WHERE"));
    }

    /// 验证 DELETE 有 WHERE 正常（回归测试）。
    #[cfg(feature = "postgresql")]
    #[test]
    fn delete_with_where_succeeds() {
        let result = QueryBuilder::delete_from("users")
            .and_where("id")
            .eq(1)
            .build(&PostgresBackend);

        assert!(result.is_ok());
        assert!(result.unwrap().sql.contains("WHERE"));
    }

    // ============================================================
    // Phase 4 — CTE + UPDATE/DELETE 场景
    // ============================================================

    /// 验证 CTE + 无 WHERE UPDATE + allow_unsafe_update 放行。
    #[cfg(feature = "postgresql")]
    #[test]
    fn cte_update_without_where_with_allow_unsafe_succeeds() {
        let cte = QueryBuilder::select(&["id"]).from("archived_users");
        let result = QueryBuilder::update("users")
            .with_cte("archived", cte)
            .update_set("status", "archived")
            .allow_unsafe_update()
            .build(&PostgresBackend);

        assert!(result.is_ok());
        assert!(result.unwrap().sql.contains("WITH"));
    }

    /// 验证 CTE + 无 WHERE DELETE + allow_unsafe_delete 放行。
    #[cfg(feature = "postgresql")]
    #[test]
    fn cte_delete_without_where_with_allow_unsafe_succeeds() {
        let cte = QueryBuilder::select(&["id"]).from("expired_sessions");
        let result = QueryBuilder::delete_from("sessions")
            .with_cte("expired", cte)
            .allow_unsafe_delete()
            .build(&PostgresBackend);

        assert!(result.is_ok());
        assert!(result.unwrap().sql.contains("WITH"));
    }

    /// 验证 CTE + 有 WHERE UPDATE 正常（不触发安全校验）。
    #[cfg(feature = "postgresql")]
    #[test]
    fn cte_update_with_where_succeeds() {
        let cte = QueryBuilder::select(&["id"])
            .from("temp_users")
            .and_where("batch")
            .eq(1);
        let inner = QueryBuilder::select(&["id"]).from_cte_ref("tmp");
        let result = QueryBuilder::update("users")
            .with_cte("tmp", cte)
            .update_set("status", "active")
            .and_where("id")
            .in_subquery(inner)
            .build(&PostgresBackend);

        let qr = result.expect("with_cte + WHERE should not trigger safety check");
        assert!(qr.sql.contains("WITH"));
        assert!(qr.sql.contains("WHERE"));
    }

    // ============================================================
    // Phase 4 — MSSQL ORDER BY 校验
    // ============================================================

    /// 验证 MSSQL OFFSET 无 ORDER BY 返回 OrderByRequired。
    #[cfg(feature = "mssql")]
    #[test]
    fn mssql_offset_without_order_by_returns_error() {
        use crate::backends::mssql::MssqlBackend;

        let result = QueryBuilder::select(&["id"])
            .from("users")
            .limit(10)
            .build(&MssqlBackend);

        assert!(matches!(result, Err(BuildError::OrderByRequired)));
    }

    /// 验证 MSSQL OFFSET 有 ORDER BY 正常。
    #[cfg(feature = "mssql")]
    #[test]
    fn mssql_offset_with_order_by_succeeds() {
        use crate::backends::mssql::MssqlBackend;

        let result = QueryBuilder::select(&["id"])
            .from("users")
            .order_by("id", SortDir::Asc)
            .limit(10)
            .build(&MssqlBackend);

        assert!(result.is_ok());
    }

    /// 验证 MSSQL 无 LIMIT/OFFSET 时不需要 ORDER BY。
    #[cfg(feature = "mssql")]
    #[test]
    fn mssql_without_offset_does_not_require_order_by() {
        use crate::backends::mssql::MssqlBackend;

        let result = QueryBuilder::select(&["id"])
            .from("users")
            .build(&MssqlBackend);

        assert!(result.is_ok());
    }

    /// 验证 Postgres OFFSET 无需 ORDER BY（回归测试）。
    #[cfg(feature = "postgresql")]
    #[test]
    fn postgres_offset_without_order_by_succeeds() {
        let result = QueryBuilder::select(&["id"])
            .from("users")
            .limit(10)
            .offset(5)
            .build(&PostgresBackend);

        assert!(result.is_ok());
        assert!(result.unwrap().sql.contains("LIMIT"));
    }
}
