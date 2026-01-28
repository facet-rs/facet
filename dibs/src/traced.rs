//! Traced database connection wrapper.
//!
//! Wraps a tokio-postgres connection/pool and logs all queries via tracing.

use tokio_postgres::types::ToSql;
use tokio_postgres::{Error, Row};
use tracing::Instrument;

/// A traced connection pool.
///
/// Wraps a `deadpool_postgres::Pool` and returns `TracedObject` from `get()`,
/// ensuring all queries are automatically logged.
///
/// # Example
///
/// ```ignore
/// use dibs::TracedPool;
///
/// let pool = TracedPool::new(pool);
/// let conn = pool.get().await?;
///
/// // All queries are automatically traced
/// conn.execute("INSERT INTO user (email) VALUES ($1)", &[&email]).await?;
/// ```
#[derive(Clone)]
pub struct TracedPool {
    inner: deadpool_postgres::Pool,
}

impl TracedPool {
    /// Create a new traced pool wrapper.
    pub fn new(pool: deadpool_postgres::Pool) -> Self {
        Self { inner: pool }
    }

    /// Get a traced connection from the pool.
    pub async fn get(&self) -> Result<TracedObject, deadpool_postgres::PoolError> {
        let conn = self.inner.get().await?;
        Ok(TracedObject { inner: conn })
    }

    /// Get the inner pool (for cases where you need the raw pool).
    pub fn inner(&self) -> &deadpool_postgres::Pool {
        &self.inner
    }
}

/// A traced connection that owns the underlying connection.
///
/// All queries executed through this wrapper are logged via tracing.
pub struct TracedObject {
    inner: deadpool_postgres::Object,
}

impl TracedObject {
    /// Execute a statement, returning the number of rows affected.
    pub async fn execute(&self, sql: &str, params: &[&(dyn ToSql + Sync)]) -> Result<u64, Error> {
        let span = tracing::debug_span!(
            "db.execute",
            sql = %sql,
            params = params.len(),
            affected = tracing::field::Empty,
        );
        use std::ops::Deref;
        let client: &tokio_postgres::Client = self.inner.deref();
        let affected = client.execute(sql, params).instrument(span.clone()).await?;
        span.record("affected", affected);
        Ok(affected)
    }

    /// Execute a query, returning all rows.
    pub async fn query(
        &self,
        sql: &str,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Vec<Row>, Error> {
        let span = tracing::debug_span!(
            "db.query",
            sql = %sql,
            params = params.len(),
            rows = tracing::field::Empty,
        );
        use std::ops::Deref;
        let client: &tokio_postgres::Client = self.inner.deref();
        let rows = client.query(sql, params).instrument(span.clone()).await?;
        span.record("rows", rows.len());
        Ok(rows)
    }

    /// Execute a query, returning at most one row.
    pub async fn query_opt(
        &self,
        sql: &str,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Option<Row>, Error> {
        let span = tracing::debug_span!(
            "db.query",
            sql = %sql,
            params = params.len(),
            rows = tracing::field::Empty,
        );
        use std::ops::Deref;
        let client: &tokio_postgres::Client = self.inner.deref();
        let row = client
            .query_opt(sql, params)
            .instrument(span.clone())
            .await?;
        span.record("rows", if row.is_some() { 1u64 } else { 0u64 });
        Ok(row)
    }

    /// Execute a query, returning exactly one row.
    pub async fn query_one(&self, sql: &str, params: &[&(dyn ToSql + Sync)]) -> Result<Row, Error> {
        let span = tracing::debug_span!(
            "db.query",
            sql = %sql,
            params = params.len(),
            rows = 1u64,
        );
        use std::ops::Deref;
        let client: &tokio_postgres::Client = self.inner.deref();
        client.query_one(sql, params).instrument(span).await
    }

    /// Get the inner connection (for cases where you need the raw connection).
    pub fn inner(&self) -> &deadpool_postgres::Object {
        &self.inner
    }
}

/// A wrapper around a database connection that logs all queries via tracing.
///
/// This is a thin wrapper that delegates to the underlying connection but adds
/// `tracing::debug_span!` around each query/execute call.
///
/// # Example
///
/// ```ignore
/// use dibs::TracedConn;
///
/// let conn = pool.get().await?;
/// let traced = TracedConn::new(&conn);
///
/// // All queries are now logged at debug level
/// traced.execute("INSERT INTO user (email) VALUES ($1)", &[&email]).await?;
/// let rows = traced.query("SELECT * FROM user WHERE id = $1", &[&id]).await?;
/// ```
pub struct TracedConn<'a, C: Connection> {
    conn: &'a C,
}

impl<'a, C: Connection> TracedConn<'a, C> {
    /// Create a new traced connection wrapper.
    pub fn new(conn: &'a C) -> Self {
        Self { conn }
    }

    /// Execute a statement, returning the number of rows affected.
    pub async fn execute(&self, sql: &str, params: &[&(dyn ToSql + Sync)]) -> Result<u64, Error> {
        let span = tracing::debug_span!(
            "db.execute",
            sql = %sql,
            params = params.len(),
            affected = tracing::field::Empty,
        );
        let affected = self
            .conn
            .execute(sql, params)
            .instrument(span.clone())
            .await?;
        span.record("affected", affected);
        Ok(affected)
    }

    /// Execute a query, returning all rows.
    pub async fn query(
        &self,
        sql: &str,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Vec<Row>, Error> {
        let span = tracing::debug_span!(
            "db.query",
            sql = %sql,
            params = params.len(),
            rows = tracing::field::Empty,
        );
        let rows = self
            .conn
            .query(sql, params)
            .instrument(span.clone())
            .await?;
        span.record("rows", rows.len());
        Ok(rows)
    }

    /// Execute a query, returning at most one row.
    pub async fn query_opt(
        &self,
        sql: &str,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Option<Row>, Error> {
        let span = tracing::debug_span!(
            "db.query",
            sql = %sql,
            params = params.len(),
            rows = tracing::field::Empty,
        );
        let row = self
            .conn
            .query_opt(sql, params)
            .instrument(span.clone())
            .await?;
        span.record("rows", if row.is_some() { 1u64 } else { 0u64 });
        Ok(row)
    }

    /// Execute a query, returning exactly one row.
    ///
    /// Returns an error if the query returns zero or more than one row.
    pub async fn query_one(&self, sql: &str, params: &[&(dyn ToSql + Sync)]) -> Result<Row, Error> {
        let span = tracing::debug_span!(
            "db.query",
            sql = %sql,
            params = params.len(),
            rows = 1u64,
        );
        self.conn.query_one(sql, params).instrument(span).await
    }
}

/// Extension trait to get a traced wrapper from a connection.
pub trait ConnectionExt: Connection + Sized {
    /// Wrap this connection in a `TracedConn` for query logging.
    fn traced(&self) -> TracedConn<'_, Self> {
        TracedConn::new(self)
    }
}

impl<C: Connection> ConnectionExt for C {}

/// Trait for database connections that can execute queries.
///
/// This is implemented for `tokio_postgres::Client` and `deadpool_postgres::Object`.
pub trait Connection: Send + Sync {
    /// Execute a statement, returning the number of rows affected.
    fn execute<'a>(
        &'a self,
        sql: &'a str,
        params: &'a [&'a (dyn ToSql + Sync)],
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<u64, Error>> + Send + 'a>>;

    /// Execute a query, returning all rows.
    fn query<'a>(
        &'a self,
        sql: &'a str,
        params: &'a [&'a (dyn ToSql + Sync)],
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<Row>, Error>> + Send + 'a>>;

    /// Execute a query, returning at most one row.
    fn query_opt<'a>(
        &'a self,
        sql: &'a str,
        params: &'a [&'a (dyn ToSql + Sync)],
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Option<Row>, Error>> + Send + 'a>>;

    /// Execute a query, returning exactly one row.
    fn query_one<'a>(
        &'a self,
        sql: &'a str,
        params: &'a [&'a (dyn ToSql + Sync)],
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Row, Error>> + Send + 'a>>;
}

impl Connection for tokio_postgres::Client {
    fn execute<'a>(
        &'a self,
        sql: &'a str,
        params: &'a [&'a (dyn ToSql + Sync)],
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<u64, Error>> + Send + 'a>> {
        Box::pin(tokio_postgres::Client::execute(self, sql, params))
    }

    fn query<'a>(
        &'a self,
        sql: &'a str,
        params: &'a [&'a (dyn ToSql + Sync)],
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<Row>, Error>> + Send + 'a>>
    {
        Box::pin(tokio_postgres::Client::query(self, sql, params))
    }

    fn query_opt<'a>(
        &'a self,
        sql: &'a str,
        params: &'a [&'a (dyn ToSql + Sync)],
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Option<Row>, Error>> + Send + 'a>>
    {
        Box::pin(tokio_postgres::Client::query_opt(self, sql, params))
    }

    fn query_one<'a>(
        &'a self,
        sql: &'a str,
        params: &'a [&'a (dyn ToSql + Sync)],
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Row, Error>> + Send + 'a>> {
        Box::pin(tokio_postgres::Client::query_one(self, sql, params))
    }
}

impl Connection for deadpool_postgres::Object {
    fn execute<'a>(
        &'a self,
        sql: &'a str,
        params: &'a [&'a (dyn ToSql + Sync)],
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<u64, Error>> + Send + 'a>> {
        // Deref to the underlying Client to avoid recursion
        use std::ops::Deref;
        let client: &tokio_postgres::Client = self.deref();
        Box::pin(client.execute(sql, params))
    }

    fn query<'a>(
        &'a self,
        sql: &'a str,
        params: &'a [&'a (dyn ToSql + Sync)],
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<Row>, Error>> + Send + 'a>>
    {
        use std::ops::Deref;
        let client: &tokio_postgres::Client = self.deref();
        Box::pin(client.query(sql, params))
    }

    fn query_opt<'a>(
        &'a self,
        sql: &'a str,
        params: &'a [&'a (dyn ToSql + Sync)],
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Option<Row>, Error>> + Send + 'a>>
    {
        use std::ops::Deref;
        let client: &tokio_postgres::Client = self.deref();
        Box::pin(client.query_opt(sql, params))
    }

    fn query_one<'a>(
        &'a self,
        sql: &'a str,
        params: &'a [&'a (dyn ToSql + Sync)],
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Row, Error>> + Send + 'a>> {
        use std::ops::Deref;
        let client: &tokio_postgres::Client = self.deref();
        Box::pin(client.query_one(sql, params))
    }
}
