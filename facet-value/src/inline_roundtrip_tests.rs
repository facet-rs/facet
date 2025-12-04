use crate::format::{format_value, format_value_with_spans};
use crate::{VArray, VObject, Value, value};

fn json_to_value(json: &serde_json::Value) -> Value {
    match json {
        serde_json::Value::Null => Value::NULL,
        serde_json::Value::Bool(b) => Value::from(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::from(i)
            } else if let Some(u) = n.as_u64() {
                Value::from(u as i64)
            } else {
                Value::from(n.as_f64().unwrap())
            }
        }
        serde_json::Value::String(s) => Value::from(s.as_str()),
        serde_json::Value::Array(items) => {
            let mut arr = VArray::new();
            for item in items {
                arr.push(json_to_value(item));
            }
            arr.into()
        }
        serde_json::Value::Object(map) => {
            let mut obj = VObject::new();
            for (k, v) in map {
                obj.insert(k.as_str(), json_to_value(v));
            }
            obj.into()
        }
    }
}

#[test]
fn inline_string_round_trip_through_json_formatting() {
    let value = Value::from("facet");
    assert!(value.is_inline_string());

    let formatted = format_value(&value);
    let parsed = serde_json::from_str::<serde_json::Value>(&formatted).expect("valid json");
    let roundtrip = json_to_value(&parsed);

    assert_eq!(roundtrip, value);
    assert!(
        roundtrip.is_inline_string(),
        "round-tripped value should remain inline"
    );
}

#[test]
fn inline_strings_survive_nested_json_round_trip() {
    let mut inner = VObject::new();
    inner.insert("short", Value::from("tiny"));
    inner.insert("long", Value::from("this string will force heap storage"));

    let mut array = VArray::new();
    array.push(Value::from("a"));
    array.push(Value::from("bc"));
    array.push(Value::from(inner));

    let root = value!({
        "title": "facet",
        "data": array,
        "flag": true
    });

    let formatted = format_value(&root);
    let parsed = serde_json::from_str::<serde_json::Value>(&formatted).expect("valid json");
    let roundtrip = json_to_value(&parsed);

    assert_eq!(roundtrip, root);
    let obj = roundtrip.as_object().unwrap();
    let title = obj.get("title").unwrap();
    assert!(title.is_inline_string(), "title should remain inline");
    assert_eq!(title.as_string().unwrap().as_str(), "facet");
}

#[test]
fn facet_pretty_formatting_preserves_inline_strings() {
    let value = value!({
        "view": ["inline", "strings", "stay", "put"]
    });
    let formatted = format_value_with_spans(&value);
    assert!(
        formatted.text.contains("\"inline\""),
        "pretty output should include inline literal"
    );

    let arr = value
        .as_object()
        .unwrap()
        .get("view")
        .unwrap()
        .as_array()
        .unwrap();
    for entry in arr.iter() {
        assert!(
            entry.is_inline_string(),
            "pretty formatting should not force heap allocation"
        );
    }
}
