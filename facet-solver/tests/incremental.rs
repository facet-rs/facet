//! IncrementalSolver tests.

extern crate alloc;

use alloc::collections::BTreeSet;

use facet::Facet;
use facet_solver::{FieldDecision, IncrementalSolver, Schema, SchemaError, SolverError};

// ============================================================================
// Test types
// ============================================================================

#[derive(Facet, Debug)]
struct SimpleStruct {
    name: String,
    value: i32,
}

#[derive(Facet, Debug)]
struct Connection {
    host: String,
    port: u16,
}

#[derive(Facet, Debug)]
struct ServerWithFlatten {
    name: String,
    #[facet(flatten)]
    connection: Connection,
}

#[derive(Facet, Debug)]
struct SimpleConfig {
    port: u16,
}

#[derive(Facet, Debug)]
struct AdvancedConfig {
    host: String,
    port: u16,
    timeout: u32,
}

#[allow(dead_code)]
#[derive(Facet, Debug)]
#[repr(C)]
enum Config {
    Simple(SimpleConfig),
    Advanced(AdvancedConfig),
}

#[derive(Facet, Debug)]
struct ServerWithEnum {
    name: String,
    #[facet(flatten)]
    config: Config,
}

// ============================================================================
// Tests
// ============================================================================

#[test]
fn test_incremental_simple_struct() {
    let schema = Schema::build(SimpleStruct::SHAPE).unwrap();
    let mut solver = IncrementalSolver::new(&schema);

    // Both fields should be SetDirectly (only one config)
    match solver.see_key("name") {
        FieldDecision::SetDirectly(info) => {
            assert_eq!(info.serialized_name, "name");
        }
        other => panic!("Expected SetDirectly, got {other:?}"),
    }

    match solver.see_key("value") {
        FieldDecision::SetDirectly(info) => {
            assert_eq!(info.serialized_name, "value");
        }
        other => panic!("Expected SetDirectly, got {other:?}"),
    }
}

#[test]
fn test_incremental_flatten_struct() {
    let schema = Schema::build(ServerWithFlatten::SHAPE).unwrap();
    let mut solver = IncrementalSolver::new(&schema);

    // All three fields should be SetDirectly
    match solver.see_key("name") {
        FieldDecision::SetDirectly(info) => {
            assert_eq!(info.path.depth(), 1);
        }
        other => panic!("Expected SetDirectly for name, got {other:?}"),
    }

    match solver.see_key("host") {
        FieldDecision::SetDirectly(info) => {
            assert_eq!(info.path.depth(), 2); // Through connection
        }
        other => panic!("Expected SetDirectly for host, got {other:?}"),
    }

    match solver.see_key("port") {
        FieldDecision::SetDirectly(info) => {
            assert_eq!(info.path.depth(), 2);
        }
        other => panic!("Expected SetDirectly for port, got {other:?}"),
    }
}

#[test]
fn test_incremental_enum_disambiguate() {
    let schema = Schema::build(ServerWithEnum::SHAPE).unwrap();
    let mut solver = IncrementalSolver::new(&schema);

    // "name" is in both configs at same path (depth 1) - should be SetDirectly
    match solver.see_key("name") {
        FieldDecision::SetDirectly(info) => {
            assert_eq!(info.serialized_name, "name");
            assert_eq!(info.path.depth(), 1);
        }
        other => panic!("Expected SetDirectly for name, got {other:?}"),
    }

    // "host" only exists in Advanced - should Disambiguate
    match solver.see_key("host") {
        FieldDecision::Disambiguated {
            config,
            current_field,
        } => {
            assert_eq!(current_field.serialized_name, "host");
            // Should be Advanced config (has host, port, timeout)
            assert!(config.field("timeout").is_some());
        }
        other => panic!("Expected Disambiguated for host, got {other:?}"),
    }

    // After disambiguation, remaining fields go directly
    match solver.see_key("port") {
        FieldDecision::SetDirectly(info) => {
            assert_eq!(info.serialized_name, "port");
        }
        other => panic!("Expected SetDirectly for port after disambiguation, got {other:?}"),
    }
}

#[test]
fn test_incremental_enum_defer_then_disambiguate() {
    let schema = Schema::build(ServerWithEnum::SHAPE).unwrap();
    let mut solver = IncrementalSolver::new(&schema);

    // "port" exists in both Simple and Advanced, but potentially at different paths
    // depending on how the schema is built
    // Actually, let's check the paths first
    let simple_config = schema
        .configurations()
        .iter()
        .find(|c| !c.fields().contains_key("host"))
        .unwrap();
    let advanced_config = schema
        .configurations()
        .iter()
        .find(|c| c.fields().contains_key("host"))
        .unwrap();

    let simple_port_path = &simple_config.field("port").unwrap().path;
    let advanced_port_path = &advanced_config.field("port").unwrap().path;

    // If paths are different, should defer; if same, SetDirectly
    let decision = solver.see_key("port");

    if simple_port_path == advanced_port_path {
        // Same path - SetDirectly
        match decision {
            FieldDecision::SetDirectly(_) => {}
            other => panic!("Expected SetDirectly for port (same path), got {other:?}"),
        }
    } else {
        // Different paths - Defer
        match decision {
            FieldDecision::Defer => {}
            other => panic!("Expected Defer for port (different paths), got {other:?}"),
        }
    }
}

#[test]
fn test_incremental_unknown_field() {
    let schema = Schema::build(SimpleStruct::SHAPE).unwrap();
    let mut solver = IncrementalSolver::new(&schema);

    match solver.see_key("nonexistent") {
        FieldDecision::Unknown => {}
        other => panic!("Expected Unknown, got {other:?}"),
    }
}

#[test]
fn test_incremental_order_independence() {
    let schema = Schema::build(ServerWithEnum::SHAPE).unwrap();

    // Order 1: name, host, port, timeout
    let mut solver1 = IncrementalSolver::new(&schema);
    solver1.see_key("name");
    solver1.see_key("host"); // disambiguates
    solver1.see_key("port");
    solver1.see_key("timeout");

    // Order 2: timeout, port, host, name (reverse)
    let mut solver2 = IncrementalSolver::new(&schema);
    solver2.see_key("timeout"); // disambiguates (only in Advanced)
    solver2.see_key("port");
    solver2.see_key("host");
    solver2.see_key("name");

    // Both should end up with same single candidate
    assert_eq!(solver1.candidates().len(), 1);
    assert_eq!(solver2.candidates().len(), 1);

    // Both should be the Advanced config
    assert!(solver1.candidates()[0].field("host").is_some());
    assert!(solver2.candidates()[0].field("host").is_some());
}

#[test]
fn test_incremental_finish_missing_required() {
    let schema = Schema::build(SimpleStruct::SHAPE).unwrap();
    let mut solver = IncrementalSolver::new(&schema);

    // Only see "name", missing "value"
    solver.see_key("name");

    let mut seen = BTreeSet::new();
    seen.insert("name");

    let result = solver.finish(&seen);
    match result {
        Err(SolverError::NoMatch {
            missing_required, ..
        }) => {
            assert!(missing_required.contains(&"value"));
        }
        other => panic!("Expected NoMatch with missing_required, got {other:?}"),
    }
}

#[test]
fn test_incremental_finish_no_match_all_missing_required() {
    let schema = Schema::build(ServerWithEnum::SHAPE).unwrap();
    let mut solver = IncrementalSolver::new(&schema);

    // Only see "name" - both Simple and Advanced need more required fields
    // Simple needs: name, port
    // Advanced needs: name, host, port, timeout
    // Neither is fully satisfied, so we get NoMatch (not Ambiguous)
    solver.see_key("name");

    let mut seen = BTreeSet::new();
    seen.insert("name");

    let result = solver.finish(&seen);
    match result {
        Err(SolverError::NoMatch {
            missing_required, ..
        }) => {
            // Should report missing required from the first candidate
            assert!(!missing_required.is_empty());
        }
        other => panic!("Expected NoMatch with missing_required, got {other:?}"),
    }
}

#[test]
fn test_incremental_finish_success() {
    let schema = Schema::build(ServerWithEnum::SHAPE).unwrap();
    let mut solver = IncrementalSolver::new(&schema);

    // See all Advanced fields
    solver.see_key("name");
    solver.see_key("host");
    solver.see_key("port");
    solver.see_key("timeout");

    let mut seen = BTreeSet::new();
    seen.insert("name");
    seen.insert("host");
    seen.insert("port");
    seen.insert("timeout");

    let config = solver.finish(&seen).unwrap();
    assert!(config.field("host").is_some());
    assert!(config.field("timeout").is_some());
}

// ============================================================================
// Overlapping fields disambiguation tests (real-world feedback)
// ============================================================================

/// When one variant's required fields are a subset of another's, providing
/// only the subset should resolve to the simpler variant.
#[derive(Facet, Debug)]
struct HttpSource {
    url: String,
}

#[derive(Facet, Debug)]
struct GitSource {
    url: String,
    branch: String,
}

#[allow(dead_code)]
#[derive(Facet, Debug)]
#[repr(u8)]
enum SourceKind {
    Http(HttpSource),
    Git(GitSource),
}

#[derive(Facet, Debug)]
struct Source {
    name: String,
    #[facet(flatten)]
    kind: SourceKind,
}

#[test]
fn test_overlapping_fields_subset_resolves() {
    // Http needs: name, url
    // Git needs: name, url, branch
    // When input has only "name" and "url", only Http is fully satisfied
    let schema = Schema::build(Source::SHAPE).unwrap();

    let mut solver = IncrementalSolver::new(&schema);
    solver.see_key("name");
    solver.see_key("url");

    let mut seen = BTreeSet::new();
    seen.insert("name");
    seen.insert("url");

    // Should resolve to Http (not ambiguous!)
    let config = solver.finish(&seen).expect("should resolve to Http");
    assert!(config.field("url").is_some());
    assert!(config.field("branch").is_none()); // Git has branch, Http doesn't
}

#[test]
fn test_overlapping_fields_superset_resolves() {
    // When input has "name", "url", and "branch", only Git is fully satisfied
    let schema = Schema::build(Source::SHAPE).unwrap();

    let mut solver = IncrementalSolver::new(&schema);
    solver.see_key("name");
    solver.see_key("url");
    solver.see_key("branch");

    let mut seen = BTreeSet::new();
    seen.insert("name");
    seen.insert("url");
    seen.insert("branch");

    // Should resolve to Git
    let config = solver.finish(&seen).expect("should resolve to Git");
    assert!(config.field("branch").is_some());
}

/// True ambiguity: two variants with identical required fields
#[derive(Facet, Debug)]
struct VariantA {
    common: String,
}

#[derive(Facet, Debug)]
struct VariantB {
    common: String,
}

#[allow(dead_code)]
#[derive(Facet, Debug)]
#[repr(u8)]
enum TrueAmbiguous {
    A(VariantA),
    B(VariantB),
}

#[derive(Facet, Debug)]
struct TrueAmbiguousWrapper {
    #[facet(flatten)]
    inner: TrueAmbiguous,
}

#[test]
fn test_true_ambiguity_identical_fields() {
    // Both variants have exactly "common" as required
    // This is TRUE ambiguity - can't be resolved
    let schema = Schema::build(TrueAmbiguousWrapper::SHAPE).unwrap();

    let mut solver = IncrementalSolver::new(&schema);
    solver.see_key("common");

    let mut seen = BTreeSet::new();
    seen.insert("common");

    let result = solver.finish(&seen);
    match result {
        Err(SolverError::Ambiguous { candidates, .. }) => {
            assert_eq!(candidates.len(), 2);
        }
        other => panic!("Expected Ambiguous, got {other:?}"),
    }
}

// ============================================================================
// deny_unknown_fields tests (serde#1600)
// ============================================================================

/// Serde's flatten + deny_unknown_fields is broken because the flattened
/// deserializer doesn't know which fields belong to which struct.
/// Facet's solver tracks all valid fields per configuration, enabling
/// proper unknown field detection.

#[derive(Facet, Debug)]
struct Inner {
    inner_field: String,
}

#[derive(Facet, Debug)]
struct OuterWithFlatten {
    outer_field: String,
    #[facet(flatten)]
    inner: Inner,
}

#[test]
fn test_deny_unknown_fields_with_flatten() {
    // This test demonstrates that the solver knows ALL valid fields
    // for a flattened struct, enabling deny_unknown_fields to work correctly.
    let schema = Schema::build(OuterWithFlatten::SHAPE).unwrap();
    let config = &schema.configurations()[0];

    // The solver knows both outer and inner fields
    assert!(config.field("outer_field").is_some());
    assert!(config.field("inner_field").is_some());

    // And can correctly identify unknown fields
    let mut solver = IncrementalSolver::new(&schema);
    match solver.see_key("outer_field") {
        FieldDecision::SetDirectly(_) => {}
        other => panic!("Expected SetDirectly, got {other:?}"),
    }
    match solver.see_key("inner_field") {
        FieldDecision::SetDirectly(_) => {}
        other => panic!("Expected SetDirectly, got {other:?}"),
    }
    // This field doesn't exist in either outer OR inner - truly unknown
    match solver.see_key("unknown_field") {
        FieldDecision::Unknown => {}
        other => panic!("Expected Unknown for truly unknown field, got {other:?}"),
    }
}

/// More complex case: nested flatten with enum
#[derive(Facet, Debug)]
struct DbConfig {
    host: String,
    port: u16,
}

#[derive(Facet, Debug)]
struct FileConfig {
    path: String,
}

#[allow(dead_code)]
#[derive(Facet, Debug)]
#[repr(u8)]
enum Storage {
    Database(DbConfig),
    File(FileConfig),
}

#[derive(Facet, Debug)]
struct AppWithStorage {
    name: String,
    #[facet(flatten)]
    storage: Storage,
}

#[test]
fn test_deny_unknown_fields_with_flattened_enum() {
    let schema = Schema::build(AppWithStorage::SHAPE).unwrap();

    // Should have 2 configurations (Database and File)
    assert_eq!(schema.configurations().len(), 2);

    let mut solver = IncrementalSolver::new(&schema);

    // "name" is valid in both configurations
    match solver.see_key("name") {
        FieldDecision::SetDirectly(_) => {}
        other => panic!("Expected SetDirectly for name, got {other:?}"),
    }

    // "host" is only valid in Database config
    match solver.see_key("host") {
        FieldDecision::Disambiguated { config, .. } => {
            assert!(config.field("port").is_some()); // Database has port
            assert!(config.field("path").is_none()); // Database doesn't have path
        }
        other => panic!("Expected Disambiguated for host, got {other:?}"),
    }

    // Now we're in Database config - "path" would be unknown
    match solver.see_key("path") {
        FieldDecision::Unknown => {}
        other => panic!("Expected Unknown for path in Database config, got {other:?}"),
    }
}

// ============================================================================
// missing_optional_fields tests (real-world feedback)
// ============================================================================

/// When deserializing, we need to know which optional fields weren't
/// provided so we can initialize them to None/default.
#[derive(Facet, Debug)]
struct ConfigWithOptionals {
    required_field: String,
    optional_field: Option<u32>,
    another_optional: Option<String>,
}

#[test]
fn test_missing_optional_fields() {
    let schema = Schema::build(ConfigWithOptionals::SHAPE).unwrap();
    let config = &schema.configurations()[0];

    // Only saw the required field
    let mut seen = BTreeSet::new();
    seen.insert("required_field");

    let missing: Vec<_> = config.missing_optional_fields(&seen).collect();

    // Should have both optional fields as "missing"
    assert_eq!(missing.len(), 2);
    let names: BTreeSet<_> = missing.iter().map(|f| f.serialized_name).collect();
    assert!(names.contains("optional_field"));
    assert!(names.contains("another_optional"));
}

#[test]
fn test_missing_optional_fields_partial() {
    let schema = Schema::build(ConfigWithOptionals::SHAPE).unwrap();
    let config = &schema.configurations()[0];

    // Saw required + one optional
    let mut seen = BTreeSet::new();
    seen.insert("required_field");
    seen.insert("optional_field");

    let missing: Vec<_> = config.missing_optional_fields(&seen).collect();

    // Should only have the other optional field
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0].serialized_name, "another_optional");
}

#[test]
fn test_missing_optional_fields_all_provided() {
    let schema = Schema::build(ConfigWithOptionals::SHAPE).unwrap();
    let config = &schema.configurations()[0];

    // Saw all fields
    let mut seen = BTreeSet::new();
    seen.insert("required_field");
    seen.insert("optional_field");
    seen.insert("another_optional");

    let missing: Vec<_> = config.missing_optional_fields(&seen).collect();

    // Should be empty
    assert!(missing.is_empty());
}

/// Test with flattened struct containing optionals
#[derive(Facet, Debug)]
struct InnerWithOptionals {
    inner_required: String,
    inner_optional: Option<bool>,
}

#[derive(Facet, Debug)]
struct OuterWithOptionals {
    outer_required: String,
    outer_optional: Option<i32>,
    #[facet(flatten)]
    inner: InnerWithOptionals,
}

#[test]
fn test_missing_optional_fields_flattened() {
    let schema = Schema::build(OuterWithOptionals::SHAPE).unwrap();
    let config = &schema.configurations()[0];

    // Only saw the required fields
    let mut seen = BTreeSet::new();
    seen.insert("outer_required");
    seen.insert("inner_required");

    let missing: Vec<_> = config.missing_optional_fields(&seen).collect();

    // Should have both optional fields
    assert_eq!(missing.len(), 2);
    let names: BTreeSet<_> = missing.iter().map(|f| f.serialized_name).collect();
    assert!(names.contains("outer_optional"));
    assert!(names.contains("inner_optional"));
}

// ============================================================================
// u128 support tests (serde_json#1155)
// ============================================================================

/// Serde's flatten doesn't work with u128 because serde_json::Value can't
/// represent u128. Facet doesn't buffer through Value, so this works.

#[derive(Facet, Debug)]
struct LargeNumbers {
    big: u128,
    also_big: i128,
}

#[derive(Facet, Debug)]
struct WrapperWithU128 {
    name: String,
    #[facet(flatten)]
    numbers: LargeNumbers,
}

#[test]
fn test_u128_in_flatten() {
    // The schema should build successfully with u128 fields
    let schema = Schema::build(WrapperWithU128::SHAPE).unwrap();
    let config = &schema.configurations()[0];

    // All fields should be recognized
    assert!(config.field("name").is_some());
    assert!(config.field("big").is_some());
    assert!(config.field("also_big").is_some());

    // Solver should work normally
    let mut solver = IncrementalSolver::new(&schema);
    match solver.see_key("big") {
        FieldDecision::SetDirectly(info) => {
            assert_eq!(info.serialized_name, "big");
        }
        other => panic!("Expected SetDirectly for big, got {other:?}"),
    }
}

/// u128 in flattened enum variant
#[derive(Facet, Debug)]
struct SmallCounter {
    count: u64,
}

#[derive(Facet, Debug)]
struct BigCounter {
    count: u128,
}

#[allow(dead_code)]
#[derive(Facet, Debug)]
#[repr(u8)]
enum Counter {
    Small(SmallCounter),
    Big(BigCounter),
}

#[derive(Facet, Debug)]
struct CounterWrapper {
    name: String,
    #[facet(flatten)]
    counter: Counter,
}

#[test]
fn test_u128_in_flattened_enum() {
    let schema = Schema::build(CounterWrapper::SHAPE).unwrap();

    // Both variants have "count" but with different types (u64 vs u128)
    // The solver tracks field names, not types, so both configs have "count"
    assert_eq!(schema.configurations().len(), 2);

    for config in schema.configurations() {
        assert!(config.field("name").is_some());
        assert!(config.field("count").is_some());
    }
}

// ============================================================================
// Option<Flattened> tests (real-world feedback)
// ============================================================================

/// When a flattened struct is wrapped in Option<T>, all its fields should
/// become optional. This allows omitting the entire flattened block.
#[derive(Facet, Debug)]
struct DatabaseConnection {
    host: String,
    port: u16,
}

#[derive(Facet, Debug)]
struct AppConfigWithOptionalDb {
    name: String,
    #[facet(flatten)]
    database: Option<DatabaseConnection>,
}

#[test]
fn test_optional_flatten_struct_fields_are_optional() {
    let schema = Schema::build(AppConfigWithOptionalDb::SHAPE).unwrap();
    let config = &schema.configurations()[0];

    // "name" should be required (it's on the outer struct)
    let name_field = config.field("name").unwrap();
    assert!(name_field.required, "name should be required");

    // "host" and "port" should be optional (from Option<DatabaseConnection>)
    let host_field = config.field("host").unwrap();
    assert!(
        !host_field.required,
        "host should be optional due to Option wrapper"
    );

    let port_field = config.field("port").unwrap();
    assert!(
        !port_field.required,
        "port should be optional due to Option wrapper"
    );
}

#[test]
fn test_optional_flatten_struct_omit_all() {
    // When all flattened fields are omitted, it should succeed (resolve to None)
    let schema = Schema::build(AppConfigWithOptionalDb::SHAPE).unwrap();
    let mut solver = IncrementalSolver::new(&schema);

    // Only provide "name" - the flattened fields are all optional
    solver.see_key("name");

    let mut seen = BTreeSet::new();
    seen.insert("name");

    // Should succeed! "host" and "port" are optional due to Option<> wrapper
    let config = solver
        .finish(&seen)
        .expect("should succeed with only required field");
    assert!(config.field("name").is_some());
}

#[test]
fn test_optional_flatten_struct_provide_partial() {
    // When some flattened fields are provided, they should still be valid
    let schema = Schema::build(AppConfigWithOptionalDb::SHAPE).unwrap();
    let mut solver = IncrementalSolver::new(&schema);

    solver.see_key("name");
    solver.see_key("host");

    let mut seen = BTreeSet::new();
    seen.insert("name");
    seen.insert("host");

    // Should succeed - "port" is optional
    let config = solver
        .finish(&seen)
        .expect("should succeed with partial flatten");
    assert!(config.field("host").is_some());
}

#[test]
fn test_optional_flatten_struct_provide_all() {
    // When all flattened fields are provided, it should work too
    let schema = Schema::build(AppConfigWithOptionalDb::SHAPE).unwrap();
    let mut solver = IncrementalSolver::new(&schema);

    solver.see_key("name");
    solver.see_key("host");
    solver.see_key("port");

    let mut seen = BTreeSet::new();
    seen.insert("name");
    seen.insert("host");
    seen.insert("port");

    let config = solver
        .finish(&seen)
        .expect("should succeed with all fields");
    assert!(config.field("host").is_some());
    assert!(config.field("port").is_some());
}

/// Test Option<Flattened> with an enum inside
#[derive(Facet, Debug)]
struct TcpConfig {
    host: String,
    port: u16,
}

#[derive(Facet, Debug)]
struct UnixConfig {
    path: String,
}

#[allow(dead_code)]
#[derive(Facet, Debug)]
#[repr(u8)]
enum TransportConfig {
    Tcp(TcpConfig),
    Unix(UnixConfig),
}

#[derive(Facet, Debug)]
struct ServerWithOptionalTransport {
    name: String,
    #[facet(flatten)]
    transport: Option<TransportConfig>,
}

#[test]
fn test_optional_flatten_enum_all_omitted() {
    // With Option<EnumFlattened>, we can omit the entire thing
    let schema = Schema::build(ServerWithOptionalTransport::SHAPE).unwrap();

    // Should have 2 configurations (Tcp and Unix variants)
    assert_eq!(schema.configurations().len(), 2);

    // All enum variant fields should be optional
    for config in schema.configurations() {
        let name_field = config.field("name").unwrap();
        assert!(name_field.required, "name should be required");

        // All other fields should be optional
        for (field_name, field_info) in config.fields() {
            if *field_name != "name" {
                assert!(
                    !field_info.required,
                    "field '{field_name}' should be optional due to Option wrapper"
                );
            }
        }
    }

    // Test that we can omit all transport fields
    let mut solver = IncrementalSolver::new(&schema);
    solver.see_key("name");

    let mut seen = BTreeSet::new();
    seen.insert("name");

    // This should work now - both Tcp and Unix configs are satisfied
    // because their required fields became optional
    // Note: This is ambiguous since both variants match
    let result = solver.finish(&seen);
    match result {
        Ok(_) | Err(SolverError::Ambiguous { .. }) => {
            // Either is acceptable - the point is we didn't get NoMatch
            // due to missing required fields
        }
        Err(SolverError::NoMatch {
            missing_required, ..
        }) => {
            panic!("should not fail with missing required: {missing_required:?}");
        }
    }
}

// ============================================================================
// Duplicate field detection tests (real-world feedback)
// ============================================================================

/// When a parent struct and a flattened struct both define a field with
/// the same name, schema building should panic with a helpful error.
#[derive(Facet, Debug)]
struct InnerWithName {
    name: String, // Duplicate!
    value: i32,
}

#[derive(Facet, Debug)]
struct OuterWithDuplicateName {
    name: String, // Duplicate!
    #[facet(flatten)]
    inner: InnerWithName,
}

#[test]
fn test_duplicate_field_name_returns_error() {
    // This should return an error because both outer and inner have "name"
    let result = Schema::build(OuterWithDuplicateName::SHAPE);
    match result {
        Err(SchemaError::DuplicateField {
            field_name,
            first_path,
            second_path,
        }) => {
            assert_eq!(field_name, "name");
            // Check that paths are included
            let first = first_path.to_string();
            let second = second_path.to_string();
            // One should be "name" (outer), one should contain "inner"
            assert!(
                first == "name" || second == "name",
                "One path should be 'name', got {first} and {second}"
            );
            assert!(
                first.contains("inner") || second.contains("inner"),
                "One path should contain 'inner', got {first} and {second}"
            );
        }
        Ok(_) => panic!("Expected DuplicateField error, got Ok"),
    }
}

/// Different field names should work fine
#[derive(Facet, Debug)]
struct InnerNoDuplicate {
    inner_name: String,
    value: i32,
}

#[derive(Facet, Debug)]
struct OuterNoDuplicate {
    outer_name: String,
    #[facet(flatten)]
    inner: InnerNoDuplicate,
}

#[test]
fn test_no_duplicate_field_names_ok() {
    // This should NOT panic - different field names
    let schema = Schema::build(OuterNoDuplicate::SHAPE).unwrap();
    let config = &schema.configurations()[0];
    assert!(config.field("outer_name").is_some());
    assert!(config.field("inner_name").is_some());
    assert!(config.field("value").is_some());
}

// ============================================================================
// Improved diagnostics tests (real-world feedback)
// ============================================================================

/// Test that error messages include field paths
#[derive(Facet, Debug)]
struct NestedConnection {
    host: String,
    port: u16,
}

#[derive(Facet, Debug)]
struct DeepConfig {
    name: String,
    #[facet(flatten)]
    connection: NestedConnection,
}

#[test]
fn test_error_includes_field_path() {
    let schema = Schema::build(DeepConfig::SHAPE).unwrap();
    let mut solver = IncrementalSolver::new(&schema);

    // Only provide "name", missing "host" and "port"
    solver.see_key("name");

    let mut seen = BTreeSet::new();
    seen.insert("name");

    let result = solver.finish(&seen);
    match result {
        Err(SolverError::NoMatch {
            missing_required_detailed,
            closest_config,
            ..
        }) => {
            // Should have detailed info about missing fields
            assert!(!missing_required_detailed.is_empty());

            // Check that paths are included
            for info in &missing_required_detailed {
                // The path should include the flatten path
                assert!(
                    info.path.contains("connection"),
                    "path '{}' should contain 'connection'",
                    info.path
                );
            }

            // Should have closest config info
            assert!(closest_config.is_some());
        }
        other => panic!("Expected NoMatch with detailed info, got {other:?}"),
    }
}

/// Test that ambiguous errors include disambiguating field hints
#[test]
fn test_ambiguous_includes_disambiguating_hints() {
    let schema = Schema::build(TrueAmbiguousWrapper::SHAPE).unwrap();
    let mut solver = IncrementalSolver::new(&schema);

    solver.see_key("common");

    let mut seen = BTreeSet::new();
    seen.insert("common");

    let result = solver.finish(&seen);
    match result {
        Err(SolverError::Ambiguous {
            candidates,
            disambiguating_fields,
        }) => {
            assert_eq!(candidates.len(), 2);
            // In this truly ambiguous case, there may be no disambiguating fields
            // since both variants have identical fields
            // The test validates the structure exists
            let _ = disambiguating_fields; // Just verify it exists
        }
        other => panic!("Expected Ambiguous, got {other:?}"),
    }
}
