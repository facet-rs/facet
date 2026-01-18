use crate::{MigrationFn, Result};
use tokio_postgres::Client;

/// A registered migration.
pub struct Migration {
    /// Version string, e.g. "2026-01-17-create-users"
    pub version: &'static str,
    /// Function name for debugging
    pub name: &'static str,
    /// The migration function
    pub run: MigrationFn,
    /// Source file path (from file!())
    pub source_file: &'static str,
}

/// Context passed to migration functions.
pub struct MigrationContext<'a> {
    client: &'a Client,
}

impl<'a> MigrationContext<'a> {
    pub fn new(client: &'a Client) -> Self {
        Self { client }
    }

    /// Execute a SQL statement.
    pub async fn execute(&self, sql: &str) -> Result<u64> {
        Ok(self.client.execute(sql, &[]).await?)
    }

    /// Execute a SQL statement with parameters.
    pub async fn execute_params(
        &self,
        sql: &str,
        params: &[&(dyn tokio_postgres::types::ToSql + Sync)],
    ) -> Result<u64> {
        Ok(self.client.execute(sql, params).await?)
    }

    /// Run a backfill operation in batches until it returns 0 rows affected.
    pub async fn backfill<F, Fut>(&self, mut f: F) -> Result<u64>
    where
        F: FnMut(&Client) -> Fut,
        Fut: std::future::Future<Output = Result<u64>>,
    {
        let mut total = 0u64;
        loop {
            let affected = f(self.client).await?;
            if affected == 0 {
                break;
            }
            total += affected;
        }
        Ok(total)
    }

    /// Get the underlying client for complex operations.
    pub fn client(&self) -> &Client {
        self.client
    }
}

/// Runs migrations against a database.
pub struct MigrationRunner<'a> {
    client: &'a Client,
}

impl<'a> MigrationRunner<'a> {
    pub fn new(client: &'a Client) -> Self {
        Self { client }
    }

    /// Ensure the migrations tracking table exists.
    pub async fn init(&self) -> Result<()> {
        self.client
            .execute(
                "CREATE TABLE IF NOT EXISTS _dibs_migrations (
                    version TEXT PRIMARY KEY,
                    applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
                )",
                &[],
            )
            .await?;
        Ok(())
    }

    /// Get all applied migration versions.
    pub async fn applied(&self) -> Result<Vec<String>> {
        let rows = self
            .client
            .query("SELECT version FROM _dibs_migrations ORDER BY version", &[])
            .await?;
        Ok(rows.iter().map(|r| r.get(0)).collect())
    }

    /// Get all pending migrations (registered but not applied).
    pub fn pending(&self, applied: &[String]) -> Vec<&'static Migration> {
        let mut migrations: Vec<_> = inventory::iter::<Migration>
            .into_iter()
            .filter(|m| !applied.contains(&m.version.to_string()))
            .collect();
        migrations.sort_by_key(|m| m.version);
        migrations
    }

    /// Run all pending migrations.
    pub async fn migrate(&self) -> Result<Vec<&'static str>> {
        self.init().await?;
        let applied = self.applied().await?;
        let pending = self.pending(&applied);

        let mut ran = Vec::new();
        for migration in pending {
            let mut ctx = MigrationContext::new(self.client);
            (migration.run)(&mut ctx).await?;

            self.client
                .execute(
                    "INSERT INTO _dibs_migrations (version) VALUES ($1)",
                    &[&migration.version],
                )
                .await?;

            ran.push(migration.version);
        }

        Ok(ran)
    }

    /// Get status of all migrations.
    pub async fn status(&self) -> Result<Vec<MigrationStatus>> {
        self.init().await?;
        let applied = self.applied().await?;

        let mut all: Vec<_> = inventory::iter::<Migration>
            .into_iter()
            .map(|m| MigrationStatus {
                version: m.version,
                name: m.name,
                applied: applied.contains(&m.version.to_string()),
                source_file: m.source_file,
            })
            .collect();
        all.sort_by_key(|m| m.version);
        Ok(all)
    }
}

/// Status of a single migration.
pub struct MigrationStatus {
    pub version: &'static str,
    pub name: &'static str,
    pub applied: bool,
    pub source_file: &'static str,
}
