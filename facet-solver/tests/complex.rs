//! Complex nesting and edge case tests.

extern crate alloc;

use alloc::collections::BTreeSet;

use facet::Facet;
use facet_solver::{FieldDecision, IncrementalSolver, Schema, SolverError};

// ============================================================================
// Three levels of nesting with flatten at each level
// ============================================================================

#[derive(Facet, Debug)]
struct Level3 {
    deep_field: i32,
    another_deep: String,
}

#[derive(Facet, Debug)]
struct Level2 {
    mid_field: i32,
    #[facet(flatten)]
    level3: Level3,
}

#[derive(Facet, Debug)]
struct Level1 {
    top_field: String,
    #[facet(flatten)]
    level2: Level2,
}

#[test]
fn test_three_level_nesting_schema() {
    let schema = Schema::build(Level1::SHAPE).unwrap();

    // Single config (no enums)
    assert_eq!(schema.resolutions().len(), 1);

    let config = &schema.resolutions()[0];

    // Should have 4 fields at different depths
    assert_eq!(config.fields().len(), 4);

    // Check depths
    assert_eq!(config.field("top_field").unwrap().path.depth(), 1);
    assert_eq!(config.field("mid_field").unwrap().path.depth(), 2);
    assert_eq!(config.field("deep_field").unwrap().path.depth(), 3);
    assert_eq!(config.field("another_deep").unwrap().path.depth(), 3);
}

#[test]
fn test_three_level_nesting_incremental() {
    let schema = Schema::build(Level1::SHAPE).unwrap();
    let mut solver = IncrementalSolver::new(&schema);

    // All should be SetDirectly (single config)
    for field in ["top_field", "mid_field", "deep_field", "another_deep"] {
        match solver.see_key(field) {
            FieldDecision::SetDirectly(info) => {
                assert_eq!(info.serialized_name, field);
            }
            other => panic!("Expected SetDirectly for {field}, got {other:?}"),
        }
    }
}

// ============================================================================
// Multiple Enums - Cartesian Product of Configurations
// ============================================================================

#[derive(Facet, Debug)]
struct AuthPassword {
    password: String,
}

#[derive(Facet, Debug)]
struct AuthToken {
    token: String,
    token_expiry: u64,
}

#[allow(dead_code)]
#[derive(Facet, Debug)]
#[repr(C)]
enum AuthMethod {
    Password(AuthPassword),
    Token(AuthToken),
}

#[derive(Facet, Debug)]
struct TransportTcp {
    tcp_port: u16,
}

#[derive(Facet, Debug)]
struct TransportUnix {
    socket_path: String,
}

#[allow(dead_code)]
#[derive(Facet, Debug)]
#[repr(C)]
enum Transport {
    Tcp(TransportTcp),
    Unix(TransportUnix),
}

#[derive(Facet, Debug)]
struct ServiceConfig {
    name: String,
    #[facet(flatten)]
    auth: AuthMethod,
    #[facet(flatten)]
    transport: Transport,
}

#[test]
fn test_multiple_enums_schema() {
    let schema = Schema::build(ServiceConfig::SHAPE).unwrap();

    // 2 auth methods × 2 transports = 4 configurations
    assert_eq!(schema.resolutions().len(), 4);

    // Each config should have name + auth fields + transport fields
    for config in schema.resolutions() {
        assert!(config.field("name").is_some());

        // Either password or (token + token_expiry)
        let has_password = config.field("password").is_some();
        let has_token = config.field("token").is_some();
        assert!(
            has_password ^ has_token,
            "Should have exactly one auth method"
        );

        // Either tcp_port or socket_path
        let has_tcp = config.field("tcp_port").is_some();
        let has_unix = config.field("socket_path").is_some();
        assert!(has_tcp ^ has_unix, "Should have exactly one transport");
    }
}

#[test]
fn test_multiple_enums_incremental_password_tcp() {
    let schema = Schema::build(ServiceConfig::SHAPE).unwrap();
    let mut solver = IncrementalSolver::new(&schema);

    // "name" is in all 4 configs at same path
    match solver.see_key("name") {
        FieldDecision::SetDirectly(_) => {}
        other => panic!("Expected SetDirectly for name, got {other:?}"),
    }
    assert_eq!(solver.candidates().len(), 4);

    // "password" narrows to 2 configs (Password × Tcp, Password × Unix)
    match solver.see_key("password") {
        FieldDecision::SetDirectly(_) | FieldDecision::Defer => {
            // Either is fine depending on path equality
        }
        FieldDecision::Disambiguated { .. } => {
            panic!("password shouldn't fully disambiguate (still 2 candidates)");
        }
        other => panic!("Unexpected for password: {other:?}"),
    }
    assert_eq!(solver.candidates().len(), 2);

    // "tcp_port" narrows to 1 resolution (Password × Tcp)
    match solver.see_key("tcp_port") {
        FieldDecision::Disambiguated { resolution, .. } => {
            assert!(resolution.field("password").is_some());
            assert!(resolution.field("tcp_port").is_some());
        }
        other => panic!("Expected Disambiguated for tcp_port, got {other:?}"),
    }
    assert_eq!(solver.candidates().len(), 1);
}

#[test]
fn test_multiple_enums_incremental_token_unix() {
    let schema = Schema::build(ServiceConfig::SHAPE).unwrap();
    let mut solver = IncrementalSolver::new(&schema);

    // Start with a transport field
    solver.see_key("socket_path"); // Narrows to Unix variants (2 configs)
    assert_eq!(solver.candidates().len(), 2);

    // Add auth field
    match solver.see_key("token") {
        FieldDecision::Disambiguated { resolution, .. } => {
            assert!(resolution.field("token").is_some());
            assert!(resolution.field("socket_path").is_some());
            assert!(resolution.field("token_expiry").is_some());
        }
        other => panic!("Expected Disambiguated for token, got {other:?}"),
    }
}

// ============================================================================
// Enum Inside Enum (Nested Enum Flattening)
// ============================================================================

#[derive(Facet, Debug)]
struct BasicRetry {
    max_retries: u32,
}

#[derive(Facet, Debug)]
struct ExponentialRetry {
    max_retries: u32,
    base_delay_ms: u64,
    max_delay_ms: u64,
}

#[allow(dead_code)]
#[derive(Facet, Debug)]
#[repr(C)]
enum RetryStrategy {
    Basic(BasicRetry),
    Exponential(ExponentialRetry),
}

#[derive(Facet, Debug)]
struct FullFeatured {
    enabled: bool,
    #[facet(flatten)]
    retry: RetryStrategy,
}

#[derive(Facet, Debug)]
struct Minimal {
    enabled: bool,
}

#[allow(dead_code)]
#[derive(Facet, Debug)]
#[repr(C)]
enum FeatureLevel {
    Full(FullFeatured),
    Min(Minimal),
}

#[derive(Facet, Debug)]
struct AppConfig {
    app_name: String,
    #[facet(flatten)]
    features: FeatureLevel,
}

#[test]
fn test_nested_enum_schema() {
    let schema = Schema::build(AppConfig::SHAPE).unwrap();

    // Min has 1 config
    // Full has 2 configs (Basic, Exponential retry)
    // Total: 3 configurations
    assert_eq!(schema.resolutions().len(), 3);

    // Find each config type
    let min_config = schema
        .resolutions()
        .iter()
        .find(|c| c.field("max_retries").is_none())
        .expect("Should have Minimal config");

    let basic_config = schema
        .resolutions()
        .iter()
        .find(|c| c.field("max_retries").is_some() && c.field("base_delay_ms").is_none())
        .expect("Should have Basic retry config");

    let exp_config = schema
        .resolutions()
        .iter()
        .find(|c| c.field("base_delay_ms").is_some())
        .expect("Should have Exponential retry config");

    // Min: app_name, enabled
    assert_eq!(min_config.fields().len(), 2);

    // Basic: app_name, enabled, max_retries
    assert_eq!(basic_config.fields().len(), 3);

    // Exponential: app_name, enabled, max_retries, base_delay_ms, max_delay_ms
    assert_eq!(exp_config.fields().len(), 5);
}

#[test]
fn test_nested_enum_incremental_minimal() {
    let schema = Schema::build(AppConfig::SHAPE).unwrap();
    let mut solver = IncrementalSolver::new(&schema);

    // app_name is in all configs
    match solver.see_key("app_name") {
        FieldDecision::SetDirectly(_) => {}
        other => panic!("Expected SetDirectly for app_name, got {other:?}"),
    }

    // enabled is in all configs (Min and Full both have it)
    match solver.see_key("enabled") {
        FieldDecision::SetDirectly(_) | FieldDecision::Defer => {}
        FieldDecision::Disambiguated { .. } => {
            panic!("enabled shouldn't disambiguate");
        }
        other => panic!("Unexpected for enabled: {other:?}"),
    }

    // With just app_name and enabled, we're still ambiguous
    let candidate_count = solver.candidates().len();
    assert!(candidate_count > 1, "Should still be ambiguous");
}

#[test]
fn test_nested_enum_incremental_exponential() {
    let schema = Schema::build(AppConfig::SHAPE).unwrap();
    let mut solver = IncrementalSolver::new(&schema);

    solver.see_key("app_name");
    solver.see_key("enabled");

    // base_delay_ms only exists in Exponential resolution
    match solver.see_key("base_delay_ms") {
        FieldDecision::Disambiguated { resolution, .. } => {
            assert!(resolution.field("base_delay_ms").is_some());
            assert!(resolution.field("max_delay_ms").is_some());
            assert!(resolution.field("max_retries").is_some());
        }
        other => panic!("Expected Disambiguated for base_delay_ms, got {other:?}"),
    }
}

// ============================================================================
// Ambiguity Edge Cases
// ============================================================================

// Two structs that share ALL field names but at different paths
#[derive(Facet, Debug)]
struct SourceA {
    shared_x: i32,
    shared_y: i32,
}

#[derive(Facet, Debug)]
struct SourceB {
    shared_x: i32, // Same name!
    shared_y: i32, // Same name!
}

#[allow(dead_code)]
#[derive(Facet, Debug)]
#[repr(C)]
enum AorB {
    A(SourceA),
    B(SourceB),
}

#[derive(Facet, Debug)]
struct AmbiguousContainer {
    #[facet(flatten)]
    source: AorB,
}

#[test]
fn test_fully_ambiguous_enum() {
    let schema = Schema::build(AmbiguousContainer::SHAPE).unwrap();

    // 2 configs with same field names
    assert_eq!(schema.resolutions().len(), 2);

    for config in schema.resolutions() {
        assert!(config.field("shared_x").is_some());
        assert!(config.field("shared_y").is_some());
    }
}

#[test]
fn test_fully_ambiguous_incremental() {
    let schema = Schema::build(AmbiguousContainer::SHAPE).unwrap();
    let mut solver = IncrementalSolver::new(&schema);

    // shared_x exists in both configs but at different paths
    // (through A vs through B)
    let decision = solver.see_key("shared_x");

    // Check if paths differ
    let configs: Vec<_> = schema.resolutions().iter().collect();
    let path_a = &configs[0].field("shared_x").unwrap().path;
    let path_b = &configs[1].field("shared_x").unwrap().path;

    if path_a == path_b {
        match decision {
            FieldDecision::SetDirectly(_) => {}
            other => panic!("Expected SetDirectly (same paths), got {other:?}"),
        }
    } else {
        match decision {
            FieldDecision::Defer => {}
            other => panic!("Expected Defer (different paths), got {other:?}"),
        }
    }

    // After seeing both fields, still ambiguous
    solver.see_key("shared_y");

    let mut seen = BTreeSet::new();
    seen.insert("shared_x");
    seen.insert("shared_y");

    // This should fail with Ambiguous error
    match solver.finish(&seen) {
        Err(SolverError::Ambiguous { candidates, .. }) => {
            assert_eq!(candidates.len(), 2);
        }
        other => panic!("Expected Ambiguous error, got {other:?}"),
    }
}

// ============================================================================
// Disambiguation at Different Points
// ============================================================================

#[derive(Facet, Debug)]
struct Early {
    unique_early: i32,
    common: String,
}

#[derive(Facet, Debug)]
struct Late {
    common: String,
    unique_late: i32,
}

#[allow(dead_code)]
#[derive(Facet, Debug)]
#[repr(C)]
enum EarlyOrLate {
    Early(Early),
    Late(Late),
}

#[derive(Facet, Debug)]
struct TimingTest {
    always_present: bool,
    #[facet(flatten)]
    timing: EarlyOrLate,
}

#[test]
fn test_early_disambiguation() {
    let schema = Schema::build(TimingTest::SHAPE).unwrap();
    let mut solver = IncrementalSolver::new(&schema);

    solver.see_key("always_present");
    assert_eq!(solver.candidates().len(), 2);

    // unique_early immediately disambiguates
    match solver.see_key("unique_early") {
        FieldDecision::Disambiguated { resolution, .. } => {
            assert!(resolution.field("unique_early").is_some());
            assert!(resolution.field("unique_late").is_none());
        }
        other => panic!("Expected early disambiguation, got {other:?}"),
    }
}

#[test]
fn test_late_disambiguation() {
    let schema = Schema::build(TimingTest::SHAPE).unwrap();
    let mut solver = IncrementalSolver::new(&schema);

    solver.see_key("always_present");
    solver.see_key("common"); // Still ambiguous (in both)
    assert_eq!(solver.candidates().len(), 2);

    // unique_late disambiguates at the end
    match solver.see_key("unique_late") {
        FieldDecision::Disambiguated { resolution, .. } => {
            assert!(resolution.field("unique_late").is_some());
            assert!(resolution.field("unique_early").is_none());
        }
        other => panic!("Expected late disambiguation, got {other:?}"),
    }
}

// ============================================================================
// Gradual Narrowing (4 → 2 → 1)
// ============================================================================

#[derive(Facet, Debug)]
struct P1 {
    p1_field: i32,
}
#[derive(Facet, Debug)]
struct P2 {
    p2_field: i32,
}
#[allow(dead_code)]
#[derive(Facet, Debug)]
#[repr(C)]
enum Primary {
    P1(P1),
    P2(P2),
}

#[derive(Facet, Debug)]
struct S1 {
    s1_field: i32,
}
#[derive(Facet, Debug)]
struct S2 {
    s2_field: i32,
}
#[allow(dead_code)]
#[derive(Facet, Debug)]
#[repr(C)]
enum Secondary {
    S1(S1),
    S2(S2),
}

#[derive(Facet, Debug)]
struct FourWay {
    base: String,
    #[facet(flatten)]
    primary: Primary,
    #[facet(flatten)]
    secondary: Secondary,
}

#[test]
fn test_gradual_narrowing() {
    let schema = Schema::build(FourWay::SHAPE).unwrap();

    // 2 × 2 = 4 configurations
    assert_eq!(schema.resolutions().len(), 4);

    let mut solver = IncrementalSolver::new(&schema);

    // base is in all 4
    solver.see_key("base");
    assert_eq!(solver.candidates().len(), 4);

    // p1_field narrows to 2 (P1×S1, P1×S2)
    solver.see_key("p1_field");
    assert_eq!(solver.candidates().len(), 2);

    // s2_field narrows to 1 (P1×S2)
    match solver.see_key("s2_field") {
        FieldDecision::Disambiguated { resolution, .. } => {
            assert!(resolution.field("p1_field").is_some());
            assert!(resolution.field("s2_field").is_some());
            assert!(resolution.field("p2_field").is_none());
            assert!(resolution.field("s1_field").is_none());
        }
        other => panic!("Expected disambiguation at s2_field, got {other:?}"),
    }
}

#[test]
fn test_gradual_narrowing_different_order() {
    let schema = Schema::build(FourWay::SHAPE).unwrap();
    let mut solver = IncrementalSolver::new(&schema);

    // Start with secondary field
    solver.see_key("s1_field"); // Narrows to (P1×S1, P2×S1)
    assert_eq!(solver.candidates().len(), 2);

    // Then primary field
    match solver.see_key("p2_field") {
        FieldDecision::Disambiguated { resolution, .. } => {
            assert!(resolution.field("p2_field").is_some());
            assert!(resolution.field("s1_field").is_some());
        }
        other => panic!("Expected disambiguation at p2_field, got {other:?}"),
    }
}

// ============================================================================
// Issue #1: Flattened struct with CHILD fields
// ============================================================================
//
// When a flattened struct contains both properties AND child nodes (KDL/XML),
// the solver should handle them correctly. Child fields have FieldFlags::CHILD.

#[derive(Facet, Debug)]
struct TlsConfig {
    cert_path: String,
    key_path: String,
}

/// A struct with both properties (port, host) and a child node (tls)
#[derive(Facet, Debug)]
struct ServiceDetails {
    port: u16,
    host: String,
    // In real KDL usage, this would be marked #[facet(child)]
    // For now we're just testing that flattening works with nested structs
    tls: Option<TlsConfig>,
}

#[derive(Facet, Debug)]
struct Owner {
    name: String,
}

/// A service with a flattened details struct AND a child node
#[derive(Facet, Debug)]
struct Service {
    name: String,
    #[facet(flatten)]
    details: ServiceDetails,
    // This would be #[facet(child)] in KDL
    owner: Owner,
}

#[test]
fn test_flatten_struct_with_nested_struct() {
    // This tests the basic case: flatten a struct that contains a nested struct
    let schema = Schema::build(Service::SHAPE).unwrap();

    // Should have 1 configuration (no enums)
    assert_eq!(schema.resolutions().len(), 1);

    let config = &schema.resolutions()[0];

    // Check all expected fields exist
    assert!(config.field("name").is_some(), "should have name");
    assert!(
        config.field("port").is_some(),
        "should have port (from flattened details)"
    );
    assert!(
        config.field("host").is_some(),
        "should have host (from flattened details)"
    );
    assert!(
        config.field("tls").is_some(),
        "should have tls (from flattened details)"
    );
    assert!(config.field("owner").is_some(), "should have owner");
}

// ============================================================================
// Issue #2: Enum with tuple variants - field extraction
// ============================================================================
//
// When an enum has tuple variants like `Simple(SimpleStruct)`, the solver
// should extract the struct's fields for disambiguation.

/// Simple mode - just has a level
#[derive(Facet, Debug)]
struct SimpleMode {
    level: u8,
}

/// Tuned mode - has level, gain, and a tuning child
#[derive(Facet, Debug)]
struct TuningConfig {
    frequency: u32,
}

#[derive(Facet, Debug)]
struct TunedMode {
    level: u8,
    gain: u8,
    // In KDL this would be #[facet(child)]
    tuning: TuningConfig,
}

/// An enum where disambiguation should happen based on which fields are present
#[allow(dead_code)]
#[derive(Facet, Debug)]
#[repr(u8)]
enum Mode {
    Simple(SimpleMode),
    Tuned(TunedMode),
}

#[derive(Facet, Debug)]
struct ModeContainer {
    #[facet(flatten)]
    mode: Mode,
}

#[test]
fn test_flatten_enum_extracts_tuple_variant_fields() {
    // This tests that the solver extracts fields from tuple variants
    let schema = Schema::build(ModeContainer::SHAPE).unwrap();

    // Should have 2 configurations: one for Simple, one for Tuned
    assert_eq!(
        schema.resolutions().len(),
        2,
        "Should have 2 configurations (Simple and Tuned)"
    );

    // Find the Simple config (only has 'level')
    let simple_config = schema
        .resolutions()
        .iter()
        .find(|c| c.field("gain").is_none())
        .expect("Should have Simple config without 'gain'");

    // Find the Tuned config (has 'level', 'gain', 'tuning')
    let tuned_config = schema
        .resolutions()
        .iter()
        .find(|c| c.field("gain").is_some())
        .expect("Should have Tuned config with 'gain'");

    // Verify Simple config fields
    assert!(
        simple_config.field("level").is_some(),
        "Simple should have 'level'"
    );
    assert!(
        simple_config.field("gain").is_none(),
        "Simple should NOT have 'gain'"
    );
    assert!(
        simple_config.field("tuning").is_none(),
        "Simple should NOT have 'tuning'"
    );

    // Verify Tuned config fields
    assert!(
        tuned_config.field("level").is_some(),
        "Tuned should have 'level'"
    );
    assert!(
        tuned_config.field("gain").is_some(),
        "Tuned should have 'gain'"
    );
    assert!(
        tuned_config.field("tuning").is_some(),
        "Tuned should have 'tuning'"
    );
}

#[test]
fn test_flatten_enum_disambiguates_by_field_presence() {
    let schema = Schema::build(ModeContainer::SHAPE).unwrap();
    let mut solver = IncrementalSolver::new(&schema);

    // 'level' exists in both variants - should not disambiguate
    match solver.see_key("level") {
        FieldDecision::SetDirectly(_) | FieldDecision::Defer => {
            // Both variants have level, so should remain ambiguous
        }
        FieldDecision::Disambiguated { .. } => {
            panic!("'level' should not disambiguate (exists in both variants)");
        }
        FieldDecision::Unknown => {
            panic!("'level' should be known");
        }
    }

    // Should still have 2 candidates
    assert_eq!(
        solver.candidates().len(),
        2,
        "After seeing 'level', should still have 2 candidates"
    );

    // 'gain' only exists in Tuned - should disambiguate
    match solver.see_key("gain") {
        FieldDecision::Disambiguated { resolution, .. } => {
            assert!(
                resolution.field("gain").is_some(),
                "Disambiguated resolution should have 'gain'"
            );
            assert!(
                resolution.field("tuning").is_some(),
                "Disambiguated resolution should have 'tuning' (Tuned variant)"
            );
        }
        other => panic!("Expected Disambiguated for 'gain', got {other:?}"),
    }
}

// ============================================================================
// Issue #2 (variation): Building schema directly on enum (not wrapped)
// ============================================================================
//
// The user mentioned "facet-solver's Schema::build produces only 1 configuration
// with no fields for this enum". Let's test building schema directly on an enum.

#[test]
fn test_schema_build_on_enum_directly() {
    // Build schema directly on the enum (not wrapped in a struct with #[facet(flatten)])
    let schema = Schema::build(Mode::SHAPE).unwrap();

    // Print debug info
    eprintln!("Mode schema configurations: {}", schema.resolutions().len());
    for (i, config) in schema.resolutions().iter().enumerate() {
        eprintln!(
            "  Config {}: {} fields, desc: {}",
            i,
            config.fields().len(),
            config.describe()
        );
        for name in config.fields().keys() {
            eprintln!("    - {name}");
        }
    }

    // The enum has 2 variants (Simple and Tuned)
    // When built directly, should still produce 2 configurations
    //
    // BUG: Currently produces only 1 empty configuration!
    // This is Issue #2: solver doesn't extract variant fields from tuple variants
    // when building schema directly on an enum.
    assert_eq!(
        schema.resolutions().len(),
        2,
        "Enum should produce 2 configurations (one per variant). \
         BUG: Schema::build on enum directly doesn't handle variants correctly."
    );

    // Each configuration should have fields
    for config in schema.resolutions() {
        assert!(
            !config.fields().is_empty(),
            "Each configuration should have fields, got: {}",
            config.describe()
        );
    }
}

// ============================================================================
// Issue #2 (variation 2): Untagged enum style (no discriminant in the data)
// ============================================================================
//
// For untagged enums in formats like JSON/KDL, the variant is determined
// purely by which fields are present.

/// Like Mode but without explicit container - simulating untagged enum handling
#[allow(dead_code)]
#[derive(Facet, Debug)]
#[repr(u8)]
enum UntaggedMode {
    /// Only has level
    Simple(SimpleMode),
    /// Has level, gain, and tuning
    Tuned(TunedMode),
}

#[test]
fn test_untagged_enum_schema_fields() {
    let schema = Schema::build(UntaggedMode::SHAPE).unwrap();

    eprintln!(
        "UntaggedMode schema configurations: {}",
        schema.resolutions().len()
    );
    for (i, config) in schema.resolutions().iter().enumerate() {
        eprintln!(
            "  Config {}: {} fields, desc: {}",
            i,
            config.fields().len(),
            config.describe()
        );
        for name in config.fields().keys() {
            eprintln!("    - {name}");
        }
    }

    // Should have 2 configurations
    assert_eq!(
        schema.resolutions().len(),
        2,
        "UntaggedMode should have 2 configurations"
    );

    // Each should have extracted fields from the tuple variant
    let simple_config = schema
        .resolutions()
        .iter()
        .find(|c| c.describe().contains("Simple"))
        .expect("Should have Simple config");

    let tuned_config = schema
        .resolutions()
        .iter()
        .find(|c| c.describe().contains("Tuned"))
        .expect("Should have Tuned config");

    // Simple should have level (from SimpleMode)
    assert!(
        simple_config.field("level").is_some(),
        "Simple config should have 'level' field"
    );

    // Tuned should have level, gain, tuning (from TunedMode)
    assert!(
        tuned_config.field("level").is_some(),
        "Tuned config should have 'level' field"
    );
    assert!(
        tuned_config.field("gain").is_some(),
        "Tuned config should have 'gain' field"
    );
    assert!(
        tuned_config.field("tuning").is_some(),
        "Tuned config should have 'tuning' field"
    );
}

// ============================================================================
// Issue #1: Flattened struct with CHILD fields (KDL-style)
// ============================================================================
//
// The user reported a SIGTRAP when processing child nodes inside a flattened struct.
// This happens in KDL where a struct has both:
// - Regular properties (like `port`, `host`)
// - Child nodes (marked with #[facet(child)])
//
// The #[facet(child)] attribute is KDL-specific and sets FieldFlags::CHILD.
// We cannot easily test this with the derive macro since #[facet(child)] may not
// be a recognized attribute. The issue is likely in how the solver handles
// or ignores fields marked with CHILD flag.
//
// For now, let's test the basic case where a flattened struct has nested structs
// (which is similar to having child nodes in structure).

#[derive(Facet, Debug)]
struct NestedChild {
    child_value: i32,
}

#[derive(Facet, Debug)]
struct DetailsWithChild {
    port: u16,
    host: String,
    // This simulates a "child" node - a nested struct that would be
    // serialized as a child element in KDL/XML formats
    nested: NestedChild,
}

#[derive(Facet, Debug)]
struct ParentWithFlattenAndChild {
    name: String,
    #[facet(flatten)]
    details: DetailsWithChild,
    // Another nested struct at the same level
    owner: Owner,
}

#[test]
fn test_flatten_struct_with_nested_children() {
    // This tests flattening a struct that has nested structs (simulating children)
    let schema = Schema::build(ParentWithFlattenAndChild::SHAPE).unwrap();

    eprintln!(
        "ParentWithFlattenAndChild configurations: {}",
        schema.resolutions().len()
    );
    for (i, config) in schema.resolutions().iter().enumerate() {
        eprintln!("  Config {}: {} fields", i, config.fields().len());
        for name in config.fields().keys() {
            eprintln!("    - {name}");
        }
    }

    // Should have 1 configuration (no enums)
    assert_eq!(schema.resolutions().len(), 1);

    let config = &schema.resolutions()[0];

    // Check expected fields
    assert!(config.field("name").is_some(), "should have name");
    assert!(
        config.field("port").is_some(),
        "should have port (from flattened details)"
    );
    assert!(
        config.field("host").is_some(),
        "should have host (from flattened details)"
    );
    assert!(
        config.field("nested").is_some(),
        "should have nested (from flattened details)"
    );
    assert!(config.field("owner").is_some(), "should have owner");
}
