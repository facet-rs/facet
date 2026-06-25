//! Azure DevOps-shaped reproduction: adjacently tagged enum with a RawJson catch-all.
//!
//! This is intentionally ignored so it can live in the Facet checkout as a
//! copyable GitHub issue repro without breaking the normal test suite.

use facet::Facet;
use facet_json::{RawJson, from_str};

#[derive(Debug, Facet)]
#[facet(tag = "scheme", content = "parameters")]
#[repr(C)]
enum Authorization {
    ServicePrincipal(ServicePrincipalParameters),
    #[facet(other)]
    Other(RawJson<'static>),
}

#[derive(Debug, Facet)]
#[facet(rename_all = "camelCase")]
struct ServicePrincipalParameters {
    service_principal_id: String,
    tenant_id: String,
}

#[test]
fn azure_devops_known_adjacent_tagged_variant_deserializes() {
    let json = r#"{"scheme":"ServicePrincipal","parameters":{"servicePrincipalId":"spn","tenantId":"tenant"}}"#;

    let parsed: Authorization = from_str(json).unwrap();

    let Authorization::ServicePrincipal(parameters) = parsed else {
        panic!("expected Authorization::ServicePrincipal");
    };
    assert_eq!(parameters.service_principal_id, "spn");
    assert_eq!(parameters.tenant_id, "tenant");
}

#[test]
#[ignore = "Facet currently errors before trying the #[facet(other)] RawJson fallback"]
fn azure_devops_unknown_adjacent_tagged_variant_falls_back_to_raw_json() {
    let json = r#"{"scheme":"Token","parameters":{"token":"redacted"}}"#;

    let parsed: Authorization = from_str(json).unwrap();

    let Authorization::Other(raw) = parsed else {
        panic!("expected Authorization::Other");
    };
    assert_eq!(raw.as_str(), json);
}
