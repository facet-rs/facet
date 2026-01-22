//! Type mapping from Styx schemas to Go types.

use crate::error::GenError;
use facet_styx::Schema;
use std::collections::HashMap;

/// A Go type representation.
#[derive(Debug, Clone)]
pub enum GoType {
    /// A struct type
    Struct {
        fields: Vec<StructField>,
        doc: Option<String>,
    },
    /// An enum type (represented as string constants)
    Enum {
        variants: Vec<EnumVariant>,
        doc: Option<String>,
    },
    /// A primitive or built-in type
    #[allow(dead_code)]
    Primitive(String),
}

/// A field in a Go struct.
#[derive(Debug, Clone)]
pub struct StructField {
    /// Go field name (PascalCase)
    pub go_name: String,
    /// JSON field name (snake_case or original)
    pub json_name: String,
    /// Styx field name (original)
    pub styx_name: String,
    /// Go type name
    pub type_name: String,
    /// Whether the field is optional
    pub optional: bool,
    /// Field documentation
    pub doc: Option<String>,
    /// Validation constraints
    pub constraints: Option<FieldConstraints>,
}

/// Validation constraints for a field.
#[derive(Debug, Clone, Default)]
pub struct FieldConstraints {
    pub min_length: Option<usize>,
    pub max_length: Option<usize>,
    pub min_int: Option<i64>,
    pub max_int: Option<i64>,
    pub min_float: Option<f64>,
    pub max_float: Option<f64>,
}

/// An enum variant.
#[derive(Debug, Clone)]
pub struct EnumVariant {
    /// Variant name
    pub name: String,
    /// Variant documentation
    pub doc: Option<String>,
}

/// Maps Styx schema types to Go types.
pub struct TypeMapper {
    types: HashMap<String, GoType>,
}

impl TypeMapper {
    pub fn new() -> Self {
        Self {
            types: HashMap::new(),
        }
    }

    /// Register a named type from the schema.
    pub fn register_type(&mut self, name: &str, schema: &Schema) -> Result<(), GenError> {
        let go_type = self.map_schema_type(Some(name), schema)?;
        self.types.insert(name.to_string(), go_type);
        Ok(())
    }

    /// Get all registered types.
    pub fn types(&self) -> &HashMap<String, GoType> {
        &self.types
    }

    /// Map a schema type to a Go type, registering nested types as needed.
    fn map_schema_type(
        &mut self,
        parent_name: Option<&str>,
        schema: &Schema,
    ) -> Result<GoType, GenError> {
        match schema {
            Schema::String(_) => Ok(GoType::Primitive("string".to_string())),
            Schema::Int(_) => Ok(GoType::Primitive("int64".to_string())),
            Schema::Float(_) => Ok(GoType::Primitive("float64".to_string())),
            Schema::Bool => Ok(GoType::Primitive("bool".to_string())),
            Schema::Unit => Ok(GoType::Primitive("struct{}".to_string())),
            Schema::Any => Ok(GoType::Primitive("interface{}".to_string())),
            Schema::Object(obj_schema) => {
                let mut struct_fields = Vec::new();
                for (documented_key, field_schema) in &obj_schema.0 {
                    let key = documented_key.value();
                    if let Some(field_name) = &key.value {
                        let field_doc = documented_key.doc.as_ref().map(|lines| {
                            lines
                                .iter()
                                .map(|line| line.strip_prefix(' ').unwrap_or(line))
                                .collect::<Vec<_>>()
                                .join(" ")
                        });
                        let field =
                            self.map_field(parent_name, field_name, field_schema, field_doc)?;
                        struct_fields.push(field);
                    }
                }
                Ok(GoType::Struct {
                    fields: struct_fields,
                    doc: None,
                })
            }
            Schema::Seq(seq_schema) => {
                let item_schema = &seq_schema.0.0;
                let item_type = self.type_name(parent_name, item_schema.value())?;
                Ok(GoType::Primitive(format!("[]{}", item_type)))
            }
            Schema::Tuple(tuple_schema) => {
                if tuple_schema.0.is_empty() {
                    Ok(GoType::Primitive("[]interface{}".to_string()))
                } else {
                    let first_type = self.type_name(parent_name, tuple_schema.0[0].value())?;
                    Ok(GoType::Primitive(format!("[]{}", first_type)))
                }
            }
            Schema::Map(map_schema) => {
                let schemas = &map_schema.0;
                let key_type = if schemas.is_empty() {
                    "string".to_string()
                } else {
                    self.type_name(parent_name, schemas[0].value())?
                };
                let value_type = if schemas.len() < 2 {
                    "interface{}".to_string()
                } else {
                    self.type_name(parent_name, schemas[1].value())?
                };
                Ok(GoType::Primitive(format!(
                    "map[{}]{}",
                    key_type, value_type
                )))
            }
            Schema::Enum(enum_schema) => {
                let enum_variants = enum_schema
                    .0
                    .keys()
                    .map(|documented_name| EnumVariant {
                        name: documented_name.value().clone(),
                        doc: documented_name.doc().map(|lines| {
                            lines
                                .iter()
                                .map(|line| line.strip_prefix(' ').unwrap_or(line))
                                .collect::<Vec<_>>()
                                .join("\n")
                        }),
                    })
                    .collect();
                Ok(GoType::Enum {
                    variants: enum_variants,
                    doc: None,
                })
            }
            Schema::Optional(opt_schema) => {
                let inner = opt_schema.0.0.value();
                let inner_type = self.type_name(parent_name, inner)?;
                Ok(GoType::Primitive(format!("*{}", inner_type)))
            }
            Schema::Default(default_schema) => {
                let inner = default_schema.0.1.value();
                self.map_schema_type(parent_name, inner)
            }
            Schema::Union(_) | Schema::OneOf(_) => Ok(GoType::Primitive("interface{}".to_string())),
            Schema::Flatten(flatten_schema) => {
                let inner = flatten_schema.0.0.value();
                self.map_schema_type(parent_name, inner)
            }
            Schema::Deprecated(depr_schema) => {
                let inner = depr_schema.0.1.value();
                self.map_schema_type(parent_name, inner)
            }
            Schema::Literal(_) => Ok(GoType::Primitive("string".to_string())),
            Schema::Type { name } => {
                if let Some(n) = name {
                    Ok(GoType::Primitive(to_pascal_case(n)))
                } else {
                    Ok(GoType::Primitive("interface{}".to_string()))
                }
            }
        }
    }

    /// Map a field to a struct field, registering nested types as needed.
    fn map_field(
        &mut self,
        parent_name: Option<&str>,
        field_name: &str,
        schema: &Schema,
        doc: Option<String>,
    ) -> Result<StructField, GenError> {
        // Unwrap Default and Optional wrappers to find the core type
        let mut current = schema;
        let mut is_optional = false;

        loop {
            match current {
                Schema::Default(default_schema) => {
                    current = default_schema.0.1.value();
                }
                Schema::Optional(opt_schema) => {
                    is_optional = true;
                    current = opt_schema.0.0.value();
                }
                _ => break,
            }
        }

        let inner_schema = current;

        // Check if this is a nested object - if so, generate a named type for it
        let type_name = if matches!(inner_schema, Schema::Object(_)) {
            // Generate a type name for the nested object
            let nested_type_name = if let Some(parent) = parent_name {
                format!("{}{}", parent, to_pascal_case(field_name))
            } else {
                to_pascal_case(field_name)
            };

            // Register the nested type if it doesn't exist yet
            if !self.types.contains_key(&nested_type_name) {
                let nested_type = self.map_schema_type(Some(&nested_type_name), inner_schema)?;
                self.types.insert(nested_type_name.clone(), nested_type);
            }

            nested_type_name
        } else {
            self.type_name(parent_name, inner_schema)?
        };

        let type_name = if is_optional
            && !type_name.starts_with('*')
            && !type_name.starts_with('[')
            && !type_name.starts_with("map[")
        {
            format!("*{}", type_name)
        } else {
            type_name
        };

        let constraints = self.extract_constraints(inner_schema);

        Ok(StructField {
            go_name: to_pascal_case(field_name),
            json_name: field_name.to_string(),
            styx_name: field_name.to_string(),
            type_name,
            optional: is_optional,
            doc,
            constraints,
        })
    }

    /// Extract constraints from a schema type.
    fn extract_constraints(&self, schema: &Schema) -> Option<FieldConstraints> {
        match schema {
            Schema::String(constraints_opt) => constraints_opt.as_ref().map(|c| FieldConstraints {
                min_length: c.min_len,
                max_length: c.max_len,
                ..Default::default()
            }),
            Schema::Int(constraints_opt) => constraints_opt.as_ref().map(|c| FieldConstraints {
                min_int: c.min.map(|v| v as i64),
                max_int: c.max.map(|v| v as i64),
                ..Default::default()
            }),
            Schema::Float(constraints_opt) => constraints_opt.as_ref().map(|c| FieldConstraints {
                min_float: c.min,
                max_float: c.max,
                ..Default::default()
            }),
            _ => None,
        }
    }

    /// Get the Go type name for a schema type (non-nested objects return generic map).
    fn type_name(&self, _parent_name: Option<&str>, schema: &Schema) -> Result<String, GenError> {
        match schema {
            Schema::String(_) => Ok("string".to_string()),
            Schema::Int(_) => Ok("int64".to_string()),
            Schema::Float(_) => Ok("float64".to_string()),
            Schema::Bool => Ok("bool".to_string()),
            Schema::Unit => Ok("struct{}".to_string()),
            Schema::Any => Ok("interface{}".to_string()),
            Schema::Object(_) => Ok("map[string]interface{}".to_string()),
            Schema::Seq(seq_schema) => {
                let item_type = self.type_name(_parent_name, seq_schema.0.0.value())?;
                Ok(format!("[]{}", item_type))
            }
            Schema::Tuple(tuple_schema) => {
                if tuple_schema.0.is_empty() {
                    Ok("[]interface{}".to_string())
                } else {
                    let first_type = self.type_name(_parent_name, tuple_schema.0[0].value())?;
                    Ok(format!("[]{}", first_type))
                }
            }
            Schema::Map(map_schema) => {
                let schemas = &map_schema.0;
                let key_type = if schemas.is_empty() {
                    "string".to_string()
                } else {
                    self.type_name(_parent_name, schemas[0].value())?
                };
                let value_type = if schemas.len() < 2 {
                    "interface{}".to_string()
                } else {
                    self.type_name(_parent_name, schemas[1].value())?
                };
                Ok(format!("map[{}]{}", key_type, value_type))
            }
            Schema::Enum(_) => Ok("string".to_string()),
            Schema::Optional(opt_schema) => {
                let inner_type = self.type_name(_parent_name, opt_schema.0.0.value())?;
                if inner_type.starts_with('*')
                    || inner_type.starts_with('[')
                    || inner_type.starts_with("map[")
                {
                    Ok(inner_type)
                } else {
                    Ok(format!("*{}", inner_type))
                }
            }
            Schema::Default(default_schema) => {
                self.type_name(_parent_name, default_schema.0.1.value())
            }
            Schema::Union(_) | Schema::OneOf(_) => Ok("interface{}".to_string()),
            Schema::Flatten(flatten_schema) => {
                self.type_name(_parent_name, flatten_schema.0.0.value())
            }
            Schema::Deprecated(depr_schema) => {
                self.type_name(_parent_name, depr_schema.0.1.value())
            }
            Schema::Literal(_) => Ok("string".to_string()),
            Schema::Type { name } => {
                if let Some(n) = name {
                    Ok(to_pascal_case(n))
                } else {
                    Ok("interface{}".to_string())
                }
            }
        }
    }
}

fn to_pascal_case(s: &str) -> String {
    s.split(['_', '-'])
        .filter(|s| !s.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().chain(chars).collect(),
            }
        })
        .collect()
}
