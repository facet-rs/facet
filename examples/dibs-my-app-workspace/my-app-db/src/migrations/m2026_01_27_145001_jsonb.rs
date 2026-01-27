//! Migration: jsonb
//! Created: 2026-01-27 14:50:01 CET

use dibs::{MigrationContext, MigrationResult};

#[dibs::migration]
pub async fn migrate(ctx: &mut MigrationContext<'_>) -> MigrationResult<()> {
    ctx.execute(
        "ALTER TABLE \"product\" ALTER COLUMN \"metadata\" TYPE JSONB USING \"metadata\"::JSONB",
    )
    .await?;
    ctx.execute("ALTER TABLE \"product_source\" ADD CONSTRAINT \"product_source_product_id_fkey\" FOREIGN KEY (\"product_id\") REFERENCES \"product\" (\"id\")").await?;
    ctx.execute("ALTER TABLE \"product_translation\" ADD CONSTRAINT \"product_translation_product_id_fkey\" FOREIGN KEY (\"product_id\") REFERENCES \"product\" (\"id\")").await?;
    ctx.execute("ALTER TABLE \"product_variant\" ALTER COLUMN \"attributes\" TYPE JSONB USING \"attributes\"::JSONB").await?;
    ctx.execute("ALTER TABLE \"product_variant\" ADD CONSTRAINT \"product_variant_product_id_fkey\" FOREIGN KEY (\"product_id\") REFERENCES \"product\" (\"id\")").await?;
    ctx.execute("ALTER TABLE \"variant_price\" ADD CONSTRAINT \"variant_price_variant_id_fkey\" FOREIGN KEY (\"variant_id\") REFERENCES \"product_variant\" (\"id\")").await?;

    Ok(())
}
