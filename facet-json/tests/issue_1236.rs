//! Test for https://github.com/facet-rs/facet/issues/1236
//! Newtypes marked with #[facet(transparent)] should serialize as their inner value.

use std::{collections::HashMap, sync::Arc};

use facet::Facet;

#[derive(Clone, Debug, Facet)]
struct Data {
    #[facet(default, proxy = Proxy)]
    pub corr: Option<Arc<HashMap<(String, String), f64>>>,
}

#[derive(Facet)]
#[facet(transparent)]
struct Proxy(Vec<(String, String, f64)>);

impl TryFrom<Proxy> for Option<Arc<HashMap<(String, String), f64>>> {
    type Error = String;

    fn try_from(v: Proxy) -> Result<Self, Self::Error> {
        let map = HashMap::from_iter(v.0.into_iter().map(|v| ((v.0, v.1), v.2)));
        Ok(Some(Arc::new(map)))
    }
}

impl From<&Option<Arc<HashMap<(String, String), f64>>>> for Proxy {
    fn from(v: &Option<Arc<HashMap<(String, String), f64>>>) -> Self {
        match v {
            None => Proxy(vec![]),
            Some(a) => {
                let a: Vec<(String, String, f64)> = a
                    .iter()
                    .map(|((a, b), c)| (a.clone(), b.clone(), *c))
                    .collect();
                Proxy(a)
            }
        }
    }
}

#[test]
fn test_repro_1236() {
    let json = r#"{"corr":[["a","b",0.95]]}"#;
    let d: Data = facet_json::from_str(json).unwrap();
    assert!(d.corr.is_some());
    let corr = d.corr.as_ref().unwrap();
    assert_eq!(corr.get(&("a".to_string(), "b".to_string())), Some(&0.95));

    // Verify serialization roundtrip
    let serialized = facet_json::to_string(&d).unwrap();
    assert_eq!(serialized, json);
}

#[derive(Debug, PartialEq, Facet)]
#[facet(transparent)]
struct ScalarNewtype(i32);

#[test]
fn test_newtype_scalar_roundtrip() {
    // Transparent newtype with scalar should serialize as the inner value
    let value = ScalarNewtype(42);
    let json = facet_json::to_string(&value).unwrap();
    assert_eq!(json, "42");

    let parsed: ScalarNewtype = facet_json::from_str(&json).unwrap();
    assert_eq!(parsed, value);
}

#[derive(Debug, PartialEq, Facet)]
#[facet(transparent)]
struct VecNewtype(Vec<i32>);

#[test]
fn test_newtype_vec_roundtrip() {
    // Transparent newtype with Vec should serialize as the inner array (no double wrapping)
    let value = VecNewtype(vec![1, 2, 3]);
    let json = facet_json::to_string(&value).unwrap();
    assert_eq!(json, "[1,2,3]");

    let parsed: VecNewtype = facet_json::from_str(&json).unwrap();
    assert_eq!(parsed, value);
}

#[test]
fn test_newtype_empty_vec_roundtrip() {
    // Edge case: empty Vec inside newtype
    let value = VecNewtype(vec![]);
    let json = facet_json::to_string(&value).unwrap();
    assert_eq!(json, "[]");

    let parsed: VecNewtype = facet_json::from_str(&json).unwrap();
    assert_eq!(parsed, value);
}

#[derive(Debug, PartialEq, Facet)]
#[facet(transparent)]
struct InnerNewtype(i32);

#[derive(Debug, PartialEq, Facet)]
#[facet(transparent)]
struct OuterNewtype(InnerNewtype);

#[test]
fn test_nested_newtypes_roundtrip() {
    // Nested transparent newtypes should both serialize as the inner value
    let value = OuterNewtype(InnerNewtype(42));
    let json = facet_json::to_string(&value).unwrap();
    assert_eq!(json, "42");

    let parsed: OuterNewtype = facet_json::from_str(&json).unwrap();
    assert_eq!(parsed, value);
}

#[test]
fn test_plain_tuple_not_transparent() {
    // Plain tuple (i32,) should serialize as array [42], not as 42
    let value: (i32,) = (42,);
    let json = facet_json::to_string(&value).unwrap();
    assert_eq!(json, "[42]");

    let parsed: (i32,) = facet_json::from_str(&json).unwrap();
    assert_eq!(parsed, value);
}

#[derive(Debug, PartialEq, Facet)]
struct TwoFieldTuple(i32, i32);

#[test]
fn test_multi_field_tuple_struct_not_transparent() {
    // Tuple struct with 2+ fields is NOT a newtype, should use array syntax
    let value = TwoFieldTuple(1, 2);
    let json = facet_json::to_string(&value).unwrap();
    assert_eq!(json, "[1,2]");

    let parsed: TwoFieldTuple = facet_json::from_str(&json).unwrap();
    assert_eq!(parsed, value);
}

#[derive(Debug, PartialEq, Facet)]
#[facet(transparent)]
struct TupleNewtype((i32, i32));

#[test]
fn test_newtype_containing_tuple_roundtrip() {
    // Outer transparent newtype unwraps, inner tuple uses array syntax
    let value = TupleNewtype((1, 2));
    let json = facet_json::to_string(&value).unwrap();
    assert_eq!(json, "[1,2]");

    let parsed: TupleNewtype = facet_json::from_str(&json).unwrap();
    assert_eq!(parsed, value);
}
