//! Generate LuaLS type annotations from facet type metadata.

extern crate alloc;

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

use facet_core::{Def, Facet, Field, Shape, StructKind, Type, UserType};

use crate::consts;

/// Generate LuaLS annotations for a single type.
pub fn to_lua_annotations<T: Facet<'static>>() -> String {
    let mut generator = LuaGenerator::new();
    generator.add_shape(T::SHAPE);
    generator.finish()
}

/// Stable identity for a shape (its static address).
fn shape_key(shape: &'static Shape) -> usize {
    shape as *const Shape as usize
}

/// Generator for LuaLS type annotations.
pub struct LuaGenerator {
    /// Generated type definitions, keyed by assigned type name for sorting
    generated: BTreeMap<String, String>,
    /// Types queued for generation
    queue: Vec<&'static Shape>,
    /// Shapes already generated (to avoid infinite recursion)
    seen: BTreeSet<usize>,
    /// Lua type name assigned to each shape
    names: BTreeMap<usize, String>,
    /// Names already taken, to detect same-identifier collisions
    taken: BTreeSet<String>,
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
            names: BTreeMap::new(),
            taken: BTreeSet::new(),
        }
    }

    /// Add a type to generate.
    pub fn add_type<T: Facet<'static>>(&mut self) {
        self.add_shape(T::SHAPE);
    }

    /// Add a shape to generate.
    pub fn add_shape(&mut self, shape: &'static Shape) {
        if !self.seen.contains(&shape_key(shape)) {
            self.queue.push(shape);
        }
    }

    /// The Lua type name assigned to a shape.
    ///
    /// The first shape with a given identifier gets the bare name; a
    /// different shape with the same identifier (same-named types from
    /// different modules) is disambiguated by its module path, then by a
    /// numeric suffix.
    fn name_for_shape(&mut self, shape: &'static Shape) -> String {
        let key = shape_key(shape);
        if let Some(name) = self.names.get(&key) {
            return name.clone();
        }
        let base = shape.type_identifier;
        let name = if !self.taken.contains(base) {
            base.to_string()
        } else {
            let qualified = shape
                .module_path
                .map(|m| format!("{}.{}", m.replace("::", "."), base));
            match qualified {
                Some(q) if !self.taken.contains(&q) => q,
                _ => {
                    let mut i = 2;
                    loop {
                        let candidate = format!("{}_{}", base, i);
                        if !self.taken.contains(&candidate) {
                            break candidate;
                        }
                        i += 1;
                    }
                }
            }
        };
        self.taken.insert(name.clone());
        self.names.insert(key, name.clone());
        name
    }

    /// Finish generation and return the Lua annotation code.
    pub fn finish(mut self) -> String {
        // Process queue until empty
        while let Some(shape) = self.queue.pop() {
            if self.seen.contains(&shape_key(shape)) {
                continue;
            }
            self.seen.insert(shape_key(shape));
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
        let name = self.name_for_shape(shape);

        // Handle proxy types using the shape they serialize through.
        if let Some(proxy_def) = shape.proxy {
            let proxy_type = self.type_for_shape(proxy_def.shape);
            write_doc_comment(&mut output, shape.doc);
            writeln!(output, "---@alias {} {}", name, proxy_type).unwrap();
            self.generated.insert(name, output);
            return;
        }

        // Handle transparent wrappers - generate a type alias to the inner type
        if let Some(inner) = shape.inner {
            // type_for_shape handles queuing user types that need generation;
            // no explicit add_shape needed (avoids leaking aliases like `String`)
            let inner_type = self.type_for_shape(inner);
            write_doc_comment(&mut output, shape.doc);
            writeln!(output, "---@alias {} {}", name, inner_type).unwrap();
            self.generated.insert(name, output);
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
                writeln!(output, "---@alias {} {}", name, type_str).unwrap();
            }
        }

        self.generated.insert(name, output);
    }

    fn generate_struct(
        &mut self,
        output: &mut String,
        shape: &'static Shape,
        fields: &'static [Field],
        kind: StructKind,
    ) {
        let name = self.name_for_shape(shape);
        match kind {
            StructKind::Unit => {
                write_doc_comment(output, shape.doc);
                // Unit structs map to nil
                writeln!(output, "---@alias {} nil", name).unwrap();
            }
            StructKind::TupleStruct | StructKind::Tuple if fields.is_empty() => {
                write_doc_comment(output, shape.doc);
                writeln!(output, "---@alias {} nil", name).unwrap();
            }
            StructKind::TupleStruct if fields.len() == 1 => {
                // Newtype: alias to inner type
                let inner_type = self.type_for_shape(fields[0].shape.get());
                write_doc_comment(output, shape.doc);
                writeln!(output, "---@alias {} {}", name, inner_type).unwrap();
            }
            StructKind::TupleStruct | StructKind::Tuple => {
                // Tuple struct: positional LuaLS tuple type
                write_doc_comment(output, shape.doc);
                let tuple_type = self.tuple_type_string(fields);
                writeln!(output, "---@alias {} {}", name, tuple_type).unwrap();
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
        let name = self.name_for_shape(shape);
        // `deny_unknown_fields` maps to LuaLS exact classes: no extra fields
        let exact = shape.has_deny_unknown_fields_attr();
        self.write_class_fields(output, &name, exact, fields);
    }

    /// Generate a named class definition and insert it into `generated`.
    fn generate_named_class(&mut self, class_name: &str, fields: &'static [Field]) {
        let mut class_output = String::new();
        self.write_class_fields(&mut class_output, class_name, false, fields);
        self.generated.insert(class_name.to_string(), class_output);
    }

    fn insert_class_output(
        &mut self,
        class_name: &str,
        mut class_output: String,
        indexed_fields: String,
    ) {
        append_indexed_fields(&mut class_output, indexed_fields);
        self.generated.insert(class_name.to_string(), class_output);
    }

    /// Write `---@class Name` header followed by `---@field` lines for visible fields.
    fn write_class_fields(
        &mut self,
        output: &mut String,
        class_name: &str,
        exact: bool,
        fields: &'static [Field],
    ) {
        let attr = if exact { "(exact) " } else { "" };
        writeln!(output, "---@class {}{}", attr, class_name).unwrap();
        let mut indexed_fields = String::new();
        let mut flatten_stack = Vec::new();
        self.write_field_annotations(
            output,
            &mut indexed_fields,
            fields,
            false,
            &mut flatten_stack,
        );
        append_indexed_fields(output, indexed_fields);
    }

    fn write_field_annotations(
        &mut self,
        output: &mut String,
        indexed_output: &mut String,
        fields: &'static [Field],
        force_optional: bool,
        flatten_stack: &mut Vec<*const Shape>,
    ) {
        for field in fields {
            if field.should_skip_serializing_unconditional() {
                continue;
            }

            if field.is_flattened() {
                let (inner_shape, parent_is_optional) =
                    Self::unwrap_to_inner_shape(field.shape.get());
                if let Type::User(UserType::Struct(st)) = &inner_shape.ty {
                    let inner_key = inner_shape as *const Shape;
                    if flatten_stack.contains(&inner_key) {
                        continue;
                    }
                    flatten_stack.push(inner_key);
                    self.write_field_annotations(
                        output,
                        indexed_output,
                        st.fields,
                        force_optional || parent_is_optional,
                        flatten_stack,
                    );
                    flatten_stack.pop();
                    continue;
                }
            }

            let (type_string, optional) = self.field_type_info(field, force_optional);
            write_partitioned_field_with_doc(
                output,
                indexed_output,
                field.effective_name(),
                optional,
                &type_string,
                field.doc,
            );
        }
    }

    /// Get the Lua type string and optional status for a field.
    fn field_type_info(&mut self, field: &Field, force_optional: bool) -> (String, bool) {
        if let Def::Option(opt) = &field.shape.get().def {
            (self.type_for_shape(opt.t), true)
        } else {
            (
                self.type_for_shape(field.shape.get()),
                force_optional || field.default.is_some(),
            )
        }
    }

    fn unwrap_to_inner_shape(shape: &'static Shape) -> (&'static Shape, bool) {
        if let Def::Option(opt) = &shape.def {
            let (inner, _) = Self::unwrap_to_inner_shape(opt.t);
            return (inner, true);
        }
        if let Def::Pointer(ptr) = &shape.def
            && let Some(pointee) = ptr.pointee
        {
            return Self::unwrap_to_inner_shape(pointee);
        }
        if let Some(inner) = shape.inner {
            return Self::unwrap_to_inner_shape(inner);
        }
        if let Some(proxy_def) = shape.proxy {
            return Self::unwrap_to_inner_shape(proxy_def.shape);
        }
        (shape, false)
    }

    /// Build a LuaLS tuple type: `[T1, T2]` (fixed length, positionally typed).
    fn tuple_type_string(&mut self, fields: &[Field]) -> String {
        let parts: Vec<String> = fields
            .iter()
            .map(|f| self.type_for_shape(f.shape.get()))
            .collect();
        format!("[{}]", parts.join(", "))
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
            let name = self.name_for_shape(shape);
            writeln!(output, "---@alias {} integer", name).unwrap();
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

        let name = self.name_for_shape(shape);
        if all_unit {
            self.write_string_literal_alias(output, &name, enum_type);
        } else {
            let mut variant_types = Vec::new();
            for variant in enum_type.variants {
                let vtype = self.generate_external_variant(shape, variant);
                variant_types.push((vtype, variant.doc));
            }

            self.write_alias_variants(output, &name, &variant_types);
        }
    }

    /// Generate a single externally-tagged variant. Returns the type reference.
    fn generate_external_variant(
        &mut self,
        parent_shape: &'static Shape,
        variant: &facet_core::Variant,
    ) -> String {
        let variant_name = variant.effective_name();
        let variant_type_name = variant.name;

        match variant.data.kind {
            StructKind::Unit => lua_string_literal(variant_name),
            StructKind::TupleStruct if variant.data.fields.len() == 1 => {
                // Newtype variant: { VariantName = value }
                let class_name = format!(
                    "{}.{}",
                    self.name_for_shape(parent_shape),
                    variant_type_name
                );
                let inner_type = self.type_for_shape(variant.data.fields[0].shape.get());

                let (mut class_output, mut indexed_fields) =
                    class_output_with_header(variant.doc, &class_name);
                write_partitioned_field_annotation(
                    &mut class_output,
                    &mut indexed_fields,
                    variant_name,
                    false,
                    &inner_type,
                );
                self.insert_class_output(&class_name, class_output, indexed_fields);

                class_name
            }
            StructKind::TupleStruct => {
                // Multi-field tuple variant: { VariantName = { [1]=v1, [2]=v2 } }
                let class_name = format!(
                    "{}.{}",
                    self.name_for_shape(parent_shape),
                    variant_type_name
                );
                let tuple_type = self.tuple_type_string(variant.data.fields);

                let (mut class_output, mut indexed_fields) =
                    class_output_with_header(variant.doc, &class_name);
                write_partitioned_field_annotation(
                    &mut class_output,
                    &mut indexed_fields,
                    variant_name,
                    false,
                    &tuple_type,
                );
                self.insert_class_output(&class_name, class_output, indexed_fields);

                class_name
            }
            _ => {
                // Struct variant: { VariantName = { field1=v1, ... } }
                let class_name = format!(
                    "{}.{}",
                    self.name_for_shape(parent_shape),
                    variant_type_name
                );
                let data_class_name = format!("{}._", class_name);

                // Outer wrapper class
                let (mut class_output, mut indexed_fields) =
                    class_output_with_header(variant.doc, &class_name);
                write_partitioned_field_annotation(
                    &mut class_output,
                    &mut indexed_fields,
                    variant_name,
                    false,
                    &data_class_name,
                );
                self.insert_class_output(&class_name, class_output, indexed_fields);

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

        let name = self.name_for_shape(shape);
        self.write_alias_variants(output, &name, &variant_types);
    }

    /// Generate a single internally-tagged variant. Returns the type reference.
    fn generate_internal_variant(
        &mut self,
        parent_shape: &'static Shape,
        variant: &facet_core::Variant,
        tag_key: &str,
    ) -> String {
        let variant_name = variant.effective_name();
        let class_name = format!("{}.{}", self.name_for_shape(parent_shape), variant.name);

        let (mut class_output, mut indexed_fields) =
            class_output_with_header(variant.doc, &class_name);
        // Tag field with literal string type
        write_partitioned_field_annotation(
            &mut class_output,
            &mut indexed_fields,
            tag_key,
            false,
            &lua_string_literal(variant_name),
        );

        match variant.data.kind {
            StructKind::Unit => {
                // Just the tag field
            }
            StructKind::TupleStruct if variant.data.fields.len() == 1 => {
                // Internally-tagged newtype with struct inner: fields get flattened
                let inner_shape = variant.data.fields[0].shape.get();
                if let Type::User(UserType::Struct(st)) = &inner_shape.ty {
                    let mut flatten_stack = vec![inner_shape as *const Shape];
                    self.write_field_annotations(
                        &mut class_output,
                        &mut indexed_fields,
                        st.fields,
                        false,
                        &mut flatten_stack,
                    );
                }
            }
            _ => {
                // Struct variant: flatten all fields alongside the tag
                let mut flatten_stack = Vec::new();
                self.write_field_annotations(
                    &mut class_output,
                    &mut indexed_fields,
                    variant.data.fields,
                    false,
                    &mut flatten_stack,
                );
            }
        }

        self.insert_class_output(&class_name, class_output, indexed_fields);
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

        let name = self.name_for_shape(shape);
        self.write_alias_variants(output, &name, &variant_types);
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
        let class_name = format!("{}.{}", self.name_for_shape(parent_shape), variant.name);

        let (mut class_output, mut indexed_fields) =
            class_output_with_header(variant.doc, &class_name);
        // Tag field with literal string type
        write_partitioned_field_annotation(
            &mut class_output,
            &mut indexed_fields,
            tag_key,
            false,
            &lua_string_literal(variant_name),
        );

        match variant.data.kind {
            StructKind::Unit => {
                // Just the tag field, no content
            }
            StructKind::TupleStruct if variant.data.fields.len() == 1 => {
                // Content is the single inner value
                let inner_type = self.type_for_shape(variant.data.fields[0].shape.get());
                write_partitioned_field_annotation(
                    &mut class_output,
                    &mut indexed_fields,
                    content_key,
                    false,
                    &inner_type,
                );
            }
            StructKind::TupleStruct => {
                // Content is a tuple
                let tuple_type = self.tuple_type_string(variant.data.fields);
                write_partitioned_field_annotation(
                    &mut class_output,
                    &mut indexed_fields,
                    content_key,
                    false,
                    &tuple_type,
                );
            }
            _ => {
                // Content is a struct — generate inner class
                let data_class_name = format!("{}._", class_name);
                write_partitioned_field_annotation(
                    &mut class_output,
                    &mut indexed_fields,
                    content_key,
                    false,
                    &data_class_name,
                );
                self.generate_named_class(&data_class_name, variant.data.fields);
            }
        }

        self.insert_class_output(&class_name, class_output, indexed_fields);
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

        let name = self.name_for_shape(shape);
        self.write_alias_variants(output, &name, &variant_types);
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
                let class_name = format!("{}.{}", self.name_for_shape(parent_shape), variant.name);
                self.generate_named_class(&class_name, variant.data.fields);
                class_name
            }
        }
    }

    /// Write a string literal alias for an all-unit enum.
    fn write_string_literal_alias(
        &self,
        output: &mut String,
        type_name: &str,
        enum_type: &facet_core::EnumType,
    ) {
        let has_docs = enum_type.variants.iter().any(|v| !v.doc.is_empty());

        if has_docs {
            writeln!(output, "---@alias {}", type_name).unwrap();
            for variant in enum_type.variants {
                let variant_name = variant.effective_name();
                write!(output, "---| {}", lua_string_literal(variant_name)).unwrap();
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
                .map(|v| lua_string_literal(v.effective_name()))
                .collect();
            writeln!(output, "---@alias {} {}", type_name, variants.join(" | ")).unwrap();
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
        if let Some(proxy_def) = shape.proxy {
            return self.type_for_shape(proxy_def.shape);
        }

        // Check Def first - these take precedence over transparent wrappers
        match &shape.def {
            Def::Scalar => self.scalar_type(shape),
            Def::Option(opt) => {
                // `T?` is only valid at the end of a whole type expression in
                // LuaLS; a parenthesized union works in any position
                // (unions, arrays, table values).
                format!("({}|nil)", self.type_for_shape(opt.t))
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
                            self.name_for_shape(shape)
                        }
                    }
                    Type::User(UserType::Enum(_)) => {
                        self.add_shape(shape);
                        self.name_for_shape(shape)
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

            // Integers that always fit Lua's signed 64-bit range
            "u8" | "u16" | "u32" | "i8" | "i16" | "i32" | "i64" | "isize" => "integer".to_string(),

            // Integers that can exceed Lua's range serialize as decimal
            // strings above i64::MAX (and 128-bit types always may), so the
            // annotation admits both encodings.
            "u64" | "u128" | "i128" | "usize" => "(integer|string)".to_string(),

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

fn class_output_with_header(doc: &[&str], class_name: &str) -> (String, String) {
    let mut output = String::new();
    write_doc_comment(&mut output, doc);
    writeln!(output, "---@class {}", class_name).unwrap();
    (output, String::new())
}

fn append_indexed_fields(output: &mut String, indexed_fields: String) {
    output.push_str(&indexed_fields);
}

fn write_field_annotation(output: &mut String, name: &str, optional: bool, type_string: &str) {
    let opt = if optional { "?" } else { "" };
    if consts::is_lua_identifier(name) {
        writeln!(output, "---@field {}{} {}", name, opt, type_string).unwrap();
    } else {
        // LuaLS accepts a full type expression as a bracketed field key, so a
        // string-literal key annotates the exact serialized key.
        writeln!(
            output,
            "---@field [{}]{} {}",
            lua_string_literal(name),
            opt,
            type_string
        )
        .unwrap();
    }
}

fn write_partitioned_field_with_doc(
    output: &mut String,
    indexed_output: &mut String,
    name: &str,
    optional: bool,
    type_string: &str,
    doc: &[&str],
) {
    let output = if consts::is_lua_identifier(name) {
        output
    } else {
        indexed_output
    };
    write_doc_comment(output, doc);
    write_field_annotation(output, name, optional, type_string);
}

fn write_partitioned_field_annotation(
    output: &mut String,
    indexed_output: &mut String,
    name: &str,
    optional: bool,
    type_string: &str,
) {
    if consts::is_lua_identifier(name) {
        write_field_annotation(output, name, optional, type_string);
    } else {
        write_field_annotation(indexed_output, name, optional, type_string);
    }
}

fn lua_string_literal(value: &str) -> String {
    let mut output = String::new();
    output.push('"');
    for c in value.chars() {
        match c {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            c if c.is_ascii_control() => write!(output, "\\{:03}", c as u8).unwrap(),
            c => output.push(c),
        }
    }
    output.push('"');
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet::Facet;

    mod first_dup {
        use facet::Facet;
        #[derive(Facet)]
        pub struct Dup {
            pub x: i32,
        }
    }
    mod second_dup {
        use facet::Facet;
        #[derive(Facet)]
        pub struct Dup {
            pub y: bool,
        }
    }

    #[test]
    fn test_option_in_type_position_is_parenthesized_union() {
        // `integer?[]` misparses in LuaLS (`?` is only valid at the end of a
        // whole type expression); nested options must use `(T|nil)`.
        #[derive(Facet)]
        struct S {
            xs: Vec<Option<i32>>,
        }
        let lua = to_lua_annotations::<S>();
        assert!(lua.contains("---@field xs (integer|nil)[]"), "got:\n{lua}");
    }

    #[test]
    fn test_same_named_types_get_distinct_annotations() {
        #[derive(Facet)]
        struct Holder {
            a: first_dup::Dup,
            b: second_dup::Dup,
        }
        let lua = to_lua_annotations::<Holder>();
        // Both Dup types must be generated, under distinct names
        assert_eq!(lua.matches("---@class").count(), 3, "got:\n{lua}");
        assert!(lua.contains("---@field x integer"), "got:\n{lua}");
        assert!(lua.contains("---@field y boolean"), "got:\n{lua}");
        // The two field references must point at different type names
        let a_line = lua.lines().find(|l| l.starts_with("---@field a ")).unwrap();
        let b_line = lua.lines().find(|l| l.starts_with("---@field b ")).unwrap();
        assert_ne!(
            a_line["---@field a ".len()..],
            b_line["---@field b ".len()..]
        );
    }

    #[test]
    fn test_deny_unknown_fields_emits_exact_class() {
        #[derive(Facet)]
        #[facet(deny_unknown_fields)]
        struct Strict {
            a: i32,
        }
        let lua = to_lua_annotations::<Strict>();
        assert!(lua.contains("---@class (exact) Strict"), "got:\n{lua}");
    }

    #[test]
    fn test_tuple_uses_luals_tuple_syntax() {
        #[derive(Facet)]
        struct Point(f32, f64);
        let lua = to_lua_annotations::<Point>();
        assert!(
            lua.contains("---@alias Point [number, number]"),
            "got:\n{lua}"
        );
    }

    #[test]
    fn test_integers_above_lua_range_annotate_as_union() {
        // u64/usize/u128/i128 may serialize as decimal strings; the
        // annotation must admit both encodings. u32 always fits.
        #[derive(Facet)]
        struct S {
            a: u64,
            b: u128,
            c: i128,
            d: usize,
            e: u32,
        }
        let lua = to_lua_annotations::<S>();
        assert!(lua.contains("---@field a (integer|string)"), "got:\n{lua}");
        assert!(lua.contains("---@field b (integer|string)"), "got:\n{lua}");
        assert!(lua.contains("---@field c (integer|string)"), "got:\n{lua}");
        assert!(lua.contains("---@field d (integer|string)"), "got:\n{lua}");
        assert!(lua.contains("---@field e integer"), "got:\n{lua}");
    }

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
    fn test_default_field_is_optional() {
        #[derive(Facet)]
        struct Config {
            required: String,
            #[facet(default)]
            retries: u32,
        }

        let lua = to_lua_annotations::<Config>();
        assert!(lua.contains("---@field required string"));
        assert!(lua.contains("---@field retries? integer"));
    }

    #[test]
    fn test_skip_serializing_field_is_omitted() {
        #[derive(Facet)]
        struct Config {
            visible: String,
            #[facet(skip_serializing)]
            internal: String,
        }

        let lua = to_lua_annotations::<Config>();
        assert!(lua.contains("---@field visible string"));
        assert!(!lua.contains("internal"));
    }

    #[test]
    fn test_flattened_struct_fields_are_inlined() {
        #[derive(Facet)]
        struct Coords {
            x: i32,
            y: i32,
        }

        #[derive(Facet)]
        struct NamedPoint {
            name: String,
            #[facet(flatten)]
            coords: Coords,
        }

        let lua = to_lua_annotations::<NamedPoint>();
        assert!(lua.contains("---@field name string"));
        assert!(lua.contains("---@field x integer"));
        assert!(lua.contains("---@field y integer"));
        assert!(!lua.contains("coords"));
    }

    #[test]
    fn test_proxy_to_scalar_uses_proxy_type() {
        #[derive(Facet)]
        #[facet(proxy = String)]
        struct UserId(u64);

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

        let lua = to_lua_annotations::<UserId>();
        assert!(lua.contains("---@alias UserId string"));
    }

    #[test]
    fn test_invalid_lua_field_names_use_luals_index_fields() {
        #[derive(Facet)]
        struct Config {
            #[facet(rename = "end")]
            keyword: String,
            #[facet(rename = "@type")]
            special: String,
            #[facet(rename = "first-name")]
            kebab: String,
        }

        let lua = to_lua_annotations::<Config>();
        assert!(lua.contains(r#"---@field ["end"] string"#));
        assert!(lua.contains(r#"---@field ["@type"] string"#));
        assert!(lua.contains(r#"---@field ["first-name"] string"#));
    }

    #[test]
    fn test_luals_index_fields_follow_named_fields() {
        #[derive(Facet)]
        struct Config {
            #[facet(rename = "first-name")]
            renamed: String,
            valid: u32,
        }

        let lua = to_lua_annotations::<Config>();
        let named_pos = lua.find("---@field valid integer").unwrap();
        let indexed_pos = lua.find(r#"---@field ["first-name"] string"#).unwrap();
        assert!(named_pos < indexed_pos);
    }

    #[test]
    fn test_invalid_lua_tag_and_content_keys_use_luals_index_fields() {
        #[derive(Facet)]
        #[facet(tag = "@type", content = "payload-value")]
        #[repr(C)]
        #[allow(dead_code)]
        enum Event {
            Text(String),
        }

        let lua = to_lua_annotations::<Event>();
        assert!(lua.contains(r#"---@field ["@type"] "Text""#));
        assert!(lua.contains(r#"---@field ["payload-value"] string"#));
    }

    #[test]
    fn test_renamed_complex_variant_uses_safe_type_name() {
        #[derive(Facet)]
        #[repr(C)]
        #[allow(dead_code)]
        enum Event {
            #[facet(rename = "@event")]
            Renamed { value: i32 },
        }

        let lua = to_lua_annotations::<Event>();
        assert!(lua.contains("---@class Event.Renamed"));
        assert!(lua.contains(r#"---@field ["@event"] Event.Renamed._"#));
        assert!(!lua.contains("---@class Event.@event"));
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
