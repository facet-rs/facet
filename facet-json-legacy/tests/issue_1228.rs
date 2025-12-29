use facet::Facet;
use facet_json_legacy::from_str;
use facet_testhelpers::test;

/// Test for issue #1228: Deserializing dataless enum values
/// https://github.com/facet-rs/facet/issues/1228
///
/// The issue is that untagged enums with simple (dataless) variants
/// cannot deserialize from scalar string values like "AE".
#[test]
fn test_untagged_dataless_enum_as_struct_field() {
    #[derive(Clone, Copy, PartialEq, Eq, Hash, Facet, Debug)]
    #[facet(untagged)]
    #[repr(u8)]
    pub enum Alla {
        AE,
        AD,
    }

    #[derive(Clone, Copy, PartialEq, Eq, Hash, Facet, Debug)]
    pub struct Data {
        pub a: Alla,
        pub b: Alla,
    }

    // This should deserialize successfully but currently fails with:
    // JsonError { kind: InvalidValue { message: "no scalar-accepting variants in untagged enum Alla" }
    let result: Data = from_str(r#"{"a":"AE", "b":"AE"}"#).unwrap();

    assert_eq!(result.a, Alla::AE);
    assert_eq!(result.b, Alla::AE);
}

/// Test deserializing the untagged enum directly (not as a struct field)
#[test]
fn test_untagged_dataless_enum_direct() {
    #[derive(Clone, Copy, PartialEq, Eq, Hash, Facet, Debug)]
    #[facet(untagged)]
    #[repr(u8)]
    pub enum Alla {
        AE,
        AD,
    }

    // Test deserializing the enum directly from a string
    let result: Alla = from_str(r#""AE""#).unwrap();
    assert_eq!(result, Alla::AE);

    let result: Alla = from_str(r#""AD""#).unwrap();
    assert_eq!(result, Alla::AD);
}
