//! Connection pooling abstractions.
//!
//! This module provides the [`ConnectionProvider`] trait which abstracts over
//! different ways to obtain a database connection:
//!
//! - `Arc<tokio_postgres::Client>` - a single shared connection
//! - `deadpool_postgres::Pool` - a connection pool (requires `deadpool` feature)
//!
//! This allows services like [`SquelServiceImpl`](crate::SquelServiceImpl) to
//! work with either a single connection or a pool.

use std::future::Future;
use std::ops::Deref;
use std::sync::Arc;

use tokio_postgres::Client;

use crate::Error;

/// A source of database connections.
///
/// Implementations provide a way to obtain a connection that can be used
/// for database operations. The connection is returned as a guard type
/// that derefs to [`tokio_postgres::Client`].
///
/// # Example
///
/// ```ignore
/// async fn do_query<P: ConnectionProvider>(provider: &P) -> Result<(), Error> {
///     let conn = provider.get().await?;
///     conn.execute("SELECT 1", &[]).await?;
///     Ok(())
/// }
/// ```
pub trait ConnectionProvider: Clone + Send + Sync + 'static {
    /// The guard type that holds the connection.
    ///
    /// This type must deref to `Client` and will release the connection
    /// back to the pool (if applicable) when dropped.
    type Guard<'a>: Deref<Target = Client> + Send
    where
        Self: 'a;

    /// Obtain a connection from this provider.
    ///
    /// For a single connection, this returns immediately.
    /// For a pool, this may wait for a connection to become available.
    fn get(&self) -> impl Future<Output = Result<Self::Guard<'_>, Error>> + Send;
}

/// Implementation for a single shared connection.
///
/// This is useful for simple cases where you don't need pooling,
/// such as CLI tools or tests.
impl ConnectionProvider for Arc<Client> {
    type Guard<'a> = Arc<Client>;

    async fn get(&self) -> Result<Self::Guard<'_>, Error> {
        Ok(self.clone())
    }
}

/// Wrapper around a deadpool pooled connection that provides direct deref to `Client`.
#[cfg(feature = "deadpool")]
pub struct PooledConnection(deadpool_postgres::Object);

#[cfg(feature = "deadpool")]
impl Deref for PooledConnection {
    type Target = Client;

    fn deref(&self) -> &Client {
        // Object -> ClientWrapper -> Client
        &self.0
    }
}

/// Implementation for deadpool connection pool.
#[cfg(feature = "deadpool")]
impl ConnectionProvider for deadpool_postgres::Pool {
    type Guard<'a> = PooledConnection;

    async fn get(&self) -> Result<Self::Guard<'_>, Error> {
        self.get()
            .await
            .map(PooledConnection)
            .map_err(|e| Error::Pool(e.to_string()))
    }
}
