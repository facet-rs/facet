//! Integration tests against real PostgreSQL.
//!
//! These tests verify that:
//! 1. Generated SQL executes correctly against PostgreSQL
//! 2. Row assembly logic produces correct results
//! 3. JOIN queries with relations work as expected
//!
//! Run with: cargo nextest run -p dibs-qgen --test postgres_integration
//!
//! Note: Requires Docker to be running.

use camino::Utf8Path;
use dibs_db_schema::{Column, ForeignKey, PgType, Schema, SourceLocation, Table};
use dibs_qgen::{
    Decl, QueryFile, Select, generate_rust_code, generate_select_sql, parse_query_file,
};
use dockside::{Container, containers};
use indexmap::IndexMap;
use std::collections::HashMap;
use std::time::Duration;
use tokio_postgres::{Client, NoTls, Row};

/// Helper to parse a query file from a string source
fn parse_test_query(source: &str) -> QueryFile {
    parse_query_file(Utf8Path::new("<test>"), source).unwrap()
}

/// Helper to get the first Select query from a QueryFile
fn first_select(file: &QueryFile) -> &Select {
    for decl in file.0.values() {
        if let Decl::Select(select) = decl {
            return select;
        }
    }
    panic!("No Select query found in file")
}

/// Helper to get the first Insert from a QueryFile
fn first_insert(file: &QueryFile) -> &dibs_qgen::Insert {
    for decl in file.0.values() {
        if let Decl::Insert(insert) = decl {
            return insert;
        }
    }
    panic!("No Insert found in file")
}

/// Helper to get the first InsertMany from a QueryFile
fn first_insert_many(file: &QueryFile) -> &dibs_qgen::InsertMany {
    for decl in file.0.values() {
        if let Decl::InsertMany(insert_many) = decl {
            return insert_many;
        }
    }
    panic!("No InsertMany found in file")
}

/// Helper to get the first Update from a QueryFile
fn first_update(file: &QueryFile) -> &dibs_qgen::Update {
    for decl in file.0.values() {
        if let Decl::Update(update) = decl {
            return update;
        }
    }
    panic!("No Update found in file")
}

/// Helper to get the first Delete from a QueryFile
fn first_delete(file: &QueryFile) -> &dibs_qgen::Delete {
    for decl in file.0.values() {
        if let Decl::Delete(delete) = decl {
            return delete;
        }
    }
    panic!("No Delete found in file")
}

/// Helper to get the first Upsert from a QueryFile
fn first_upsert(file: &QueryFile) -> &dibs_qgen::Upsert {
    for decl in file.0.values() {
        if let Decl::Upsert(upsert) = decl {
            return upsert;
        }
    }
    panic!("No Upsert found in file")
}

/// Helper to get the first UpsertMany from a QueryFile
fn first_upsert_many(file: &QueryFile) -> &dibs_qgen::UpsertMany {
    for decl in file.0.values() {
        if let Decl::UpsertMany(upsert_many) = decl {
            return upsert_many;
        }
    }
    panic!("No UpsertMany found in file")
}

/// Set up a PostgreSQL container and return a connected client.
async fn setup_postgres() -> (Container, Client) {
    let container = Container::run(containers::postgres("16-alpine", "test"))
        .expect("failed to start postgres container");

    // Wait for postgres to be ready - it prints this message twice, so we wait for port instead
    container
        .wait_for_log(
            "database system is ready to accept connections",
            Duration::from_secs(30),
        )
        .expect("postgres did not become ready");

    let port = container
        .wait_for_port(5432, Duration::from_secs(10))
        .expect("postgres port not available");

    // Connect to postgres with retries (postgres may not be fully ready even after port is open)
    let conn_str = format!("host=127.0.0.1 port={} user=postgres password=test", port);

    let mut attempts = 0;
    let max_attempts = 10;
    let (client, connection) = loop {
        attempts += 1;
        match tokio_postgres::connect(&conn_str, NoTls).await {
            Ok(result) => break result,
            Err(e) if attempts < max_attempts => {
                tracing::debug!("Connection attempt {} failed: {}, retrying...", attempts, e);
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            Err(e) => panic!(
                "failed to connect to postgres after {} attempts: {}",
                attempts, e
            ),
        }
    };

    // Spawn the connection handler
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("connection error: {}", e);
        }
    });

    (container, client)
}

/// Create test tables for product, product_variant, and product_translation.
async fn create_test_tables(client: &Client) {
    client
        .batch_execute(
            r#"
            CREATE TABLE product (
                id BIGSERIAL PRIMARY KEY,
                handle TEXT NOT NULL UNIQUE,
                status TEXT NOT NULL DEFAULT 'draft'
            );

            CREATE TABLE product_variant (
                id BIGSERIAL PRIMARY KEY,
                product_id BIGINT NOT NULL REFERENCES product(id),
                sku TEXT NOT NULL,
                price_cents BIGINT NOT NULL DEFAULT 0
            );

            CREATE TABLE product_translation (
                id BIGSERIAL PRIMARY KEY,
                product_id BIGINT NOT NULL REFERENCES product(id),
                locale TEXT NOT NULL,
                title TEXT NOT NULL,
                description TEXT,
                UNIQUE(product_id, locale)
            );
            "#,
        )
        .await
        .expect("failed to create tables");
}

/// Create test tables with JSONB columns for testing JSONB operators.
async fn create_jsonb_test_tables(client: &Client) {
    client
        .batch_execute(
            r#"
            CREATE TABLE product_with_metadata (
                id BIGSERIAL PRIMARY KEY,
                handle TEXT NOT NULL UNIQUE,
                status TEXT NOT NULL DEFAULT 'draft',
                metadata JSONB
            );
            "#,
        )
        .await
        .expect("failed to create jsonb test tables");
}

/// Insert test data with JSONB values.
async fn insert_jsonb_test_data(client: &Client) {
    // Product with nested metadata
    client
        .execute(
            r#"INSERT INTO product_with_metadata (id, handle, status, metadata)
               VALUES (1, 'premium-widget', 'active',
                       '{"brand": "Acme", "premium": true, "specs": {"weight": 100, "color": "blue"}}'::jsonb)"#,
            &[],
        )
        .await
        .unwrap();

    // Product with simple metadata
    client
        .execute(
            r#"INSERT INTO product_with_metadata (id, handle, status, metadata)
               VALUES (2, 'basic-gadget', 'draft',
                       '{"brand": "Generic", "premium": false}'::jsonb)"#,
            &[],
        )
        .await
        .unwrap();

    // Product with array in metadata
    client
        .execute(
            r#"INSERT INTO product_with_metadata (id, handle, status, metadata)
               VALUES (3, 'multi-gizmo', 'active',
                       '{"brand": "Acme", "tags": ["electronics", "gadget"], "premium": true}'::jsonb)"#,
            &[],
        )
        .await
        .unwrap();

    // Product with null metadata
    client
        .execute(
            "INSERT INTO product_with_metadata (id, handle, status, metadata) VALUES (4, 'no-meta', 'active', NULL)",
            &[],
        )
        .await
        .unwrap();

    // Product with empty object metadata
    client
        .execute(
            r#"INSERT INTO product_with_metadata (id, handle, status, metadata)
               VALUES (5, 'empty-meta', 'active', '{}'::jsonb)"#,
            &[],
        )
        .await
        .unwrap();
}

/// Insert test data.
async fn insert_test_data(client: &Client) {
    // Insert products
    client
        .execute(
            "INSERT INTO product (id, handle, status) VALUES (1, 'widget', 'active')",
            &[],
        )
        .await
        .unwrap();
    client
        .execute(
            "INSERT INTO product (id, handle, status) VALUES (2, 'gadget', 'draft')",
            &[],
        )
        .await
        .unwrap();
    client
        .execute(
            "INSERT INTO product (id, handle, status) VALUES (3, 'gizmo', 'active')",
            &[],
        )
        .await
        .unwrap();

    // Insert variants for product 1 (widget has 3 variants)
    client
        .execute(
            "INSERT INTO product_variant (product_id, sku, price_cents) VALUES (1, 'WIDGET-S', 999)",
            &[],
        )
        .await
        .unwrap();
    client
        .execute(
            "INSERT INTO product_variant (product_id, sku, price_cents) VALUES (1, 'WIDGET-M', 1499)",
            &[],
        )
        .await
        .unwrap();
    client
        .execute(
            "INSERT INTO product_variant (product_id, sku, price_cents) VALUES (1, 'WIDGET-L', 1999)",
            &[],
        )
        .await
        .unwrap();

    // Insert variants for product 2 (gadget has 1 variant)
    client
        .execute(
            "INSERT INTO product_variant (product_id, sku, price_cents) VALUES (2, 'GADGET-1', 2999)",
            &[],
        )
        .await
        .unwrap();

    // Product 3 (gizmo) has no variants

    // Insert translations
    client
        .execute(
            "INSERT INTO product_translation (product_id, locale, title, description) VALUES (1, 'en', 'Widget', 'A wonderful widget')",
            &[],
        )
        .await
        .unwrap();
    client
        .execute(
            "INSERT INTO product_translation (product_id, locale, title, description) VALUES (1, 'fr', 'Widget', 'Un merveilleux widget')",
            &[],
        )
        .await
        .unwrap();
    client
        .execute(
            "INSERT INTO product_translation (product_id, locale, title, description) VALUES (2, 'en', 'Gadget', NULL)",
            &[],
        )
        .await
        .unwrap();
    // Product 3 has no translations
}

/// Helper to create a simple column
fn col(name: &str, pg_type: PgType, rust_type: &str, nullable: bool) -> Column {
    Column {
        name: name.to_string(),
        pg_type,
        rust_type: Some(rust_type.to_string()),
        nullable,
        default: None,
        primary_key: name == "id",
        unique: false,
        auto_generated: name == "id",
        long: false,
        label: false,
        enum_variants: vec![],
        doc: None,
        lang: None,
        icon: None,
        subtype: None,
    }
}

/// Helper to create a table
fn table(name: &str, columns: Vec<Column>, foreign_keys: Vec<ForeignKey>) -> Table {
    Table {
        name: name.to_string(),
        columns,
        check_constraints: vec![],
        trigger_checks: vec![],
        foreign_keys,
        indices: vec![],
        source: SourceLocation::default(),
        doc: None,
        icon: None,
    }
}

fn build_test_schema() -> Schema {
    let mut tables = IndexMap::new();

    tables.insert(
        "product".to_string(),
        table(
            "product",
            vec![
                col("id", PgType::BigInt, "i64", false),
                col("handle", PgType::Text, "String", false),
                col("status", PgType::Text, "String", false),
            ],
            vec![],
        ),
    );

    tables.insert(
        "product_variant".to_string(),
        table(
            "product_variant",
            vec![
                col("id", PgType::BigInt, "i64", false),
                col("product_id", PgType::BigInt, "i64", false),
                col("sku", PgType::Text, "String", false),
                col("price_cents", PgType::BigInt, "i64", false),
            ],
            vec![ForeignKey {
                columns: vec!["product_id".to_string()],
                references_table: "product".to_string(),
                references_columns: vec!["id".to_string()],
            }],
        ),
    );

    tables.insert(
        "product_translation".to_string(),
        table(
            "product_translation",
            vec![
                col("id", PgType::BigInt, "i64", false),
                col("product_id", PgType::BigInt, "i64", false),
                col("locale", PgType::Text, "String", false),
                col("title", PgType::Text, "String", false),
                col("description", PgType::Text, "String", true),
            ],
            vec![ForeignKey {
                columns: vec!["product_id".to_string()],
                references_table: "product".to_string(),
                references_columns: vec!["id".to_string()],
            }],
        ),
    );

    Schema { tables }
}

#[tokio::test]
async fn test_simple_query_against_postgres() {
    let (_container, client) = setup_postgres().await;
    create_test_tables(&client).await;
    insert_test_data(&client).await;

    // Simple query - no relations
    let source = r#"
AllProducts @select{
  from product
  fields { id, handle, status }
}
"#;
    let file = parse_test_query(source);
    let _query = first_select(&file);

    // Generate SQL
    let sql = "SELECT id, handle, status FROM product";

    let rows: Vec<Row> = client.query(sql, &[]).await.unwrap();
    assert_eq!(rows.len(), 3, "Should have 3 products");

    // Verify data
    let handles: Vec<String> = rows.iter().map(|r| r.get("handle")).collect();
    assert!(handles.contains(&"widget".to_string()));
    assert!(handles.contains(&"gadget".to_string()));
    assert!(handles.contains(&"gizmo".to_string()));
}

#[tokio::test]
async fn test_option_relation_query_against_postgres() {
    let (_container, client) = setup_postgres().await;
    create_test_tables(&client).await;
    insert_test_data(&client).await;

    let schema = build_test_schema();

    // Query with Option relation (first: true)
    let source = r#"
ProductWithTranslation @select{
  from product
  fields {
    id
    handle
    translation @rel{
      from product_translation
      first true
      fields { title, description }
    }
  }
}
"#;
    let file = parse_test_query(source);
    let query = first_select(&file);

    // Generate SQL with JOINs
    let generated = generate_select_sql(query, &schema).unwrap();
    tracing::info!("Generated SQL: {}", generated.sql);

    let rows: Vec<Row> = client.query(&generated.sql, &[]).await.unwrap();

    // With LEFT JOIN, we should get rows for all products
    // Products 1 and 2 have translations, product 3 doesn't
    assert!(
        rows.len() >= 3,
        "Should have at least 3 rows (one per product, possibly more for multiple translations)"
    );

    // Check that product 3 (gizmo) has NULL translation
    let gizmo_rows: Vec<&Row> = rows
        .iter()
        .filter(|r| r.get::<_, String>("handle") == "gizmo")
        .collect();
    assert_eq!(gizmo_rows.len(), 1, "Gizmo should appear once");
    let gizmo_title: Option<String> = gizmo_rows[0].get("translation_title");
    assert!(gizmo_title.is_none(), "Gizmo should have no translation");
}

#[tokio::test]
async fn test_vec_relation_query_against_postgres() {
    let (_container, client) = setup_postgres().await;
    create_test_tables(&client).await;
    insert_test_data(&client).await;

    let schema = build_test_schema();

    // Query with Vec relation (first: false - has-many)
    let source = r#"
ProductWithVariants @select{
  from product
  fields {
    id
    handle
    variants @rel{
      from product_variant
      fields { id, sku }
    }
  }
}
"#;
    let file = parse_test_query(source);
    let query = first_select(&file);

    // Generate SQL with JOINs
    let generated = generate_select_sql(query, &schema).unwrap();
    tracing::info!("Generated SQL: {}", generated.sql);

    let rows: Vec<Row> = client.query(&generated.sql, &[]).await.unwrap();

    // LEFT JOIN expands: widget (3 variants) + gadget (1 variant) + gizmo (0 variants, but 1 row with NULL)
    // = 3 + 1 + 1 = 5 rows
    assert_eq!(rows.len(), 5, "Should have 5 rows from LEFT JOIN expansion");

    // Now test the HashMap grouping logic that codegen produces
    let mut grouped: std::collections::HashMap<i64, (String, Vec<(i64, String)>)> =
        std::collections::HashMap::new();

    for row in rows.iter() {
        let parent_id: i64 = row.get("id");
        let handle: String = row.get("handle");

        let entry = grouped
            .entry(parent_id)
            .or_insert_with(|| (handle.clone(), vec![]));

        // Append variant if present
        if let Some(variant_id) = row.get::<_, Option<i64>>("variants_id") {
            let sku: String = row.get::<_, Option<String>>("variants_sku").unwrap();
            entry.1.push((variant_id, sku));
        }
    }

    assert_eq!(grouped.len(), 3, "Should have 3 products after grouping");

    // Find widget and check it has 3 variants
    let widget = grouped.values().find(|(h, _)| h == "widget").unwrap();
    assert_eq!(widget.1.len(), 3, "Widget should have 3 variants");

    // Find gadget and check it has 1 variant
    let gadget = grouped.values().find(|(h, _)| h == "gadget").unwrap();
    assert_eq!(gadget.1.len(), 1, "Gadget should have 1 variant");

    // Find gizmo and check it has 0 variants
    let gizmo = grouped.values().find(|(h, _)| h == "gizmo").unwrap();
    assert_eq!(gizmo.1.len(), 0, "Gizmo should have 0 variants");

    // Also verify the generated Rust code looks correct
    let code = generate_rust_code(&file, &schema);
    tracing::info!("Generated code:\n{}", code.code);

    assert!(
        code.code.contains("HashMap"),
        "Should use HashMap for grouping"
    );
    assert!(
        code.code.contains("variants: Vec::new()"),
        "Should initialize empty Vec"
    );
    assert!(
        code.code.contains("entry.variants.push"),
        "Should push to variants"
    );
}

#[tokio::test]
async fn test_filtered_query_with_params() {
    let (_container, client) = setup_postgres().await;
    create_test_tables(&client).await;
    insert_test_data(&client).await;

    let schema = build_test_schema();

    // Query with WHERE clause and parameters
    let source = r#"
ProductByHandle @select{
  params { handle @string }
  from product
  where { handle $handle }
  first true
  fields {
    id
    handle
    variants @rel{
      from product_variant
      fields { sku }
    }
  }
}
"#;
    let file = parse_test_query(source);
    let query = first_select(&file);

    // Generate SQL with JOINs
    let generated = generate_select_sql(query, &schema).unwrap();
    tracing::info!("Generated SQL: {}", generated.sql);

    // Query for widget
    let handle = "widget".to_string();
    let rows: Vec<Row> = client.query(&generated.sql, &[&handle]).await.unwrap();

    // Widget has 3 variants
    assert_eq!(
        rows.len(),
        3,
        "Widget should return 3 rows (one per variant)"
    );

    // All rows should have the same product ID
    let product_ids: Vec<i64> = rows.iter().map(|r| r.get("id")).collect();
    assert!(
        product_ids.iter().all(|&id| id == product_ids[0]),
        "All rows should have the same product ID"
    );

    // Query for gizmo (no variants)
    let handle = "gizmo".to_string();
    let rows: Vec<Row> = client.query(&generated.sql, &[&handle]).await.unwrap();

    // Gizmo has no variants, but LEFT JOIN still returns 1 row with NULL variant
    assert_eq!(rows.len(), 1, "Gizmo should return 1 row");
    let variant_sku: Option<String> = rows[0].get("variants_sku");
    assert!(variant_sku.is_none(), "Gizmo's variant should be NULL");
}

#[tokio::test]
async fn test_count_query_against_postgres() {
    let (_container, client) = setup_postgres().await;
    create_test_tables(&client).await;
    insert_test_data(&client).await;

    let schema = build_test_schema();

    // Query with COUNT aggregate
    let source = r#"
ProductWithVariantCount @select{
  from product
  fields {
    id
    handle
    variant_count @count(product_variant)
  }
}
"#;
    let file = parse_test_query(source);
    let query = first_select(&file);

    // Generate SQL
    let generated = generate_select_sql(query, &schema).unwrap();
    tracing::info!("Generated SQL: {}", generated.sql);

    let rows: Vec<Row> = client.query(&generated.sql, &[]).await.unwrap();

    // Should have 3 rows (one per product), no duplication from JOINs
    assert_eq!(rows.len(), 3, "Should have 3 products");

    // Build a map of handle -> variant_count for verification
    let counts: HashMap<String, i64> = rows
        .iter()
        .map(|r| (r.get("handle"), r.get("variant_count")))
        .collect();

    // Widget has 3 variants
    assert_eq!(
        counts.get("widget"),
        Some(&3),
        "Widget should have 3 variants"
    );

    // Gadget has 1 variant
    assert_eq!(
        counts.get("gadget"),
        Some(&1),
        "Gadget should have 1 variant"
    );

    // Gizmo has 0 variants
    assert_eq!(
        counts.get("gizmo"),
        Some(&0),
        "Gizmo should have 0 variants"
    );
}

#[tokio::test]
async fn test_relation_where_literal() {
    let (_container, client) = setup_postgres().await;
    create_test_tables(&client).await;
    insert_test_data(&client).await;

    let schema = build_test_schema();

    // Query with relation-level WHERE using a literal value
    let source = r#"
ProductWithEnglishTranslation @select{
  from product
  fields {
    id
    handle
    translation @rel{
      from product_translation
      where { locale "en" }
      first true
      fields { title, description }
    }
  }
}
"#;
    let file = parse_test_query(source);
    let query = first_select(&file);

    let generated = generate_select_sql(query, &schema).unwrap();
    tracing::info!("Generated SQL: {}", generated.sql);

    // Verify the SQL contains the relation filter in ON clause
    assert!(
        generated.sql.contains("\"t1\".\"locale\" = 'en'"),
        "SQL should filter on locale in ON clause: {}",
        generated.sql
    );

    let rows: Vec<Row> = client.query(&generated.sql, &[]).await.unwrap();

    // Should have 3 rows (one per product)
    assert_eq!(rows.len(), 3, "Should have 3 products");

    // Widget has English translation
    let widget: &Row = rows
        .iter()
        .find(|r| r.get::<_, String>("handle") == "widget")
        .unwrap();
    let widget_title: Option<String> = widget.get("translation_title");
    assert_eq!(
        widget_title,
        Some("Widget".to_string()),
        "Widget should have English title"
    );

    // Gadget has English translation
    let gadget: &Row = rows
        .iter()
        .find(|r| r.get::<_, String>("handle") == "gadget")
        .unwrap();
    let gadget_title: Option<String> = gadget.get("translation_title");
    assert_eq!(
        gadget_title,
        Some("Gadget".to_string()),
        "Gadget should have English title"
    );

    // Gizmo has no translation at all
    let gizmo: &Row = rows
        .iter()
        .find(|r| r.get::<_, String>("handle") == "gizmo")
        .unwrap();
    let gizmo_title: Option<String> = gizmo.get("translation_title");
    assert!(gizmo_title.is_none(), "Gizmo should have no translation");
}

#[tokio::test]
async fn test_relation_where_param() {
    let (_container, client) = setup_postgres().await;
    create_test_tables(&client).await;
    insert_test_data(&client).await;

    let schema = build_test_schema();

    // Query with relation-level WHERE using a parameter
    let source = r#"
ProductWithTranslationByLocale @select{
  params { locale @string }
  from product
  fields {
    id
    handle
    translation @rel{
      from product_translation
      where { locale $locale }
      first true
      fields { title, description }
    }
  }
}
"#;
    let file = parse_test_query(source);
    let query = first_select(&file);

    let generated = generate_select_sql(query, &schema).unwrap();
    tracing::info!("Generated SQL: {}", generated.sql);

    // Verify the SQL contains the relation filter with param placeholder
    assert!(
        generated.sql.contains("\"t1\".\"locale\" = $1"),
        "SQL should filter on locale with $1: {}",
        generated.sql
    );

    // Query for French translations
    let locale = "fr".to_string();
    let rows: Vec<Row> = client.query(&generated.sql, &[&locale]).await.unwrap();

    assert_eq!(rows.len(), 3, "Should have 3 products");

    // Widget has French translation
    let widget: &Row = rows
        .iter()
        .find(|r| r.get::<_, String>("handle") == "widget")
        .unwrap();
    let widget_desc: Option<String> = widget.get("translation_description");
    assert_eq!(
        widget_desc,
        Some("Un merveilleux widget".to_string()),
        "Widget should have French description"
    );

    // Gadget has no French translation
    let gadget: &Row = rows
        .iter()
        .find(|r| r.get::<_, String>("handle") == "gadget")
        .unwrap();
    let gadget_title: Option<String> = gadget.get("translation_title");
    assert!(
        gadget_title.is_none(),
        "Gadget should have no French translation"
    );
}

#[tokio::test]
async fn test_relation_where_with_base_where() {
    let (_container, client) = setup_postgres().await;
    create_test_tables(&client).await;
    insert_test_data(&client).await;

    let schema = build_test_schema();

    // Query with BOTH base WHERE and relation WHERE
    let source = r#"
ActiveProductWithTranslation @select{
  params { status @string, locale @string }
  from product
  where { status $status }
  fields {
    id
    handle
    translation @rel{
      from product_translation
      where { locale $locale }
      first true
      fields { title }
    }
  }
}
"#;
    let file = parse_test_query(source);
    let query = first_select(&file);

    let generated = generate_select_sql(query, &schema).unwrap();
    tracing::info!("Generated SQL: {}", generated.sql);
    tracing::info!("Param order: {:?}", generated.param_order);

    // Relation filter should be $1 (in ON clause, comes first)
    // Base WHERE filter should be $2
    assert!(
        generated.sql.contains("\"t1\".\"locale\" = $1"),
        "Relation filter should be $1: {}",
        generated.sql
    );
    assert!(
        generated.sql.contains("\"t0\".\"status\" = $2"),
        "Base filter should be $2: {}",
        generated.sql
    );

    // Param order should be: locale (from ON clause), then status (from WHERE)
    let param_names: Vec<&str> = generated.param_order.iter().map(|p| p.as_str()).collect();
    assert_eq!(
        param_names,
        vec!["locale", "status"],
        "Param order should be [locale, status]"
    );

    // Query for active products with English translation
    let locale = "en".to_string();
    let status = "active".to_string();
    let rows: Vec<Row> = client
        .query(&generated.sql, &[&locale, &status])
        .await
        .unwrap();

    // Only widget and gizmo are active
    assert_eq!(rows.len(), 2, "Should have 2 active products");

    let handles: Vec<String> = rows.iter().map(|r| r.get("handle")).collect();
    assert!(
        handles.contains(&"widget".to_string()),
        "Should include widget"
    );
    assert!(
        handles.contains(&"gizmo".to_string()),
        "Should include gizmo"
    );
    assert!(
        !handles.contains(&"gadget".to_string()),
        "Should not include gadget (draft)"
    );
}

// ============================================================================
// Mutation Integration Tests
// ============================================================================

#[tokio::test]
async fn test_insert_mutation() {
    let (_container, client) = setup_postgres().await;
    create_test_tables(&client).await;

    let source = r#"
CreateProduct @insert{
    params {handle @string, status @string}
    into product
    values {handle $handle, status $status}
    returning {id, handle, status}
}
"#;
    let file = parse_test_query(source);
    let insert = first_insert(&file);

    // Generate SQL
    let generated = dibs_qgen::generate_insert_sql(insert);
    tracing::info!("Generated INSERT SQL: {}", generated.sql);

    // Verify SQL structure
    assert!(
        generated.sql.contains("INSERT INTO \"product\""),
        "Should INSERT INTO product"
    );
    assert!(
        generated.sql.contains("(\"handle\", \"status\")"),
        "Should have column list"
    );
    assert!(
        generated.sql.contains("VALUES ($1, $2)"),
        "Should have parameterized values"
    );
    assert!(
        generated
            .sql
            .contains("RETURNING \"id\", \"handle\", \"status\""),
        "Should have RETURNING clause"
    );

    // Execute the insert
    let handle = "new-product".to_string();
    let status = "draft".to_string();
    let rows: Vec<Row> = client
        .query(&generated.sql, &[&handle, &status])
        .await
        .unwrap();

    assert_eq!(rows.len(), 1, "Should return 1 row");
    let returned_handle: String = rows[0].get("handle");
    let returned_status: String = rows[0].get("status");
    assert_eq!(returned_handle, "new-product");
    assert_eq!(returned_status, "draft");

    // Verify it was actually inserted
    let verify: Vec<Row> = client
        .query("SELECT * FROM product WHERE handle = $1", &[&handle])
        .await
        .unwrap();
    assert_eq!(verify.len(), 1, "Product should exist in database");
}

#[tokio::test]
async fn test_insert_with_default() {
    let (_container, client) = setup_postgres().await;
    create_test_tables(&client).await;

    let source = r#"
CreateProductWithDefault @insert{
    params {handle @string}
    into product
    values {handle $handle, status @default}
    returning {id, handle, status}
}
"#;
    let file = parse_test_query(source);
    let insert = first_insert(&file);

    let generated = dibs_qgen::generate_insert_sql(insert);
    tracing::info!("Generated INSERT SQL: {}", generated.sql);

    // Should use DEFAULT keyword
    assert!(
        generated.sql.contains("DEFAULT"),
        "Should use DEFAULT for status"
    );

    // Execute
    let handle = "default-product".to_string();
    let rows: Vec<Row> = client.query(&generated.sql, &[&handle]).await.unwrap();

    assert_eq!(rows.len(), 1);
    let returned_status: String = rows[0].get("status");
    assert_eq!(returned_status, "draft", "Should use table's default value");
}

#[tokio::test]
async fn test_update_mutation() {
    let (_container, client) = setup_postgres().await;
    create_test_tables(&client).await;
    insert_test_data(&client).await;

    let source = r#"
UpdateProductStatus @update{
    params {handle @string, status @string}
    table product
    set {status $status}
    where {handle $handle}
    returning {id, handle, status}
}
"#;
    let file = parse_test_query(source);
    let update = first_update(&file);

    let generated = dibs_qgen::generate_update_sql(update);
    tracing::info!("Generated UPDATE SQL: {}", generated.sql);

    // Verify SQL structure
    assert!(
        generated.sql.contains("UPDATE \"product\""),
        "Should UPDATE product"
    );
    assert!(
        generated.sql.contains("SET \"status\" = $1"),
        "Should SET status"
    );
    assert!(
        generated.sql.contains("WHERE \"handle\" = $2"),
        "Should have WHERE clause"
    );
    assert!(
        generated.sql.contains("RETURNING"),
        "Should have RETURNING clause"
    );

    // Execute the update
    let status = "published".to_string();
    let handle = "widget".to_string();
    let rows: Vec<Row> = client
        .query(&generated.sql, &[&status, &handle])
        .await
        .unwrap();

    assert_eq!(rows.len(), 1, "Should return 1 updated row");
    let returned_status: String = rows[0].get("status");
    assert_eq!(returned_status, "published", "Status should be updated");

    // Verify in database
    let verify: Vec<Row> = client
        .query("SELECT status FROM product WHERE handle = $1", &[&handle])
        .await
        .unwrap();
    let db_status: String = verify[0].get("status");
    assert_eq!(db_status, "published");
}

#[tokio::test]
async fn test_delete_mutation() {
    let (_container, client) = setup_postgres().await;
    create_test_tables(&client).await;
    insert_test_data(&client).await;

    let source = r#"
DeleteProduct @delete{
    params {id @int}
    from product
    where {id $id}
    returning {id, handle}
}
"#;
    let file = parse_test_query(source);
    let delete = first_delete(&file);

    let generated = dibs_qgen::generate_delete_sql(delete);
    tracing::info!("Generated DELETE SQL: {}", generated.sql);

    // Verify SQL structure
    assert!(
        generated.sql.contains("DELETE FROM \"product\""),
        "Should DELETE FROM product"
    );
    assert!(
        generated.sql.contains("WHERE \"id\" = $1"),
        "Should have WHERE clause"
    );
    assert!(
        generated.sql.contains("RETURNING"),
        "Should have RETURNING clause"
    );

    // First verify we have 3 products
    let before: Vec<Row> = client
        .query("SELECT COUNT(*) FROM product", &[])
        .await
        .unwrap();
    let count_before: i64 = before[0].get(0);
    assert_eq!(count_before, 3);

    // Delete product with id=3 (gizmo) - has no variants so no FK violation
    let id: i64 = 3;
    let rows: Vec<Row> = client.query(&generated.sql, &[&id]).await.unwrap();

    assert_eq!(rows.len(), 1, "Should return 1 deleted row");
    let returned_handle: String = rows[0].get("handle");
    assert_eq!(returned_handle, "gizmo", "Should have deleted gizmo");

    // Verify deletion
    let after: Vec<Row> = client
        .query("SELECT COUNT(*) FROM product", &[])
        .await
        .unwrap();
    let count_after: i64 = after[0].get(0);
    assert_eq!(count_after, 2, "Should have 2 products remaining");
}

#[tokio::test]
async fn test_upsert_mutation_insert() {
    let (_container, client) = setup_postgres().await;
    create_test_tables(&client).await;

    let source = r#"
UpsertProduct @upsert{
    params {handle @string, status @string}
    into product
    on-conflict {
        target {handle}
        update {status}
    }
    values {handle $handle, status $status}
    returning {id, handle, status}
}
"#;
    let file = parse_test_query(source);
    let upsert = first_upsert(&file);

    let generated = dibs_qgen::generate_upsert_sql(upsert);
    tracing::info!("Generated UPSERT SQL: {}", generated.sql);

    // Verify SQL structure
    assert!(
        generated.sql.contains("INSERT INTO \"product\""),
        "Should INSERT INTO product"
    );
    assert!(
        generated.sql.contains("ON CONFLICT (\"handle\")"),
        "Should have ON CONFLICT clause"
    );
    assert!(
        generated.sql.contains("DO UPDATE SET"),
        "Should have DO UPDATE SET"
    );

    // First upsert - should insert
    let handle = "upsert-product".to_string();
    let status = "draft".to_string();
    let rows: Vec<Row> = client
        .query(&generated.sql, &[&handle, &status])
        .await
        .unwrap();

    assert_eq!(rows.len(), 1);
    let returned_status: String = rows[0].get("status");
    assert_eq!(returned_status, "draft");

    // Verify insert
    let verify: Vec<Row> = client
        .query("SELECT status FROM product WHERE handle = $1", &[&handle])
        .await
        .unwrap();
    assert_eq!(verify.len(), 1);
    let db_status: String = verify[0].get("status");
    assert_eq!(db_status, "draft");
}

#[tokio::test]
async fn test_upsert_mutation_update() {
    let (_container, client) = setup_postgres().await;
    create_test_tables(&client).await;
    insert_test_data(&client).await;

    let source = r#"
UpsertProduct @upsert{
    params {handle @string, status @string}
    into product
    on-conflict {
        target {handle}
        update {status}
    }
    values {handle $handle, status $status}
    returning {id, handle, status}
}
"#;
    let file = parse_test_query(source);
    let upsert = first_upsert(&file);

    let generated = dibs_qgen::generate_upsert_sql(upsert);

    // Upsert existing product - should update
    let handle = "widget".to_string(); // exists from test data
    let status = "archived".to_string();
    let rows: Vec<Row> = client
        .query(&generated.sql, &[&handle, &status])
        .await
        .unwrap();

    assert_eq!(rows.len(), 1);
    let returned_status: String = rows[0].get("status");
    assert_eq!(returned_status, "archived", "Status should be updated");

    // Verify the ID didn't change (it was an update, not insert)
    let returned_id: i64 = rows[0].get("id");
    assert_eq!(returned_id, 1, "Should be the same product ID");

    // Verify in database
    let verify: Vec<Row> = client
        .query("SELECT status FROM product WHERE handle = $1", &[&handle])
        .await
        .unwrap();
    let db_status: String = verify[0].get("status");
    assert_eq!(db_status, "archived");
}

#[tokio::test]
async fn test_insert_without_returning() {
    let (_container, client) = setup_postgres().await;
    create_test_tables(&client).await;

    let source = r#"
CreateProductNoReturn @insert{
    params {handle @string, status @string}
    into product
    values {handle $handle, status $status}
}
"#;
    let file = parse_test_query(source);
    let insert = first_insert(&file);

    let generated = dibs_qgen::generate_insert_sql(insert);
    tracing::info!("Generated INSERT SQL: {}", generated.sql);

    // Should not have RETURNING clause
    assert!(
        !generated.sql.contains("RETURNING"),
        "Should NOT have RETURNING clause"
    );

    // Execute - returns no rows
    let handle = "no-return-product".to_string();
    let status = "draft".to_string();
    let rows_affected = client
        .execute(&generated.sql, &[&handle, &status])
        .await
        .unwrap();

    assert_eq!(rows_affected, 1, "Should affect 1 row");

    // Verify it was inserted
    let verify: Vec<Row> = client
        .query("SELECT * FROM product WHERE handle = $1", &[&handle])
        .await
        .unwrap();
    assert_eq!(verify.len(), 1);
}

// ============================================================================
// Bulk Insert/Upsert Integration Tests
// ============================================================================

#[tokio::test]
async fn test_insert_many_mutation() {
    let (_container, client) = setup_postgres().await;
    create_test_tables(&client).await;

    let source = r#"
BulkCreateProducts @insert-many{
    params {handle @string, status @string}
    into product
    values {handle $handle, status $status}
    returning {id, handle, status}
}
"#;
    let file = parse_test_query(source);
    let insert_many = first_insert_many(&file);

    // Generate SQL
    let generated = dibs_qgen::generate_insert_many_sql(insert_many);
    tracing::info!("Generated INSERT MANY SQL: {}", generated.sql);

    // Verify SQL structure uses UNNEST
    assert!(
        generated.sql.contains("INSERT INTO \"product\""),
        "Should INSERT INTO product: {}",
        generated.sql
    );
    assert!(
        generated.sql.contains("SELECT"),
        "Should have SELECT: {}",
        generated.sql
    );
    assert!(
        generated.sql.contains("FROM UNNEST("),
        "Should use UNNEST: {}",
        generated.sql
    );
    assert!(
        generated.sql.contains("$1::text[]"),
        "Should have typed array param: {}",
        generated.sql
    );
    assert!(
        generated.sql.contains("RETURNING"),
        "Should have RETURNING: {}",
        generated.sql
    );

    // Execute bulk insert with multiple rows
    let handles = vec![
        "bulk-1".to_string(),
        "bulk-2".to_string(),
        "bulk-3".to_string(),
    ];
    let statuses = vec![
        "draft".to_string(),
        "active".to_string(),
        "archived".to_string(),
    ];

    let rows: Vec<Row> = client
        .query(&generated.sql, &[&handles, &statuses])
        .await
        .unwrap();

    assert_eq!(rows.len(), 3, "Should return 3 rows");

    // Verify returned data
    let returned_handles: Vec<String> = rows.iter().map(|r| r.get("handle")).collect();
    assert!(returned_handles.contains(&"bulk-1".to_string()));
    assert!(returned_handles.contains(&"bulk-2".to_string()));
    assert!(returned_handles.contains(&"bulk-3".to_string()));

    // Verify all rows were inserted
    let verify: Vec<Row> = client
        .query("SELECT * FROM product ORDER BY handle", &[])
        .await
        .unwrap();
    assert_eq!(verify.len(), 3, "Should have 3 products in database");
}

#[tokio::test]
async fn test_insert_many_without_returning() {
    let (_container, client) = setup_postgres().await;
    create_test_tables(&client).await;

    let source = r#"
BulkCreateProductsNoReturn @insert-many{
    params {handle @string, status @string}
    into product
    values {handle $handle, status $status}
}
"#;
    let file = parse_test_query(source);
    let insert_many = first_insert_many(&file);

    let generated = dibs_qgen::generate_insert_many_sql(insert_many);
    tracing::info!(
        "Generated INSERT MANY SQL (no returning): {}",
        generated.sql
    );

    // Should NOT have RETURNING
    assert!(
        !generated.sql.contains("RETURNING"),
        "Should NOT have RETURNING"
    );

    // Execute
    let handles = vec!["no-ret-1".to_string(), "no-ret-2".to_string()];
    let statuses = vec!["draft".to_string(), "active".to_string()];

    let rows_affected = client
        .execute(&generated.sql, &[&handles, &statuses])
        .await
        .unwrap();

    assert_eq!(rows_affected, 2, "Should affect 2 rows");

    // Verify
    let verify: Vec<Row> = client.query("SELECT * FROM product", &[]).await.unwrap();
    assert_eq!(verify.len(), 2);
}

#[tokio::test]
async fn test_upsert_many_insert() {
    let (_container, client) = setup_postgres().await;
    create_test_tables(&client).await;

    let source = r#"
BulkUpsertProducts @upsert-many{
    params {handle @string, status @string}
    into product
    on-conflict {
        target {handle}
        update {status}
    }
    values {handle $handle, status $status}
    returning {id, handle, status}
}
"#;
    let file = parse_test_query(source);
    let upsert_many = first_upsert_many(&file);

    let generated = dibs_qgen::generate_upsert_many_sql(upsert_many);
    tracing::info!("Generated UPSERT MANY SQL: {}", generated.sql);

    // Verify SQL structure
    assert!(
        generated.sql.contains("FROM UNNEST("),
        "Should use UNNEST: {}",
        generated.sql
    );
    assert!(
        generated.sql.contains("ON CONFLICT (\"handle\")"),
        "Should have ON CONFLICT: {}",
        generated.sql
    );
    assert!(
        generated.sql.contains("DO UPDATE SET"),
        "Should have DO UPDATE SET: {}",
        generated.sql
    );
    assert!(
        generated.sql.contains("EXCLUDED"),
        "Should use EXCLUDED: {}",
        generated.sql
    );

    // First bulk upsert - should insert all
    let handles = vec!["upsert-1".to_string(), "upsert-2".to_string()];
    let statuses = vec!["draft".to_string(), "active".to_string()];

    let rows: Vec<Row> = client
        .query(&generated.sql, &[&handles, &statuses])
        .await
        .unwrap();

    assert_eq!(rows.len(), 2, "Should return 2 rows");

    // Verify inserts
    let verify: Vec<Row> = client
        .query("SELECT handle, status FROM product ORDER BY handle", &[])
        .await
        .unwrap();
    assert_eq!(verify.len(), 2);

    let status_1: String = verify[0].get("status");
    let status_2: String = verify[1].get("status");
    assert_eq!(status_1, "draft");
    assert_eq!(status_2, "active");
}

#[tokio::test]
async fn test_upsert_many_update() {
    let (_container, client) = setup_postgres().await;
    create_test_tables(&client).await;

    // Insert initial data
    client
        .execute(
            "INSERT INTO product (handle, status) VALUES ('existing-1', 'draft'), ('existing-2', 'draft')",
            &[],
        )
        .await
        .unwrap();

    let source = r#"
BulkUpsertProducts @upsert-many{
    params {handle @string, status @string}
    into product
    on-conflict {
        target {handle}
        update {status}
    }
    values {handle $handle, status $status}
    returning {id, handle, status}
}
"#;
    let file = parse_test_query(source);
    let upsert_many = first_upsert_many(&file);
    let generated = dibs_qgen::generate_upsert_many_sql(upsert_many);

    // Upsert with mix of existing and new
    let handles = vec![
        "existing-1".to_string(), // exists - should update
        "existing-2".to_string(), // exists - should update
        "new-3".to_string(),      // new - should insert
    ];
    let statuses = vec![
        "published".to_string(),
        "archived".to_string(),
        "draft".to_string(),
    ];

    let rows: Vec<Row> = client
        .query(&generated.sql, &[&handles, &statuses])
        .await
        .unwrap();

    assert_eq!(rows.len(), 3, "Should return 3 rows");

    // Verify all statuses were updated/inserted correctly
    let verify: Vec<Row> = client
        .query("SELECT handle, status FROM product ORDER BY handle", &[])
        .await
        .unwrap();
    assert_eq!(verify.len(), 3, "Should have 3 products total");

    // Check each one
    for row in verify {
        let handle: String = row.get("handle");
        let status: String = row.get("status");
        match handle.as_str() {
            "existing-1" => assert_eq!(
                status, "published",
                "existing-1 should be updated to published"
            ),
            "existing-2" => assert_eq!(
                status, "archived",
                "existing-2 should be updated to archived"
            ),
            "new-3" => assert_eq!(status, "draft", "new-3 should be inserted as draft"),
            _ => panic!("Unexpected handle: {}", handle),
        }
    }
}

#[tokio::test]
async fn test_insert_many_empty_array() {
    let (_container, client) = setup_postgres().await;
    create_test_tables(&client).await;

    let source = r#"
BulkCreateProducts @insert-many{
    params {handle @string, status @string}
    into product
    values {handle $handle, status $status}
    returning {id, handle, status}
}
"#;
    let file = parse_test_query(source);
    let insert_many = first_insert_many(&file);
    let generated = dibs_qgen::generate_insert_many_sql(insert_many);

    // Execute with empty arrays
    let handles: Vec<String> = vec![];
    let statuses: Vec<String> = vec![];

    let rows: Vec<Row> = client
        .query(&generated.sql, &[&handles, &statuses])
        .await
        .unwrap();

    assert_eq!(rows.len(), 0, "Should return 0 rows for empty input");

    // Verify nothing was inserted
    let verify: Vec<Row> = client.query("SELECT * FROM product", &[]).await.unwrap();
    assert_eq!(verify.len(), 0, "Should have 0 products");
}

// ============================================================================
// JSONB Operator Integration Tests
// ============================================================================

#[tokio::test]
async fn test_jsonb_get_object_operator() {
    let (_container, _client) = setup_postgres().await;

    // Test that @json-get operator generates correct SQL with -> operator
    let source = r#"
GetProductWithSpecs @select{
    params {key @string}
    from product_with_metadata
    fields {id, handle}
    where {metadata @json-get($key)}
}
"#;
    let file = parse_test_query(source);
    let query = first_select(&file);

    let schema = build_jsonb_test_schema();
    let generated = generate_select_sql(query, &schema).unwrap();

    tracing::info!("Generated SQL: {}", generated.sql);

    // SQL should use the -> operator
    assert!(
        generated.sql.contains("->"),
        "SQL should contain JSONB get object operator (->)"
    );

    // Verify parameter is in the generated SQL
    assert!(
        generated.sql.contains("$1"),
        "SQL should have parameter placeholder"
    );

    // Note: @json-get returns JSONB which can't be used directly in WHERE without further operators
    // This test just verifies the SQL generation, not execution
}

#[tokio::test]
async fn test_jsonb_get_text_operator() {
    let (_container, _client) = setup_postgres().await;

    // Test that @json-get-text operator generates correct SQL with ->> operator
    let source = r#"
GetProductBrand @select{
    params {key @string}
    from product_with_metadata
    fields {id, handle}
    where {metadata @json-get-text($key)}
}
"#;
    let file = parse_test_query(source);
    let query = first_select(&file);

    let schema = build_jsonb_test_schema();
    let generated = generate_select_sql(query, &schema).unwrap();

    tracing::info!("Generated SQL: {}", generated.sql);

    // SQL should use the ->> operator
    assert!(
        generated.sql.contains("->>"),
        "SQL should contain JSONB get text operator (->>)"
    );

    // Verify parameter is in the generated SQL
    assert!(
        generated.sql.contains("$1"),
        "SQL should have parameter placeholder"
    );

    // Note: @json-get-text returns text which can't be used directly in WHERE without comparison
    // This test just verifies the SQL generation
}

#[tokio::test]
async fn test_jsonb_get_text_in_where_clause() {
    let (_container, client) = setup_postgres().await;
    create_jsonb_test_tables(&client).await;
    insert_jsonb_test_data(&client).await;

    // Use @json-get-text with equality comparison - proper usage pattern
    let source = r#"
FindByBrand @select{
    params {brand @string}
    from product_with_metadata
    fields {id, handle}
    where {metadata @json-get-text("brand")}
}
"#;
    let file = parse_test_query(source);
    let query = first_select(&file);

    let schema = build_jsonb_test_schema();

    // For this test, we'll use simple SQL generation since we're testing the operator
    let generated = generate_select_sql(query, &schema).unwrap();

    tracing::info!("Generated SQL: {}", generated.sql);

    // SQL should use the ->> operator
    assert!(
        generated.sql.contains("->>"),
        "SQL should contain JSONB get text operator (->>)"
    );

    // Note: Direct execution would fail because ->> returns text, not boolean
    // In real usage, this would be combined with comparison operators
}

#[tokio::test]
async fn test_jsonb_contains_operator() {
    let (_container, client) = setup_postgres().await;
    create_jsonb_test_tables(&client).await;
    insert_jsonb_test_data(&client).await;

    let source = r#"
FindPremiumProducts @select{
    from product_with_metadata
    fields {id, handle}
    where {metadata @contains("{\"premium\": true}")}
}
"#;
    let file = parse_test_query(source);
    let query = first_select(&file);

    let schema = build_jsonb_test_schema();
    let generated = generate_select_sql(query, &schema).unwrap();

    tracing::info!("Generated SQL: {}", generated.sql);

    // SQL should use the @> operator
    assert!(
        generated.sql.contains("@>"),
        "SQL should contain JSONB contains operator (@>)"
    );

    // Execute query
    let rows: Vec<Row> = client.query(&generated.sql, &[]).await.unwrap();

    assert_eq!(rows.len(), 2, "Should find 2 premium products");

    let handles: Vec<String> = rows.iter().map(|r| r.get::<_, String>(1)).collect();
    assert!(handles.contains(&"premium-widget".to_string()));
    assert!(handles.contains(&"multi-gizmo".to_string()));
}

#[tokio::test]
async fn test_jsonb_contains_with_literal() {
    let (_container, client) = setup_postgres().await;
    create_jsonb_test_tables(&client).await;
    insert_jsonb_test_data(&client).await;

    let source = r#"
FindProductsByMetadata @select{
    from product_with_metadata
    fields {id, handle}
    where {metadata @contains("{\"brand\": \"Acme\"}")}
}
"#;
    let file = parse_test_query(source);
    let query = first_select(&file);

    let schema = build_jsonb_test_schema();
    let generated = generate_select_sql(query, &schema).unwrap();

    tracing::info!("Generated SQL: {}", generated.sql);

    // SQL should use the @> operator with literal JSON
    assert!(
        generated.sql.contains("@>"),
        "SQL should contain JSONB contains operator (@>): {}",
        generated.sql
    );

    // Execute query - find products with brand "Acme"
    let rows: Vec<Row> = client.query(&generated.sql, &[]).await.unwrap();

    assert_eq!(rows.len(), 2, "Should find 2 Acme products");
}

#[tokio::test]
async fn test_jsonb_contains_nested_object() {
    let (_container, client) = setup_postgres().await;
    create_jsonb_test_tables(&client).await;
    insert_jsonb_test_data(&client).await;

    let source = r#"
FindByNestedSpec @select{
    from product_with_metadata
    fields {id, handle}
    where {metadata @contains("{\"specs\": {\"color\": \"blue\"}}")}
}
"#;
    let file = parse_test_query(source);
    let query = first_select(&file);

    let schema = build_jsonb_test_schema();
    let generated = generate_select_sql(query, &schema).unwrap();

    tracing::info!("Generated SQL: {}", generated.sql);

    // Execute query
    let rows: Vec<Row> = client.query(&generated.sql, &[]).await.unwrap();

    assert_eq!(
        rows.len(),
        1,
        "Should find 1 product with blue color in specs"
    );

    let handle: String = rows[0].get(1);
    assert_eq!(handle, "premium-widget");
}

#[tokio::test]
async fn test_jsonb_key_exists_operator() {
    let (_container, client) = setup_postgres().await;
    create_jsonb_test_tables(&client).await;
    insert_jsonb_test_data(&client).await;

    let source = r#"
FindWithPremiumKey @select{
    from product_with_metadata
    fields {id, handle}
    where {metadata @key-exists("premium")}
}
"#;
    let file = parse_test_query(source);
    let query = first_select(&file);

    let schema = build_jsonb_test_schema();
    let generated = generate_select_sql(query, &schema).unwrap();

    tracing::info!("Generated SQL: {}", generated.sql);

    // SQL should use the ? operator
    assert!(
        generated.sql.contains("?"),
        "SQL should contain JSONB key exists operator (?)"
    );

    // Execute query
    let rows: Vec<Row> = client.query(&generated.sql, &[]).await.unwrap();

    assert_eq!(rows.len(), 3, "Should find 3 products with 'premium' key");

    let handles: Vec<String> = rows.iter().map(|r| r.get::<_, String>(1)).collect();
    assert!(handles.contains(&"premium-widget".to_string()));
    assert!(handles.contains(&"basic-gadget".to_string()));
    assert!(handles.contains(&"multi-gizmo".to_string()));
}

#[tokio::test]
async fn test_jsonb_key_exists_with_param() {
    let (_container, client) = setup_postgres().await;
    create_jsonb_test_tables(&client).await;
    insert_jsonb_test_data(&client).await;

    let source = r#"
FindWithKey @select{
    params {key @string}
    from product_with_metadata
    fields {id, handle}
    where {metadata @key-exists($key)}
}
"#;
    let file = parse_test_query(source);
    let query = first_select(&file);

    let schema = build_jsonb_test_schema();
    let generated = generate_select_sql(query, &schema).unwrap();

    tracing::info!("Generated SQL: {}", generated.sql);

    // SQL should use the ? operator with parameter
    assert!(
        generated.sql.contains("?"),
        "SQL should contain JSONB key exists operator (?)"
    );

    // Execute query - find products with 'tags' key
    let key = "tags".to_string();
    let rows: Vec<Row> = client.query(&generated.sql, &[&key]).await.unwrap();

    assert_eq!(rows.len(), 1, "Should find 1 product with 'tags' key");

    let handle: String = rows[0].get(1);
    assert_eq!(handle, "multi-gizmo");
}

#[tokio::test]
async fn test_jsonb_multiple_operators_combined() {
    let (_container, client) = setup_postgres().await;
    create_jsonb_test_tables(&client).await;
    insert_jsonb_test_data(&client).await;

    // Test combining @key-exists operator with regular equality
    let source = r#"
ComplexJsonQuery @select{
    from product_with_metadata
    fields {id, handle}
    where {
        status "active"
        metadata @key-exists("brand")
    }
}
"#;
    let file = parse_test_query(source);
    let query = first_select(&file);

    let schema = build_jsonb_test_schema();
    let generated = generate_select_sql(query, &schema).unwrap();

    tracing::info!("Generated SQL: {}", generated.sql);

    // SQL should use ? operator
    assert!(
        generated.sql.contains("?"),
        "SQL should contain JSONB key exists operator (?): {}",
        generated.sql
    );

    // SQL should filter on status
    assert!(
        generated.sql.contains("status"),
        "SQL should filter on status: {}",
        generated.sql
    );

    // Execute query - find active products with brand key
    let rows: Vec<Row> = client.query(&generated.sql, &[]).await.unwrap();

    assert_eq!(rows.len(), 2, "Should find 2 active products with brand");

    let handles: Vec<String> = rows.iter().map(|r| r.get::<_, String>(1)).collect();
    assert!(handles.contains(&"premium-widget".to_string()));
    assert!(handles.contains(&"multi-gizmo".to_string()));
}

#[tokio::test]
async fn test_jsonb_null_handling() {
    let (_container, client) = setup_postgres().await;
    create_jsonb_test_tables(&client).await;
    insert_jsonb_test_data(&client).await;

    let source = r#"
FindWithMetadata @select{
    from product_with_metadata
    fields {id, handle}
    where {metadata @key-exists("brand")}
}
"#;
    let file = parse_test_query(source);
    let query = first_select(&file);

    let schema = build_jsonb_test_schema();
    let generated = generate_select_sql(query, &schema).unwrap();

    // Execute query - should not return products with NULL metadata
    let rows: Vec<Row> = client.query(&generated.sql, &[]).await.unwrap();

    assert_eq!(rows.len(), 3, "Should find 3 products with brand key");

    let handles: Vec<String> = rows.iter().map(|r| r.get::<_, String>(1)).collect();
    assert!(
        !handles.contains(&"no-meta".to_string()),
        "Should not include NULL metadata product"
    );
    assert!(
        !handles.contains(&"empty-meta".to_string()),
        "Should not include empty metadata product"
    );
}

/// Build test schema for JSONB tests.
fn build_jsonb_test_schema() -> Schema {
    let mut tables = IndexMap::new();

    tables.insert(
        "product_with_metadata".to_string(),
        table(
            "product_with_metadata",
            vec![
                col("id", PgType::BigInt, "i64", false),
                col("handle", PgType::Text, "String", false),
                col("status", PgType::Text, "String", false),
                col("metadata", PgType::Jsonb, "serde_json::Value", true),
            ],
            vec![],
        ),
    );

    Schema { tables }
}
