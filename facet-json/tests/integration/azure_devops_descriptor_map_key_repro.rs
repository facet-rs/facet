//! Azure DevOps-shaped reproduction: opaque/proxy enum works as a value, but
//! not as a JSON map key.

use facet::Facet;
use facet_json::from_str;
use std::collections::HashMap;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Facet)]
#[facet(proxy = String)]
#[repr(C)]
enum Descriptor {
    Aad(String),
    Vssgp(String),
    Other(String),
}

impl FromStr for Descriptor {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.starts_with("aad.") {
            Ok(Self::Aad(value.to_owned()))
        } else if value.starts_with("vssgp.") {
            Ok(Self::Vssgp(value.to_owned()))
        } else {
            Ok(Self::Other(value.to_owned()))
        }
    }
}

impl TryFrom<String> for Descriptor {
    type Error = &'static str;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<&Descriptor> for String {
    fn from(value: &Descriptor) -> Self {
        match value {
            Descriptor::Aad(value) | Descriptor::Vssgp(value) | Descriptor::Other(value) => {
                value.clone()
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq, Facet)]
struct Member {
    display_name: String,
}

#[test]
fn azure_devops_opaque_proxy_enum_deserializes_from_json_string() {
    let parsed: Descriptor = from_str(r#""aad.user""#).unwrap();

    assert_eq!(parsed, Descriptor::Aad("aad.user".to_owned()));
}

#[test]
#[ignore = "Facet currently treats the JSON object key as an enum variant name instead of using the String proxy"]
fn azure_devops_opaque_proxy_enum_deserializes_as_hash_map_key() {
    let json = r#"{"aad.user":{"display_name":"Ada"}}"#;

    let parsed: HashMap<Descriptor, Member> = from_str(json).unwrap();

    assert_eq!(
        parsed.get(&Descriptor::Aad("aad.user".to_owned())),
        Some(&Member {
            display_name: "Ada".to_owned()
        })
    );
}
