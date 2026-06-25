//! Test for issue #2363: field proxies whose proxy type is an untagged enum
//! should deserialize bool, string, and number JSON values.
//!
//! The regression came from TypePlan caching a field-proxy-specific `bool` node
//! as if it were the plain `bool` node. When the proxy enum had a `Bool(bool)`
//! variant, that inner `bool` field incorrectly tried to start the outer field
//! proxy again and failed with "field does not have a proxy".

use facet::Facet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
struct BoolFirstLogin {
    #[facet(proxy = BoolFirstDefaultFlagProxy)]
    is_default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[facet(untagged)]
#[repr(C)]
enum BoolFirstDefaultFlagProxy {
    Bool(bool),
    String(String),
    Integer(i64),
}

impl From<BoolFirstDefaultFlagProxy> for bool {
    fn from(value: BoolFirstDefaultFlagProxy) -> Self {
        match value {
            BoolFirstDefaultFlagProxy::Bool(value) => value,
            BoolFirstDefaultFlagProxy::String(value) => parse_boolish(&value),
            BoolFirstDefaultFlagProxy::Integer(value) => value != 0,
        }
    }
}

impl From<&bool> for BoolFirstDefaultFlagProxy {
    fn from(value: &bool) -> Self {
        Self::Bool(*value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
struct StringFirstLogin {
    #[facet(proxy = StringFirstDefaultFlagProxy)]
    is_default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[facet(untagged)]
#[repr(C)]
enum StringFirstDefaultFlagProxy {
    String(String),
    Bool(bool),
    Integer(i64),
}

impl From<StringFirstDefaultFlagProxy> for bool {
    fn from(value: StringFirstDefaultFlagProxy) -> Self {
        match value {
            StringFirstDefaultFlagProxy::String(value) => parse_boolish(&value),
            StringFirstDefaultFlagProxy::Bool(value) => value,
            StringFirstDefaultFlagProxy::Integer(value) => value != 0,
        }
    }
}

impl From<&bool> for StringFirstDefaultFlagProxy {
    fn from(value: &bool) -> Self {
        Self::Bool(*value)
    }
}

fn parse_boolish(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "true" | "1" | "yes"
    )
}

#[test]
fn test_issue_2363_direct_untagged_enum_deserializes_bool_string_and_integer() {
    let values: Vec<BoolFirstDefaultFlagProxy> =
        facet_json::from_str(r#"[true,"true",1]"#).unwrap();

    assert_eq!(
        values,
        vec![
            BoolFirstDefaultFlagProxy::Bool(true),
            BoolFirstDefaultFlagProxy::Bool(true),
            BoolFirstDefaultFlagProxy::Integer(1),
        ]
    );
}

#[test]
fn test_issue_2363_bool_first_field_proxy_deserializes_bool_string_and_integer() {
    for (json, expected) in [
        (r#"{"is_default":true}"#, true),
        (r#"{"is_default":false}"#, false),
        (r#"{"is_default":"yes"}"#, true),
        (r#"{"is_default":"0"}"#, false),
        (r#"{"is_default":1}"#, true),
        (r#"{"is_default":0}"#, false),
    ] {
        let login: BoolFirstLogin = facet_json::from_str(json).unwrap();
        assert_eq!(login.is_default, expected, "json: {json}");
    }
}

#[test]
fn test_issue_2363_string_first_field_proxy_deserializes_bool_string_and_integer() {
    for (json, expected) in [
        (r#"{"is_default":true}"#, true),
        (r#"{"is_default":false}"#, false),
        (r#"{"is_default":"yes"}"#, true),
        (r#"{"is_default":"0"}"#, false),
        (r#"{"is_default":1}"#, true),
        (r#"{"is_default":0}"#, false),
    ] {
        let login: StringFirstLogin = facet_json::from_str(json).unwrap();
        assert_eq!(login.is_default, expected, "json: {json}");
    }
}
