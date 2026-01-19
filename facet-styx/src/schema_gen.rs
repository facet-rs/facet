//! Schema generation from Facet types.
//!
//! This module provides utilities for generating Styx schemas from Rust types
//! that implement `Facet`.

use facet_core::{
    Def, DefaultSource, Field, NumericType, PrimitiveType, PtrConst, PtrMut, PtrUninit, Shape,
    ShapeLayout, TextualType, Type, UserType,
};
use facet_reflect::Peek;
use std::collections::HashSet;
use std::fmt::Write as _;
use std::marker::PhantomData;
use std::path::Path;
use std::ptr::NonNull;

use crate::peek_to_string;

/// Try to get the default value for a field as a string.
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
        // Zero-sized type
        return None;
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

    // Serialize to styx string
    let styx_str = peek_to_string(peek).ok()?;

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
    ///
    /// This becomes part of the `source` in config files:
    /// `@schema {source crate:myapp-config@1, cli myapp}`
    pub fn crate_name(mut self, name: impl Into<String>) -> Self {
        self.crate_name = Some(name.into());
        self
    }

    /// Set the version for the schema ID.
    ///
    /// This becomes part of the `source` in config files:
    /// `@schema {source crate:myapp-config@1, cli myapp}`
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    /// Set the CLI binary name.
    ///
    /// When users create config files, they can reference your schema like:
    /// `@schema {source crate:myapp-config@1, cli myapp}`
    pub fn cli(mut self, cli: impl Into<String>) -> Self {
        self.cli = Some(cli.into());
        self
    }

    /// Write the schema to `$OUT_DIR/{filename}`.
    ///
    /// Panics if:
    /// - `OUT_DIR` is not set (i.e., not in a build script)
    /// - `crate_name` was not set
    /// - `version` was not set
    pub fn write(self, filename: &str) {
        let out_dir =
            std::env::var("OUT_DIR").expect("OUT_DIR not set - are you in a build script?");
        let path = Path::new(&out_dir).join(filename);

        let schema = self.generate();
        std::fs::write(&path, schema).expect("failed to write schema");
    }

    /// Generate the schema as a string.
    ///
    /// Panics if `crate_name` or `version` was not set.
    pub fn generate(self) -> String {
        let crate_name = self
            .crate_name
            .expect("crate_name is required - call .crate_name(\"...\")");
        let version = self
            .version
            .expect("version is required - call .version(\"...\")");

        let id = format!("crate:{crate_name}@{version}");

        let shape = T::SHAPE;
        let mut generator = SchemaGenerator::new(id, self.cli);
        generator.generate(shape)
    }
}

impl<T: facet_core::Facet<'static>> Default for GenerateSchema<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Generate a Styx schema from a Facet type and write it to `$OUT_DIR/{filename}`.
///
/// **Deprecated**: Use [`GenerateSchema`] builder instead, which requires an explicit ID.
///
/// This function auto-generates the ID from the type name, which may not match
/// your crate naming conventions.
#[deprecated(
    since = "0.2.0",
    note = "use GenerateSchema::new().crate_name(...).version(...).cli(...).write(filename) instead"
)]
pub fn generate_schema<T: facet_core::Facet<'static>>(filename: &str) {
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set - are you in a build script?");
    let path = Path::new(&out_dir).join(filename);

    let schema = schema_from_type::<T>();
    std::fs::write(&path, schema).expect("failed to write schema");
}

/// Generate a Styx schema string from a Facet type.
///
/// Returns the schema as a string that can be written to a file or used directly.
/// The ID is auto-generated from the type name.
pub fn schema_from_type<T: facet_core::Facet<'static>>() -> String {
    let shape = T::SHAPE;
    let mut generator = SchemaGenerator::new_auto(shape);
    generator.generate(shape)
}

/// Internal schema generator that tracks state during generation.
struct SchemaGenerator {
    /// Schema ID (e.g., "crate:myapp-config@1")
    id: String,
    /// Optional CLI binary name
    cli: Option<String>,
    /// Named type definitions to emit after the main schema
    type_defs: String,
    /// Types currently being generated (for cycle detection)
    generating: HashSet<&'static str>,
}

impl SchemaGenerator {
    fn new(id: String, cli: Option<String>) -> Self {
        Self {
            id,
            cli,
            type_defs: String::new(),
            generating: HashSet::new(),
        }
    }

    fn new_auto(shape: &'static Shape) -> Self {
        Self {
            id: shape.type_identifier.to_string(),
            cli: None,
            type_defs: String::new(),
            generating: HashSet::new(),
        }
    }

    fn generate(&mut self, shape: &'static Shape) -> String {
        let mut output = String::new();

        // Meta block
        writeln!(output, "meta {{").unwrap();
        writeln!(output, "    id {}", self.id).unwrap();
        if let Some(cli) = &self.cli {
            writeln!(output, "    cli {cli}").unwrap();
        }

        // Add description from doc comments if present
        if !shape.doc.is_empty() {
            let description: String = shape
                .doc
                .iter()
                .map(|s| s.trim())
                .collect::<Vec<_>>()
                .join(" ");
            writeln!(output, "    description {}", quote_string(&description)).unwrap();
        }

        writeln!(output, "}}").unwrap();

        // Schema block
        writeln!(output, "schema {{").unwrap();

        // Generate the root type inline
        let root_schema = self.shape_to_schema(shape, 1);
        writeln!(output, "    @ {root_schema}").unwrap();

        writeln!(output, "}}").unwrap();

        // Append any named type definitions
        if !self.type_defs.is_empty() {
            output.push('\n');
            output.push_str(&self.type_defs);
        }

        output
    }

    /// Convert a shape to its Styx schema representation.
    fn shape_to_schema(&mut self, shape: &'static Shape, depth: usize) -> String {
        // Handle based on Def first (semantic definition)
        match &shape.def {
            Def::Scalar => self.scalar_to_schema(shape),
            Def::Option(opt_def) => {
                let inner = self.shape_to_schema(opt_def.t, depth);
                format!("@optional({inner})")
            }
            Def::List(list_def) => {
                let inner = self.shape_to_schema(list_def.t, depth);
                format!("@seq({inner})")
            }
            Def::Array(array_def) => {
                let inner = self.shape_to_schema(array_def.t, depth);
                format!("@seq({inner})")
            }
            Def::Map(map_def) => {
                let key = self.shape_to_schema(map_def.k, depth);
                let value = self.shape_to_schema(map_def.v, depth);
                format!("@map({key} {value})")
            }
            Def::Set(set_def) => {
                let inner = self.shape_to_schema(set_def.t, depth);
                format!("@seq({inner})")
            }
            Def::Result(result_def) => {
                let ok = self.shape_to_schema(result_def.t, depth);
                let err = self.shape_to_schema(result_def.e, depth);
                format!("@enum{{ok {ok}, err {err}}}")
            }
            Def::Pointer(ptr_def) => {
                // For smart pointers like Arc<T>, Box<T>, just use the pointee type
                if let Some(pointee) = ptr_def.pointee {
                    self.shape_to_schema(pointee, depth)
                } else {
                    "@any".to_string()
                }
            }
            Def::Slice(slice_def) => {
                let inner = self.shape_to_schema(slice_def.t, depth);
                format!("@seq({inner})")
            }
            Def::Undefined | Def::NdArray(_) | Def::DynamicValue(_) => {
                // Fall back to Type-based handling
                self.type_to_schema(shape, depth)
            }
            // Def is non_exhaustive, handle any future variants
            _ => self.type_to_schema(shape, depth),
        }
    }

    /// Convert based on Type (Rust type category).
    fn type_to_schema(&mut self, shape: &'static Shape, depth: usize) -> String {
        match &shape.ty {
            Type::Primitive(prim) => self.primitive_type_to_schema(prim, shape),
            Type::User(user) => self.user_type_to_schema(user, shape, depth),
            Type::Sequence(seq) => {
                use facet_core::SequenceType;
                match seq {
                    SequenceType::Array(arr) => {
                        let inner = self.shape_to_schema(arr.t, depth);
                        format!("@seq({inner})")
                    }
                    SequenceType::Slice(slice) => {
                        let inner = self.shape_to_schema(slice.t, depth);
                        format!("@seq({inner})")
                    }
                }
            }
            Type::Pointer(_) | Type::Undefined => "@any".to_string(),
        }
    }

    /// Convert scalar types (primitives and opaque types like String) to Styx schema.
    fn scalar_to_schema(&self, shape: &'static Shape) -> String {
        match &shape.ty {
            Type::Primitive(prim) => self.primitive_type_to_schema(prim, shape),
            Type::User(UserType::Opaque) => {
                // Handle well-known opaque types
                let type_id = shape.type_identifier;
                match type_id {
                    "String" | "str" | "Cow" => "@string".to_string(),
                    "PathBuf" | "Path" => "@string".to_string(),
                    "OsString" | "OsStr" => "@string".to_string(),
                    "Url" | "Uri" => "@string".to_string(),
                    "Uuid" => "@string".to_string(),
                    "Duration" => "@string".to_string(),
                    "SystemTime" | "Instant" => "@string".to_string(),
                    "IpAddr" | "Ipv4Addr" | "Ipv6Addr" => "@string".to_string(),
                    "SocketAddr" | "SocketAddrV4" | "SocketAddrV6" => "@string".to_string(),
                    _ => format!("@{type_id}"),
                }
            }
            _ => "@any".to_string(),
        }
    }

    fn primitive_type_to_schema(&self, prim: &PrimitiveType, _shape: &'static Shape) -> String {
        match prim {
            PrimitiveType::Boolean => "@bool".to_string(),
            PrimitiveType::Numeric(num) => match num {
                NumericType::Integer { .. } => "@int".to_string(),
                NumericType::Float => "@float".to_string(),
            },
            PrimitiveType::Textual(text) => match text {
                TextualType::Char => "@string".to_string(),
                TextualType::Str => "@string".to_string(),
            },
            PrimitiveType::Never => "@unit".to_string(),
        }
    }

    /// Convert user-defined types (struct, enum, union) to Styx schema.
    fn user_type_to_schema(
        &mut self,
        user: &UserType,
        shape: &'static Shape,
        depth: usize,
    ) -> String {
        let type_id = shape.type_identifier;

        // Cycle detection: if we're already generating this type, emit a reference
        // to break the cycle and prevent stack overflow
        if self.generating.contains(type_id) {
            return format!("@{type_id}");
        }

        match user {
            UserType::Struct(struct_type) => {
                // Track that we're generating this type
                self.generating.insert(type_id);
                let result = self.struct_to_schema(struct_type, shape, depth);
                self.generating.remove(type_id);
                result
            }
            UserType::Enum(enum_type) => {
                // Track that we're generating this type
                self.generating.insert(type_id);
                let result = self.enum_to_schema(enum_type, shape, depth);
                self.generating.remove(type_id);
                result
            }
            UserType::Union(_) => {
                // Unions are tricky - treat as any for now
                "@any".to_string()
            }
            UserType::Opaque => {
                // Opaque types - check if it's a known type like String
                let type_id = shape.type_identifier;
                match type_id {
                    "String" | "str" | "&str" | "Cow" => "@string".to_string(),
                    "PathBuf" | "Path" => "@string".to_string(),
                    "OsString" | "OsStr" => "@string".to_string(),
                    "Url" | "Uri" => "@string".to_string(),
                    "Uuid" => "@string".to_string(),
                    "Duration" => "@string".to_string(),
                    "SystemTime" | "Instant" => "@string".to_string(),
                    "IpAddr" | "Ipv4Addr" | "Ipv6Addr" => "@string".to_string(),
                    "SocketAddr" | "SocketAddrV4" | "SocketAddrV6" => "@string".to_string(),
                    _ => {
                        // Reference to a named type
                        format!("@{type_id}")
                    }
                }
            }
        }
    }

    fn struct_to_schema(
        &mut self,
        struct_type: &facet_core::StructType,
        _shape: &'static Shape,
        depth: usize,
    ) -> String {
        use facet_core::StructKind;

        match struct_type.kind {
            StructKind::Unit => "@unit".to_string(),
            StructKind::Tuple | StructKind::TupleStruct => {
                // Tuple structs become sequences
                if struct_type.fields.len() == 1 {
                    // Newtype - just use inner type
                    self.shape_to_schema(struct_type.fields[0].shape(), depth)
                } else {
                    // Multiple fields - use a sequence
                    let fields: Vec<String> = struct_type
                        .fields
                        .iter()
                        .map(|f| self.shape_to_schema(f.shape(), depth))
                        .collect();
                    format!("@seq({})", fields.join(" "))
                }
            }
            StructKind::Struct => {
                // Named struct - emit as @object{...}
                let indent = "    ".repeat(depth);
                let field_indent = "    ".repeat(depth + 1);

                let mut fields = String::new();
                for field in struct_type.fields {
                    // Add doc comment if present
                    for doc in field.doc {
                        let doc = doc.trim();
                        if !doc.is_empty() {
                            writeln!(fields, "{field_indent}/// {doc}").unwrap();
                        }
                    }

                    let field_name = field.effective_name();
                    let mut field_schema = self.shape_to_schema(field.shape(), depth + 1);

                    // Wrap with @default if field has a default value
                    if let Some(default_value) = field_default_value(field) {
                        field_schema = format!("@default({default_value} {field_schema})");
                    }

                    writeln!(fields, "{field_indent}{field_name} {field_schema}").unwrap();
                }

                if fields.is_empty() {
                    "@object{}".to_string()
                } else {
                    format!("@object{{\n{fields}{indent}}}")
                }
            }
        }
    }

    #[allow(unused_variables)]
    fn enum_to_schema(
        &mut self,
        enum_type: &facet_core::EnumType,
        shape: &'static Shape,
        depth: usize,
    ) -> String {
        use facet_core::StructKind;

        let indent = "    ".repeat(depth);
        let variant_indent = "    ".repeat(depth + 1);

        let mut variants = String::new();
        for variant in enum_type.variants {
            // Add doc comment if present
            for doc in variant.doc {
                let doc = doc.trim();
                if !doc.is_empty() {
                    writeln!(variants, "{variant_indent}/// {doc}").unwrap();
                }
            }

            let variant_name = variant.effective_name();
            let variant_schema = match variant.data.kind {
                StructKind::Unit => "@unit".to_string(),
                StructKind::Tuple | StructKind::TupleStruct => {
                    if variant.data.fields.len() == 1 {
                        self.shape_to_schema(variant.data.fields[0].shape(), depth + 1)
                    } else {
                        let fields: Vec<String> = variant
                            .data
                            .fields
                            .iter()
                            .map(|f| self.shape_to_schema(f.shape(), depth + 1))
                            .collect();
                        format!("@seq({})", fields.join(" "))
                    }
                }
                StructKind::Struct => {
                    // Struct variant - emit as inline object
                    self.struct_to_schema(&variant.data, shape, depth + 1)
                }
            };

            writeln!(variants, "{variant_indent}{variant_name} {variant_schema}").unwrap();
        }

        if variants.is_empty() {
            "@enum{}".to_string()
        } else {
            format!("@enum{{\n{variants}{indent}}}")
        }
    }
}

/// Quote a string for Styx output.
fn quote_string(s: &str) -> String {
    // Check if the string needs quoting
    let needs_quotes = s.is_empty()
        || s.contains(char::is_whitespace)
        || s.contains('"')
        || s.contains('\\')
        || s.contains('{')
        || s.contains('}')
        || s.contains('(')
        || s.contains(')')
        || s.starts_with('@');

    if !needs_quotes {
        return s.to_string();
    }

    // Escape and quote
    let mut quoted = String::with_capacity(s.len() + 2);
    quoted.push('"');
    for c in s.chars() {
        match c {
            '"' => quoted.push_str("\\\""),
            '\\' => quoted.push_str("\\\\"),
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\t' => quoted.push_str("\\t"),
            _ => quoted.push(c),
        }
    }
    quoted.push('"');
    quoted
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
        assert!(schema.contains("meta {"));
        assert!(schema.contains("id Config"));
        assert!(schema.contains("schema {"));
        assert!(schema.contains("name @string"));
        assert!(schema.contains("port @int"));
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
        assert!(schema.contains("debug @optional(@bool)"));
    }

    #[test]
    fn test_with_vec() {
        #[derive(Facet)]
        #[allow(dead_code)]
        struct Config {
            items: Vec<String>,
        }

        let schema = schema_from_type::<Config>();
        assert!(schema.contains("items @seq(@string)"));
    }

    #[test]
    fn test_nested_struct() {
        #[derive(Facet)]
        #[allow(dead_code)]
        struct Inner {
            value: i32,
        }

        #[derive(Facet)]
        #[allow(dead_code)]
        struct Outer {
            inner: Inner,
        }

        let schema = schema_from_type::<Outer>();
        assert!(schema.contains("inner @object{"));
        assert!(schema.contains("value @int"));
    }

    #[test]
    fn test_enum() {
        #[derive(Facet)]
        #[repr(u8)]
        #[allow(dead_code)]
        enum Status {
            Active,
            Inactive,
            Pending(String),
        }

        let schema = schema_from_type::<Status>();
        assert!(schema.contains("@enum{"));
        assert!(schema.contains("Active @unit"));
        assert!(schema.contains("Inactive @unit"));
        assert!(schema.contains("Pending @string"));
    }

    #[test]
    fn test_recursive_type_no_stack_overflow() {
        // Test that recursive types don't cause a stack overflow.
        // The generator should detect cycles and emit type references.
        use std::collections::HashMap;

        #[derive(Facet)]
        #[repr(u8)]
        #[allow(dead_code)]
        enum RecursiveSchema {
            String,
            Int,
            Object(HashMap<String, Box<RecursiveSchema>>),
            Seq(Box<RecursiveSchema>),
        }

        // This should complete without stack overflow
        let schema = schema_from_type::<RecursiveSchema>();
        tracing::debug!("Recursive schema:\n{schema}");

        // Verify the schema was generated
        assert!(schema.contains("@enum{"));
        assert!(schema.contains("String @unit"));
        assert!(schema.contains("Int @unit"));
        // The recursive reference should be broken with a type reference
        assert!(
            schema.contains("@RecursiveSchema") || schema.contains("Object @map"),
            "schema should contain type reference or map"
        );
    }

    #[test]
    fn test_direct_self_reference() {
        // Test a type that directly references itself
        use std::collections::HashMap;

        #[derive(Facet)]
        #[allow(dead_code)]
        struct TreeNode {
            value: String,
            children: Option<HashMap<String, Box<TreeNode>>>,
        }

        // This should complete without stack overflow
        let schema = schema_from_type::<TreeNode>();
        tracing::debug!("TreeNode schema:\n{schema}");

        assert!(schema.contains("value @string"));
        assert!(schema.contains("children @optional"));
    }

    #[test]
    fn test_styx_schema_schema_type() {
        // Test the actual styx_schema::Schema type from issue #5
        // This was causing stack overflow before the fix
        use styx_schema::Schema;

        // This should complete without stack overflow
        let schema = schema_from_type::<Schema>();
        tracing::debug!("styx_schema::Schema schema:\n{schema}");

        // Just verify it generated something - the exact output depends on Schema's definition
        assert!(schema.contains("@enum{"));
        assert!(schema.len() > 100, "schema should have substantial content");
    }
}
