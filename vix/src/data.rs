use std::collections::BTreeMap;

use facet_value::ValueType;
use snark::grammar::RawGrammarJson;
use snark::lexical::LexicalFacts;
use snark::lower::weavy::{WeavyParsePlan, parse_prepared_weavy_with_report};
use snark::parser::{ParserGrammar, ResolvedCstNode};
use snark::validated::ValidatedGrammar;

use crate::exec::Tree;
use crate::value::{Payload, Value};

const CFG_GRAMMAR_JSON: &str = include_str!(concat!(env!("OUT_DIR"), "/cfg_grammar.json"));

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

pub(crate) fn parse_cfg(input: Value) -> Result<Value, String> {
    let text = input_text(input)?;
    parse_cfg_text(&text)
}

pub(crate) fn parse_rustc_cfg(input: Value) -> Result<Value, String> {
    let text = input_text(input)?;
    Ok(doc_list(
        text.lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(|line| Value::Str(line.to_string()))
            .collect(),
    ))
}

fn string_array(values: Vec<String>) -> Value {
    Value::Array(values.into_iter().map(Value::Str).collect())
}

fn doc_list(values: Vec<Value>) -> Value {
    values.into_iter().rev().fold(
        Value::Map(BTreeMap::from([(
            Value::Str("tag".to_string()),
            Value::Str("nil".to_string()),
        )])),
        |tail, head| {
            Value::Map(BTreeMap::from([
                (
                    Value::Str("tag".to_string()),
                    Value::Str("cons".to_string()),
                ),
                (Value::Str("head".to_string()), head),
                (Value::Str("tail".to_string()), tail),
            ]))
        },
    )
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

fn parse_cfg_text(text: &str) -> Result<Value, String> {
    let raw = RawGrammarJson::from_tree_sitter_json_str(CFG_GRAMMAR_JSON)
        .map_err(|err| format!("cfg grammar import failed: {err}"))?;
    let validated =
        ValidatedGrammar::from_raw(&raw).map_err(|err| format!("cfg grammar invalid: {err}"))?;
    let lexical = LexicalFacts::from_grammar(&validated);
    let parser = ParserGrammar::normalize_from_validated(&validated, &lexical)
        .map_err(|err| format!("cfg grammar normalization failed: {err}"))?
        .prepare_productions_for_items()
        .map_err(|err| format!("cfg grammar preparation failed: {err}"))?;
    let table = snark::parser::ParseTable::from_grammar(&parser)
        .map_err(|err| format!("cfg parse table failed: {err}"))?;
    let plan = WeavyParsePlan::new(&validated, &parser, &table)
        .map_err(|err| format!("cfg weavy parse plan failed: {err}"))?;
    let report = parse_prepared_weavy_with_report(&plan, &parser, &table, text)
        .map_err(|err| format!("cfg parse failed: {err:?}"))?;
    let resolved = report
        .accepted_resolved_tree(&parser, text)
        .ok_or_else(|| format!("cfg parse did not accept `{text}`"))?;
    cfg_value_from_node(&resolved)
}

fn cfg_value_from_node(node: &ResolvedCstNode) -> Result<Value, String> {
    match node.kind() {
        "ROOT" | "source_file" | "cfg" => {
            let child = node
                .children()
                .iter()
                .find(|child| matches!(child.field(), Some("expr") | Some("triple")))
                .ok_or_else(|| format!("cfg node `{}` had no expression child", node.kind()))?;
            cfg_value_from_node(child)
        }
        "any" => cfg_list_value("any", node),
        "all" => cfg_list_value("all", node),
        "not" => Ok(Value::Map(BTreeMap::from([
            (Value::Str("tag".to_string()), Value::Str("not".to_string())),
            (
                Value::Str("expr".to_string()),
                cfg_value_from_node(cfg_field(node, "expr")?)?,
            ),
        ]))),
        "key_value" => Ok(Value::Map(BTreeMap::from([
            (Value::Str("tag".to_string()), Value::Str("kv".to_string())),
            (
                Value::Str("key".to_string()),
                Value::Str(cfg_node_text(cfg_field(node, "key")?)),
            ),
            (
                Value::Str("value".to_string()),
                Value::Str(unquote_cfg_string(&cfg_node_text(cfg_field(
                    node, "value",
                )?))?),
            ),
        ]))),
        "atom" => Ok(Value::Map(BTreeMap::from([
            (
                Value::Str("tag".to_string()),
                Value::Str("atom".to_string()),
            ),
            (
                Value::Str("name".to_string()),
                Value::Str(cfg_node_text(cfg_field(node, "name")?)),
            ),
        ]))),
        "triple" => Ok(Value::Map(BTreeMap::from([
            (
                Value::Str("tag".to_string()),
                Value::Str("triple".to_string()),
            ),
            (
                Value::Str("value".to_string()),
                Value::Str(cfg_node_text(node)),
            ),
        ]))),
        other => Err(format!("unexpected cfg node `{other}`")),
    }
}

fn cfg_list_value(tag: &str, node: &ResolvedCstNode) -> Result<Value, String> {
    let exprs = node
        .children()
        .iter()
        .filter(|child| child.field() == Some("expr"))
        .map(cfg_value_from_node)
        .collect::<Result<_, _>>()?;
    Ok(Value::Map(BTreeMap::from([
        (Value::Str("tag".to_string()), Value::Str(tag.to_string())),
        (Value::Str("exprs".to_string()), doc_list(exprs)),
    ])))
}

fn cfg_field<'a>(node: &'a ResolvedCstNode, field: &str) -> Result<&'a ResolvedCstNode, String> {
    node.children()
        .iter()
        .find(|child| child.field() == Some(field))
        .ok_or_else(|| format!("cfg node `{}` missing field `{field}`", node.kind()))
}

fn cfg_node_text(node: &ResolvedCstNode) -> String {
    if let Some(text) = node.text() {
        return text.to_string();
    }
    let mut out = String::new();
    for child in node.children() {
        out.push_str(&cfg_node_text(child));
    }
    out
}

fn unquote_cfg_string(text: &str) -> Result<String, String> {
    let Some(inner) = text
        .strip_prefix('"')
        .and_then(|text| text.strip_suffix('"'))
    else {
        return Err(format!("cfg string literal was not quoted: `{text}`"));
    };
    Ok(inner.replace("\\\"", "\"").replace("\\\\", "\\"))
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
