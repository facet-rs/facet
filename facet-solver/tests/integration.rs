//! Integration tests: Full deserialize flow with json-event-parser.
//!
//! These tests demonstrate the complete pattern for deserializer implementors:
//! 1. Build schema for the target type
//! 2. Probe JSON keys to disambiguate the configuration
//! 3. Rewind to the start of the ambiguous section
//! 4. Deserialize into the resolved type
//!
//! Run with `cargo test -p facet-solver --test integration -- --nocapture` to see tracing output.

extern crate alloc;

use alloc::vec::Vec;
use std::io::Cursor;
use std::sync::Once;

use facet::Facet;
use facet_core::Shape;
use facet_solver::{
    KeyResult, ProbeResult, ProbingSolver, Resolution, SatisfyResult, Schema, Solver,
};
use json_event_parser::{JsonEvent, ReaderJsonParser};

use tracing::{debug, info, info_span, trace, warn};

/// Compute a specificity score for a shape. Lower score = more specific.
fn specificity_score(shape: &'static Shape) -> u64 {
    match shape.type_identifier {
        "u8" | "i8" => 8,
        "u16" | "i16" => 16,
        "u32" | "i32" | "f32" => 32,
        "u64" | "i64" | "f64" => 64,
        "u128" | "i128" => 128,
        "usize" | "isize" => 64,
        _ => 1000,
    }
}

static INIT: Once = Once::new();

/// Initialize the tracing subscriber for tests.
/// Only initializes once, even if called multiple times.
fn init_tracing() {
    INIT.call_once(|| {
        tracing_subscriber::fmt()
            .with_test_writer()
            .with_max_level(tracing::Level::TRACE)
            .with_target(false)
            .init();
    });
}

// ============================================================================
// Example 1: Simple flattened enum (top-level disambiguation)
// ============================================================================

#[derive(Facet, Debug, PartialEq)]
struct TextMessage {
    content: String,
}

#[derive(Facet, Debug, PartialEq)]
struct BinaryMessage {
    data: String, // hex-encoded for simplicity
    encoding: String,
}

#[allow(dead_code)]
#[derive(Facet, Debug)]
#[repr(u8)]
enum MessagePayload {
    Text(TextMessage),
    Binary(BinaryMessage),
}

#[derive(Facet, Debug)]
struct Message {
    id: String,
    timestamp: u64,
    #[facet(flatten)]
    payload: MessagePayload,
}

/// A minimal JSON deserializer that demonstrates the probe-rewind-deserialize pattern.
struct JsonDeserializer<'a> {
    data: &'a [u8],
}

impl<'a> JsonDeserializer<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data }
    }

    /// Probe the JSON to find which configuration matches.
    ///
    /// Uses the unified Solver that supports both:
    /// - Key-based disambiguation (different keys in different variants)
    /// - Type-based disambiguation (same keys but different value types)
    fn probe_for_config<'s>(&self, schema: &'s Schema) -> Result<&'s Resolution, &'static str> {
        let _span = info_span!("probe_for_config").entered();

        info!(
            configurations = schema.resolutions().len(),
            "starting probe"
        );

        for (i, config) in schema.resolutions().iter().enumerate() {
            debug!(
                config_index = i,
                fields = ?config.fields().keys().collect::<Vec<_>>(),
                "available configuration"
            );
        }

        let mut cursor = Cursor::new(self.data);
        let mut parser = ReaderJsonParser::new(&mut cursor);
        let mut solver = Solver::new(schema);

        // Track current path and pending key
        let mut path: Vec<String> = Vec::new();
        let mut pending_key: Option<String> = None;

        loop {
            let event = parser.parse_next().map_err(|_| "parse error")?;

            match event {
                JsonEvent::ObjectKey(key) => {
                    let key_str = key.into_owned();

                    // Convert path to &[&str] for the solver
                    let path_refs: Vec<&str> = path.iter().map(|s| s.as_str()).collect();
                    let full_path: Vec<&str> = path_refs
                        .iter()
                        .copied()
                        .chain(Some(key_str.as_str()))
                        .collect();

                    trace!(path = ?full_path, "saw key");

                    let candidates_before = solver.candidates().len();

                    match solver.probe_key(&path_refs, &key_str) {
                        KeyResult::Solved(config) => {
                            info!(
                                key = %key_str,
                                path = ?path_refs,
                                config = %config.resolution().describe(),
                                "SOLVED! Key disambiguated to single configuration"
                            );
                            return Ok(config.resolution());
                        }
                        KeyResult::Unknown => {
                            warn!(key = %key_str, path = ?path_refs, "no configuration matches");
                            return Err("no matching configuration");
                        }
                        KeyResult::Unambiguous { shape } => {
                            let candidates_after = solver.candidates().len();
                            if candidates_after < candidates_before {
                                debug!(
                                    key = %key_str,
                                    path = ?path_refs,
                                    shape = shape.type_identifier,
                                    before = candidates_before,
                                    after = candidates_after,
                                    "narrowed candidates (same type)"
                                );
                            } else {
                                trace!(
                                    key = %key_str,
                                    shape = shape.type_identifier,
                                    candidates = candidates_after,
                                    "key in all remaining candidates (same type)"
                                );
                            }
                        }
                        KeyResult::Ambiguous { .. } => {
                            // Different types at this path! Need value-based disambiguation.
                            // We'll handle this when we see the actual value.
                            debug!(
                                key = %key_str,
                                path = ?path_refs,
                                "AMBIGUOUS: different types at this path, will disambiguate by value"
                            );
                        }
                    }

                    pending_key = Some(key_str);
                }
                JsonEvent::StartObject => {
                    // Descend: push pending key onto path
                    if let Some(key) = pending_key.take() {
                        debug!(key = %key, "descending into object");
                        path.push(key);
                    } else {
                        trace!("starting root object");
                    }
                }
                JsonEvent::EndObject => {
                    if let Some(key) = path.pop() {
                        debug!(key = %key, "ascending from object");
                    } else {
                        trace!("ending root object");
                    }
                }
                JsonEvent::StartArray => {
                    if let Some(key) = pending_key.take() {
                        trace!(key = %key, "starting array (skipping)");
                    }
                }
                JsonEvent::EndArray => {
                    trace!("ending array");
                }
                JsonEvent::Eof => {
                    trace!("reached EOF");
                    break;
                }
                JsonEvent::String(s) => {
                    if let Some(key) = pending_key.take() {
                        trace!(key = %key, value = %s, "string value");
                        // Try type-based disambiguation for string values
                        let full_path: Vec<&str> = path
                            .iter()
                            .map(|s| s.as_str())
                            .chain(Some(key.as_str()))
                            .collect();
                        self.try_disambiguate_by_value(
                            &mut solver,
                            &full_path,
                            &JsonEvent::String(s.clone()),
                        );
                    }
                }
                JsonEvent::Number(n) => {
                    if let Some(key) = pending_key.take() {
                        trace!(key = %key, value = %n, "number value");
                        // Try type-based disambiguation for number values
                        let full_path: Vec<&str> = path
                            .iter()
                            .map(|s| s.as_str())
                            .chain(Some(key.as_str()))
                            .collect();
                        self.try_disambiguate_by_value(
                            &mut solver,
                            &full_path,
                            &JsonEvent::Number(n.clone()),
                        );
                    }
                }
                JsonEvent::Boolean(b) => {
                    if let Some(key) = pending_key.take() {
                        trace!(key = %key, value = %b, "boolean value");
                    }
                }
                JsonEvent::Null => {
                    if let Some(key) = pending_key.take() {
                        trace!(key = %key, "null value");
                    }
                }
            }

            // Check if we've solved after processing value
            if solver.candidates().len() == 1 {
                let config = solver.candidates()[0];
                info!(config = %config.resolution().describe(), "SOLVED by value type disambiguation!");
                return Ok(config.resolution());
            }
        }

        // Exhausted input - check final state
        info!(
            remaining_candidates = solver.candidates().len(),
            "finished scanning, checking final state"
        );

        match solver.finish() {
            Ok(config) => {
                info!(config = %config.resolution().describe(), "resolved at end of input");
                Ok(config.resolution())
            }
            Err(e) => {
                warn!(error = %e, "failed to resolve");
                Err("ambiguous: could not disambiguate")
            }
        }
    }

    /// Try to disambiguate by checking if the value fits the expected types.
    fn try_disambiguate_by_value<'s>(
        &self,
        solver: &mut Solver<'s>,
        path: &[&str],
        value: &JsonEvent<'_>,
    ) {
        // Get the shapes at this path
        let shapes = solver.get_shapes_at_path(path);
        if shapes.len() <= 1 {
            return; // Already unambiguous or no info
        }

        trace!(
            path = ?path,
            shapes = ?shapes.iter().map(|s| s.type_identifier).collect::<Vec<_>>(),
            "checking value type disambiguation"
        );

        // Filter shapes based on whether the value can be parsed as that type,
        // and pair with specificity score (lower = more specific)
        let mut satisfied_with_scores: Vec<_> = shapes
            .iter()
            .filter(|shape| self.value_fits_type(value, shape))
            .map(|shape| (*shape, specificity_score(shape)))
            .collect();

        if satisfied_with_scores.is_empty() {
            return;
        }

        // Sort by specificity score (ascending) - most specific first
        satisfied_with_scores.sort_by_key(|(_, score)| *score);

        // Pick the most specific type that satisfies the value
        let most_specific = satisfied_with_scores[0].0;

        if satisfied_with_scores.len() < shapes.len() || satisfied_with_scores.len() > 1 {
            debug!(
                path = ?path,
                original = shapes.len(),
                satisfied = satisfied_with_scores.len(),
                picked = most_specific.type_identifier,
                "narrowing by value type (picking most specific)"
            );
            // Only keep the most specific shape
            solver.satisfy_at_path(path, &[most_specific]);
        }
    }

    /// Check if a JSON value can be parsed as a given type.
    fn value_fits_type(&self, value: &JsonEvent<'_>, shape: &facet_core::Shape) -> bool {
        match value {
            JsonEvent::Number(n) => {
                let n_str = n.as_ref();
                match shape.type_identifier {
                    "u8" => n_str.parse::<u8>().is_ok(),
                    "u16" => n_str.parse::<u16>().is_ok(),
                    "u32" => n_str.parse::<u32>().is_ok(),
                    "u64" => n_str.parse::<u64>().is_ok(),
                    "i8" => n_str.parse::<i8>().is_ok(),
                    "i16" => n_str.parse::<i16>().is_ok(),
                    "i32" => n_str.parse::<i32>().is_ok(),
                    "i64" => n_str.parse::<i64>().is_ok(),
                    "f32" | "f64" => true, // floats accept any number
                    _ => false,
                }
            }
            JsonEvent::String(_) => {
                matches!(shape.type_identifier, "String" | "str")
            }
            JsonEvent::Boolean(_) => shape.type_identifier == "bool",
            _ => false,
        }
    }

    /// Parse a string value at the current position
    fn parse_string(
        parser: &mut ReaderJsonParser<&mut Cursor<&[u8]>>,
    ) -> Result<String, &'static str> {
        match parser.parse_next().map_err(|_| "parse error")? {
            JsonEvent::String(s) => Ok(s.into_owned()),
            _ => Err("expected string"),
        }
    }

    /// Parse a u64 value at the current position
    fn parse_u64(parser: &mut ReaderJsonParser<&mut Cursor<&[u8]>>) -> Result<u64, &'static str> {
        match parser.parse_next().map_err(|_| "parse error")? {
            JsonEvent::Number(n) => n.parse().map_err(|_| "invalid number"),
            _ => Err("expected number"),
        }
    }

    /// Deserialize into Message, using the resolved configuration
    fn deserialize_message(&self, config: &Resolution) -> Result<Message, &'static str> {
        let _span = info_span!("deserialize_message", config = %config.describe()).entered();

        let mut cursor = Cursor::new(self.data);
        let mut parser = ReaderJsonParser::new(&mut cursor);

        // Expect StartObject
        match parser.parse_next().map_err(|_| "parse error")? {
            JsonEvent::StartObject => {}
            _ => return Err("expected object"),
        }

        let mut id: Option<String> = None;
        let mut timestamp: Option<u64> = None;
        let mut content: Option<String> = None;
        let mut data: Option<String> = None;
        let mut encoding: Option<String> = None;

        // Parse fields
        loop {
            match parser.parse_next().map_err(|_| "parse error")? {
                JsonEvent::ObjectKey(key) => {
                    trace!(key = %key, "parsing field");
                    match key.as_ref() {
                        "id" => id = Some(Self::parse_string(&mut parser)?),
                        "timestamp" => timestamp = Some(Self::parse_u64(&mut parser)?),
                        "content" => content = Some(Self::parse_string(&mut parser)?),
                        "data" => data = Some(Self::parse_string(&mut parser)?),
                        "encoding" => encoding = Some(Self::parse_string(&mut parser)?),
                        _ => {
                            trace!(key = %key, "skipping unknown field");
                            Self::skip_value(&mut parser)?;
                        }
                    }
                }
                JsonEvent::EndObject => break,
                _ => return Err("unexpected event in object"),
            }
        }

        // Build the result based on which configuration was selected
        let payload = if config.has_key_path(&["content"]) {
            info!("building Text variant");
            MessagePayload::Text(TextMessage {
                content: content.ok_or("missing content")?,
            })
        } else if config.has_key_path(&["data"]) {
            info!("building Binary variant");
            MessagePayload::Binary(BinaryMessage {
                data: data.ok_or("missing data")?,
                encoding: encoding.ok_or("missing encoding")?,
            })
        } else {
            return Err("unknown configuration");
        };

        Ok(Message {
            id: id.ok_or("missing id")?,
            timestamp: timestamp.ok_or("missing timestamp")?,
            payload,
        })
    }

    /// Skip over a JSON value (for unknown fields)
    fn skip_value(parser: &mut ReaderJsonParser<&mut Cursor<&[u8]>>) -> Result<(), &'static str> {
        let mut depth = 0i32;
        loop {
            match parser.parse_next().map_err(|_| "parse error")? {
                JsonEvent::StartObject | JsonEvent::StartArray => depth += 1,
                JsonEvent::EndObject | JsonEvent::EndArray => {
                    depth -= 1;
                    if depth < 0 {
                        return Err("unbalanced nesting");
                    }
                    if depth == 0 {
                        return Ok(());
                    }
                }
                JsonEvent::String(_)
                | JsonEvent::Number(_)
                | JsonEvent::Boolean(_)
                | JsonEvent::Null => {
                    if depth == 0 {
                        return Ok(());
                    }
                }
                JsonEvent::ObjectKey(_) => {}
                JsonEvent::Eof => return Err("unexpected EOF"),
            }
        }
    }
}

#[test]
fn test_deserialize_text_message() {
    init_tracing();
    let _span = info_span!("test_deserialize_text_message").entered();

    let json = br#"{"id": "msg-001", "timestamp": 1699900000, "content": "Hello, world!"}"#;
    info!(json = %String::from_utf8_lossy(json), "input JSON");

    let schema = Schema::build(Message::SHAPE).unwrap();
    let deserializer = JsonDeserializer::new(json);

    // Step 1: Probe to find configuration
    let config = deserializer
        .probe_for_config(&schema)
        .expect("should resolve");

    // Verify we got the Text variant
    assert!(config.has_key_path(&["content"]));
    assert!(!config.has_key_path(&["data"]));

    // Step 2: Deserialize using resolved config
    let message = deserializer
        .deserialize_message(config)
        .expect("should deserialize");

    assert_eq!(message.id, "msg-001");
    assert_eq!(message.timestamp, 1699900000);
    match &message.payload {
        MessagePayload::Text(text) => {
            assert_eq!(text.content, "Hello, world!");
        }
        _ => panic!("expected Text variant"),
    }

    info!(?message, "successfully deserialized");
}

#[test]
fn test_deserialize_binary_message() {
    init_tracing();
    let _span = info_span!("test_deserialize_binary_message").entered();

    let json =
        br#"{"id": "msg-002", "timestamp": 1699900001, "data": "deadbeef", "encoding": "hex"}"#;
    info!(json = %String::from_utf8_lossy(json), "input JSON");

    let schema = Schema::build(Message::SHAPE).unwrap();
    let deserializer = JsonDeserializer::new(json);

    // Step 1: Probe to find configuration
    let config = deserializer
        .probe_for_config(&schema)
        .expect("should resolve");

    // Verify we got the Binary variant
    assert!(config.has_key_path(&["data"]));
    assert!(!config.has_key_path(&["content"]));

    // Step 2: Deserialize using resolved config
    let message = deserializer
        .deserialize_message(config)
        .expect("should deserialize");

    assert_eq!(message.id, "msg-002");
    assert_eq!(message.timestamp, 1699900001);
    match &message.payload {
        MessagePayload::Binary(bin) => {
            assert_eq!(bin.data, "deadbeef");
            assert_eq!(bin.encoding, "hex");
        }
        _ => panic!("expected Binary variant"),
    }

    info!(?message, "successfully deserialized");
}

#[test]
fn test_deserialize_fields_in_any_order() {
    init_tracing();
    let _span = info_span!("test_deserialize_fields_in_any_order").entered();

    // JSON with fields in different order - should still work
    let json = br#"{"content": "Hello!", "id": "msg-003", "timestamp": 1699900002}"#;
    info!(json = %String::from_utf8_lossy(json), "input JSON (fields reordered)");

    let schema = Schema::build(Message::SHAPE).unwrap();
    let deserializer = JsonDeserializer::new(json);

    let config = deserializer
        .probe_for_config(&schema)
        .expect("should resolve");
    let message = deserializer
        .deserialize_message(config)
        .expect("should deserialize");

    assert_eq!(message.id, "msg-003");
    match &message.payload {
        MessagePayload::Text(text) => assert_eq!(text.content, "Hello!"),
        _ => panic!("expected Text variant"),
    }

    info!(?message, "successfully deserialized");
}

// ============================================================================
// Example 2: Nested disambiguation (the "Annoying" case)
// ============================================================================
//
// This is the key scenario: both variants have field "config", but the
// CONTENTS of "config" differ. We need to look INSIDE to disambiguate.

#[derive(Facet, Debug, PartialEq)]
struct DatabaseConfig {
    host: String,
    port: u64,
}

#[derive(Facet, Debug, PartialEq)]
struct FileConfig {
    path: String,
    readonly: bool,
}

#[allow(dead_code)]
#[derive(Facet, Debug)]
#[repr(u8)]
enum StorageBackend {
    Database(DatabaseConfig),
    File(FileConfig),
}

#[derive(Facet, Debug)]
struct AppConfig {
    name: String,
    #[facet(flatten)]
    storage: StorageBackend,
}

#[test]
fn test_nested_disambiguation_database() {
    init_tracing();
    let _span = info_span!("test_nested_disambiguation_database").entered();

    // Both variants would have fields at the top level after flattening
    // Database has: host, port
    // File has: path, readonly
    let json = br#"{"name": "myapp", "host": "localhost", "port": 5432}"#;
    info!(json = %String::from_utf8_lossy(json), "input JSON");

    let schema = Schema::build(AppConfig::SHAPE).unwrap();
    assert_eq!(schema.resolutions().len(), 2);

    let deserializer = JsonDeserializer::new(json);
    let config = deserializer
        .probe_for_config(&schema)
        .expect("should resolve");

    // Should have resolved to Database (has "host")
    assert!(config.has_key_path(&["host"]));
    assert!(config.has_key_path(&["port"]));
    assert!(!config.has_key_path(&["path"]));

    info!(config = %config.describe(), "resolved to Database");
}

#[test]
fn test_nested_disambiguation_file() {
    init_tracing();
    let _span = info_span!("test_nested_disambiguation_file").entered();

    let json = br#"{"name": "myapp", "path": "/var/data/app.db", "readonly": true}"#;
    info!(json = %String::from_utf8_lossy(json), "input JSON");

    let schema = Schema::build(AppConfig::SHAPE).unwrap();
    let deserializer = JsonDeserializer::new(json);
    let config = deserializer
        .probe_for_config(&schema)
        .expect("should resolve");

    // Should have resolved to File (has "path")
    assert!(config.has_key_path(&["path"]));
    assert!(config.has_key_path(&["readonly"]));
    assert!(!config.has_key_path(&["host"]));

    info!(config = %config.describe(), "resolved to File");
}

// ============================================================================
// Example 3: Deep nesting - must probe multiple levels
// ============================================================================

#[derive(Facet, Debug, PartialEq)]
struct PostgresDetails {
    schema_name: String,
}

#[derive(Facet, Debug, PartialEq)]
struct MySqlDetails {
    charset: String,
}

#[allow(dead_code)]
#[derive(Facet, Debug)]
#[repr(u8)]
enum DbType {
    Postgres(PostgresDetails),
    MySql(MySqlDetails),
}

#[derive(Facet, Debug)]
struct ConnectionPool {
    max_connections: u64,
    #[facet(flatten)]
    db_type: DbType,
}

#[derive(Facet, Debug)]
struct ServiceConfig {
    service_name: String,
    pool: ConnectionPool, // NOT flattened - nested object
}

// Note: ServiceConfig doesn't flatten ConnectionPool, so it has only 1 config.
// The 2 configs (Postgres vs MySql) are at the ConnectionPool level.
// This is correct - when deserializing the "pool" field, you'd use a nested solver.

#[test]
fn test_connection_pool_schema() {
    init_tracing();
    let _span = info_span!("test_connection_pool_schema").entered();

    // ConnectionPool HAS a flattened enum, so it has 2 configurations
    let schema = Schema::build(ConnectionPool::SHAPE).unwrap();

    assert_eq!(schema.resolutions().len(), 2);

    let postgres_config = schema
        .resolutions()
        .iter()
        .find(|c| c.has_key_path(&["schema_name"]))
        .expect("should have postgres config");

    let mysql_config = schema
        .resolutions()
        .iter()
        .find(|c| c.has_key_path(&["charset"]))
        .expect("should have mysql config");

    // Both have max_connections
    assert!(postgres_config.has_key_path(&["max_connections"]));
    assert!(mysql_config.has_key_path(&["max_connections"]));

    // But different type-specific fields
    assert!(!postgres_config.has_key_path(&["charset"]));
    assert!(!mysql_config.has_key_path(&["schema_name"]));

    info!("ConnectionPool has 2 configurations: Postgres and MySql");
}

#[test]
fn test_probe_connection_pool_postgres() {
    init_tracing();
    let _span = info_span!("test_probe_connection_pool_postgres").entered();

    let json = br#"{"max_connections": 10, "schema_name": "public"}"#;
    info!(json = %String::from_utf8_lossy(json), "input JSON");

    let schema = Schema::build(ConnectionPool::SHAPE).unwrap();
    let deserializer = JsonDeserializer::new(json);
    let resolved = deserializer
        .probe_for_config(&schema)
        .expect("should resolve");

    assert!(resolved.has_key_path(&["schema_name"]));
    info!(config = %resolved.describe(), "resolved to Postgres");
}

#[test]
fn test_probe_connection_pool_mysql() {
    init_tracing();
    let _span = info_span!("test_probe_connection_pool_mysql").entered();

    let json = br#"{"max_connections": 20, "charset": "utf8mb4"}"#;
    info!(json = %String::from_utf8_lossy(json), "input JSON");

    let schema = Schema::build(ConnectionPool::SHAPE).unwrap();
    let deserializer = JsonDeserializer::new(json);
    let resolved = deserializer
        .probe_for_config(&schema)
        .expect("should resolve");

    assert!(resolved.has_key_path(&["charset"]));
    info!(config = %resolved.describe(), "resolved to MySql");
}

// ============================================================================
// Example 4: The truly "Annoying" case - same field name, different nested types
// ============================================================================

#[derive(Facet, Debug, PartialEq)]
struct FooInner {
    foo_specific: String,
}

#[derive(Facet, Debug, PartialEq)]
struct BarInner {
    bar_specific: String,
}

#[allow(dead_code)]
#[derive(Facet, Debug)]
#[repr(u8)]
enum AnnoyingEnum {
    Foo { payload: FooInner },
    Bar { payload: BarInner },
}

#[derive(Facet, Debug)]
struct AnnoyingWrapper {
    common: String,
    #[facet(flatten)]
    inner: AnnoyingEnum,
}

#[test]
fn test_truly_annoying_case() {
    init_tracing();
    let _span = info_span!("test_truly_annoying_case").entered();

    // Both variants have "payload" field, but contents differ!
    // Foo's payload has: foo_specific
    // Bar's payload has: bar_specific
    //
    // JSON: {"common": "x", "payload": {"foo_specific": "y"}}
    // vs:   {"common": "x", "payload": {"bar_specific": "z"}}
    //
    // We must probe INTO payload to disambiguate.

    let json_foo = br#"{"common": "shared", "payload": {"foo_specific": "foo value"}}"#;
    let json_bar = br#"{"common": "shared", "payload": {"bar_specific": "bar value"}}"#;

    let schema = Schema::build(AnnoyingWrapper::SHAPE).unwrap();

    info!("Schema has {} configurations", schema.resolutions().len());
    for (i, config) in schema.resolutions().iter().enumerate() {
        info!(
            config_index = i,
            paths = ?config.known_paths(),
            "configuration paths"
        );
    }

    // Verify schema tracks the nested paths
    let foo_config = schema
        .resolutions()
        .iter()
        .find(|c| c.has_key_path(&["payload", "foo_specific"]))
        .expect("should have Foo config");

    let bar_config = schema
        .resolutions()
        .iter()
        .find(|c| c.has_key_path(&["payload", "bar_specific"]))
        .expect("should have Bar config");

    // Both have "payload" at top level
    assert!(foo_config.has_key_path(&["payload"]));
    assert!(bar_config.has_key_path(&["payload"]));

    // But different nested keys
    assert!(!foo_config.has_key_path(&["payload", "bar_specific"]));
    assert!(!bar_config.has_key_path(&["payload", "foo_specific"]));

    // Probe and resolve Foo
    info!("--- Probing Foo variant ---");
    info!(json = %String::from_utf8_lossy(json_foo), "input JSON");
    let deserializer_foo = JsonDeserializer::new(json_foo);
    let resolved_foo = deserializer_foo
        .probe_for_config(&schema)
        .expect("should resolve foo");
    assert!(resolved_foo.has_key_path(&["payload", "foo_specific"]));

    // Probe and resolve Bar
    info!("--- Probing Bar variant ---");
    info!(json = %String::from_utf8_lossy(json_bar), "input JSON");
    let deserializer_bar = JsonDeserializer::new(json_bar);
    let resolved_bar = deserializer_bar
        .probe_for_config(&schema)
        .expect("should resolve bar");
    assert!(resolved_bar.has_key_path(&["payload", "bar_specific"]));

    info!("Both variants successfully disambiguated by probing nested keys!");
}

// ============================================================================
// Example 5: The SUPER annoying case - same nested path, different types!
// ============================================================================
//
// This is the ultimate test: both variants have the SAME nested key path
// (payload.value) but the VALUE has different types (u8 vs u16).
//
// To disambiguate, we need BOTH:
// 1. Nested path awareness (to know we're at payload.value)
// 2. Type-based disambiguation (u8 can't hold 1000, u16 can)
//
// Currently NO solver handles this case!

#[derive(Facet, Debug, PartialEq)]
struct SmallPayload {
    value: u8,
}

#[derive(Facet, Debug, PartialEq)]
struct LargePayload {
    value: u16,
}

#[allow(dead_code)]
#[derive(Facet, Debug)]
#[repr(u8)]
enum SuperAnnoyingEnum {
    Small { payload: SmallPayload },
    Large { payload: LargePayload },
}

#[derive(Facet, Debug)]
struct SuperAnnoyingWrapper {
    common: String,
    #[facet(flatten)]
    inner: SuperAnnoyingEnum,
}

#[test]
fn test_super_annoying_same_path_different_types() {
    init_tracing();
    let _span = info_span!("test_super_annoying_same_path_different_types").entered();

    // Both variants have "payload.value" but:
    // - Small.payload.value: u8 (max 255)
    // - Large.payload.value: u16 (max 65535)
    //
    // JSON with value 1000 should resolve to Large (u8 can't hold it)

    let schema = Schema::build(SuperAnnoyingWrapper::SHAPE).unwrap();

    info!("Schema has {} configurations", schema.resolutions().len());
    for (i, config) in schema.resolutions().iter().enumerate() {
        info!(
            config_index = i,
            paths = ?config.known_paths(),
            "configuration paths"
        );
    }

    // Verify schema tracks the nested paths with their types
    let small_config = schema
        .resolutions()
        .iter()
        .find(|c| c.describe().contains("Small"))
        .expect("should have Small config");

    let large_config = schema
        .resolutions()
        .iter()
        .find(|c| c.describe().contains("Large"))
        .expect("should have Large config");

    // Both have payload.value path
    assert!(small_config.has_key_path(&["payload", "value"]));
    assert!(large_config.has_key_path(&["payload", "value"]));

    // But with different types!
    let small_field = small_config.field("payload").expect("should have payload");
    let large_field = large_config.field("payload").expect("should have payload");

    info!(
        small_payload_type = small_field.value_shape.type_identifier,
        large_payload_type = large_field.value_shape.type_identifier,
        "payload field types"
    );

    // Now the actual test: disambiguate based on VALUE
    // JSON: {"common": "test", "payload": {"value": 1000}}
    // 1000 > 255, so only Large (u16) can accept it

    let json_large = br#"{"common": "test", "payload": {"value": 1000}}"#;
    let json_small = br#"{"common": "test", "payload": {"value": 42}}"#;

    info!(json = %String::from_utf8_lossy(json_large), "testing large value (1000)");

    // This is where we need the unified solver!
    // ProbingSolver can track nested paths but can't do type disambiguation
    // Solver can do type disambiguation but doesn't track nested paths

    let deserializer = JsonDeserializer::new(json_large);
    let result = deserializer.probe_for_config(&schema);

    // With current ProbingSolver, this will be AMBIGUOUS because both have payload.value
    // We WANT it to resolve to Large based on the value 1000 not fitting in u8

    // Currently this FAILS because ProbingSolver doesn't do type disambiguation
    // Once we have unified solver, this should pass
    let config = result.expect("unified solver should disambiguate by nested value type");

    assert!(
        config.describe().contains("Large"),
        "Expected Large variant for value 1000 (doesn't fit in u8), got: {}",
        config.describe()
    );

    info!(config = %config.describe(), "correctly resolved to Large based on value type");

    // Now test the small value - 42 fits in u8, so it should resolve to Small
    info!(json = %String::from_utf8_lossy(json_small), "testing small value (42)");

    let deserializer = JsonDeserializer::new(json_small);
    let result = deserializer.probe_for_config(&schema);

    let config = result.expect("should resolve to Small for value that fits in u8");

    assert!(
        config.describe().contains("Small"),
        "Expected Small variant for value 42 (fits in u8), got: {}",
        config.describe()
    );

    info!(config = %config.describe(), "correctly resolved to Small based on value type");
}

// ============================================================================
// Sadistic Type-Based Disambiguation Tests
// ============================================================================
//
// These tests exercise the Solver's ability to disambiguate based on VALUE types,
// not just key presence. This is the "witness" pattern where the deserializer
// tells the solver which types the actual value can satisfy.

// ----------------------------------------------------------------------------
// Test: (u8, u16) integer range disambiguation
// ----------------------------------------------------------------------------

#[derive(Facet, Debug)]
struct SmallInt {
    value: u8,
}

#[derive(Facet, Debug)]
struct LargeInt {
    value: u16,
}

#[allow(dead_code)]
#[derive(Facet, Debug)]
#[repr(u8)]
enum IntSize {
    Small(SmallInt),
    Large(LargeInt),
}

#[derive(Facet, Debug)]
struct IntContainer {
    #[facet(flatten)]
    inner: IntSize,
}

/// Helper: Parse JSON and probe with the Solver for integer disambiguation
fn probe_int_json(json: &[u8]) -> Result<String, String> {
    let schema = Schema::build(IntContainer::SHAPE).unwrap();

    let mut cursor = Cursor::new(json);
    let mut parser = ReaderJsonParser::new(&mut cursor);

    // Skip StartObject
    match parser.parse_next().map_err(|e| format!("{e:?}"))? {
        JsonEvent::StartObject => {}
        _ => return Err("expected object".into()),
    }

    // Collect all key-value pairs first (to avoid lifetime issues)
    let mut fields: Vec<(String, String)> = Vec::new();

    loop {
        match parser.parse_next().map_err(|e| format!("{e:?}"))? {
            JsonEvent::ObjectKey(key) => {
                let key_str = key.into_owned();
                let value_str = match parser.parse_next().map_err(|e| format!("{e:?}"))? {
                    JsonEvent::Number(n) => n.into_owned(),
                    _ => return Err("expected number".into()),
                };
                fields.push((key_str, value_str));
            }
            JsonEvent::EndObject => break,
            _ => return Err("unexpected event".into()),
        }
    }

    // Now use the solver with references that live long enough
    let mut solver = Solver::new(&schema);

    for (key, value_str) in &fields {
        match solver.see_key(key) {
            KeyResult::Solved(config) => return Ok(config.resolution().describe()),
            KeyResult::Unknown => return Err(format!("unknown key: {key}")),
            KeyResult::Unambiguous { .. } => {
                // Same type in all candidates - continue
            }
            KeyResult::Ambiguous { fields } => {
                // Different types - check which fields work
                let value: u64 = value_str.parse().map_err(|_| "invalid number")?;

                let satisfied: Vec<_> = fields
                    .iter()
                    .filter(|(f, _)| match f.value_shape.type_identifier {
                        "u8" => value <= u8::MAX as u64,
                        "u16" => value <= u16::MAX as u64,
                        "u32" => value <= u32::MAX as u64,
                        "u64" => true,
                        _ => false,
                    })
                    .map(|(f, _)| *f)
                    .collect();

                match solver.satisfy(&satisfied) {
                    SatisfyResult::Solved(config) => return Ok(config.resolution().describe()),
                    SatisfyResult::Continue => {}
                    SatisfyResult::NoMatch => return Err("no type can accept value".into()),
                }
            }
        }
    }

    // Finish - check required fields
    solver
        .finish()
        .map(|c| c.resolution().describe())
        .map_err(|e| format!("{e}"))
}

#[test]
fn test_int_range_small_value() {
    init_tracing();
    let _span = info_span!("test_int_range_small_value").entered();

    // 42 fits in both u8 and u16 - should be ambiguous
    let json = br#"{"value": 42}"#;
    info!(json = %String::from_utf8_lossy(json), "input JSON");

    let result = probe_int_json(json);
    info!(result = ?result, "result");

    // Both u8 and u16 can hold 42, so this is ambiguous
    let err = result.expect_err("should be ambiguous");
    assert!(
        err.contains("Ambiguous"),
        "Expected Ambiguous error, got: {err}"
    );
    assert!(
        err.contains("Small") && err.contains("Large"),
        "Expected both variants in error, got: {err}"
    );
}

#[test]
fn test_int_range_large_value() {
    init_tracing();
    let _span = info_span!("test_int_range_large_value").entered();

    // 1000 doesn't fit in u8 (max 255), only u16 works
    let json = br#"{"value": 1000}"#;
    info!(json = %String::from_utf8_lossy(json), "input JSON");

    let result = probe_int_json(json).expect("should resolve");
    info!(result = %result, "resolved");

    assert!(
        result.contains("Large"),
        "Expected Large variant (u16), got: {result}"
    );
}

#[test]
fn test_int_range_boundary_255() {
    init_tracing();
    let _span = info_span!("test_int_range_boundary_255").entered();

    // 255 is u8::MAX - fits in both, so ambiguous
    let json = br#"{"value": 255}"#;
    info!(json = %String::from_utf8_lossy(json), "input JSON");

    let result = probe_int_json(json);
    info!(result = ?result, "result");

    // Both u8 and u16 can hold 255, so this is ambiguous
    let err = result.expect_err("should be ambiguous");
    assert!(
        err.contains("Ambiguous"),
        "Expected Ambiguous error, got: {err}"
    );
}

#[test]
fn test_int_range_boundary_256() {
    init_tracing();
    let _span = info_span!("test_int_range_boundary_256").entered();

    // 256 exceeds u8::MAX - only u16 works
    let json = br#"{"value": 256}"#;
    info!(json = %String::from_utf8_lossy(json), "input JSON");

    let result = probe_int_json(json).expect("should resolve");
    info!(result = %result, "resolved");

    assert!(
        result.contains("Large"),
        "Expected Large variant (256 > u8::MAX), got: {result}"
    );
}

// ----------------------------------------------------------------------------
// Test: (i8, u8) signed/unsigned disambiguation
// ----------------------------------------------------------------------------

#[derive(Facet, Debug)]
struct SignedVariant {
    num: i8,
}

#[derive(Facet, Debug)]
struct UnsignedVariant {
    num: u8,
}

#[allow(dead_code)]
#[derive(Facet, Debug)]
#[repr(u8)]
enum SignedUnsigned {
    Signed(SignedVariant),
    Unsigned(UnsignedVariant),
}

#[derive(Facet, Debug)]
struct SignContainer {
    #[facet(flatten)]
    inner: SignedUnsigned,
}

fn probe_signed_json(json: &[u8]) -> Result<String, String> {
    let schema = Schema::build(SignContainer::SHAPE).unwrap();

    let mut cursor = Cursor::new(json);
    let mut parser = ReaderJsonParser::new(&mut cursor);

    match parser.parse_next().map_err(|e| format!("{e:?}"))? {
        JsonEvent::StartObject => {}
        _ => return Err("expected object".into()),
    }

    // Collect all key-value pairs first
    let mut fields: Vec<(String, String)> = Vec::new();

    loop {
        match parser.parse_next().map_err(|e| format!("{e:?}"))? {
            JsonEvent::ObjectKey(key) => {
                let key_str = key.into_owned();
                let value_str = match parser.parse_next().map_err(|e| format!("{e:?}"))? {
                    JsonEvent::Number(n) => n.into_owned(),
                    _ => return Err("expected number".into()),
                };
                fields.push((key_str, value_str));
            }
            JsonEvent::EndObject => break,
            _ => return Err("unexpected event".into()),
        }
    }

    let mut solver = Solver::new(&schema);

    for (key, value_str) in &fields {
        match solver.see_key(key) {
            KeyResult::Solved(config) => return Ok(config.resolution().describe()),
            KeyResult::Unknown => return Err(format!("unknown key: {key}")),
            KeyResult::Unambiguous { .. } => {}
            KeyResult::Ambiguous { fields } => {
                let value: i64 = value_str.parse().map_err(|_| "invalid number")?;

                let satisfied: Vec<_> = fields
                    .iter()
                    .filter(|(f, _)| match f.value_shape.type_identifier {
                        "i8" => value >= i8::MIN as i64 && value <= i8::MAX as i64,
                        "u8" => value >= 0 && value <= u8::MAX as i64,
                        "i16" => value >= i16::MIN as i64 && value <= i16::MAX as i64,
                        "u16" => value >= 0 && value <= u16::MAX as i64,
                        _ => false,
                    })
                    .map(|(f, _)| *f)
                    .collect();

                match solver.satisfy(&satisfied) {
                    SatisfyResult::Solved(config) => return Ok(config.resolution().describe()),
                    SatisfyResult::Continue => {}
                    SatisfyResult::NoMatch => return Err("no type can accept value".into()),
                }
            }
        }
    }

    solver
        .finish()
        .map(|c| c.resolution().describe())
        .map_err(|e| format!("{e}"))
}

#[test]
fn test_signed_negative_value() {
    init_tracing();
    let _span = info_span!("test_signed_negative_value").entered();

    // -10 only fits in i8, not u8
    let json = br#"{"num": -10}"#;
    info!(json = %String::from_utf8_lossy(json), "input JSON");

    let result = probe_signed_json(json).expect("should resolve");
    info!(result = %result, "resolved");

    assert!(
        result.contains("Signed"),
        "Expected Signed variant (-10 needs signed), got: {result}"
    );
}

#[test]
fn test_unsigned_large_positive() {
    init_tracing();
    let _span = info_span!("test_unsigned_large_positive").entered();

    // 200 fits in u8 (0-255) but not i8 (-128 to 127)
    let json = br#"{"num": 200}"#;
    info!(json = %String::from_utf8_lossy(json), "input JSON");

    let result = probe_signed_json(json).expect("should resolve");
    info!(result = %result, "resolved");

    assert!(
        result.contains("Unsigned"),
        "Expected Unsigned variant (200 > i8::MAX), got: {result}"
    );
}

#[test]
fn test_signed_unsigned_overlap() {
    init_tracing();
    let _span = info_span!("test_signed_unsigned_overlap").entered();

    // 50 fits in both i8 and u8 - should be ambiguous
    let json = br#"{"num": 50}"#;
    info!(json = %String::from_utf8_lossy(json), "input JSON");

    let result = probe_signed_json(json);
    info!(result = ?result, "result");

    // Both i8 and u8 can hold 50, so this is ambiguous
    let err = result.expect_err("should be ambiguous");
    assert!(
        err.contains("Ambiguous"),
        "Expected Ambiguous error, got: {err}"
    );
}

// ----------------------------------------------------------------------------
// Test: (i64, f64, String) - same field name, different types
// ----------------------------------------------------------------------------

#[derive(Facet, Debug)]
struct IntPayload {
    data: i64,
}

#[derive(Facet, Debug)]
struct FloatPayload {
    data: f64,
}

#[derive(Facet, Debug)]
struct TextPayload {
    data: String,
}

#[allow(dead_code)]
#[derive(Facet, Debug)]
#[repr(u8)]
enum MultiTypePayload {
    Int(IntPayload),
    Float(FloatPayload),
    Text(TextPayload),
}

#[derive(Facet, Debug)]
struct MultiTypeContainer {
    #[facet(flatten)]
    payload: MultiTypePayload,
}

/// Parsed JSON value for multitype tests
#[allow(dead_code)]
enum ParsedValue {
    Number(String),
    String(String),
}

fn probe_multitype_json(json: &[u8]) -> Result<String, String> {
    let schema = Schema::build(MultiTypeContainer::SHAPE).unwrap();

    let mut cursor = Cursor::new(json);
    let mut parser = ReaderJsonParser::new(&mut cursor);

    match parser.parse_next().map_err(|e| format!("{e:?}"))? {
        JsonEvent::StartObject => {}
        _ => return Err("expected object".into()),
    }

    // Collect all key-value pairs first
    let mut fields: Vec<(String, ParsedValue)> = Vec::new();

    loop {
        match parser.parse_next().map_err(|e| format!("{e:?}"))? {
            JsonEvent::ObjectKey(key) => {
                let key_str = key.into_owned();
                let value = match parser.parse_next().map_err(|e| format!("{e:?}"))? {
                    JsonEvent::Number(n) => ParsedValue::Number(n.into_owned()),
                    JsonEvent::String(s) => ParsedValue::String(s.into_owned()),
                    _ => return Err("expected number or string".into()),
                };
                fields.push((key_str, value));
            }
            JsonEvent::EndObject => break,
            _ => return Err("unexpected event".into()),
        }
    }

    let mut solver = Solver::new(&schema);

    for (key, value) in &fields {
        match solver.see_key(key) {
            KeyResult::Solved(config) => return Ok(config.resolution().describe()),
            KeyResult::Unknown => return Err(format!("unknown key: {key}")),
            KeyResult::Unambiguous { .. } => {}
            KeyResult::Ambiguous { fields } => {
                let satisfied: Vec<_> = fields
                    .iter()
                    .filter(|(f, _)| {
                        match (value, f.value_shape.type_identifier) {
                            // JSON string -> only String field accepts
                            (ParsedValue::String(_), "String") => true,
                            // JSON number with decimal -> only f64 accepts
                            (ParsedValue::Number(_n), "f64") => true,
                            (ParsedValue::Number(_n), "f32") => true,
                            // JSON integer -> i64 and f64 both accept
                            (ParsedValue::Number(n), "i64") => !n.contains('.'),
                            (ParsedValue::Number(n), "i32") => !n.contains('.'),
                            _ => false,
                        }
                    })
                    .map(|(f, _)| *f)
                    .collect();

                match solver.satisfy(&satisfied) {
                    SatisfyResult::Solved(config) => return Ok(config.resolution().describe()),
                    SatisfyResult::Continue => {}
                    SatisfyResult::NoMatch => return Err("no type can accept value".into()),
                }
            }
        }
    }

    solver
        .finish()
        .map(|c| c.resolution().describe())
        .map_err(|e| format!("{e}"))
}

#[test]
fn test_multitype_integer_value() {
    init_tracing();
    let _span = info_span!("test_multitype_integer_value").entered();

    // JSON integer - both i64 and f64 can accept, should be ambiguous
    let json = br#"{"data": 42}"#;
    info!(json = %String::from_utf8_lossy(json), "input JSON");

    let result = probe_multitype_json(json);
    info!(result = ?result, "result");

    // Both i64 and f64 can accept 42, so this is ambiguous
    let err = result.expect_err("should be ambiguous");
    assert!(
        err.contains("Ambiguous"),
        "Expected Ambiguous error, got: {err}"
    );
}

#[test]
fn test_multitype_float_value() {
    init_tracing();
    let _span = info_span!("test_multitype_float_value").entered();

    // JSON float - only f64 can accept (strict integer parsing for i64)
    let json = br#"{"data": 3.14}"#;
    info!(json = %String::from_utf8_lossy(json), "input JSON");

    let result = probe_multitype_json(json).expect("should resolve");
    info!(result = %result, "resolved");

    assert!(
        result.contains("Float"),
        "Expected Float variant (3.14 has decimal), got: {result}"
    );
}

#[test]
fn test_multitype_string_value() {
    init_tracing();
    let _span = info_span!("test_multitype_string_value").entered();

    // JSON string - only String can accept
    let json = br#"{"data": "hello world"}"#;
    info!(json = %String::from_utf8_lossy(json), "input JSON");

    let result = probe_multitype_json(json).expect("should resolve");
    info!(result = %result, "resolved");

    assert!(
        result.contains("Text"),
        "Expected Text variant (string value), got: {result}"
    );
}

// ----------------------------------------------------------------------------
// Test: DateTime vs UUID vs String - string format try-parsing
// ----------------------------------------------------------------------------
//
// All three accept JSON strings, but we disambiguate by parsing format:
// - DateTime: ISO 8601 format (contains T or - and :)
// - UUID: 8-4-4-4-12 hex format
// - Plain string: anything else

#[derive(Facet, Debug)]
struct DateTimePayload {
    id: String, // Would be chrono::DateTime in real code
}

#[derive(Facet, Debug)]
struct UuidPayload {
    id: String, // Would be uuid::Uuid in real code
}

#[derive(Facet, Debug)]
struct PlainPayload {
    id: String,
}

#[allow(dead_code)]
#[derive(Facet, Debug)]
#[repr(u8)]
enum StringFormat {
    DateTime(DateTimePayload),
    Uuid(UuidPayload),
    Plain(PlainPayload),
}

#[derive(Facet, Debug)]
struct StringFormatContainer {
    #[facet(flatten)]
    inner: StringFormat,
}

/// Check if string looks like ISO 8601 datetime
fn looks_like_datetime(s: &str) -> bool {
    // Simplified check: has both date separator and time components
    s.len() >= 19 && (s.contains('T') || (s.contains('-') && s.contains(':')))
}

/// Check if string looks like a UUID (8-4-4-4-12 hex)
fn looks_like_uuid(s: &str) -> bool {
    if s.len() != 36 {
        return false;
    }
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 5 {
        return false;
    }
    let expected_lens = [8, 4, 4, 4, 12];
    parts
        .iter()
        .zip(expected_lens.iter())
        .all(|(p, &len)| p.len() == len && p.chars().all(|c| c.is_ascii_hexdigit()))
}

fn probe_string_format_json(json: &[u8]) -> Result<String, String> {
    let schema = Schema::build(StringFormatContainer::SHAPE).unwrap();

    let mut cursor = Cursor::new(json);
    let mut parser = ReaderJsonParser::new(&mut cursor);

    match parser.parse_next().map_err(|e| format!("{e:?}"))? {
        JsonEvent::StartObject => {}
        _ => return Err("expected object".into()),
    }

    // Collect all key-value pairs first
    let mut fields: Vec<(String, String)> = Vec::new();

    loop {
        match parser.parse_next().map_err(|e| format!("{e:?}"))? {
            JsonEvent::ObjectKey(key) => {
                let key_str = key.into_owned();
                let value = match parser.parse_next().map_err(|e| format!("{e:?}"))? {
                    JsonEvent::String(s) => s.into_owned(),
                    _ => return Err("expected string value".into()),
                };
                fields.push((key_str, value));
            }
            JsonEvent::EndObject => break,
            _ => return Err("unexpected event".into()),
        }
    }

    let mut solver = Solver::new(&schema);

    for (key, value) in &fields {
        match solver.see_key(key) {
            KeyResult::Solved(config) => return Ok(config.resolution().describe()),
            KeyResult::Unknown => return Err(format!("unknown key: {key}")),
            KeyResult::Unambiguous { .. } => {
                // All variants have same type (String) - disambiguate by format
                let looks_dt = looks_like_datetime(value);
                let looks_uuid = looks_like_uuid(value);

                if looks_dt {
                    return Ok("StringFormat::DateTime".into());
                } else if looks_uuid {
                    return Ok("StringFormat::Uuid".into());
                } else {
                    return Ok("StringFormat::Plain".into());
                }
            }
            KeyResult::Ambiguous { fields } => {
                let satisfied: Vec<_> = fields.iter().map(|(f, _)| *f).collect();

                match solver.satisfy(&satisfied) {
                    SatisfyResult::Solved(config) => return Ok(config.resolution().describe()),
                    SatisfyResult::Continue => {}
                    SatisfyResult::NoMatch => return Err("no type can accept value".into()),
                }
            }
        }
    }

    solver
        .finish()
        .map(|c| c.resolution().describe())
        .map_err(|e| format!("{e}"))
}

#[test]
fn test_string_format_datetime() {
    init_tracing();
    let _span = info_span!("test_string_format_datetime").entered();

    // ISO 8601 datetime string
    let json = br#"{"id": "2024-01-15T10:30:00Z"}"#;
    info!(json = %String::from_utf8_lossy(json), "input JSON");

    let result = probe_string_format_json(json).expect("should resolve");
    info!(result = %result, "resolved");

    assert!(
        result.contains("DateTime"),
        "Expected DateTime variant, got: {result}"
    );
}

#[test]
fn test_string_format_uuid() {
    init_tracing();
    let _span = info_span!("test_string_format_uuid").entered();

    // UUID v4 format
    let json = br#"{"id": "550e8400-e29b-41d4-a716-446655440000"}"#;
    info!(json = %String::from_utf8_lossy(json), "input JSON");

    let result = probe_string_format_json(json).expect("should resolve");
    info!(result = %result, "resolved");

    assert!(
        result.contains("Uuid"),
        "Expected Uuid variant, got: {result}"
    );
}

#[test]
fn test_string_format_plain() {
    init_tracing();
    let _span = info_span!("test_string_format_plain").entered();

    // Plain string - doesn't match DateTime or UUID patterns
    let json = br#"{"id": "hello-world"}"#;
    info!(json = %String::from_utf8_lossy(json), "input JSON");

    let result = probe_string_format_json(json).expect("should resolve");
    info!(result = %result, "resolved");

    assert!(
        result.contains("Plain"),
        "Expected Plain variant, got: {result}"
    );
}

#[test]
fn test_string_format_ambiguous_datelike() {
    init_tracing();
    let _span = info_span!("test_string_format_ambiguous_datelike").entered();

    // This looks datetime-ish but is a valid UUID-ish thing...
    // Actually this is clearly a datetime
    let json = br#"{"id": "2024-12-25T00:00:00"}"#;
    info!(json = %String::from_utf8_lossy(json), "input JSON");

    let result = probe_string_format_json(json).expect("should resolve");
    info!(result = %result, "resolved");

    assert!(
        result.contains("DateTime"),
        "Expected DateTime variant for date string, got: {result}"
    );
}

// ----------------------------------------------------------------------------
// Test: Complex nesting with type disambiguation
// ----------------------------------------------------------------------------
//
// Nested objects where disambiguation happens at multiple levels

#[derive(Facet, Debug)]
struct HttpEndpoint {
    url: String,
    method: String,
}

#[derive(Facet, Debug)]
struct GrpcEndpoint {
    url: String,
    service: String,
}

#[allow(dead_code)]
#[derive(Facet, Debug)]
#[repr(u8)]
enum EndpointKind {
    Http(HttpEndpoint),
    Grpc(GrpcEndpoint),
}

#[derive(Facet, Debug)]
struct EndpointConfig {
    name: String,
    #[facet(flatten)]
    endpoint: EndpointKind,
}

#[test]
fn test_nested_key_disambiguation_http() {
    init_tracing();
    let _span = info_span!("test_nested_key_disambiguation_http").entered();

    // "method" only in Http variant
    let json = br#"{"name": "api", "url": "https://api.example.com", "method": "POST"}"#;
    info!(json = %String::from_utf8_lossy(json), "input JSON");

    let schema = Schema::build(EndpointConfig::SHAPE).unwrap();
    let deserializer = JsonDeserializer::new(json);
    let config = deserializer
        .probe_for_config(&schema)
        .expect("should resolve");

    assert!(
        config.describe().contains("Http"),
        "Expected Http variant, got: {}",
        config.describe()
    );
}

#[test]
fn test_nested_key_disambiguation_grpc() {
    init_tracing();
    let _span = info_span!("test_nested_key_disambiguation_grpc").entered();

    // "service" only in Grpc variant
    let json = br#"{"name": "api", "url": "grpc://api.example.com", "service": "UserService"}"#;
    info!(json = %String::from_utf8_lossy(json), "input JSON");

    let schema = Schema::build(EndpointConfig::SHAPE).unwrap();
    let deserializer = JsonDeserializer::new(json);
    let config = deserializer
        .probe_for_config(&schema)
        .expect("should resolve");

    assert!(
        config.describe().contains("Grpc"),
        "Expected Grpc variant, got: {}",
        config.describe()
    );
}

// ============================================================================
// Enum Representation Detection Tests
// ============================================================================

use facet_solver::EnumRepr;

/// Test that `EnumRepr::from_shape` correctly detects untagged enums.
#[test]
fn test_enum_repr_detection_untagged() {
    #[derive(Facet)]
    #[repr(u8)]
    #[facet(untagged)]
    #[allow(dead_code)]
    enum UntaggedEnum {
        Int(i64),
        String(String),
    }

    let repr = EnumRepr::from_shape(UntaggedEnum::SHAPE);
    assert_eq!(repr, EnumRepr::Flattened);
}

/// Test that `EnumRepr::from_shape` correctly detects internally-tagged enums.
#[test]
fn test_enum_repr_detection_internally_tagged() {
    #[derive(Facet)]
    #[repr(u8)]
    #[facet(tag = "type")]
    #[allow(dead_code)]
    enum InternallyTaggedEnum {
        Request { id: String },
        Response { id: String },
    }

    let repr = EnumRepr::from_shape(InternallyTaggedEnum::SHAPE);
    assert_eq!(repr, EnumRepr::InternallyTagged { tag: "type" });
}

/// Test that `EnumRepr::from_shape` correctly detects adjacently-tagged enums.
#[test]
fn test_enum_repr_detection_adjacently_tagged() {
    #[derive(Facet)]
    #[repr(u8)]
    #[facet(tag = "t", content = "c")]
    #[allow(dead_code)]
    enum AdjacentlyTaggedEnum {
        Para(String),
        Code(String),
    }

    let repr = EnumRepr::from_shape(AdjacentlyTaggedEnum::SHAPE);
    assert_eq!(
        repr,
        EnumRepr::AdjacentlyTagged {
            tag: "t",
            content: "c"
        }
    );
}

/// Test that `EnumRepr::from_shape` defaults to ExternallyTagged for plain enums.
/// This is the correct default for flattened enums - the variant name becomes a key.
#[test]
fn test_enum_repr_detection_default() {
    #[derive(Facet)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum PlainEnum {
        Active,
        Inactive,
    }

    let repr = EnumRepr::from_shape(PlainEnum::SHAPE);
    assert_eq!(repr, EnumRepr::ExternallyTagged);
}

/// Test that `Schema::build_auto` works with internally-tagged enums.
#[test]
fn test_schema_build_auto_internally_tagged() {
    init_tracing();

    #[derive(Facet)]
    #[repr(u8)]
    #[facet(tag = "type")]
    #[allow(dead_code)]
    enum ApiMessage {
        Request { method: String, params: String },
        Response { result: String },
    }

    #[derive(Facet)]
    struct ApiPayload {
        id: String,
        #[facet(flatten)]
        message: ApiMessage,
    }

    let schema = Schema::build_auto(ApiPayload::SHAPE).expect("should build schema");
    let resolutions = schema.resolutions();

    // Should have 2 resolutions (one per variant)
    assert_eq!(resolutions.len(), 2);

    // Each resolution should have the "type" tag field
    for resolution in resolutions {
        let field_names: Vec<_> = resolution.fields().keys().copied().collect();
        assert!(
            field_names.contains(&"type"),
            "Expected 'type' tag field, got: {field_names:?}"
        );
    }
}

/// Test that `Schema::build_auto` works with adjacently-tagged enums.
#[test]
fn test_schema_build_auto_adjacently_tagged() {
    init_tracing();

    #[derive(Facet)]
    #[repr(u8)]
    #[facet(tag = "kind", content = "data")]
    #[allow(dead_code)]
    enum Block {
        Paragraph { text: String },
        Code { lang: String, source: String },
    }

    #[derive(Facet)]
    struct Document {
        title: String,
        #[facet(flatten)]
        content: Block,
    }

    let schema = Schema::build_auto(Document::SHAPE).expect("should build schema");
    let resolutions = schema.resolutions();

    // Should have 2 resolutions (one per variant)
    assert_eq!(resolutions.len(), 2);

    // Each resolution should have the "kind" tag field
    for resolution in resolutions {
        let field_names: Vec<_> = resolution.fields().keys().copied().collect();
        assert!(
            field_names.contains(&"kind"),
            "Expected 'kind' tag field, got: {field_names:?}"
        );
    }
}
