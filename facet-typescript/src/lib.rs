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

    #[inline]
    fn shape_key(shape: &'static Shape) -> &'static str {
        shape.type_identifier
    }

    /// Unwrap through options, pointers, transparent wrappers, and proxies to get the effective shape.
    ///
    /// Returns the unwrapped shape along with a flag indicating whether an `Option` was encountered.
    fn unwrap_to_inner_shape(shape: &'static Shape) -> (&'static Shape, bool) {
        // Unwrap Option<T> first so we can mark fields as optional.
        if let Def::Option(opt) = &shape.def {
            let (inner, _) = Self::unwrap_to_inner_shape(opt.t);
            return (inner, true);
        }
        // Unwrap pointers (Arc, Box, etc.)
        if let Def::Pointer(ptr) = &shape.def
            && let Some(pointee) = ptr.pointee
        {
            return Self::unwrap_to_inner_shape(pointee);
        }
        // Unwrap transparent wrappers
        if let Some(inner) = shape.inner {
            let (inner_shape, is_optional) = Self::unwrap_to_inner_shape(inner);
            return (inner_shape, is_optional);
        }
        // Handle proxy types - use the proxy's shape for serialization
        if let Some(proxy_def) = shape.proxy {
            return Self::unwrap_to_inner_shape(proxy_def.shape);
        }
        (shape, false)
    }

    /// Format a field for inline object types (e.g., in enum variants).
    /// Returns a string like `"fieldName: Type"` or `"fieldName?: Type"` for Option fields.
    fn format_inline_field(&mut self, field: &Field, force_optional: bool) -> String {
        let field_name = field.effective_name();
        let field_shape = field.shape.get();

        if let Def::Option(opt) = &field_shape.def {
            let inner_type = self.type_for_shape(opt.t);
            format!("{}?: {}", field_name, inner_type)
        } else if force_optional {
            let field_type = self.type_for_shape(field_shape);
            format!("{}?: {}", field_name, field_type)
        } else {
            let field_type = self.type_for_shape(field_shape);
            format!("{}: {}", field_name, field_type)
        }
    }

    /// Collect inline field strings for a struct's fields, handling skip and flatten.
    fn collect_inline_fields(
        &mut self,
        fields: &'static [Field],
        force_optional: bool,
    ) -> Vec<String> {
        let mut flatten_stack: Vec<&'static str> = Vec::new();
        self.collect_inline_fields_guarded(fields, force_optional, &mut flatten_stack)
    }

    fn collect_inline_fields_guarded(
        &mut self,
        fields: &'static [Field],
        force_optional: bool,
        flatten_stack: &mut Vec<&'static str>,
    ) -> Vec<String> {
        let mut result = Vec::new();
        for field in fields {
            if field.should_skip_serializing_unconditional() {
                continue;
            }
            if field.is_flattened() {
                let (inner_shape, parent_is_optional) =
                    Self::unwrap_to_inner_shape(field.shape.get());
                if let Type::User(UserType::Struct(st)) = &inner_shape.ty {
                    let inner_key = Self::shape_key(inner_shape);
                    if flatten_stack.contains(&inner_key) {
                        continue;
                    }
                    flatten_stack.push(inner_key);
                    result.extend(self.collect_inline_fields_guarded(
                        st.fields,
                        force_optional || parent_is_optional,
                        flatten_stack,
                    ));
                    flatten_stack.pop();
                    continue;
                }
            }
            result.push(self.format_inline_field(field, force_optional));
        }
        result
    }

    /// Check if a struct has any fields that will be serialized.
    /// This accounts for skipped fields and flattened structs.
    fn has_serializable_fields(
        field_owner_shape: &'static Shape,
        fields: &'static [Field],
    ) -> bool {
        let mut flatten_stack: Vec<&'static str> = Vec::new();
        flatten_stack.push(Self::shape_key(field_owner_shape));
        Self::has_serializable_fields_guarded(fields, &mut flatten_stack)
    }

    fn has_serializable_fields_guarded(
        fields: &'static [Field],
        flatten_stack: &mut Vec<&'static str>,
    ) -> bool {
        for field in fields {
            if field.should_skip_serializing_unconditional() {
                continue;
            }
            if field.is_flattened() {
                let (inner_shape, _) = Self::unwrap_to_inner_shape(field.shape.get());
                if let Type::User(UserType::Struct(st)) = &inner_shape.ty {
                    let inner_key = Self::shape_key(inner_shape);
                    if flatten_stack.contains(&inner_key) {
                        continue;
                    }
                    flatten_stack.push(inner_key);
                    let has_fields =
                        Self::has_serializable_fields_guarded(st.fields, flatten_stack);
                    flatten_stack.pop();
                    if has_fields {
                        return true;
                    }
                    continue;
                }
            }
            // Found a field that will be serialized
            return true;
        }
        false
    }

    /// Write struct fields to output, handling skip and flatten recursively.
    fn write_struct_fields_for_shape(
        &mut self,
        field_owner_shape: &'static Shape,
        fields: &'static [Field],
    ) {
        let mut flatten_stack: Vec<&'static str> = Vec::new();
        flatten_stack.push(Self::shape_key(field_owner_shape));
        self.write_struct_fields_guarded(fields, false, &mut flatten_stack);
    }

    fn write_struct_fields_guarded(
        &mut self,
        fields: &'static [Field],
        force_optional: bool,
        flatten_stack: &mut Vec<&'static str>,
    ) {
        for field in fields {
            if field.should_skip_serializing_unconditional() {
                continue;
            }
            if field.is_flattened() {
                let (inner_shape, parent_is_optional) =
                    Self::unwrap_to_inner_shape(field.shape.get());
                if let Type::User(UserType::Struct(st)) = &inner_shape.ty {
                    let inner_key = Self::shape_key(inner_shape);
                    if flatten_stack.contains(&inner_key) {
                        continue;
                    }
                    flatten_stack.push(inner_key);
                    self.write_struct_fields_guarded(
                        st.fields,
                        force_optional || parent_is_optional,
                        flatten_stack,
                    );
                    flatten_stack.pop();
                    continue;
                }
            }
            self.write_field(field, force_optional);
        }
    }

    /// Write a single field to the output.
    fn write_field(&mut self, field: &Field, force_optional: bool) {
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
        let field_shape = field.shape.get();

        self.write_indent();

        // Use optional marker for Option fields or when explicitly forced (flattened Option parents).
        if let Def::Option(opt) = &field_shape.def {
            let inner_type = self.type_for_shape(opt.t);
            writeln!(self.output, "{}?: {};", field_name, inner_type).unwrap();
        } else if force_optional {
            let field_type = self.type_for_shape(field_shape);
            writeln!(self.output, "{}?: {};", field_name, field_type).unwrap();
        } else {
            let field_type = self.type_for_shape(field_shape);
            writeln!(self.output, "{}: {};", field_name, field_type).unwrap();
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

        // Generate doc comment if present (before proxy handling so proxied types keep their docs)
        if !shape.doc.is_empty() {
            self.output.push_str("/**\n");
            for line in shape.doc {
                self.output.push_str(" *");
                self.output.push_str(line);
                self.output.push('\n');
            }
            self.output.push_str(" */\n");
        }

        // Handle proxy types - use the proxy's shape for generation
        // but keep the original type name
        if let Some(proxy_def) = shape.proxy {
            let proxy_shape = proxy_def.shape;
            match &proxy_shape.ty {
                Type::User(UserType::Struct(st)) => {
                    self.generate_struct(shape, proxy_shape, st.fields, st.kind);
                    return;
                }
                Type::User(UserType::Enum(en)) => {
                    self.generate_enum(shape, en);
                    return;
                }
                _ => {
                    // For non-struct/enum proxies (scalars, tuples, collections, etc.),
                    // generate a type alias to the proxy's type
                    let proxy_type = self.type_for_shape(proxy_shape);
                    writeln!(
                        self.output,
                        "export type {} = {};",
                        shape.type_identifier, proxy_type
                    )
                    .unwrap();
                    self.output.push('\n');
                    return;
                }
            }
        }

        match &shape.ty {
            Type::User(UserType::Struct(st)) => {
                self.generate_struct(shape, shape, st.fields, st.kind);
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
        exported_shape: &'static Shape,
        field_owner_shape: &'static Shape,
        fields: &'static [Field],
        kind: StructKind,
    ) {
        match kind {
            StructKind::Unit => {
                // Unit struct as null
                writeln!(
                    self.output,
                    "export type {} = null;",
                    exported_shape.type_identifier
                )
                .unwrap();
            }
            StructKind::TupleStruct if fields.len() == 1 => {
                // Newtype - type alias to inner
                let inner_type = self.type_for_shape(fields[0].shape.get());
                writeln!(
                    self.output,
                    "export type {} = {};",
                    exported_shape.type_identifier, inner_type
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
                    exported_shape.type_identifier,
                    types.join(", ")
                )
                .unwrap();
            }
            StructKind::Struct => {
                // Empty structs should use `object` type to prevent accepting primitives
                if !Self::has_serializable_fields(field_owner_shape, fields) {
                    writeln!(
                        self.output,
                        "export type {} = object;",
                        exported_shape.type_identifier
                    )
                    .unwrap();
                } else {
                    writeln!(
                        self.output,
                        "export interface {} {{",
                        exported_shape.type_identifier
                    )
                    .unwrap();
                    self.indent += 1;

                    self.write_struct_fields_for_shape(field_owner_shape, fields);

                    self.indent -= 1;
                    self.output.push_str("}\n");
                }
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

        // Check if the enum is untagged
        let is_untagged = shape.is_untagged();

        if is_untagged {
            // Untagged enum: simple union of variant types
            let mut variant_types = Vec::new();

            for variant in enum_type.variants {
                match variant.data.kind {
                    StructKind::Unit => {
                        // Unit variant in untagged enum - this is unusual but we represent as null
                        variant_types.push("null".to_string());
                    }
                    StructKind::TupleStruct if variant.data.fields.len() == 1 => {
                        // Newtype variant: just the inner type
                        let inner = self.type_for_shape(variant.data.fields[0].shape.get());
                        variant_types.push(inner);
                    }
                    StructKind::TupleStruct => {
                        // Multi-element tuple variant: [T1, T2, ...]
                        let types: Vec<String> = variant
                            .data
                            .fields
                            .iter()
                            .map(|f| self.type_for_shape(f.shape.get()))
                            .collect();
                        variant_types.push(format!("[{}]", types.join(", ")));
                    }
                    _ => {
                        // Struct variant: inline object type
                        let field_types = self.collect_inline_fields(variant.data.fields, false);
                        variant_types.push(format!("{{ {} }}", field_types.join("; ")));
                    }
                }
            }

            writeln!(
                self.output,
                "export type {} = {};",
                shape.type_identifier,
                variant_types.join(" | ")
            )
            .unwrap();
        } else if all_unit {
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
                    StructKind::TupleStruct => {
                        // Multi-element tuple variant: { VariantName: [T1, T2, ...] }
                        let types: Vec<String> = variant
                            .data
                            .fields
                            .iter()
                            .map(|f| self.type_for_shape(f.shape.get()))
                            .collect();
                        variant_types.push(format!(
                            "{{ {}: [{}] }}",
                            variant_name,
                            types.join(", ")
                        ));
                    }
                    _ => {
                        // Struct variant: { VariantName: { ...fields } }
                        let field_types = self.collect_inline_fields(variant.data.fields, false);
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
                    Type::User(UserType::Struct(st)) => {
                        // Handle tuples specially - inline them as [T1, T2, ...] since their
                        // type_identifier "(…)" is not a valid TypeScript identifier
                        if st.kind == StructKind::Tuple {
                            let types: Vec<String> = st
                                .fields
                                .iter()
                                .map(|f| self.type_for_shape(f.shape.get()))
                                .collect();
                            format!("[{}]", types.join(", "))
                        } else {
                            self.add_shape(shape);
                            shape.type_identifier.to_string()
                        }
                    }
                    Type::User(UserType::Enum(_)) => {
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
    use alloc::collections::BTreeMap;
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

    #[test]
    fn test_struct_with_tuple_field() {
        #[derive(Facet)]
        struct Container {
            coordinates: (i32, i32),
        }

        let ts = to_typescript::<Container>();
        insta::assert_snapshot!(ts);
    }

    #[test]
    fn test_struct_with_single_element_tuple() {
        #[derive(Facet)]
        struct Wrapper {
            value: (String,),
        }

        let ts = to_typescript::<Wrapper>();
        insta::assert_snapshot!(ts);
    }

    #[test]
    fn test_enum_with_tuple_variant() {
        #[derive(Facet)]
        #[repr(C)]
        #[allow(dead_code)]
        enum Event {
            Click { x: i32, y: i32 },
            Move((i32, i32)),
            Resize { dimensions: (u32, u32) },
        }

        let ts = to_typescript::<Event>();
        insta::assert_snapshot!(ts);
    }

    #[test]
    fn test_untagged_enum() {
        #[derive(Facet)]
        #[facet(untagged)]
        #[repr(C)]
        #[allow(dead_code)]
        pub enum Value {
            Text(String),
            Number(f64),
        }

        let ts = to_typescript::<Value>();
        insta::assert_snapshot!(ts);
    }

    #[test]
    fn test_untagged_enum_unit_and_struct_variants() {
        #[derive(Facet)]
        #[facet(untagged)]
        #[repr(C)]
        #[allow(dead_code)]
        pub enum Event {
            None,
            Data { x: i32, y: i32 },
        }

        let ts = to_typescript::<Event>();
        insta::assert_snapshot!(ts);
    }

    #[test]
    fn test_enum_with_tuple_struct_variant() {
        #[derive(Facet)]
        #[allow(dead_code)]
        pub struct Point {
            x: f64,
            y: f64,
        }

        #[derive(Facet)]
        #[repr(u8)]
        #[allow(dead_code)]
        pub enum Shape {
            Line(Point, Point),
        }

        let ts = to_typescript::<Shape>();
        insta::assert_snapshot!(ts);
    }

    #[test]
    fn test_enum_with_proxy_struct() {
        #[derive(Facet)]
        #[facet(proxy = PointProxy)]
        #[allow(dead_code)]
        pub struct Point {
            xxx: f64,
            yyy: f64,
        }

        #[derive(Facet)]
        #[allow(dead_code)]
        pub struct PointProxy {
            x: f64,
            y: f64,
        }

        impl From<PointProxy> for Point {
            fn from(p: PointProxy) -> Self {
                Self { xxx: p.x, yyy: p.y }
            }
        }

        impl From<&Point> for PointProxy {
            fn from(p: &Point) -> Self {
                Self { x: p.xxx, y: p.yyy }
            }
        }

        #[derive(Facet)]
        #[repr(u8)]
        #[facet(untagged)]
        #[allow(dead_code)]
        pub enum Shape {
            Circle { center: Point, radius: f64 },
            Line(Point, Point),
        }

        let ts = to_typescript::<Shape>();
        insta::assert_snapshot!(ts);
    }

    #[test]
    fn test_enum_with_proxy_enum() {
        #[derive(Facet)]
        #[repr(u8)]
        #[facet(proxy = StatusProxy)]
        pub enum Status {
            Unknown,
        }

        #[derive(Facet)]
        #[repr(u8)]
        pub enum StatusProxy {
            Active,
            Inactive,
        }

        impl From<StatusProxy> for Status {
            fn from(_: StatusProxy) -> Self {
                Self::Unknown
            }
        }

        impl From<&Status> for StatusProxy {
            fn from(_: &Status) -> Self {
                Self::Active
            }
        }

        let ts = to_typescript::<Status>();
        insta::assert_snapshot!(ts);
    }

    #[test]
    fn test_proxy_to_scalar() {
        /// A user ID that serializes as a string
        #[derive(Facet)]
        #[facet(proxy = String)]
        #[allow(dead_code)]
        pub struct UserId(u64);

        impl From<String> for UserId {
            fn from(s: String) -> Self {
                Self(s.parse().unwrap_or(0))
            }
        }

        impl From<&UserId> for String {
            fn from(id: &UserId) -> Self {
                id.0.to_string()
            }
        }

        let ts = to_typescript::<UserId>();
        insta::assert_snapshot!(ts);
    }

    #[test]
    fn test_proxy_preserves_doc_comments() {
        /// This is a point in 2D space.
        /// It has x and y coordinates.
        #[derive(Facet)]
        #[facet(proxy = PointProxy)]
        #[allow(dead_code)]
        pub struct Point {
            internal_x: f64,
            internal_y: f64,
        }

        #[derive(Facet)]
        #[allow(dead_code)]
        pub struct PointProxy {
            x: f64,
            y: f64,
        }

        impl From<PointProxy> for Point {
            fn from(p: PointProxy) -> Self {
                Self {
                    internal_x: p.x,
                    internal_y: p.y,
                }
            }
        }

        impl From<&Point> for PointProxy {
            fn from(p: &Point) -> Self {
                Self {
                    x: p.internal_x,
                    y: p.internal_y,
                }
            }
        }

        let ts = to_typescript::<Point>();
        insta::assert_snapshot!(ts);
    }

    #[test]
    fn test_untagged_enum_optional_fields() {
        #[derive(Facet)]
        #[facet(untagged)]
        #[repr(C)]
        #[allow(dead_code)]
        pub enum Config {
            Simple {
                name: String,
            },
            Full {
                name: String,
                description: Option<String>,
                count: Option<u32>,
            },
        }

        let ts = to_typescript::<Config>();
        insta::assert_snapshot!(ts);
    }

    #[test]
    fn test_flatten_variants() {
        use std::sync::Arc;

        // Inner struct with a skipped field to test skip handling
        #[derive(Facet)]
        pub struct Coords {
            pub x: i32,
            pub y: i32,
            #[facet(skip)]
            pub internal: u8,
        }

        // Direct flatten
        #[derive(Facet)]
        pub struct FlattenDirect {
            pub name: String,
            #[facet(flatten)]
            pub coords: Coords,
        }

        // Flatten through Arc<T>
        #[derive(Facet)]
        pub struct FlattenArc {
            pub name: String,
            #[facet(flatten)]
            pub coords: Arc<Coords>,
        }

        // Flatten through Box<T>
        #[derive(Facet)]
        pub struct FlattenBox {
            pub name: String,
            #[facet(flatten)]
            pub coords: Box<Coords>,
        }

        // Flatten Option<T> makes inner fields optional
        #[derive(Facet)]
        pub struct FlattenOption {
            pub name: String,
            #[facet(flatten)]
            pub coords: Option<Coords>,
        }

        // Nested Option<Arc<T>> tests multi-layer unwrapping
        #[derive(Facet)]
        pub struct FlattenOptionArc {
            pub name: String,
            #[facet(flatten)]
            pub coords: Option<Arc<Coords>>,
        }

        // Non-struct flatten (BTreeMap) falls through to normal field output
        #[derive(Facet)]
        pub struct FlattenMap {
            pub name: String,
            #[facet(flatten)]
            pub extra: BTreeMap<String, String>,
        }

        let ts_direct = to_typescript::<FlattenDirect>();
        let ts_arc = to_typescript::<FlattenArc>();
        let ts_box = to_typescript::<FlattenBox>();
        let ts_option = to_typescript::<FlattenOption>();
        let ts_option_arc = to_typescript::<FlattenOptionArc>();
        let ts_map = to_typescript::<FlattenMap>();

        insta::assert_snapshot!("flatten_direct", ts_direct);
        insta::assert_snapshot!("flatten_arc", ts_arc);
        insta::assert_snapshot!("flatten_box", ts_box);
        insta::assert_snapshot!("flatten_option", ts_option);
        insta::assert_snapshot!("flatten_option_arc", ts_option_arc);
        insta::assert_snapshot!("flatten_map", ts_map);
    }

    #[test]
    fn test_tagged_enum_optional_fields() {
        #[derive(Facet)]
        #[repr(u8)]
        #[allow(dead_code)]
        enum Message {
            Simple {
                text: String,
            },
            Full {
                text: String,
                metadata: Option<String>,
                count: Option<u32>,
            },
        }

        let ts = to_typescript::<Message>();
        insta::assert_snapshot!(ts);
    }

    #[test]
    fn test_flatten_proxy_struct() {
        #[derive(Facet)]
        #[facet(proxy = CoordsProxy)]
        #[allow(dead_code)]
        struct Coords {
            internal_x: f64,
            internal_y: f64,
        }

        #[derive(Facet)]
        #[allow(dead_code)]
        struct CoordsProxy {
            x: f64,
            y: f64,
        }

        impl From<CoordsProxy> for Coords {
            fn from(p: CoordsProxy) -> Self {
                Self {
                    internal_x: p.x,
                    internal_y: p.y,
                }
            }
        }

        impl From<&Coords> for CoordsProxy {
            fn from(c: &Coords) -> Self {
                Self {
                    x: c.internal_x,
                    y: c.internal_y,
                }
            }
        }

        #[derive(Facet)]
        #[allow(dead_code)]
        struct Shape {
            name: String,
            #[facet(flatten)]
            coords: Coords,
        }

        let ts = to_typescript::<Shape>();
        insta::assert_snapshot!(ts);
    }

    #[test]
    fn test_enum_variant_skipped_field() {
        #[derive(Facet)]
        #[repr(u8)]
        #[allow(dead_code)]
        enum Event {
            Data {
                visible: String,
                #[facet(skip)]
                internal: u64,
            },
        }

        let ts = to_typescript::<Event>();
        insta::assert_snapshot!(ts);
    }

    #[test]
    fn test_enum_variant_flatten() {
        // BUG: Enum struct variants should inline flattened fields
        #[derive(Facet)]
        #[allow(dead_code)]
        struct Metadata {
            author: String,
            version: u32,
        }

        #[derive(Facet)]
        #[repr(u8)]
        #[allow(dead_code)]
        enum Document {
            Article {
                title: String,
                #[facet(flatten)]
                meta: Metadata,
            },
        }

        let ts = to_typescript::<Document>();
        insta::assert_snapshot!(ts);
    }

    #[test]
    fn test_nested_flatten_struct() {
        #[derive(Facet)]
        #[allow(dead_code)]
        struct Inner {
            x: i32,
            y: i32,
        }

        #[derive(Facet)]
        #[allow(dead_code)]
        struct Middle {
            #[facet(flatten)]
            inner: Inner,
            z: i32,
        }

        #[derive(Facet)]
        #[allow(dead_code)]
        struct Outer {
            name: String,
            #[facet(flatten)]
            middle: Middle,
        }

        let ts = to_typescript::<Outer>();
        insta::assert_snapshot!(ts);
    }

    #[test]
    fn test_flatten_recursive_option_box() {
        #[derive(Facet)]
        struct Node {
            value: u32,
            #[facet(flatten)]
            next: Option<Box<Node>>,
        }

        let ts = to_typescript::<Node>();
        insta::assert_snapshot!("flatten_recursive_option_box", ts);
    }

    #[test]
    fn test_skip_serializing_struct_field() {
        #[derive(Facet)]
        struct Data {
            visible: String,
            #[facet(skip_serializing)]
            internal: u64,
        }

        let ts = to_typescript::<Data>();
        insta::assert_snapshot!("skip_serializing_struct_field", ts);
    }

    #[test]
    fn test_skip_serializing_inline_enum_variant_and_flatten_cycle_guard() {
        #[derive(Facet)]
        struct Node {
            value: u32,
            #[facet(flatten)]
            next: Option<Box<Node>>,
        }

        #[derive(Facet)]
        #[repr(u8)]
        enum Wrapper {
            Item {
                #[facet(flatten)]
                node: Node,
            },
            Data {
                visible: String,
                #[facet(skip_serializing)]
                internal: u64,
            },
        }

        let item = Wrapper::Item {
            node: Node {
                value: 1,
                next: None,
            },
        };
        match item {
            Wrapper::Item { node } => assert_eq!(node.value, 1),
            Wrapper::Data { .. } => unreachable!(),
        }

        let data = Wrapper::Data {
            visible: String::new(),
            internal: 0,
        };
        match data {
            Wrapper::Data { visible, internal } => {
                assert!(visible.is_empty());
                assert_eq!(internal, 0);
            }
            Wrapper::Item { .. } => unreachable!(),
        }

        let ts = to_typescript::<Wrapper>();
        insta::assert_snapshot!(
            "skip_serializing_inline_enum_variant_and_flatten_cycle_guard",
            ts
        );
    }

    #[test]
    fn test_empty_struct() {
        #[derive(Facet)]
        struct Data {
            empty: Empty,
        }

        #[derive(Facet)]
        struct Empty {}

        let e = to_typescript::<Empty>();
        let d = to_typescript::<Data>();
        insta::assert_snapshot!("test_empty_struct", e);
        insta::assert_snapshot!("test_empty_struct_wrap", d);
    }

    #[test]
    fn test_empty_struct_with_skipped_fields() {
        #[derive(Facet)]
        struct EmptyAfterSkip {
            #[facet(skip_serializing)]
            internal: String,
        }

        let ts = to_typescript::<EmptyAfterSkip>();
        insta::assert_snapshot!("test_empty_struct_with_skipped_fields", ts);
    }

    #[test]
    fn test_empty_struct_multiple_references() {
        #[derive(Facet)]
        struct Container {
            first: Empty,
            second: Empty,
            third: Option<Empty>,
        }

        #[derive(Facet)]
        struct Empty {}

        let ts = to_typescript::<Container>();
        insta::assert_snapshot!("test_empty_struct_multiple_references", ts);
    }

    #[test]
    fn test_flatten_empty_struct() {
        #[derive(Facet)]
        struct Empty {}

        #[derive(Facet)]
        struct Wrapper {
            #[facet(flatten)]
            empty: Empty,
        }

        let ts = to_typescript::<Wrapper>();
        insta::assert_snapshot!("test_flatten_empty_struct", ts);
    }
}
