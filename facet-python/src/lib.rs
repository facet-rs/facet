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
//! let py = to_python::<User>(false);
//! assert!(py.contains("class User(TypedDict"));
//! ```

extern crate alloc;

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

use facet_core::{Def, Facet, Field, Shape, StructKind, Type, UserType};

/// Check if a field name is a Python reserved keyword using binary search
fn is_python_keyword(name: &str) -> bool {
    // Python reserved keywords - MUST be sorted alphabetically for binary search
    const KEYWORDS: &[&str] = &[
        "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class",
        "continue", "def", "del", "elif", "else", "except", "finally", "for", "from", "global",
        "if", "import", "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise", "return",
        "try", "while", "with", "yield",
    ];
    KEYWORDS.binary_search(&name).is_ok()
}

/// A field in a TypedDict, used for shared generation logic.
struct TypedDictField<'a> {
    name: &'a str,
    type_string: String,
    required: bool,
    doc: &'a [&'a str],
}

impl<'a> TypedDictField<'a> {
    fn new(name: &'a str, type_string: String, required: bool) -> Self {
        Self {
            name,
            type_string,
            required,
            doc: &[],
        }
    }

    fn with_doc(name: &'a str, type_string: String, required: bool, doc: &'a [&'a str]) -> Self {
        Self {
            name,
            type_string,
            required,
            doc,
        }
    }

    /// Get the full type string with Required[] wrapper if needed
    fn full_type_string(&self) -> String {
        if self.required {
            format!("Required[{}]", self.type_string)
        } else {
            self.type_string.clone()
        }
    }
}

/// Check if any field has a name that is a Python reserved keyword
fn has_reserved_keyword_field(fields: &[TypedDictField]) -> bool {
    fields.iter().any(|f| is_python_keyword(f.name))
}

/// Generate TypedDict using functional syntax: `Name = TypedDict("Name", {...}, total=False)`
///
/// This syntax is required when field names are Python reserved keywords.
fn write_typed_dict_functional(output: &mut String, class_name: &str, fields: &[TypedDictField]) {
    writeln!(output, "{} = TypedDict(", class_name).unwrap();
    writeln!(output, "    \"{}\",", class_name).unwrap();
    output.push_str("    {");

    let mut first = true;
    for field in fields {
        if !first {
            output.push_str(", ");
        }
        first = false;

        write!(output, "\"{}\": {}", field.name, field.full_type_string()).unwrap();
    }

    output.push_str("},\n");
    output.push_str("    total=False,\n");
    output.push(')');
}

/// Generate TypedDict using class syntax: `class Name(TypedDict, total=False): ...`
///
/// This is the preferred syntax for readability when no field names conflict with keywords.
fn write_typed_dict_class(output: &mut String, class_name: &str, fields: &[TypedDictField]) {
    writeln!(output, "class {}(TypedDict, total=False):", class_name).unwrap();

    if fields.is_empty() {
        output.push_str("    pass");
        return;
    }

    for field in fields {
        // Generate doc comment for field
        for line in field.doc {
            output.push_str("    #");
            output.push_str(line);
            output.push('\n');
        }

        writeln!(output, "    {}: {}", field.name, field.full_type_string()).unwrap();
    }
}

/// Generate a TypedDict, choosing between class and functional syntax.
///
/// Uses functional syntax when any field name is a Python reserved keyword,
/// otherwise uses the more readable class syntax.
fn write_typed_dict(output: &mut String, class_name: &str, fields: &[TypedDictField]) {
    if has_reserved_keyword_field(fields) {
        write_typed_dict_functional(output, class_name, fields);
    } else {
        write_typed_dict_class(output, class_name, fields);
    }
}

/// Generate Python definitions for a single type.
///
/// Returns a string containing the Python TypedDict or type declaration.
pub fn to_python<T: Facet<'static>>(write_imports: bool) -> String {
    let mut generator = PythonGenerator::new();
    generator.add_shape(T::SHAPE);
    generator.finish(write_imports)
}

/// Generator for Python type definitions.
///
/// Use this when you need to generate multiple related types.
pub struct PythonGenerator {
    /// Generated type definitions, keyed by type name for sorting
    generated: BTreeMap<String, String>,
    /// Types queued for generation
    queue: Vec<&'static Shape>,
    /// Typing imports used (Any, Literal, Required, TypedDict)
    imports: BTreeSet<&'static str>,
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
            generated: BTreeMap::new(),
            queue: Vec::new(),
            imports: BTreeSet::new(),
        }
    }

    /// Add a type to generate.
    pub fn add_type<T: Facet<'static>>(&mut self) {
        self.add_shape(T::SHAPE);
    }

    /// Add a shape to generate.
    pub fn add_shape(&mut self, shape: &'static Shape) {
        if !self.generated.contains_key(shape.type_identifier) {
            self.queue.push(shape);
        }
    }

    /// Finish generation and return the Python code.
    ///
    /// If `write_imports` is true, a `from typing import ...` line will be
    /// prepended with only the imports actually used in the generated code.
    pub fn finish(mut self, write_imports: bool) -> String {
        // Process queue until empty
        while let Some(shape) = self.queue.pop() {
            if self.generated.contains_key(shape.type_identifier) {
                continue;
            }
            // Insert a placeholder to mark as "being generated"
            self.generated
                .insert(shape.type_identifier.to_string(), String::new());
            self.generate_shape(shape);
        }

        // Collect all generated code in sorted order (BTreeMap iterates in key order)
        let mut output = String::new();

        // Write imports if requested and any were used
        if write_imports && !self.imports.is_empty() {
            let imports: Vec<&str> = self.imports.iter().copied().collect();
            writeln!(output, "from typing import {}", imports.join(", ")).unwrap();
            output.push('\n');
        }

        for code in self.generated.values() {
            output.push_str(code);
        }
        output
    }

    fn generate_shape(&mut self, shape: &'static Shape) {
        let mut output = String::new();

        // Handle transparent wrappers - generate a type alias to the inner type
        if let Some(inner) = shape.inner {
            self.add_shape(inner);
            let inner_type = self.type_for_shape(inner);
            write_doc_comment(&mut output, shape.doc);
            writeln!(output, "type {} = {}", shape.type_identifier, inner_type).unwrap();
            output.push('\n');
            self.generated
                .insert(shape.type_identifier.to_string(), output);
            return;
        }

        match &shape.ty {
            Type::User(UserType::Struct(st)) => {
                self.generate_struct(&mut output, shape, st.fields, st.kind);
            }
            Type::User(UserType::Enum(en)) => {
                self.generate_enum(&mut output, shape, en);
            }
            _ => {
                // For other types, generate a type alias
                let type_str = self.type_for_shape(shape);
                write_doc_comment(&mut output, shape.doc);
                writeln!(output, "type {} = {}", shape.type_identifier, type_str).unwrap();
                output.push('\n');
            }
        }

        self.generated
            .insert(shape.type_identifier.to_string(), output);
    }

    fn generate_struct(
        &mut self,
        output: &mut String,
        shape: &'static Shape,
        fields: &'static [Field],
        kind: StructKind,
    ) {
        match kind {
            StructKind::Unit => {
                write_doc_comment(output, shape.doc);
                writeln!(output, "{} = None", shape.type_identifier).unwrap();
            }
            StructKind::TupleStruct | StructKind::Tuple if fields.is_empty() => {
                // Empty tuple struct like `struct Empty();` - treat like unit struct
                write_doc_comment(output, shape.doc);
                writeln!(output, "{} = None", shape.type_identifier).unwrap();
            }
            StructKind::TupleStruct if fields.len() == 1 => {
                let inner_type = self.type_for_shape(fields[0].shape.get());
                write_doc_comment(output, shape.doc);
                writeln!(output, "{} = {}", shape.type_identifier, inner_type).unwrap();
            }
            StructKind::TupleStruct | StructKind::Tuple => {
                let types: Vec<String> = fields
                    .iter()
                    .map(|f| self.type_for_shape(f.shape.get()))
                    .collect();
                write_doc_comment(output, shape.doc);
                writeln!(
                    output,
                    "{} = tuple[{}]",
                    shape.type_identifier,
                    types.join(", ")
                )
                .unwrap();
            }
            StructKind::Struct => {
                self.generate_typed_dict(output, shape, fields);
            }
        }
        output.push('\n');
    }

    /// Generate a TypedDict for a struct, choosing between class and functional syntax.
    ///
    /// Uses functional syntax when any field name is a Python reserved keyword,
    /// otherwise uses the more readable class syntax.
    fn generate_typed_dict(
        &mut self,
        output: &mut String,
        shape: &'static Shape,
        fields: &'static [Field],
    ) {
        self.imports.insert("TypedDict");

        let visible_fields: Vec<_> = fields
            .iter()
            .filter(|f| !f.flags.contains(facet_core::FieldFlags::SKIP))
            .collect();

        // Convert to TypedDictField for shared generation logic
        let typed_dict_fields: Vec<_> = visible_fields
            .iter()
            .map(|f| {
                let (type_string, required) = self.field_type_info(f);
                TypedDictField::with_doc(f.effective_name(), type_string, required, f.doc)
            })
            .collect();

        // Track Required import if any field needs it
        if typed_dict_fields.iter().any(|f| f.required) {
            self.imports.insert("Required");
        }

        write_doc_comment(output, shape.doc);
        write_typed_dict(output, shape.type_identifier, &typed_dict_fields);
    }

    /// Get the Python type string and required status for a field.
    fn field_type_info(&mut self, field: &Field) -> (String, bool) {
        if let Def::Option(opt) = &field.shape.get().def {
            (self.type_for_shape(opt.t), false)
        } else {
            (self.type_for_shape(field.shape.get()), true)
        }
    }

    fn generate_enum(
        &mut self,
        output: &mut String,
        shape: &'static Shape,
        enum_type: &facet_core::EnumType,
    ) {
        let all_unit = enum_type
            .variants
            .iter()
            .all(|v| matches!(v.data.kind, StructKind::Unit));

        write_doc_comment(output, shape.doc);

        if all_unit {
            self.generate_enum_unit_variants(output, shape, enum_type);
        } else {
            self.generate_enum_with_data(output, shape, enum_type);
        }
        output.push('\n');
    }

    /// Generate a simple enum where all variants are unit variants.
    ///
    /// Produces: `EnumName = Union[Literal["A"], Literal["B"], ...]`
    fn generate_enum_unit_variants(
        &mut self,
        output: &mut String,
        shape: &'static Shape,
        enum_type: &facet_core::EnumType,
    ) {
        self.imports.insert("Literal");
        self.imports.insert("Union");

        let variants: Vec<String> = enum_type
            .variants
            .iter()
            .map(|v| format!("Literal[\"{}\"]", v.effective_name()))
            .collect();

        writeln!(
            output,
            "{} = Union[{}]",
            shape.type_identifier,
            variants.join(", ")
        )
        .unwrap();
    }

    /// Generate an enum with data variants (discriminated union).
    ///
    /// Generates TypedDict wrapper classes for each variant, then unions them.
    fn generate_enum_with_data(
        &mut self,
        output: &mut String,
        shape: &'static Shape,
        enum_type: &facet_core::EnumType,
    ) {
        let mut variant_class_names = Vec::new();

        for variant in enum_type.variants {
            let variant_type_name = self.generate_enum_variant(variant);
            variant_class_names.push(variant_type_name);
        }

        self.imports.insert("Union");
        writeln!(
            output,
            "{} = Union[{}]",
            shape.type_identifier,
            variant_class_names.join(", ")
        )
        .unwrap();
    }

    /// Generate a single enum variant and return its type reference.
    ///
    /// Returns either `Literal["name"]` for unit variants or `"ClassName"` for data variants.
    fn generate_enum_variant(&mut self, variant: &facet_core::Variant) -> String {
        let variant_name = variant.effective_name();
        let pascal_variant_name = to_pascal_case(variant_name);

        match variant.data.kind {
            StructKind::Unit => {
                self.imports.insert("Literal");
                format!("Literal[\"{}\"]", variant_name)
            }
            StructKind::TupleStruct if variant.data.fields.len() == 1 => {
                self.generate_newtype_variant(variant_name, &pascal_variant_name, variant);
                format!("\"{}\"", pascal_variant_name)
            }
            _ => {
                self.generate_struct_variant(variant_name, &pascal_variant_name, variant);
                format!("\"{}\"", pascal_variant_name)
            }
        }
    }

    /// Generate a newtype variant (single-field tuple variant).
    ///
    /// Produces a wrapper class: `class VariantName(TypedDict): variant_name: Required[InnerType]`
    fn generate_newtype_variant(
        &mut self,
        variant_name: &str,
        pascal_variant_name: &str,
        variant: &facet_core::Variant,
    ) {
        self.imports.insert("TypedDict");
        self.imports.insert("Required");

        let inner_type = self.type_for_shape(variant.data.fields[0].shape.get());

        let fields = [TypedDictField::new(variant_name, inner_type, true)];

        let mut output = String::new();
        write_typed_dict(&mut output, pascal_variant_name, &fields);
        output.push('\n');

        self.generated
            .insert(pascal_variant_name.to_string(), output);
    }

    /// Generate a struct variant (multiple fields or named fields).
    ///
    /// Produces a data class and a wrapper class:
    /// - `class VariantNameData(TypedDict): field1: Type1, ...`
    /// - `class VariantName(TypedDict): variant_name: Required[VariantNameData]`
    fn generate_struct_variant(
        &mut self,
        variant_name: &str,
        pascal_variant_name: &str,
        variant: &facet_core::Variant,
    ) {
        self.imports.insert("TypedDict");
        self.imports.insert("Required");

        let data_class_name = format!("{}Data", pascal_variant_name);

        // Generate the data class fields
        let data_fields: Vec<_> = variant
            .data
            .fields
            .iter()
            .map(|field| {
                let field_type = self.type_for_shape(field.shape.get());
                TypedDictField::new(field.effective_name(), field_type, true)
            })
            .collect();

        let mut data_output = String::new();
        write_typed_dict(&mut data_output, &data_class_name, &data_fields);
        data_output.push('\n');
        self.generated.insert(data_class_name.clone(), data_output);

        // Generate the wrapper class
        let wrapper_fields = [TypedDictField::new(variant_name, data_class_name, true)];

        let mut wrapper_output = String::new();
        write_typed_dict(&mut wrapper_output, pascal_variant_name, &wrapper_fields);
        wrapper_output.push('\n');

        self.generated
            .insert(pascal_variant_name.to_string(), wrapper_output);
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
                format!(
                    "dict[{}, {}]",
                    self.type_for_shape(map.k),
                    self.type_for_shape(map.v)
                )
            }
            Def::Pointer(ptr) => {
                // Smart pointers are transparent
                if let Some(pointee) = ptr.pointee {
                    self.type_for_shape(pointee)
                } else {
                    self.imports.insert("Any");
                    "Any".to_string()
                }
            }
            Def::Undefined => {
                // User-defined types - queue for generation and return quoted name
                match &shape.ty {
                    Type::User(UserType::Struct(st)) => {
                        // Handle tuples specially - inline them as tuple[...] since their
                        // type_identifier "(…)" is not a valid Python identifier
                        if st.kind == StructKind::Tuple {
                            let types: Vec<String> = st
                                .fields
                                .iter()
                                .map(|f| self.type_for_shape(f.shape.get()))
                                .collect();
                            format!("tuple[{}]", types.join(", "))
                        } else {
                            self.add_shape(shape);
                            format!("\"{}\"", shape.type_identifier)
                        }
                    }
                    Type::User(UserType::Enum(_)) => {
                        self.add_shape(shape);
                        format!("\"{}\"", shape.type_identifier)
                    }
                    _ => {
                        // For other undefined types, check if it's a transparent wrapper
                        if let Some(inner) = shape.inner {
                            self.type_for_shape(inner)
                        } else {
                            self.imports.insert("Any");
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
                    self.imports.insert("Any");
                    "Any".to_string()
                }
            }
        }
    }
}

/// Write a doc comment to the output.
fn write_doc_comment(output: &mut String, doc: &[&str]) {
    if !doc.is_empty() {
        for line in doc {
            output.push('#');
            output.push_str(line);
            output.push('\n');
        }
    }
}

/// Get the Python type for a scalar shape.
impl PythonGenerator {
    /// Get the Python type for a scalar shape.
    fn scalar_type(&mut self, shape: &'static Shape) -> String {
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
            _ => {
                self.imports.insert("Any");
                "Any".to_string()
            }
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

        let py = to_python::<User>(false);
        insta::assert_snapshot!(py);
    }

    #[test]
    fn test_optional_field() {
        #[derive(Facet)]
        struct Config {
            required: String,
            optional: Option<String>,
        }

        let py = to_python::<Config>(false);
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

        let py = to_python::<Status>(false);
        insta::assert_snapshot!(py);
    }

    #[test]
    fn test_vec() {
        #[derive(Facet)]
        struct Data {
            items: Vec<String>,
        }

        let py = to_python::<Data>(false);
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

        let py = to_python::<Outer>(false);
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

        let py = to_python::<ValidationErrorCode>(false);
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

        let py = to_python::<GitStatus>(false);
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

        let py = to_python::<ApiResponse>(false);
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

        let py = to_python::<UserProfile>(false);
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

        let py = to_python::<Message>(false);
        insta::assert_snapshot!(py);
    }

    #[test]
    fn test_unit_struct() {
        #[derive(Facet)]
        struct Empty;

        let py = to_python::<Empty>(false);
        insta::assert_snapshot!(py);
    }

    #[test]
    fn test_tuple_struct() {
        #[derive(Facet)]
        struct Point(f32, f64);

        let py = to_python::<Point>(false);
        insta::assert_snapshot!(py);
    }

    #[test]
    fn test_newtype_struct() {
        #[derive(Facet)]
        struct UserId(u64);

        let py = to_python::<UserId>(false);
        insta::assert_snapshot!(py);
    }

    #[test]
    fn test_hashmap() {
        use std::collections::HashMap;

        #[derive(Facet)]
        struct Registry {
            entries: HashMap<String, i32>,
        }

        let py = to_python::<Registry>(false);
        insta::assert_snapshot!(py);
    }

    #[test]
    fn test_mixed_enum_variants() {
        #[derive(Facet)]
        #[repr(C)]
        #[allow(dead_code)]
        enum Event {
            /// Unit variant
            Empty,
            /// Newtype variant
            Id(u64),
            /// Struct variant
            Data { name: String, value: f64 },
        }

        let py = to_python::<Event>(false);
        insta::assert_snapshot!(py);
    }

    #[test]
    fn test_with_imports() {
        #[derive(Facet)]
        struct User {
            name: String,
            age: u32,
        }

        let py = to_python::<User>(true);
        insta::assert_snapshot!(py);
    }

    #[test]
    fn test_enum_with_imports() {
        #[derive(Facet)]
        #[repr(u8)]
        enum Status {
            Active,
            Inactive,
        }

        let py = to_python::<Status>(true);
        insta::assert_snapshot!(py);
    }

    #[test]
    fn test_transparent_wrapper() {
        #[derive(Facet)]
        #[facet(transparent)]
        struct UserId(String);

        let py = to_python::<UserId>(false);
        // This should generate "type UserId = str" not "UserId = str"
        insta::assert_snapshot!(py);
    }

    #[test]
    fn test_transparent_wrapper_with_inner_type() {
        #[derive(Facet)]
        struct Inner {
            value: i32,
        }

        #[derive(Facet)]
        #[facet(transparent)]
        struct Wrapper(Inner);

        let py = to_python::<Wrapper>(false);
        // This should generate "type Wrapper = Inner" not "Wrapper = Inner"
        insta::assert_snapshot!(py);
    }

    #[test]
    fn test_struct_with_tuple_field() {
        #[derive(Facet)]
        struct Container {
            /// A tuple field containing coordinates
            coordinates: (i32, i32),
        }

        let py = to_python::<Container>(false);
        // This should NOT generate "(…)" as a type - it should properly expand the tuple
        insta::assert_snapshot!(py);
    }

    #[test]
    fn test_struct_with_reserved_keyword_field() {
        #[derive(Facet)]
        struct TradeOrder {
            from: f64,
            to: f64,
            quantity: f64,
        }

        let py = to_python::<TradeOrder>(false);
        // This should use functional TypedDict syntax since "from" is a Python keyword
        insta::assert_snapshot!(py);
    }

    #[test]
    fn test_struct_with_multiple_reserved_keywords() {
        #[derive(Facet)]
        struct ControlFlow {
            r#if: bool,
            r#else: String,
            r#return: i32,
        }

        let py = to_python::<ControlFlow>(false);
        // Multiple Python keywords - should use functional syntax
        insta::assert_snapshot!(py);
    }

    #[test]
    fn test_enum_variant_name_is_reserved_keyword() {
        #[derive(Facet)]
        #[repr(C)]
        #[facet(rename_all = "snake_case")]
        #[allow(dead_code)]
        enum ImportSource {
            /// Import from a file
            From(String),
            /// Import from a URL
            Url(String),
        }

        let py = to_python::<ImportSource>(false);
        // The variant "From" becomes field name "from" which is a Python keyword
        // Should use functional TypedDict syntax for the wrapper class
        insta::assert_snapshot!(py);
    }

    #[test]
    fn test_enum_data_variant_with_reserved_keyword_field() {
        #[derive(Facet)]
        #[repr(C)]
        #[allow(dead_code)]
        enum Transfer {
            /// A transfer between accounts
            Move {
                from: String,
                to: String,
                amount: f64,
            },
            /// Cancel the transfer
            Cancel,
        }

        let py = to_python::<Transfer>(false);
        // The data variant "Move" has fields "from" and "to" which are Python keywords
        // Should use functional TypedDict syntax for the data class
        insta::assert_snapshot!(py);
    }

    #[test]
    fn test_hashmap_with_integer_keys() {
        use std::collections::HashMap;

        #[derive(Facet)]
        struct IntKeyedMap {
            /// Map with integer keys
            counts: HashMap<i32, String>,
        }

        let py = to_python::<IntKeyedMap>(false);
        insta::assert_snapshot!(py);
    }

    #[test]
    fn test_empty_tuple_struct() {
        #[derive(Facet)]
        struct EmptyTuple();

        let py = to_python::<EmptyTuple>(false);
        insta::assert_snapshot!(py);
    }

    #[test]
    fn test_hashmap_with_enum_keys() {
        use std::collections::HashMap;

        #[derive(Facet, Hash, PartialEq, Eq)]
        #[repr(u8)]
        enum Priority {
            Low,
            Medium,
            High,
        }

        #[derive(Facet)]
        struct TaskMap {
            tasks: HashMap<Priority, String>,
        }

        let py = to_python::<TaskMap>(false);
        insta::assert_snapshot!(py);
    }
}
