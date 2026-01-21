// Test cases for issues 1721 and 1724

use facet::Facet;
use facet_json::{from_str as from_json, to_string};

#[test]
fn test_deserialize_flattened_enum() {
    #[derive(Facet, Clone, Debug, PartialEq)]
    pub struct O {
        #[facet(flatten)]
        pub p: Pd,
    }

    #[derive(Facet, Clone, Debug, PartialEq)]
    #[facet(tag = "ty")]
    #[repr(C)]
    pub enum Pd {
        A(Ai),
    }

    #[derive(Facet, Clone, Debug, PartialEq)]
    pub struct Ai {
        pub pi: String,
    }

    let json = r#"{"ty":"A","pi":"1000"}"#;
    let parsed: O = from_json(json).expect("Failed to deserialize JSON");

    // Verify the parsed structure is correct
    assert_eq!(
        parsed.p,
        Pd::A(Ai {
            pi: "1000".to_string()
        })
    );

    // Test round-trip serialization
    let serialized = to_string(&parsed).expect("Failed to serialize to JSON");
    assert_eq!(
        json, serialized,
        "Round-trip failed: input and output JSON do not match"
    );
}

#[test]
fn test_deserialize_flattened_enum_with_same_name() {
    #[derive(Clone, Facet, Debug, PartialEq)]
    #[facet(tag = "model")]
    #[repr(C)]
    pub enum Mod {
        A { s: f64 },
        B { s: f64 },
    }

    #[derive(Clone, Facet, Debug, PartialEq)]
    pub struct Outer {
        pub c: String,
        #[facet(flatten)]
        pub model: Mod,
    }

    let json = r#"{"c":"example","s":0.0,"model":"B"}"#;
    let parsed: Outer = from_json(json).expect("Failed to deserialize JSON");

    // Verify the parsed structure is correct
    assert_eq!(parsed.c, "example");
    assert_eq!(parsed.model, Mod::B { s: 0.0 });

    // Test round-trip serialization
    // Note: JSON field order is not semantically significant, so we compare parsed values
    let serialized = to_string(&parsed).expect("Failed to serialize to JSON");
    let reparsed: Outer = from_json(&serialized).expect("Failed to re-parse serialized JSON");
    assert_eq!(
        parsed, reparsed,
        "Round-trip failed: parsed values do not match"
    );
}
