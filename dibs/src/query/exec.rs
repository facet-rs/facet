//! Query execution against Postgres.

use super::{
    BuiltQuery, DeleteQuery, InsertQuery, Row, SelectQuery, SqlParam, UpdateQuery, Value,
    pg_row_to_row,
};
use crate::Error;
use crate::schema::{Schema, Table};
use tokio_postgres::Client;

/// A database connection that can execute queries.
///
/// Wraps a tokio_postgres Client and provides schema-aware query execution.
pub struct Db<'a> {
    client: &'a Client,
    schema: Schema,
}

impl<'a> Db<'a> {
    /// Create a new Db from a client.
    ///
    /// Collects the schema from registered tables.
    pub fn new(client: &'a Client) -> Self {
        Self {
            client,
            schema: Schema::collect(),
        }
    }

    /// Get the schema.
    pub fn schema(&self) -> &Schema {
        &self.schema
    }

    /// Look up a table by name.
    pub fn table(&self, name: &str) -> Option<&Table> {
        self.schema.tables.iter().find(|t| t.name == name)
    }

    /// Start building a SELECT query for a table.
    pub fn select(&self, table: &str) -> Result<SelectBuilder<'_>, Error> {
        let table_def = self
            .table(table)
            .ok_or_else(|| Error::UnknownTable(table.to_string()))?;
        Ok(SelectBuilder {
            db: self,
            table: table_def,
            query: SelectQuery::new(table),
        })
    }

    /// Start building an INSERT query for a table.
    pub fn insert(&self, table: &str) -> Result<InsertBuilder<'_>, Error> {
        let table_def = self
            .table(table)
            .ok_or_else(|| Error::UnknownTable(table.to_string()))?;
        Ok(InsertBuilder {
            db: self,
            table: table_def,
            query: InsertQuery::new(table),
        })
    }

    /// Start building an UPDATE query for a table.
    pub fn update(&self, table: &str) -> Result<UpdateBuilder<'_>, Error> {
        let table_def = self
            .table(table)
            .ok_or_else(|| Error::UnknownTable(table.to_string()))?;
        Ok(UpdateBuilder {
            db: self,
            table: table_def,
            query: UpdateQuery::new(table),
        })
    }

    /// Start building a DELETE query for a table.
    pub fn delete(&self, table: &str) -> Result<DeleteBuilder<'_>, Error> {
        let table_def = self
            .table(table)
            .ok_or_else(|| Error::UnknownTable(table.to_string()))?;
        Ok(DeleteBuilder {
            db: self,
            table: table_def,
            query: DeleteQuery::new(table),
        })
    }

    /// Execute a built query and return rows.
    async fn execute_select(&self, query: BuiltQuery, table: &Table) -> Result<Vec<Row>, Error> {
        let params: Vec<SqlParam> = query.params.iter().map(SqlParam).collect();
        let params_ref: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = params
            .iter()
            .map(|p| p as &(dyn tokio_postgres::types::ToSql + Sync))
            .collect();

        let rows = self.client.query(&query.sql, &params_ref).await?;

        let columns: Vec<_> = table
            .columns
            .iter()
            .map(|c| (c.name.clone(), c.pg_type))
            .collect();

        rows.iter()
            .map(|row| pg_row_to_row(row, &columns))
            .collect()
    }

    /// Execute a mutation query (INSERT/UPDATE/DELETE) and return affected count.
    async fn execute_mutation(&self, query: BuiltQuery) -> Result<u64, Error> {
        let params: Vec<SqlParam> = query.params.iter().map(SqlParam).collect();
        let params_ref: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = params
            .iter()
            .map(|p| p as &(dyn tokio_postgres::types::ToSql + Sync))
            .collect();

        let affected = self.client.execute(&query.sql, &params_ref).await?;
        Ok(affected)
    }

    /// Execute a mutation with RETURNING and return the row.
    async fn execute_returning(
        &self,
        query: BuiltQuery,
        table: &Table,
    ) -> Result<Option<Row>, Error> {
        let params: Vec<SqlParam> = query.params.iter().map(SqlParam).collect();
        let params_ref: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = params
            .iter()
            .map(|p| p as &(dyn tokio_postgres::types::ToSql + Sync))
            .collect();

        let rows = self.client.query(&query.sql, &params_ref).await?;

        if rows.is_empty() {
            return Ok(None);
        }

        let columns: Vec<_> = table
            .columns
            .iter()
            .map(|c| (c.name.clone(), c.pg_type))
            .collect();

        Ok(Some(pg_row_to_row(&rows[0], &columns)?))
    }
}

/// Builder for SELECT queries.
pub struct SelectBuilder<'a> {
    db: &'a Db<'a>,
    table: &'a Table,
    query: SelectQuery,
}

impl<'a> SelectBuilder<'a> {
    /// Select specific columns.
    pub fn columns(mut self, cols: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.query = self.query.columns(cols);
        self
    }

    /// Add a filter. The column name is validated against the schema.
    pub fn filter(mut self, expr: super::Expr) -> Self {
        // TODO: validate column names in expr against schema
        self.query = self.query.filter(expr);
        self
    }

    /// Add ORDER BY.
    pub fn order_by(mut self, column: impl Into<String>, dir: super::SortDir) -> Self {
        self.query = self.query.order_by(column, dir);
        self
    }

    /// Set LIMIT.
    pub fn limit(mut self, n: u32) -> Self {
        self.query = self.query.limit(n);
        self
    }

    /// Set OFFSET.
    pub fn offset(mut self, n: u32) -> Self {
        self.query = self.query.offset(n);
        self
    }

    /// Execute and return all matching rows.
    pub async fn all(self) -> Result<Vec<Row>, Error> {
        let built = self.query.build();
        self.db.execute_select(built, self.table).await
    }

    /// Execute and return the first matching row.
    pub async fn one(self) -> Result<Option<Row>, Error> {
        let mut rows = self.limit(1).all().await?;
        Ok(rows.pop())
    }

    /// Execute and return the count of matching rows.
    pub async fn count(self) -> Result<u64, Error> {
        // Build a COUNT(*) query instead
        let mut query = self.query;
        query.columns = vec!["COUNT(*)".to_string()];
        let built = query.build();

        let params: Vec<SqlParam> = built.params.iter().map(SqlParam).collect();
        let params_ref: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = params
            .iter()
            .map(|p| p as &(dyn tokio_postgres::types::ToSql + Sync))
            .collect();

        let rows = self.db.client.query(&built.sql, &params_ref).await?;
        let count: i64 = rows[0].get(0);
        Ok(count as u64)
    }
}

/// Builder for INSERT queries.
pub struct InsertBuilder<'a> {
    db: &'a Db<'a>,
    table: &'a Table,
    query: InsertQuery,
}

impl<'a> InsertBuilder<'a> {
    /// Set the values to insert.
    pub fn values(
        mut self,
        data: impl IntoIterator<Item = (impl Into<String>, impl Into<Value>)>,
    ) -> Self {
        self.query = self.query.values(data);
        self
    }

    /// Execute the insert, returning the number of rows affected.
    pub async fn execute(self) -> Result<u64, Error> {
        let built = self.query.build();
        self.db.execute_mutation(built).await
    }

    /// Execute the insert with RETURNING *, returning the inserted row.
    pub async fn returning(mut self) -> Result<Option<Row>, Error> {
        self.query = self.query.returning_all();
        let built = self.query.build();
        self.db.execute_returning(built, self.table).await
    }
}

/// Builder for UPDATE queries.
pub struct UpdateBuilder<'a> {
    db: &'a Db<'a>,
    table: &'a Table,
    query: UpdateQuery,
}

impl<'a> UpdateBuilder<'a> {
    /// Set the columns and values to update.
    pub fn set(
        mut self,
        data: impl IntoIterator<Item = (impl Into<String>, impl Into<Value>)>,
    ) -> Self {
        self.query = self.query.set(data);
        self
    }

    /// Add a filter condition.
    pub fn filter(mut self, expr: super::Expr) -> Self {
        self.query = self.query.filter(expr);
        self
    }

    /// Execute the update, returning the number of rows affected.
    pub async fn execute(self) -> Result<u64, Error> {
        let built = self.query.build();
        self.db.execute_mutation(built).await
    }

    /// Execute the update with RETURNING *, returning the first updated row.
    pub async fn returning(mut self) -> Result<Option<Row>, Error> {
        self.query = self.query.returning_all();
        let built = self.query.build();
        self.db.execute_returning(built, self.table).await
    }
}

/// Builder for DELETE queries.
pub struct DeleteBuilder<'a> {
    db: &'a Db<'a>,
    table: &'a Table,
    query: DeleteQuery,
}

impl<'a> DeleteBuilder<'a> {
    /// Add a filter condition.
    pub fn filter(mut self, expr: super::Expr) -> Self {
        self.query = self.query.filter(expr);
        self
    }

    /// Execute the delete, returning the number of rows affected.
    pub async fn execute(self) -> Result<u64, Error> {
        let built = self.query.build();
        self.db.execute_mutation(built).await
    }

    /// Execute the delete with RETURNING *, returning the first deleted row.
    pub async fn returning(mut self) -> Result<Option<Row>, Error> {
        self.query = self.query.returning_all();
        let built = self.query.build();
        self.db.execute_returning(built, self.table).await
    }
}
