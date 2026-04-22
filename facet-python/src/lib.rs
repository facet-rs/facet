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
    fn new(name: &'a str, type_string: String, required: bool, doc: &'a [&'a str]) -> Self {
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
fn write_typed_dict(output: &mut String, class_name: &str, fields: &[TypedDictField]) {
    if has_reserved_keyword_field(fields) {
        write_typed_dict_functional(output, class_name, fields);
    } else {
        write_typed_dict_class(output, class_name, fields);
    }
}

/// Generate Python definitions for a single type.
pub fn to_python<T: Facet<'static>>(write_imports: bool) -> String {
    let mut generator = PythonGenerator::new();
    generator.add_shape(T::SHAPE);
    generator.finish(write_imports)
}

/// Generator for Python type definitions.
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
        // Invariant: we must generate in lexia order to ensure that forward references are quoted correctly
        let mut output = String::new();

        // Write imports if requested
        if write_imports {
            // Always emit __future__ annotations for postponed evaluation
            // This allows forward references and | syntax without runtime issues
            writeln!(output, "from __future__ import annotations").unwrap();

            if !self.imports.is_empty() {
                let imports: Vec<&str> = self.imports.iter().copied().collect();
                writeln!(output, "from typing import {}", imports.join(", ")).unwrap();
            }
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
            let inner_type = self.type_for_shape(inner, None);
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
                let type_str = self.type_for_shape(shape, None);
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
                let inner_type = self.type_for_shape(fields[0].shape.get(), None);
                write_doc_comment(output, shape.doc);
                writeln!(output, "{} = {}", shape.type_identifier, inner_type).unwrap();
            }
            StructKind::TupleStruct | StructKind::Tuple => {
                let types: Vec<String> = fields
                    .iter()
                    .map(|f| self.type_for_shape(f.shape.get(), None))
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
    fn generate_typed_dict(
        &mut self,
        output: &mut String,
        shape: &'static Shape,
        fields: &'static [Field],
    ) {
        self.imports.insert("TypedDict");

        // Collect fields, recursively inlining any #[facet(flatten)] fields.
        let all_fields = self.collect_flat_fields(fields);

        // Functional form uses runtime expressions — quote forward references.
        // Check all fields, including those inlined from flattened structs.
        let needs_functional = all_fields
            .iter()
            .any(|(f, _)| is_python_keyword(f.effective_name()));
        let quote_after: Option<&str> = if needs_functional {
            Some(shape.type_identifier)
        } else {
            None
        };

        // Convert to TypedDictField for shared generation logic
        let typed_dict_fields: Vec<_> = all_fields
            .iter()
            .map(|(f, force_optional)| {
                let (type_string, required) = self.field_type_info(f, quote_after);
                let required = required && !force_optional;
                TypedDictField::new(f.effective_name(), type_string, required, f.doc)
            })
            .collect();

        // Track Required import if any field needs it
        if typed_dict_fields.iter().any(|f| f.required) {
            self.imports.insert("Required");
        }

        write_doc_comment(output, shape.doc);
        write_typed_dict(output, shape.type_identifier, &typed_dict_fields);
    }

    /// Unwrap through `Option<T>`, pointers (`Arc<T>`, `Box<T>`), and transparent
    /// wrappers to reach the effective inner shape for flatten purposes.
    ///
    /// Returns `(inner_shape, was_optional)` where `was_optional` is `true` if an
    /// `Option` layer was encountered.
    fn unwrap_to_inner_shape(shape: &'static Shape) -> (&'static Shape, bool) {
        // Option<T> — mark optional and recurse on T.
        if let Def::Option(opt) = &shape.def {
            let (inner, _) = Self::unwrap_to_inner_shape(opt.t);
            return (inner, true);
        }
        // Arc<T>, Box<T>, etc. — unwrap the pointee.
        if let Def::Pointer(ptr) = &shape.def
            && let Some(pointee) = ptr.pointee
        {
            return Self::unwrap_to_inner_shape(pointee);
        }
        // Transparent wrappers (#[facet(transparent)]).
        if let Some(inner) = shape.inner {
            let (inner_shape, is_optional) = Self::unwrap_to_inner_shape(inner);
            return (inner_shape, is_optional);
        }
        (shape, false)
    }

    /// Collect visible fields, inlining `#[facet(flatten)]` ones.
    ///
    /// Each entry is `(field, force_optional)`. `force_optional` is `true` when the
    /// field was inlined from an `Option<Struct>` flatten, meaning the child field
    /// must be treated as optional in the parent regardless of its own shape.
    fn collect_flat_fields(&mut self, fields: &'static [Field]) -> Vec<(&'static Field, bool)> {
        let mut flatten_stack: Vec<&'static str> = Vec::new();
        self.collect_flat_fields_guarded(fields, false, &mut flatten_stack)
    }

    fn collect_flat_fields_guarded(
        &mut self,
        fields: &'static [Field],
        force_optional: bool,
        flatten_stack: &mut Vec<&'static str>,
    ) -> Vec<(&'static Field, bool)> {
        let mut result = Vec::new();
        for field in fields {
            // Covers both #[facet(skip)] and #[facet(skip_serializing)].
            if field.should_skip_serializing_unconditional() {
                continue;
            }

            if field.is_flattened() {
                // Unwrap Option/pointer/transparent layers to reach the struct shape.
                let (inner_shape, parent_is_optional) =
                    Self::unwrap_to_inner_shape(field.shape.get());

                // Queue the struct itself (not any Option/pointer wrapper) so it
                // gets its own TypedDict if referenced elsewhere.
                self.add_shape(inner_shape);

                if let Type::User(UserType::Struct(st)) = &inner_shape.ty {
                    // Cycle guard: skip self-referential shapes.
                    let key = inner_shape.type_identifier;
                    if flatten_stack.contains(&key) {
                        continue;
                    }
                    flatten_stack.push(key);
                    let inner = self.collect_flat_fields_guarded(
                        st.fields,
                        force_optional || parent_is_optional,
                        flatten_stack,
                    );
                    result.extend(inner);
                    flatten_stack.pop();
                } else {
                    // Non-struct flatten (e.g. a map) — emit as a regular field.
                    result.push((field, force_optional));
                }
            } else {
                result.push((field, force_optional));
            }
        }
        result
    }

    /// Get the Python type string and required status for a field.
    fn field_type_info(&mut self, field: &Field, quote_after: Option<&str>) -> (String, bool) {
        if let Def::Option(opt) = &field.shape.get().def {
            (self.type_for_shape(opt.t, quote_after), false)
        } else {
            // Fields with a default value are optional in JSON — facet fills in
            // the default when the key is absent. Matches facet-typescript behaviour.
            let required = field.default.is_none();
            (
                self.type_for_shape(field.shape.get(), quote_after),
                required,
            )
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
    fn generate_enum_unit_variants(
        &mut self,
        output: &mut String,
        shape: &'static Shape,
        enum_type: &facet_core::EnumType,
    ) {
        self.imports.insert("Literal");

        let variants: Vec<String> = enum_type
            .variants
            .iter()
            .map(|v| format!("Literal[\"{}\"]", v.effective_name()))
            .collect();

        writeln!(
            output,
            "type {} = {}",
            shape.type_identifier,
            variants.join(" | ")
        )
        .unwrap();
    }

    /// Generate an enum with data variants (discriminated union).
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

        writeln!(
            output,
            "type {} = {}",
            shape.type_identifier,
            variant_class_names.join(" | ")
        )
        .unwrap();
    }

    /// Generate a single enum variant and return its type reference.
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
                pascal_variant_name.to_string()
            }
            StructKind::TupleStruct => {
                self.generate_tuple_variant(variant_name, &pascal_variant_name, variant);
                pascal_variant_name.to_string()
            }
            _ => {
                self.generate_struct_variant(variant_name, &pascal_variant_name, variant);
                pascal_variant_name.to_string()
            }
        }
    }

    /// Generate a newtype variant (single-field tuple variant).
    fn generate_newtype_variant(
        &mut self,
        variant_name: &str,
        pascal_variant_name: &str,
        variant: &facet_core::Variant,
    ) {
        self.imports.insert("TypedDict");
        self.imports.insert("Required");

        // Functional form uses runtime expressions — quote forward references.
        let quote_after: Option<&str> = if is_python_keyword(variant_name) {
            Some(pascal_variant_name)
        } else {
            None
        };

        let inner_type = self.type_for_shape(variant.data.fields[0].shape.get(), quote_after);

        let fields = [TypedDictField::new(variant_name, inner_type, true, &[])];

        let mut output = String::new();
        write_typed_dict(&mut output, pascal_variant_name, &fields);
        output.push('\n');

        self.generated
            .insert(pascal_variant_name.to_string(), output);
    }

    /// Generate a tuple variant (multiple fields).
    fn generate_tuple_variant(
        &mut self,
        variant_name: &str,
        pascal_variant_name: &str,
        variant: &facet_core::Variant,
    ) {
        self.imports.insert("TypedDict");
        self.imports.insert("Required");

        // Functional form uses runtime expressions — quote forward references.
        let quote_after: Option<&str> = if is_python_keyword(variant_name) {
            Some(pascal_variant_name)
        } else {
            None
        };

        let types: Vec<String> = variant
            .data
            .fields
            .iter()
            .map(|f| self.type_for_shape(f.shape.get(), quote_after))
            .collect();

        // Note: types should never be empty here because:
        // - Single-field tuple structs are handled by generate_newtype_variant
        // - Zero-field tuple variants (e.g., A()) fail to compile in the derive macro
        let inner_type = format!("tuple[{}]", types.join(", "));

        let fields = [TypedDictField::new(variant_name, inner_type, true, &[])];

        let mut output = String::new();
        write_typed_dict(&mut output, pascal_variant_name, &fields);
        output.push('\n');

        self.generated
            .insert(pascal_variant_name.to_string(), output);
    }

    /// Generate a struct variant (multiple fields or named fields).
    fn generate_struct_variant(
        &mut self,
        variant_name: &str,
        pascal_variant_name: &str,
        variant: &facet_core::Variant,
    ) {
        self.imports.insert("TypedDict");
        self.imports.insert("Required");

        let data_class_name = format!("{}Data", pascal_variant_name);

        // Functional form uses runtime expressions — quote forward references.
        let needs_functional = variant
            .data
            .fields
            .iter()
            .any(|f| is_python_keyword(f.effective_name()));
        let quote_after: Option<&str> = if needs_functional {
            Some(&data_class_name)
        } else {
            None
        };

        // Generate the data class fields
        let data_fields: Vec<_> = variant
            .data
            .fields
            .iter()
            .map(|field| {
                let field_type = self.type_for_shape(field.shape.get(), quote_after);
                TypedDictField::new(field.effective_name(), field_type, true, &[])
            })
            .collect();

        let mut data_output = String::new();
        write_typed_dict(&mut data_output, &data_class_name, &data_fields);
        data_output.push('\n');
        self.generated.insert(data_class_name.clone(), data_output);

        // Quote data_class_name if wrapper will use functional form (forward ref).
        let wrapper_type_str =
            if is_python_keyword(variant_name) && data_class_name.as_str() > pascal_variant_name {
                format!("\"{}\"", data_class_name)
            } else {
                data_class_name.clone()
            };
        let wrapper_fields = [TypedDictField::new(
            variant_name,
            wrapper_type_str,
            true,
            &[],
        )];

        let mut wrapper_output = String::new();
        write_typed_dict(&mut wrapper_output, pascal_variant_name, &wrapper_fields);
        wrapper_output.push('\n');

        self.generated
            .insert(pascal_variant_name.to_string(), wrapper_output);
    }

    /// Get the Python type string for a shape.
    /// `quote_after` quotes user-defined names sorting after it (forward refs).
    fn type_for_shape(&mut self, shape: &'static Shape, quote_after: Option<&str>) -> String {
        // Check Def first - these take precedence over transparent wrappers
        match &shape.def {
            Def::Scalar => self.scalar_type(shape),
            Def::Option(opt) => {
                format!("{} | None", self.type_for_shape(opt.t, quote_after))
            }
            Def::List(list) => {
                format!("list[{}]", self.type_for_shape(list.t, quote_after))
            }
            Def::Array(arr) => {
                format!("list[{}]", self.type_for_shape(arr.t, quote_after))
            }
            Def::Set(set) => {
                format!("list[{}]", self.type_for_shape(set.t, quote_after))
            }
            Def::Map(map) => {
                format!(
                    "dict[{}, {}]",
                    self.type_for_shape(map.k, quote_after),
                    self.type_for_shape(map.v, quote_after)
                )
            }
            Def::Pointer(ptr) => match ptr.pointee {
                Some(pointee) => self.type_for_shape(pointee, quote_after),
                None => {
                    self.imports.insert("Any");
                    "Any".to_string()
                }
            },
            Def::Undefined => {
                // User-defined types - queue for generation and return name
                match &shape.ty {
                    Type::User(UserType::Struct(st)) => {
                        // Handle tuples specially - inline them as tuple[...] since their
                        // type_identifier "(…)" is not a valid Python identifier
                        if st.kind == StructKind::Tuple {
                            let types: Vec<String> = st
                                .fields
                                .iter()
                                .map(|f| self.type_for_shape(f.shape.get(), quote_after))
                                .collect();
                            format!("tuple[{}]", types.join(", "))
                        } else {
                            self.add_shape(shape);
                            self.maybe_quote(shape.type_identifier, quote_after)
                        }
                    }
                    Type::User(UserType::Enum(_)) => {
                        self.add_shape(shape);
                        self.maybe_quote(shape.type_identifier, quote_after)
                    }
                    _ => self.inner_type_or_any(shape, quote_after),
                }
            }
            _ => self.inner_type_or_any(shape, quote_after),
        }
    }

    /// Wrap a type name in quotes if it is a forward reference (sorts after `quote_after`).
    fn maybe_quote(&self, name: &str, quote_after: Option<&str>) -> String {
        if let Some(after) = quote_after
            && name > after
        {
            return format!("\"{}\"", name);
        }
        name.to_string()
    }

    /// Get the inner type for transparent wrappers, or "Any" as fallback.
    fn inner_type_or_any(&mut self, shape: &'static Shape, quote_after: Option<&str>) -> String {
        match shape.inner {
            Some(inner) => self.type_for_shape(inner, quote_after),
            None => {
                self.imports.insert("Any");
                "Any".to_string()
            }
        }
    }

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

/// Write a doc comment to the output.
fn write_doc_comment(output: &mut String, doc: &[&str]) {
    for line in doc {
        output.push('#');
        output.push_str(line);
        output.push('\n');
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

    #[test]
    fn test_enum_tuple_variant() {
        #[derive(Facet)]
        #[repr(C)]
        #[allow(dead_code)]
        enum TupleVariant {
            Point(i32, i32),
        }
        let py = to_python::<TupleVariant>(false);
        insta::assert_snapshot!(py);
    }

    #[test]
    fn test_enum_struct_variant_forward_reference() {
        // This test verifies that struct variant data classes are quoted
        // to handle forward references correctly in Python.
        // Without quoting, Python would fail with "NameError: name 'DataData' is not defined"
        // because DataData is defined after Data in alphabetical order.
        #[derive(Facet)]
        #[repr(C)]
        #[allow(dead_code)]
        enum Message {
            // Struct variant with inline fields - generates MessageData class
            Data { name: String, value: f64 },
        }
        let py = to_python::<Message>(false);
        insta::assert_snapshot!(py);
    }

    #[test]
    fn test_functional_typed_dict_no_type_keyword() {
        // Regression test for https://github.com/facet-rs/facet/issues/2131
        #[derive(Facet)]
        struct Bug {
            from: Option<String>,
        }

        let py = to_python::<Bug>(false);
        assert!(
            !py.starts_with("type "),
            "functional TypedDict should NOT start with `type` keyword, got:\n{py}"
        );
        insta::assert_snapshot!(py);
    }

    #[test]
    fn test_functional_typed_dict_forward_ref_quoted() {
        // Regression test for https://github.com/facet-rs/facet/issues/2131
        #[derive(Facet)]
        #[allow(dead_code)]
        struct Recipient {
            name: String,
        }

        #[derive(Facet)]
        #[allow(dead_code)]
        struct Addr {
            from: String,
            to: Recipient,
        }

        let py = to_python::<Addr>(false);
        assert!(
            py.contains("Required[\"Recipient\"]"),
            "forward reference in functional TypedDict should be quoted, got:\n{py}"
        );
        insta::assert_snapshot!(py);
    }

    #[test]
    fn test_flatten() {
        #[derive(Facet)]
        struct Inner {
            x: f64,
            y: f64,
        }

        #[derive(Facet)]
        struct Outer {
            #[facet(flatten)]
            inner: Inner,
            z: String,
        }

        let py = to_python::<Outer>(false);

        // Inner should still be generated as its own TypedDict
        assert!(
            py.contains("class Inner(TypedDict, total=False):"),
            "#[facet(flatten)] — Inner should still be generated as its own TypedDict, got:\n{py}"
        );

        // The flattened field 'inner' must NOT appear as a key in Outer
        assert!(
            !py.contains("inner: Required[Inner]"),
            "#[facet(flatten)] — 'inner' should be inlined, not a nested field, got:\n{py}"
        );

        // x and y must be inlined directly into Outer
        assert!(
            py.contains("    x: Required[float]"),
            "#[facet(flatten)] — 'x' should be inlined from Inner into Outer, got:\n{py}"
        );
        assert!(
            py.contains("    y: Required[float]"),
            "#[facet(flatten)] — 'y' should be inlined from Inner into Outer, got:\n{py}"
        );

        insta::assert_snapshot!(py);
    }

    #[test]
    fn test_flatten_option() {
        // #[facet(flatten)] on Option<Struct> — the inlined fields should be
        // optional in the parent TypedDict because their JSON presence is
        // conditional on the Option being Some.
        #[derive(Facet)]
        struct Coords {
            x: f64,
            y: f64,
        }

        #[derive(Facet)]
        struct Entity {
            name: String,
            #[facet(flatten)]
            coords: Option<Coords>,
        }

        let py = to_python::<Entity>(false);

        // Coords must still be generated as its own TypedDict
        assert!(
            py.contains("class Coords(TypedDict, total=False):"),
            "flatten Option — Coords should still be generated, got:\n{py}"
        );
        // The raw 'coords' field must NOT appear as a nested key
        assert!(
            !py.contains("coords:"),
            "flatten Option — 'coords' key should not appear in Entity, got:\n{py}"
        );
        // x and y must be inlined as optional (bare type, no Required[]).
        // Coords' own definition still uses Required[float], so check with
        // indentation to match Entity's lines only.
        assert!(
            py.contains("    x: float"),
            "flatten Option — 'x' should be inlined as optional float in Entity, got:\n{py}"
        );
        assert!(
            py.contains("    y: float"),
            "flatten Option — 'y' should be inlined as optional float in Entity, got:\n{py}"
        );

        insta::assert_snapshot!(py);
    }

    #[test]
    fn test_flatten_with_rename_all() {
        // Flattened struct with rename_all — inlined fields should use the
        // renamed effective names, not the original Rust field names.
        #[derive(Facet)]
        #[facet(rename_all = "camelCase")]
        struct Coords {
            pos_x: f64,
            pos_y: f64,
        }

        #[derive(Facet)]
        struct Entity {
            #[facet(flatten)]
            coords: Coords,
            label: String,
        }

        let py = to_python::<Entity>(false);

        // Renamed fields must appear, not the Rust names
        assert!(
            py.contains("posX: Required[float]"),
            "flatten + rename_all — 'posX' should be inlined into Entity, got:\n{py}"
        );
        assert!(
            py.contains("posY: Required[float]"),
            "flatten + rename_all — 'posY' should be inlined into Entity, got:\n{py}"
        );
        // Raw Rust names must NOT appear in the output
        assert!(
            !py.contains("pos_x"),
            "flatten + rename_all — raw 'pos_x' should not appear, got:\n{py}"
        );

        insta::assert_snapshot!(py);
    }

    #[test]
    fn test_flatten_with_optional_fields() {
        // Optional fields inside a flattened struct must remain optional
        // (i.e. not wrapped in Required[]) in the parent TypedDict.
        #[derive(Facet)]
        struct Meta {
            description: Option<String>,
            version: u32,
        }

        #[derive(Facet)]
        struct Package {
            name: String,
            #[facet(flatten)]
            meta: Meta,
        }

        let py = to_python::<Package>(false);

        // Optional field must stay optional — in this generator optional fields
        // use a bare type (no Required[]) because the TypedDict is total=False.
        assert!(
            py.contains("description: str"),
            "flatten + optional — 'description' should be optional (bare type) in Package, got:\n{py}"
        );
        assert!(
            !py.contains("description: Required[str]"),
            "flatten + optional — 'description' must NOT be wrapped in Required[], got:\n{py}"
        );
        // Required field must stay required
        assert!(
            py.contains("version: Required[int]"),
            "flatten + optional — 'version' should be required in Package, got:\n{py}"
        );

        insta::assert_snapshot!(py);
    }

    #[test]
    fn test_flatten_multilevel() {
        // A flattened struct that itself contains a flattened struct —
        // all fields should end up in the outermost TypedDict.
        #[derive(Facet)]
        struct Point {
            x: f64,
            y: f64,
        }

        #[derive(Facet)]
        struct ColoredPoint {
            #[facet(flatten)]
            point: Point,
            color: String,
        }

        #[derive(Facet)]
        struct Scene {
            #[facet(flatten)]
            colored_point: ColoredPoint,
            name: String,
        }

        let py = to_python::<Scene>(false);

        // x and y must be inlined all the way into Scene
        assert!(
            py.contains("    x: Required[float]"),
            "multi-level flatten — 'x' should reach Scene, got:\n{py}"
        );
        assert!(
            py.contains("    y: Required[float]"),
            "multi-level flatten — 'y' should reach Scene, got:\n{py}"
        );
        assert!(
            py.contains("    color: Required[str]"),
            "multi-level flatten — 'color' should reach Scene, got:\n{py}"
        );
        // Neither intermediate field name should appear as a key
        assert!(
            !py.contains("colored_point:"),
            "multi-level flatten — 'colored_point' key should not appear in Scene, got:\n{py}"
        );
        assert!(
            !py.contains("point:"),
            "multi-level flatten — 'point' key should not appear in Scene, got:\n{py}"
        );

        insta::assert_snapshot!(py);
    }

    #[test]
    fn test_flatten_preserves_field_docs() {
        // Doc comments on fields inside a flattened struct should be
        // preserved when those fields are inlined into the parent TypedDict.
        #[derive(Facet)]
        struct Dims {
            /// Width in pixels
            width: u32,
            /// Height in pixels
            height: u32,
        }

        #[derive(Facet)]
        struct Image {
            #[facet(flatten)]
            dims: Dims,
            path: String,
        }

        let py = to_python::<Image>(false);

        assert!(
            py.contains("width: Required[int]"),
            "flatten docs — 'width' should be inlined into Image, got:\n{py}"
        );
        assert!(
            py.contains("height: Required[int]"),
            "flatten docs — 'height' should be inlined into Image, got:\n{py}"
        );
        assert!(
            !py.contains("dims: Required[Dims]"),
            "flatten docs — 'dims' key should not appear in Image, got:\n{py}"
        );

        insta::assert_snapshot!(py);
    }

    #[test]
    fn test_flatten_arc() {
        // #[facet(flatten)] on Arc<Struct> — should inline the same as a plain
        // struct flatten, since Arc is just a pointer wrapper.
        use std::sync::Arc;

        #[derive(Facet)]
        struct Coords {
            x: f64,
            y: f64,
        }

        #[derive(Facet)]
        struct Entity {
            name: String,
            #[facet(flatten)]
            coords: Arc<Coords>,
        }

        let py = to_python::<Entity>(false);

        assert!(
            py.contains("class Coords(TypedDict, total=False):"),
            "flatten Arc — Coords should still be generated, got:\n{py}"
        );
        assert!(
            !py.contains("coords:"),
            "flatten Arc — 'coords' key should not appear in Entity, got:\n{py}"
        );
        assert!(
            py.contains("    x: Required[float]"),
            "flatten Arc — 'x' should be inlined as required float in Entity, got:\n{py}"
        );
        assert!(
            py.contains("    y: Required[float]"),
            "flatten Arc — 'y' should be inlined as required float in Entity, got:\n{py}"
        );

        insta::assert_snapshot!(py);
    }

    #[test]
    fn test_flatten_option_arc() {
        // #[facet(flatten)] on Option<Arc<Struct>> — multi-layer unwrap.
        // Fields should be optional (from the Option) despite the Arc wrapper.
        use std::sync::Arc;

        #[derive(Facet)]
        struct Coords {
            x: f64,
            y: f64,
        }

        #[derive(Facet)]
        struct Entity {
            name: String,
            #[facet(flatten)]
            coords: Option<Arc<Coords>>,
        }

        let py = to_python::<Entity>(false);

        assert!(
            py.contains("class Coords(TypedDict, total=False):"),
            "flatten Option<Arc> — Coords should still be generated, got:\n{py}"
        );
        assert!(
            !py.contains("coords:"),
            "flatten Option<Arc> — 'coords' key should not appear in Entity, got:\n{py}"
        );
        // Fields must be optional (bare type) because of the Option wrapper
        assert!(
            py.contains("    x: float"),
            "flatten Option<Arc> — 'x' should be inlined as optional float in Entity, got:\n{py}"
        );
        assert!(
            py.contains("    y: float"),
            "flatten Option<Arc> — 'y' should be inlined as optional float in Entity, got:\n{py}"
        );

        insta::assert_snapshot!(py);
    }

    #[test]
    fn test_flatten_skip_serializing_field() {
        // A #[facet(skip_serializing)] field inside a flattened struct must
        // NOT appear in the parent TypedDict — it is excluded from the wire
        // format so it must not be part of the Python type either.
        #[derive(Facet)]
        struct Coords {
            x: f64,
            y: f64,
            #[facet(skip_serializing)]
            internal: u8,
        }

        #[derive(Facet)]
        struct Entity {
            name: String,
            #[facet(flatten)]
            coords: Coords,
        }

        let py = to_python::<Entity>(false);

        assert!(
            py.contains("    x: Required[float]"),
            "flatten skip_serializing — 'x' should be inlined, got:\n{py}"
        );
        assert!(
            py.contains("    y: Required[float]"),
            "flatten skip_serializing — 'y' should be inlined, got:\n{py}"
        );
        assert!(
            !py.contains("internal"),
            "flatten skip_serializing — 'internal' must not appear anywhere, got:\n{py}"
        );

        insta::assert_snapshot!(py);
    }

    #[test]
    fn test_default_field_not_required() {
        // Fields with #[facet(default)] are optional in JSON — facet fills in
        // the default when the key is absent. They must not be Required[T].
        #[derive(Facet)]
        struct Config {
            name: String,
            #[facet(default)]
            retries: u32,
            #[facet(default = 30)]
            timeout: u32,
            required_value: i32,
        }

        let py = to_python::<Config>(false);

        // Non-default fields must still be Required
        assert!(
            py.contains("name: Required[str]"),
            "default — 'name' has no default so must be Required, got:\n{py}"
        );
        assert!(
            py.contains("required_value: Required[int]"),
            "default — 'required_value' has no default so must be Required, got:\n{py}"
        );
        // Fields with defaults must NOT be Required
        assert!(
            !py.contains("retries: Required[int]"),
            "default — 'retries' has a default so must NOT be Required, got:\n{py}"
        );
        assert!(
            py.contains("retries: int"),
            "default — 'retries' should be bare int (optional), got:\n{py}"
        );
        assert!(
            !py.contains("timeout: Required[int]"),
            "default — 'timeout' has a default so must NOT be Required, got:\n{py}"
        );
        assert!(
            py.contains("timeout: int"),
            "default — 'timeout' should be bare int (optional), got:\n{py}"
        );

        insta::assert_snapshot!(py);
    }
}
