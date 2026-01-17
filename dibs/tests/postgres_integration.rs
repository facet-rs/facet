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

    // Check that SQL contains expected statements
    assert!(sql.contains("CREATE TABLE test_users"));
    assert!(sql.contains("CREATE TABLE test_posts"));
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
