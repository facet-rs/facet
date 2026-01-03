//! Integration tests for facet-tokio-postgres using testcontainers.

use facet::Facet;
use facet_tokio_postgres::from_row;
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::postgres::Postgres;
use tokio_postgres::NoTls;

async fn setup_postgres() -> (
    testcontainers::ContainerAsync<Postgres>,
    tokio_postgres::Client,
) {
    let container = Postgres::default().start().await.unwrap();
    let host = container.get_host().await.unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();

    let conn_string = format!("host={host} port={port} user=postgres password=postgres");
    let (client, connection) = tokio_postgres::connect(&conn_string, NoTls).await.unwrap();

    // Spawn the connection handler
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("connection error: {}", e);
        }
    });

    (container, client)
}

#[tokio::test]
async fn test_basic_struct() {
    #[derive(Debug, Facet, PartialEq)]
    struct User {
        id: i32,
        name: String,
        active: bool,
    }

    let (_container, client) = setup_postgres().await;

    // Create table
    client
        .execute(
            "CREATE TABLE users (id INTEGER, name TEXT, active BOOLEAN)",
            &[],
        )
        .await
        .unwrap();

    // Insert data
    client
        .execute(
            "INSERT INTO users (id, name, active) VALUES (1, 'Alice', true)",
            &[],
        )
        .await
        .unwrap();

    // Query and deserialize
    let row = client
        .query_one("SELECT id, name, active FROM users", &[])
        .await
        .unwrap();

    let user: User = from_row(&row).unwrap();

    assert_eq!(user.id, 1);
    assert_eq!(user.name, "Alice");
    assert!(user.active);
}

#[tokio::test]
async fn test_optional_fields() {
    #[derive(Debug, Facet, PartialEq)]
    struct Person {
        id: i32,
        name: String,
        email: Option<String>,
    }

    let (_container, client) = setup_postgres().await;

    client
        .execute(
            "CREATE TABLE people (id INTEGER, name TEXT, email TEXT)",
            &[],
        )
        .await
        .unwrap();

    // Insert with NULL email
    client
        .execute(
            "INSERT INTO people (id, name, email) VALUES (1, 'Bob', NULL)",
            &[],
        )
        .await
        .unwrap();

    // Insert with email
    client
        .execute(
            "INSERT INTO people (id, name, email) VALUES (2, 'Carol', 'carol@example.com')",
            &[],
        )
        .await
        .unwrap();

    let rows = client
        .query("SELECT id, name, email FROM people ORDER BY id", &[])
        .await
        .unwrap();

    let bob: Person = from_row(&rows[0]).unwrap();
    assert_eq!(bob.id, 1);
    assert_eq!(bob.name, "Bob");
    assert_eq!(bob.email, None);

    let carol: Person = from_row(&rows[1]).unwrap();
    assert_eq!(carol.id, 2);
    assert_eq!(carol.name, "Carol");
    assert_eq!(carol.email, Some("carol@example.com".to_string()));
}

#[tokio::test]
async fn test_numeric_types() {
    #[derive(Debug, Facet, PartialEq)]
    struct Numbers {
        small: i16,
        medium: i32,
        large: i64,
        float32: f32,
        float64: f64,
    }

    let (_container, client) = setup_postgres().await;

    client
        .execute(
            "CREATE TABLE numbers (small SMALLINT, medium INTEGER, large BIGINT, float32 REAL, float64 DOUBLE PRECISION)",
            &[],
        )
        .await
        .unwrap();

    client
        .execute(
            "INSERT INTO numbers VALUES (42, 1000000, 9223372036854775807, 1.5, 2.5)",
            &[],
        )
        .await
        .unwrap();

    let row = client
        .query_one("SELECT * FROM numbers", &[])
        .await
        .unwrap();

    let nums: Numbers = from_row(&row).unwrap();

    assert_eq!(nums.small, 42);
    assert_eq!(nums.medium, 1_000_000);
    assert_eq!(nums.large, 9_223_372_036_854_775_807);
    assert!((nums.float32 - 1.5).abs() < 0.001);
    assert!((nums.float64 - 2.5).abs() < 0.0000001);
}

#[tokio::test]
async fn test_bytea() {
    #[derive(Debug, Facet, PartialEq)]
    struct BinaryData {
        id: i32,
        data: Vec<u8>,
    }

    let (_container, client) = setup_postgres().await;

    client
        .execute("CREATE TABLE binary_data (id INTEGER, data BYTEA)", &[])
        .await
        .unwrap();

    let bytes: &[u8] = &[0xDE, 0xAD, 0xBE, 0xEF];
    client
        .execute("INSERT INTO binary_data VALUES (1, $1)", &[&bytes])
        .await
        .unwrap();

    let row = client
        .query_one("SELECT * FROM binary_data", &[])
        .await
        .unwrap();

    let result: BinaryData = from_row(&row).unwrap();

    assert_eq!(result.id, 1);
    assert_eq!(result.data, vec![0xDE, 0xAD, 0xBE, 0xEF]);
}

#[tokio::test]
async fn test_field_alias() {
    #[derive(Debug, Facet, PartialEq)]
    struct AliasedUser {
        #[facet(rename = "user_id")]
        id: i32,
        #[facet(rename = "user_name")]
        name: String,
    }

    let (_container, client) = setup_postgres().await;

    client
        .execute(
            "CREATE TABLE aliased_users (user_id INTEGER, user_name TEXT)",
            &[],
        )
        .await
        .unwrap();

    client
        .execute("INSERT INTO aliased_users VALUES (42, 'Dave')", &[])
        .await
        .unwrap();

    let row = client
        .query_one("SELECT * FROM aliased_users", &[])
        .await
        .unwrap();

    let user: AliasedUser = from_row(&row).unwrap();

    assert_eq!(user.id, 42);
    assert_eq!(user.name, "Dave");
}

#[tokio::test]
async fn test_missing_column_with_default() {
    #[derive(Debug, Facet, PartialEq)]
    struct WithDefault {
        id: i32,
        #[facet(default)]
        count: i32,
    }

    let (_container, client) = setup_postgres().await;

    client
        .execute("CREATE TABLE with_default (id INTEGER)", &[])
        .await
        .unwrap();

    client
        .execute("INSERT INTO with_default VALUES (1)", &[])
        .await
        .unwrap();

    let row = client
        .query_one("SELECT id FROM with_default", &[])
        .await
        .unwrap();

    let result: WithDefault = from_row(&row).unwrap();

    assert_eq!(result.id, 1);
    assert_eq!(result.count, 0); // Default for i32
}

#[tokio::test]
async fn test_missing_column_with_string_gets_default() {
    // String has Default, so missing columns just get empty string
    #[derive(Debug, Facet, PartialEq)]
    struct WithStringDefault {
        id: i32,
        name: String, // Has Default, will be ""
    }

    let (_container, client) = setup_postgres().await;

    client
        .execute("CREATE TABLE string_default (id INTEGER)", &[])
        .await
        .unwrap();

    client
        .execute("INSERT INTO string_default VALUES (1)", &[])
        .await
        .unwrap();

    let row = client
        .query_one("SELECT id FROM string_default", &[])
        .await
        .unwrap();

    // This succeeds because String has Default
    let result: WithStringDefault = from_row(&row).unwrap();
    assert_eq!(result.id, 1);
    assert_eq!(result.name, ""); // Default empty string
}

#[tokio::test]
async fn test_type_mismatch_errors() {
    #[derive(Debug, Facet)]
    struct TypeMismatch {
        id: i32,
    }

    let (_container, client) = setup_postgres().await;

    client
        .execute("CREATE TABLE type_mismatch (id TEXT)", &[])
        .await
        .unwrap();

    client
        .execute("INSERT INTO type_mismatch VALUES ('not a number')", &[])
        .await
        .unwrap();

    let row = client
        .query_one("SELECT id FROM type_mismatch", &[])
        .await
        .unwrap();

    let result = from_row::<TypeMismatch>(&row);

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("type mismatch"));
}
