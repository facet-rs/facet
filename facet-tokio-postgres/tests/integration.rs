//! Integration tests for facet-tokio-postgres.
//!
//! These tests require the `test-postgres` feature to be enabled.
//! They support two modes:
//! - CI mode: Uses GitHub service container (set POSTGRES_HOST and POSTGRES_PORT env vars)
//! - Local mode: Uses testcontainers to spin up a postgres container (requires docker)

#![cfg(feature = "test-postgres")]

use facet::Facet;
use facet_tokio_postgres::from_row;
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::postgres::Postgres;
use tokio_postgres::NoTls;

/// Holds the postgres connection and optionally the container (for local mode).
/// The container must be kept alive for the duration of the test.
struct PostgresHandle {
    client: tokio_postgres::Client,
    _container: Option<testcontainers::ContainerAsync<Postgres>>,
}

async fn setup_postgres() -> PostgresHandle {
    // Check for CI mode (service container)
    if let (Ok(host), Ok(port)) = (
        std::env::var("POSTGRES_HOST"),
        std::env::var("POSTGRES_PORT"),
    ) {
        let conn_string = format!("host={host} port={port} user=postgres password=postgres");
        let (client, connection) = tokio_postgres::connect(&conn_string, NoTls).await.unwrap();

        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("connection error: {}", e);
            }
        });

        return PostgresHandle {
            client,
            _container: None,
        };
    }

    // Local mode: use testcontainers
    let container = Postgres::default().start().await.unwrap();
    let host = container.get_host().await.unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();

    let conn_string = format!("host={host} port={port} user=postgres password=postgres");
    let (client, connection) = tokio_postgres::connect(&conn_string, NoTls).await.unwrap();

    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("connection error: {}", e);
        }
    });

    PostgresHandle {
        client,
        _container: Some(container),
    }
}

#[tokio::test]
async fn test_basic_struct() {
    #[derive(Debug, Facet, PartialEq)]
    struct User {
        id: i32,
        name: String,
        active: bool,
    }

    let handle = setup_postgres().await;
    let client = &handle.client;

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

    let handle = setup_postgres().await;
    let client = &handle.client;

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

    let handle = setup_postgres().await;
    let client = &handle.client;

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

    let handle = setup_postgres().await;
    let client = &handle.client;

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

    let handle = setup_postgres().await;
    let client = &handle.client;

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

    let handle = setup_postgres().await;
    let client = &handle.client;

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

    let handle = setup_postgres().await;
    let client = &handle.client;

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

    let handle = setup_postgres().await;
    let client = &handle.client;

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

#[cfg(feature = "uuid")]
#[tokio::test]
async fn test_uuid() {
    use uuid::Uuid;

    #[derive(Debug, Facet, PartialEq)]
    struct Product {
        id: Uuid,
        name: String,
    }

    let handle = setup_postgres().await;
    let client = &handle.client;

    client
        .execute("CREATE TABLE products (id UUID, name TEXT)", &[])
        .await
        .unwrap();

    let test_uuid = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
    client
        .execute(
            "INSERT INTO products (id, name) VALUES ($1::text::uuid, 'Widget')",
            &[&test_uuid.to_string()],
        )
        .await
        .unwrap();

    let row = client
        .query_one("SELECT id::text, name FROM products", &[])
        .await
        .unwrap();

    let product: Product = from_row(&row).unwrap();

    assert_eq!(product.id, test_uuid);
    assert_eq!(product.name, "Widget");
}

#[cfg(feature = "uuid")]
#[tokio::test]
async fn test_optional_uuid() {
    use uuid::Uuid;

    #[derive(Debug, Facet, PartialEq)]
    struct OptionalUuid {
        id: i32,
        external_id: Option<Uuid>,
    }

    let handle = setup_postgres().await;
    let client = &handle.client;

    client
        .execute(
            "CREATE TABLE optional_uuids (id INTEGER, external_id UUID)",
            &[],
        )
        .await
        .unwrap();

    // Insert with UUID
    let test_uuid = Uuid::parse_str("a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11").unwrap();
    client
        .execute(
            "INSERT INTO optional_uuids VALUES (1, $1::text::uuid)",
            &[&test_uuid.to_string()],
        )
        .await
        .unwrap();

    // Insert with NULL
    client
        .execute("INSERT INTO optional_uuids VALUES (2, NULL)", &[])
        .await
        .unwrap();

    let rows = client
        .query(
            "SELECT id, external_id::text FROM optional_uuids ORDER BY id",
            &[],
        )
        .await
        .unwrap();

    let with_uuid: OptionalUuid = from_row(&rows[0]).unwrap();
    assert_eq!(with_uuid.id, 1);
    assert_eq!(with_uuid.external_id, Some(test_uuid));

    let without_uuid: OptionalUuid = from_row(&rows[1]).unwrap();
    assert_eq!(without_uuid.id, 2);
    assert_eq!(without_uuid.external_id, None);
}

#[cfg(feature = "jiff02")]
#[tokio::test]
async fn test_timestamp() {
    use jiff::Timestamp;

    #[derive(Debug, Facet, PartialEq)]
    struct Event {
        id: i32,
        created_at: Timestamp,
    }

    let handle = setup_postgres().await;
    let client = &handle.client;

    client
        .execute(
            "CREATE TABLE events (id INTEGER, created_at TIMESTAMPTZ)",
            &[],
        )
        .await
        .unwrap();

    client
        .execute(
            "INSERT INTO events VALUES (1, '2024-06-19T15:22:45Z'::timestamptz)",
            &[],
        )
        .await
        .unwrap();

    let row = client
        .query_one(
            "SELECT id, to_char(created_at, 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') as created_at FROM events",
            &[],
        )
        .await
        .unwrap();

    let event: Event = from_row(&row).unwrap();

    assert_eq!(event.id, 1);
    assert_eq!(event.created_at, "2024-06-19T15:22:45Z".parse().unwrap());
}

#[cfg(feature = "jiff02")]
#[tokio::test]
async fn test_optional_timestamp() {
    use jiff::Timestamp;

    #[derive(Debug, Facet, PartialEq)]
    struct OptionalTimestamp {
        id: i32,
        deleted_at: Option<Timestamp>,
    }

    let handle = setup_postgres().await;
    let client = &handle.client;

    client
        .execute(
            "CREATE TABLE optional_timestamps (id INTEGER, deleted_at TIMESTAMPTZ)",
            &[],
        )
        .await
        .unwrap();

    // Insert with timestamp
    client
        .execute(
            "INSERT INTO optional_timestamps VALUES (1, '2024-01-15T10:30:00Z'::timestamptz)",
            &[],
        )
        .await
        .unwrap();

    // Insert with NULL
    client
        .execute("INSERT INTO optional_timestamps VALUES (2, NULL)", &[])
        .await
        .unwrap();

    let rows = client
        .query(
            "SELECT id, to_char(deleted_at, 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') as deleted_at FROM optional_timestamps ORDER BY id",
            &[],
        )
        .await
        .unwrap();

    let with_timestamp: OptionalTimestamp = from_row(&rows[0]).unwrap();
    assert_eq!(with_timestamp.id, 1);
    assert_eq!(
        with_timestamp.deleted_at,
        Some("2024-01-15T10:30:00Z".parse().unwrap())
    );

    let without_timestamp: OptionalTimestamp = from_row(&rows[1]).unwrap();
    assert_eq!(without_timestamp.id, 2);
    assert_eq!(without_timestamp.deleted_at, None);
}

#[cfg(feature = "chrono")]
#[tokio::test]
async fn test_chrono_datetime_utc() {
    use chrono::{DateTime, Utc};

    #[derive(Debug, Facet, PartialEq)]
    struct ChronoEvent {
        id: i32,
        created_at: DateTime<Utc>,
    }

    let handle = setup_postgres().await;
    let client = &handle.client;

    client
        .execute(
            "CREATE TABLE chrono_events (id INTEGER, created_at TIMESTAMPTZ)",
            &[],
        )
        .await
        .unwrap();

    client
        .execute(
            "INSERT INTO chrono_events VALUES (1, '2024-06-19T15:22:45Z'::timestamptz)",
            &[],
        )
        .await
        .unwrap();

    let row = client
        .query_one(
            "SELECT id, to_char(created_at, 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') as created_at FROM chrono_events",
            &[],
        )
        .await
        .unwrap();

    let event: ChronoEvent = from_row(&row).unwrap();

    assert_eq!(event.id, 1);
    assert_eq!(
        event.created_at,
        "2024-06-19T15:22:45Z".parse::<DateTime<Utc>>().unwrap()
    );
}

#[cfg(feature = "chrono")]
#[tokio::test]
async fn test_chrono_naive_date() {
    use chrono::NaiveDate;

    #[derive(Debug, Facet, PartialEq)]
    struct ChronoDateRecord {
        id: i32,
        birth_date: NaiveDate,
    }

    let handle = setup_postgres().await;
    let client = &handle.client;

    client
        .execute(
            "CREATE TABLE chrono_dates (id INTEGER, birth_date DATE)",
            &[],
        )
        .await
        .unwrap();

    client
        .execute(
            "INSERT INTO chrono_dates VALUES (1, '1990-05-15'::date)",
            &[],
        )
        .await
        .unwrap();

    let row = client
        .query_one(
            "SELECT id, to_char(birth_date, 'YYYY-MM-DD') as birth_date FROM chrono_dates",
            &[],
        )
        .await
        .unwrap();

    let record: ChronoDateRecord = from_row(&row).unwrap();

    assert_eq!(record.id, 1);
    assert_eq!(
        record.birth_date,
        NaiveDate::parse_from_str("1990-05-15", "%Y-%m-%d").unwrap()
    );
}

#[cfg(feature = "chrono")]
#[tokio::test]
async fn test_optional_chrono() {
    use chrono::{DateTime, Utc};

    #[derive(Debug, Facet, PartialEq)]
    struct OptionalChronoTimestamp {
        id: i32,
        updated_at: Option<DateTime<Utc>>,
    }

    let handle = setup_postgres().await;
    let client = &handle.client;

    client
        .execute(
            "CREATE TABLE optional_chrono (id INTEGER, updated_at TIMESTAMPTZ)",
            &[],
        )
        .await
        .unwrap();

    // Insert with timestamp
    client
        .execute(
            "INSERT INTO optional_chrono VALUES (1, '2024-01-15T10:30:00Z'::timestamptz)",
            &[],
        )
        .await
        .unwrap();

    // Insert with NULL
    client
        .execute("INSERT INTO optional_chrono VALUES (2, NULL)", &[])
        .await
        .unwrap();

    let rows = client
        .query(
            "SELECT id, to_char(updated_at, 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') as updated_at FROM optional_chrono ORDER BY id",
            &[],
        )
        .await
        .unwrap();

    let with_timestamp: OptionalChronoTimestamp = from_row(&rows[0]).unwrap();
    assert_eq!(with_timestamp.id, 1);
    assert_eq!(
        with_timestamp.updated_at,
        Some("2024-01-15T10:30:00Z".parse::<DateTime<Utc>>().unwrap())
    );

    let without_timestamp: OptionalChronoTimestamp = from_row(&rows[1]).unwrap();
    assert_eq!(without_timestamp.id, 2);
    assert_eq!(without_timestamp.updated_at, None);
}

#[cfg(feature = "time")]
#[tokio::test]
async fn test_time_offset_datetime() {
    use time::OffsetDateTime;

    #[derive(Debug, Facet, PartialEq)]
    struct TimeEvent {
        id: i32,
        created_at: OffsetDateTime,
    }

    let handle = setup_postgres().await;
    let client = &handle.client;

    client
        .execute(
            "CREATE TABLE time_events (id INTEGER, created_at TIMESTAMPTZ)",
            &[],
        )
        .await
        .unwrap();

    client
        .execute(
            "INSERT INTO time_events VALUES (1, '2024-06-19T15:22:45Z'::timestamptz)",
            &[],
        )
        .await
        .unwrap();

    let row = client
        .query_one(
            "SELECT id, to_char(created_at, 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') as created_at FROM time_events",
            &[],
        )
        .await
        .unwrap();

    let event: TimeEvent = from_row(&row).unwrap();

    assert_eq!(event.id, 1);
    assert_eq!(
        event.created_at,
        OffsetDateTime::parse(
            "2024-06-19T15:22:45Z",
            &time::format_description::well_known::Rfc3339
        )
        .unwrap()
    );
}

#[cfg(feature = "time")]
#[tokio::test]
async fn test_optional_time() {
    use time::OffsetDateTime;

    #[derive(Debug, Facet, PartialEq)]
    struct OptionalTimeTimestamp {
        id: i32,
        modified_at: Option<OffsetDateTime>,
    }

    let handle = setup_postgres().await;
    let client = &handle.client;

    client
        .execute(
            "CREATE TABLE optional_time (id INTEGER, modified_at TIMESTAMPTZ)",
            &[],
        )
        .await
        .unwrap();

    // Insert with timestamp
    client
        .execute(
            "INSERT INTO optional_time VALUES (1, '2024-01-15T10:30:00Z'::timestamptz)",
            &[],
        )
        .await
        .unwrap();

    // Insert with NULL
    client
        .execute("INSERT INTO optional_time VALUES (2, NULL)", &[])
        .await
        .unwrap();

    let rows = client
        .query(
            "SELECT id, to_char(modified_at, 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') as modified_at FROM optional_time ORDER BY id",
            &[],
        )
        .await
        .unwrap();

    let with_timestamp: OptionalTimeTimestamp = from_row(&rows[0]).unwrap();
    assert_eq!(with_timestamp.id, 1);
    assert_eq!(
        with_timestamp.modified_at,
        Some(
            OffsetDateTime::parse(
                "2024-01-15T10:30:00Z",
                &time::format_description::well_known::Rfc3339
            )
            .unwrap()
        )
    );

    let without_timestamp: OptionalTimeTimestamp = from_row(&rows[1]).unwrap();
    assert_eq!(without_timestamp.id, 2);
    assert_eq!(without_timestamp.modified_at, None);
}
