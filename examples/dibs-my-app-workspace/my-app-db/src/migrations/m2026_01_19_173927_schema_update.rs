//! Migration: schema-update
//! Created: 2026-01-19 17:39:27 CET

use dibs::{MigrationContext, MigrationResult};

#[dibs::migration]
pub async fn migrate(ctx: &mut MigrationContext<'_>) -> MigrationResult<()> {
    ctx.execute("DROP TABLE \"comment\"").await?;
    ctx.execute("DROP TABLE \"post_like\"").await?;
    ctx.execute("DROP TABLE \"post_tag\"").await?;
    ctx.execute(
        r#"
CREATE TABLE "product" (
    "id" BIGINT PRIMARY KEY,
    "handle" TEXT NOT NULL UNIQUE,
    "status" TEXT NOT NULL DEFAULT 'draft',
    "active" BOOLEAN NOT NULL DEFAULT true,
    "metadata" TEXT,
    "created_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    "updated_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    "deleted_at" TIMESTAMPTZ
)
"#,
    )
    .await?;
    ctx.execute(
        r#"
CREATE TABLE "product_source" (
    "id" BIGINT PRIMARY KEY,
    "product_id" BIGINT NOT NULL,
    "vendor" TEXT NOT NULL,
    "external_id" TEXT NOT NULL,
    "last_synced_at" TIMESTAMPTZ,
    "raw_data" TEXT
)
"#,
    )
    .await?;
    ctx.execute(
        r#"
CREATE TABLE "product_translation" (
    "id" BIGINT PRIMARY KEY,
    "product_id" BIGINT NOT NULL,
    "locale" TEXT NOT NULL,
    "title" TEXT NOT NULL,
    "description" TEXT
)
"#,
    )
    .await?;
    ctx.execute(
        r#"
CREATE TABLE "product_variant" (
    "id" BIGINT PRIMARY KEY,
    "product_id" BIGINT NOT NULL,
    "sku" TEXT NOT NULL UNIQUE,
    "title" TEXT NOT NULL,
    "attributes" TEXT,
    "manage_inventory" BOOLEAN NOT NULL DEFAULT true,
    "allow_backorder" BOOLEAN NOT NULL DEFAULT false,
    "sort_order" INTEGER NOT NULL DEFAULT 0,
    "created_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    "updated_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    "deleted_at" TIMESTAMPTZ
)
"#,
    )
    .await?;
    ctx.execute("DROP TABLE \"tag\"").await?;
    ctx.execute("DROP TABLE \"user_follow\"").await?;
    ctx.execute(
        r#"
CREATE TABLE "variant_price" (
    "id" BIGINT PRIMARY KEY,
    "variant_id" BIGINT NOT NULL,
    "currency_code" TEXT NOT NULL,
    "amount" NUMERIC NOT NULL,
    "region" TEXT,
    "created_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    "updated_at" TIMESTAMPTZ NOT NULL DEFAULT now()
)
"#,
    )
    .await?;
    ctx.execute("DROP TABLE \"post\"").await?;
    ctx.execute("DROP TABLE \"user\"").await?;
    ctx.execute("DROP TABLE \"category\"").await?;

    Ok(())
}
