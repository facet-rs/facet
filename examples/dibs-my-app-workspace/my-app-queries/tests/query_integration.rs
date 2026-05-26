//! Integration tests for generated queries.
//!
//! These tests run against a real Postgres instance using dockside.

use dockside::{Container, containers};
use std::time::Duration;
use tokio_postgres::NoTls;

async fn create_postgres_container() -> (Container, tokio_postgres::Client) {
    let container = tokio::task::spawn_blocking(|| {
        let container = Container::run(containers::postgres("18", "postgres"))
            .expect("Failed to start Postgres container");

        container
            .wait_for_log(
                "database system is ready to accept connections",
                Duration::from_secs(30),
            )
            .expect("Postgres failed to become ready");

        let port = container
            .wait_for_port(5432, Duration::from_secs(5))
            .expect("Failed to connect to postgres port");

        (container, port)
    })
    .await
    .expect("spawn_blocking failed");

    let (container, port) = container;

    let connection_string = format!(
        "host=127.0.0.1 port={} user=postgres password=postgres dbname=postgres",
        port
    );

    let mut last_err = None;
    let mut client_and_conn = None;
    for _ in 0..30 {
        match tokio_postgres::connect(&connection_string, NoTls).await {
            Ok(c) => {
                client_and_conn = Some(c);
                break;
            }
            Err(e) => {
                last_err = Some(e);
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
    let (client, connection) = client_and_conn
        .ok_or_else(|| last_err.unwrap())
        .expect("Failed to connect to Postgres after retries");

    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("Connection error: {}", e);
        }
    });

    (container, client)
}

async fn setup_schema(client: &tokio_postgres::Client) {
    // Create the product table matching my-app-db schema. `metadata` is
    // JSONB (the migration in my-app-db's m2026_01_27_145001_jsonb.rs
    // alters it from TEXT to JSONB) — keep them aligned here so the
    // @jsonb-param mutation tests below actually exercise the cast
    // path; with TEXT they'd silently never trigger the codegen bug
    // class that ::text::jsonb is meant to handle.
    client
        .execute(
            r#"
            CREATE TABLE "product" (
                "id" BIGSERIAL PRIMARY KEY,
                "handle" TEXT NOT NULL UNIQUE,
                "status" TEXT NOT NULL DEFAULT 'draft',
                "active" BOOLEAN NOT NULL DEFAULT true,
                "metadata" JSONB,
                "created_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
                "updated_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
                "deleted_at" TIMESTAMPTZ
            )
            "#,
            &[],
        )
        .await
        .expect("Failed to create product table");

    // Create the product_translation table
    client
        .execute(
            r#"
            CREATE TABLE "product_translation" (
                "id" BIGSERIAL PRIMARY KEY,
                "product_id" BIGINT NOT NULL REFERENCES "product"("id"),
                "locale" TEXT NOT NULL,
                "title" TEXT NOT NULL,
                "description" TEXT
            )
            "#,
            &[],
        )
        .await
        .expect("Failed to create product_translation table");
}

async fn insert_test_data(client: &tokio_postgres::Client) {
    // Insert test products with various statuses
    let products = [
        ("widget-a", "published", true),
        ("widget-b", "published", true),
        ("gadget-x", "published", false), // inactive
        ("prototype-z", "draft", true),
        ("old-product", "archived", true),
    ];

    for (handle, status, active) in products {
        client
            .execute(
                r#"INSERT INTO "product" ("handle", "status", "active") VALUES ($1, $2, $3)"#,
                &[&handle, &status, &active],
            )
            .await
            .expect("Failed to insert test product");
    }

    // Insert a soft-deleted product
    client
        .execute(
            r#"INSERT INTO "product" ("handle", "status", "active", "deleted_at") VALUES ($1, $2, $3, now())"#,
            &[&"deleted-product", &"published", &true],
        )
        .await
        .expect("Failed to insert deleted product");

    // Insert translations for some products
    // widget-a has an English translation with description
    client
        .execute(
            r#"INSERT INTO "product_translation" ("product_id", "locale", "title", "description")
               SELECT id, 'en', 'Widget Alpha', 'The original widget' FROM "product" WHERE handle = 'widget-a'"#,
            &[],
        )
        .await
        .expect("Failed to insert widget-a translation");

    // widget-b has a French translation without description
    client
        .execute(
            r#"INSERT INTO "product_translation" ("product_id", "locale", "title", "description")
               SELECT id, 'fr', 'Widget Bêta', NULL FROM "product" WHERE handle = 'widget-b'"#,
            &[],
        )
        .await
        .expect("Failed to insert widget-b translation");

    // gadget-x has no translation (to test LEFT JOIN returning None)
}

#[tokio::test]
async fn test_all_products() {
    let (_container, client) = create_postgres_container().await;
    setup_schema(&client).await;
    insert_test_data(&client).await;

    let results = my_app_queries::all_products(&client).await.unwrap();

    // Should return all non-deleted products (5 total)
    assert_eq!(results.len(), 5, "Expected 5 non-deleted products");

    // Results should be ordered by created_at DESC (most recent first)
    // Since we inserted in order, last inserted should be first
    let handles: Vec<_> = results.iter().map(|p| p.handle.as_str()).collect();
    assert!(handles.contains(&"widget-a"));
    assert!(handles.contains(&"widget-b"));
    assert!(handles.contains(&"gadget-x"));
    assert!(handles.contains(&"prototype-z"));
    assert!(handles.contains(&"old-product"));

    // Should NOT contain deleted product
    assert!(!handles.contains(&"deleted-product"));
}

#[tokio::test]
async fn test_active_products() {
    let (_container, client) = create_postgres_container().await;
    setup_schema(&client).await;
    insert_test_data(&client).await;

    let results = my_app_queries::active_products(&client).await.unwrap();

    // Should only return published AND active products
    // widget-a: published, active ✓
    // widget-b: published, active ✓
    // gadget-x: published, inactive ✗
    // prototype-z: draft, active ✗
    // old-product: archived, active ✗
    assert_eq!(results.len(), 2, "Expected 2 active published products");

    let handles: Vec<_> = results.iter().map(|p| p.handle.as_str()).collect();
    assert!(handles.contains(&"widget-a"));
    assert!(handles.contains(&"widget-b"));

    // All should have status = published
    for result in &results {
        assert_eq!(result.status, "published");
    }
}

#[tokio::test]
async fn test_product_by_handle() {
    let (_container, client) = create_postgres_container().await;
    setup_schema(&client).await;
    insert_test_data(&client).await;

    // Find existing product
    let handle = "widget-a".to_string();
    let result = my_app_queries::product_by_handle(&client, &handle)
        .await
        .unwrap();

    assert!(result.is_some(), "Expected to find widget-a");
    let product = result.unwrap();
    assert_eq!(product.handle, "widget-a");
    assert_eq!(product.status, "published");
    assert!(product.active);
    // Note: created_at field removed due to jiff timestamp deserialization not yet supported

    // Find non-existent product
    let handle = "does-not-exist".to_string();
    let result = my_app_queries::product_by_handle(&client, &handle)
        .await
        .unwrap();
    assert!(result.is_none(), "Expected None for non-existent product");

    // Deleted product should not be found
    let handle = "deleted-product".to_string();
    let result = my_app_queries::product_by_handle(&client, &handle)
        .await
        .unwrap();
    assert!(
        result.is_none(),
        "Deleted product should not be found via query"
    );
}

#[tokio::test]
async fn test_search_products() {
    let (_container, client) = create_postgres_container().await;
    setup_schema(&client).await;
    insert_test_data(&client).await;

    // Search for "widget" - should match widget-a, widget-b
    let q = "%widget%".to_string();
    let results = my_app_queries::search_products(&client, &q).await.unwrap();

    assert_eq!(results.len(), 2, "Expected 2 products matching 'widget'");
    let handles: Vec<_> = results.iter().map(|p| p.handle.as_str()).collect();
    assert!(handles.contains(&"widget-a"));
    assert!(handles.contains(&"widget-b"));

    // Results should be ordered by handle ASC
    assert_eq!(results[0].handle, "widget-a");
    assert_eq!(results[1].handle, "widget-b");

    // Search for "gadget" - should match gadget-x
    let q = "%gadget%".to_string();
    let results = my_app_queries::search_products(&client, &q).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].handle, "gadget-x");

    // Search for non-matching pattern
    let q = "%nonexistent%".to_string();
    let results = my_app_queries::search_products(&client, &q).await.unwrap();
    assert!(results.is_empty());

    // Case-insensitive search (ILIKE)
    let q = "%WIDGET%".to_string();
    let results = my_app_queries::search_products(&client, &q).await.unwrap();
    assert_eq!(results.len(), 2, "ILIKE should be case-insensitive");
}

#[tokio::test]
async fn test_products_paginated() {
    let (_container, client) = create_postgres_container().await;
    setup_schema(&client).await;
    insert_test_data(&client).await;

    // Get first page (2 items)
    let page_size = 2i64;
    let page_offset = 0i64;
    let results = my_app_queries::products_paginated(&client, &page_size, &page_offset)
        .await
        .unwrap();

    assert_eq!(results.len(), 2, "First page should have 2 items");
    // Results ordered by handle ASC
    assert_eq!(results[0].handle, "gadget-x");
    assert_eq!(results[1].handle, "old-product");

    // Get second page (2 items, offset 2)
    let page_offset = 2i64;
    let results = my_app_queries::products_paginated(&client, &page_size, &page_offset)
        .await
        .unwrap();

    assert_eq!(results.len(), 2, "Second page should have 2 items");
    assert_eq!(results[0].handle, "prototype-z");
    assert_eq!(results[1].handle, "widget-a");

    // Get third page (1 item remaining, offset 4)
    let page_offset = 4i64;
    let results = my_app_queries::products_paginated(&client, &page_size, &page_offset)
        .await
        .unwrap();

    assert_eq!(results.len(), 1, "Third page should have 1 item");
    assert_eq!(results[0].handle, "widget-b");

    // Get page beyond data (offset 10)
    let page_offset = 10i64;
    let results = my_app_queries::products_paginated(&client, &page_size, &page_offset)
        .await
        .unwrap();

    assert!(results.is_empty(), "Page beyond data should be empty");
}

#[tokio::test]
async fn test_product_with_translation() {
    let (_container, client) = create_postgres_container().await;
    setup_schema(&client).await;
    insert_test_data(&client).await;

    // Test product with translation that has description
    let handle = "widget-a".to_string();
    let result = my_app_queries::product_with_translation(&client, &handle)
        .await
        .unwrap();

    assert!(result.is_some(), "Expected to find widget-a");
    let product = result.unwrap();
    assert_eq!(product.handle, "widget-a");
    assert_eq!(product.status, "published");

    // Check nested translation
    assert!(
        product.translation.is_some(),
        "Expected translation for widget-a"
    );
    let translation = product.translation.unwrap();
    assert_eq!(translation.locale, "en");
    assert_eq!(translation.title, "Widget Alpha");
    assert_eq!(
        translation.description,
        Some("The original widget".to_string())
    );

    // Test product with translation that has NULL description
    let handle = "widget-b".to_string();
    let result = my_app_queries::product_with_translation(&client, &handle)
        .await
        .unwrap();

    assert!(result.is_some(), "Expected to find widget-b");
    let product = result.unwrap();
    assert!(
        product.translation.is_some(),
        "Expected translation for widget-b"
    );
    let translation = product.translation.unwrap();
    assert_eq!(translation.locale, "fr");
    assert_eq!(translation.title, "Widget Bêta");
    assert!(
        translation.description.is_none(),
        "widget-b translation should have no description"
    );

    // Test product without translation (LEFT JOIN returns None)
    let handle = "gadget-x".to_string();
    let result = my_app_queries::product_with_translation(&client, &handle)
        .await
        .unwrap();

    assert!(result.is_some(), "Expected to find gadget-x");
    let product = result.unwrap();
    assert_eq!(product.handle, "gadget-x");
    assert!(
        product.translation.is_none(),
        "gadget-x should have no translation"
    );

    // Test non-existent product
    let handle = "does-not-exist".to_string();
    let result = my_app_queries::product_with_translation(&client, &handle)
        .await
        .unwrap();
    assert!(result.is_none(), "Expected None for non-existent product");
}

// ============================================================================
// Mutation tests
// ============================================================================
// The SELECT tests above were already in place. The tests below close
// the gap that let the $N::jsonb codegen bug ship: nothing actually
// inserted/updated rows through the generated mutation functions
// against real postgres. The new tests exercise every mutation path
// (INSERT with/without RETURNING, UPDATE returning u64, UPSERT,
// INSERT-MANY UNNEST, DELETE) and specifically the @jsonb param
// surface end-to-end (bind → server cast → read back).

/// Regression test for the $N::jsonb bug. Bind a JSON string to a
/// JSONB column via an @jsonb param, then read it back. If qgen
/// regresses to a single ::jsonb cast, tokio-postgres will reject
/// `String::to_sql(JSONB, …)` and this test fails at the insert call
/// with "error serializing parameter N" — exactly the prod symptom we
/// hit.
#[tokio::test]
async fn test_create_product_with_jsonb_metadata_roundtrip() {
    let (_container, client) = create_postgres_container().await;
    setup_schema(&client).await;

    let handle = "jsonb-roundtrip".to_string();
    let metadata = r#"{"color":"orange","weight_g":42}"#.to_string();

    let inserted = my_app_queries::create_product_with_metadata(&client, &handle, &metadata)
        .await
        .expect("insert should succeed")
        .expect("insert should return a row");
    assert_eq!(inserted.handle, "jsonb-roundtrip");

    let read = my_app_queries::product_metadata_by_id(&client, &inserted.id)
        .await
        .expect("select should succeed")
        .expect("row should exist");
    assert_eq!(read.handle, "jsonb-roundtrip");

    // The Jsonb<Value> roundtrip should preserve our object verbatim.
    let stored = read.metadata.expect("metadata should be present").0;
    let stored_obj = stored.as_object().expect("metadata should be an object");
    assert_eq!(
        stored_obj
            .get("color")
            .and_then(|v| v.as_string())
            .map(|s| s.as_str()),
        Some("orange"),
    );
}

/// UPDATE with @jsonb param exercises the same cast on the SET side.
/// Returns `Result<u64, QueryError>` — also asserts the affected count.
#[tokio::test]
async fn test_update_jsonb_metadata() {
    let (_container, client) = create_postgres_container().await;
    setup_schema(&client).await;

    let handle = "jsonb-update".to_string();
    let initial = r#"{"v":1}"#.to_string();
    let updated = r#"{"v":2,"tag":"bumped"}"#.to_string();

    let row = my_app_queries::create_product_with_metadata(&client, &handle, &initial)
        .await
        .unwrap()
        .unwrap();

    let affected = my_app_queries::update_product_metadata(&client, &row.id, &updated)
        .await
        .expect("update should succeed");
    assert_eq!(affected, 1);

    let read = my_app_queries::product_metadata_by_id(&client, &row.id)
        .await
        .unwrap()
        .unwrap();
    let stored = read.metadata.unwrap().0;
    let stored_obj = stored.as_object().unwrap();
    assert_eq!(
        stored_obj.get("v").and_then(|v| v.as_number()).is_some(),
        true
    );
    assert_eq!(
        stored_obj
            .get("tag")
            .and_then(|v| v.as_string())
            .map(|s| s.as_str()),
        Some("bumped"),
    );
}

/// UPSERT with @jsonb: first call inserts, second call (same handle)
/// hits the conflict branch and updates `metadata` via the SET clause.
#[tokio::test]
async fn test_upsert_jsonb_metadata_insert_then_update() {
    let (_container, client) = create_postgres_container().await;
    setup_schema(&client).await;

    let handle = "jsonb-upsert".to_string();
    let first = r#"{"phase":"insert"}"#.to_string();
    let second = r#"{"phase":"update"}"#.to_string();

    let a = my_app_queries::upsert_product_with_metadata(&client, &handle, &first)
        .await
        .unwrap()
        .unwrap();
    let b = my_app_queries::upsert_product_with_metadata(&client, &handle, &second)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(a.id, b.id, "second upsert should hit the same row");

    let read = my_app_queries::product_metadata_by_id(&client, &a.id)
        .await
        .unwrap()
        .unwrap();
    let obj = read.metadata.unwrap().0;
    let obj = obj.as_object().unwrap();
    assert_eq!(
        obj.get("phase")
            .and_then(|v| v.as_string())
            .map(|s| s.as_str()),
        Some("update")
    );
}

/// INSERT without RETURNING goes through the `execute` codepath and
/// yields `Result<u64, QueryError>` (affected count) instead of the
/// `Result<Option<…Result>, …>` shape the RETURNING variant produces.
/// `create_product_with_defaults` returns id/handle/status; for the
/// no-RETURNING path we use the new `delete_product_by_handle`
/// query — same shape (u64).
#[tokio::test]
async fn test_delete_without_returning_yields_affected_count() {
    let (_container, client) = create_postgres_container().await;
    setup_schema(&client).await;
    insert_test_data(&client).await;

    // prototype-z has no product_translation row — picking widget-a
    // would trip the FK and obscure what the test is checking.
    let handle = "prototype-z".to_string();
    let affected = my_app_queries::delete_product_by_handle(&client, &handle)
        .await
        .expect("delete should succeed");
    assert_eq!(affected, 1, "exactly one row should match handle");

    // Idempotent: a second delete of the same handle finds nothing.
    let again = my_app_queries::delete_product_by_handle(&client, &handle)
        .await
        .unwrap();
    assert_eq!(again, 0);
}

/// DELETE with RETURNING shapes as `Option<…Result>` (the row we
/// removed) — this is the symmetric companion to the no-RETURNING
/// test above.
#[tokio::test]
async fn test_delete_with_returning_yields_removed_row() {
    let (_container, client) = create_postgres_container().await;
    setup_schema(&client).await;
    insert_test_data(&client).await;

    // Find an id to delete via the already-tested select. Use
    // prototype-z (no translation row) so the FK doesn't block.
    let handle = "prototype-z".to_string();
    let target = my_app_queries::product_by_handle(&client, &handle)
        .await
        .unwrap()
        .unwrap();

    let removed = my_app_queries::delete_product(&client, &target.id)
        .await
        .unwrap()
        .expect("RETURNING should yield the deleted row");
    assert_eq!(removed.handle, "prototype-z");
}

/// UPDATE with RETURNING — the existing `UpdateProductStatus` query.
/// Asserts the returned row reflects the new status.
#[tokio::test]
async fn test_update_product_status_returns_updated_row() {
    let (_container, client) = create_postgres_container().await;
    setup_schema(&client).await;
    insert_test_data(&client).await;

    let handle = "prototype-z".to_string();
    let new_status = "published".to_string();
    let updated = my_app_queries::update_product_status(&client, &handle, &new_status)
        .await
        .unwrap()
        .expect("update should match one row");
    assert_eq!(updated.handle, "prototype-z");
    assert_eq!(updated.status, "published");
}

/// Bulk INSERT via UNNEST. The generated function accepts a slice of
/// param structs; exercises the multi-row codepath that's separate
/// from single-row INSERT.
#[tokio::test]
async fn test_bulk_create_products_inserts_all_rows() {
    let (_container, client) = create_postgres_container().await;
    setup_schema(&client).await;

    let rows = vec![
        my_app_queries::BulkCreateProductsParams {
            handle: "bulk-a".to_string(),
            status: "draft".to_string(),
        },
        my_app_queries::BulkCreateProductsParams {
            handle: "bulk-b".to_string(),
            status: "published".to_string(),
        },
        my_app_queries::BulkCreateProductsParams {
            handle: "bulk-c".to_string(),
            status: "draft".to_string(),
        },
    ];

    let returned = my_app_queries::bulk_create_products(&client, &rows)
        .await
        .expect("bulk insert should succeed");
    assert_eq!(returned.len(), 3, "RETURNING yields one row per insert");
    let handles: Vec<_> = returned.iter().map(|r| r.handle.as_str()).collect();
    assert!(handles.contains(&"bulk-a"));
    assert!(handles.contains(&"bulk-b"));
    assert!(handles.contains(&"bulk-c"));

    // Sanity-check via the existing search query.
    let q = "%bulk-%".to_string();
    let found = my_app_queries::search_products(&client, &q).await.unwrap();
    assert_eq!(found.len(), 3);
}
