use std::collections::BTreeMap;

use facet_value::ValueType;

use crate::exec::Tree;
use crate::value::{Payload, Value};

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

#[derive(Default)]
struct BuildScriptDirectives {
    rustc_cfg: Vec<String>,
    rustc_link_lib: Vec<String>,
    rustc_link_search: Vec<String>,
    rustc_env: Vec<String>,
    rustc_check_cfg: Vec<String>,
    warning: Vec<String>,
    rerun_if_changed: Vec<String>,
    rerun_if_env_changed: Vec<String>,
}

pub(crate) fn parse_build_directives(input: Value) -> Result<Value, String> {
    let text = input_text(input)?;
    let mut directives = BuildScriptDirectives::default();
    for line in text.lines() {
        let Some(rest) = line
            .strip_prefix("cargo::")
            .or_else(|| line.strip_prefix("cargo:"))
        else {
            continue;
        };
        let Some((key, value)) = rest.split_once('=') else {
            continue;
        };
        match key {
            "rustc-cfg" => directives.rustc_cfg.push(value.to_string()),
            "rustc-link-lib" => directives.rustc_link_lib.push(value.to_string()),
            "rustc-link-search" => directives.rustc_link_search.push(value.to_string()),
            "rustc-env" => directives.rustc_env.push(value.to_string()),
            "rustc-check-cfg" => directives.rustc_check_cfg.push(value.to_string()),
            "warning" => directives.warning.push(value.to_string()),
            "rerun-if-changed" => directives.rerun_if_changed.push(value.to_string()),
            "rerun-if-env-changed" => directives.rerun_if_env_changed.push(value.to_string()),
            _ => {}
        }
    }
    Ok(Value::Map(BTreeMap::from([
        (
            Value::Str("rustc_cfg".to_string()),
            string_array(directives.rustc_cfg),
        ),
        (
            Value::Str("rustc_link_lib".to_string()),
            string_array(directives.rustc_link_lib),
        ),
        (
            Value::Str("rustc_link_search".to_string()),
            string_array(directives.rustc_link_search),
        ),
        (
            Value::Str("rustc_env".to_string()),
            string_array(directives.rustc_env),
        ),
        (
            Value::Str("rustc_check_cfg".to_string()),
            string_array(directives.rustc_check_cfg),
        ),
        (
            Value::Str("warning".to_string()),
            string_array(directives.warning),
        ),
        (
            Value::Str("rerun_if_changed".to_string()),
            string_array(directives.rerun_if_changed),
        ),
        (
            Value::Str("rerun_if_env_changed".to_string()),
            string_array(directives.rerun_if_env_changed),
        ),
    ])))
}

fn string_array(values: Vec<String>) -> Value {
    Value::Array(values.into_iter().map(Value::Str).collect())
}

fn input_text(input: Value) -> Result<String, String> {
    match input {
        Value::Str(text) => Ok(text),
        Value::Tree(Tree { entries, blobs }) => {
            let len = entries.len() + blobs.len();
            if len != 1 {
                return Err(format!(
                    "parser input tree must contain exactly one blob, got {len}"
                ));
            }
            let (path, contents) = entries
                .into_iter()
                .map(|(path, contents)| Ok((path, contents)))
                .chain(blobs.into_iter().map(|(path, contents)| {
                    String::from_utf8(contents)
                        .map(|contents| (path, contents))
                        .map_err(|err| err.to_string())
                }))
                .next()
                .expect("one parser input")?;
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
