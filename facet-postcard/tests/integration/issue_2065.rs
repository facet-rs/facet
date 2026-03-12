#![cfg(feature = "jit")]

use facet::Facet;
use facet_postcard::{Segment, peek_to_scatter_plan, to_scatter_plan, to_vec};
use facet_reflect::Peek;

#[derive(Debug, Facet)]
struct MixedBlobs<'a> {
    id: u32,
    borrowed_str: &'a str,
    owned_str: String,
    borrowed_bytes: &'a [u8],
    fixed_bytes: [u8; 4],
    vec_bytes: Vec<u8>,
}

#[derive(Debug, Facet)]
struct EmptyBlobs<'a> {
    id: u32,
    borrowed_str: &'a str,
    owned_str: String,
    borrowed_bytes: &'a [u8],
    fixed_bytes: [u8; 0],
    vec_bytes: Vec<u8>,
}

fn flatten(plan: &facet_postcard::ScatterPlan<'_>) -> Vec<u8> {
    let mut out = vec![0u8; plan.total_size()];
    plan.write_into(&mut out)
        .expect("scatter plan should flatten");
    out
}

#[test]
fn issue_2065_scatter_plan_matches_to_vec() {
    facet_testhelpers::setup();

    let value = MixedBlobs {
        id: 42,
        borrowed_str: "borrowed-str",
        owned_str: "owned-string".to_string(),
        borrowed_bytes: b"borrowed-bytes",
        fixed_bytes: [1, 2, 3, 4],
        vec_bytes: vec![5, 6, 7, 8, 9],
    };

    let plan = to_scatter_plan(&value).expect("scatter plan should serialize");
    let expected = to_vec(&value).expect("regular serialization should succeed");

    assert_eq!(plan.total_size(), expected.len());
    assert_eq!(flatten(&plan), expected);

    assert!(
        plan.segments()
            .iter()
            .any(|seg| matches!(seg, Segment::Staged { .. }))
    );
    assert!(
        plan.segments()
            .iter()
            .any(|seg| matches!(seg, Segment::Reference { .. }))
    );

    let ref_count = plan
        .segments()
        .iter()
        .filter(|seg| matches!(seg, Segment::Reference { .. }))
        .count();
    assert!(
        ref_count >= 5,
        "expected at least 5 reference segments for the blob/string fields, got {ref_count}"
    );
}

#[test]
fn issue_2065_peek_and_typed_paths_match() {
    facet_testhelpers::setup();

    let value = MixedBlobs {
        id: 7,
        borrowed_str: "peek-path",
        owned_str: "owned-peek".to_string(),
        borrowed_bytes: b"peek-bytes",
        fixed_bytes: [9, 8, 7, 6],
        vec_bytes: vec![3, 2, 1],
    };

    let typed = to_scatter_plan(&value).expect("typed scatter plan");
    let peeked = peek_to_scatter_plan(Peek::new(&value)).expect("peek scatter plan");
    let expected = to_vec(&value).expect("regular serialization should succeed");

    assert_eq!(typed.total_size(), peeked.total_size());
    assert_eq!(flatten(&typed), expected);
    assert_eq!(flatten(&peeked), expected);
}

#[test]
fn issue_2065_write_into_requires_exact_length() {
    facet_testhelpers::setup();

    let value = MixedBlobs {
        id: 99,
        borrowed_str: "length-check",
        owned_str: "owned".to_string(),
        borrowed_bytes: b"bytes",
        fixed_bytes: [0, 1, 2, 3],
        vec_bytes: vec![4, 5, 6],
    };

    let plan = to_scatter_plan(&value).expect("scatter plan should serialize");

    let mut short = vec![0u8; plan.total_size().saturating_sub(1)];
    assert!(plan.write_into(&mut short).is_err());

    let mut long = vec![0u8; plan.total_size() + 1];
    assert!(plan.write_into(&mut long).is_err());
}

#[test]
fn issue_2065_empty_blob_fields_have_no_empty_reference_segments() {
    facet_testhelpers::setup();

    let value = EmptyBlobs {
        id: 0,
        borrowed_str: "",
        owned_str: String::new(),
        borrowed_bytes: b"",
        fixed_bytes: [],
        vec_bytes: vec![],
    };

    let plan = to_scatter_plan(&value).expect("scatter plan should serialize");
    let expected = to_vec(&value).expect("regular serialization should succeed");

    assert_eq!(plan.total_size(), expected.len());
    assert_eq!(flatten(&plan), expected);

    for segment in plan.segments() {
        if let Segment::Reference { bytes } = segment {
            assert!(
                !bytes.is_empty(),
                "zero-length references should not be emitted"
            );
        }
    }
}
