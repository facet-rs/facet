//! Generate JSON Schema from facet type metadata.
//!
//! This crate uses facet's reflection capabilities to generate JSON Schema definitions
//! from any type that implements `Facet`.
//!
//! # Example
//!
//! ```
//! use facet::Facet;
//! use facet_json_schema::to_schema;
//!
//! #[derive(Facet)]
//! struct User {
//!     name: String,
//!     age: u32,
//!     email: Option<String>,
//! }
//!
//! let schema = to_schema::<User>();
//! println!("{}", schema);
//! ```

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use facet::Facet;
use facet_core::{Def, Field, Shape, StructKind, Type, UserType};

/// A JSON Schema definition.
///
/// This is a simplified representation that covers the most common cases.
/// It can be serialized to JSON using facet-json.
#[derive(Debug, Clone, Facet)]
#[facet(skip_all_unless_truthy)]
pub struct JsonSchema {
    /// The JSON Schema dialect
    #[facet(rename = "$schema")]
    pub schema: Option<String>,

    /// Reference to another schema definition
    #[facet(rename = "$ref")]
    pub ref_: Option<String>,

    /// Schema definitions for reuse
    #[facet(rename = "$defs")]
    pub defs: Option<BTreeMap<String, JsonSchema>>,

    /// The type of the schema
    #[facet(rename = "type")]
    pub type_: Option<SchemaType>,

    /// For objects: the properties
    pub properties: Option<BTreeMap<String, JsonSchema>>,

    /// For objects: required property names
    pub required: Option<Vec<String>>,

    /// For objects: additional properties schema or false
    #[facet(rename = "additionalProperties")]
    pub additional_properties: Option<AdditionalProperties>,

    /// For arrays: the items schema
    pub items: Option<Box<JsonSchema>>,

    /// For strings: enumerated values
    #[facet(rename = "enum")]
    pub enum_: Option<Vec<String>>,

    /// For numbers: minimum value
    pub minimum: Option<i128>,

    /// For numbers: maximum value
    pub maximum: Option<u128>,

    /// For oneOf/anyOf/allOf
    #[facet(rename = "oneOf")]
    pub one_of: Option<Vec<JsonSchema>>,

    #[facet(rename = "anyOf")]
    pub any_of: Option<Vec<JsonSchema>>,

    #[facet(rename = "allOf")]
    pub all_of: Option<Vec<JsonSchema>>,

    /// Description from doc comments
    pub description: Option<String>,

    /// Title (type name)
    pub title: Option<String>,

    /// Constant value
    #[facet(rename = "const")]
    pub const_: Option<String>,
}

/// JSON Schema type
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "lowercase")]
#[repr(u8)]
pub enum SchemaType {
    String,
    Number,
    Integer,
    Boolean,
    Array,
    Object,
    Null,
}

/// Additional properties can be a boolean or a schema
#[derive(Debug, Clone, Facet)]
#[facet(untagged)]
#[repr(u8)]
pub enum AdditionalProperties {
    Bool(bool),
    Schema(Box<JsonSchema>),
}

impl Default for JsonSchema {
    fn default() -> Self {
        Self::new()
    }
}

impl JsonSchema {
    /// Create an empty schema
    pub const fn new() -> Self {
        Self {
            schema: None,
            ref_: None,
            defs: None,
            type_: None,
            properties: None,
            required: None,
            additional_properties: None,
            items: None,
            enum_: None,
            minimum: None,
            maximum: None,
            one_of: None,
            any_of: None,
            all_of: None,
            description: None,
            title: None,
            const_: None,
        }
    }

    /// Create a schema with a $schema dialect
    pub fn with_dialect(dialect: &str) -> Self {
        Self {
            schema: Some(dialect.into()),
            ..Self::new()
        }
    }

    /// Create a reference to another schema
    pub fn reference(ref_path: &str) -> Self {
        Self {
            ref_: Some(ref_path.into()),
            ..Self::new()
        }
    }
}

/// Generate a JSON Schema from a facet type.
///
/// This returns a `JsonSchema` struct that can be serialized to JSON.
pub fn schema_for<T: Facet<'static>>() -> JsonSchema {
    let mut ctx = SchemaContext::new();
    let schema = ctx.schema_for_shape(T::SHAPE);

    // If we collected any definitions, add them to the root
    if ctx.defs.is_empty() {
        schema
    } else {
        JsonSchema {
            schema: Some("https://json-schema.org/draft/2020-12/schema".into()),
            defs: Some(ctx.defs),
            ..schema
        }
    }
}

/// Generate a JSON Schema string from a facet type.
pub fn to_schema<T: Facet<'static>>() -> String {
    let schema = schema_for::<T>();
    facet_json::to_string_pretty(&schema).expect("JSON Schema serialization should not fail")
}

/// Context for schema generation, tracking definitions to avoid cycles.
struct SchemaContext {
    /// Collected schema definitions
    defs: BTreeMap<String, JsonSchema>,
    /// Types currently being processed (for cycle detection)
    in_progress: Vec<&'static str>,
}

impl SchemaContext {
    const fn new() -> Self {
        Self {
            defs: BTreeMap::new(),
            in_progress: Vec::new(),
        }
    }

    fn schema_for_shape(&mut self, shape: &'static Shape) -> JsonSchema {
        // Check for cycles - if we're already processing this type, emit a $ref
        let type_name = shape.type_identifier;
        if self.in_progress.contains(&type_name) {
            return JsonSchema::reference(&format!("#/$defs/{}", type_name));
        }

        // Build description from doc comments
        let description = if shape.doc.is_empty() {
            None
        } else {
            Some(shape.doc.join("\n").trim().to_string())
        };

        // Handle the type based on its definition
        // NOTE: We check Def BEFORE shape.inner because types like Vec<T> set
        // .inner() for type parameter propagation but should still be treated
        // as List, not as transparent wrappers.
        match &shape.def {
            Def::Scalar => self.schema_for_scalar(shape, description),
            Def::Option(opt) => {
                // Option<T> becomes anyOf: [schema(T), {type: "null"}]
                let inner_schema = self.schema_for_shape(opt.t);
                JsonSchema {
                    any_of: Some(vec![
                        inner_schema,
                        JsonSchema {
                            type_: Some(SchemaType::Null),
                            ..JsonSchema::new()
                        },
                    ]),
                    description,
                    ..JsonSchema::new()
                }
            }
            Def::List(list) => JsonSchema {
                type_: Some(SchemaType::Array),
                items: Some(Box::new(self.schema_for_shape(list.t))),
                description,
                ..JsonSchema::new()
            },
            Def::Array(arr) => JsonSchema {
                type_: Some(SchemaType::Array),
                items: Some(Box::new(self.schema_for_shape(arr.t))),
                description,
                ..JsonSchema::new()
            },
            Def::Set(set) => JsonSchema {
                type_: Some(SchemaType::Array),
                items: Some(Box::new(self.schema_for_shape(set.t))),
                description,
                ..JsonSchema::new()
            },
            Def::Map(map) => {
                // Maps become objects with additionalProperties
                JsonSchema {
                    type_: Some(SchemaType::Object),
                    additional_properties: Some(AdditionalProperties::Schema(Box::new(
                        self.schema_for_shape(map.v),
                    ))),
                    description,
                    ..JsonSchema::new()
                }
            }
            Def::Undefined => {
                // Check if it's a struct or enum via Type
                match &shape.ty {
                    Type::User(UserType::Struct(st)) => {
                        self.schema_for_struct(shape, st.fields, st.kind, description)
                    }
                    Type::User(UserType::Enum(en)) => self.schema_for_enum(shape, en, description),
                    _ => {
                        // For other undefined types, check if it's a transparent wrapper
                        if let Some(inner) = shape.inner {
                            self.schema_for_shape(inner)
                        } else {
                            JsonSchema {
                                description,
                                ..JsonSchema::new()
                            }
                        }
                    }
                }
            }
            _ => {
                // For other defs, check if it's a transparent wrapper
                if let Some(inner) = shape.inner {
                    self.schema_for_shape(inner)
                } else {
                    JsonSchema {
                        description,
                        ..JsonSchema::new()
                    }
                }
            }
        }
    }

    fn schema_for_scalar(
        &mut self,
        shape: &'static Shape,
        description: Option<String>,
    ) -> JsonSchema {
        let type_name = shape.type_identifier;

        // Map common Rust types to JSON Schema types
        let (type_, minimum, maximum) = match type_name {
            // Strings
            "String" | "str" | "&str" | "Cow" => (Some(SchemaType::String), None, None),

            // Booleans
            "bool" => (Some(SchemaType::Boolean), None, None),

            // Unsigned integers
            "u8" => (Some(SchemaType::Integer), Some(0), Some(u8::MAX as u128)),
            "u16" => (Some(SchemaType::Integer), Some(0), Some(u16::MAX as u128)),
            "u32" => (Some(SchemaType::Integer), Some(0), Some(u32::MAX as u128)),
            "u64" => (Some(SchemaType::Integer), Some(0), Some(u64::MAX as u128)),
            "u128" => (Some(SchemaType::Integer), Some(0), Some(u128::MAX)),
            "usize" => (Some(SchemaType::Integer), Some(0), Some(u64::MAX as u128)),

            // Signed integers
            "i8" => (
                Some(SchemaType::Integer),
                Some(i8::MIN as i128),
                Some(i8::MAX as u128),
            ),
            "i16" => (
                Some(SchemaType::Integer),
                Some(i16::MIN as i128),
                Some(i16::MAX as u128),
            ),
            "i32" => (
                Some(SchemaType::Integer),
                Some(i32::MIN as i128),
                Some(i32::MAX as u128),
            ),
            "i64" => (
                Some(SchemaType::Integer),
                Some(i64::MIN as i128),
                Some(i64::MAX as u128),
            ),
            "i128" => (
                Some(SchemaType::Integer),
                Some(i128::MIN),
                Some(i128::MAX as u128),
            ),
            "isize" => (
                Some(SchemaType::Integer),
                Some(i64::MIN as i128),
                Some(i64::MAX as u128),
            ),

            // Floats
            "f32" | "f64" => (Some(SchemaType::Number), None, None),

            // Char as string
            "char" => (Some(SchemaType::String), None, None),

            // Unknown scalar - no type constraint
            _ => (None, None, None),
        };

        JsonSchema {
            type_,
            minimum,
            maximum,
            description,
            ..JsonSchema::new()
        }
    }

    fn schema_for_struct(
        &mut self,
        shape: &'static Shape,
        fields: &'static [Field],
        kind: StructKind,
        description: Option<String>,
    ) -> JsonSchema {
        match kind {
            StructKind::Unit => {
                // Unit struct serializes as null or empty object
                JsonSchema {
                    type_: Some(SchemaType::Null),
                    description,
                    ..JsonSchema::new()
                }
            }
            StructKind::TupleStruct if fields.len() == 1 => {
                // Newtype - serialize as the inner type
                self.schema_for_shape(fields[0].shape.get())
            }
            StructKind::TupleStruct | StructKind::Tuple => {
                // Tuple struct as array - collect items for prefixItems
                let _items: Vec<JsonSchema> = fields
                    .iter()
                    .map(|f| self.schema_for_shape(f.shape.get()))
                    .collect();

                // TODO: Use prefixItems for proper tuple schema (JSON Schema 2020-12)
                JsonSchema {
                    type_: Some(SchemaType::Array),
                    description,
                    ..JsonSchema::new()
                }
            }
            StructKind::Struct => {
                // Mark as in progress for cycle detection
                self.in_progress.push(shape.type_identifier);

                let mut properties = BTreeMap::new();
                let mut required = Vec::new();

                for field in fields {
                    // Skip fields marked with skip
                    if field.flags.contains(facet_core::FieldFlags::SKIP) {
                        continue;
                    }

                    let field_name = field.effective_name();
                    let field_schema = self.schema_for_shape(field.shape.get());

                    // Check if field is required (not Option and no default)
                    let is_option = matches!(field.shape.get().def, Def::Option(_));
                    let has_default = field.default.is_some();

                    if !is_option && !has_default {
                        required.push(field_name.to_string());
                    }

                    properties.insert(field_name.to_string(), field_schema);
                }

                self.in_progress.pop();

                JsonSchema {
                    type_: Some(SchemaType::Object),
                    properties: Some(properties),
                    required: if required.is_empty() {
                        None
                    } else {
                        Some(required)
                    },
                    additional_properties: Some(AdditionalProperties::Bool(false)),
                    description,
                    title: Some(shape.type_identifier.to_string()),
                    ..JsonSchema::new()
                }
            }
        }
    }

    fn schema_for_enum(
        &mut self,
        shape: &'static Shape,
        enum_type: &facet_core::EnumType,
        description: Option<String>,
    ) -> JsonSchema {
        // Check if all variants are unit variants (simple string enum)
        let all_unit = enum_type
            .variants
            .iter()
            .all(|v| matches!(v.data.kind, StructKind::Unit));

        if all_unit {
            // Simple string enum
            let values: Vec<String> = enum_type
                .variants
                .iter()
                .map(|v| v.effective_name().to_string())
                .collect();

            JsonSchema {
                type_: Some(SchemaType::String),
                enum_: Some(values),
                description,
                title: Some(shape.type_identifier.to_string()),
                ..JsonSchema::new()
            }
        } else {
            // Complex enum - use oneOf with discriminator
            // This handles internally tagged, externally tagged, adjacently tagged, and untagged
            let variants: Vec<JsonSchema> = enum_type
                .variants
                .iter()
                .map(|v| {
                    let variant_name = v.effective_name().to_string();
                    match v.data.kind {
                        StructKind::Unit => {
                            // Unit variant: { "type": "VariantName" } or just "VariantName"
                            JsonSchema {
                                const_: Some(variant_name),
                                ..JsonSchema::new()
                            }
                        }
                        StructKind::TupleStruct if v.data.fields.len() == 1 => {
                            // Newtype variant: { "VariantName": <inner> }
                            let mut props = BTreeMap::new();
                            props.insert(
                                variant_name.clone(),
                                self.schema_for_shape(v.data.fields[0].shape.get()),
                            );
                            JsonSchema {
                                type_: Some(SchemaType::Object),
                                properties: Some(props),
                                required: Some(vec![variant_name]),
                                additional_properties: Some(AdditionalProperties::Bool(false)),
                                ..JsonSchema::new()
                            }
                        }
                        _ => {
                            // Struct variant: { "VariantName": { ...fields } }
                            let inner =
                                self.schema_for_struct(shape, v.data.fields, v.data.kind, None);
                            let mut props = BTreeMap::new();
                            props.insert(variant_name.clone(), inner);
                            JsonSchema {
                                type_: Some(SchemaType::Object),
                                properties: Some(props),
                                required: Some(vec![variant_name]),
                                additional_properties: Some(AdditionalProperties::Bool(false)),
                                ..JsonSchema::new()
                            }
                        }
                    }
                })
                .collect();

            JsonSchema {
                one_of: Some(variants),
                description,
                title: Some(shape.type_identifier.to_string()),
                ..JsonSchema::new()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_struct() {
        #[derive(Facet)]
        struct User {
            name: String,
            age: u32,
        }

        let schema = to_schema::<User>();
        insta::assert_snapshot!(schema);
    }

    #[test]
    fn test_optional_field() {
        #[derive(Facet)]
        struct Config {
            required: String,
            optional: Option<String>,
        }

        let schema = to_schema::<Config>();
        insta::assert_snapshot!(schema);
    }

    #[test]
    fn test_simple_enum() {
        #[derive(Facet)]
        #[repr(u8)]
        enum Status {
            Active,
            Inactive,
            Pending,
        }

        let schema = to_schema::<Status>();
        insta::assert_snapshot!(schema);
    }

    #[test]
    fn test_vec() {
        #[derive(Facet)]
        struct Data {
            items: Vec<String>,
        }

        let schema = to_schema::<Data>();
        insta::assert_snapshot!(schema);
    }

    #[test]
    fn test_enum_rename_all_snake_case() {
        #[derive(Facet)]
        #[facet(rename_all = "snake_case")]
        #[repr(u8)]
        enum ValidationErrorCode {
            CircularDependency,
            InvalidNaming,
            UnknownRequirement,
        }

        let schema = to_schema::<ValidationErrorCode>();
        insta::assert_snapshot!(schema);
    }

    #[test]
    fn test_struct_rename_all_camel_case() {
        #[derive(Facet)]
        #[facet(rename_all = "camelCase")]
        struct ApiResponse {
            user_name: String,
            created_at: String,
            is_active: bool,
        }

        let schema = to_schema::<ApiResponse>();
        insta::assert_snapshot!(schema);
    }

    #[test]
    fn test_enum_with_data_rename_all() {
        #[allow(dead_code)]
        #[derive(Facet)]
        #[facet(rename_all = "snake_case")]
        #[repr(C)]
        enum Message {
            TextMessage { content: String },
            ImageUpload { url: String, width: u32 },
        }

        let schema = to_schema::<Message>();
        insta::assert_snapshot!(schema);
    }
}
