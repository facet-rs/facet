//! Repro for field-level proxy deserialization through an untagged enum with a
//! bool newtype variant.
//!
//! A direct untagged enum can deserialize heterogeneous scalar JSON values, but
//! the same enum shape currently fails when used as a field proxy and the JSON
//! value is a boolean.

use facet::Facet;
use facet_json::RawJson;

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
struct RawJsonLogin {
    #[facet(proxy = RawJson<'static>)]
    is_default: RawJsonDefaultFlag,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[facet(transparent)]
struct RawJsonDefaultFlag(bool);

impl TryFrom<RawJson<'static>> for RawJsonDefaultFlag {
    type Error = String;

    fn try_from(value: RawJson<'static>) -> Result<Self, Self::Error> {
        let value = value.as_str().trim();
        let value = value.strip_prefix('"').unwrap_or(value);
        let value = value.strip_suffix('"').unwrap_or(value);
        Ok(Self(matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "true" | "1" | "yes"
        )))
    }
}

impl From<&RawJsonDefaultFlag> for RawJson<'static> {
    fn from(value: &RawJsonDefaultFlag) -> Self {
        RawJson::from_owned(value.0.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
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
            BoolFirstDefaultFlagProxy::String(value) => matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "true" | "1" | "yes"
            ),
            BoolFirstDefaultFlagProxy::Integer(value) => value != 0,
        }
    }
}

impl From<&bool> for BoolFirstDefaultFlagProxy {
    fn from(value: &bool) -> Self {
        Self::Bool(*value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
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
            StringFirstDefaultFlagProxy::String(value) => matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "true" | "1" | "yes"
            ),
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

#[test]
fn direct_untagged_enum_deserializes_bool_string_and_integer() {
    let values: Vec<BoolFirstDefaultFlagProxy> = facet_json::from_str(r#"[true,"true",1]"#).unwrap();

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
fn string_first_field_proxy_untagged_enum_deserializes_string_and_integer() {
    let string_login: StringFirstLogin = facet_json::from_str(r#"{"is_default":"true"}"#).unwrap();
    let integer_login: StringFirstLogin = facet_json::from_str(r#"{"is_default":1}"#).unwrap();

    assert!(string_login.is_default);
    assert!(integer_login.is_default);
}

#[test]
fn raw_json_field_proxy_deserializes_bool_string_and_integer() {
    let bool_login: RawJsonLogin = facet_json::from_str(r#"{"is_default":true}"#).unwrap();
    let string_login: RawJsonLogin = facet_json::from_str(r#"{"is_default":"true"}"#).unwrap();
    let integer_login: RawJsonLogin = facet_json::from_str(r#"{"is_default":1}"#).unwrap();

    assert!(bool_login.is_default.0);
    assert!(string_login.is_default.0);
    assert!(integer_login.is_default.0);
}

#[test]
// #[ignore = "currently fails with `Operation failed on shape bool: field does not have a proxy`"]
fn bool_first_field_proxy_untagged_enum_deserializes_string() {
    let login: BoolFirstLogin = facet_json::from_str(r#"{"is_default":"true"}"#).unwrap();

    assert!(login.is_default);
}

#[test]
// #[ignore = "currently fails with `Operation failed on shape bool: field does not have a proxy`"]
fn bool_first_field_proxy_untagged_enum_deserializes_bool() {
    let login: BoolFirstLogin = facet_json::from_str(r#"{"is_default":true}"#).unwrap();

    assert!(login.is_default);
}

#[test]
// #[ignore = "currently fails with `Operation failed on shape bool: field does not have a proxy`"]
fn string_first_field_proxy_untagged_enum_deserializes_bool() {
    let login: StringFirstLogin = facet_json::from_str(r#"{"is_default":true}"#).unwrap();

    assert!(login.is_default);
}
