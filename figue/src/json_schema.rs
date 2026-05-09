//! JSON Schema export for figue config roots.

use std::fmt;
use std::fs;
use std::path::{Path as FsPath, PathBuf};
use std::string::String;
use std::vec::Vec;

use facet::Facet;

use crate::config_value::ConfigValue;
use crate::schema::{
    ConfigEnumSchema, ConfigEnumVariantSchema, ConfigFieldSchema, ConfigStructSchema,
    ConfigValueSchema, Docs, LeafKind, ScalarType, Schema, error::SchemaError,
};

/// One generated JSON Schema document and the file name it should be written to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonSchemaFile {
    /// File name, derived from the config root name, for example `cfg.schema.json`.
    pub file_name: String,
    /// Pretty-printed JSON Schema contents.
    pub contents: String,
}

/// Error returned while generating or writing JSON Schema files.
#[derive(Debug)]
pub enum JsonSchemaError {
    /// The figue schema could not be built from the target type.
    Schema(SchemaError),
    /// A filesystem operation failed while writing schemas.
    Io(std::io::Error),
}

impl fmt::Display for JsonSchemaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JsonSchemaError::Schema(err) => write!(f, "{err}"),
            JsonSchemaError::Io(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for JsonSchemaError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            JsonSchemaError::Schema(err) => Some(err),
            JsonSchemaError::Io(err) => Some(err),
        }
    }
}

impl From<SchemaError> for JsonSchemaError {
    fn from(value: SchemaError) -> Self {
        Self::Schema(value)
    }
}

impl From<std::io::Error> for JsonSchemaError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

/// Generate one JSON Schema document per config root in `T`.
///
/// File names are derived from effective config root names:
/// `cfg` becomes `cfg.schema.json`.
pub fn generate_json_schemas<T>() -> Result<Vec<JsonSchemaFile>, JsonSchemaError>
where
    T: Facet<'static>,
{
    let schema = Schema::from_shape(T::SHAPE)?;
    Ok(generate_json_schemas_for_schema(&schema))
}

/// Write one JSON Schema file per config root in `T` to `output_dir`.
///
/// The output directory is created if it does not already exist. The returned
/// paths are the files that were written.
pub fn write_json_schemas<T>(
    output_dir: impl AsRef<FsPath>,
) -> Result<Vec<PathBuf>, JsonSchemaError>
where
    T: Facet<'static>,
{
    let files = generate_json_schemas::<T>()?;
    write_json_schema_files(output_dir, &files)
}

pub(crate) fn generate_json_schemas_for_schema(schema: &Schema) -> Vec<JsonSchemaFile> {
    schema
        .configs()
        .iter()
        .map(|config| {
            let root_name = config.field_name().unwrap_or("config");
            let json = config_root_schema(config);
            JsonSchemaFile {
                file_name: format!("{}.schema.json", sanitize_file_stem(root_name)),
                contents: json.to_pretty_string(),
            }
        })
        .collect()
}

pub(crate) fn write_json_schema_files(
    output_dir: impl AsRef<FsPath>,
    files: &[JsonSchemaFile],
) -> Result<Vec<PathBuf>, JsonSchemaError> {
    let output_dir = output_dir.as_ref();
    fs::create_dir_all(output_dir)?;

    let mut written = Vec::with_capacity(files.len());
    for file in files {
        let path = output_dir.join(&file.file_name);
        fs::write(&path, &file.contents)?;
        written.push(path);
    }

    Ok(written)
}

fn config_root_schema(config: &ConfigStructSchema) -> Json {
    let mut object = Vec::new();
    object.push((
        "$schema".to_string(),
        Json::String("https://json-schema.org/draft/2020-12/schema".to_string()),
    ));
    object.push((
        "title".to_string(),
        Json::String(config.shape().to_string()),
    ));
    if let Some(description) = description(config.docs()) {
        object.push(("description".to_string(), Json::String(description)));
    }
    append_struct_keywords(&mut object, config);
    Json::Object(object)
}

fn struct_schema(config: &ConfigStructSchema) -> Json {
    let mut object = Vec::new();
    append_struct_keywords(&mut object, config);
    Json::Object(object)
}

fn append_struct_keywords(object: &mut Vec<(String, Json)>, config: &ConfigStructSchema) {
    object.push(("type".to_string(), Json::String("object".to_string())));
    object.push(("additionalProperties".to_string(), Json::Bool(false)));

    let mut properties = Vec::new();
    let mut required = Vec::new();

    for (field_name, field) in config.fields() {
        properties.push((field_name.clone(), field_schema(field)));
        if is_required_field(field) {
            required.push(Json::String(field_name.clone()));
        }
    }

    object.push(("properties".to_string(), Json::Object(properties)));
    if !required.is_empty() {
        object.push(("required".to_string(), Json::Array(required)));
    }
}

fn field_schema(field: &ConfigFieldSchema) -> Json {
    let mut schema = value_schema(field.value());

    if let Some(description) = description(field.docs()) {
        schema.insert("description", Json::String(description));
    }
    if field.is_sensitive() {
        schema.insert("writeOnly", Json::Bool(true));
    }
    if let Some(default) = field.default().and_then(config_value_to_json) {
        schema.insert("default", default);
    }

    schema
}

fn value_schema(value: &ConfigValueSchema) -> Json {
    match value {
        ConfigValueSchema::Struct(config) => struct_schema(config),
        ConfigValueSchema::Vec(vec) => Json::object([
            ("type", Json::String("array".to_string())),
            ("items", value_schema(vec.element())),
        ]),
        ConfigValueSchema::Option { value, .. } => Json::object([(
            "anyOf",
            Json::Array(vec![
                value_schema(value),
                Json::object([("type", Json::String("null".to_string()))]),
            ]),
        )]),
        ConfigValueSchema::Enum(enum_schema) => enum_schema_json(enum_schema),
        ConfigValueSchema::Leaf(leaf) => match leaf.kind() {
            LeafKind::Scalar(scalar) => scalar_schema(scalar),
            LeafKind::Enum { variants } => Json::object([
                ("type", Json::String("string".to_string())),
                (
                    "enum",
                    Json::Array(variants.iter().map(|v| Json::String(v.clone())).collect()),
                ),
            ]),
        },
    }
}

fn scalar_schema(scalar: &ScalarType) -> Json {
    let schema_type = match scalar {
        ScalarType::Bool => "boolean",
        ScalarType::String | ScalarType::Other => "string",
        ScalarType::Integer => "integer",
        ScalarType::Float => "number",
    };
    Json::object([("type", Json::String(schema_type.to_string()))])
}

fn enum_schema_json(enum_schema: &ConfigEnumSchema) -> Json {
    let all_unit = enum_schema
        .variants()
        .values()
        .all(|variant| variant.fields().is_empty());

    if all_unit {
        return Json::object([
            ("type", Json::String("string".to_string())),
            (
                "enum",
                Json::Array(
                    enum_schema
                        .variants()
                        .keys()
                        .map(|name| Json::String(name.clone()))
                        .collect(),
                ),
            ),
        ]);
    }

    Json::object([(
        "oneOf",
        Json::Array(
            enum_schema
                .variants()
                .iter()
                .map(|(variant_name, variant)| variant_schema(variant_name, variant))
                .collect(),
        ),
    )])
}

fn variant_schema(variant_name: &str, variant: &ConfigEnumVariantSchema) -> Json {
    if variant.fields().is_empty() {
        let mut object = vec![("const".to_string(), Json::String(variant_name.to_string()))];
        if let Some(description) = description(variant.docs()) {
            object.push(("description".to_string(), Json::String(description)));
        }
        return Json::Object(object);
    }

    let fields = ConfigStructLike { variant };
    let variant_object = fields.to_json_schema();
    let mut object = vec![
        ("type".to_string(), Json::String("object".to_string())),
        ("additionalProperties".to_string(), Json::Bool(false)),
        (
            "properties".to_string(),
            Json::Object(vec![(variant_name.to_string(), variant_object)]),
        ),
        (
            "required".to_string(),
            Json::Array(vec![Json::String(variant_name.to_string())]),
        ),
    ];
    if let Some(description) = description(variant.docs()) {
        object.push(("description".to_string(), Json::String(description)));
    }
    Json::Object(object)
}

struct ConfigStructLike<'a> {
    variant: &'a ConfigEnumVariantSchema,
}

impl ConfigStructLike<'_> {
    fn to_json_schema(&self) -> Json {
        let mut properties = Vec::new();
        let mut required = Vec::new();

        for (field_name, field) in self.variant.fields() {
            properties.push((field_name.clone(), field_schema(field)));
            if is_required_field(field) {
                required.push(Json::String(field_name.clone()));
            }
        }

        let mut object = vec![
            ("type".to_string(), Json::String("object".to_string())),
            ("additionalProperties".to_string(), Json::Bool(false)),
            ("properties".to_string(), Json::Object(properties)),
        ];
        if !required.is_empty() {
            object.push(("required".to_string(), Json::Array(required)));
        }
        Json::Object(object)
    }
}

fn is_required_field(field: &ConfigFieldSchema) -> bool {
    !matches!(field.value(), ConfigValueSchema::Option { .. }) && field.default().is_none()
}

fn description(docs: &Docs) -> Option<String> {
    match (docs.summary(), docs.details()) {
        (Some(summary), Some(details)) => Some(format!("{summary}\n\n{details}")),
        (Some(summary), None) => Some(summary.to_string()),
        (None, Some(details)) => Some(details.to_string()),
        (None, None) => None,
    }
}

fn config_value_to_json(value: &ConfigValue) -> Option<Json> {
    match value {
        ConfigValue::Null(_) => Some(Json::Null),
        ConfigValue::Bool(value) => Some(Json::Bool(value.value)),
        ConfigValue::Integer(value) => Some(Json::Number(value.value.to_string())),
        ConfigValue::Float(value) => Some(Json::Number(value.value.to_string())),
        ConfigValue::String(value) => Some(Json::String(value.value.clone())),
        ConfigValue::Array(value) => Some(Json::Array(
            value
                .value
                .iter()
                .filter_map(config_value_to_json)
                .collect(),
        )),
        ConfigValue::Object(value) => Some(Json::Object(
            value
                .value
                .iter()
                .filter_map(|(key, value)| Some((key.clone(), config_value_to_json(value)?)))
                .collect(),
        )),
        ConfigValue::Enum(value) => Some(Json::Object(vec![(
            value.value.variant.clone(),
            Json::Object(
                value
                    .value
                    .fields
                    .iter()
                    .filter_map(|(key, value)| Some((key.clone(), config_value_to_json(value)?)))
                    .collect(),
            ),
        )])),
    }
}

fn sanitize_file_stem(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.') {
                ch
            } else {
                '-'
            }
        })
        .collect();

    if sanitized.is_empty() {
        "config".to_string()
    } else {
        sanitized
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Json {
    Null,
    Bool(bool),
    Number(String),
    String(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

impl Json {
    fn object<const N: usize>(entries: [(&str, Json); N]) -> Self {
        Self::Object(
            entries
                .into_iter()
                .map(|(key, value)| (key.to_string(), value))
                .collect(),
        )
    }

    fn insert(&mut self, key: &str, value: Json) {
        let Self::Object(entries) = self else {
            return;
        };
        entries.push((key.to_string(), value));
    }

    fn to_pretty_string(&self) -> String {
        let mut output = String::new();
        self.write_pretty(&mut output, 0);
        output.push('\n');
        output
    }

    fn write_pretty(&self, output: &mut String, indent: usize) {
        match self {
            Json::Null => output.push_str("null"),
            Json::Bool(value) => output.push_str(if *value { "true" } else { "false" }),
            Json::Number(value) => output.push_str(value),
            Json::String(value) => write_json_string(output, value),
            Json::Array(values) => {
                if values.is_empty() {
                    output.push_str("[]");
                    return;
                }

                output.push('[');
                output.push('\n');
                for (index, value) in values.iter().enumerate() {
                    write_indent(output, indent + 2);
                    value.write_pretty(output, indent + 2);
                    if index + 1 != values.len() {
                        output.push(',');
                    }
                    output.push('\n');
                }
                write_indent(output, indent);
                output.push(']');
            }
            Json::Object(entries) => {
                if entries.is_empty() {
                    output.push_str("{}");
                    return;
                }

                output.push('{');
                output.push('\n');
                for (index, (key, value)) in entries.iter().enumerate() {
                    write_indent(output, indent + 2);
                    write_json_string(output, key);
                    output.push_str(": ");
                    value.write_pretty(output, indent + 2);
                    if index + 1 != entries.len() {
                        output.push(',');
                    }
                    output.push('\n');
                }
                write_indent(output, indent);
                output.push('}');
            }
        }
    }
}

fn write_indent(output: &mut String, indent: usize) {
    for _ in 0..indent {
        output.push(' ');
    }
}

fn write_json_string(output: &mut String, value: &str) {
    output.push('"');
    for ch in value.chars() {
        match ch {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            '\u{08}' => output.push_str("\\b"),
            '\u{0c}' => output.push_str("\\f"),
            ch if ch.is_control() => output.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => output.push(ch),
        }
    }
    output.push('"');
}
