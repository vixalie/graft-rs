use crate::param::Param;

/// 执行器 trait 使 builder 不局限于 SQL 生成，可提供端到端执行接口。
#[async_trait::async_trait]
pub trait Executor {
    /// 执行不返回行的 SQL（INSERT/UPDATE/DELETE）。
    async fn execute(&mut self, sql: &str, params: &[Param]) -> Result<u64, crate::result::BuildError>;

    /// 查询单行。
    async fn query_row(
        &mut self,
        sql: &str,
        params: &[Param],
    ) -> Result<Vec<Param>, crate::result::BuildError>;

    /// 查询多行。
    async fn query(
        &mut self,
        sql: &str,
        params: &[Param],
    ) -> Result<Vec<Vec<Param>>, crate::result::BuildError>;
}
