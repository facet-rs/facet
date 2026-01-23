use facet::Facet;
use facet_json::{from_str as from_json, to_string as to_json};

#[derive(Facet, Debug, Clone, PartialEq, Eq)]
pub struct Base {
    pub name: String,
    pub value: i32,
}

#[derive(Facet, Debug, Clone, PartialEq, Eq)]
#[facet(tag = "type")]
#[repr(C)]
pub enum Wrapper {
    #[facet(rename = "foo")]
    Foo {
        #[facet(flatten)]
        base: Base,
        extra: String,
    },
    #[facet(rename = "bar")]
    Bar {
        #[facet(flatten)]
        base: Base,
    },
}

#[test]
fn test_flatten_in_internally_tagged_enum_foo() {
    let foo = Wrapper::Foo {
        base: Base {
            name: "test".to_string(),
            value: 42,
        },
        extra: "extra".to_string(),
    };

    let json = to_json(&foo).expect("Failed to serialize JSON");
    eprintln!("Serialized JSON: {}", json);

    let deserialized: Wrapper = from_json(&json).expect("Failed to deserialize JSON");
    assert_eq!(foo, deserialized);
}

#[test]
fn test_flatten_in_internally_tagged_enum_bar() {
    let bar = Wrapper::Bar {
        base: Base {
            name: "bar_test".to_string(),
            value: 123,
        },
    };

    let json = to_json(&bar).expect("Failed to serialize JSON");
    eprintln!("Serialized JSON: {}", json);

    let deserialized: Wrapper = from_json(&json).expect("Failed to deserialize JSON");
    assert_eq!(bar, deserialized);
}

#[test]
fn test_flatten_roundtrip_from_manual_json() {
    // Test deserializing from manually constructed JSON
    let json = r#"{"type":"foo","name":"manual","value":99,"extra":"test"}"#;
    let deserialized: Wrapper = from_json(json).expect("Failed to deserialize JSON");

    assert_eq!(
        deserialized,
        Wrapper::Foo {
            base: Base {
                name: "manual".to_string(),
                value: 99,
            },
            extra: "test".to_string(),
        }
    );
}
