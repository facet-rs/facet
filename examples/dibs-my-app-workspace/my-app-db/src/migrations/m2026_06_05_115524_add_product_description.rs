//! Migration: add_product_description
//! Created: 2026-06-05 11:55:24 CEST

use dibs::{MigrationContext, MigrationResult};

#[dibs::migration]
pub async fn migrate(ctx: &mut MigrationContext<'_>) -> MigrationResult<()> {
    ctx.execute("ALTER TABLE \"product\" ADD COLUMN \"description\" TEXT")
        .await?;

    Ok(())
}
