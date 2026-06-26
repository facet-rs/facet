//! Test for issue #2341: string-proxy domain keys should work as JSON object
//! keys.
//!
//! Azure DevOps returns descriptor-indexed maps. Before the fix, callers could
//! deserialize the wire JSON as `HashMap<String, T>` and then map the string
//! keys into their domain key type. Direct deserialization into
//! `HashMap<Descriptor, T>` failed because map-key deserialization tried to
//! select an enum variant named by the object key before honoring the
//! container-level `String` proxy.

use facet::Facet;
use facet_json::from_str;
use std::collections::HashMap;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Facet)]
#[facet(opaque, proxy = String)]
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
    descriptor: Descriptor,
    display_name: String,
}

fn map_members_by_descriptor(
    members_by_descriptor: HashMap<String, Member>,
) -> Result<HashMap<Descriptor, Member>, &'static str> {
    members_by_descriptor
        .into_iter()
        .map(|(descriptor, member)| Ok((descriptor.parse()?, member)))
        .collect()
}

#[test]
fn test_issue_2341_proxy_enum_deserializes_from_json_string() {
    let parsed: Descriptor = from_str(r#""aad.user""#).unwrap();

    assert_eq!(parsed, Descriptor::Aad("aad.user".to_owned()));
}

#[test]
fn test_issue_2341_mapper_workaround_maps_string_keys_after_deserializing_wire_map() {
    let json = r#"{"aad.user":{"descriptor":"aad.user","display_name":"Ada"}}"#;

    let members_by_descriptor: HashMap<String, Member> = from_str(json).unwrap();
    let members = map_members_by_descriptor(members_by_descriptor).unwrap();

    let descriptor = Descriptor::Aad("aad.user".to_owned());
    let member = members
        .get(&descriptor)
        .expect("mapped descriptor key should be present");
    assert_eq!(member.display_name, "Ada");
    assert_eq!(&member.descriptor, &descriptor);
}

#[test]
fn test_issue_2341_proxy_enum_deserializes_as_hash_map_key() {
    let json = r#"{"aad.user":{"descriptor":"aad.user","display_name":"Ada"}}"#;

    let parsed: HashMap<Descriptor, Member> = from_str(json).unwrap();

    assert_eq!(
        parsed.get(&Descriptor::Aad("aad.user".to_owned())),
        Some(&Member {
            descriptor: Descriptor::Aad("aad.user".to_owned()),
            display_name: "Ada".to_owned()
        })
    );
}
