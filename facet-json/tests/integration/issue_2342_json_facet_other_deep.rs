//! Test for issue #2342: `#[facet(other)]` should handle unknown JSON enum
//! variants even when the fallback payload is the full nested object.
//!
//! The minimal string case already exercises the shallow fallback path. The
//! Azure DevOps-shaped case is deeper: an adjacently tagged enum falls back to
//! `Other(RawJson)` and should preserve the complete object, not just the
//! adjacent content field.

use facet::Facet;
use facet_json::{RawJson, from_str};

#[derive(Debug, PartialEq, Eq, Facet)]
#[repr(C)]
enum SimpleKind {
    First,
    Second,
    #[facet(other)]
    Other(String),
}

#[derive(Debug, Facet)]
#[facet(tag = "scheme", content = "parameters")]
#[repr(C)]
enum Authorization {
    ServicePrincipal(ServicePrincipalParameters),
    UsernamePassword(UsernamePasswordParameters),
    #[facet(other)]
    Other(RawJson<'static>),
}

#[derive(Debug, Facet)]
#[facet(rename_all = "camelCase")]
struct ServicePrincipalParameters {
    service_principal_id: String,
    tenant_id: String,
}

#[derive(Debug, Facet)]
#[facet(rename_all = "camelCase")]
struct UsernamePasswordParameters {
    username: String,
    password: String,
}

#[test]
fn test_issue_2342_known_unit_variant_deserializes_from_string() {
    let parsed: SimpleKind = from_str(r#""First""#).unwrap();

    assert_eq!(parsed, SimpleKind::First);
}

#[test]
fn test_issue_2342_unknown_string_variant_falls_back_to_other() {
    let parsed: SimpleKind = from_str(r#""Third""#).unwrap();

    assert_eq!(parsed, SimpleKind::Other("Third".to_owned()));
}

#[test]
fn test_issue_2342_known_service_principal_adjacent_tagged_variant_deserializes() {
    let json = r#"{"scheme":"ServicePrincipal","parameters":{"servicePrincipalId":"spn","tenantId":"tenant"}}"#;

    let parsed: Authorization = from_str(json).unwrap();

    let Authorization::ServicePrincipal(parameters) = parsed else {
        panic!("expected Authorization::ServicePrincipal");
    };
    assert_eq!(parameters.service_principal_id, "spn");
    assert_eq!(parameters.tenant_id, "tenant");
}

#[test]
fn test_issue_2342_known_username_password_adjacent_tagged_variant_deserializes() {
    let json =
        r#"{"scheme":"UsernamePassword","parameters":{"username":"user","password":"secret"}}"#;

    let parsed: Authorization = from_str(json).unwrap();

    let Authorization::UsernamePassword(parameters) = parsed else {
        panic!("expected Authorization::UsernamePassword");
    };
    assert_eq!(parameters.username, "user");
    assert_eq!(parameters.password, "secret");
}

#[test]
fn test_issue_2342_unknown_adjacent_tagged_variant_falls_back_to_raw_json() {
    let json = r#"{"scheme":"Token","parameters":{"token":"redacted"}}"#;

    let parsed: Authorization = from_str(json).unwrap();

    let Authorization::Other(raw) = parsed else {
        panic!("expected Authorization::Other");
    };
    assert_eq!(raw.as_str(), json);
}
