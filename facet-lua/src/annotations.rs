//! Generate LuaLS type annotations from facet type metadata.

extern crate alloc;

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

use facet_core::{Def, Facet, Field, Shape, StructKind, Type, UserType};

/// Generate LuaLS annotations for a single type.
pub fn to_lua_annotations<T: Facet<'static>>() -> String {
    let mut generator = LuaGenerator::new();
    generator.add_shape(T::SHAPE);
    generator.finish()
}

/// Generator for LuaLS type annotations.
pub struct LuaGenerator {
    /// Generated type definitions, keyed by type name for sorting
    generated: BTreeMap<String, String>,
    /// Types queued for generation
    queue: Vec<&'static Shape>,
    /// Set of type identifiers already seen (to avoid infinite recursion)
    seen: BTreeSet<String>,
}

impl Default for LuaGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl LuaGenerator {
    /// Create a new Lua annotation generator.
    pub const fn new() -> Self {
        Self {
            generated: BTreeMap::new(),
            queue: Vec::new(),
            seen: BTreeSet::new(),
        }
    }

    /// Add a type to generate.
    pub fn add_type<T: Facet<'static>>(&mut self) {
        self.add_shape(T::SHAPE);
    }

    /// Add a shape to generate.
    pub fn add_shape(&mut self, shape: &'static Shape) {
        if !self.seen.contains(shape.type_identifier) {
            self.queue.push(shape);
        }
    }

    /// Finish generation and return the Lua annotation code.
    pub fn finish(mut self) -> String {
        // Process queue until empty
        while let Some(shape) = self.queue.pop() {
            if self.seen.contains(shape.type_identifier) {
                continue;
            }
            self.seen.insert(shape.type_identifier.to_string());
            self.generate_shape(shape);
        }

        // Collect all generated code in sorted order
        let mut output = String::new();
        let mut first = true;
        for code in self.generated.values() {
            if !first {
                output.push('\n');
            }
            first = false;
            output.push_str(code);
        }
        output
    }

    fn generate_shape(&mut self, shape: &'static Shape) {
        let mut output = String::new();

        // Handle transparent wrappers - generate a type alias to the inner type
        if let Some(inner) = shape.inner {
            // type_for_shape handles queuing user types that need generation;
            // no explicit add_shape needed (avoids leaking aliases like `String`)
            let inner_type = self.type_for_shape(inner);
            write_doc_comment(&mut output, shape.doc);
            writeln!(output, "---@alias {} {}", shape.type_identifier, inner_type).unwrap();
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
                writeln!(output, "---@alias {} {}", shape.type_identifier, type_str).unwrap();
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
                // Unit structs map to nil
                writeln!(output, "---@alias {} nil", shape.type_identifier).unwrap();
            }
            StructKind::TupleStruct | StructKind::Tuple if fields.is_empty() => {
                write_doc_comment(output, shape.doc);
                writeln!(output, "---@alias {} nil", shape.type_identifier).unwrap();
            }
            StructKind::TupleStruct if fields.len() == 1 => {
                // Newtype: alias to inner type
                let inner_type = self.type_for_shape(fields[0].shape.get());
                write_doc_comment(output, shape.doc);
                writeln!(output, "---@alias {} {}", shape.type_identifier, inner_type).unwrap();
            }
            StructKind::TupleStruct | StructKind::Tuple => {
                // Tuple struct: use indexed table type for positional info
                write_doc_comment(output, shape.doc);
                let tuple_type = self.tuple_type_string(fields);
                writeln!(output, "---@alias {} {}", shape.type_identifier, tuple_type).unwrap();
            }
            StructKind::Struct => {
                self.generate_class(output, shape, fields);
            }
        }
    }

    fn generate_class(
        &mut self,
        output: &mut String,
        shape: &'static Shape,
        fields: &'static [Field],
    ) {
        write_doc_comment(output, shape.doc);
        self.write_class_fields(output, shape.type_identifier, fields);
    }

    /// Generate a named class definition and insert it into `generated`.
    fn generate_named_class(&mut self, class_name: &str, fields: &'static [Field]) {
        let mut class_output = String::new();
        self.write_class_fields(&mut class_output, class_name, fields);
        self.generated.insert(class_name.to_string(), class_output);
    }

    /// Write `---@class Name` header followed by `---@field` lines for visible fields.
    fn write_class_fields(
        &mut self,
        output: &mut String,
        class_name: &str,
        fields: &'static [Field],
    ) {
        writeln!(output, "---@class {}", class_name).unwrap();
        for field in fields {
            if field.flags.contains(facet_core::FieldFlags::SKIP) {
                continue;
            }
            let (type_string, optional) = self.field_type_info(field);
            let name = field.effective_name();
            for line in field.doc {
                write!(output, "---").unwrap();
                output.push_str(line);
                output.push('\n');
            }
            if optional {
                writeln!(output, "---@field {}? {}", name, type_string).unwrap();
            } else {
                writeln!(output, "---@field {} {}", name, type_string).unwrap();
            }
        }
    }

    /// Get the Lua type string and optional status for a field.
    fn field_type_info(&mut self, field: &Field) -> (String, bool) {
        if let Def::Option(opt) = &field.shape.get().def {
            (self.type_for_shape(opt.t), true)
        } else {
            (self.type_for_shape(field.shape.get()), false)
        }
    }

    /// Build a Lua indexed table type for a tuple: `{ [1]: T1, [2]: T2 }`.
    fn tuple_type_string(&mut self, fields: &[Field]) -> String {
        let parts: Vec<String> = fields
            .iter()
            .enumerate()
            .map(|(i, f)| format!("[{}]: {}", i + 1, self.type_for_shape(f.shape.get())))
            .collect();
        format!("{{ {} }}", parts.join(", "))
    }

    fn generate_enum(
        &mut self,
        output: &mut String,
        shape: &'static Shape,
        enum_type: &facet_core::EnumType,
    ) {
        let tag = shape.get_tag_attr();
        let content = shape.get_content_attr();
        let is_numeric = shape.is_numeric();
        let is_untagged = shape.is_untagged();

        write_doc_comment(output, shape.doc);

        if is_numeric && tag.is_none() {
            // Numeric enum: serializes as integer discriminant
            writeln!(output, "---@alias {} integer", shape.type_identifier).unwrap();
        } else if is_untagged {
            self.generate_untagged_enum(output, shape, enum_type);
        } else {
            match (tag, content) {
                (Some(tag_key), Some(content_key)) => {
                    self.generate_adjacently_tagged_enum(
                        output,
                        shape,
                        enum_type,
                        tag_key,
                        content_key,
                    );
                }
                (Some(tag_key), None) => {
                    self.generate_internally_tagged_enum(output, shape, enum_type, tag_key);
                }
                _ => {
                    // Externally tagged (default)
                    self.generate_externally_tagged_enum(output, shape, enum_type);
                }
            }
        }
    }

    fn generate_externally_tagged_enum(
        &mut self,
        output: &mut String,
        shape: &'static Shape,
        enum_type: &facet_core::EnumType,
    ) {
        let all_unit = enum_type
            .variants
            .iter()
            .all(|v| matches!(v.data.kind, StructKind::Unit));

        if all_unit {
            self.write_string_literal_alias(output, shape, enum_type);
        } else {
            let mut variant_types = Vec::new();
            for variant in enum_type.variants {
                let vtype = self.generate_external_variant(shape, variant);
                variant_types.push((vtype, variant.doc));
            }

            self.write_alias_variants(output, shape.type_identifier, &variant_types);
        }
    }

    /// Generate a single externally-tagged variant. Returns the type reference.
    fn generate_external_variant(
        &mut self,
        parent_shape: &'static Shape,
        variant: &facet_core::Variant,
    ) -> String {
        let variant_name = variant.effective_name();

        match variant.data.kind {
            StructKind::Unit => {
                format!("\"{}\"", variant_name)
            }
            StructKind::TupleStruct if variant.data.fields.len() == 1 => {
                // Newtype variant: { VariantName = value }
                let class_name = format!("{}.{}", parent_shape.type_identifier, variant_name);
                let inner_type = self.type_for_shape(variant.data.fields[0].shape.get());

                let mut class_output = String::new();
                write_doc_comment(&mut class_output, variant.doc);
                writeln!(class_output, "---@class {}", class_name).unwrap();
                writeln!(class_output, "---@field {} {}", variant_name, inner_type).unwrap();
                self.generated.insert(class_name.clone(), class_output);

                class_name
            }
            StructKind::TupleStruct => {
                // Multi-field tuple variant: { VariantName = { [1]=v1, [2]=v2 } }
                let class_name = format!("{}.{}", parent_shape.type_identifier, variant_name);
                let tuple_type = self.tuple_type_string(variant.data.fields);

                let mut class_output = String::new();
                write_doc_comment(&mut class_output, variant.doc);
                writeln!(class_output, "---@class {}", class_name).unwrap();
                writeln!(class_output, "---@field {} {}", variant_name, tuple_type).unwrap();
                self.generated.insert(class_name.clone(), class_output);

                class_name
            }
            _ => {
                // Struct variant: { VariantName = { field1=v1, ... } }
                let class_name = format!("{}.{}", parent_shape.type_identifier, variant_name);
                let data_class_name = format!("{}._", class_name);

                // Outer wrapper class
                let mut class_output = String::new();
                write_doc_comment(&mut class_output, variant.doc);
                writeln!(class_output, "---@class {}", class_name).unwrap();
                writeln!(
                    class_output,
                    "---@field {} {}",
                    variant_name, data_class_name
                )
                .unwrap();
                self.generated.insert(class_name.clone(), class_output);

                // Inner data class
                self.generate_named_class(&data_class_name, variant.data.fields);

                class_name
            }
        }
    }

    fn generate_internally_tagged_enum(
        &mut self,
        output: &mut String,
        shape: &'static Shape,
        enum_type: &facet_core::EnumType,
        tag_key: &str,
    ) {
        let mut variant_types = Vec::new();

        for variant in enum_type.variants {
            let vtype = self.generate_internal_variant(shape, variant, tag_key);
            variant_types.push((vtype, variant.doc));
        }

        self.write_alias_variants(output, shape.type_identifier, &variant_types);
    }

    /// Generate a single internally-tagged variant. Returns the type reference.
    fn generate_internal_variant(
        &mut self,
        parent_shape: &'static Shape,
        variant: &facet_core::Variant,
        tag_key: &str,
    ) -> String {
        let variant_name = variant.effective_name();
        let class_name = format!("{}.{}", parent_shape.type_identifier, variant_name);

        let mut class_output = String::new();
        write_doc_comment(&mut class_output, variant.doc);
        writeln!(class_output, "---@class {}", class_name).unwrap();
        // Tag field with literal string type
        writeln!(class_output, "---@field {} \"{}\"", tag_key, variant_name).unwrap();

        match variant.data.kind {
            StructKind::Unit => {
                // Just the tag field
            }
            StructKind::TupleStruct if variant.data.fields.len() == 1 => {
                // Internally-tagged newtype with struct inner: fields get flattened
                let inner_shape = variant.data.fields[0].shape.get();
                if let Type::User(UserType::Struct(st)) = &inner_shape.ty {
                    for field in st.fields {
                        if field.flags.contains(facet_core::FieldFlags::SKIP) {
                            continue;
                        }
                        let (type_string, optional) = self.field_type_info(field);
                        let name = field.effective_name();
                        if optional {
                            writeln!(class_output, "---@field {}? {}", name, type_string).unwrap();
                        } else {
                            writeln!(class_output, "---@field {} {}", name, type_string).unwrap();
                        }
                    }
                }
            }
            _ => {
                // Struct variant: flatten all fields alongside the tag
                for field in variant.data.fields {
                    if field.flags.contains(facet_core::FieldFlags::SKIP) {
                        continue;
                    }
                    let (type_string, optional) = self.field_type_info(field);
                    let name = field.effective_name();
                    if optional {
                        writeln!(class_output, "---@field {}? {}", name, type_string).unwrap();
                    } else {
                        writeln!(class_output, "---@field {} {}", name, type_string).unwrap();
                    }
                }
            }
        }

        self.generated.insert(class_name.clone(), class_output);
        class_name
    }

    fn generate_adjacently_tagged_enum(
        &mut self,
        output: &mut String,
        shape: &'static Shape,
        enum_type: &facet_core::EnumType,
        tag_key: &str,
        content_key: &str,
    ) {
        let mut variant_types = Vec::new();

        for variant in enum_type.variants {
            let vtype = self.generate_adjacent_variant(shape, variant, tag_key, content_key);
            variant_types.push((vtype, variant.doc));
        }

        self.write_alias_variants(output, shape.type_identifier, &variant_types);
    }

    /// Generate a single adjacently-tagged variant. Returns the type reference.
    fn generate_adjacent_variant(
        &mut self,
        parent_shape: &'static Shape,
        variant: &facet_core::Variant,
        tag_key: &str,
        content_key: &str,
    ) -> String {
        let variant_name = variant.effective_name();
        let class_name = format!("{}.{}", parent_shape.type_identifier, variant_name);

        let mut class_output = String::new();
        write_doc_comment(&mut class_output, variant.doc);
        writeln!(class_output, "---@class {}", class_name).unwrap();
        // Tag field with literal string type
        writeln!(class_output, "---@field {} \"{}\"", tag_key, variant_name).unwrap();

        match variant.data.kind {
            StructKind::Unit => {
                // Just the tag field, no content
            }
            StructKind::TupleStruct if variant.data.fields.len() == 1 => {
                // Content is the single inner value
                let inner_type = self.type_for_shape(variant.data.fields[0].shape.get());
                writeln!(class_output, "---@field {} {}", content_key, inner_type).unwrap();
            }
            StructKind::TupleStruct => {
                // Content is a tuple
                let tuple_type = self.tuple_type_string(variant.data.fields);
                writeln!(class_output, "---@field {} {}", content_key, tuple_type).unwrap();
            }
            _ => {
                // Content is a struct — generate inner class
                let data_class_name = format!("{}._", class_name);
                writeln!(
                    class_output,
                    "---@field {} {}",
                    content_key, data_class_name
                )
                .unwrap();
                self.generate_named_class(&data_class_name, variant.data.fields);
            }
        }

        self.generated.insert(class_name.clone(), class_output);
        class_name
    }

    fn generate_untagged_enum(
        &mut self,
        output: &mut String,
        shape: &'static Shape,
        enum_type: &facet_core::EnumType,
    ) {
        let mut variant_types = Vec::new();

        for variant in enum_type.variants {
            let vtype = self.generate_untagged_variant(shape, variant);
            variant_types.push((vtype, variant.doc));
        }

        self.write_alias_variants(output, shape.type_identifier, &variant_types);
    }

    /// Generate a single untagged variant type. Returns the type reference.
    fn generate_untagged_variant(
        &mut self,
        parent_shape: &'static Shape,
        variant: &facet_core::Variant,
    ) -> String {
        match variant.data.kind {
            StructKind::Unit => "nil".to_string(),
            StructKind::TupleStruct if variant.data.fields.len() == 1 => {
                self.type_for_shape(variant.data.fields[0].shape.get())
            }
            StructKind::TupleStruct => self.tuple_type_string(variant.data.fields),
            _ => {
                // Struct variant: generate a class for the fields
                let variant_name = variant.effective_name();
                let class_name = format!("{}.{}", parent_shape.type_identifier, variant_name);
                self.generate_named_class(&class_name, variant.data.fields);
                class_name
            }
        }
    }

    /// Write a string literal alias for an all-unit enum.
    fn write_string_literal_alias(
        &self,
        output: &mut String,
        shape: &'static Shape,
        enum_type: &facet_core::EnumType,
    ) {
        let has_docs = enum_type.variants.iter().any(|v| !v.doc.is_empty());

        if has_docs {
            writeln!(output, "---@alias {}", shape.type_identifier).unwrap();
            for variant in enum_type.variants {
                let variant_name = variant.effective_name();
                write!(output, "---| \"{}\"", variant_name).unwrap();
                if !variant.doc.is_empty() {
                    let doc_text: Vec<&str> = variant.doc.iter().map(|s| s.trim()).collect();
                    write!(output, " # {}", doc_text.join(" ")).unwrap();
                }
                output.push('\n');
            }
        } else {
            let variants: Vec<String> = enum_type
                .variants
                .iter()
                .map(|v| format!("\"{}\"", v.effective_name()))
                .collect();
            writeln!(
                output,
                "---@alias {} {}",
                shape.type_identifier,
                variants.join(" | ")
            )
            .unwrap();
        }
    }

    /// Write an alias as a union of variant types, using multi-line form when docs are present.
    fn write_alias_variants(
        &self,
        output: &mut String,
        type_name: &str,
        variants: &[(String, &[&str])],
    ) {
        let has_docs = variants.iter().any(|(_, doc)| !doc.is_empty());

        if has_docs {
            writeln!(output, "---@alias {}", type_name).unwrap();
            for (vtype, doc) in variants {
                write!(output, "---| {}", vtype).unwrap();
                if !doc.is_empty() {
                    let doc_text: Vec<&str> = doc.iter().map(|s| s.trim()).collect();
                    write!(output, " # {}", doc_text.join(" ")).unwrap();
                }
                output.push('\n');
            }
        } else {
            let type_strs: Vec<&str> = variants.iter().map(|(t, _)| t.as_str()).collect();
            writeln!(output, "---@alias {} {}", type_name, type_strs.join(" | ")).unwrap();
        }
    }

    fn type_for_shape(&mut self, shape: &'static Shape) -> String {
        // Check Def first - these take precedence over transparent wrappers
        match &shape.def {
            Def::Scalar => self.scalar_type(shape),
            Def::Option(opt) => {
                format!("{}?", self.type_for_shape(opt.t))
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
                format!(
                    "table<{}, {}>",
                    self.type_for_shape(map.k),
                    self.type_for_shape(map.v)
                )
            }
            Def::Pointer(ptr) => match ptr.pointee {
                Some(pointee) => self.type_for_shape(pointee),
                None => "any".to_string(),
            },
            Def::Undefined => {
                // User-defined types - queue for generation and return name
                match &shape.ty {
                    Type::User(UserType::Struct(st)) => {
                        // Handle tuples specially - inline them
                        if st.kind == StructKind::Tuple {
                            if st.fields.is_empty() {
                                "nil".to_string()
                            } else if st.fields.len() == 1 {
                                self.type_for_shape(st.fields[0].shape.get())
                            } else {
                                self.tuple_type_string(st.fields)
                            }
                        } else {
                            self.add_shape(shape);
                            shape.type_identifier.to_string()
                        }
                    }
                    Type::User(UserType::Enum(_)) => {
                        self.add_shape(shape);
                        shape.type_identifier.to_string()
                    }
                    _ => self.inner_type_or_any(shape),
                }
            }
            _ => self.inner_type_or_any(shape),
        }
    }

    /// Get the inner type for transparent wrappers, or "any" as fallback.
    fn inner_type_or_any(&mut self, shape: &'static Shape) -> String {
        match shape.inner {
            Some(inner) => self.type_for_shape(inner),
            None => "any".to_string(),
        }
    }

    /// Get the Lua type for a scalar shape.
    fn scalar_type(&self, shape: &'static Shape) -> String {
        match shape.type_identifier {
            // Strings
            "String" | "str" | "&str" | "Cow" => "string".to_string(),

            // Booleans
            "bool" => "boolean".to_string(),

            // Integers
            "u8" | "u16" | "u32" | "u64" | "u128" | "usize" | "i8" | "i16" | "i32" | "i64"
            | "i128" | "isize" => "integer".to_string(),

            // Floats
            "f32" | "f64" => "number".to_string(),

            // Char as string
            "char" => "string".to_string(),

            // Unknown scalar
            _ => "any".to_string(),
        }
    }
}

/// Write a doc comment as LuaLS comment.
fn write_doc_comment(output: &mut String, doc: &[&str]) {
    let additional: usize = doc.iter().map(|line| 3 + line.len() + 1).sum();
    output.reserve(additional);
    for line in doc {
        output.push_str("---");
        output.push_str(line);
        output.push('\n');
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

        let lua = to_lua_annotations::<User>();
        insta::assert_snapshot!(lua);
    }

    #[test]
    fn test_optional_field() {
        #[derive(Facet)]
        struct Config {
            required: String,
            optional: Option<String>,
        }

        let lua = to_lua_annotations::<Config>();
        insta::assert_snapshot!(lua);
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

        let lua = to_lua_annotations::<Status>();
        insta::assert_snapshot!(lua);
    }

    #[test]
    fn test_vec() {
        #[derive(Facet)]
        struct Data {
            items: Vec<String>,
        }

        let lua = to_lua_annotations::<Data>();
        insta::assert_snapshot!(lua);
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

        let lua = to_lua_annotations::<Outer>();
        insta::assert_snapshot!(lua);
    }

    #[test]
    fn test_unit_struct() {
        #[derive(Facet)]
        struct Empty;

        let lua = to_lua_annotations::<Empty>();
        insta::assert_snapshot!(lua);
    }

    #[test]
    fn test_tuple_struct() {
        #[derive(Facet)]
        struct Point(f32, f64);

        let lua = to_lua_annotations::<Point>();
        insta::assert_snapshot!(lua);
    }

    #[test]
    fn test_newtype_struct() {
        #[derive(Facet)]
        struct UserId(u64);

        let lua = to_lua_annotations::<UserId>();
        insta::assert_snapshot!(lua);
    }

    #[test]
    fn test_hashmap() {
        use std::collections::HashMap;

        #[derive(Facet)]
        struct Registry {
            entries: HashMap<String, i32>,
        }

        let lua = to_lua_annotations::<Registry>();
        insta::assert_snapshot!(lua);
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

        let lua = to_lua_annotations::<Event>();
        insta::assert_snapshot!(lua);
    }

    #[test]
    fn test_transparent_wrapper() {
        #[derive(Facet)]
        #[facet(transparent)]
        struct UserId(String);

        let lua = to_lua_annotations::<UserId>();
        insta::assert_snapshot!(lua);
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

        let lua = to_lua_annotations::<Wrapper>();
        insta::assert_snapshot!(lua);
    }

    #[test]
    fn test_struct_with_tuple_field() {
        #[derive(Facet)]
        struct Container {
            coordinates: (i32, i32),
        }

        let lua = to_lua_annotations::<Container>();
        insta::assert_snapshot!(lua);
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

        let lua = to_lua_annotations::<ValidationErrorCode>();
        insta::assert_snapshot!(lua);
    }

    #[test]
    fn test_enum_struct_variant() {
        #[derive(Facet)]
        #[repr(C)]
        #[allow(dead_code)]
        enum Message {
            TextMessage { content: String },
            ImageUpload { url: String, width: u32 },
        }

        let lua = to_lua_annotations::<Message>();
        insta::assert_snapshot!(lua);
    }

    #[test]
    fn test_multi_type_generation() {
        #[derive(Facet)]
        struct User {
            name: String,
            age: u32,
        }

        #[derive(Facet)]
        #[repr(u8)]
        enum Role {
            Admin,
            User,
        }

        let mut generator = LuaGenerator::new();
        generator.add_type::<User>();
        generator.add_type::<Role>();
        let lua = generator.finish();
        insta::assert_snapshot!(lua);
    }

    #[test]
    fn test_internally_tagged_enum() {
        #[derive(Facet)]
        #[facet(tag = "type")]
        #[repr(C)]
        #[allow(dead_code)]
        enum Request {
            Ping,
            Echo { message: String },
        }

        let lua = to_lua_annotations::<Request>();
        insta::assert_snapshot!(lua);
    }

    #[test]
    fn test_adjacently_tagged_enum() {
        #[derive(Facet)]
        #[facet(tag = "t", content = "c")]
        #[repr(C)]
        #[allow(dead_code)]
        enum Action {
            Stop,
            Move(f64),
            Resize { width: u32, height: u32 },
        }

        let lua = to_lua_annotations::<Action>();
        insta::assert_snapshot!(lua);
    }

    #[test]
    fn test_untagged_enum() {
        #[derive(Facet)]
        #[facet(untagged)]
        #[repr(C)]
        #[allow(dead_code)]
        enum Value {
            Text(String),
            Number(f64),
            Flag(bool),
        }

        let lua = to_lua_annotations::<Value>();
        insta::assert_snapshot!(lua);
    }

    #[test]
    fn test_enum_with_variant_docs() {
        #[derive(Facet)]
        #[repr(u8)]
        enum Color {
            /// The color red
            Red,
            /// The color green
            Green,
            /// The color blue
            Blue,
        }

        let lua = to_lua_annotations::<Color>();
        insta::assert_snapshot!(lua);
    }
}
