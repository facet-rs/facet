use facet_json_legacy::from_str;
use facet_testhelpers::test;
use facet_value::Value;

#[test]
fn deserialize_null_into_value() {
    let v: Value = from_str("null").unwrap();
    assert!(v.is_null());
}

#[test]
fn deserialize_true_into_value() {
    let v: Value = from_str("true").unwrap();
    assert_eq!(v.as_bool(), Some(true));
}

#[test]
fn deserialize_false_into_value() {
    let v: Value = from_str("false").unwrap();
    assert_eq!(v.as_bool(), Some(false));
}

#[test]
fn deserialize_integer_into_value() {
    let v: Value = from_str("42").unwrap();
    assert_eq!(v.as_number().and_then(|n| n.to_i64()), Some(42));
}

#[test]
fn deserialize_negative_integer_into_value() {
    let v: Value = from_str("-123").unwrap();
    assert_eq!(v.as_number().and_then(|n| n.to_i64()), Some(-123));
}

#[test]
fn deserialize_float_into_value() {
    let v: Value = from_str("3.15").unwrap();
    let f = v.as_number().and_then(|n| n.to_f64()).unwrap();
    assert!((f - 3.15).abs() < 0.001);
}

#[test]
fn deserialize_string_into_value() {
    let v: Value = from_str(r#""hello world""#).unwrap();
    assert_eq!(v.as_string().map(|s| s.as_str()), Some("hello world"));
}

#[test]
fn deserialize_empty_string_into_value() {
    let v: Value = from_str(r#""""#).unwrap();
    assert_eq!(v.as_string().map(|s| s.as_str()), Some(""));
}

#[test]
fn deserialize_empty_array_into_value() {
    let v: Value = from_str("[]").unwrap();
    let arr = v.as_array().unwrap();
    assert!(arr.is_empty());
}

#[test]
fn deserialize_integer_array_into_value() {
    let v: Value = from_str("[1, 2, 3]").unwrap();
    let arr = v.as_array().unwrap();
    assert_eq!(arr.len(), 3);
    assert_eq!(arr[0].as_number().and_then(|n| n.to_i64()), Some(1));
    assert_eq!(arr[1].as_number().and_then(|n| n.to_i64()), Some(2));
    assert_eq!(arr[2].as_number().and_then(|n| n.to_i64()), Some(3));
}

#[test]
fn deserialize_mixed_array_into_value() {
    let v: Value = from_str(r#"[1, "two", true, null]"#).unwrap();
    let arr = v.as_array().unwrap();
    assert_eq!(arr.len(), 4);
    assert_eq!(arr[0].as_number().and_then(|n| n.to_i64()), Some(1));
    assert_eq!(arr[1].as_string().map(|s| s.as_str()), Some("two"));
    assert_eq!(arr[2].as_bool(), Some(true));
    assert!(arr[3].is_null());
}

#[test]
fn deserialize_nested_array_into_value() {
    let v: Value = from_str("[[1, 2], [3, 4]]").unwrap();
    let arr = v.as_array().unwrap();
    assert_eq!(arr.len(), 2);

    let inner0 = arr[0].as_array().unwrap();
    assert_eq!(inner0.len(), 2);
    assert_eq!(inner0[0].as_number().and_then(|n| n.to_i64()), Some(1));
    assert_eq!(inner0[1].as_number().and_then(|n| n.to_i64()), Some(2));

    let inner1 = arr[1].as_array().unwrap();
    assert_eq!(inner1.len(), 2);
    assert_eq!(inner1[0].as_number().and_then(|n| n.to_i64()), Some(3));
    assert_eq!(inner1[1].as_number().and_then(|n| n.to_i64()), Some(4));
}
