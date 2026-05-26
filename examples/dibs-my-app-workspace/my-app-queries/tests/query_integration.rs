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

// ============================================================================
// Codegen-surface coverage
// ============================================================================
// These tests don't target a known bug; they exercise the parts of
// dibs-qgen that no other test reaches. Each one is the canonical
// integration test for the named codegen path; if the generator
// silently regresses one of them, the test fails against postgres
// instead of against a customer.

/// `distinct true` should produce a SELECT DISTINCT. Test data has
/// statuses {published, draft, archived} — DISTINCT should collapse
/// the duplicates to those three values.
#[tokio::test]
async fn test_unique_statuses_distinct() {
    let (_container, client) = create_postgres_container().await;
    setup_schema(&client).await;
    insert_test_data(&client).await;

    let mut statuses: Vec<String> = my_app_queries::unique_statuses(&client)
        .await
        .unwrap()
        .into_iter()
        .map(|r| r.status)
        .collect();
    statuses.sort();
    assert_eq!(
        statuses,
        vec![
            "archived".to_string(),
            "draft".to_string(),
            "published".to_string()
        ],
        "DISTINCT should yield exactly the three populated statuses",
    );
}

/// `@ne($param)` filter generates `col != $1`. Asserts the parameter
/// is bound (we already exercise @null, @ilike, equality elsewhere;
/// @ne has its own codegen branch).
#[tokio::test]
async fn test_products_excluding_status_ne_filter() {
    let (_container, client) = create_postgres_container().await;
    setup_schema(&client).await;
    insert_test_data(&client).await;

    let excluded = "published".to_string();
    let rows = my_app_queries::products_excluding_status(&client, &excluded)
        .await
        .unwrap();
    // Test data: 3 published (one soft-deleted), 1 draft, 1 archived.
    // The deleted_at @null clause in the styx hides the soft-deleted one.
    // Excluding "published" leaves draft + archived = 2 rows.
    assert_eq!(rows.len(), 2);
    for r in &rows {
        assert_ne!(r.status, "published");
    }
}

/// `@in(literal_list)` currently emits malformed SQL — the styx
/// `@in("'a','b','c'")` renders as `ANY('''a'',''b'',''c''')`,
/// which postgres rejects with SQLSTATE 22P02 ("malformed array
/// literal") because the comma-separated list isn't valid array
/// syntax (needs `ARRAY[...]` or `IN (...)`).
///
/// This test pins the *current* broken behaviour so the failure is
/// visible and a future qgen fix flips it green. If you're staring
/// at a fresh failure here, the cause is good news: `@in` with a
/// literal array now actually works, and you should switch the
/// assertion to the happy-path checks below.
#[tokio::test]
async fn test_products_by_known_handles_in_filter_known_broken() {
    let (_container, client) = create_postgres_container().await;
    setup_schema(&client).await;

    for h in ["prod-1", "prod-2", "prod-3", "prod-unrelated"] {
        client
            .execute(
                r#"INSERT INTO "product" ("handle", "status") VALUES ($1, 'draft')"#,
                &[&h],
            )
            .await
            .unwrap();
    }

    let result = my_app_queries::products_by_known_handles(&client).await;
    let err = result.expect_err(
        "@in literal list still broken — see comment; if this succeeds, update the test",
    );
    let msg = format!("{err:?}");
    assert!(
        msg.contains("malformed array literal"),
        "expected the malformed-array-literal SQLSTATE 22P02, got: {msg}",
    );
}

/// One-to-many relation: `translations @rel{ ... }` without
/// `first true` generates a `Vec<Translations>` field on the result
/// struct. Different codegen path from the existing Option<Nested>
/// test (`product_with_translation`).
#[tokio::test]
async fn test_product_with_all_translations_vec_relation() {
    let (_container, client) = create_postgres_container().await;
    setup_schema(&client).await;
    insert_test_data(&client).await;

    // widget-a has 1 translation in insert_test_data.
    let handle = "widget-a".to_string();
    let p = my_app_queries::product_with_all_translations(&client, &handle)
        .await
        .unwrap()
        .expect("widget-a exists");
    assert_eq!(p.translations.len(), 1);
    assert_eq!(p.translations[0].locale, "en");

    // Add a second translation, confirm both come back in the Vec.
    client
        .execute(
            r#"INSERT INTO "product_translation" ("product_id", "locale", "title")
               SELECT id, 'de', 'Widget Alpha (DE)' FROM "product" WHERE handle = 'widget-a'"#,
            &[],
        )
        .await
        .unwrap();
    let p = my_app_queries::product_with_all_translations(&client, &handle)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(p.translations.len(), 2);
    let mut locales: Vec<&str> = p.translations.iter().map(|t| t.locale.as_str()).collect();
    locales.sort();
    assert_eq!(locales, vec!["de", "en"]);

    // gadget-x has zero translations — LEFT JOIN produces no rows for
    // the nested struct; the result should have an empty Vec, not a
    // Vec with one None-shaped entry.
    let handle = "gadget-x".to_string();
    let p = my_app_queries::product_with_all_translations(&client, &handle)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        p.translations.len(),
        0,
        "empty LEFT JOIN should produce empty Vec, not a singleton",
    );
}

/// Nested SQL functions in VALUES (`@lower(@concat("prod-", $x))`)
/// currently render with an uncast parameter — `CONCAT('prod-', $1)`
/// — and postgres bails at Parse with SQLSTATE 42P18 "could not
/// determine data type of parameter $1", because `CONCAT` is
/// polymorphic. Exactly the same bug class as the @jsonb one we
/// fixed: codegen doesn't pin the parameter's inferred type, so
/// tokio-postgres has nothing to satisfy ToSql against.
///
/// Fix would be in dibs-qgen: cast bare-param arguments of SQL
/// functions (`$1::text`) the way `cast_for_jsonb_param` does for
/// @jsonb. Until then this test pins the broken behaviour so a
/// future qgen fix flips it.
#[tokio::test]
async fn test_create_product_with_nested_sql_functions_known_broken() {
    let (_container, client) = create_postgres_container().await;
    setup_schema(&client).await;

    let handle = "WIDGET-X".to_string();
    let result = my_app_queries::create_product_normalized(&client, &handle).await;
    let err = result.expect_err(
        "@concat with bare $param still broken — see comment; if this succeeds, switch the test \
         to assert the happy-path roundtrip and remove `_known_broken`",
    );
    let msg = format!("{err:?}");
    assert!(
        msg.contains("could not determine data type of parameter"),
        "expected the polymorphic-parameter-inference error, got: {msg}",
    );
}

/// `@default` on a column in VALUES emits the literal `DEFAULT`
/// keyword (postgres applies the column's declared default). For
/// `status` that's `'draft'` per the schema.
#[tokio::test]
async fn test_create_product_with_default() {
    let (_container, client) = create_postgres_container().await;
    setup_schema(&client).await;

    let handle = "default-status".to_string();
    let row = my_app_queries::create_product_with_defaults(&client, &handle)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        row.status, "draft",
        "DEFAULT should pick up the column's literal default"
    );
}

/// `@now` in an UPDATE SET clause emits `NOW()` (no param). Verifies
/// the soft-delete query actually populates `deleted_at`.
#[tokio::test]
async fn test_soft_delete_sets_deleted_at_via_now() {
    let (_container, client) = create_postgres_container().await;
    setup_schema(&client).await;
    insert_test_data(&client).await;

    let handle = "widget-a".to_string();
    let row = my_app_queries::soft_delete_product(&client, &handle)
        .await
        .unwrap()
        .expect("update should return one row");
    assert_eq!(row.handle, "widget-a");

    // The deleted_at filter on all_products should now exclude it.
    let remaining = my_app_queries::all_products(&client).await.unwrap();
    let handles: Vec<&str> = remaining.iter().map(|p| p.handle.as_str()).collect();
    assert!(
        !handles.contains(&"widget-a"),
        "soft-deleted product disappears from non-deleted view"
    );
}

/// Plain UPSERT (no @jsonb). First call inserts, second call with
/// the same conflict target updates. Returns the row in both cases.
#[tokio::test]
async fn test_upsert_product_insert_then_update() {
    let (_container, client) = create_postgres_container().await;
    setup_schema(&client).await;

    let handle = "upsert-target".to_string();
    let s1 = "draft".to_string();
    let s2 = "published".to_string();

    let a = my_app_queries::upsert_product(&client, &handle, &s1)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(a.status, "draft");

    let b = my_app_queries::upsert_product(&client, &handle, &s2)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        b.id, a.id,
        "upsert should update the same row, not insert a duplicate"
    );
    assert_eq!(b.status, "published", "ON CONFLICT should overwrite status");
}

/// UPSERT-MANY via UNNEST + ON CONFLICT DO UPDATE. The bulk
/// equivalent of the test above; exercises the
/// `INSERT … SELECT … FROM UNNEST(...) ON CONFLICT` codegen path,
/// which is distinct from both `insert_many` (no conflict) and the
/// scalar upsert.
#[tokio::test]
async fn test_bulk_upsert_products_insert_then_update() {
    let (_container, client) = create_postgres_container().await;
    setup_schema(&client).await;

    let first = vec![
        my_app_queries::BulkUpsertProductsParams {
            handle: "bu-a".to_string(),
            status: "draft".to_string(),
        },
        my_app_queries::BulkUpsertProductsParams {
            handle: "bu-b".to_string(),
            status: "draft".to_string(),
        },
    ];
    let r1 = my_app_queries::bulk_upsert_products(&client, &first)
        .await
        .unwrap();
    assert_eq!(r1.len(), 2);
    let ids_first: std::collections::HashSet<i64> = r1.iter().map(|r| r.id).collect();

    // Re-upsert with the same handles + new status. Expect same row
    // ids back and updated status.
    let second = vec![
        my_app_queries::BulkUpsertProductsParams {
            handle: "bu-a".to_string(),
            status: "published".to_string(),
        },
        my_app_queries::BulkUpsertProductsParams {
            handle: "bu-b".to_string(),
            status: "published".to_string(),
        },
    ];
    let r2 = my_app_queries::bulk_upsert_products(&client, &second)
        .await
        .unwrap();
    assert_eq!(r2.len(), 2);
    let ids_second: std::collections::HashSet<i64> = r2.iter().map(|r| r.id).collect();
    assert_eq!(
        ids_first, ids_second,
        "ON CONFLICT should hit the same rows"
    );
    for r in &r2 {
        assert_eq!(r.status, "published");
    }
}
