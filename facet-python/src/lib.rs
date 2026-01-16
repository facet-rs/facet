//! Generate Python type definitions from facet type metadata.
//!
//! This crate uses facet's reflection capabilities to generate Python
//! type hints and TypedDicts from any type that implements `Facet`.
//!
//! # Example
//!
//! ```
//! use facet::Facet;
//! use facet_python::to_python;
//!
//! #[derive(Facet)]
//! struct User {
//!     name: String,
//!     age: u32,
//!     email: Option<String>,
//! }
//!
//! let py = to_python::<User>();
//! assert!(py.contains("class User(TypedDict"));
//! ```

extern crate alloc;

use alloc::collections::BTreeSet;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

use facet_core::{Def, Facet, Field, Shape, StructKind, Type, UserType};

/// Generate Python definitions for a single type.
///
/// Returns a string containing the Python TypedDict or type declaration.
pub fn to_python<T: Facet<'static>>() -> String {
    let mut generator = PythonGenerator::new();
    generator.add_shape(T::SHAPE);
    generator.finish()
}

/// Generator for Python type definitions.
///
/// Use this when you need to generate multiple related types.
pub struct PythonGenerator {
    output: String,
    /// Types already generated (by type identifier)
    generated: BTreeSet<&'static str>,
    /// Types queued for generation
    queue: Vec<&'static Shape>,
}

impl Default for PythonGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl PythonGenerator {
    /// Create a new Python generator.
    pub const fn new() -> Self {
        Self {
            output: String::new(),
            generated: BTreeSet::new(),
            queue: Vec::new(),
        }
    }

    /// Add a type to generate.
    pub fn add_type<T: Facet<'static>>(&mut self) {
        self.add_shape(T::SHAPE);
    }

    /// Add a shape to generate.
    pub fn add_shape(&mut self, shape: &'static Shape) {
        if !self.generated.contains(shape.type_identifier) {
            self.queue.push(shape);
        }
    }

    /// Finish generation and return the Python code.
    pub fn finish(mut self) -> String {
        // Process queue until empty
        while let Some(shape) = self.queue.pop() {
            if self.generated.contains(shape.type_identifier) {
                continue;
            }
            self.generated.insert(shape.type_identifier);
            self.generate_shape(shape);
        }
        self.output
    }

    fn generate_shape(&mut self, shape: &'static Shape) {
        // Handle transparent wrappers - generate a type alias to the inner type
        if let Some(inner) = shape.inner {
            self.add_shape(inner);
            let inner_type = self.type_for_shape(inner);
            self.write_doc_comment(shape.doc);
            writeln!(self.output, "{} = {}", shape.type_identifier, inner_type).unwrap();
            self.output.push('\n');
            return;
        }

        match &shape.ty {
            Type::User(UserType::Struct(st)) => {
                self.generate_struct(shape, st.fields, st.kind);
            }
            Type::User(UserType::Enum(en)) => {
                self.generate_enum(shape, en);
            }
            _ => {
                // For other types, generate a type alias
                let type_str = self.type_for_shape(shape);
                self.write_doc_comment(shape.doc);
                writeln!(self.output, "{} = {}", shape.type_identifier, type_str).unwrap();
                self.output.push('\n');
            }
        }
    }

    fn write_doc_comment(&mut self, doc: &[&str]) {
        if !doc.is_empty() {
            for line in doc {
                self.output.push('#');
                self.output.push_str(line);
                self.output.push('\n');
            }
        }
    }

    fn generate_struct(
        &mut self,
        shape: &'static Shape,
        fields: &'static [Field],
        kind: StructKind,
    ) {
        match kind {
            StructKind::Unit => {
                // Unit struct as None type alias
                self.write_doc_comment(shape.doc);
                writeln!(self.output, "{} = None", shape.type_identifier).unwrap();
            }
            StructKind::TupleStruct if fields.len() == 1 => {
                // Newtype - type alias to inner
                let inner_type = self.type_for_shape(fields[0].shape.get());
                self.write_doc_comment(shape.doc);
                writeln!(self.output, "{} = {}", shape.type_identifier, inner_type).unwrap();
            }
            StructKind::TupleStruct | StructKind::Tuple => {
                // Tuple type
                let types: Vec<String> = fields
                    .iter()
                    .map(|f| self.type_for_shape(f.shape.get()))
                    .collect();
                self.write_doc_comment(shape.doc);
                writeln!(
                    self.output,
                    "{} = tuple[{}]",
                    shape.type_identifier,
                    types.join(", ")
                )
                .unwrap();
            }
            StructKind::Struct => {
                self.write_doc_comment(shape.doc);
                writeln!(
                    self.output,
                    "class {}(TypedDict, total=False):",
                    shape.type_identifier
                )
                .unwrap();

                let visible_fields: Vec<_> = fields
                    .iter()
                    .filter(|f| !f.flags.contains(facet_core::FieldFlags::SKIP))
                    .collect();

                if visible_fields.is_empty() {
                    self.output.push_str("    pass\n");
                } else {
                    for field in visible_fields {
                        // Generate doc comment for field
                        if !field.doc.is_empty() {
                            for line in field.doc {
                                self.output.push_str("    #");
                                self.output.push_str(line);
                                self.output.push('\n');
                            }
                        }

                        let field_name = field.effective_name();
                        let is_option = matches!(field.shape.get().def, Def::Option(_));

                        if is_option {
                            // Optional field - unwrap the Option and don't use Required
                            if let Def::Option(opt) = &field.shape.get().def {
                                let inner_type = self.type_for_shape(opt.t);
                                writeln!(self.output, "    {}: {}", field_name, inner_type)
                                    .unwrap();
                            }
                        } else {
                            // Required field - wrap in Required[]
                            let field_type = self.type_for_shape(field.shape.get());
                            writeln!(self.output, "    {}: Required[{}]", field_name, field_type)
                                .unwrap();
                        }
                    }
                }
            }
        }
        self.output.push('\n');
    }

    fn generate_enum(&mut self, shape: &'static Shape, enum_type: &facet_core::EnumType) {
        // Check if all variants are unit variants (simple Literal union)
        let all_unit = enum_type
            .variants
            .iter()
            .all(|v| matches!(v.data.kind, StructKind::Unit));

        if all_unit {
            // Simple Literal union
            let variants: Vec<String> = enum_type
                .variants
                .iter()
                .map(|v| format!("Literal[\"{}\"]", v.effective_name()))
                .collect();
            self.write_doc_comment(shape.doc);
            writeln!(
                self.output,
                "{} = {}",
                shape.type_identifier,
                variants.join(" | ")
            )
            .unwrap();
        } else {
            // Discriminated union with data
            // Generate TypedDict classes for each variant, then union them
            let mut variant_class_names = Vec::new();

            for variant in enum_type.variants {
                let variant_name = variant.effective_name();
                let pascal_variant_name = to_pascal_case(variant_name);

                match variant.data.kind {
                    StructKind::Unit => {
                        // Unit variant - wrapper class with Literal value
                        writeln!(
                            self.output,
                            "class {}(TypedDict, total=False):",
                            pascal_variant_name
                        )
                        .unwrap();
                        writeln!(
                            self.output,
                            "    {}: Required[Literal[\"{}\"]]",
                            variant_name, variant_name
                        )
                        .unwrap();
                        self.output.push('\n');
                        variant_class_names.push(pascal_variant_name);
                    }
                    StructKind::TupleStruct if variant.data.fields.len() == 1 => {
                        // Newtype variant - wrapper class pointing to inner type
                        let inner_type = self.type_for_shape(variant.data.fields[0].shape.get());
                        writeln!(
                            self.output,
                            "class {}(TypedDict, total=False):",
                            pascal_variant_name
                        )
                        .unwrap();
                        writeln!(
                            self.output,
                            "    {}: Required[{}]",
                            variant_name, inner_type
                        )
                        .unwrap();
                        self.output.push('\n');
                        variant_class_names.push(pascal_variant_name);
                    }
                    _ => {
                        // Struct variant - generate data class and wrapper class
                        let data_class_name = format!("{}Data", pascal_variant_name);

                        // Generate the data class
                        writeln!(
                            self.output,
                            "class {}(TypedDict, total=False):",
                            data_class_name
                        )
                        .unwrap();

                        if variant.data.fields.is_empty() {
                            self.output.push_str("    pass\n");
                        } else {
                            for field in variant.data.fields {
                                let field_name = field.effective_name();
                                let field_type = self.type_for_shape(field.shape.get());
                                writeln!(
                                    self.output,
                                    "    {}: Required[{}]",
                                    field_name, field_type
                                )
                                .unwrap();
                            }
                        }
                        self.output.push('\n');

                        // Generate the wrapper class
                        writeln!(
                            self.output,
                            "class {}(TypedDict, total=False):",
                            pascal_variant_name
                        )
                        .unwrap();
                        writeln!(
                            self.output,
                            "    {}: Required[{}]",
                            variant_name, data_class_name
                        )
                        .unwrap();
                        self.output.push('\n');

                        variant_class_names.push(pascal_variant_name);
                    }
                }
            }

            // Generate the union type alias
            self.write_doc_comment(shape.doc);
            writeln!(
                self.output,
                "{} = {}",
                shape.type_identifier,
                variant_class_names.join(" | ")
            )
            .unwrap();
        }
        self.output.push('\n');
    }

    fn type_for_shape(&mut self, shape: &'static Shape) -> String {
        // Check Def first - these take precedence over transparent wrappers
        match &shape.def {
            Def::Scalar => self.scalar_type(shape),
            Def::Option(opt) => {
                format!("{} | None", self.type_for_shape(opt.t))
            }
            Def::List(list) => {
                format!("list[{}]", self.type_for_shape(list.t))
            }
            Def::Array(arr) => {
                format!("list[{}]", self.type_for_shape(arr.t))
            }
            Def::Set(set) => {
                format!("list[{}]", self.type_for_shape(set.t))
            }
            Def::Map(map) => {
                format!("dict[str, {}]", self.type_for_shape(map.v))
            }
            Def::Pointer(ptr) => {
                // Smart pointers are transparent
                if let Some(pointee) = ptr.pointee {
                    self.type_for_shape(pointee)
                } else {
                    "Any".to_string()
                }
            }
            Def::Undefined => {
                // User-defined types - queue for generation and return quoted name
                match &shape.ty {
                    Type::User(UserType::Struct(_) | UserType::Enum(_)) => {
                        self.add_shape(shape);
                        format!("\"{}\"", shape.type_identifier)
                    }
                    _ => {
                        // For other undefined types, check if it's a transparent wrapper
                        if let Some(inner) = shape.inner {
                            self.type_for_shape(inner)
                        } else {
                            "Any".to_string()
                        }
                    }
                }
            }
            _ => {
                // For other defs, check if it's a transparent wrapper
                if let Some(inner) = shape.inner {
                    self.type_for_shape(inner)
                } else {
                    "Any".to_string()
                }
            }
        }
    }

    fn scalar_type(&self, shape: &'static Shape) -> String {
        match shape.type_identifier {
            // Strings
            "String" | "str" | "&str" | "Cow" => "str".to_string(),

            // Booleans
            "bool" => "bool".to_string(),

            // Integers
            "u8" | "u16" | "u32" | "u64" | "u128" | "usize" | "i8" | "i16" | "i32" | "i64"
            | "i128" | "isize" => "int".to_string(),

            // Floats
            "f32" | "f64" => "float".to_string(),

            // Char as string
            "char" => "str".to_string(),

            // Unknown scalar
            _ => "Any".to_string(),
        }
    }
}

/// Convert a snake_case or other string to PascalCase.
fn to_pascal_case(s: &str) -> String {
    let mut result = String::new();
    let mut capitalize_next = true;

    for c in s.chars() {
        if c == '_' || c == '-' {
            capitalize_next = true;
        } else if capitalize_next {
            result.push(c.to_ascii_uppercase());
            capitalize_next = false;
        } else {
            result.push(c);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet::Facet;

    #[test]
    fn test_simple_struct() {
        #[derive(Facet)]
        struct User {
            name: String,
            age: u32,
        }

        let py = to_python::<User>();
        insta::assert_snapshot!(py);
    }

    #[test]
    fn test_optional_field() {
        #[derive(Facet)]
        struct Config {
            required: String,
            optional: Option<String>,
        }

        let py = to_python::<Config>();
        insta::assert_snapshot!(py);
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

        let py = to_python::<Status>();
        insta::assert_snapshot!(py);
    }

    #[test]
    fn test_vec() {
        #[derive(Facet)]
        struct Data {
            items: Vec<String>,
        }

        let py = to_python::<Data>();
        insta::assert_snapshot!(py);
    }

    #[test]
    fn test_nested_types() {
        #[derive(Facet)]
        struct Inner {
            value: i32,
        }

        #[derive(Facet)]
        struct Outer {
            inner: Inner,
            name: String,
        }

        let py = to_python::<Outer>();
        insta::assert_snapshot!(py);
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

        let py = to_python::<ValidationErrorCode>();
        insta::assert_snapshot!(py);
    }

    #[test]
    fn test_enum_rename_individual() {
        #[derive(Facet)]
        #[repr(u8)]
        enum GitStatus {
            #[facet(rename = "dirty")]
            Dirty,
            #[facet(rename = "staged")]
            Staged,
            #[facet(rename = "clean")]
            Clean,
        }

        let py = to_python::<GitStatus>();
        insta::assert_snapshot!(py);
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

        let py = to_python::<ApiResponse>();
        insta::assert_snapshot!(py);
    }

    #[test]
    fn test_struct_rename_individual() {
        #[derive(Facet)]
        struct UserProfile {
            #[facet(rename = "userName")]
            user_name: String,
            #[facet(rename = "emailAddress")]
            email: String,
        }

        let py = to_python::<UserProfile>();
        insta::assert_snapshot!(py);
    }

    #[test]
    fn test_enum_with_data_rename_all() {
        #[derive(Facet)]
        #[facet(rename_all = "snake_case")]
        #[repr(C)]
        #[allow(dead_code)]
        enum Message {
            TextMessage { content: String },
            ImageUpload { url: String, width: u32 },
        }

        let py = to_python::<Message>();
        insta::assert_snapshot!(py);
    }
}
