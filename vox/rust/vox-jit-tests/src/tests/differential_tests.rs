//! Differential tests: oracle (reflective interpreter) vs. candidate engines.
//!
//! These tests establish that for every input the oracle can handle, any
//! candidate engine must produce the same output or the same error class.
//!
//! When the IR interpreter (task #3) or JIT (task #8+) land, add them as
//! candidates here by constructing `FnPtrEngine { name: "ir", f: ir_decode }`
//! and including them in the `candidates` slice.

use vox_postcard::{
    SchemaSet, build_identity_plan, build_plan, plan::PlanInput, serialize::to_vec,
};
use vox_schema::SchemaRegistry;

use crate::{
    differential::{DifferentialCase, IrEngine, ReflectiveOracle, assert_differential},
    fixtures::*,
};
use vox_types::schema::extract_schemas;

// ---------------------------------------------------------------------------
// Helper: build SchemaSet from a Facet type
// ---------------------------------------------------------------------------

fn schema_set_for<T: facet::Facet<'static>>() -> SchemaSet {
    let extracted = extract_schemas(T::SHAPE).expect("schema extraction failed");
    SchemaSet::from_root_and_schemas(extracted.root.clone(), extracted.schemas.clone())
}

// ---------------------------------------------------------------------------
// Differential: identity plan (same type on both sides)
// ---------------------------------------------------------------------------

/// Run values through oracle + IR interpreter, asserting identical results.
fn assert_oracle_roundtrip<T>(values: &[T])
where
    T: facet::Facet<'static> + PartialEq + std::fmt::Debug + Clone + Send + Sync,
{
    let oracle = ReflectiveOracle;
    let ir = IrEngine;
    let plan = build_identity_plan(T::SHAPE);
    let registry = SchemaRegistry::new();

    // Own the labels and bytes so they live long enough — no Box::leak needed.
    let labels: Vec<String> = (0..values.len()).map(|i| format!("case-{i}")).collect();
    let encoded: Vec<Vec<u8>> = values
        .iter()
        .map(|v| to_vec(v).expect("encode failed"))
        .collect();

    let cases: Vec<DifferentialCase<'_>> = labels
        .iter()
        .zip(encoded.iter())
        .map(|(label, bytes)| DifferentialCase {
            label: label.as_str(),
            bytes: bytes.as_slice(),
            plan: &plan,
            registry: &registry,
        })
        .collect();

    assert_differential::<T>(&oracle, &[&ir], &cases);
}

#[test]
fn differential_scalars() {
    assert_oracle_roundtrip(&[Scalars::sample()]);
}

#[test]
fn differential_strings() {
    assert_oracle_roundtrip(&[StringFields::sample(), StringFields::empty()]);
}

#[test]
fn differential_byte_vec() {
    assert_oracle_roundtrip(&[ByteVec::sample(), ByteVec::empty()]);
}

#[test]
fn differential_vec_u32() {
    assert_oracle_roundtrip(&[VecU32::sample(), VecU32::empty(), VecU32::large()]);
}

#[test]
fn differential_vec_string() {
    assert_oracle_roundtrip(&[VecString::sample()]);
}

#[test]
fn differential_option() {
    assert_oracle_roundtrip(&[WithOption::some(), WithOption::none()]);
}

#[test]
fn differential_enum_unit_variants() {
    assert_oracle_roundtrip(&[Color::Red, Color::Green, Color::Blue]);
}

#[test]
fn differential_enum_with_payload() {
    assert_oracle_roundtrip(&[
        Shape::Circle(1.5),
        Shape::Rect { w: 10.0, h: 20.0 },
        Shape::Point,
    ]);
}

#[test]
fn differential_enum_with_string_payload() {
    assert_oracle_roundtrip(&Command::all_variants());
}

#[test]
fn differential_array() {
    assert_oracle_roundtrip(&[WithArray::sample()]);
}

#[test]
fn differential_nested_struct() {
    assert_oracle_roundtrip(&[Outer::sample()]);
}

// ---------------------------------------------------------------------------
// Differential: translation plan (remote has extra field)
//
// Both sides must share the same type name for build_plan to accept them.
// We use module-scoped types named "Record" on each side.
// ---------------------------------------------------------------------------

#[test]
fn differential_skip_unknown_remote_field() {
    mod remote {
        #[derive(facet::Facet, Debug, PartialEq, Clone)]
        pub struct Record {
            pub value: u32,
            pub extra: String,
        }
    }
    mod local {
        #[derive(facet::Facet, Debug, PartialEq, Clone)]
        pub struct Record {
            pub value: u32,
        }
    }

    let remote_val = remote::Record {
        value: 42,
        extra: "bonus".to_string(),
    };

    let remote_set = schema_set_for::<remote::Record>();
    let local_set = schema_set_for::<local::Record>();
    let plan = build_plan(&PlanInput {
        remote: &remote_set,
        local: &local_set,
    })
    .expect("build_plan failed for skip-unknown test");

    let bytes = to_vec(&remote_val).expect("encode failed");

    let oracle = ReflectiveOracle;
    let ir = IrEngine;
    let cases = [DifferentialCase {
        label: "skip-extra-field",
        bytes: &bytes,
        plan: &plan,
        registry: &remote_set.registry,
    }];

    assert_differential::<local::Record>(&oracle, &[&ir], &cases);
}

// ---------------------------------------------------------------------------
// Differential: boundary values that stress varint encoding
// ---------------------------------------------------------------------------

#[test]
fn differential_varint_boundaries() {
    // u32 boundary values exercise multi-byte varint paths
    let values: Vec<u32> = vec![
        0,
        1,
        127,   // 1 byte
        128,   // 2 bytes
        16383, // 2 bytes max
        16384, // 3 bytes
        u32::MAX,
    ];
    assert_oracle_roundtrip(&values);
}

#[test]
fn differential_signed_varint_boundaries() {
    let values: Vec<i32> = vec![0, 1, -1, 63, -64, 64, -65, i32::MAX, i32::MIN];
    assert_oracle_roundtrip(&values);
}

#[test]
fn differential_empty_vec_of_structs() {
    #[derive(facet::Facet, Debug, PartialEq, Clone)]
    struct Elem {
        x: u32,
        y: u32,
    }

    assert_oracle_roundtrip::<Vec<Elem>>(&[vec![], vec![Elem { x: 1, y: 2 }]]);
}
