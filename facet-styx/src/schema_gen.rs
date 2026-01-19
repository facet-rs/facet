//! Schema generation from Facet types.
//!
//! This module provides utilities for generating Styx schemas from Rust types
//! that implement `Facet`.

use facet_core::{
    Def, DefaultSource, Field, NumericType, PrimitiveType, PtrConst, PtrMut, PtrUninit, Shape,
    ShapeLayout, Type, UserType,
};
use facet_reflect::Peek;
use std::collections::HashSet;
use std::fmt::Write;
use std::marker::PhantomData;
use std::path::Path;
use std::ptr::NonNull;

use crate::peek_to_string_with_options;
use styx_format::FormatOptions;

/// Try to get the default value for a field as a styx string.
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

    // Serialize to styx string (compact/inline format)
    let options = FormatOptions::default().inline();
    let styx_str = peek_to_string_with_options(peek, &options).ok()?;

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
        generator.generate_schema_file(&id, self.cli.as_deref(), shape)
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
    generator.generate_schema_file(id, None, shape)
}

/// Internal schema generator that outputs styx schema syntax.
struct SchemaGenerator {
    /// Types currently being generated (for cycle detection)
    generating: HashSet<&'static str>,
    /// Output buffer
    output: String,
    /// Current indentation level
    indent: usize,
}

impl SchemaGenerator {
    fn new() -> Self {
        Self {
            generating: HashSet::new(),
            output: String::new(),
            indent: 0,
        }
    }

    fn write_indent(&mut self) {
        for _ in 0..self.indent {
            self.output.push_str("    ");
        }
    }

    fn generate_schema_file(
        &mut self,
        id: &str,
        cli: Option<&str>,
        shape: &'static Shape,
    ) -> String {
        // Meta block
        self.output.push_str("meta {\n");
        self.indent += 1;

        self.write_indent();
        writeln!(self.output, "id {id}").unwrap();

        if let Some(cli) = cli {
            self.write_indent();
            writeln!(self.output, "cli {cli}").unwrap();
        }

        if !shape.doc.is_empty() {
            let desc = shape
                .doc
                .iter()
                .map(|s| s.trim())
                .collect::<Vec<_>>()
                .join(" ");
            self.write_indent();
            writeln!(self.output, "description \"{}\"", escape_string(&desc)).unwrap();
        }

        self.indent -= 1;
        self.output.push_str("}\n\n");

        // Schema block
        self.output.push_str("schema ");
        self.shape_to_schema(shape);
        self.output.push('\n');

        std::mem::take(&mut self.output)
    }

    /// Convert a shape to schema syntax and write to output.
    fn shape_to_schema(&mut self, shape: &'static Shape) {
        match &shape.def {
            Def::Scalar => self.scalar_to_schema(shape),
            Def::Option(opt_def) => {
                self.output.push_str("@optional(");
                self.shape_to_schema(opt_def.t);
                self.output.push(')');
            }
            Def::List(list_def) => {
                self.output.push_str("@seq(");
                self.shape_to_schema(list_def.t);
                self.output.push(')');
            }
            Def::Array(array_def) => {
                self.output.push_str("@seq(");
                self.shape_to_schema(array_def.t);
                self.output.push(')');
            }
            Def::Map(map_def) => {
                self.output.push_str("@map(");
                self.shape_to_schema(map_def.k);
                self.output.push(' ');
                self.shape_to_schema(map_def.v);
                self.output.push(')');
            }
            Def::Set(set_def) => {
                self.output.push_str("@seq(");
                self.shape_to_schema(set_def.t);
                self.output.push(')');
            }
            Def::Result(result_def) => {
                self.output.push_str("@enum{\n");
                self.indent += 1;
                self.write_indent();
                self.output.push_str("ok ");
                self.shape_to_schema(result_def.t);
                self.output.push('\n');
                self.write_indent();
                self.output.push_str("err ");
                self.shape_to_schema(result_def.e);
                self.output.push('\n');
                self.indent -= 1;
                self.write_indent();
                self.output.push('}');
            }
            Def::Pointer(ptr_def) => {
                if let Some(pointee) = ptr_def.pointee {
                    self.shape_to_schema(pointee);
                } else {
                    self.output.push_str("@any");
                }
            }
            Def::Slice(slice_def) => {
                self.output.push_str("@seq(");
                self.shape_to_schema(slice_def.t);
                self.output.push(')');
            }
            Def::Undefined | Def::NdArray(_) | Def::DynamicValue(_) => self.type_to_schema(shape),
            _ => self.type_to_schema(shape),
        }
    }

    fn type_to_schema(&mut self, shape: &'static Shape) {
        match &shape.ty {
            Type::Primitive(prim) => self.primitive_to_schema(prim),
            Type::User(user) => self.user_type_to_schema(user, shape),
            Type::Sequence(seq) => {
                use facet_core::SequenceType;
                match seq {
                    SequenceType::Array(arr) => {
                        self.output.push_str("@seq(");
                        self.shape_to_schema(arr.t);
                        self.output.push(')');
                    }
                    SequenceType::Slice(slice) => {
                        self.output.push_str("@seq(");
                        self.shape_to_schema(slice.t);
                        self.output.push(')');
                    }
                }
            }
            Type::Pointer(_) | Type::Undefined => {
                self.output.push_str("@any");
            }
        }
    }

    fn scalar_to_schema(&mut self, shape: &'static Shape) {
        match &shape.ty {
            Type::Primitive(prim) => self.primitive_to_schema(prim),
            Type::User(UserType::Opaque) => {
                let type_id = shape.type_identifier;
                match type_id {
                    "String" | "str" | "Cow" | "PathBuf" | "Path" | "OsString" | "OsStr"
                    | "Url" | "Uri" | "Uuid" | "Duration" | "SystemTime" | "Instant" | "IpAddr"
                    | "Ipv4Addr" | "Ipv6Addr" | "SocketAddr" | "SocketAddrV4" | "SocketAddrV6" => {
                        self.output.push_str("@string");
                    }
                    _ => {
                        write!(self.output, "@{type_id}").unwrap();
                    }
                }
            }
            _ => {
                self.output.push_str("@any");
            }
        }
    }

    fn primitive_to_schema(&mut self, prim: &PrimitiveType) {
        match prim {
            PrimitiveType::Boolean => self.output.push_str("@bool"),
            PrimitiveType::Numeric(num) => match num {
                NumericType::Integer { .. } => self.output.push_str("@int"),
                NumericType::Float => self.output.push_str("@float"),
            },
            PrimitiveType::Textual(_) => self.output.push_str("@string"),
            PrimitiveType::Never => self.output.push_str("@unit"),
        }
    }

    fn user_type_to_schema(&mut self, user: &UserType, shape: &'static Shape) {
        let type_id = shape.type_identifier;

        // Cycle detection
        if self.generating.contains(type_id) {
            write!(self.output, "@{type_id}").unwrap();
            return;
        }

        match user {
            UserType::Struct(struct_type) => {
                self.generating.insert(type_id);
                self.struct_to_schema(struct_type);
                self.generating.remove(type_id);
            }
            UserType::Enum(enum_type) => {
                self.generating.insert(type_id);
                self.enum_to_schema(enum_type);
                self.generating.remove(type_id);
            }
            UserType::Union(_) => {
                self.output.push_str("@any");
            }
            UserType::Opaque => match type_id {
                "String" | "str" | "&str" | "Cow" | "PathBuf" | "Path" => {
                    self.output.push_str("@string");
                }
                _ => {
                    write!(self.output, "@{type_id}").unwrap();
                }
            },
        }
    }

    fn struct_to_schema(&mut self, struct_type: &facet_core::StructType) {
        use facet_core::StructKind;

        match struct_type.kind {
            StructKind::Unit => {
                self.output.push_str("@unit");
            }
            StructKind::Tuple | StructKind::TupleStruct => {
                if struct_type.fields.len() == 1 {
                    // Newtype - unwrap
                    self.shape_to_schema(struct_type.fields[0].shape());
                } else {
                    // Tuple as any for now
                    self.output.push_str("@any");
                }
            }
            StructKind::Struct => {
                self.output.push_str("@object{\n");
                self.indent += 1;

                for field in struct_type.fields {
                    let field_name = field.effective_name();
                    self.write_indent();

                    // Handle catch-all field
                    if field_name.is_empty() {
                        self.output.push_str("@ ");
                    } else {
                        write!(self.output, "{field_name} ").unwrap();
                    }

                    // Check for default value
                    if let Some(default_value) = field_default_value(field) {
                        self.output.push_str("@default(");
                        self.output.push_str(&default_value);
                        self.output.push(' ');
                        self.shape_to_schema(field.shape());
                        self.output.push(')');
                    } else {
                        self.shape_to_schema(field.shape());
                    }

                    self.output.push('\n');
                }

                self.indent -= 1;
                self.write_indent();
                self.output.push('}');
            }
        }
    }

    fn enum_to_schema(&mut self, enum_type: &facet_core::EnumType) {
        use facet_core::StructKind;

        self.output.push_str("@enum{\n");
        self.indent += 1;

        for variant in enum_type.variants {
            let variant_name = variant.effective_name();
            self.write_indent();
            write!(self.output, "{variant_name} ").unwrap();

            match variant.data.kind {
                StructKind::Unit => {
                    self.output.push_str("@unit");
                }
                StructKind::Tuple | StructKind::TupleStruct => {
                    if variant.data.fields.len() == 1 {
                        self.shape_to_schema(variant.data.fields[0].shape());
                    } else {
                        self.output.push_str("@any");
                    }
                }
                StructKind::Struct => {
                    self.struct_to_schema(&variant.data);
                }
            }

            self.output.push('\n');
        }

        self.indent -= 1;
        self.write_indent();
        self.output.push('}');
    }
}

/// Escape a string for styx output.
fn escape_string(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            _ => result.push(c),
        }
    }
    result
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
        assert!(schema.contains("@default("));
        assert!(schema.contains("8080"));
    }
}
