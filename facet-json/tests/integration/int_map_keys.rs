//! Integer-keyed maps must deserialize regardless of key width.
//!
//! Map keys arrive as strings and are parsed in
//! `facet-format::deserializer::setters::deserialize_map_key_terminal_inner`,
//! which must parse to the exact target width — `Partial::set` does not
//! convert between numeric sizes, so parsing everything as i64/u64 fails for
//! narrower key types.

use std::collections::BTreeMap;

use facet::Facet;
use facet_json::from_str;

#[derive(Facet, Debug, PartialEq)]
struct Keys {
    i8s: BTreeMap<i8, String>,
    i32s: BTreeMap<i32, String>,
    u16s: BTreeMap<u16, String>,
    u128s: BTreeMap<u128, String>,
}

#[test]
fn integer_map_keys_parse_at_every_width() {
    let json = r#"{
        "i8s": {"-3": "a"},
        "i32s": {"100000": "b"},
        "u16s": {"65535": "c"},
        "u128s": {"340282366920938463463374607431768211455": "d"}
    }"#;
    let keys: Keys = from_str(json).unwrap();
    assert_eq!(keys.i8s.get(&-3).map(String::as_str), Some("a"));
    assert_eq!(keys.i32s.get(&100_000).map(String::as_str), Some("b"));
    assert_eq!(keys.u16s.get(&65_535).map(String::as_str), Some("c"));
    assert_eq!(keys.u128s.get(&u128::MAX).map(String::as_str), Some("d"));
}

#[test]
fn out_of_range_integer_map_key_errors() {
    let err = from_str::<BTreeMap<i8, String>>(r#"{"300": "x"}"#).unwrap_err();
    assert!(
        err.to_string().contains("valid integer for map key"),
        "got: {err}"
    );
}
