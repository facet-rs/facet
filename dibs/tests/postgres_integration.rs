//! Integration tests using testcontainers with Postgres 18.

use dibs::{Schema, Table};
use facet::Facet;
use testcontainers::{ImageExt, runners::AsyncRunner};
use testcontainers_modules::postgres::Postgres;
use tokio_postgres::NoTls;

// Test table definitions
#[derive(Facet)]
#[facet(derive(dibs::Table), dibs::table = "test_users")]
struct TestUser {
    #[facet(dibs::pk)]
    id: i64,

    #[facet(dibs::unique)]
    email: String,

    #[facet(dibs::index)]
    name: String,

    bio: Option<String>,

    #[facet(dibs::default = "0")]
    created_at: i64,
}

#[derive(Facet)]
#[facet(derive(dibs::Table), dibs::table = "test_posts")]
struct TestPost {
    #[facet(dibs::pk)]
    id: i64,

    title: String,

    body: String,

    #[facet(dibs::fk = "test_users.id", dibs::index)]
    author_id: i64,
}

// Helper to create a minimal column for tests
fn test_column(name: &str, pg_type: dibs::PgType, nullable: bool, primary_key: bool, unique: bool) -> dibs::Column {
    dibs::Column {
        name: name.to_string(),
        pg_type,
        rust_type: None,
        nullable,
        default: None,
        primary_key,
        unique,
        auto_generated: false,
        long: false,
        label: false,
        enum_variants: vec![],
        doc: None,
        lang: None,
        icon: None,
        subtype: None,
    }
}

fn test_column_with_default(name: &str, pg_type: dibs::PgType, default: &str) -> dibs::Column {
    dibs::Column {
        name: name.to_string(),
        pg_type,
        rust_type: None,
        nullable: false,
        default: Some(default.to_string()),
        primary_key: false,
        unique: false,
        auto_generated: false,
        long: false,
        label: false,
        enum_variants: vec![],
        doc: None,
        lang: None,
        icon: None,
        subtype: None,
    }
}

fn test_table(name: &str, columns: Vec<dibs::Column>, foreign_keys: Vec<dibs::ForeignKey>, indices: Vec<dibs::Index>) -> dibs::Table {
    dibs::Table {
        name: name.to_string(),
        columns,
        foreign_keys,
        indices,
        source: dibs::SourceLocation { file: None, line: None, column: None },
        doc: None,
        icon: None,
    }
}

async fn create_postgres_container() -> (
    testcontainers::ContainerAsync<Postgres>,
    tokio_postgres::Client,
) {
    let container = Postgres::default()
        .with_tag("18")
        .start()
        .await
        .expect("Failed to start Postgres container");

    let host = container.get_host().await.unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();

    let connection_string = format!(
        "host={} port={} user=postgres password=postgres dbname=postgres",
        host, port
    );

    let (client, connection) = tokio_postgres::connect(&connection_string, NoTls)
        .await
        .expect("Failed to connect to Postgres");

    // Spawn connection handler
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("Connection error: {}", e);
        }
    });

    (container, client)
}

#[tokio::test]
async fn test_schema_collection() {
    // Just test that schema collection works
    let schema = Schema::collect();

    // Should have our test tables
    let table_names: Vec<_> = schema.tables.iter().map(|t| t.name.as_str()).collect();
    assert!(
        table_names.contains(&"test_users"),
        "Expected test_users table, got: {:?}",
        table_names
    );
    assert!(
        table_names.contains(&"test_posts"),
        "Expected test_posts table, got: {:?}",
        table_names
    );
}

#[tokio::test]
async fn test_sql_generation() {
    let schema = Schema::collect();
    let sql = schema.to_sql();

    // Check that SQL contains expected statements (identifiers are quoted)
    assert!(sql.contains("CREATE TABLE \"test_users\""));
    assert!(sql.contains("CREATE TABLE \"test_posts\""));
    assert!(sql.contains("PRIMARY KEY"));
    assert!(sql.contains("NOT NULL"));
    assert!(sql.contains("UNIQUE"));
    assert!(sql.contains("FOREIGN KEY"));
    assert!(sql.contains("CREATE INDEX"));
}

#[tokio::test]
async fn test_execute_schema_on_postgres() {
    let (_container, client) = create_postgres_container().await;

    let schema = Schema::collect();

    // Filter to only our test tables
    let test_tables: Vec<&Table> = schema
        .tables
        .iter()
        .filter(|t| t.name.starts_with("test_"))
        .collect();

    // Create tables
    for table in &test_tables {
        let sql = table.to_create_table_sql();
        client
            .batch_execute(&sql)
            .await
            .unwrap_or_else(|e| panic!("Failed to create table {}: {e}", table.name));
    }

    // Add foreign keys
    for table in &test_tables {
        for fk in &table.foreign_keys {
            let sql = format!(
                "ALTER TABLE {} ADD CONSTRAINT fk_{}_{} FOREIGN KEY ({}) REFERENCES {}({})",
                table.name,
                table.name,
                fk.columns.join("_"),
                fk.columns.join(", "),
                fk.references_table,
                fk.references_columns.join(", ")
            );
            client
                .batch_execute(&sql)
                .await
                .unwrap_or_else(|e| panic!("Failed to add FK on {}: {e}", table.name));
        }
    }

    // Create indices
    for table in &test_tables {
        for idx in &table.indices {
            let sql = table.to_create_index_sql(idx);
            client
                .batch_execute(&sql)
                .await
                .unwrap_or_else(|e| panic!("Failed to create index {}: {e}", idx.name));
        }
    }

    // Verify tables exist by querying information_schema
    let rows = client
        .query(
            "SELECT table_name FROM information_schema.tables WHERE table_schema = 'public' ORDER BY table_name",
            &[],
        )
        .await
        .expect("Failed to query tables");

    let table_names: Vec<String> = rows.iter().map(|r| r.get(0)).collect();
    assert!(table_names.contains(&"test_users".to_string()));
    assert!(table_names.contains(&"test_posts".to_string()));

    // Verify columns exist
    let rows = client
        .query(
            "SELECT column_name, data_type, is_nullable FROM information_schema.columns WHERE table_name = 'test_users' ORDER BY ordinal_position",
            &[],
        )
        .await
        .expect("Failed to query columns");

    let columns: Vec<(String, String, String)> = rows
        .iter()
        .map(|r| (r.get(0), r.get(1), r.get(2)))
        .collect();

    // Check id column
    assert!(columns.iter().any(|(name, _, _)| name == "id"));
    // Check email column
    assert!(columns.iter().any(|(name, _, _)| name == "email"));
    // Check bio is nullable
    assert!(
        columns
            .iter()
            .any(|(name, _, nullable)| name == "bio" && nullable == "YES")
    );

    // Verify foreign key exists
    let rows = client
        .query(
            "SELECT constraint_name FROM information_schema.table_constraints WHERE table_name = 'test_posts' AND constraint_type = 'FOREIGN KEY'",
            &[],
        )
        .await
        .expect("Failed to query constraints");

    assert!(
        !rows.is_empty(),
        "Expected foreign key constraint on test_posts"
    );

    // Verify index exists
    let rows = client
        .query(
            "SELECT indexname FROM pg_indexes WHERE tablename = 'test_users' AND indexname LIKE 'idx_%'",
            &[],
        )
        .await
        .expect("Failed to query indexes");

    assert!(!rows.is_empty(), "Expected index on test_users");
}

#[tokio::test]
async fn test_insert_and_query_data() {
    let (_container, client) = create_postgres_container().await;

    let schema = Schema::collect();

    // Filter to only our test tables and create them
    let test_tables: Vec<&Table> = schema
        .tables
        .iter()
        .filter(|t| t.name.starts_with("test_"))
        .collect();

    for table in &test_tables {
        client
            .batch_execute(&table.to_create_table_sql())
            .await
            .unwrap();
    }

    // Add foreign keys
    for table in &test_tables {
        for fk in &table.foreign_keys {
            let sql = format!(
                "ALTER TABLE {} ADD CONSTRAINT fk_{}_{} FOREIGN KEY ({}) REFERENCES {}({})",
                table.name,
                table.name,
                fk.columns.join("_"),
                fk.columns.join(", "),
                fk.references_table,
                fk.references_columns.join(", ")
            );
            client.batch_execute(&sql).await.unwrap();
        }
    }

    // Insert a user
    client
        .execute(
            "INSERT INTO test_users (id, email, name, bio, created_at) VALUES ($1, $2, $3, $4, $5)",
            &[
                &1i64,
                &"alice@example.com",
                &"Alice",
                &Some("Hello!"),
                &1234567890i64,
            ],
        )
        .await
        .expect("Failed to insert user");

    // Insert a post
    client
        .execute(
            "INSERT INTO test_posts (id, title, body, author_id) VALUES ($1, $2, $3, $4)",
            &[&1i64, &"First Post", &"Hello World", &1i64],
        )
        .await
        .expect("Failed to insert post");

    // Query back
    let rows = client
        .query(
            "SELECT id, email, name FROM test_users WHERE id = $1",
            &[&1i64],
        )
        .await
        .expect("Failed to query user");

    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    let email: &str = row.get(1);
    let name: &str = row.get(2);
    assert_eq!(email, "alice@example.com");
    assert_eq!(name, "Alice");

    // Query with join
    let rows = client
        .query(
            "SELECT p.title, u.name FROM test_posts p JOIN test_users u ON p.author_id = u.id",
            &[],
        )
        .await
        .expect("Failed to query with join");

    assert_eq!(rows.len(), 1);
    let title: &str = rows[0].get(0);
    let author: &str = rows[0].get(1);
    assert_eq!(title, "First Post");
    assert_eq!(author, "Alice");
}

#[tokio::test]
async fn test_diff_rust_vs_database() {
    let (_container, client) = create_postgres_container().await;

    // Create a database schema that differs from our Rust schema
    // Our Rust schema has test_users with: id, email (unique), name (indexed), bio (nullable), created_at
    // Let's create a DB schema that's missing the 'bio' column and has an extra 'legacy_field'
    client
        .batch_execute(
            r#"
            CREATE TABLE test_users (
                id BIGINT PRIMARY KEY,
                email TEXT NOT NULL UNIQUE,
                name TEXT NOT NULL,
                created_at BIGINT NOT NULL DEFAULT 0,
                legacy_field TEXT
            );

            CREATE INDEX idx_test_users_name ON test_users(name);
            "#,
        )
        .await
        .expect("Failed to create tables");

    // Introspect the database
    let db_schema = dibs::Schema::from_database(&client)
        .await
        .expect("Failed to introspect schema");

    // Get the Rust schema (collected from test structs in this file)
    let rust_schema = dibs::Schema::collect();

    // Find just the test_users table in the Rust schema
    let rust_test_users = rust_schema
        .tables
        .iter()
        .find(|t| t.name == "test_users")
        .expect("test_users should be in Rust schema");

    // Create a schema with just test_users for comparison
    let rust_schema_subset = dibs::Schema {
        tables: vec![rust_test_users.clone()],
    };

    // Diff: what changes are needed to make DB match Rust?
    let diff = rust_schema_subset.diff(&db_schema);

    assert!(!diff.is_empty(), "Should detect differences");

    // Find the test_users diff
    let users_diff = diff
        .table_diffs
        .iter()
        .find(|d| d.table == "test_users")
        .expect("Should have diff for test_users");

    // The diff algorithm uses rename heuristics: when it sees a column in DB (legacy_field)
    // with the same type and nullability as a column in Rust (bio), it suggests a rename
    // instead of add+drop. This is the correct behavior.
    let has_rename = users_diff
        .changes
        .iter()
        .any(|c| matches!(c, dibs::Change::RenameColumn { from, to } if from == "legacy_field" && to == "bio"));

    assert!(
        has_rename,
        "Should detect rename from 'legacy_field' to 'bio'. Got: {}",
        diff
    );
}

#[tokio::test]
async fn test_diff_no_changes() {
    let (_container, client) = create_postgres_container().await;

    // Create a table that exactly matches our Rust definition
    client
        .batch_execute(
            r#"
            CREATE TABLE test_users (
                id BIGINT PRIMARY KEY,
                email TEXT NOT NULL UNIQUE,
                name TEXT NOT NULL,
                bio TEXT,
                created_at BIGINT NOT NULL DEFAULT 0
            );

            CREATE INDEX idx_test_users_name ON test_users(name);
            "#,
        )
        .await
        .expect("Failed to create tables");

    let db_schema = dibs::Schema::from_database(&client)
        .await
        .expect("Failed to introspect schema");

    let rust_schema = dibs::Schema::collect();
    let rust_test_users = rust_schema
        .tables
        .iter()
        .find(|t| t.name == "test_users")
        .expect("test_users should be in Rust schema");

    let rust_schema_subset = dibs::Schema {
        tables: vec![rust_test_users.clone()],
    };

    let diff = rust_schema_subset.diff(&db_schema);

    // The schemas should match (no changes needed)
    assert!(
        diff.is_empty(),
        "Should detect no changes when schemas match. Got: {}",
        diff
    );
}

#[tokio::test]
async fn test_introspect_schema_from_database() {
    let (_container, client) = create_postgres_container().await;

    // First create some tables
    client
        .batch_execute(
            r#"
            CREATE TABLE users (
                id BIGINT PRIMARY KEY,
                email TEXT NOT NULL UNIQUE,
                name TEXT NOT NULL,
                bio TEXT,
                created_at BIGINT NOT NULL DEFAULT 0
            );

            CREATE TABLE posts (
                id BIGINT PRIMARY KEY,
                title TEXT NOT NULL,
                body TEXT NOT NULL,
                author_id BIGINT NOT NULL REFERENCES users(id)
            );

            CREATE INDEX idx_posts_author_id ON posts(author_id);
            CREATE INDEX idx_users_name ON users(name);
            "#,
        )
        .await
        .expect("Failed to create tables");

    // Now introspect
    let schema = Schema::from_database(&client)
        .await
        .expect("Failed to introspect schema");

    // Should have 2 tables
    assert_eq!(schema.tables.len(), 2);

    // Find users table
    let users = schema.tables.iter().find(|t| t.name == "users").unwrap();
    assert_eq!(users.columns.len(), 5);

    // Check id column
    let id_col = users.columns.iter().find(|c| c.name == "id").unwrap();
    assert!(id_col.primary_key);
    assert_eq!(id_col.pg_type, dibs::PgType::BigInt);
    assert!(!id_col.nullable);

    // Check email column
    let email_col = users.columns.iter().find(|c| c.name == "email").unwrap();
    assert!(email_col.unique);
    assert!(!email_col.nullable);

    // Check bio column (nullable)
    let bio_col = users.columns.iter().find(|c| c.name == "bio").unwrap();
    assert!(bio_col.nullable);
    assert!(!bio_col.unique);

    // Check index
    assert!(users.indices.iter().any(|i| i.name == "idx_users_name"));

    // Find posts table
    let posts = schema.tables.iter().find(|t| t.name == "posts").unwrap();

    // Check foreign key
    assert_eq!(posts.foreign_keys.len(), 1);
    let fk = &posts.foreign_keys[0];
    assert_eq!(fk.references_table, "users");
    assert_eq!(fk.references_columns, vec!["id"]);

    // Check index
    assert!(
        posts
            .indices
            .iter()
            .any(|i| i.name == "idx_posts_author_id")
    );
}

#[tokio::test]
async fn test_table_rename_execution() {
    let (_container, client) = create_postgres_container().await;

    // Create initial schema with plural table names
    client
        .batch_execute(
            r#"
            CREATE TABLE users (
                id BIGINT PRIMARY KEY,
                email TEXT NOT NULL UNIQUE,
                name TEXT NOT NULL
            );

            CREATE TABLE posts (
                id BIGINT PRIMARY KEY,
                title TEXT NOT NULL,
                author_id BIGINT NOT NULL REFERENCES users(id)
            );

            CREATE INDEX idx_posts_author_id ON posts(author_id);
            "#,
        )
        .await
        .expect("Failed to create initial schema");

    // Introspect current DB state
    let db_schema = dibs::Schema::from_database(&client)
        .await
        .expect("Failed to introspect");

    // Define desired schema with singular names
    let desired = dibs::Schema {
        tables: vec![
            test_table(
                "user",
                vec![
                    test_column("id", dibs::PgType::BigInt, false, true, false),
                    test_column("email", dibs::PgType::Text, false, false, true),
                    test_column("name", dibs::PgType::Text, false, false, false),
                ],
                vec![],
                vec![],
            ),
            test_table(
                "post",
                vec![
                    test_column("id", dibs::PgType::BigInt, false, true, false),
                    test_column("title", dibs::PgType::Text, false, false, false),
                    test_column("author_id", dibs::PgType::BigInt, false, false, false),
                ],
                vec![dibs::ForeignKey {
                    columns: vec!["author_id".to_string()],
                    references_table: "user".to_string(),
                    references_columns: vec!["id".to_string()],
                }],
                vec![dibs::Index {
                    name: "idx_post_author_id".to_string(),
                    columns: vec!["author_id".to_string()],
                    unique: false,
                }],
            ),
        ],
    };

    // Generate diff
    let diff = desired.diff(&db_schema);
    println!("Diff:\n{}", diff);

    // Should detect renames, not add/drop
    assert!(
        diff.table_diffs.iter().any(|td| td.changes.iter().any(|c| {
            matches!(c, dibs::Change::RenameTable { from, to } if from == "users" && to == "user")
        })),
        "Should detect users -> user rename"
    );
    assert!(
        diff.table_diffs.iter().any(|td| td.changes.iter().any(|c| {
            matches!(c, dibs::Change::RenameTable { from, to } if from == "posts" && to == "post")
        })),
        "Should detect posts -> post rename"
    );

    // Generate and execute migration SQL using the solver
    let sql = diff.to_sql();
    println!("Migration SQL:\n{}", sql);

    client
        .batch_execute(&sql)
        .await
        .expect("Failed to execute rename migration");

    // Verify tables were renamed
    let rows = client
        .query(
            "SELECT table_name FROM information_schema.tables WHERE table_schema = 'public' ORDER BY table_name",
            &[],
        )
        .await
        .expect("Failed to query tables");

    let table_names: Vec<String> = rows.iter().map(|r| r.get(0)).collect();
    assert!(
        table_names.contains(&"user".to_string()),
        "Expected 'user' table, got: {:?}",
        table_names
    );
    assert!(
        table_names.contains(&"post".to_string()),
        "Expected 'post' table, got: {:?}",
        table_names
    );
    assert!(
        !table_names.contains(&"users".to_string()),
        "Old 'users' table should not exist"
    );
    assert!(
        !table_names.contains(&"posts".to_string()),
        "Old 'posts' table should not exist"
    );

    // Verify FK still works by inserting data
    client
        .execute(
            "INSERT INTO \"user\" (id, email, name) VALUES ($1, $2, $3)",
            &[&1i64, &"alice@example.com", &"Alice"],
        )
        .await
        .expect("Failed to insert user");

    client
        .execute(
            "INSERT INTO post (id, title, author_id) VALUES ($1, $2, $3)",
            &[&1i64, &"Hello World", &1i64],
        )
        .await
        .expect("Failed to insert post");

    // Verify FK constraint is enforced
    let result = client
        .execute(
            "INSERT INTO post (id, title, author_id) VALUES ($1, $2, $3)",
            &[&2i64, &"Bad Post", &999i64], // Invalid author_id
        )
        .await;
    assert!(result.is_err(), "FK constraint should reject invalid author_id");
}

#[tokio::test]
async fn test_column_type_change() {
    let (_container, client) = create_postgres_container().await;

    // Create table with INTEGER column
    client
        .batch_execute(
            r#"
            CREATE TABLE products (
                id BIGINT PRIMARY KEY,
                name TEXT NOT NULL,
                price INTEGER NOT NULL
            );
            "#,
        )
        .await
        .expect("Failed to create table");

    // Insert some data
    client
        .execute(
            "INSERT INTO products (id, name, price) VALUES ($1, $2, $3)",
            &[&1i64, &"Widget", &100i32],
        )
        .await
        .expect("Failed to insert");

    // Introspect
    let db_schema = dibs::Schema::from_database(&client)
        .await
        .expect("Failed to introspect");

    // Desired schema with BIGINT price
    let desired = dibs::Schema {
        tables: vec![test_table(
            "products",
            vec![
                test_column("id", dibs::PgType::BigInt, false, true, false),
                test_column("name", dibs::PgType::Text, false, false, false),
                test_column("price", dibs::PgType::BigInt, false, false, false), // Changed from Integer
            ],
            vec![],
            vec![],
        )],
    };

    let diff = desired.diff(&db_schema);
    println!("Diff:\n{}", diff);

    // Should detect type change
    assert!(
        diff.table_diffs.iter().any(|td| td.changes.iter().any(|c| {
            matches!(c, dibs::Change::AlterColumnType { name, .. } if name == "price")
        })),
        "Should detect price type change"
    );

    // Execute migration
    let sql = diff.to_sql();
    println!("Migration SQL:\n{}", sql);

    client
        .batch_execute(&sql)
        .await
        .expect("Failed to execute type change migration");

    // Verify data is preserved
    let rows = client
        .query("SELECT price FROM products WHERE id = $1", &[&1i64])
        .await
        .expect("Failed to query");

    let price: i64 = rows[0].get(0);
    assert_eq!(price, 100, "Data should be preserved after type change");

    // Verify new type accepts larger values
    client
        .execute(
            "INSERT INTO products (id, name, price) VALUES ($1, $2, $3)",
            &[&2i64, &"Big Widget", &10_000_000_000i64],
        )
        .await
        .expect("BIGINT should accept large values");
}

#[tokio::test]
async fn test_add_column_with_default() {
    let (_container, client) = create_postgres_container().await;

    // Create table
    client
        .batch_execute(
            r#"
            CREATE TABLE items (
                id BIGINT PRIMARY KEY,
                name TEXT NOT NULL
            );
            INSERT INTO items (id, name) VALUES (1, 'First');
            "#,
        )
        .await
        .expect("Failed to create table");

    // Introspect
    let db_schema = dibs::Schema::from_database(&client)
        .await
        .expect("Failed to introspect");

    // Desired schema with new column
    let desired = dibs::Schema {
        tables: vec![test_table(
            "items",
            vec![
                test_column("id", dibs::PgType::BigInt, false, true, false),
                test_column("name", dibs::PgType::Text, false, false, false),
                test_column_with_default("quantity", dibs::PgType::Integer, "0"),
            ],
            vec![],
            vec![],
        )],
    };

    let diff = desired.diff(&db_schema);
    println!("Diff:\n{}", diff);

    // Execute migration
    let sql = diff.to_sql();
    println!("Migration SQL:\n{}", sql);

    client
        .batch_execute(&sql)
        .await
        .expect("Failed to execute add column migration");

    // Verify existing row got default value
    let rows = client
        .query("SELECT quantity FROM items WHERE id = $1", &[&1i64])
        .await
        .expect("Failed to query");

    let quantity: i32 = rows[0].get(0);
    assert_eq!(quantity, 0, "Existing row should have default value");
}

#[tokio::test]
async fn test_meta_tables() {
    let (_container, client) = create_postgres_container().await;

    // Create meta tables
    let meta_sql = dibs::create_meta_tables_sql();
    client
        .batch_execute(&meta_sql)
        .await
        .expect("Failed to create meta tables");

    // Verify meta tables exist
    let rows = client
        .query(
            "SELECT table_name FROM information_schema.tables WHERE table_schema = 'public' AND table_name LIKE '__dibs_%' ORDER BY table_name",
            &[],
        )
        .await
        .expect("Failed to query meta tables");

    let table_names: Vec<String> = rows.iter().map(|r| r.get(0)).collect();
    assert!(table_names.contains(&"__dibs_migrations".to_string()));
    assert!(table_names.contains(&"__dibs_tables".to_string()));
    assert!(table_names.contains(&"__dibs_columns".to_string()));
    assert!(table_names.contains(&"__dibs_indices".to_string()));

    // Sync schema to meta tables
    let schema = Schema::collect();
    let sync_sql = dibs::sync_tables_sql(&schema, Some("test_migration"));
    client
        .batch_execute(&sync_sql)
        .await
        .expect("Failed to sync meta tables");

    // Verify table metadata was inserted
    let rows = client
        .query(
            "SELECT table_name, source_file, doc_comment, created_by_migration FROM __dibs_tables ORDER BY table_name",
            &[],
        )
        .await
        .expect("Failed to query __dibs_tables");

    assert!(!rows.is_empty(), "Expected rows in __dibs_tables");

    // Find a test table
    let test_user_row = rows.iter().find(|r| {
        let name: &str = r.get(0);
        name == "test_users"
    });
    assert!(
        test_user_row.is_some(),
        "Expected test_users in __dibs_tables"
    );

    let row = test_user_row.unwrap();
    let source_file: Option<&str> = row.get(1);
    let migration: Option<&str> = row.get(3);

    // Source file should be populated
    assert!(
        source_file.is_some(),
        "Expected source_file to be populated"
    );
    assert!(source_file.unwrap().contains("postgres_integration.rs"));

    // Migration should be recorded
    assert_eq!(migration, Some("test_migration"));

    // Verify column metadata
    let rows = client
        .query(
            "SELECT column_name, sql_type, is_primary_key, is_unique FROM __dibs_columns WHERE table_name = 'test_users' ORDER BY column_name",
            &[],
        )
        .await
        .expect("Failed to query __dibs_columns");

    assert!(!rows.is_empty(), "Expected rows in __dibs_columns");

    // Check id column is marked as primary key
    let id_row = rows.iter().find(|r| {
        let name: &str = r.get(0);
        name == "id"
    });
    assert!(id_row.is_some());
    let is_pk: bool = id_row.unwrap().get(2);
    assert!(is_pk, "Expected id to be primary key");

    // Check email column is marked as unique
    let email_row = rows.iter().find(|r| {
        let name: &str = r.get(0);
        name == "email"
    });
    assert!(email_row.is_some());
    let is_unique: bool = email_row.unwrap().get(3);
    assert!(is_unique, "Expected email to be unique");

    // Record a migration
    let record_sql = dibs::record_migration_sql("test_migration", Some("abc123"), Some(42));
    client
        .batch_execute(&record_sql)
        .await
        .expect("Failed to record migration");

    // Verify migration was recorded
    let rows = client
        .query(
            "SELECT name, checksum, execution_time_ms FROM __dibs_migrations WHERE name = 'test_migration'",
            &[],
        )
        .await
        .expect("Failed to query __dibs_migrations");

    assert_eq!(rows.len(), 1);
    let checksum: Option<&str> = rows[0].get(1);
    let time_ms: Option<i32> = rows[0].get(2);
    assert_eq!(checksum, Some("abc123"));
    assert_eq!(time_ms, Some(42));
}
