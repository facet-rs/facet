use facet::Facet;
use facet_yaml::{from_str as from_yaml, to_string as to_yaml};

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
fn test_flatten_in_internally_tagged_enum_yaml() {
    let foo = Wrapper::Foo {
        base: Base {
            name: "test".to_string(),
            value: 42,
        },
        extra: "extra".to_string(),
    };

    let yaml = to_yaml(&foo).expect("Failed to serialize YAML");
    eprintln!("Serialized YAML:\n{}", yaml);

    let deserialized: Wrapper = from_yaml(&yaml).expect("Failed to deserialize YAML");
    assert_eq!(foo, deserialized);
}
