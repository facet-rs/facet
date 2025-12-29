//! Advanced #[facet(flatten)] tests - matching facet-solver's sadistic test cases

use facet::Facet;
use facet_json_legacy::{from_str, to_string};
use facet_testhelpers::test;

// ============================================================================
// Three levels of nesting with flatten at each level
// ============================================================================

#[derive(Facet, Debug, PartialEq)]
struct Level3 {
    deep_field: i32,
    another_deep: String,
}

#[derive(Facet, Debug, PartialEq)]
struct Level2 {
    mid_field: i32,
    #[facet(flatten)]
    level3: Level3,
}

#[derive(Facet, Debug, PartialEq)]
struct Level1 {
    top_field: String,
    #[facet(flatten)]
    level2: Level2,
}

#[test]
fn test_three_level_nesting_roundtrip() {
    let original = Level1 {
        top_field: "top".into(),
        level2: Level2 {
            mid_field: 42,
            level3: Level3 {
                deep_field: 100,
                another_deep: "deep".into(),
            },
        },
    };

    let json = to_string(&original);
    // All fields should be flattened to top level
    assert!(json.contains("\"top_field\""));
    assert!(json.contains("\"mid_field\""));
    assert!(json.contains("\"deep_field\""));
    assert!(json.contains("\"another_deep\""));

    let parsed: Level1 = from_str(&json).expect("should parse three-level nested");
    assert_eq!(original, parsed);
}

#[test]
fn test_three_level_nesting_deserialize() {
    let json = r#"{"top_field":"hello","mid_field":1,"deep_field":2,"another_deep":"world"}"#;
    let parsed: Level1 = from_str(json).expect("should deserialize three-level nested");

    assert_eq!(parsed.top_field, "hello");
    assert_eq!(parsed.level2.mid_field, 1);
    assert_eq!(parsed.level2.level3.deep_field, 2);
    assert_eq!(parsed.level2.level3.another_deep, "world");
}

// ============================================================================
// Multiple Enums - Cartesian Product of Configurations
// ============================================================================

#[derive(Facet, Debug, PartialEq)]
struct AuthPassword {
    password: String,
}

#[derive(Facet, Debug, PartialEq)]
struct AuthToken {
    token: String,
    token_expiry: u64,
}

#[allow(dead_code)]
#[derive(Facet, Debug, PartialEq)]
#[repr(C)]
enum AuthMethod {
    Password(AuthPassword),
    Token(AuthToken),
}

#[derive(Facet, Debug, PartialEq)]
struct TransportTcp {
    tcp_port: u16,
}

#[derive(Facet, Debug, PartialEq)]
struct TransportUnix {
    socket_path: String,
}

#[allow(dead_code)]
#[derive(Facet, Debug, PartialEq)]
#[repr(C)]
enum Transport {
    Tcp(TransportTcp),
    Unix(TransportUnix),
}

#[derive(Facet, Debug, PartialEq)]
struct ServiceConfig {
    name: String,
    #[facet(flatten)]
    auth: AuthMethod,
    #[facet(flatten)]
    transport: Transport,
}

#[test]
fn test_multiple_enums_password_tcp() {
    // 2 auth methods × 2 transports = 4 possible configurations
    let json = r#"{"name":"myservice","Password":{"password":"secret"},"Tcp":{"tcp_port":8080}}"#;
    let parsed: ServiceConfig = from_str(json).expect("should parse Password+Tcp");

    assert_eq!(parsed.name, "myservice");
    assert!(
        matches!(parsed.auth, AuthMethod::Password(AuthPassword { ref password }) if password == "secret")
    );
    assert!(matches!(
        parsed.transport,
        Transport::Tcp(TransportTcp { tcp_port: 8080 })
    ));
}

#[test]
fn test_multiple_enums_token_unix() {
    let json = r#"{"name":"myservice","Token":{"token":"abc123","token_expiry":3600},"Unix":{"socket_path":"/tmp/sock"}}"#;
    let parsed: ServiceConfig = from_str(json).expect("should parse Token+Unix");

    assert_eq!(parsed.name, "myservice");
    assert!(
        matches!(parsed.auth, AuthMethod::Token(AuthToken { ref token, token_expiry: 3600 }) if token == "abc123")
    );
    assert!(
        matches!(parsed.transport, Transport::Unix(TransportUnix { ref socket_path }) if socket_path == "/tmp/sock")
    );
}

#[test]
fn test_multiple_enums_roundtrip() {
    let original = ServiceConfig {
        name: "test".into(),
        auth: AuthMethod::Token(AuthToken {
            token: "xyz".into(),
            token_expiry: 7200,
        }),
        transport: Transport::Tcp(TransportTcp { tcp_port: 443 }),
    };

    let json = to_string(&original);
    let parsed: ServiceConfig = from_str(&json).expect("roundtrip should work");
    assert_eq!(original, parsed);
}

// ============================================================================
// u128 support (serde_json can't do this with flatten!)
// ============================================================================

#[derive(Facet, Debug, PartialEq)]
struct LargeNumbers {
    big: u128,
    also_big: i128,
}

#[derive(Facet, Debug, PartialEq)]
struct WrapperWithU128 {
    name: String,
    #[facet(flatten)]
    numbers: LargeNumbers,
}

#[test]
fn test_u128_in_flatten() {
    let original = WrapperWithU128 {
        name: "bignum".into(),
        numbers: LargeNumbers {
            big: 340282366920938463463374607431768211455_u128, // u128::MAX
            also_big: -170141183460469231731687303715884105728_i128, // i128::MIN
        },
    };

    let json = to_string(&original);
    let parsed: WrapperWithU128 = from_str(&json).expect("u128 should work in flatten");
    assert_eq!(original, parsed);
}

// ============================================================================
// Option<Flattened> - all inner fields become optional
// ============================================================================

#[derive(Facet, Debug, PartialEq)]
struct DatabaseConnection {
    db_host: String,
    db_port: u16,
}

#[derive(Facet, Debug, PartialEq)]
struct AppConfigWithOptionalDb {
    name: String,
    #[facet(flatten)]
    database: Option<DatabaseConnection>,
}

#[test]
fn test_optional_flatten_present() {
    let json = r#"{"name":"myapp","db_host":"localhost","db_port":5432}"#;
    let parsed: AppConfigWithOptionalDb = from_str(json).expect("should parse with db");

    assert_eq!(parsed.name, "myapp");
    assert!(parsed.database.is_some());
    let db = parsed.database.unwrap();
    assert_eq!(db.db_host, "localhost");
    assert_eq!(db.db_port, 5432);
}

#[test]
fn test_optional_flatten_absent() {
    // When Option<Flattened>, we can omit all inner fields
    let json = r#"{"name":"myapp"}"#;
    let parsed: AppConfigWithOptionalDb = from_str(json).expect("should parse without db");

    assert_eq!(parsed.name, "myapp");
    assert!(parsed.database.is_none());
}

#[test]
fn test_optional_flatten_roundtrip_some() {
    let original = AppConfigWithOptionalDb {
        name: "app".into(),
        database: Some(DatabaseConnection {
            db_host: "db.example.com".into(),
            db_port: 3306,
        }),
    };

    let json = to_string(&original);
    let parsed: AppConfigWithOptionalDb = from_str(&json).expect("roundtrip Some");
    assert_eq!(original, parsed);
}

#[test]
fn test_optional_flatten_roundtrip_none() {
    let original = AppConfigWithOptionalDb {
        name: "app".into(),
        database: None,
    };

    let json = to_string(&original);
    let parsed: AppConfigWithOptionalDb = from_str(&json).expect("roundtrip None");
    assert_eq!(original, parsed);
}

// ============================================================================
// Different repr types (u8, u16, i32)
// ============================================================================

#[allow(dead_code)]
#[derive(Facet, Debug, PartialEq)]
#[repr(u8)]
enum SmallEnum {
    A { val: i32 },
    B { other: String },
}

#[allow(dead_code)]
#[derive(Facet, Debug, PartialEq)]
#[repr(u16)]
enum MediumEnum {
    X { x_val: f64 },
    Y { y_val: bool },
}

#[derive(Facet, Debug, PartialEq)]
struct MultiRepr {
    #[facet(flatten)]
    small: SmallEnum,
    #[facet(flatten)]
    medium: MediumEnum,
}

#[test]
fn test_different_repr_types() {
    let json = r#"{"A":{"val":42},"X":{"x_val":3.125}}"#;
    let parsed: MultiRepr = from_str(json).expect("should handle different repr types");

    assert!(matches!(parsed.small, SmallEnum::A { val: 42 }));
    assert!(matches!(parsed.medium, MediumEnum::X { x_val } if (x_val - 3.125).abs() < 0.001));
}

// ============================================================================
// DateTime and complex types
// ============================================================================

#[cfg(feature = "jiff")]
mod jiff_tests {
    use super::*;
    use jiff::Timestamp;

    #[derive(Facet, Debug, PartialEq)]
    struct EventData {
        event_time: Timestamp,
        event_name: String,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct LogEntry {
        id: u64,
        #[facet(flatten)]
        event: EventData,
    }

    #[test]
    fn test_datetime_in_flatten() {
        let json = r#"{"id":123,"event_time":"2024-01-15T10:30:00Z","event_name":"test"}"#;
        let parsed: LogEntry = from_str(json).expect("should parse DateTime in flatten");

        assert_eq!(parsed.id, 123);
        assert_eq!(parsed.event.event_name, "test");
    }
}

// ============================================================================
// Overlapping fields - subset resolution
// ============================================================================

#[derive(Facet, Debug, PartialEq)]
struct HttpSource {
    url: String,
}

#[derive(Facet, Debug, PartialEq)]
struct GitSource {
    url: String,
    branch: String,
}

#[allow(dead_code)]
#[derive(Facet, Debug, PartialEq)]
#[repr(u8)]
enum SourceKind {
    Http(HttpSource),
    Git(GitSource),
}

#[derive(Facet, Debug, PartialEq)]
struct Source {
    name: String,
    #[facet(flatten)]
    kind: SourceKind,
}

#[test]
fn test_overlapping_fields_git() {
    // Git has url + branch
    let json = r#"{"name":"repo","Git":{"url":"https://github.com/example","branch":"main"}}"#;
    let parsed: Source = from_str(json).expect("should parse Git source");

    assert_eq!(parsed.name, "repo");
    assert!(
        matches!(parsed.kind, SourceKind::Git(GitSource { ref url, ref branch })
        if url == "https://github.com/example" && branch == "main")
    );
}

#[test]
fn test_overlapping_fields_http() {
    // Http only has url
    let json = r#"{"name":"api","Http":{"url":"https://api.example.com"}}"#;
    let parsed: Source = from_str(json).expect("should parse Http source");

    assert_eq!(parsed.name, "api");
    assert!(
        matches!(parsed.kind, SourceKind::Http(HttpSource { ref url })
        if url == "https://api.example.com")
    );
}

// ============================================================================
// Vec in flattened context
// ============================================================================

#[derive(Facet, Debug, PartialEq)]
struct Tags {
    tags: Vec<String>,
    priority: u8,
}

#[derive(Facet, Debug, PartialEq)]
struct Article {
    title: String,
    #[facet(flatten)]
    metadata: Tags,
}

#[test]
fn test_vec_in_flatten() {
    let json = r#"{"title":"Hello World","tags":["rust","facet","json"],"priority":1}"#;
    let parsed: Article = from_str(json).expect("should parse Vec in flatten");

    assert_eq!(parsed.title, "Hello World");
    assert_eq!(parsed.metadata.tags, vec!["rust", "facet", "json"]);
    assert_eq!(parsed.metadata.priority, 1);
}

#[test]
fn test_vec_in_flatten_roundtrip() {
    let original = Article {
        title: "Test".into(),
        metadata: Tags {
            tags: vec!["a".into(), "b".into()],
            priority: 5,
        },
    };

    let json = to_string(&original);
    let parsed: Article = from_str(&json).expect("roundtrip");
    assert_eq!(original, parsed);
}
