//! Generate TypeScript type definitions from facet type metadata.
//!
//! This crate uses facet's reflection capabilities to generate TypeScript
//! interfaces and types from any type that implements `Facet`.
//!
//! # Example
//!
//! ```
//! use facet::Facet;
//! use facet_typescript::to_typescript;
//!
//! #[derive(Facet)]
//! struct User {
//!     name: String,
//!     age: u32,
//!     email: Option<String>,
//! }
//!
//! let ts = to_typescript::<User>();
//! assert!(ts.contains("export interface User"));
//! ```

extern crate alloc;

use alloc::collections::BTreeSet;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

use facet_core::{Def, Facet, Field, Shape, StructKind, Type, UserType};

/// Generate TypeScript definitions for a single type.
///
/// Returns a string containing the TypeScript interface or type declaration.
pub fn to_typescript<T: Facet<'static>>() -> String {
    let mut generator = TypeScriptGenerator::new();
    generator.add_shape(T::SHAPE);
    generator.finish()
}

/// Generator for TypeScript type definitions.
///
/// Use this when you need to generate multiple related types.
pub struct TypeScriptGenerator {
    output: String,
    /// Types already generated (by type identifier)
    generated: BTreeSet<&'static str>,
    /// Types queued for generation
    queue: Vec<&'static Shape>,
    /// Indentation level
    indent: usize,
}

impl Default for TypeScriptGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl TypeScriptGenerator {
    /// Create a new TypeScript generator.
    pub const fn new() -> Self {
        Self {
            output: String::new(),
            generated: BTreeSet::new(),
            queue: Vec::new(),
            indent: 0,
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

    /// Finish generation and return the TypeScript code.
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

    fn write_indent(&mut self) {
        for _ in 0..self.indent {
            self.output.push_str("  ");
        }
    }

    fn generate_shape(&mut self, shape: &'static Shape) {
        // Handle transparent wrappers - generate the inner type instead
        if let Some(inner) = shape.inner {
            self.add_shape(inner);
            // Generate a type alias
            let inner_type = self.type_for_shape(inner);
            writeln!(
                self.output,
                "export type {} = {};",
                shape.type_identifier, inner_type
            )
            .unwrap();
            self.output.push('\n');
            return;
        }

        // Generate doc comment if present
        if !shape.doc.is_empty() {
            self.output.push_str("/**\n");
            for line in shape.doc {
                self.output.push_str(" *");
                self.output.push_str(line);
                self.output.push('\n');
            }
            self.output.push_str(" */\n");
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
                writeln!(
                    self.output,
                    "export type {} = {};",
                    shape.type_identifier, type_str
                )
                .unwrap();
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
                // Unit struct as null
                writeln!(self.output, "export type {} = null;", shape.type_identifier).unwrap();
            }
            StructKind::TupleStruct if fields.len() == 1 => {
                // Newtype - type alias to inner
                let inner_type = self.type_for_shape(fields[0].shape.get());
                writeln!(
                    self.output,
                    "export type {} = {};",
                    shape.type_identifier, inner_type
                )
                .unwrap();
            }
            StructKind::TupleStruct | StructKind::Tuple => {
                // Tuple as array type
                let types: Vec<String> = fields
                    .iter()
                    .map(|f| self.type_for_shape(f.shape.get()))
                    .collect();
                writeln!(
                    self.output,
                    "export type {} = [{}];",
                    shape.type_identifier,
                    types.join(", ")
                )
                .unwrap();
            }
            StructKind::Struct => {
                writeln!(self.output, "export interface {} {{", shape.type_identifier).unwrap();
                self.indent += 1;

                for field in fields {
                    // Skip fields marked with skip
                    if field.flags.contains(facet_core::FieldFlags::SKIP) {
                        continue;
                    }

                    // Generate doc comment for field
                    if !field.doc.is_empty() {
                        self.write_indent();
                        self.output.push_str("/**\n");
                        for line in field.doc {
                            self.write_indent();
                            self.output.push_str(" *");
                            self.output.push_str(line);
                            self.output.push('\n');
                        }
                        self.write_indent();
                        self.output.push_str(" */\n");
                    }

                    let field_name = field.effective_name();
                    let is_option = matches!(field.shape.get().def, Def::Option(_));

                    self.write_indent();

                    // Use optional marker for Option fields
                    if is_option {
                        // Unwrap the Option to get the inner type
                        if let Def::Option(opt) = &field.shape.get().def {
                            let inner_type = self.type_for_shape(opt.t);
                            writeln!(self.output, "{}?: {};", field_name, inner_type).unwrap();
                        }
                    } else {
                        let field_type = self.type_for_shape(field.shape.get());
                        writeln!(self.output, "{}: {};", field_name, field_type).unwrap();
                    }
                }

                self.indent -= 1;
                self.output.push_str("}\n");
            }
        }
        self.output.push('\n');
    }

    fn generate_enum(&mut self, shape: &'static Shape, enum_type: &facet_core::EnumType) {
        // Check if all variants are unit variants (simple string union)
        let all_unit = enum_type
            .variants
            .iter()
            .all(|v| matches!(v.data.kind, StructKind::Unit));

        if all_unit {
            // Simple string literal union
            let variants: Vec<String> = enum_type
                .variants
                .iter()
                .map(|v| format!("\"{}\"", v.effective_name()))
                .collect();
            writeln!(
                self.output,
                "export type {} = {};",
                shape.type_identifier,
                variants.join(" | ")
            )
            .unwrap();
        } else {
            // Discriminated union
            // Generate each variant as a separate interface, then union them
            let mut variant_types = Vec::new();

            for variant in enum_type.variants {
                let variant_name = variant.effective_name();
                match variant.data.kind {
                    StructKind::Unit => {
                        // Unit variant as object with type discriminator
                        variant_types.push(format!("{{ {}: \"{}\" }}", variant_name, variant_name));
                    }
                    StructKind::TupleStruct if variant.data.fields.len() == 1 => {
                        // Newtype variant: { VariantName: InnerType }
                        let inner = self.type_for_shape(variant.data.fields[0].shape.get());
                        variant_types.push(format!("{{ {}: {} }}", variant_name, inner));
                    }
                    _ => {
                        // Struct variant: { VariantName: { ...fields } }
                        let mut field_types = Vec::new();
                        for field in variant.data.fields {
                            let field_name = field.effective_name();
                            let field_type = self.type_for_shape(field.shape.get());
                            field_types.push(format!("{}: {}", field_name, field_type));
                        }
                        variant_types.push(format!(
                            "{{ {}: {{ {} }} }}",
                            variant_name,
                            field_types.join("; ")
                        ));
                    }
                }
            }

            writeln!(
                self.output,
                "export type {} =\n  | {};",
                shape.type_identifier,
                variant_types.join("\n  | ")
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
                format!("{} | null", self.type_for_shape(opt.t))
            }
            Def::List(list) => {
                format!("{}[]", self.type_for_shape(list.t))
            }
            Def::Array(arr) => {
                format!("{}[]", self.type_for_shape(arr.t))
            }
            Def::Set(set) => {
                format!("{}[]", self.type_for_shape(set.t))
            }
            Def::Map(map) => {
                format!("Record<string, {}>", self.type_for_shape(map.v))
            }
            Def::Pointer(ptr) => {
                // Smart pointers are transparent
                if let Some(pointee) = ptr.pointee {
                    self.type_for_shape(pointee)
                } else {
                    "unknown".to_string()
                }
            }
            Def::Undefined => {
                // User-defined types - queue for generation and return name
                match &shape.ty {
                    Type::User(UserType::Struct(_) | UserType::Enum(_)) => {
                        self.add_shape(shape);
                        shape.type_identifier.to_string()
                    }
                    _ => {
                        // For other undefined types, check if it's a transparent wrapper
                        if let Some(inner) = shape.inner {
                            self.type_for_shape(inner)
                        } else {
                            "unknown".to_string()
                        }
                    }
                }
            }
            _ => {
                // For other defs, check if it's a transparent wrapper
                if let Some(inner) = shape.inner {
                    self.type_for_shape(inner)
                } else {
                    "unknown".to_string()
                }
            }
        }
    }

    fn scalar_type(&self, shape: &'static Shape) -> String {
        match shape.type_identifier {
            // Strings
            "String" | "str" | "&str" | "Cow" => "string".to_string(),

            // Booleans
            "bool" => "boolean".to_string(),

            // Numbers (all become number in TypeScript)
            "u8" | "u16" | "u32" | "u64" | "u128" | "usize" | "i8" | "i16" | "i32" | "i64"
            | "i128" | "isize" | "f32" | "f64" => "number".to_string(),

            // Char as string
            "char" => "string".to_string(),

            // Unknown scalar
            _ => "unknown".to_string(),
        }
    }
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

        let ts = to_typescript::<User>();
        insta::assert_snapshot!(ts);
    }

    #[test]
    fn test_optional_field() {
        #[derive(Facet)]
        struct Config {
            required: String,
            optional: Option<String>,
        }

        let ts = to_typescript::<Config>();
        insta::assert_snapshot!(ts);
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

        let ts = to_typescript::<Status>();
        insta::assert_snapshot!(ts);
    }

    #[test]
    fn test_vec() {
        #[derive(Facet)]
        struct Data {
            items: Vec<String>,
        }

        let ts = to_typescript::<Data>();
        insta::assert_snapshot!(ts);
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

        let ts = to_typescript::<Outer>();
        insta::assert_snapshot!(ts);
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

        let ts = to_typescript::<ValidationErrorCode>();
        insta::assert_snapshot!(ts);
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

        let ts = to_typescript::<GitStatus>();
        insta::assert_snapshot!(ts);
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

        let ts = to_typescript::<ApiResponse>();
        insta::assert_snapshot!(ts);
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

        let ts = to_typescript::<UserProfile>();
        insta::assert_snapshot!(ts);
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

        let ts = to_typescript::<Message>();
        insta::assert_snapshot!(ts);
    }
}
