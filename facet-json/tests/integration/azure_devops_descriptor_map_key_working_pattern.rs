//! Working pattern for descriptor-like JSON map keys.
//!
//! The failing enum repro shows that an enum key is treated as an enum variant
//! name before string proxy conversion can construct the descriptor. A current
//! workaround is to make the map key type transparently string-shaped, then add
//! normal Rust helpers for the semantic classification.

use facet::Facet;
use facet_json::{from_str, to_string};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Facet)]
#[facet(transparent)]
struct Descriptor(String);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DescriptorKind {
    Aad,
    Vssgp,
    Other,
}

impl Descriptor {
    fn kind(&self) -> DescriptorKind {
        if self.0.starts_with("aad.") {
            DescriptorKind::Aad
        } else if self.0.starts_with("vssgp.") {
            DescriptorKind::Vssgp
        } else {
            DescriptorKind::Other
        }
    }
}

#[derive(Debug, PartialEq, Eq, Facet)]
struct Member {
    display_name: String,
}

#[test]
fn transparent_string_newtype_deserializes_from_json_string() {
    let parsed: Descriptor = from_str(r#""aad.user""#).unwrap();

    assert_eq!(parsed, Descriptor("aad.user".to_owned()));
    assert_eq!(parsed.kind(), DescriptorKind::Aad);
}

#[test]
fn transparent_string_newtype_deserializes_as_hash_map_key() {
    let json = r#"{"aad.user":{"display_name":"Ada"}}"#;

    let parsed: HashMap<Descriptor, Member> = from_str(json).unwrap();

    let key = Descriptor("aad.user".to_owned());
    assert_eq!(key.kind(), DescriptorKind::Aad);
    assert_eq!(
        parsed.get(&key),
        Some(&Member {
            display_name: "Ada".to_owned()
        })
    );
}

#[test]
fn transparent_string_newtype_serializes_as_hash_map_key() {
    let map = HashMap::from([(
        Descriptor("vssgp.group".to_owned()),
        Member {
            display_name: "Cloud Ops".to_owned(),
        },
    )]);

    let json = to_string(&map).unwrap();

    assert_eq!(json, r#"{"vssgp.group":{"display_name":"Cloud Ops"}}"#);
}
