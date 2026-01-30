//! Schema generation from Facet types.
//!
//! This module provides utilities for generating Styx schemas from Rust types
//! that implement `Facet`.

use facet_core::{
    Def, DefaultSource, Facet, Field, NumericType, PrimitiveType, PtrConst, PtrMut, PtrUninit,
    Shape, ShapeLayout, Type, UserType,
};
use facet_reflect::Peek;
use std::collections::{HashMap, HashSet};
use std::marker::PhantomData;
use std::path::Path;
use std::ptr::NonNull;

use crate::peek_to_string_expr;
use crate::schema_types::{
    DefaultSchema, Documented, EnumSchema, LspExtensionConfig, MapSchema, Meta, ObjectKey,
    ObjectSchema, OptionalSchema, RawStyx, Schema, SchemaFile, SeqSchema, TupleSchema,
};

/// Strip exactly one leading space from a doc line if present.
///
/// Rust doc comments like `/// text` produce doc strings with a leading space: `" text"`.
/// This function normalizes them to `"text"`.
fn strip_doc_leading_space(s: &str) -> String {
    s.strip_prefix(' ').unwrap_or(s).to_string()
}

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
            if unsafe { shape.call_default_in_place(ptr_uninit) }.is_none() {
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
    lsp: Option<LspExtensionConfig>,
    _marker: PhantomData<T>,
}

impl<T: facet_core::Facet<'static>> GenerateSchema<T> {
    /// Create a new schema generator.
    pub fn new() -> Self {
        Self {
            crate_name: None,
            version: None,
            cli: None,
            lsp: None,
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

    /// Set the LSP extension configuration.
    ///
    /// When set, the generated schema will include this configuration in `meta.lsp`.
    /// The Styx LSP will spawn this extension process to provide domain-specific
    /// completions, hover info, and diagnostics.
    ///
    /// # Example
    ///
    /// ```ignore
    /// GenerateSchema::<Config>::new()
    ///     .crate_name("my-tool")
    ///     .version("1")
    ///     .cli("my-tool")
    ///     .lsp(LspExtensionConfig {
    ///         launch: vec!["my-tool".into(), "lsp-extension".into()],
    ///         capabilities: None,
    ///     })
    ///     .write("config.styx");
    /// ```
    pub fn lsp(mut self, lsp: LspExtensionConfig) -> Self {
        self.lsp = Some(lsp);
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
        let schema_file = self.generate_schema_file();
        crate::to_string(&schema_file).expect("failed to serialize schema")
    }

    /// Generate the schema as a SchemaFile struct.
    ///
    /// This is more efficient than `generate()` when you need to inspect
    /// the schema programmatically, as it avoids the string serialization.
    pub fn generate_schema_file(self) -> SchemaFile {
        let crate_name = self
            .crate_name
            .expect("crate_name is required - call .crate_name(\"...\")");
        let version = self
            .version
            .expect("version is required - call .version(\"...\")");

        let id = format!("crate:{crate_name}@{version}");
        generate_schema_file_inner::<T>(id, self.cli, self.lsp)
    }
}

impl<T: facet_core::Facet<'static>> Default for GenerateSchema<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Generate a Styx schema string from a Facet type.
pub fn schema_from_type<T: facet_core::Facet<'static>>() -> String {
    let schema_file = schema_file_from_type::<T>();
    crate::to_string(&schema_file).expect("failed to serialize schema")
}

/// Generate a SchemaFile directly from a Facet type.
///
/// This is more efficient than `schema_from_type` when you need to inspect
/// the schema programmatically, as it avoids the string serialization and
/// parsing round-trip.
pub fn schema_file_from_type<T: facet_core::Facet<'static>>() -> SchemaFile {
    let shape = T::SHAPE;
    let id = shape.type_identifier.to_string();
    generate_schema_file_inner::<T>(id, None, None)
}

/// Internal function that generates a SchemaFile with the given id and optional cli/lsp.
fn generate_schema_file_inner<T: facet_core::Facet<'static>>(
    id: String,
    cli: Option<String>,
    lsp: Option<LspExtensionConfig>,
) -> SchemaFile {
    let shape = T::SHAPE;

    let mut generator = SchemaGenerator::new();

    // Generate the root schema - inline it directly at @ (None key)
    // Use generate_type_definition to inline the root, while nested types get references
    let root_schema = generator
        .generate_type_definition(shape)
        .unwrap_or_else(|| generator.shape_to_schema(shape));

    // Build the schema map with root and all named type definitions
    let mut schema_map: HashMap<Option<String>, Schema> = HashMap::new();
    schema_map.insert(None, root_schema);

    // Process all pending types (types that were referenced but need definitions)
    while let Some(pending_shape) = generator.take_pending() {
        // Schema is a well-known built-in type - don't generate its definition
        if pending_shape == Schema::SHAPE {
            continue;
        }

        let type_name = pending_shape.type_identifier.to_string();
        // Only add if not already defined
        if !schema_map.contains_key(&Some(type_name.clone()))
            && let Some(type_schema) = generator.generate_type_definition(pending_shape)
        {
            schema_map.insert(Some(type_name), type_schema);
        }
    }

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

    SchemaFile {
        meta: Meta {
            id,
            version: None,
            cli,
            description,
            lsp,
        },
        imports: None,
        schema: schema_map,
    }
}

/// Convert a Schema to a tag name for use in ObjectKey.
///
/// Maps built-in types to their tag names:
/// - Schema::String(_) → "string"
/// - Schema::Int(_) → "int"
/// - Schema::Float(_) → "float"
/// - Schema::Bool → "bool"
/// - Schema::Unit → "unit"
/// - Schema::Any → "any"
/// - Schema::Type { name } → the type name
/// - Complex types → serialized form (for future extensibility)
fn schema_to_tag_name(schema: &Schema) -> String {
    match schema {
        Schema::String(_) => "string".to_string(),
        Schema::Int(_) => "int".to_string(),
        Schema::Float(_) => "float".to_string(),
        Schema::Bool => "bool".to_string(),
        Schema::Unit => "unit".to_string(),
        Schema::Any => "any".to_string(),
        Schema::Type { name } => name.clone().unwrap_or_default(),
        // For complex types, we could serialize them, but for now use "any"
        // This handles cases like @object, @seq, @map, etc. being used as keys
        _ => "any".to_string(),
    }
}

/// Internal schema generator that builds typed Schema structs.
struct SchemaGenerator {
    /// Types currently being generated (for cycle detection)
    generating: HashSet<&'static str>,
    /// Types that have been queued for definition
    queued_types: HashSet<&'static str>,
    /// Types pending generation (shapes to process)
    pending_types: Vec<&'static Shape>,
}

impl SchemaGenerator {
    fn new() -> Self {
        Self {
            generating: HashSet::new(),
            queued_types: HashSet::new(),
            pending_types: Vec::new(),
        }
    }

    /// Queue a type for definition if not already queued.
    fn queue_type(&mut self, shape: &'static Shape) {
        // Schema is a well-known built-in type - don't queue it or its dependencies
        if shape == Schema::SHAPE {
            return;
        }

        let type_id = shape.type_identifier;
        if !self.queued_types.contains(type_id) {
            self.queued_types.insert(type_id);
            self.pending_types.push(shape);
        }
    }

    /// Take the next pending type to generate, if any.
    fn take_pending(&mut self) -> Option<&'static Shape> {
        self.pending_types.pop()
    }

    /// Generate the schema definition for a user type (struct or enum).
    /// This is used when generating named type definitions.
    fn generate_type_definition(&mut self, shape: &'static Shape) -> Option<Schema> {
        match &shape.ty {
            Type::User(user) => {
                let type_id = shape.type_identifier;
                self.generating.insert(type_id);
                let result = match user {
                    UserType::Struct(struct_type) => Some(self.struct_to_schema(struct_type)),
                    UserType::Enum(enum_type) => Some(self.enum_to_schema(enum_type)),
                    _ => None,
                };
                self.generating.remove(type_id);
                result
            }
            _ => None,
        }
    }

    /// Convert a shape to a Schema.
    fn shape_to_schema(&mut self, shape: &'static Shape) -> Schema {
        // Handle metadata containers (like Documented<T>) - unwrap to inner value type
        if let Some(inner_shape) = facet_reflect::get_metadata_container_value_shape(shape) {
            return self.shape_to_schema(inner_shape);
        }

        match &shape.def {
            Def::Scalar => self.scalar_to_schema(shape),
            Def::Option(opt_def) => {
                let inner = self.shape_to_schema(opt_def.t);
                Schema::Optional(OptionalSchema((Documented::new(Box::new(inner)),)))
            }
            Def::List(list_def) => {
                let inner = self.shape_to_schema(list_def.t);
                Schema::Seq(SeqSchema((Documented::new(Box::new(inner)),)))
            }
            Def::Array(array_def) => {
                let inner = self.shape_to_schema(array_def.t);
                Schema::Seq(SeqSchema((Documented::new(Box::new(inner)),)))
            }
            Def::Map(map_def) => {
                let key = self.shape_to_schema(map_def.k);
                let value = self.shape_to_schema(map_def.v);
                Schema::Map(MapSchema(vec![
                    Documented::new(key),
                    Documented::new(value),
                ]))
            }
            Def::Set(set_def) => {
                let inner = self.shape_to_schema(set_def.t);
                Schema::Seq(SeqSchema((Documented::new(Box::new(inner)),)))
            }
            Def::Result(result_def) => {
                let ok = self.shape_to_schema(result_def.t);
                let err = self.shape_to_schema(result_def.e);
                let mut variants = HashMap::new();
                variants.insert(Documented::new("ok".to_string()), ok);
                variants.insert(Documented::new("err".to_string()), err);
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
                Schema::Seq(SeqSchema((Documented::new(Box::new(inner)),)))
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
                        Schema::Seq(SeqSchema((Documented::new(Box::new(inner)),)))
                    }
                    SequenceType::Slice(slice) => {
                        let inner = self.shape_to_schema(slice.t);
                        Schema::Seq(SeqSchema((Documented::new(Box::new(inner)),)))
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

        // Schema is a well-known built-in type - emit a reference without generating its definition.
        // This avoids inlining the entire Schema enum (which describes types) into generated schemas.
        if shape == Schema::SHAPE {
            return Schema::Type {
                name: Some("Schema".to_string()),
            };
        }

        // Cycle detection - if we're already generating this type, return a reference
        if self.generating.contains(type_id) {
            self.queue_type(shape);
            return Schema::Type {
                name: Some(type_id.to_string()),
            };
        }

        match user {
            // For structs and enums, always emit a type reference and queue for definition
            // This gives all complex types their own named definitions
            UserType::Struct(_) | UserType::Enum(_) => {
                self.queue_type(shape);
                Schema::Type {
                    name: Some(type_id.to_string()),
                }
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
                    // Tuple with multiple fields - each position has a distinct type
                    let elements: Vec<Documented<Schema>> = struct_type
                        .fields
                        .iter()
                        .map(|field| Documented::new(self.shape_to_schema(field.shape())))
                        .collect();
                    Schema::Tuple(TupleSchema(elements))
                }
            }
            StructKind::Struct => {
                let mut fields: HashMap<Documented<ObjectKey>, Schema> = HashMap::new();

                for field in struct_type.fields {
                    let field_name = field.effective_name();
                    let field_schema = self.shape_to_schema(field.shape());

                    // Extract doc comments from field
                    let doc = if field.doc.is_empty() {
                        None
                    } else {
                        Some(
                            field
                                .doc
                                .iter()
                                .map(|s| strip_doc_leading_space(s))
                                .collect(),
                        )
                    };

                    // Handle flattened fields - inline their contents
                    if field.is_flattened() {
                        match field_schema {
                            // Flattened map: add catch-all entry with the value type
                            Schema::Map(MapSchema(types)) => {
                                // @map(@V) has 1 type, @map(@K @V) has 2
                                // Get key type tag and value schema
                                let (key_tag, value_schema) = if types.len() == 1 {
                                    // Only value type, key defaults to @string
                                    (
                                        "string".to_string(),
                                        types.into_iter().next().unwrap().value,
                                    )
                                } else {
                                    // Key and value types
                                    let mut iter = types.into_iter();
                                    let key_schema = iter.next().unwrap().value;
                                    let value_schema = iter.next().unwrap().value;
                                    // Convert key schema to tag name
                                    let key_tag = schema_to_tag_name(&key_schema);
                                    (key_tag, value_schema)
                                };
                                let key = Documented {
                                    value: ObjectKey::typed(key_tag),
                                    doc,
                                };
                                fields.insert(key, value_schema);
                            }
                            // Flattened object: merge its fields into parent
                            Schema::Object(ObjectSchema(inner_fields)) => {
                                for (inner_key, inner_schema) in inner_fields {
                                    fields.insert(inner_key, inner_schema);
                                }
                            }
                            // Other flattened types: use unit catch-all
                            other => {
                                let key = Documented {
                                    value: ObjectKey::unit(),
                                    doc,
                                };
                                fields.insert(key, other);
                            }
                        }
                        continue;
                    }

                    let mut field_schema = field_schema;

                    // Wrap with @default if field has a default value
                    if let Some(default_value_str) = field_default_value(field) {
                        let default_value = RawStyx::new(default_value_str);
                        field_schema = Schema::Default(DefaultSchema((
                            default_value,
                            Documented::new(Box::new(field_schema)),
                        )));
                    }

                    // Handle catch-all field (empty name) vs regular named field
                    let key = if field_name.is_empty() {
                        Documented {
                            value: ObjectKey::unit(),
                            doc,
                        }
                    } else {
                        Documented {
                            value: ObjectKey::named(field_name),
                            doc,
                        }
                    };

                    fields.insert(key, field_schema);
                }

                Schema::Object(ObjectSchema(fields))
            }
        }
    }

    fn enum_to_schema(&mut self, enum_type: &facet_core::EnumType) -> Schema {
        use facet_core::StructKind;

        // If any variant has #[facet(other)], this enum accepts any tag,
        // so emit @any instead of trying to enumerate variants
        if enum_type.variants.iter().any(|v| v.is_other()) {
            return Schema::Any;
        }

        let mut variants: HashMap<Documented<String>, Schema> = HashMap::new();

        for variant in enum_type.variants {
            let variant_name = variant.effective_name().to_string();
            let variant_schema = match variant.data.kind {
                StructKind::Unit => Schema::Unit,
                StructKind::Tuple | StructKind::TupleStruct => {
                    if variant.data.fields.len() == 1 {
                        self.shape_to_schema(variant.data.fields[0].shape())
                    } else {
                        // Tuple variant with multiple fields
                        let elements: Vec<Documented<Schema>> = variant
                            .data
                            .fields
                            .iter()
                            .map(|field| Documented::new(self.shape_to_schema(field.shape())))
                            .collect();
                        Schema::Tuple(TupleSchema(elements))
                    }
                }
                StructKind::Struct => self.struct_to_schema(&variant.data),
            };

            // Extract doc comments from variant
            let doc = if variant.doc.is_empty() {
                None
            } else {
                Some(
                    variant
                        .doc
                        .iter()
                        .map(|s| strip_doc_leading_space(s))
                        .collect(),
                )
            };

            variants.insert(
                Documented {
                    value: variant_name,
                    doc,
                },
                variant_schema,
            );
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

    // =========================================================================
    // Roundtrip tests - parse generated schema back into SchemaFile
    // =========================================================================

    #[test]
    fn test_roundtrip_simple_struct() {
        #[derive(Facet)]
        #[allow(dead_code)]
        struct Config {
            name: String,
            port: u16,
        }

        let schema_str = schema_from_type::<Config>();
        tracing::debug!("Generated schema:\n{schema_str}");

        // Parse it back
        let parsed: SchemaFile =
            crate::from_str(&schema_str).expect("failed to parse generated schema");

        // Verify structure
        assert_eq!(parsed.meta.id, "Config");
        assert!(
            parsed.schema.contains_key(&None),
            "should have root schema with None key"
        );

        let root_schema = parsed.schema.get(&None).expect("missing root schema");
        if let Schema::Object(obj) = root_schema {
            assert!(
                obj.0
                    .contains_key(&Documented::new(ObjectKey::named("name")))
            );
            assert!(
                obj.0
                    .contains_key(&Documented::new(ObjectKey::named("port")))
            );
        } else {
            panic!("expected root schema to be Object, got {:?}", root_schema);
        }
    }

    #[test]
    fn test_roundtrip_with_option() {
        #[derive(Facet)]
        #[allow(dead_code)]
        struct Config {
            name: String,
            debug: Option<bool>,
        }

        let schema_str = schema_from_type::<Config>();
        tracing::debug!("Generated schema:\n{schema_str}");

        let parsed: SchemaFile =
            crate::from_str(&schema_str).expect("failed to parse generated schema");

        let root_schema = parsed.schema.get(&None).expect("missing root schema");
        if let Schema::Object(obj) = root_schema {
            let debug_schema = obj
                .0
                .get(&Documented::new(ObjectKey::named("debug")))
                .expect("missing debug field");
            assert!(
                matches!(debug_schema, Schema::Optional(_)),
                "debug should be Optional, got {:?}",
                debug_schema
            );
        } else {
            panic!("expected root schema to be Object");
        }
    }

    #[test]
    fn test_roundtrip_with_vec() {
        #[derive(Facet)]
        #[allow(dead_code)]
        struct Config {
            items: Vec<String>,
        }

        let schema_str = schema_from_type::<Config>();
        tracing::debug!("Generated schema:\n{schema_str}");

        let parsed: SchemaFile =
            crate::from_str(&schema_str).expect("failed to parse generated schema");

        let root_schema = parsed.schema.get(&None).expect("missing root schema");
        if let Schema::Object(obj) = root_schema {
            let items_schema = obj
                .0
                .get(&Documented::new(ObjectKey::named("items")))
                .expect("missing items field");
            assert!(
                matches!(items_schema, Schema::Seq(_)),
                "items should be Seq, got {:?}",
                items_schema
            );
        } else {
            panic!("expected root schema to be Object");
        }
    }

    #[test]
    fn test_roundtrip_with_default() {
        #[derive(Facet)]
        #[allow(dead_code)]
        struct Config {
            name: String,
            #[facet(default = 8080)]
            port: u16,
        }

        let schema_str = schema_from_type::<Config>();
        tracing::debug!("Generated schema:\n{schema_str}");

        let parsed: SchemaFile =
            crate::from_str(&schema_str).expect("failed to parse generated schema");

        let root_schema = parsed.schema.get(&None).expect("missing root schema");
        if let Schema::Object(obj) = root_schema {
            let port_schema = obj
                .0
                .get(&Documented::new(ObjectKey::named("port")))
                .expect("missing port field");
            if let Schema::Default(default_schema) = port_schema {
                assert_eq!(
                    default_schema.0.0.as_str(),
                    "8080",
                    "default value should be 8080"
                );
            } else {
                panic!("port should be Default, got {:?}", port_schema);
            }
        } else {
            panic!("expected root schema to be Object");
        }
    }

    #[test]
    fn test_roundtrip_with_nested_default() {
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
            inner: Inner,
        }

        let schema_str = schema_from_type::<Config>();
        tracing::debug!("Generated schema:\n{schema_str}");

        let parsed: SchemaFile =
            crate::from_str(&schema_str).expect("failed to parse generated schema");

        // Root schema should reference @Inner
        let root_schema = parsed.schema.get(&None).expect("missing root schema");
        if let Schema::Object(obj) = root_schema {
            let inner_schema = obj
                .0
                .get(&Documented::new(ObjectKey::named("inner")))
                .expect("missing inner field");
            // Inner should be a type reference now
            assert!(
                matches!(inner_schema, Schema::Type { name: Some(n) } if n == "Inner"),
                "inner should be Type reference, got {:?}",
                inner_schema
            );
        } else {
            panic!("expected root schema to be Object");
        }

        // Inner type should have its own definition with defaults
        let inner_def = parsed
            .schema
            .get(&Some("Inner".to_string()))
            .expect("missing Inner type definition");
        if let Schema::Object(inner_obj) = inner_def {
            // Check that the inner object has fields with @default wrappers
            let enabled_schema = inner_obj
                .0
                .get(&Documented::new(ObjectKey::named("enabled")))
                .expect("missing enabled field");
            assert!(
                matches!(enabled_schema, Schema::Default(_)),
                "enabled should have @default wrapper"
            );
            let port_schema = inner_obj
                .0
                .get(&Documented::new(ObjectKey::named("port")))
                .expect("missing port field");
            assert!(
                matches!(port_schema, Schema::Default(_)),
                "port should have @default wrapper"
            );
        } else {
            panic!("Inner should be Object, got {:?}", inner_def);
        }
    }

    #[test]
    fn test_schema_uses_at_for_root_key() {
        #[derive(Facet)]
        #[allow(dead_code)]
        struct Config {
            name: String,
        }

        let schema_str = schema_from_type::<Config>();
        tracing::debug!("Generated schema:\n{schema_str}");

        // The schema should use @ as the key for the root type, not "None"
        assert!(
            schema_str.contains("@ @object")
                || schema_str.contains("@\n")
                || schema_str.contains("@ {"),
            "schema should use @ for root key, not None. Got:\n{}",
            schema_str
        );
        assert!(
            !schema_str.contains("None @object"),
            "schema should not contain 'None @object'. Got:\n{}",
            schema_str
        );
    }

    #[test]
    fn test_debug_nested_default_serialization() {
        use crate::to_string_compact;

        #[derive(Facet, Default)]
        #[allow(dead_code)]
        struct Inner {
            #[facet(default = true)]
            enabled: bool,
            #[facet(default = 8080)]
            port: u16,
        }

        // Check what Inner::default() serializes to
        let inner_default = Inner::default();
        let serialized = to_string_compact(&inner_default).expect("serialization should work");
        tracing::debug!("Inner::default() serializes to: {}", serialized);

        // Now check what the schema looks like
        let schema_str = schema_from_type::<Inner>();
        tracing::debug!("Inner schema:\n{}", schema_str);

        // Check if the schema contains @default wrappers at field level
        assert!(
            schema_str.contains("@default"),
            "Inner schema should contain @default for fields with defaults"
        );
    }

    #[test]
    fn test_doc_comment_leading_space_trimmed() {
        /// Configuration with documented fields.
        #[derive(Facet)]
        #[allow(dead_code)]
        struct Config {
            /// The hostname to connect to.
            host: String,
            /// The port number.
            /// Can be any valid port.
            port: u16,
        }

        let schema = schema_from_type::<Config>();
        tracing::debug!("Generated schema:\n{schema}");

        // Doc comments should have exactly one space after ///
        // NOT "///  The hostname" (double space)
        assert!(
            schema.contains("/// The hostname"),
            "Doc comment should have single space after ///"
        );
        assert!(
            !schema.contains("///  The hostname"),
            "Doc comment should NOT have double space after ///"
        );
        assert!(
            schema.contains("/// The port number"),
            "First line of multi-line doc should have single space"
        );
        assert!(
            schema.contains("/// Can be any valid port"),
            "Second line of multi-line doc should have single space"
        );
    }

    #[test]
    fn test_flatten_hashmap() {
        use std::collections::HashMap;

        #[derive(Facet)]
        #[allow(dead_code)]
        struct Decl {
            value: String,
        }

        #[derive(Facet)]
        #[allow(dead_code)]
        struct QueryFile {
            #[facet(flatten)]
            decls: HashMap<String, Decl>,
        }

        let schema = schema_from_type::<QueryFile>();
        tracing::debug!("Generated schema:\n{schema}");

        // Flattened HashMap should NOT have a "decls" key
        assert!(
            !schema.contains("decls"),
            "schema should NOT contain 'decls' key for flattened HashMap. Got:\n{}",
            schema
        );
        // Should have typed catch-all @string with the value type (now a @Decl reference)
        assert!(
            schema.contains("@string @Decl"),
            "schema should have typed catch-all @string entry with @Decl reference. Got:\n{}",
            schema
        );
        // Decl should have its own definition
        assert!(
            schema.contains("Decl @object"),
            "schema should contain Decl type definition. Got:\n{}",
            schema
        );
        // Decl should contain the 'value' field
        assert!(
            schema.contains("value @string"),
            "Decl should contain 'value' field. Got:\n{}",
            schema
        );
    }

    #[test]
    fn test_flatten_struct() {
        #[derive(Facet)]
        #[allow(dead_code)]
        struct Common {
            name: String,
            id: u64,
        }

        #[derive(Facet)]
        #[allow(dead_code)]
        struct Extended {
            #[facet(flatten)]
            common: Common,
            extra: String,
        }

        let schema = schema_from_type::<Extended>();
        tracing::debug!("Generated schema:\n{schema}");

        // Flattened struct should NOT have a "common" key
        assert!(
            !schema.contains("common"),
            "schema should NOT contain 'common' key for flattened struct. Got:\n{}",
            schema
        );
        // Should have the flattened struct's fields directly
        assert!(
            schema.contains("name @string"),
            "schema should contain 'name' field from Common. Got:\n{}",
            schema
        );
        assert!(
            schema.contains("id @int"),
            "schema should contain 'id' field from Common. Got:\n{}",
            schema
        );
        // Should still have the regular field
        assert!(
            schema.contains("extra @string"),
            "schema should contain 'extra' field. Got:\n{}",
            schema
        );
    }

    #[test]
    fn test_flatten_hashmap_with_optional_value() {
        use std::collections::HashMap;

        /// ORDER BY clause.
        #[derive(Facet)]
        #[allow(dead_code)]
        struct OrderBy {
            /// Column name -> direction ("asc" or "desc", None means asc)
            #[facet(flatten)]
            pub columns: HashMap<String, Option<String>>,
        }

        let schema = schema_from_type::<OrderBy>();
        tracing::debug!("Generated schema:\n{schema}");

        // Flattened HashMap<String, Option<String>> should produce @string @optional(@string)
        // NOT "@" @optional(@string) or "\"@\"" @optional(@string)
        assert!(
            schema.contains("@string @optional"),
            "schema should have typed catch-all @string with @optional value. Got:\n{}",
            schema
        );
        assert!(
            !schema.contains("\"@\""),
            "schema should NOT contain quoted @ key. Got:\n{}",
            schema
        );
    }

    #[test]
    fn test_recursive_types_get_definitions() {
        /// A node in a tree structure.
        #[derive(Facet)]
        #[allow(dead_code)]
        struct TreeNode {
            value: String,
            children: Vec<TreeNode>,
        }

        let schema = schema_from_type::<TreeNode>();
        tracing::debug!("Generated schema:\n{schema}");

        // The schema should contain a named definition for TreeNode
        // because it references itself recursively
        assert!(
            schema.contains("TreeNode @object"),
            "schema should have named TreeNode definition. Got:\n{}",
            schema
        );

        // The children field should reference @TreeNode
        assert!(
            schema.contains("@TreeNode"),
            "schema should reference @TreeNode. Got:\n{}",
            schema
        );

        // Parse and verify structure
        let parsed: SchemaFile =
            crate::from_str(&schema).expect("failed to parse generated schema");

        // Should have root definition and TreeNode definition
        assert!(
            parsed.schema.contains_key(&Some("TreeNode".to_string())),
            "schema should have TreeNode type definition"
        );
    }

    #[test]
    fn test_mutually_recursive_types_get_definitions() {
        /// A container that can hold items.
        #[derive(Facet)]
        #[allow(dead_code)]
        struct Container {
            name: String,
            items: Vec<Item>,
        }

        /// An item that can contain other containers.
        #[derive(Facet)]
        #[allow(dead_code)]
        struct Item {
            id: u32,
            nested: Option<Container>,
        }

        let schema = schema_from_type::<Container>();
        tracing::debug!("Generated schema:\n{schema}");

        // Parse and verify both types have definitions
        let parsed: SchemaFile =
            crate::from_str(&schema).expect("failed to parse generated schema");

        // Should have definitions for both Container and Item
        // Note: Container is the root so it's at None, Item should be at Some("Item")
        assert!(
            parsed.schema.contains_key(&Some("Item".to_string())),
            "schema should have Item type definition. Keys: {:?}",
            parsed.schema.keys().collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_enum_with_facet_other_becomes_any() {
        /// An enum that accepts any tag via #[facet(other)].
        #[derive(Facet)]
        #[facet(rename_all = "lowercase")]
        #[repr(u8)]
        #[allow(dead_code)]
        enum ValueExpr {
            /// Default value (@default).
            Default,
            /// Everything else: functions and bare scalars.
            #[facet(other)]
            Other {
                #[facet(tag)]
                tag: Option<String>,
                #[facet(content)]
                content: Option<String>,
            },
        }

        #[derive(Facet)]
        #[allow(dead_code)]
        struct Config {
            value: ValueExpr,
        }

        let schema = schema_from_type::<Config>();
        tracing::debug!("Generated schema:\n{schema}");

        // The ValueExpr type should be @any because of #[facet(other)]
        assert!(
            schema.contains("ValueExpr @any"),
            "enum with #[facet(other)] should become @any. Got:\n{}",
            schema
        );
        // Should NOT have an enum definition with "other" as a variant
        assert!(
            !schema.contains("@enum"),
            "should not emit @enum for type with #[facet(other)]. Got:\n{}",
            schema
        );
        assert!(
            !schema.contains("other"),
            "should not contain 'other' as a variant name. Got:\n{}",
            schema
        );
    }
}
