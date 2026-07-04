use std::collections::BTreeMap;

use facet_value::ValueType;

use crate::exec::Tree;
use crate::oracle::{Payload, Value};

pub(crate) fn parse_toml(input: Value) -> Result<Value, String> {
    let text = input_text(input)?;
    let value: facet_value::Value =
        facet_toml::from_str(&text).map_err(|e| format!("toml parse failed: {e}"))?;
    from_facet_value(&value)
}

pub(crate) fn parse_json(input: Value) -> Result<Value, String> {
    let text = input_text(input)?;
    let value: facet_value::Value =
        facet_json::from_str(&text).map_err(|e| format!("json parse failed: {e}"))?;
    from_facet_value(&value)
}

fn input_text(input: Value) -> Result<String, String> {
    match input {
        Value::Str(text) => Ok(text),
        Value::Tree(Tree { entries }) => {
            let len = entries.len();
            let [(path, contents)] =
                entries
                    .into_iter()
                    .collect::<Vec<_>>()
                    .try_into()
                    .map_err(|_| {
                        format!("parser input tree must contain exactly one blob, got {len}")
                    })?;
            if contents.is_empty() {
                Err(format!("parser input blob `{path}` is empty"))
            } else {
                Ok(contents)
            }
        }
        other => Err(format!(
            "parser input must be a string or single-blob tree, got {other:?}"
        )),
    }
}

fn from_facet_value(value: &facet_value::Value) -> Result<Value, String> {
    match value.value_type() {
        ValueType::Null => Ok(option_none()),
        ValueType::Bool => Ok(Value::Bool(value.as_bool().expect("bool value"))),
        ValueType::Number => {
            let number = value.as_number().expect("number value");
            if let Some(int) = number.to_i64() {
                Ok(Value::Int(int))
            } else {
                Ok(Value::Float(number.to_f64_lossy()))
            }
        }
        ValueType::String => Ok(Value::Str(
            value
                .as_string()
                .expect("string value")
                .as_str()
                .to_string(),
        )),
        ValueType::Array => Ok(Value::Array(
            value
                .as_array()
                .expect("array value")
                .iter()
                .map(from_facet_value)
                .collect::<Result<_, _>>()?,
        )),
        ValueType::Object => {
            let mut map = BTreeMap::new();
            for (key, value) in value.as_object().expect("object value").iter() {
                map.insert(
                    Value::Str(key.as_str().to_string()),
                    from_facet_value(value)?,
                );
            }
            Ok(Value::Map(map))
        }
        ValueType::DateTime => {
            // TOML datetime shape-checking/coercion is deferred; v1 exposes it as text.
            Ok(Value::Str(format!(
                "{:?}",
                value.as_datetime().expect("datetime value")
            )))
        }
        other => Err(format!("unsupported parser value kind {other:?}")),
    }
}

fn option_none() -> Value {
    Value::Variant {
        enum_name: "Option".to_string(),
        index: 1,
        name: "None".to_string(),
        payload: Payload::Unit,
    }
}
