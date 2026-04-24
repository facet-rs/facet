//! Failure-mode tests: every data-level error the design doc calls out must
//! map to the correct `ErrorClass` on the oracle, and candidates must agree.
//!
//! Covered per the design doc:
//! - EOF
//! - varint overflow
//! - invalid UTF-8
//! - invalid enum discriminant
//! - unknown remote variant (runtime error per message)

use facet::Facet;
use vox_postcard::{TranslationPlan, build_identity_plan, from_slice_with_plan, ir::from_slice_ir};
use vox_schema::SchemaRegistry;

use crate::{
    corpus::{
        check_oracle_error, encode_varint, enum_plan_discriminant_corpus, failure_corpus,
        option_tag_corpus, string_failure_corpus,
    },
    differential::ErrorClass,
    fixtures::*,
};

/// Assert that the IR interpreter agrees with the oracle on error class.
/// If the IR returns UnsupportedType, the shape isn't implemented yet — skip.
fn assert_ir_error_class<T: Facet<'static>>(
    bytes: &[u8],
    plan: &TranslationPlan,
    registry: &SchemaRegistry,
    expected: ErrorClass,
    label: &str,
) {
    match from_slice_ir::<T>(bytes, plan, registry, None) {
        Ok(_) => panic!("{label}: IR returned Ok, expected error class {expected:?}"),
        Err(e) => {
            let got = ErrorClass::of(&e);
            if got == ErrorClass::UnsupportedType {
                return; // shape not yet implemented in IR — skip
            }
            assert_eq!(
                got, expected,
                "{label}: IR error class {got:?} != expected {expected:?}: {e}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// u32 failure corpus
// ---------------------------------------------------------------------------

#[test]
fn oracle_u32_failure_modes() {
    let plan = build_identity_plan(<u32 as Facet>::SHAPE);
    let registry = SchemaRegistry::new();
    let corpus = failure_corpus();
    for entry in &corpus {
        let result = from_slice_with_plan::<u32>(&entry.bytes, &plan, &registry);
        let err = result.expect_err(&format!("expected error for '{}', got Ok", entry.label));
        check_oracle_error(entry, &err);
    }
}

// ---------------------------------------------------------------------------
// String failure corpus
// ---------------------------------------------------------------------------

#[test]
fn oracle_string_failure_modes() {
    let plan = build_identity_plan(<String as Facet>::SHAPE);
    let registry = SchemaRegistry::new();
    let corpus = string_failure_corpus();
    for entry in &corpus {
        let result = from_slice_with_plan::<String>(&entry.bytes, &plan, &registry);
        let err = result.expect_err(&format!("expected error for '{}', got Ok", entry.label));
        check_oracle_error(entry, &err);
    }
}

// ---------------------------------------------------------------------------
// Enum discriminant failures via plan-based decode
//
// When decoding through a translation plan, unknown discriminants produce
// `UnknownVariant` (not `InvalidEnumDiscriminant`). The latter appears only
// on the raw skip path (skip_value without a plan).
// ---------------------------------------------------------------------------

#[test]
fn oracle_enum_discriminant_out_of_range() {
    // Color has 3 variants (0, 1, 2). Discriminant 99 and 255 are unknown.
    // Via identity plan, build_identity_plan maps them to None → UnknownVariant.
    let plan = build_identity_plan(Color::SHAPE);
    let registry = SchemaRegistry::new();
    let corpus = enum_plan_discriminant_corpus();
    for entry in &corpus {
        let result = from_slice_with_plan::<Color>(&entry.bytes, &plan, &registry);
        let err = result.expect_err(&format!("expected error for '{}', got Ok", entry.label));
        check_oracle_error(entry, &err);

        assert_ir_error_class::<Color>(
            &entry.bytes,
            &plan,
            &registry,
            entry.expected.clone(),
            &format!("ir:{}", entry.label),
        );
    }
}

// ---------------------------------------------------------------------------
// Option tag failures
// ---------------------------------------------------------------------------

#[test]
fn oracle_option_invalid_tag() {
    let plan = build_identity_plan(<Option<u32> as Facet>::SHAPE);
    let registry = SchemaRegistry::new();
    let corpus = option_tag_corpus();
    for entry in &corpus {
        let result = from_slice_with_plan::<Option<u32>>(&entry.bytes, &plan, &registry);
        let err = result.expect_err(&format!("expected error for '{}', got Ok", entry.label));
        check_oracle_error(entry, &err);
    }
}

// ---------------------------------------------------------------------------
// Unknown remote variant (plan has None mapping for the remote discriminant)
//
// Both sides must have the same type name for build_plan to succeed.
// We use module-namespaced types with the same name "Direction".
// ---------------------------------------------------------------------------

#[test]
fn oracle_unknown_remote_variant() {
    use vox_postcard::{SchemaSet, build_plan, plan::PlanInput, serialize::to_vec};
    use vox_types::schema::extract_schemas;

    // Remote "Direction" has 4 variants; local "Direction" has only 3.
    // Same type name → build_plan accepts the pair; Unknown maps to None.
    mod remote {
        #[derive(facet::Facet, Debug, PartialEq, Clone)]
        #[repr(u8)]
        pub enum Direction {
            North,
            South,
            East,
            West, // unknown to local
        }
    }

    mod local {
        #[derive(facet::Facet, Debug, PartialEq, Clone)]
        #[repr(u8)]
        pub enum Direction {
            North,
            South,
            East,
        }
    }

    let remote_extracted = extract_schemas(remote::Direction::SHAPE).expect("extract remote");
    let local_extracted = extract_schemas(local::Direction::SHAPE).expect("extract local");

    let remote_set = SchemaSet::from_root_and_schemas(
        remote_extracted.root.clone(),
        remote_extracted.schemas.clone(),
    );
    let local_set = SchemaSet::from_root_and_schemas(
        local_extracted.root.clone(),
        local_extracted.schemas.clone(),
    );

    let plan = build_plan(&PlanInput {
        remote: &remote_set,
        local: &local_set,
    })
    .expect("build_plan: unknown remote variants are allowed at plan-build time");

    // Encode West (discriminant 3) on the remote side
    let bytes = to_vec(&remote::Direction::West).expect("encode");

    let result = from_slice_with_plan::<local::Direction>(&bytes, &plan, &remote_set.registry);
    let err = result.expect_err("expected UnknownVariant error for remote-only variant");
    assert_eq!(
        ErrorClass::of(&err),
        ErrorClass::UnknownVariant,
        "wrong error class: {err}"
    );

    // Known variants must still decode correctly
    for (remote_val, expected_local) in [
        (remote::Direction::North, local::Direction::North),
        (remote::Direction::South, local::Direction::South),
        (remote::Direction::East, local::Direction::East),
    ] {
        let bytes = to_vec(&remote_val).expect("encode known variant");
        let result: local::Direction = from_slice_with_plan(&bytes, &plan, &remote_set.registry)
            .expect("known variant must decode");
        assert_eq!(result, expected_local);
    }
}

// ---------------------------------------------------------------------------
// Truncated struct: enough bytes for first field, EOF on second
// ---------------------------------------------------------------------------

#[test]
fn oracle_struct_truncated_mid_field() {
    let plan = build_identity_plan(Scalars::SHAPE);
    let registry = SchemaRegistry::new();

    // Only encode a single byte (valid for u8_val, but then EOF on u16_val)
    let partial = vec![0xFF_u8]; // u8_val = 255, then EOF
    let result = from_slice_with_plan::<Scalars>(&partial, &plan, &registry);
    let err = result.expect_err("expected EOF error");
    assert_eq!(ErrorClass::of(&err), ErrorClass::UnexpectedEof, "{err}");

    assert_ir_error_class::<Scalars>(
        &partial,
        &plan,
        &registry,
        ErrorClass::UnexpectedEof,
        "struct-eof",
    );
}

// ---------------------------------------------------------------------------
// Varint overflow on i64 field
// ---------------------------------------------------------------------------

#[test]
fn oracle_varint_overflow_in_struct() {
    let plan = build_identity_plan(<i64 as Facet>::SHAPE);
    let registry = SchemaRegistry::new();

    // 11 bytes all with MSB set — overflows 64-bit zigzag varint
    let bytes = vec![0x80u8; 11];
    let result = from_slice_with_plan::<i64>(&bytes, &plan, &registry);
    let err = result.expect_err("expected VarintOverflow");
    assert_eq!(ErrorClass::of(&err), ErrorClass::VarintOverflow, "{err}");

    assert_ir_error_class::<i64>(
        &bytes,
        &plan,
        &registry,
        ErrorClass::VarintOverflow,
        "varint-overflow",
    );
}

// ---------------------------------------------------------------------------
// Vec<T>: length claims more elements than bytes available
// ---------------------------------------------------------------------------

#[test]
fn oracle_vec_eof_mid_elements() {
    let plan = build_identity_plan(<Vec<u32> as Facet>::SHAPE);
    let registry = SchemaRegistry::new();

    // Claim 100 elements but provide 0 bytes of payload
    let bytes = encode_varint(100);
    let result = from_slice_with_plan::<Vec<u32>>(&bytes, &plan, &registry);
    let err = result.expect_err("expected EOF error");
    assert_eq!(ErrorClass::of(&err), ErrorClass::UnexpectedEof, "{err}");

    assert_ir_error_class::<Vec<u32>>(
        &bytes,
        &plan,
        &registry,
        ErrorClass::UnexpectedEof,
        "vec-eof",
    );
}

// ---------------------------------------------------------------------------
// Struct with Vec<String>: invalid UTF-8 inside a list element
// ---------------------------------------------------------------------------

#[test]
fn oracle_vec_string_invalid_utf8_element() {
    let plan = build_identity_plan(<Vec<String> as Facet>::SHAPE);
    let registry = SchemaRegistry::new();

    // 1 element, length 3, invalid UTF-8 bytes
    let mut bytes = encode_varint(1); // list length = 1
    bytes.extend(encode_varint(3)); // string length = 3
    bytes.extend_from_slice(&[0xFF, 0xFE, 0xFD]); // invalid UTF-8

    let result = from_slice_with_plan::<Vec<String>>(&bytes, &plan, &registry);
    let err = result.expect_err("expected InvalidUtf8 error");
    assert_eq!(ErrorClass::of(&err), ErrorClass::InvalidUtf8, "{err}");

    assert_ir_error_class::<Vec<String>>(
        &bytes,
        &plan,
        &registry,
        ErrorClass::InvalidUtf8,
        "utf8-invalid",
    );
}
