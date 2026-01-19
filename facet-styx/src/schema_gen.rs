//! Schema generation from Facet types.
//!
//! This module provides utilities for generating Styx schemas from Rust types
//! that implement `Facet`.

use facet_core::{
    Def, DefaultSource, Field, NumericType, PrimitiveType, PtrConst, PtrMut, PtrUninit, Shape,
    ShapeLayout, Type, UserType,
};
use facet_reflect::Peek;
use std::collections::{HashMap, HashSet};
use std::marker::PhantomData;
use std::path::Path;
use std::ptr::NonNull;

use crate::peek_to_string_expr;
use crate::schema_types::{
    DefaultSchema, EnumSchema, MapSchema, Meta, ObjectSchema, OptionalSchema, Schema, SchemaFile,
    SeqSchema,
};

/// Try to get the default value for a field as a styx expression string.
/// Returns None if the field has no default or if serialization fails.
fn field_default_value(field: &Field) -> Option<String> {
    let default_source = field.default?;
    let shape = field.shape();

    // Get layout
    let layout = match shape.layout {
        ShapeLayout::Sized(l) => l,
        ShapeLayout::Unsized => return None,
    };

    if layout.size() == 0 {
        // Zero-sized type - return unit
        return Some("@".to_string());
    }

    // Allocate memory for the value
    let ptr = unsafe { std::alloc::alloc(layout) };
    if ptr.is_null() {
        return None;
    }
    let ptr = unsafe { NonNull::new_unchecked(ptr) };

    // Initialize with the default value
    let ptr_uninit = PtrUninit::new(ptr.as_ptr());
    match default_source {
        DefaultSource::Custom(default_fn) => {
            unsafe { default_fn(ptr_uninit) };
        }
        DefaultSource::FromTrait => {
            let ptr_mut = unsafe { ptr_uninit.assume_init() };
            if unsafe { shape.call_default_in_place(ptr_mut) }.is_none() {
                unsafe { std::alloc::dealloc(ptr.as_ptr(), layout) };
                return None;
            }
        }
    }

    // Create a Peek to serialize
    let ptr_const = PtrConst::new(ptr.as_ptr());
    let peek = unsafe { Peek::unchecked_new(ptr_const, shape) };

    // Serialize to styx expression string (with braces for objects)
    let styx_str = peek_to_string_expr(peek).ok()?;

    // Drop the value and free memory
    unsafe {
        shape.call_drop_in_place(PtrMut::new(ptr.as_ptr()));
        std::alloc::dealloc(ptr.as_ptr(), layout);
    }

    Some(styx_str)
}

/// Builder for generating Styx schemas from Facet types.
///
/// Use in build scripts to generate schema files:
///
/// ```rust,ignore
/// // build.rs
/// fn main() {
///     facet_styx::GenerateSchema::<MyConfig>::new()
///         .crate_name("myapp-config")
///         .version("1")
///         .cli("myapp")
///         .write("schema.styx");
/// }
/// ```
///
/// The generated schema can then be embedded:
///
/// ```rust,ignore
/// // src/main.rs
/// styx_embed::embed_outdir_file!("schema.styx");
/// ```
pub struct GenerateSchema<T: facet_core::Facet<'static>> {
    crate_name: Option<String>,
    version: Option<String>,
    cli: Option<String>,
    _marker: PhantomData<T>,
}

impl<T: facet_core::Facet<'static>> GenerateSchema<T> {
    /// Create a new schema generator.
    pub fn new() -> Self {
        Self {
            crate_name: None,
            version: None,
            cli: None,
            _marker: PhantomData,
        }
    }

    /// Set the crate name for the schema ID.
    pub fn crate_name(mut self, name: impl Into<String>) -> Self {
        self.crate_name = Some(name.into());
        self
    }

    /// Set the version for the schema ID.
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    /// Set the CLI binary name.
    pub fn cli(mut self, cli: impl Into<String>) -> Self {
        self.cli = Some(cli.into());
        self
    }

    /// Write the schema to `$OUT_DIR/{filename}`.
    pub fn write(self, filename: &str) {
        let out_dir =
            std::env::var("OUT_DIR").expect("OUT_DIR not set - are you in a build script?");
        let path = Path::new(&out_dir).join(filename);

        let schema = self.generate();
        std::fs::write(&path, schema).expect("failed to write schema");
    }

    /// Generate the schema as a string.
    pub fn generate(self) -> String {
        let crate_name = self
            .crate_name
            .expect("crate_name is required - call .crate_name(\"...\")");
        let version = self
            .version
            .expect("version is required - call .version(\"...\")");

        let id = format!("crate:{crate_name}@{version}");
        let shape = T::SHAPE;

        let mut generator = SchemaGenerator::new();
        let root_schema = generator.shape_to_schema(shape);

        let description = if shape.doc.is_empty() {
            None
        } else {
            Some(
                shape
                    .doc
                    .iter()
                    .map(|s| s.trim())
                    .collect::<Vec<_>>()
                    .join(" "),
            )
        };

        let schema_file = SchemaFile {
            meta: Meta {
                id,
                version: None,
                cli: self.cli,
                description,
            },
            imports: None,
            schema: {
                let mut map = HashMap::new();
                map.insert(None, root_schema);
                map
            },
        };

        crate::to_string(&schema_file).expect("failed to serialize schema")
    }
}

impl<T: facet_core::Facet<'static>> Default for GenerateSchema<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Generate a Styx schema string from a Facet type.
pub fn schema_from_type<T: facet_core::Facet<'static>>() -> String {
    let shape = T::SHAPE;
    let id = shape.type_identifier;

    let mut generator = SchemaGenerator::new();
    let root_schema = generator.shape_to_schema(shape);

    let description = if shape.doc.is_empty() {
        None
    } else {
        Some(
            shape
                .doc
                .iter()
                .map(|s| s.trim())
                .collect::<Vec<_>>()
                .join(" "),
        )
    };

    let schema_file = SchemaFile {
        meta: Meta {
            id: id.to_string(),
            version: None,
            cli: None,
            description,
        },
        imports: None,
        schema: {
            let mut map = HashMap::new();
            map.insert(None, root_schema);
            map
        },
    };

    crate::to_string(&schema_file).expect("failed to serialize schema")
}

/// Internal schema generator that builds typed Schema structs.
struct SchemaGenerator {
    /// Types currently being generated (for cycle detection)
    generating: HashSet<&'static str>,
}

impl SchemaGenerator {
    fn new() -> Self {
        Self {
            generating: HashSet::new(),
        }
    }

    /// Convert a shape to a Schema.
    fn shape_to_schema(&mut self, shape: &'static Shape) -> Schema {
        match &shape.def {
            Def::Scalar => self.scalar_to_schema(shape),
            Def::Option(opt_def) => {
                let inner = self.shape_to_schema(opt_def.t);
                Schema::Optional(OptionalSchema((Box::new(inner),)))
            }
            Def::List(list_def) => {
                let inner = self.shape_to_schema(list_def.t);
                Schema::Seq(SeqSchema((Box::new(inner),)))
            }
            Def::Array(array_def) => {
                let inner = self.shape_to_schema(array_def.t);
                Schema::Seq(SeqSchema((Box::new(inner),)))
            }
            Def::Map(map_def) => {
                let key = self.shape_to_schema(map_def.k);
                let value = self.shape_to_schema(map_def.v);
                Schema::Map(MapSchema(vec![key, value]))
            }
            Def::Set(set_def) => {
                let inner = self.shape_to_schema(set_def.t);
                Schema::Seq(SeqSchema((Box::new(inner),)))
            }
            Def::Result(result_def) => {
                let ok = self.shape_to_schema(result_def.t);
                let err = self.shape_to_schema(result_def.e);
                let mut variants = HashMap::new();
                variants.insert("ok".to_string(), ok);
                variants.insert("err".to_string(), err);
                Schema::Enum(EnumSchema(variants))
            }
            Def::Pointer(ptr_def) => {
                if let Some(pointee) = ptr_def.pointee {
                    self.shape_to_schema(pointee)
                } else {
                    Schema::Any
                }
            }
            Def::Slice(slice_def) => {
                let inner = self.shape_to_schema(slice_def.t);
                Schema::Seq(SeqSchema((Box::new(inner),)))
            }
            Def::Undefined | Def::NdArray(_) | Def::DynamicValue(_) => self.type_to_schema(shape),
            _ => self.type_to_schema(shape),
        }
    }

    fn type_to_schema(&mut self, shape: &'static Shape) -> Schema {
        match &shape.ty {
            Type::Primitive(prim) => self.primitive_to_schema(prim),
            Type::User(user) => self.user_type_to_schema(user, shape),
            Type::Sequence(seq) => {
                use facet_core::SequenceType;
                match seq {
                    SequenceType::Array(arr) => {
                        let inner = self.shape_to_schema(arr.t);
                        Schema::Seq(SeqSchema((Box::new(inner),)))
                    }
                    SequenceType::Slice(slice) => {
                        let inner = self.shape_to_schema(slice.t);
                        Schema::Seq(SeqSchema((Box::new(inner),)))
                    }
                }
            }
            Type::Pointer(_) | Type::Undefined => Schema::Any,
        }
    }

    fn scalar_to_schema(&self, shape: &'static Shape) -> Schema {
        match &shape.ty {
            Type::Primitive(prim) => self.primitive_to_schema(prim),
            Type::User(UserType::Opaque) => {
                let type_id = shape.type_identifier;
                match type_id {
                    "String" | "str" | "Cow" | "PathBuf" | "Path" | "OsString" | "OsStr"
                    | "Url" | "Uri" | "Uuid" | "Duration" | "SystemTime" | "Instant" | "IpAddr"
                    | "Ipv4Addr" | "Ipv6Addr" | "SocketAddr" | "SocketAddrV4" | "SocketAddrV6" => {
                        Schema::String(None)
                    }
                    _ => Schema::Type {
                        name: Some(type_id.to_string()),
                    },
                }
            }
            _ => Schema::Any,
        }
    }

    fn primitive_to_schema(&self, prim: &PrimitiveType) -> Schema {
        match prim {
            PrimitiveType::Boolean => Schema::Bool,
            PrimitiveType::Numeric(num) => match num {
                NumericType::Integer { .. } => Schema::Int(None),
                NumericType::Float => Schema::Float(None),
            },
            PrimitiveType::Textual(_) => Schema::String(None),
            PrimitiveType::Never => Schema::Unit,
        }
    }

    fn user_type_to_schema(&mut self, user: &UserType, shape: &'static Shape) -> Schema {
        let type_id = shape.type_identifier;

        // Cycle detection
        if self.generating.contains(type_id) {
            return Schema::Type {
                name: Some(type_id.to_string()),
            };
        }

        match user {
            UserType::Struct(struct_type) => {
                self.generating.insert(type_id);
                let result = self.struct_to_schema(struct_type);
                self.generating.remove(type_id);
                result
            }
            UserType::Enum(enum_type) => {
                self.generating.insert(type_id);
                let result = self.enum_to_schema(enum_type);
                self.generating.remove(type_id);
                result
            }
            UserType::Union(_) => Schema::Any,
            UserType::Opaque => match type_id {
                "String" | "str" | "&str" | "Cow" | "PathBuf" | "Path" => Schema::String(None),
                _ => Schema::Type {
                    name: Some(type_id.to_string()),
                },
            },
        }
    }

    fn struct_to_schema(&mut self, struct_type: &facet_core::StructType) -> Schema {
        use facet_core::StructKind;

        match struct_type.kind {
            StructKind::Unit => Schema::Unit,
            StructKind::Tuple | StructKind::TupleStruct => {
                if struct_type.fields.len() == 1 {
                    // Newtype - unwrap
                    self.shape_to_schema(struct_type.fields[0].shape())
                } else {
                    // Tuple - not well supported, use Any
                    Schema::Any
                }
            }
            StructKind::Struct => {
                let mut fields: HashMap<Option<String>, Schema> = HashMap::new();

                for field in struct_type.fields {
                    let field_name = field.effective_name();
                    let mut field_schema = self.shape_to_schema(field.shape());

                    // Wrap with @default if field has a default value
                    if let Some(default_value) = field_default_value(field) {
                        field_schema =
                            Schema::Default(DefaultSchema((default_value, Box::new(field_schema))));
                    }

                    // Handle catch-all field (empty name)
                    let key = if field_name.is_empty() {
                        None
                    } else {
                        Some(field_name.to_string())
                    };

                    fields.insert(key, field_schema);
                }

                Schema::Object(ObjectSchema(fields))
            }
        }
    }

    fn enum_to_schema(&mut self, enum_type: &facet_core::EnumType) -> Schema {
        use facet_core::StructKind;

        let mut variants: HashMap<String, Schema> = HashMap::new();

        for variant in enum_type.variants {
            let variant_name = variant.effective_name().to_string();
            let variant_schema = match variant.data.kind {
                StructKind::Unit => Schema::Unit,
                StructKind::Tuple | StructKind::TupleStruct => {
                    if variant.data.fields.len() == 1 {
                        self.shape_to_schema(variant.data.fields[0].shape())
                    } else {
                        Schema::Any
                    }
                }
                StructKind::Struct => self.struct_to_schema(&variant.data),
            };

            variants.insert(variant_name, variant_schema);
        }

        Schema::Enum(EnumSchema(variants))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet::Facet;
    use facet_testhelpers::test;

    #[test]
    fn test_simple_struct() {
        #[derive(Facet)]
        #[allow(dead_code)]
        struct Config {
            name: String,
            port: u16,
        }

        let schema = schema_from_type::<Config>();
        tracing::debug!("Generated schema:\n{schema}");
        assert!(schema.contains("meta"));
        assert!(schema.contains("name"));
        assert!(schema.contains("port"));
    }

    #[test]
    fn test_with_option() {
        #[derive(Facet)]
        #[allow(dead_code)]
        struct Config {
            name: String,
            debug: Option<bool>,
        }

        let schema = schema_from_type::<Config>();
        tracing::debug!("Generated schema:\n{schema}");
        assert!(schema.contains("debug"));
        assert!(schema.contains("optional"));
    }

    #[test]
    fn test_with_vec() {
        #[derive(Facet)]
        #[allow(dead_code)]
        struct Config {
            items: Vec<String>,
        }

        let schema = schema_from_type::<Config>();
        tracing::debug!("Generated schema:\n{schema}");
        assert!(schema.contains("items"));
        assert!(schema.contains("seq"));
    }

    #[test]
    fn test_with_default() {
        #[derive(Facet)]
        #[allow(dead_code)]
        struct Config {
            name: String,
            #[facet(default = 8080)]
            port: u16,
        }

        let schema = schema_from_type::<Config>();
        tracing::debug!("Generated schema:\n{schema}");
        assert!(schema.contains("default"));
        assert!(schema.contains("8080"));
    }

    #[test]
    fn test_with_nested_default() {
        #[derive(Facet, Default)]
        #[allow(dead_code)]
        struct Inner {
            #[facet(default = true)]
            enabled: bool,
            #[facet(default = 8080)]
            port: u16,
        }

        #[derive(Facet)]
        #[allow(dead_code)]
        struct Config {
            #[facet(default)]
            inner: Inner,
        }

        let schema = schema_from_type::<Config>();
        tracing::debug!("Generated schema:\n{schema}");
        // The nested default should be serialized with braces
        assert!(schema.contains("default"));
        assert!(schema.contains("enabled"));
    }
}
