use crate::{MigrationFn, Result};
use tokio_postgres::{Client, Transaction};

/// A registered migration.
pub struct Migration {
    /// Version string, e.g. "2026-01-17-create-users"
    pub version: &'static str,
    /// Function name for debugging
    pub name: &'static str,
    /// The migration function
    pub run: MigrationFn,
    /// Source file path (CARGO_MANIFEST_DIR, file!())
    pub source_file: (&'static str, &'static str),
}

impl Migration {
    /// Get the resolved source file path.
    ///
    /// This handles the complexity of `file!()` in workspace members, where
    /// `file!()` returns a path relative to the workspace root (e.g.,
    /// `examples/my-app-db/src/...`) while `CARGO_MANIFEST_DIR` is the
    /// absolute path to the crate (e.g., `/path/to/workspace/examples/my-app-db`).
    pub fn source_path(&self) -> std::path::PathBuf {
        let (manifest_dir, file_path) = self.source_file;
        let file_path = std::path::Path::new(file_path);

        if file_path.is_absolute() {
            return file_path.to_path_buf();
        }

        // Try manifest_dir + file_path first (works for non-workspace crates)
        let full = std::path::Path::new(manifest_dir).join(file_path);
        if full.exists() {
            return full;
        }

        // file!() in workspace members includes the path from workspace root
        // e.g., file!() = "examples/my-app-db/src/..." and manifest_dir ends with "examples/my-app-db"
        // Strip the duplicated crate path portion
        if let Some(crate_name) = std::path::Path::new(manifest_dir).file_name() {
            let crate_name = crate_name.to_string_lossy();
            let file_str = file_path.to_string_lossy();
            if let Some(pos) = file_str.find(&*crate_name) {
                let relative = &file_str[pos + crate_name.len()..];
                let relative = relative.trim_start_matches('/');
                let full = std::path::Path::new(manifest_dir).join(relative);
                if full.exists() {
                    return full;
                }
            }
        }

        // Try walking up to workspace root
        let mut workspace = std::path::Path::new(manifest_dir);
        while let Some(parent) = workspace.parent() {
            let candidate = parent.join(file_path);
            if candidate.exists() {
                return candidate;
            }
            workspace = parent;
        }

        // Last resort: return the combined path even if it doesn't exist
        std::path::Path::new(manifest_dir).join(file_path)
    }
}

/// Context passed to migration functions.
///
/// Wraps a database transaction, ensuring all migration operations are atomic.
pub struct MigrationContext<'a> {
    tx: &'a Transaction<'a>,
}

impl<'a> MigrationContext<'a> {
    pub fn new(tx: &'a Transaction<'a>) -> Self {
        Self { tx }
    }

    /// Execute a SQL statement.
    pub async fn execute(&self, sql: &str) -> Result<u64> {
        Ok(self.tx.execute(sql, &[]).await?)
    }

    /// Execute a SQL statement with parameters.
    pub async fn execute_params(
        &self,
        sql: &str,
        params: &[&(dyn tokio_postgres::types::ToSql + Sync)],
    ) -> Result<u64> {
        Ok(self.tx.execute(sql, params).await?)
    }

    /// Run a backfill operation in batches until it returns 0 rows affected.
    ///
    /// Note: Since we're in a transaction, all batches are part of the same
    /// atomic operation. For very large backfills that need to commit
    /// incrementally, consider breaking into multiple migrations.
    pub async fn backfill<F, Fut>(&self, mut f: F) -> Result<u64>
    where
        F: FnMut(&Transaction<'a>) -> Fut,
        Fut: std::future::Future<Output = Result<u64>>,
    {
        let mut total = 0u64;
        loop {
            let affected = f(self.tx).await?;
            if affected == 0 {
                break;
            }
            total += affected;
        }
        Ok(total)
    }

    /// Get the underlying transaction for complex operations.
    pub fn transaction(&self) -> &Transaction<'a> {
        self.tx
    }
}

/// Runs migrations against a database.
pub struct MigrationRunner<'a> {
    client: &'a mut Client,
}

impl<'a> MigrationRunner<'a> {
    pub fn new(client: &'a mut Client) -> Self {
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
    ///
    /// Each migration runs in its own transaction. If a migration fails,
    /// all its changes are rolled back and subsequent migrations are skipped.
    pub async fn migrate(&mut self) -> Result<Vec<&'static str>> {
        self.init().await?;
        let applied = self.applied().await?;
        let pending = self.pending(&applied);

        let mut ran = Vec::new();
        for migration in pending {
            // Each migration runs in its own transaction
            let tx = self.client.transaction().await?;

            let mut ctx = MigrationContext::new(&tx);
            (migration.run)(&mut ctx).await?;

            // Record the migration as applied (inside the same transaction)
            tx.execute(
                "INSERT INTO _dibs_migrations (version) VALUES ($1)",
                &[&migration.version],
            )
            .await?;

            // Commit the transaction
            tx.commit().await?;

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
                source_path: m.source_path(),
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
    pub source_path: std::path::PathBuf,
}
