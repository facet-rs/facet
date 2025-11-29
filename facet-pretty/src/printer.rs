//! Pretty printer implementation for Facet types

use alloc::collections::BTreeMap;
use core::{
    fmt::{self, Write},
    hash::{Hash, Hasher},
    str,
};
use std::hash::DefaultHasher;

use facet_core::{
    Def, DynDateTimeKind, DynValueKind, Facet, Field, FieldFlags, PointerType, PrimitiveType,
    SequenceType, Shape, StructKind, StructType, TextualType, Type, TypeNameOpts, UserType,
};
use facet_reflect::{Peek, ValueId};

use crate::color::ColorGenerator;

/// A formatter for pretty-printing Facet types
pub struct PrettyPrinter {
    /// usize::MAX is a special value that means indenting with tabs instead of spaces
    indent_size: usize,
    max_depth: Option<usize>,
    color_generator: ColorGenerator,
    use_colors: bool,
    list_u8_as_bytes: bool,
}

impl Default for PrettyPrinter {
    fn default() -> Self {
        Self {
            indent_size: 2,
            max_depth: None,
            color_generator: ColorGenerator::default(),
            use_colors: std::env::var_os("NO_COLOR").is_none(),
            list_u8_as_bytes: true,
        }
    }
}

impl PrettyPrinter {
    /// Create a new PrettyPrinter with default settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the indentation size
    pub fn with_indent_size(mut self, size: usize) -> Self {
        self.indent_size = size;
        self
    }

    /// Set the maximum depth for recursive printing
    pub fn with_max_depth(mut self, depth: usize) -> Self {
        self.max_depth = Some(depth);
        self
    }

    /// Set the color generator
    pub fn with_color_generator(mut self, generator: ColorGenerator) -> Self {
        self.color_generator = generator;
        self
    }

    /// Enable or disable colors
    pub fn with_colors(mut self, use_colors: bool) -> Self {
        self.use_colors = use_colors;
        self
    }

    /// Format a value to a string
    pub fn format<'a, T: ?Sized + Facet<'a>>(&self, value: &T) -> String {
        let value = Peek::new(value);

        let mut output = String::new();
        self.format_peek_internal(value, &mut output, &mut BTreeMap::new())
            .expect("Formatting failed");

        output
    }

    /// Format a value to a formatter
    pub fn format_to<'a, T: ?Sized + Facet<'a>>(
        &self,
        value: &T,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        let value = Peek::new(value);
        self.format_peek_internal(value, f, &mut BTreeMap::new())
    }

    /// Format a value to a string
    pub fn format_peek(&self, value: Peek<'_, '_>) -> String {
        let mut output = String::new();
        self.format_peek_internal(value, &mut output, &mut BTreeMap::new())
            .expect("Formatting failed");
        output
    }

    pub(crate) fn shape_chunkiness(shape: &Shape) -> usize {
        let mut shape = shape;
        while let Type::Pointer(PointerType::Reference(inner)) = shape.ty {
            shape = inner.target;
        }

        match shape.ty {
            Type::Pointer(_) | Type::Primitive(_) => 1,
            Type::Sequence(SequenceType::Array(ty)) => {
                Self::shape_chunkiness(ty.t).saturating_mul(ty.n)
            }
            Type::Sequence(SequenceType::Slice(_)) => usize::MAX,
            Type::User(ty) => match ty {
                UserType::Struct(ty) => {
                    let mut sum = 0usize;
                    for field in ty.fields {
                        sum = sum.saturating_add(Self::shape_chunkiness((field.shape)()));
                    }
                    sum
                }
                UserType::Enum(ty) => {
                    let mut max = 0usize;
                    for variant in ty.variants {
                        max = Ord::max(max, {
                            let mut sum = 0usize;
                            for field in variant.data.fields {
                                sum = sum.saturating_add(Self::shape_chunkiness((field.shape)()));
                            }
                            sum
                        })
                    }
                    max
                }
                UserType::Opaque | UserType::Union(_) => 1,
            },
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn format_peek_internal_(
        &self,
        value: Peek<'_, '_>,
        f: &mut dyn Write,
        visited: &mut BTreeMap<ValueId, usize>,
        format_depth: usize,
        type_depth: usize,
        short: bool,
    ) -> fmt::Result {
        let mut value = value;
        while let Ok(ptr) = value.into_pointer()
            && let Some(pointee) = ptr.borrow_inner()
        {
            value = pointee;
        }
        let shape = value.shape();

        if let Some(prev_type_depth) = visited.insert(value.id(), type_depth) {
            self.write_type_name(f, &value)?;
            self.write_punctuation(f, " { ")?;
            self.write_comment(
                f,
                &format!(
                    "/* cycle detected at {} (first seen at type_depth {}) */",
                    value.id(),
                    prev_type_depth,
                ),
            )?;
            visited.remove(&value.id());
            return Ok(());
        }

        match (shape.def, shape.ty) {
            (_, Type::Primitive(PrimitiveType::Textual(TextualType::Str))) => {
                let value = value.get::<str>().unwrap();
                let mut hashes = 0usize;

                let mut rest = value;
                while let Some(idx) = rest.find('"') {
                    rest = &rest[idx + 1..];
                    let before = rest.len();
                    rest = rest.trim_start_matches('#');
                    let after = rest.len();
                    let count = before - after;
                    hashes = Ord::max(hashes, 1 + count);
                }

                let pad = "";
                let width = hashes.saturating_sub(1);
                if hashes > 0 {
                    write!(f, "r{pad:#<width$}")?;
                }
                write!(f, "\"")?;
                if self.use_colors {
                    write!(f, "\x1b[33m{value}\x1b[0m")?; // yellow
                } else {
                    write!(f, "{value}")?;
                }
                write!(f, "\"")?;
                if hashes > 0 {
                    write!(f, "{pad:#<width$}")?;
                }
            }
            (Def::Scalar, _) => self.format_scalar(value, f)?,
            (Def::Option(_), _) => {
                let option = value.into_option().unwrap();

                // Print the Option name
                self.write_type_name(f, &value)?;

                if let Some(inner) = option.value() {
                    self.write_punctuation(f, "Some(")?;
                    self.format_peek_internal_(
                        inner,
                        f,
                        visited,
                        format_depth,
                        type_depth + 1,
                        short,
                    )?;
                    self.write_punctuation(f, ")")?;
                } else {
                    self.write_punctuation(f, "None")?;
                }
            }

            (_, Type::Pointer(PointerType::Raw(_) | PointerType::Function(_))) => {
                self.write_type_name(f, &value)?;
                let addr = unsafe { value.data().read::<*const ()>() };
                let value = Peek::new(&addr);
                self.format_scalar(value, f)?;
            }

            (_, Type::User(UserType::Union(_))) => {
                if !short {
                    for &line in shape.doc {
                        self.write_comment(f, &format!("///{line}"))?;
                        writeln!(f)?;
                        self.indent(f, format_depth)?;
                    }
                }
                self.write_type_name(f, &value)?;

                self.write_punctuation(f, " { ")?;
                self.write_comment(f, "/* contents of untagged union */")?;
                self.write_punctuation(f, " }")?;
            }

            (
                _,
                Type::User(UserType::Struct(
                    ty @ StructType {
                        kind: StructKind::Tuple | StructKind::TupleStruct,
                        ..
                    },
                )),
            ) => {
                if !short {
                    for &line in shape.doc {
                        self.write_comment(f, &format!("///{line}"))?;
                        writeln!(f)?;
                        self.indent(f, format_depth)?;
                    }
                }

                self.write_type_name(f, &value)?;
                if matches!(ty.kind, StructKind::Tuple) {
                    write!(f, " ")?;
                }
                let value = value.into_struct().unwrap();

                let fields = ty.fields;
                self.format_tuple_fields(
                    &|i| value.field(i).unwrap(),
                    f,
                    visited,
                    format_depth,
                    type_depth,
                    fields,
                    short,
                    matches!(ty.kind, StructKind::Tuple),
                )?;
            }

            (
                _,
                Type::User(UserType::Struct(
                    ty @ StructType {
                        kind: StructKind::Struct | StructKind::Unit,
                        ..
                    },
                )),
            ) => {
                if !short {
                    for &line in shape.doc {
                        self.write_comment(f, &format!("///{line}"))?;
                        writeln!(f)?;
                        self.indent(f, format_depth)?;
                    }
                }

                self.write_type_name(f, &value)?;

                if matches!(ty.kind, StructKind::Struct) {
                    let value = value.into_struct().unwrap();
                    self.format_struct_fields(
                        &|i| value.field(i).unwrap(),
                        f,
                        visited,
                        format_depth,
                        type_depth,
                        ty.fields,
                        short,
                    )?;
                }
            }

            (_, Type::User(UserType::Enum(_))) => {
                let enum_peek = value.into_enum().unwrap();
                match enum_peek.active_variant() {
                    Err(_) => {
                        // Print the enum name
                        self.write_type_name(f, &value)?;
                        self.write_punctuation(f, " {")?;
                        self.write_comment(f, " /* cannot determine variant */ ")?;
                        self.write_punctuation(f, "}")?;
                    }
                    Ok(variant) => {
                        if !short {
                            for &line in shape.doc {
                                self.write_comment(f, &format!("///{line}"))?;
                                writeln!(f)?;
                                self.indent(f, format_depth)?;
                            }
                            for &line in variant.doc {
                                self.write_comment(f, &format!("///{line}"))?;
                                writeln!(f)?;
                                self.indent(f, format_depth)?;
                            }
                        }
                        self.write_type_name(f, &value)?;
                        self.write_punctuation(f, "::")?;

                        // Variant docs are already handled above

                        // Get the active variant name - we've already checked above that we can get it
                        // This is the same variant, but we're repeating the code here to ensure consistency

                        // Apply color for variant name
                        if self.use_colors {
                            if self.use_colors {
                                write!(f, "\x1b[1m{}\x1b[0m", variant.name)?;
                            // bold
                            } else {
                                write!(f, "{}", variant.name)?;
                            }
                        } else {
                            write!(f, "{}", variant.name)?;
                        }

                        // Process the variant fields based on the variant kind
                        match variant.data.kind {
                            StructKind::Unit => {
                                // Unit variant has no fields, nothing more to print
                            }
                            StructKind::Struct => self.format_struct_fields(
                                &|i| enum_peek.field(i).unwrap().unwrap(),
                                f,
                                visited,
                                format_depth,
                                type_depth,
                                variant.data.fields,
                                short,
                            )?,
                            _ => self.format_tuple_fields(
                                &|i| enum_peek.field(i).unwrap().unwrap(),
                                f,
                                visited,
                                format_depth,
                                type_depth,
                                variant.data.fields,
                                short,
                                false,
                            )?,
                        }
                    }
                };
            }

            _ if value.into_list_like().is_ok() => {
                let list = value.into_list_like().unwrap();

                // When recursing into a list, always increment format_depth
                // Only increment type_depth if we're moving to a different address

                // Print the list name
                self.write_type_name(f, &value)?;

                if !list.is_empty() {
                    if list.def().t().is_type::<u8>() && self.list_u8_as_bytes {
                        self.write_punctuation(f, " [")?;
                        for (idx, item) in list.iter().enumerate() {
                            if !short && idx % 16 == 0 {
                                writeln!(f)?;
                                self.indent(f, format_depth + 1)?;
                            }
                            write!(f, " ")?;

                            let byte = *item.get::<u8>().unwrap();
                            if self.use_colors {
                                let mut hasher = DefaultHasher::new();
                                byte.hash(&mut hasher);
                                let hash = hasher.finish();
                                let color = self.color_generator.generate_color(hash);
                                write!(f, "\x1b[38;2;{};{};{}m", color.r, color.g, color.b)?;
                            }
                            // Display the byte in hex format
                            write!(f, "{byte:02x}")?;
                        }
                        if self.use_colors {
                            write!(f, "\x1b[0m")?
                        }
                        if !short {
                            writeln!(f)?;
                            self.indent(f, format_depth)?;
                        }
                        self.write_punctuation(f, "]")?;
                    } else {
                        self.write_punctuation(f, " [")?;
                        let len = list.len();
                        for (idx, item) in list.iter().enumerate() {
                            if !short {
                                writeln!(f)?;
                                self.indent(f, format_depth + 1)?;
                            }
                            self.format_peek_internal_(
                                item,
                                f,
                                visited,
                                format_depth + 1,
                                type_depth + 1,
                                short,
                            )?;

                            if !short || idx + 1 < len {
                                self.write_punctuation(f, ",")?;
                            } else {
                                write!(f, " ")?;
                            }
                        }
                        if !short {
                            writeln!(f)?;
                            self.indent(f, format_depth)?;
                        }
                        self.write_punctuation(f, "]")?;
                    }
                } else {
                    self.write_punctuation(f, "[]")?;
                }
            }

            _ if value.into_set().is_ok() => {
                self.write_type_name(f, &value)?;

                let value = value.into_set().unwrap();
                self.write_punctuation(f, " [")?;
                if !value.is_empty() {
                    let len = value.len();
                    for (idx, item) in value.iter().enumerate() {
                        if !short {
                            writeln!(f)?;
                            self.indent(f, format_depth + 1)?;
                        }
                        self.format_peek_internal_(
                            item,
                            f,
                            visited,
                            format_depth + 1,
                            type_depth + 1,
                            short,
                        )?;
                        if !short || idx + 1 < len {
                            self.write_punctuation(f, ",")?;
                        } else {
                            write!(f, " ")?;
                        }
                    }
                    if !short {
                        writeln!(f)?;
                        self.indent(f, format_depth)?;
                    }
                }
                self.write_punctuation(f, "]")?;
            }

            (Def::Map(def), _) => {
                let key_is_short = Self::shape_chunkiness(def.k) <= 2;

                self.write_type_name(f, &value)?;

                let value = value.into_map().unwrap();
                self.write_punctuation(f, " [")?;

                if !value.is_empty() {
                    let len = value.len();
                    for (idx, (key, value)) in value.iter().enumerate() {
                        if !short {
                            writeln!(f)?;
                            self.indent(f, format_depth + 1)?;
                        }
                        self.format_peek_internal_(
                            key,
                            f,
                            visited,
                            format_depth + 1,
                            type_depth + 1,
                            key_is_short,
                        )?;
                        self.write_punctuation(f, " => ")?;
                        self.format_peek_internal_(
                            value,
                            f,
                            visited,
                            format_depth + 1,
                            type_depth + 1,
                            short,
                        )?;
                        if !short || idx + 1 < len {
                            self.write_punctuation(f, ",")?;
                        } else {
                            write!(f, " ")?;
                        }
                    }
                    if !short {
                        writeln!(f)?;
                        self.indent(f, format_depth)?;
                    }
                }

                self.write_punctuation(f, "]")?;
            }

            (Def::DynamicValue(_), _) => {
                let dyn_val = value.into_dynamic_value().unwrap();
                match dyn_val.kind() {
                    DynValueKind::Null => {
                        self.write_keyword(f, "null")?;
                    }
                    DynValueKind::Bool => {
                        if let Some(b) = dyn_val.as_bool() {
                            self.write_keyword(f, if b { "true" } else { "false" })?;
                        }
                    }
                    DynValueKind::Number => {
                        if let Some(n) = dyn_val.as_i64() {
                            self.format_number(f, &n.to_string())?;
                        } else if let Some(n) = dyn_val.as_u64() {
                            self.format_number(f, &n.to_string())?;
                        } else if let Some(n) = dyn_val.as_f64() {
                            self.format_number(f, &n.to_string())?;
                        }
                    }
                    DynValueKind::String => {
                        if let Some(s) = dyn_val.as_str() {
                            self.format_string(f, s)?;
                        }
                    }
                    DynValueKind::Bytes => {
                        if let Some(bytes) = dyn_val.as_bytes() {
                            self.format_bytes(f, bytes)?;
                        }
                    }
                    DynValueKind::Array => {
                        let len = dyn_val.array_len().unwrap_or(0);
                        if len == 0 {
                            self.write_punctuation(f, "[]")?;
                        } else {
                            self.write_punctuation(f, "[")?;
                            for idx in 0..len {
                                if !short {
                                    writeln!(f)?;
                                    self.indent(f, format_depth + 1)?;
                                }
                                if let Some(elem) = dyn_val.array_get(idx) {
                                    self.format_peek_internal_(
                                        elem,
                                        f,
                                        visited,
                                        format_depth + 1,
                                        type_depth + 1,
                                        short,
                                    )?;
                                }
                                if !short || idx + 1 < len {
                                    self.write_punctuation(f, ",")?;
                                } else {
                                    write!(f, " ")?;
                                }
                            }
                            if !short {
                                writeln!(f)?;
                                self.indent(f, format_depth)?;
                            }
                            self.write_punctuation(f, "]")?;
                        }
                    }
                    DynValueKind::Object => {
                        let len = dyn_val.object_len().unwrap_or(0);
                        if len == 0 {
                            self.write_punctuation(f, "{}")?;
                        } else {
                            self.write_punctuation(f, "{")?;
                            for idx in 0..len {
                                if !short {
                                    writeln!(f)?;
                                    self.indent(f, format_depth + 1)?;
                                }
                                if let Some((key, val)) = dyn_val.object_get_entry(idx) {
                                    self.write_field_name(f, key)?;
                                    self.write_punctuation(f, ": ")?;
                                    self.format_peek_internal_(
                                        val,
                                        f,
                                        visited,
                                        format_depth + 1,
                                        type_depth + 1,
                                        short,
                                    )?;
                                }
                                if !short || idx + 1 < len {
                                    self.write_punctuation(f, ",")?;
                                } else {
                                    write!(f, " ")?;
                                }
                            }
                            if !short {
                                writeln!(f)?;
                                self.indent(f, format_depth)?;
                            }
                            self.write_punctuation(f, "}")?;
                        }
                    }
                    DynValueKind::DateTime => {
                        // Format datetime using the vtable's get_datetime
                        #[allow(clippy::uninlined_format_args)]
                        if let Some((year, month, day, hour, minute, second, nanos, kind)) =
                            dyn_val.as_datetime()
                        {
                            match kind {
                                DynDateTimeKind::Offset { offset_minutes } => {
                                    if nanos > 0 {
                                        write!(
                                            f,
                                            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:09}",
                                            year, month, day, hour, minute, second, nanos
                                        )?;
                                    } else {
                                        write!(
                                            f,
                                            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
                                            year, month, day, hour, minute, second
                                        )?;
                                    }
                                    if offset_minutes == 0 {
                                        write!(f, "Z")?;
                                    } else {
                                        let sign = if offset_minutes >= 0 { '+' } else { '-' };
                                        let abs = offset_minutes.abs();
                                        write!(f, "{}{:02}:{:02}", sign, abs / 60, abs % 60)?;
                                    }
                                }
                                DynDateTimeKind::LocalDateTime => {
                                    if nanos > 0 {
                                        write!(
                                            f,
                                            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:09}",
                                            year, month, day, hour, minute, second, nanos
                                        )?;
                                    } else {
                                        write!(
                                            f,
                                            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
                                            year, month, day, hour, minute, second
                                        )?;
                                    }
                                }
                                DynDateTimeKind::LocalDate => {
                                    write!(f, "{:04}-{:02}-{:02}", year, month, day)?;
                                }
                                DynDateTimeKind::LocalTime => {
                                    if nanos > 0 {
                                        write!(
                                            f,
                                            "{:02}:{:02}:{:02}.{:09}",
                                            hour, minute, second, nanos
                                        )?;
                                    } else {
                                        write!(f, "{:02}:{:02}:{:02}", hour, minute, second)?;
                                    }
                                }
                            }
                        }
                    }
                }
            }

            _ => write!(f, "unsupported peek variant: {value:?}")?,
        }

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn format_tuple_fields<'mem, 'facet>(
        &self,
        peek_field: &dyn Fn(usize) -> Peek<'mem, 'facet>,
        f: &mut dyn Write,
        visited: &mut BTreeMap<ValueId, usize>,
        format_depth: usize,
        type_depth: usize,
        fields: &[Field],
        short: bool,
        force_trailing_comma: bool,
    ) -> fmt::Result {
        self.write_punctuation(f, "(")?;
        if let [field] = fields
            && field.doc.is_empty()
        {
            let field = peek_field(0);
            self.format_peek_internal_(field, f, visited, format_depth, type_depth, short)?;

            if force_trailing_comma {
                self.write_punctuation(f, ",")?;
            }
        } else if !fields.is_empty() {
            for idx in 0..fields.len() {
                if !short {
                    writeln!(f)?;
                    self.indent(f, format_depth + 1)?;

                    for &line in fields[idx].doc {
                        self.write_comment(f, &format!("///{line}"))?;
                        writeln!(f)?;
                        self.indent(f, format_depth + 1)?;
                    }
                }

                if fields[idx].flags.contains(FieldFlags::SENSITIVE) {
                    self.write_redacted(f, "[REDACTED]")?;
                } else {
                    self.format_peek_internal_(
                        peek_field(idx),
                        f,
                        visited,
                        format_depth + 1,
                        type_depth + 1,
                        short,
                    )?;
                }

                if !short || idx + 1 < fields.len() {
                    self.write_punctuation(f, ",")?;
                } else {
                    write!(f, " ")?;
                }
            }
            if !short {
                writeln!(f)?;
                self.indent(f, format_depth)?;
            }
        }
        self.write_punctuation(f, ")")?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn format_struct_fields<'mem, 'facet>(
        &self,
        peek_field: &dyn Fn(usize) -> Peek<'mem, 'facet>,
        f: &mut dyn Write,
        visited: &mut BTreeMap<ValueId, usize>,
        format_depth: usize,
        type_depth: usize,
        fields: &[Field],
        short: bool,
    ) -> fmt::Result {
        self.write_punctuation(f, " {")?;
        if !fields.is_empty() {
            for idx in 0..fields.len() {
                if !short {
                    writeln!(f)?;
                    self.indent(f, format_depth + 1)?;
                }

                for &line in fields[idx].doc {
                    self.write_comment(f, &format!("///{line}"))?;
                    writeln!(f)?;
                    self.indent(f, format_depth + 1)?;
                }

                self.write_field_name(f, fields[idx].name)?;
                self.write_punctuation(f, ": ")?;
                if fields[idx].flags.contains(FieldFlags::SENSITIVE) {
                    self.write_redacted(f, "[REDACTED]")?;
                } else {
                    self.format_peek_internal_(
                        peek_field(idx),
                        f,
                        visited,
                        format_depth + 1,
                        type_depth + 1,
                        short,
                    )?;
                }

                if !short || idx + 1 < fields.len() {
                    self.write_punctuation(f, ",")?;
                } else {
                    write!(f, " ")?;
                }
            }
            if !short {
                writeln!(f)?;
                self.indent(f, format_depth)?;
            }
        }
        self.write_punctuation(f, "}")?;
        Ok(())
    }

    fn indent(&self, f: &mut dyn Write, indent: usize) -> fmt::Result {
        if self.indent_size == usize::MAX {
            write!(f, "{:\t<width$}", "", width = indent)
        } else {
            write!(f, "{: <width$}", "", width = indent * self.indent_size)
        }
    }

    /// Internal method to format a Peek value
    pub(crate) fn format_peek_internal(
        &self,
        value: Peek<'_, '_>,
        f: &mut dyn Write,
        visited: &mut BTreeMap<ValueId, usize>,
    ) -> fmt::Result {
        self.format_peek_internal_(value, f, visited, 0, 0, false)
    }

    /// Format a scalar value
    fn format_scalar(&self, value: Peek, f: &mut dyn Write) -> fmt::Result {
        // Generate a color for this shape
        let mut hasher = DefaultHasher::new();
        value.shape().id.hash(&mut hasher);
        let hash = hasher.finish();
        let color = self.color_generator.generate_color(hash);

        // Display the value
        struct DisplayWrapper<'mem, 'facet>(&'mem Peek<'mem, 'facet>);

        impl fmt::Display for DisplayWrapper<'_, '_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                if self.0.shape().is_display() {
                    write!(f, "{}", self.0)?;
                } else if self.0.shape().is_debug() {
                    write!(f, "{:?}", self.0)?;
                } else {
                    write!(f, "{}", self.0.shape())?;
                    write!(f, "(…)")?;
                }
                Ok(())
            }
        }

        // Apply color if needed and display
        if self.use_colors {
            // We need to use direct ANSI codes for RGB colors
            write!(
                f,
                "\x1b[38;2;{};{};{}m{}",
                color.r,
                color.g,
                color.b,
                DisplayWrapper(&value)
            )?;
            write!(f, "\x1b[0m")?;
        } else {
            write!(f, "{}", DisplayWrapper(&value))?;
        }

        Ok(())
    }

    /// Write a keyword (null, true, false) with coloring
    fn write_keyword(&self, f: &mut dyn Write, keyword: &str) -> fmt::Result {
        if self.use_colors {
            write!(f, "\x1b[35m{keyword}\x1b[0m") // magenta for keywords
        } else {
            write!(f, "{keyword}")
        }
    }

    /// Format a number for dynamic values
    fn format_number(&self, f: &mut dyn Write, s: &str) -> fmt::Result {
        if self.use_colors {
            write!(f, "\x1b[36m{s}\x1b[0m") // cyan for numbers
        } else {
            write!(f, "{s}")
        }
    }

    /// Format a string for dynamic values
    fn format_string(&self, f: &mut dyn Write, s: &str) -> fmt::Result {
        if self.use_colors {
            write!(f, "\x1b[33m{s:?}\x1b[0m") // yellow for strings
        } else {
            write!(f, "{s:?}")
        }
    }

    /// Format bytes for dynamic values
    fn format_bytes(&self, f: &mut dyn Write, bytes: &[u8]) -> fmt::Result {
        write!(f, "b\"")?;
        for byte in bytes {
            write!(f, "\\x{byte:02x}")?;
        }
        write!(f, "\"")
    }

    /// Write styled type name to formatter
    fn write_type_name(&self, f: &mut dyn Write, peek: &Peek) -> fmt::Result {
        struct TypeNameWriter<'mem, 'facet>(&'mem Peek<'mem, 'facet>);

        impl core::fmt::Display for TypeNameWriter<'_, '_> {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                self.0.type_name(f, TypeNameOpts::infinite())
            }
        }
        let type_name = TypeNameWriter(peek);

        if self.use_colors {
            write!(f, "\x1b[1m{type_name}\x1b[0m") // bold
        } else {
            write!(f, "{type_name}")
        }
    }

    /// Style a type name and return it as a string
    #[allow(dead_code)]
    fn style_type_name(&self, peek: &Peek) -> String {
        let mut result = String::new();
        self.write_type_name(&mut result, peek).unwrap();
        result
    }

    /// Write styled field name to formatter
    fn write_field_name(&self, f: &mut dyn Write, name: &str) -> fmt::Result {
        if self.use_colors {
            // Use cyan color for field names (approximating original RGB color)
            write!(f, "\x1b[36m{name}\x1b[0m") // cyan
        } else {
            write!(f, "{name}")
        }
    }

    /// Write styled punctuation to formatter
    fn write_punctuation(&self, f: &mut dyn Write, text: &str) -> fmt::Result {
        if self.use_colors {
            write!(f, "\x1b[2m{text}\x1b[0m") // dim
        } else {
            write!(f, "{text}")
        }
    }

    /// Write styled comment to formatter
    fn write_comment(&self, f: &mut dyn Write, text: &str) -> fmt::Result {
        if self.use_colors {
            write!(f, "\x1b[2m{text}\x1b[0m") // dim
        } else {
            write!(f, "{text}")
        }
    }

    /// Write styled redacted value to formatter
    fn write_redacted(&self, f: &mut dyn Write, text: &str) -> fmt::Result {
        if self.use_colors {
            // Use bright red and bold for redacted values
            if self.use_colors {
                write!(f, "\x1b[91;1m{text}\x1b[0m") // bright red + bold
            } else {
                write!(f, "{text}")
            }
        } else {
            write!(f, "{text}")
        }
    }

    /// Style a redacted value and return it as a string
    #[allow(dead_code)]
    fn style_redacted(&self, text: &str) -> String {
        let mut result = String::new();
        self.write_redacted(&mut result, text).unwrap();
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Basic tests for the PrettyPrinter
    #[test]
    fn test_pretty_printer_default() {
        let printer = PrettyPrinter::default();
        assert_eq!(printer.indent_size, 2);
        assert_eq!(printer.max_depth, None);
        assert!(printer.use_colors);
    }

    #[test]
    fn test_pretty_printer_with_methods() {
        let printer = PrettyPrinter::new()
            .with_indent_size(4)
            .with_max_depth(3)
            .with_colors(false);

        assert_eq!(printer.indent_size, 4);
        assert_eq!(printer.max_depth, Some(3));
        assert!(!printer.use_colors);
    }
}
