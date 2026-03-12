//! Regression test for https://github.com/facet-rs/facet/issues/2124
//!
//! Internally-tagged enums with newtype variants wrapping structs or other
//! tagged enums fail to serialize/deserialize.

use facet::Facet;
use facet_testhelpers::test;

// ---------------------------------------------------------------------------
// Shared types
// ---------------------------------------------------------------------------

#[derive(Facet, Clone, PartialEq, Debug)]
pub struct Filter {
    pub name: String,
}

#[derive(Facet, Clone, PartialEq, Debug)]
#[facet(tag = "add_type")]
#[repr(C)]
pub enum AddOp {
    Full,
    Filtered { filter: Filter },
}

#[derive(Facet, Clone, PartialEq, Debug)]
#[facet(tag = "inner_type")]
#[repr(C)]
pub enum Inner {
    Include(AddOp),
    Exclude(Filter),
}

#[derive(Facet, Clone, PartialEq, Debug)]
#[facet(tag = "outer_type")]
#[repr(C)]
pub enum Outer {
    Nested(Inner),
    Simple { value: f64 },
}

// ---------------------------------------------------------------------------
// Newtype variant wrapping a plain struct
// ---------------------------------------------------------------------------

#[test]
fn test_issue_2124_inner_exclude() {
    let expected = Inner::Exclude(Filter { name: "x".into() });

    // Roundtrip
    let json = facet_json::to_string(&expected).unwrap();
    let back: Inner = facet_json::from_str(&json).unwrap();
    assert_eq!(expected, back);

    // Deserialize from known JSON
    let back: Inner = facet_json::from_str(r#"{"inner_type":"Exclude","name":"x"}"#).unwrap();
    assert_eq!(expected, back);
}

// ---------------------------------------------------------------------------
// Newtype wrapping a tagged enum (two levels of tags)
// ---------------------------------------------------------------------------

#[test]
fn test_issue_2124_inner_include_full() {
    let expected = Inner::Include(AddOp::Full);

    let json = facet_json::to_string(&expected).unwrap();
    let back: Inner = facet_json::from_str(&json).unwrap();
    assert_eq!(expected, back);

    let back: Inner =
        facet_json::from_str(r#"{"inner_type":"Include","add_type":"Full"}"#).unwrap();
    assert_eq!(expected, back);
}

#[test]
fn test_issue_2124_inner_include_filtered() {
    let expected = Inner::Include(AddOp::Filtered {
        filter: Filter { name: "f".into() },
    });

    let json = facet_json::to_string(&expected).unwrap();
    let back: Inner = facet_json::from_str(&json).unwrap();
    assert_eq!(expected, back);

    let back: Inner = facet_json::from_str(
        r#"{"inner_type":"Include","add_type":"Filtered","filter":{"name":"f"}}"#,
    )
    .unwrap();
    assert_eq!(expected, back);
}

// ---------------------------------------------------------------------------
// Three levels of tags (outer → inner → add_type)
// ---------------------------------------------------------------------------

#[test]
fn test_issue_2124_outer_nested_include_filtered() {
    let expected = Outer::Nested(Inner::Include(AddOp::Filtered {
        filter: Filter { name: "f".into() },
    }));

    let json = facet_json::to_string(&expected).unwrap();
    let back: Outer = facet_json::from_str(&json).unwrap();
    assert_eq!(expected, back);

    let back: Outer = facet_json::from_str(
        r#"{"outer_type":"Nested","inner_type":"Include","add_type":"Filtered","filter":{"name":"f"}}"#,
    )
    .unwrap();
    assert_eq!(expected, back);
}

#[test]
fn test_issue_2124_outer_nested_exclude() {
    let expected = Outer::Nested(Inner::Exclude(Filter {
        name: "gone".into(),
    }));

    let json = facet_json::to_string(&expected).unwrap();
    let back: Outer = facet_json::from_str(&json).unwrap();
    assert_eq!(expected, back);

    let back: Outer =
        facet_json::from_str(r#"{"outer_type":"Nested","inner_type":"Exclude","name":"gone"}"#)
            .unwrap();
    assert_eq!(expected, back);
}

#[test]
fn test_issue_2124_outer_simple() {
    // Struct variant (not a newtype) — should already work, included for completeness.
    let expected = Outer::Simple { value: 1.5 };

    let json = facet_json::to_string(&expected).unwrap();
    let back: Outer = facet_json::from_str(&json).unwrap();
    assert_eq!(expected, back);

    let back: Outer = facet_json::from_str(r#"{"outer_type":"Simple","value":1.5}"#).unwrap();
    assert_eq!(expected, back);
}

// ---------------------------------------------------------------------------
// Three-level nesting: Top(tagged) → Middle(tagged newtype) → Config(struct)
// ---------------------------------------------------------------------------

#[derive(Facet, Clone, PartialEq, Debug)]
pub struct Config {
    pub enabled: bool,
}

#[derive(Facet, Clone, PartialEq, Debug)]
#[facet(tag = "level2")]
#[repr(C)]
pub enum Middle {
    Wrap(Config),
}

#[derive(Facet, Clone, PartialEq, Debug)]
#[facet(tag = "level1")]
#[repr(C)]
pub enum Top {
    Deep(Middle),
}

#[test]
fn test_issue_2124_three_level_nesting() {
    let expected = Top::Deep(Middle::Wrap(Config { enabled: true }));

    let json = facet_json::to_string(&expected).unwrap();
    let back: Top = facet_json::from_str(&json).unwrap();
    assert_eq!(expected, back);

    let back: Top =
        facet_json::from_str(r#"{"level1":"Deep","level2":"Wrap","enabled":true}"#).unwrap();
    assert_eq!(expected, back);
}

// ---------------------------------------------------------------------------
// Mixed variant kinds: unit + struct + newtype in one tagged enum
// ---------------------------------------------------------------------------

#[derive(Facet, Clone, PartialEq, Debug)]
#[facet(tag = "kind")]
#[repr(C)]
pub enum Mixed {
    Unit,
    Named { x: i32 },
    Newtype(Filter),
}

#[test]
fn test_issue_2124_mixed_variants() {
    // Unit and Named variants should already work; Newtype is the new case.
    let cases: Vec<(Mixed, &str)> = vec![
        (Mixed::Unit, r#"{"kind":"Unit"}"#),
        (Mixed::Named { x: 42 }, r#"{"kind":"Named","x":42}"#),
        (
            Mixed::Newtype(Filter { name: "abc".into() }),
            r#"{"kind":"Newtype","name":"abc"}"#,
        ),
    ];

    for (expected, known_json) in cases {
        let json = facet_json::to_string(&expected).unwrap();
        let back: Mixed = facet_json::from_str(&json).unwrap();
        assert_eq!(expected, back);

        let back: Mixed = facet_json::from_str(known_json).unwrap();
        assert_eq!(expected, back);
    }
}

// ---------------------------------------------------------------------------
// Newtype wrapping a struct with optional fields (defaults must apply)
// ---------------------------------------------------------------------------

#[derive(Facet, Clone, PartialEq, Debug)]
pub struct OptFields {
    pub required: String,
    pub optional: Option<i32>,
}

#[derive(Facet, Clone, PartialEq, Debug)]
#[facet(tag = "t")]
#[repr(C)]
pub enum WithOpt {
    Val(OptFields),
}

// ---------------------------------------------------------------------------
// Regression test: newtype wrapping a struct with #[facet(flatten)].
// Previously the newtype deserialization path used `field_lookup.find()` (a
// flat name→index lookup) which did not recurse into flattened sub-structs.
// Fixed by extracting `read_tagged_object_fields` which uses `find_field_path`
// when `has_flatten` is set.
// ---------------------------------------------------------------------------

#[derive(Facet, Clone, PartialEq, Debug)]
pub struct GeoCoords {
    pub lat: f64,
    pub lng: f64,
}

#[derive(Facet, Clone, PartialEq, Debug)]
pub struct Location {
    pub label: String,
    #[facet(flatten)]
    pub coords: GeoCoords,
}

#[derive(Facet, Clone, PartialEq, Debug)]
#[facet(tag = "type")]
#[repr(C)]
pub enum Place {
    /// Newtype variant wrapping a struct that has a flattened field.
    /// JSON: {"type":"Pin","label":"HQ","lat":1.0,"lng":2.0}
    Pin(Location),
    /// Struct variant for comparison.
    Inline {
        label: String,
        #[facet(flatten)]
        coords: GeoCoords,
    },
}

#[test]
fn test_issue_2124_newtype_with_flatten_struct_variant_works() {
    // Struct variant with flatten — this already works via the has_flatten /
    // find_field_path code path. Included to contrast with the newtype case.
    let expected = Place::Inline {
        label: "HQ".into(),
        coords: GeoCoords { lat: 1.0, lng: 2.0 },
    };

    let json = facet_json::to_string(&expected).unwrap();
    let back: Place = facet_json::from_str(&json).unwrap();
    assert_eq!(expected, back);

    let back: Place =
        facet_json::from_str(r#"{"type":"Inline","label":"HQ","lat":1.0,"lng":2.0}"#).unwrap();
    assert_eq!(expected, back);
}

#[test]
fn test_issue_2124_newtype_with_flatten() {
    // Newtype variant wrapping a struct with #[facet(flatten)].
    let expected = Place::Pin(Location {
        label: "HQ".into(),
        coords: GeoCoords { lat: 1.0, lng: 2.0 },
    });

    let json = facet_json::to_string(&expected).unwrap();
    let back: Place = facet_json::from_str(&json).unwrap();
    assert_eq!(expected, back);

    let back: Place =
        facet_json::from_str(r#"{"type":"Pin","label":"HQ","lat":1.0,"lng":2.0}"#).unwrap();
    assert_eq!(expected, back);
}

// ---------------------------------------------------------------------------
// Regression test: newtype wrapping an internally-tagged enum whose struct
// variant has #[facet(flatten)]. Same root cause as above, also fixed by the
// shared `read_tagged_object_fields` helper.
// ---------------------------------------------------------------------------

#[derive(Facet, Clone, PartialEq, Debug)]
pub struct Metadata {
    pub author: String,
    pub version: u32,
}

#[derive(Facet, Clone, PartialEq, Debug)]
#[facet(tag = "kind")]
#[repr(C)]
pub enum Document {
    Report {
        title: String,
        #[facet(flatten)]
        meta: Metadata,
    },
}

#[derive(Facet, Clone, PartialEq, Debug)]
#[facet(tag = "wrapper")]
#[repr(C)]
pub enum Envelope {
    Doc(Document),
}

#[test]
fn test_issue_2124_newtype_inner_enum_with_flatten() {
    // Two-level newtype: Envelope(tagged) → Document(tagged) → Report { flatten }
    let expected = Envelope::Doc(Document::Report {
        title: "Annual".into(),
        meta: Metadata {
            author: "Alice".into(),
            version: 3,
        },
    });

    let json = facet_json::to_string(&expected).unwrap();
    let back: Envelope = facet_json::from_str(&json).unwrap();
    assert_eq!(expected, back);

    let back: Envelope = facet_json::from_str(
        r#"{"wrapper":"Doc","kind":"Report","title":"Annual","author":"Alice","version":3}"#,
    )
    .unwrap();
    assert_eq!(expected, back);
}

// ---------------------------------------------------------------------------
// Duplicate tag keys across nesting levels must produce a clear error
// ---------------------------------------------------------------------------

#[derive(Facet, Clone, PartialEq, Debug)]
#[facet(tag = "type")]
#[repr(C)]
pub enum InnerSameTag {
    A { x: i32 },
}

#[derive(Facet, Clone, PartialEq, Debug)]
#[facet(tag = "type")]
#[repr(C)]
pub enum OuterSameTag {
    Wrap(InnerSameTag),
}

#[test]
fn test_issue_2124_duplicate_tag_key_serialize_error() {
    // Both enums use #[facet(tag = "type")]. When flattened into a single
    // object the two "type" keys are ambiguous — serialization must fail.
    let value = OuterSameTag::Wrap(InnerSameTag::A { x: 1 });
    let result = facet_json::to_string(&value);
    assert!(
        result.is_err(),
        "expected error for duplicate tag key, got: {result:?}"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("same tag key"),
        "error should mention 'same tag key', got: {err}"
    );
}

#[test]
fn test_issue_2124_duplicate_tag_key_deserialize_error() {
    // Attempting to deserialize a JSON object where both nesting levels share
    // the same tag key must fail with a clear error.
    let json = r#"{"type":"Wrap","type":"A","x":1}"#;
    let result = facet_json::from_str::<OuterSameTag>(json);
    assert!(
        result.is_err(),
        "expected error for duplicate tag key, got: {result:?}"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("same tag key"),
        "error should mention 'same tag key', got: {err}"
    );
}

// ---------------------------------------------------------------------------
// Flattened field name equals tag key — the field is silently shadowed
// ---------------------------------------------------------------------------

#[derive(Facet, Clone, PartialEq, Debug)]
pub struct HasTypeField {
    /// This field has the same name as the tag key used by the wrapping enum.
    pub kind: String,
    pub other: i32,
}

#[derive(Facet, Clone, PartialEq, Debug)]
#[facet(tag = "kind")]
#[repr(C)]
pub enum TagCollidesWithField {
    Wrap(HasTypeField),
}

#[test]
fn test_issue_2124_field_name_equals_tag_key_roundtrip() {
    // The struct field `kind` collides with the enum's tag key `kind`.
    // During serialization the tag is written first, then the struct's fields
    // are flattened — producing two `kind` entries in the JSON object.
    let value = TagCollidesWithField::Wrap(HasTypeField {
        kind: "should_be_lost".into(),
        other: 42,
    });

    let json = facet_json::to_string(&value).unwrap();
    // The JSON will contain two "kind" keys — the tag and the field.
    assert!(
        json.contains(r#""kind":"Wrap"#),
        "tag must be present: {json}"
    );

    // Deserializing back fails: the tag skip logic swallows all "kind" keys,
    // so the required struct field is never populated → missing field error.
    let result = facet_json::from_str::<TagCollidesWithField>(&json);
    assert!(
        result.is_err(),
        "should error because the struct field is shadowed by the tag key"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("kind"),
        "error should mention the missing field 'kind', got: {err}"
    );
}

#[test]
fn test_issue_2124_field_name_equals_tag_key_deser_from_known() {
    // Explicit JSON where the struct's `kind` field appears (after the tag).
    // The deserializer skips all occurrences of the tag key, so the required
    // field is never set — resulting in a missing-field error.
    let json = r#"{"kind":"Wrap","kind":"hello","other":99}"#;
    let result = facet_json::from_str::<TagCollidesWithField>(json);
    assert!(
        result.is_err(),
        "should error because the struct field is shadowed by the tag key"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("kind"),
        "error should mention the missing field 'kind', got: {err}"
    );
}

// ---------------------------------------------------------------------------
// Unknown fields in the newtype-chain path (without flatten)
// ---------------------------------------------------------------------------

#[test]
fn test_issue_2124_unknown_fields_skipped_in_newtype() {
    // Extra/unknown keys in the JSON should be silently skipped when
    // deserializing through a newtype chain (no #[facet(flatten)]).
    let json = r#"{"inner_type":"Exclude","name":"x","unknown_key":"ignored","another":123}"#;
    let back: Inner = facet_json::from_str(json).unwrap();
    assert_eq!(back, Inner::Exclude(Filter { name: "x".into() }));
}

#[test]
fn test_issue_2124_unknown_fields_skipped_in_nested_newtype() {
    // Three-level nesting with unknown fields scattered in the JSON object.
    let json = r#"{"outer_type":"Nested","bogus":true,"inner_type":"Include","add_type":"Full","extra":"nope"}"#;
    let back: Outer = facet_json::from_str(json).unwrap();
    assert_eq!(back, Outer::Nested(Inner::Include(AddOp::Full)));
}

// ---------------------------------------------------------------------------
// Unknown fields in the newtype-chain path (with flatten)
// ---------------------------------------------------------------------------

#[test]
fn test_issue_2124_unknown_fields_skipped_with_flatten_newtype() {
    // Place::Pin is a newtype wrapping Location which has #[facet(flatten)].
    // Unknown keys should be silently skipped.
    let json = r#"{"type":"Pin","label":"HQ","lat":1.0,"lng":2.0,"unknown":"skip_me"}"#;
    let back: Place = facet_json::from_str(json).unwrap();
    assert_eq!(
        back,
        Place::Pin(Location {
            label: "HQ".into(),
            coords: GeoCoords { lat: 1.0, lng: 2.0 },
        })
    );
}

#[test]
fn test_issue_2124_unknown_fields_skipped_with_flatten_nested_enum() {
    // Envelope::Doc is a newtype wrapping Document (tagged enum) whose
    // Report variant has #[facet(flatten)]. Unknown keys should be skipped.
    let json = r#"{"wrapper":"Doc","kind":"Report","title":"Annual","author":"Alice","version":3,"junk":false}"#;
    let back: Envelope = facet_json::from_str(json).unwrap();
    assert_eq!(
        back,
        Envelope::Doc(Document::Report {
            title: "Annual".into(),
            meta: Metadata {
                author: "Alice".into(),
                version: 3,
            },
        })
    );
}

// ---------------------------------------------------------------------------
// Newtype wrapping a struct with optional fields (defaults must apply)
// ---------------------------------------------------------------------------

#[test]
fn test_issue_2124_newtype_optional_fields() {
    let cases: Vec<(WithOpt, &str)> = vec![
        (
            WithOpt::Val(OptFields {
                required: "hi".into(),
                optional: None,
            }),
            r#"{"t":"Val","required":"hi","optional":null}"#,
        ),
        (
            WithOpt::Val(OptFields {
                required: "hi".into(),
                optional: Some(7),
            }),
            r#"{"t":"Val","required":"hi","optional":7}"#,
        ),
    ];

    for (expected, known_json) in cases {
        let json = facet_json::to_string(&expected).unwrap();
        let back: WithOpt = facet_json::from_str(&json).unwrap();
        assert_eq!(expected, back);

        let back: WithOpt = facet_json::from_str(known_json).unwrap();
        assert_eq!(expected, back);
    }
}
