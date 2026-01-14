//! Tests for type-based disambiguation using the state machine solver.
//!
//! These tests verify that the Solver correctly handles cases where
//! multiple variants have the same field name but different types.

use facet::Facet;
use facet_solver::{KeyResult, SatisfyResult, Schema, Solver, SolverError};
use facet_testhelpers::test;

// ============================================================================
// Test 1: Integer range disambiguation (u8 vs u16)
// ============================================================================

/// Two variants with the same field name but different integer sizes.
/// Demonstrates serde-like "first fit wins" behavior.
#[allow(dead_code)]
#[derive(Facet, Debug)]
#[repr(u8)]
enum IntegerRange {
    /// Small variant - can only hold values 0-255
    Small(u8),
    /// Large variant - can hold values 0-65535
    Large(u16),
}

#[derive(Facet, Debug)]
struct IntegerContainer {
    #[facet(flatten)]
    value: IntegerRange,
}

/// Helper to check if a value fits in a shape based on type_identifier
fn value_fits_in_shape(value: u128, shape: &facet_core::Shape) -> bool {
    match shape.type_identifier {
        "u8" => u8::try_from(value).is_ok(),
        "u16" => u16::try_from(value).is_ok(),
        "u32" => u32::try_from(value).is_ok(),
        "u64" => u64::try_from(value).is_ok(),
        "i8" => i8::try_from(value as i128).is_ok(),
        "i16" => i16::try_from(value as i128).is_ok(),
        "i32" => i32::try_from(value as i128).is_ok(),
        "i64" => i64::try_from(value as i128).is_ok(),
        _ => true, // Unknown type, assume it fits
    }
}

fn signed_value_fits_in_shape(value: i128, shape: &facet_core::Shape) -> bool {
    match shape.type_identifier {
        "i8" => i8::try_from(value).is_ok(),
        "i16" => i16::try_from(value).is_ok(),
        "i32" => i32::try_from(value).is_ok(),
        "i64" => i64::try_from(value).is_ok(),
        "u8" => value >= 0 && u8::try_from(value).is_ok(),
        "u16" => value >= 0 && u16::try_from(value).is_ok(),
        "u32" => value >= 0 && u32::try_from(value).is_ok(),
        "u64" => value >= 0 && u64::try_from(value).is_ok(),
        _ => true,
    }
}

#[test]
fn test_integer_small_value_picks_first() {
    // Value 32 fits in both u8 and u16
    // Should return Ambiguous since multiple variants match
    let schema = Schema::build(IntegerContainer::SHAPE).unwrap();

    assert_eq!(
        schema.resolutions().len(),
        2,
        "Should have 2 configurations: Small and Large"
    );

    let mut solver = Solver::new(&schema);

    match solver.see_key("0") {
        KeyResult::Ambiguous { fields } => {
            // Value 32 fits in both u8 and u16
            let satisfied: Vec<_> = fields
                .iter()
                .filter(|(f, _)| value_fits_in_shape(32, f.value_shape))
                .map(|(f, _)| *f)
                .collect();

            // Both should be satisfied
            assert_eq!(satisfied.len(), 2);

            match solver.satisfy(&satisfied) {
                SatisfyResult::Continue => {
                    // Multiple configs still viable - should return Ambiguous
                    let err = solver.finish().expect_err("should be ambiguous");
                    match err {
                        SolverError::Ambiguous { candidates, .. } => {
                            assert!(
                                candidates.iter().any(|c| c.contains("Small"))
                                    && candidates.iter().any(|c| c.contains("Large")),
                                "Expected both Small and Large in candidates, got: {candidates:?}"
                            );
                        }
                        other => panic!("Expected Ambiguous error, got: {other:?}"),
                    }
                }
                other => panic!("Expected Continue, got: {other:?}"),
            }
        }
        other => panic!("Expected Ambiguous, got: {other:?}"),
    }
}

#[test]
fn test_integer_large_value_picks_second() {
    // Value 1000 doesn't fit in u8 (max 255), so only u16 works
    let schema = Schema::build(IntegerContainer::SHAPE).unwrap();

    let mut solver = Solver::new(&schema);

    match solver.see_key("0") {
        KeyResult::Ambiguous { fields } => {
            // Value 1000 only fits in u16
            let satisfied: Vec<_> = fields
                .iter()
                .filter(|(f, _)| value_fits_in_shape(1000, f.value_shape))
                .map(|(f, _)| *f)
                .collect();

            // Only u16 should be satisfied
            assert_eq!(satisfied.len(), 1);
            assert_eq!(satisfied[0].value_shape.type_identifier, "u16");

            match solver.satisfy(&satisfied) {
                SatisfyResult::Solved(config) => {
                    let desc = config.resolution().describe();
                    assert!(
                        desc.contains("Large"),
                        "Expected Large variant, got: {desc}"
                    );
                }
                other => panic!("Expected Solved, got: {other:?}"),
            }
        }
        other => panic!("Expected Ambiguous, got: {other:?}"),
    }
}

#[test]
fn test_integer_boundary_255() {
    // Value 255 is the max for u8 - should still satisfy both, hence ambiguous
    let schema = Schema::build(IntegerContainer::SHAPE).unwrap();

    let mut solver = Solver::new(&schema);

    match solver.see_key("0") {
        KeyResult::Ambiguous { fields } => {
            let satisfied: Vec<_> = fields
                .iter()
                .filter(|(f, _)| value_fits_in_shape(255, f.value_shape))
                .map(|(f, _)| *f)
                .collect();

            assert_eq!(satisfied.len(), 2); // Both u8 and u16 can hold 255

            solver.satisfy(&satisfied);
            let err = solver.finish().expect_err("should be ambiguous");
            match err {
                SolverError::Ambiguous { candidates, .. } => {
                    assert!(
                        candidates.iter().any(|c| c.contains("Small"))
                            && candidates.iter().any(|c| c.contains("Large")),
                        "Expected both variants in candidates, got: {candidates:?}"
                    );
                }
                other => panic!("Expected Ambiguous error, got: {other:?}"),
            }
        }
        other => panic!("Expected Ambiguous, got: {other:?}"),
    }
}

#[test]
fn test_integer_boundary_256() {
    // Value 256 is just over u8 max - should only satisfy u16
    let schema = Schema::build(IntegerContainer::SHAPE).unwrap();

    let mut solver = Solver::new(&schema);

    match solver.see_key("0") {
        KeyResult::Ambiguous { fields } => {
            let satisfied: Vec<_> = fields
                .iter()
                .filter(|(f, _)| value_fits_in_shape(256, f.value_shape))
                .map(|(f, _)| *f)
                .collect();

            assert_eq!(satisfied.len(), 1);

            match solver.satisfy(&satisfied) {
                SatisfyResult::Solved(config) => {
                    let desc = config.resolution().describe();
                    assert!(
                        desc.contains("Large"),
                        "Expected Large variant for 256, got: {desc}"
                    );
                }
                other => panic!("Expected Solved, got: {other:?}"),
            }
        }
        other => panic!("Expected Ambiguous, got: {other:?}"),
    }
}

// ============================================================================
// Test 2: Signed vs Unsigned integers
// ============================================================================

#[allow(dead_code)]
#[derive(Facet, Debug)]
#[repr(u8)]
enum SignedUnsigned {
    Signed(i8),
    Unsigned(u8),
}

#[derive(Facet, Debug)]
struct SignedContainer {
    #[facet(flatten)]
    value: SignedUnsigned,
}

#[test]
fn test_negative_value_picks_signed() {
    // Negative values can only go in signed types
    let schema = Schema::build(SignedContainer::SHAPE).unwrap();

    let mut solver = Solver::new(&schema);

    match solver.see_key("0") {
        KeyResult::Ambiguous { fields } => {
            let satisfied: Vec<_> = fields
                .iter()
                .filter(|(f, _)| signed_value_fits_in_shape(-10, f.value_shape))
                .map(|(f, _)| *f)
                .collect();

            // Only i8 can hold -10
            assert_eq!(satisfied.len(), 1);
            assert_eq!(satisfied[0].value_shape.type_identifier, "i8");

            match solver.satisfy(&satisfied) {
                SatisfyResult::Solved(config) => {
                    let desc = config.resolution().describe();
                    assert!(
                        desc.contains("Signed"),
                        "Expected Signed variant for -10, got: {desc}"
                    );
                }
                other => panic!("Expected Solved, got: {other:?}"),
            }
        }
        other => panic!("Expected Ambiguous, got: {other:?}"),
    }
}

#[test]
fn test_positive_value_fits_both_picks_first() {
    // Positive value 50 fits in both i8 and u8 - should be ambiguous
    let schema = Schema::build(SignedContainer::SHAPE).unwrap();

    let mut solver = Solver::new(&schema);

    match solver.see_key("0") {
        KeyResult::Ambiguous { fields } => {
            let satisfied: Vec<_> = fields
                .iter()
                .filter(|(f, _)| signed_value_fits_in_shape(50, f.value_shape))
                .map(|(f, _)| *f)
                .collect();

            // Both can hold 50
            assert_eq!(satisfied.len(), 2);

            solver.satisfy(&satisfied);
            let err = solver.finish().expect_err("should be ambiguous");
            match err {
                SolverError::Ambiguous { candidates, .. } => {
                    assert!(
                        candidates.iter().any(|c| c.contains("Signed"))
                            && candidates.iter().any(|c| c.contains("Unsigned")),
                        "Expected both variants in candidates, got: {candidates:?}"
                    );
                }
                other => panic!("Expected Ambiguous error, got: {other:?}"),
            }
        }
        other => panic!("Expected Ambiguous, got: {other:?}"),
    }
}

#[test]
fn test_large_positive_picks_unsigned() {
    // Value 200 fits in u8 (0-255) but not i8 (-128 to 127)
    let schema = Schema::build(SignedContainer::SHAPE).unwrap();

    let mut solver = Solver::new(&schema);

    match solver.see_key("0") {
        KeyResult::Ambiguous { fields } => {
            let satisfied: Vec<_> = fields
                .iter()
                .filter(|(f, _)| value_fits_in_shape(200, f.value_shape))
                .map(|(f, _)| *f)
                .collect();

            // Only u8 can hold 200
            assert_eq!(satisfied.len(), 1);
            assert_eq!(satisfied[0].value_shape.type_identifier, "u8");

            match solver.satisfy(&satisfied) {
                SatisfyResult::Solved(config) => {
                    let desc = config.resolution().describe();
                    assert!(
                        desc.contains("Unsigned"),
                        "Expected Unsigned variant for 200, got: {desc}"
                    );
                }
                other => panic!("Expected Solved, got: {other:?}"),
            }
        }
        other => panic!("Expected Ambiguous, got: {other:?}"),
    }
}

// ============================================================================
// Test 3: Key-based disambiguation still works
// ============================================================================

#[derive(Facet, Debug)]
struct HttpConfig {
    url: String,
}

#[derive(Facet, Debug)]
struct GitConfig {
    url: String,
    branch: String,
}

#[allow(dead_code)]
#[derive(Facet, Debug)]
#[repr(u8)]
enum SourceConfig {
    Http(HttpConfig),
    Git(GitConfig),
}

#[derive(Facet, Debug)]
struct Source {
    #[facet(flatten)]
    config: SourceConfig,
}

#[test]
fn test_key_disambiguation_git() {
    // Having "branch" key disambiguates to Git
    let schema = Schema::build(Source::SHAPE).unwrap();

    let mut solver = Solver::new(&schema);

    // "url" is in both - should be Unambiguous (same type: String)
    match solver.see_key("url") {
        KeyResult::Unambiguous { shape } => {
            assert_eq!(shape.type_identifier, "String");
        }
        other => panic!("Expected Unambiguous for 'url', got: {other:?}"),
    }

    // "branch" only exists in Git - should disambiguate
    match solver.see_key("branch") {
        KeyResult::Solved(config) => {
            assert!(
                config.resolution().describe().contains("Git"),
                "Expected Git variant, got: {}",
                config.resolution().describe()
            );
        }
        other => panic!("Expected Solved for 'branch', got: {other:?}"),
    }
}

#[test]
fn test_key_disambiguation_http() {
    // Only having "url" (no "branch") - Git missing required field
    let schema = Schema::build(Source::SHAPE).unwrap();

    let mut solver = Solver::new(&schema);

    // "url" is in both
    match solver.see_key("url") {
        KeyResult::Unambiguous { .. } => {}
        other => panic!("Expected Unambiguous for 'url', got: {other:?}"),
    }

    // Finish - Git is filtered out because it's missing required "branch"
    let config = solver.finish().expect("should resolve");
    assert!(
        config.resolution().describe().contains("Http"),
        "Expected Http variant (Git missing required 'branch'), got: {}",
        config.resolution().describe()
    );
}

// ============================================================================
// Test 4: DateTime vs UUID vs String (string parsing disambiguation)
// ============================================================================

// Mock types to simulate DateTime and UUID
// In real code these would be from chrono/uuid crates

#[derive(Facet, Debug)]
struct MockDateTime {
    _phantom: std::marker::PhantomData<()>,
}

#[derive(Facet, Debug)]
struct MockUuid {
    _phantom: std::marker::PhantomData<()>,
}

#[derive(Facet, Debug)]
struct DateTimeVariant {
    value: String, // Would be DateTime in real code
}

#[derive(Facet, Debug)]
struct UuidVariant {
    value: String, // Would be Uuid in real code
}

#[derive(Facet, Debug)]
struct StringVariant {
    value: String,
}

#[allow(dead_code)]
#[derive(Facet, Debug)]
#[repr(u8)]
enum StringParsing {
    DateTime(DateTimeVariant),
    Uuid(UuidVariant),
    Plain(StringVariant),
}

#[derive(Facet, Debug)]
struct StringContainer {
    #[facet(flatten)]
    inner: StringParsing,
}

#[test]
fn test_string_parsing_datetime() {
    let schema = Schema::build(StringContainer::SHAPE).unwrap();
    let mut solver = Solver::new(&schema);

    // All three variants have "value: String" - should be Unambiguous at key level
    // (They all have the same Shape for the field)
    match solver.see_key("value") {
        KeyResult::Unambiguous { shape } => {
            // All variants use String, so it's unambiguous at the type level
            assert_eq!(shape.type_identifier, "String");

            // At finish() time, all three variants are still viable since they
            // all have "value" field. This is now correctly reported as Ambiguous.
            let err = solver.finish().expect_err("should be ambiguous");
            match err {
                SolverError::Ambiguous { candidates, .. } => {
                    assert!(
                        candidates.iter().any(|c| c.contains("DateTime"))
                            && candidates.iter().any(|c| c.contains("Uuid"))
                            && candidates.iter().any(|c| c.contains("Plain")),
                        "Expected all variants in candidates, got: {candidates:?}"
                    );
                }
                other => panic!("Expected Ambiguous error, got: {other:?}"),
            }
        }
        KeyResult::Ambiguous { .. } => {
            panic!("Expected Unambiguous since all variants have String type");
        }
        other => panic!("Unexpected result: {other:?}"),
    }
}

// To actually test DateTime vs UUID vs String disambiguation,
// we need variants with DIFFERENT types. Let's create a more realistic test:

#[derive(Facet, Debug)]
struct RealDateTimeVariant {
    timestamp: i64, // Unix timestamp
}

#[derive(Facet, Debug)]
struct RealUuidVariant {
    id: u128, // UUID as u128
}

#[derive(Facet, Debug)]
struct RealStringVariant {
    text: String,
}

#[allow(dead_code)]
#[derive(Facet, Debug)]
#[repr(u8)]
enum DifferentTypes {
    Timestamp(RealDateTimeVariant),
    Uuid(RealUuidVariant),
    Text(RealStringVariant),
}

#[derive(Facet, Debug)]
struct DifferentTypesContainer {
    #[facet(flatten)]
    inner: DifferentTypes,
}

#[test]
fn test_different_field_names_no_ambiguity() {
    // Each variant has a unique field name - pure key disambiguation
    let schema = Schema::build(DifferentTypesContainer::SHAPE).unwrap();
    let mut solver = Solver::new(&schema);

    // "timestamp" only in Timestamp variant
    match solver.see_key("timestamp") {
        KeyResult::Solved(config) => {
            assert!(config.resolution().describe().contains("Timestamp"));
        }
        other => panic!("Expected Solved, got: {other:?}"),
    }
}

#[test]
fn test_uuid_field_disambiguation() {
    let schema = Schema::build(DifferentTypesContainer::SHAPE).unwrap();
    let mut solver = Solver::new(&schema);

    // "id" only in Uuid variant
    match solver.see_key("id") {
        KeyResult::Solved(config) => {
            assert!(config.resolution().describe().contains("Uuid"));
        }
        other => panic!("Expected Solved, got: {other:?}"),
    }
}

// Now the REAL sadistic case: same field name, different types!

#[derive(Facet, Debug)]
struct IntPayload {
    data: i64,
}

#[derive(Facet, Debug)]
struct FloatPayload {
    data: f64,
}

#[derive(Facet, Debug)]
struct StringPayload {
    data: String,
}

#[allow(dead_code)]
#[derive(Facet, Debug)]
#[repr(u8)]
enum SameFieldDifferentTypes {
    Int(IntPayload),
    Float(FloatPayload),
    Text(StringPayload),
}

#[derive(Facet, Debug)]
struct SadisticContainer {
    #[facet(flatten)]
    payload: SameFieldDifferentTypes,
}

fn type_accepts_int(type_id: &str) -> bool {
    matches!(type_id, "i64" | "i32" | "i16" | "i8" | "f64" | "f32")
}

fn type_accepts_float(type_id: &str) -> bool {
    matches!(type_id, "f64" | "f32")
}

fn type_accepts_string(type_id: &str) -> bool {
    matches!(type_id, "String" | "str")
}

#[test]
fn test_same_field_different_types_int_value() {
    // "data" field exists in all three, but with different types!
    let schema = Schema::build(SadisticContainer::SHAPE).unwrap();
    let mut solver = Solver::new(&schema);

    match solver.see_key("data") {
        KeyResult::Ambiguous { fields } => {
            // Should have 3 different fields with different shapes
            assert_eq!(fields.len(), 3, "Expected 3 different types for 'data'");

            // Simulate seeing an integer value - only Int and Float can accept it
            let satisfied: Vec<_> = fields
                .iter()
                .filter(|(f, _)| type_accepts_int(f.value_shape.type_identifier))
                .map(|(f, _)| *f)
                .collect();

            // i64 and f64 should both accept integers
            assert_eq!(satisfied.len(), 2);

            match solver.satisfy(&satisfied) {
                SatisfyResult::Continue => {
                    // Still ambiguous between Int and Float - should return Ambiguous
                    let err = solver.finish().expect_err("should be ambiguous");
                    match err {
                        SolverError::Ambiguous { candidates, .. } => {
                            assert!(
                                candidates.iter().any(|c| c.contains("Int"))
                                    && candidates.iter().any(|c| c.contains("Float")),
                                "Expected Int and Float in candidates, got: {candidates:?}"
                            );
                        }
                        other => panic!("Expected Ambiguous error, got: {other:?}"),
                    }
                }
                other => panic!("Expected Continue, got: {other:?}"),
            }
        }
        other => panic!("Expected Ambiguous, got: {other:?}"),
    }
}

#[test]
fn test_same_field_different_types_float_value() {
    let schema = Schema::build(SadisticContainer::SHAPE).unwrap();
    let mut solver = Solver::new(&schema);

    match solver.see_key("data") {
        KeyResult::Ambiguous { fields } => {
            // Simulate seeing a float value (e.g., 3.14) - only Float can accept it
            // (assuming strict parsing where "3.14" can't be parsed as i64)
            let satisfied: Vec<_> = fields
                .iter()
                .filter(|(f, _)| type_accepts_float(f.value_shape.type_identifier))
                .map(|(f, _)| *f)
                .collect();

            assert_eq!(satisfied.len(), 1);
            assert_eq!(satisfied[0].value_shape.type_identifier, "f64");

            match solver.satisfy(&satisfied) {
                SatisfyResult::Solved(config) => {
                    assert!(
                        config.resolution().describe().contains("Float"),
                        "Expected Float variant, got: {}",
                        config.resolution().describe()
                    );
                }
                other => panic!("Expected Solved, got: {other:?}"),
            }
        }
        other => panic!("Expected Ambiguous, got: {other:?}"),
    }
}

#[test]
fn test_same_field_different_types_string_value() {
    let schema = Schema::build(SadisticContainer::SHAPE).unwrap();
    let mut solver = Solver::new(&schema);

    match solver.see_key("data") {
        KeyResult::Ambiguous { fields } => {
            // Simulate seeing a string value - only String can accept it
            let satisfied: Vec<_> = fields
                .iter()
                .filter(|(f, _)| type_accepts_string(f.value_shape.type_identifier))
                .map(|(f, _)| *f)
                .collect();

            assert_eq!(satisfied.len(), 1);
            assert_eq!(satisfied[0].value_shape.type_identifier, "String");

            match solver.satisfy(&satisfied) {
                SatisfyResult::Solved(config) => {
                    assert!(
                        config.resolution().describe().contains("Text"),
                        "Expected Text variant, got: {}",
                        config.resolution().describe()
                    );
                }
                other => panic!("Expected Solved, got: {other:?}"),
            }
        }
        other => panic!("Expected Ambiguous, got: {other:?}"),
    }
}

// ============================================================================
// Test 5: Unambiguous when types match
// ============================================================================

#[derive(Facet, Debug)]
struct ConfigA {
    name: String,
    value_a: i32,
}

#[derive(Facet, Debug)]
struct ConfigB {
    name: String,
    value_b: i32,
}

#[allow(dead_code)]
#[derive(Facet, Debug)]
#[repr(u8)]
enum ConfigChoice {
    A(ConfigA),
    B(ConfigB),
}

#[derive(Facet, Debug)]
struct ConfigContainer {
    #[facet(flatten)]
    config: ConfigChoice,
}

#[test]
fn test_same_type_is_unambiguous() {
    // Both configs have "name: String" - should be Unambiguous
    let schema = Schema::build(ConfigContainer::SHAPE).unwrap();

    let mut solver = Solver::new(&schema);

    match solver.see_key("name") {
        KeyResult::Unambiguous { shape } => {
            assert_eq!(shape.type_identifier, "String");
        }
        other => panic!("Expected Unambiguous for 'name', got: {other:?}"),
    }

    // "value_a" disambiguates to A
    match solver.see_key("value_a") {
        KeyResult::Solved(config) => {
            assert!(config.resolution().describe().contains("::A"));
        }
        other => panic!("Expected Solved for 'value_a', got: {other:?}"),
    }
}
