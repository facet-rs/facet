//! Pretty printer implementation for Facet types

use alloc::borrow::Cow;
use alloc::collections::BTreeMap;
use core::{
    fmt::{self, Write},
    hash::{Hash, Hasher},
    str,
};
use std::{hash::DefaultHasher, sync::LazyLock};

use facet_core::{
    Def, DynDateTimeKind, DynValueKind, Facet, Field, PointerType, PrimitiveType, SequenceType,
    Shape, StructKind, StructType, TextualType, Type, TypeNameOpts, UserType,
};
use facet_reflect::{Peek, ValueId};

use owo_colors::{OwoColorize, Rgb};

use crate::color::ColorGenerator;
use crate::shape::{FieldSpan, Path, PathSegment, Span};

/// Tokyo Night color palette (RGB values from official theme)
///
/// See: <https://github.com/tokyo-night/tokyo-night-vscode-theme>
pub mod tokyo_night {
    use owo_colors::Rgb;

    // ========================================================================
    // Core colors
    // ========================================================================

    /// Foreground - main text (#a9b1d6)
    pub const FOREGROUND: Rgb = Rgb(169, 177, 214);
    /// Background (#1a1b26)
    pub const BACKGROUND: Rgb = Rgb(26, 27, 38);
    /// Comment - muted text (#565f89)
    pub const COMMENT: Rgb = Rgb(86, 95, 137);

    // ========================================================================
    // Terminal ANSI colors
    // ========================================================================

    /// Black (#414868)
    pub const BLACK: Rgb = Rgb(65, 72, 104);
    /// Red (#f7768e)
    pub const RED: Rgb = Rgb(247, 118, 142);
    /// Green - teal/cyan green (#73daca)
    pub const GREEN: Rgb = Rgb(115, 218, 202);
    /// Yellow - warm orange-yellow (#e0af68)
    pub const YELLOW: Rgb = Rgb(224, 175, 104);
    /// Blue (#7aa2f7)
    pub const BLUE: Rgb = Rgb(122, 162, 247);
    /// Magenta - purple (#bb9af7)
    pub const MAGENTA: Rgb = Rgb(187, 154, 247);
    /// Cyan - bright cyan (#7dcfff)
    pub const CYAN: Rgb = Rgb(125, 207, 255);
    /// White - muted white (#787c99)
    pub const WHITE: Rgb = Rgb(120, 124, 153);

    /// Bright white (#acb0d0)
    pub const BRIGHT_WHITE: Rgb = Rgb(172, 176, 208);

    // ========================================================================
    // Extended syntax colors
    // ========================================================================

    /// Orange - numbers, constants (#ff9e64)
    pub const ORANGE: Rgb = Rgb(255, 158, 100);
    /// Dark green - strings (#9ece6a)
    pub const DARK_GREEN: Rgb = Rgb(158, 206, 106);

    // ========================================================================
    // Semantic/status colors
    // ========================================================================

    /// Error - bright red for errors (#db4b4b)
    pub const ERROR: Rgb = Rgb(219, 75, 75);
    /// Warning - same as yellow (#e0af68)
    pub const WARNING: Rgb = YELLOW;
    /// Info - teal-blue (#0db9d7)
    pub const INFO: Rgb = Rgb(13, 185, 215);
    /// Hint - same as comment, muted
    pub const HINT: Rgb = COMMENT;

    // ========================================================================
    // Semantic aliases for specific uses
    // ========================================================================

    /// Type names - blue, bold
    pub const TYPE_NAME: Rgb = BLUE;
    /// Field names - green/teal
    pub const FIELD_NAME: Rgb = GREEN;
    /// String literals - dark green
    pub const STRING: Rgb = DARK_GREEN;
    /// Number literals - orange
    pub const NUMBER: Rgb = ORANGE;
    /// Keywords (null, true, false) - magenta
    pub const KEYWORD: Rgb = MAGENTA;
    /// Deletions in diffs - red
    pub const DELETION: Rgb = RED;
    /// Insertions in diffs - green
    pub const INSERTION: Rgb = GREEN;
    /// Muted/unchanged - comment color
    pub const MUTED: Rgb = COMMENT;
    /// Borders - very muted, comment color
    pub const BORDER: Rgb = COMMENT;
}

/// A formatter for pretty-printing Facet types
#[derive(Clone, PartialEq)]
pub struct PrettyPrinter {
    /// usize::MAX is a special value that means indenting with tabs instead of spaces
    indent_size: usize,
    max_depth: Option<usize>,
    color_generator: ColorGenerator,
    colors: ColorMode,
    list_u8_as_bytes: bool,
    /// Skip type names for Options (show `Some(x)` instead of `Option<T>::Some(x)`)
    minimal_option_names: bool,
    /// Whether to show doc comments in output
    show_doc_comments: bool,
    /// Maximum length for strings/bytes before truncating the middle (None = no limit)
    max_content_len: Option<usize>,
}

impl Default for PrettyPrinter {
    fn default() -> Self {
        Self::new()
    }
}

impl PrettyPrinter {
    /// Create a new PrettyPrinter with default settings
    pub const fn new() -> Self {
        Self {
            indent_size: 2,
            max_depth: None,
            color_generator: ColorGenerator::new(),
            colors: ColorMode::Auto,
            list_u8_as_bytes: true,
            minimal_option_names: false,
            show_doc_comments: false,
            max_content_len: None,
        }
    }

    /// Set the indentation size
    pub const fn with_indent_size(mut self, size: usize) -> Self {
        self.indent_size = size;
        self
    }

    /// Set the maximum depth for recursive printing
    pub const fn with_max_depth(mut self, depth: usize) -> Self {
        self.max_depth = Some(depth);
        self
    }

    /// Set the color generator
    pub const fn with_color_generator(mut self, generator: ColorGenerator) -> Self {
        self.color_generator = generator;
        self
    }

    /// Enable or disable colors. Use `None` to automatically detect color support based on the `NO_COLOR` environment variable.
    pub const fn with_colors(mut self, enable_colors: ColorMode) -> Self {
        self.colors = enable_colors;
        self
    }

    /// Use minimal names for Options (show `Some(x)` instead of `Option<T>::Some(x)`)
    pub const fn with_minimal_option_names(mut self, minimal: bool) -> Self {
        self.minimal_option_names = minimal;
        self
    }

    /// Enable or disable doc comments in output
    pub const fn with_doc_comments(mut self, show: bool) -> Self {
        self.show_doc_comments = show;
        self
    }

    /// Set the maximum length for strings and byte arrays before truncating
    ///
    /// When set, strings and byte arrays longer than this limit will be
    /// truncated in the middle, showing the beginning and end with `...` between.
    pub const fn with_max_content_len(mut self, max_len: usize) -> Self {
        self.max_content_len = Some(max_len);
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
                        sum = sum.saturating_add(Self::shape_chunkiness(field.shape()));
                    }
                    sum
                }
                UserType::Enum(ty) => {
                    let mut max = 0usize;
                    for variant in ty.variants {
                        max = Ord::max(max, {
                            let mut sum = 0usize;
                            for field in variant.data.fields {
                                sum = sum.saturating_add(Self::shape_chunkiness(field.shape()));
                            }
                            sum
                        })
                    }
                    max
                }
                UserType::Opaque | UserType::Union(_) => 1,
            },
            Type::Undefined => 1,
        }
    }

    #[inline]
    fn use_colors(&self) -> bool {
        self.colors.enabled()
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

        // Unwrap transparent wrappers (e.g., newtype wrappers like IntAsString(String))
        // This matches serialization behavior where we serialize the inner value directly
        let value = value.innermost_peek();
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

        // Handle proxy types by converting to the proxy representation and formatting that
        if let Some(proxy_def) = shape.proxy {
            let result = self.format_via_proxy(
                value,
                proxy_def,
                f,
                visited,
                format_depth,
                type_depth,
                short,
            );

            visited.remove(&value.id());
            return result;
        }

        match (shape.def, shape.ty) {
            (_, Type::Primitive(PrimitiveType::Textual(TextualType::Str))) => {
                let value = value.get::<str>().unwrap();
                self.format_str_value(f, value)?;
            }
            // Handle String specially to add quotes (like &str)
            (Def::Scalar, _) if value.shape().id == <alloc::string::String as Facet>::SHAPE.id => {
                let s = value.get::<alloc::string::String>().unwrap();
                self.format_str_value(f, s)?;
            }
            (Def::Scalar, _) => self.format_scalar(value, f)?,
            (Def::Option(_), _) => {
                let option = value.into_option().unwrap();

                // Print the Option name (unless minimal mode)
                if !self.minimal_option_names {
                    self.write_type_name(f, &value)?;
                }

                if let Some(inner) = option.value() {
                    let prefix = if self.minimal_option_names {
                        "Some("
                    } else {
                        "::Some("
                    };
                    self.write_punctuation(f, prefix)?;
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
                    let suffix = if self.minimal_option_names {
                        "None"
                    } else {
                        "::None"
                    };
                    self.write_punctuation(f, suffix)?;
                }
            }

            (_, Type::Pointer(PointerType::Raw(_) | PointerType::Function(_))) => {
                self.write_type_name(f, &value)?;
                let addr = unsafe { value.data().read::<*const ()>() };
                let value = Peek::new(&addr);
                self.format_scalar(value, f)?;
            }

            (_, Type::User(UserType::Union(_))) => {
                if !short && self.show_doc_comments {
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
                if !short && self.show_doc_comments {
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
                if !short && self.show_doc_comments {
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
                        if !short && self.show_doc_comments {
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
                        if self.use_colors() {
                            write!(f, "{}", variant.name.bold())?;
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
                        let total_len = list.len();
                        let truncate = self.max_content_len.is_some_and(|max| total_len > max);

                        self.write_punctuation(f, " [")?;

                        if truncate {
                            let max = self.max_content_len.unwrap();
                            let half = max / 2;
                            let start_count = half;
                            let end_count = half;

                            // Show beginning
                            for (idx, item) in list.iter().enumerate().take(start_count) {
                                if !short && idx % 16 == 0 {
                                    writeln!(f)?;
                                    self.indent(f, format_depth + 1)?;
                                }
                                write!(f, " ")?;
                                let byte = *item.get::<u8>().unwrap();
                                if self.use_colors() {
                                    let mut hasher = DefaultHasher::new();
                                    byte.hash(&mut hasher);
                                    let hash = hasher.finish();
                                    let color = self.color_generator.generate_color(hash);
                                    let rgb = Rgb(color.r, color.g, color.b);
                                    write!(f, "{}", format!("{byte:02x}").color(rgb))?;
                                } else {
                                    write!(f, "{byte:02x}")?;
                                }
                            }

                            // Show ellipsis
                            let omitted = total_len - start_count - end_count;
                            if !short {
                                writeln!(f)?;
                                self.indent(f, format_depth + 1)?;
                            }
                            write!(f, " ...({omitted} bytes)...")?;

                            // Show end
                            for (idx, item) in list.iter().enumerate().skip(total_len - end_count) {
                                let display_idx = start_count + 1 + (idx - (total_len - end_count));
                                if !short && display_idx.is_multiple_of(16) {
                                    writeln!(f)?;
                                    self.indent(f, format_depth + 1)?;
                                }
                                write!(f, " ")?;
                                let byte = *item.get::<u8>().unwrap();
                                if self.use_colors() {
                                    let mut hasher = DefaultHasher::new();
                                    byte.hash(&mut hasher);
                                    let hash = hasher.finish();
                                    let color = self.color_generator.generate_color(hash);
                                    let rgb = Rgb(color.r, color.g, color.b);
                                    write!(f, "{}", format!("{byte:02x}").color(rgb))?;
                                } else {
                                    write!(f, "{byte:02x}")?;
                                }
                            }
                        } else {
                            for (idx, item) in list.iter().enumerate() {
                                if !short && idx % 16 == 0 {
                                    writeln!(f)?;
                                    self.indent(f, format_depth + 1)?;
                                }
                                write!(f, " ")?;

                                let byte = *item.get::<u8>().unwrap();
                                if self.use_colors() {
                                    let mut hasher = DefaultHasher::new();
                                    byte.hash(&mut hasher);
                                    let hash = hasher.finish();
                                    let color = self.color_generator.generate_color(hash);
                                    let rgb = Rgb(color.r, color.g, color.b);
                                    write!(f, "{}", format!("{byte:02x}").color(rgb))?;
                                } else {
                                    write!(f, "{byte:02x}")?;
                                }
                            }
                        }

                        if !short {
                            writeln!(f)?;
                            self.indent(f, format_depth)?;
                        }
                        self.write_punctuation(f, "]")?;
                    } else {
                        // Check if elements are simple scalars - render inline if so
                        let elem_shape = list.def().t();
                        let is_simple = Self::shape_chunkiness(elem_shape) <= 1;

                        self.write_punctuation(f, " [")?;
                        let len = list.len();
                        for (idx, item) in list.iter().enumerate() {
                            if !short && !is_simple {
                                writeln!(f)?;
                                self.indent(f, format_depth + 1)?;
                            } else if idx > 0 {
                                write!(f, " ")?;
                            }
                            self.format_peek_internal_(
                                item,
                                f,
                                visited,
                                format_depth + 1,
                                type_depth + 1,
                                short || is_simple,
                            )?;

                            if (!short && !is_simple) || idx + 1 < len {
                                self.write_punctuation(f, ",")?;
                            }
                        }
                        if !short && !is_simple {
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
                    DynValueKind::QName => {
                        // QName formatting is not yet supported via vtable
                        write!(f, "<qname>")?;
                    }
                    DynValueKind::Uuid => {
                        // UUID formatting is not yet supported via vtable
                        write!(f, "<uuid>")?;
                    }
                }
            }

            (d, t) => write!(f, "unsupported peek variant: {value:?} ({d:?}, {t:?})")?,
        }

        visited.remove(&value.id());
        Ok(())
    }

    /// Format a value through its proxy type representation.
    ///
    /// This allocates memory for the proxy type, converts the value to its proxy
    /// representation, formats the proxy, then cleans up.
    #[allow(clippy::too_many_arguments)]
    fn format_via_proxy(
        &self,
        value: Peek<'_, '_>,
        proxy_def: &'static facet_core::ProxyDef,
        f: &mut dyn Write,
        visited: &mut BTreeMap<ValueId, usize>,
        format_depth: usize,
        type_depth: usize,
        short: bool,
    ) -> fmt::Result {
        let proxy_shape = proxy_def.shape;
        let proxy_layout = match proxy_shape.layout.sized_layout() {
            Ok(layout) => layout,
            Err(_) => {
                return write!(f, "/* proxy type must be sized for formatting */");
            }
        };

        // Allocate memory for the proxy value
        let proxy_uninit = facet_core::alloc_for_layout(proxy_layout);

        // Convert target → proxy
        let convert_result = unsafe { (proxy_def.convert_out)(value.data(), proxy_uninit) };

        let proxy_ptr = match convert_result {
            Ok(ptr) => ptr,
            Err(msg) => {
                unsafe { facet_core::dealloc_for_layout(proxy_uninit.assume_init(), proxy_layout) };
                return write!(f, "/* proxy conversion failed: {msg} */");
            }
        };

        // Create a Peek to the proxy value and format it
        let proxy_peek = unsafe { Peek::unchecked_new(proxy_ptr.as_const(), proxy_shape) };
        let result =
            self.format_peek_internal_(proxy_peek, f, visited, format_depth, type_depth, short);

        // Clean up: drop the proxy value and deallocate
        unsafe {
            let _ = proxy_shape.call_drop_in_place(proxy_ptr);
            facet_core::dealloc_for_layout(proxy_ptr, proxy_layout);
        }

        result
    }

    /// Format a value through its proxy type representation (unified version for FormatOutput).
    ///
    /// This allocates memory for the proxy type, converts the value to its proxy
    /// representation, formats the proxy, then cleans up.
    #[allow(clippy::too_many_arguments)]
    fn format_via_proxy_unified<O: FormatOutput>(
        &self,
        value: Peek<'_, '_>,
        proxy_def: &'static facet_core::ProxyDef,
        out: &mut O,
        visited: &mut BTreeMap<ValueId, usize>,
        format_depth: usize,
        type_depth: usize,
        short: bool,
        current_path: Path,
    ) -> fmt::Result {
        let proxy_shape = proxy_def.shape;
        let proxy_layout = match proxy_shape.layout.sized_layout() {
            Ok(layout) => layout,
            Err(_) => {
                return write!(out, "/* proxy type must be sized for formatting */");
            }
        };

        // Allocate memory for the proxy value
        let proxy_uninit = facet_core::alloc_for_layout(proxy_layout);

        // Convert target → proxy
        let convert_result = unsafe { (proxy_def.convert_out)(value.data(), proxy_uninit) };

        let proxy_ptr = match convert_result {
            Ok(ptr) => ptr,
            Err(msg) => {
                unsafe { facet_core::dealloc_for_layout(proxy_uninit.assume_init(), proxy_layout) };
                return write!(out, "/* proxy conversion failed: {msg} */");
            }
        };

        // Create a Peek to the proxy value and format it
        let proxy_peek = unsafe { Peek::unchecked_new(proxy_ptr.as_const(), proxy_shape) };
        let result = self.format_unified(
            proxy_peek,
            out,
            visited,
            format_depth,
            type_depth,
            short,
            current_path,
        );

        // Clean up: drop the proxy value and deallocate
        unsafe {
            let _ = proxy_shape.call_drop_in_place(proxy_ptr);
            facet_core::dealloc_for_layout(proxy_ptr, proxy_layout);
        }

        result
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
            let field_value = peek_field(0);
            if let Some(proxy_def) = field.proxy() {
                self.format_via_proxy(
                    field_value,
                    proxy_def,
                    f,
                    visited,
                    format_depth,
                    type_depth,
                    short,
                )?;
            } else {
                self.format_peek_internal_(
                    field_value,
                    f,
                    visited,
                    format_depth,
                    type_depth,
                    short,
                )?;
            }

            if force_trailing_comma {
                self.write_punctuation(f, ",")?;
            }
        } else if !fields.is_empty() {
            for idx in 0..fields.len() {
                if !short {
                    writeln!(f)?;
                    self.indent(f, format_depth + 1)?;

                    if self.show_doc_comments {
                        for &line in fields[idx].doc {
                            self.write_comment(f, &format!("///{line}"))?;
                            writeln!(f)?;
                            self.indent(f, format_depth + 1)?;
                        }
                    }
                }

                if fields[idx].is_sensitive() {
                    self.write_redacted(f, "[REDACTED]")?;
                } else if let Some(proxy_def) = fields[idx].proxy() {
                    // Field-level proxy: format through the proxy type
                    self.format_via_proxy(
                        peek_field(idx),
                        proxy_def,
                        f,
                        visited,
                        format_depth + 1,
                        type_depth + 1,
                        short,
                    )?;
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
        // First, determine which fields will be printed (not skipped)
        let visible_indices: Vec<usize> = (0..fields.len())
            .filter(|&idx| {
                let field = &fields[idx];
                // SAFETY: peek_field returns a valid Peek with valid data pointer
                let field_ptr = peek_field(idx).data();
                !unsafe { field.should_skip_serializing(field_ptr) }
            })
            .collect();

        self.write_punctuation(f, " {")?;
        if !visible_indices.is_empty() {
            for (i, &idx) in visible_indices.iter().enumerate() {
                let is_last = i + 1 == visible_indices.len();

                if !short {
                    writeln!(f)?;
                    self.indent(f, format_depth + 1)?;
                }

                if self.show_doc_comments {
                    for &line in fields[idx].doc {
                        self.write_comment(f, &format!("///{line}"))?;
                        writeln!(f)?;
                        self.indent(f, format_depth + 1)?;
                    }
                }

                self.write_field_name(f, fields[idx].name)?;
                self.write_punctuation(f, ": ")?;
                if fields[idx].is_sensitive() {
                    self.write_redacted(f, "[REDACTED]")?;
                } else if let Some(proxy_def) = fields[idx].proxy() {
                    // Field-level proxy: format through the proxy type
                    self.format_via_proxy(
                        peek_field(idx),
                        proxy_def,
                        f,
                        visited,
                        format_depth + 1,
                        type_depth + 1,
                        short,
                    )?;
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

                if !short || !is_last {
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
        if self.use_colors() {
            let rgb = Rgb(color.r, color.g, color.b);
            write!(f, "{}", DisplayWrapper(&value).color(rgb))?;
        } else {
            write!(f, "{}", DisplayWrapper(&value))?;
        }

        Ok(())
    }

    /// Write a keyword (null, true, false) with coloring
    fn write_keyword(&self, f: &mut dyn Write, keyword: &str) -> fmt::Result {
        if self.use_colors() {
            write!(f, "{}", keyword.color(tokyo_night::KEYWORD))
        } else {
            write!(f, "{keyword}")
        }
    }

    /// Format a number for dynamic values
    fn format_number(&self, f: &mut dyn Write, s: &str) -> fmt::Result {
        if self.use_colors() {
            write!(f, "{}", s.color(tokyo_night::NUMBER))
        } else {
            write!(f, "{s}")
        }
    }

    /// Format a &str or String value with optional truncation and raw string handling
    fn format_str_value(&self, f: &mut dyn Write, value: &str) -> fmt::Result {
        // Check if truncation is needed
        if let Some(max) = self.max_content_len
            && value.len() > max
        {
            return self.format_truncated_str(f, value, max);
        }

        // Normal formatting with raw string handling for quotes
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
        if self.use_colors() {
            write!(f, "{}", value.color(tokyo_night::STRING))?;
        } else {
            write!(f, "{value}")?;
        }
        write!(f, "\"")?;
        if hashes > 0 {
            write!(f, "{pad:#<width$}")?;
        }
        Ok(())
    }

    /// Format a truncated string showing beginning...end
    fn format_truncated_str(&self, f: &mut dyn Write, s: &str, max: usize) -> fmt::Result {
        let half = max / 2;

        // Find char boundary for start portion
        let start_end = s
            .char_indices()
            .take_while(|(i, _)| *i < half)
            .last()
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(0);

        // Find char boundary for end portion
        let end_start = s
            .char_indices()
            .rev()
            .take_while(|(i, _)| s.len() - *i <= half)
            .last()
            .map(|(i, _)| i)
            .unwrap_or(s.len());

        let omitted = s[start_end..end_start].chars().count();
        let start_part = &s[..start_end];
        let end_part = &s[end_start..];

        if self.use_colors() {
            write!(
                f,
                "\"{}\"...({omitted} chars)...\"{}\"",
                start_part.color(tokyo_night::STRING),
                end_part.color(tokyo_night::STRING)
            )
        } else {
            write!(f, "\"{start_part}\"...({omitted} chars)...\"{end_part}\"")
        }
    }

    /// Format a string for dynamic values (uses debug escaping for special chars)
    fn format_string(&self, f: &mut dyn Write, s: &str) -> fmt::Result {
        if let Some(max) = self.max_content_len
            && s.len() > max
        {
            return self.format_truncated_str(f, s, max);
        }

        if self.use_colors() {
            write!(f, "\"{}\"", s.color(tokyo_night::STRING))
        } else {
            write!(f, "{s:?}")
        }
    }

    /// Format bytes for dynamic values
    fn format_bytes(&self, f: &mut dyn Write, bytes: &[u8]) -> fmt::Result {
        write!(f, "b\"")?;

        match self.max_content_len {
            Some(max) if bytes.len() > max => {
                // Show beginning ... end
                let half = max / 2;
                let start = half;
                let end = half;

                for byte in &bytes[..start] {
                    write!(f, "\\x{byte:02x}")?;
                }
                let omitted = bytes.len() - start - end;
                write!(f, "\"...({omitted} bytes)...b\"")?;
                for byte in &bytes[bytes.len() - end..] {
                    write!(f, "\\x{byte:02x}")?;
                }
            }
            _ => {
                for byte in bytes {
                    write!(f, "\\x{byte:02x}")?;
                }
            }
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

        if self.use_colors() {
            write!(f, "{}", type_name.color(tokyo_night::TYPE_NAME).bold())
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
        if self.use_colors() {
            write!(f, "{}", name.color(tokyo_night::FIELD_NAME))
        } else {
            write!(f, "{name}")
        }
    }

    /// Write styled punctuation to formatter
    fn write_punctuation(&self, f: &mut dyn Write, text: &str) -> fmt::Result {
        if self.use_colors() {
            write!(f, "{}", text.dimmed())
        } else {
            write!(f, "{text}")
        }
    }

    /// Write styled comment to formatter
    fn write_comment(&self, f: &mut dyn Write, text: &str) -> fmt::Result {
        if self.use_colors() {
            write!(f, "{}", text.color(tokyo_night::MUTED))
        } else {
            write!(f, "{text}")
        }
    }

    /// Write styled redacted value to formatter
    fn write_redacted(&self, f: &mut dyn Write, text: &str) -> fmt::Result {
        if self.use_colors() {
            write!(f, "{}", text.color(tokyo_night::ERROR).bold())
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

    /// Format a value with span tracking for each path.
    ///
    /// Returns a `FormattedValue` containing the plain text output and a map
    /// from paths to their byte spans in the output.
    ///
    /// This is useful for creating rich diagnostics that can highlight specific
    /// parts of a pretty-printed value.
    pub fn format_peek_with_spans(&self, value: Peek<'_, '_>) -> FormattedValue {
        let mut output = SpanTrackingOutput::new();
        let printer = Self {
            colors: ColorMode::Never, // Always disable colors for span tracking
            indent_size: self.indent_size,
            max_depth: self.max_depth,
            color_generator: self.color_generator.clone(),
            list_u8_as_bytes: self.list_u8_as_bytes,
            minimal_option_names: self.minimal_option_names,
            show_doc_comments: self.show_doc_comments,
            max_content_len: self.max_content_len,
        };
        printer
            .format_unified(
                value,
                &mut output,
                &mut BTreeMap::new(),
                0,
                0,
                false,
                vec![],
            )
            .expect("Formatting failed");

        output.into_formatted_value()
    }

    /// Unified formatting implementation that works with any FormatOutput.
    ///
    /// This is the core implementation - both `format_peek` and `format_peek_with_spans`
    /// use this internally with different output types.
    #[allow(clippy::too_many_arguments)]
    fn format_unified<O: FormatOutput>(
        &self,
        value: Peek<'_, '_>,
        out: &mut O,
        visited: &mut BTreeMap<ValueId, usize>,
        format_depth: usize,
        type_depth: usize,
        short: bool,
        current_path: Path,
    ) -> fmt::Result {
        let mut value = value;
        while let Ok(ptr) = value.into_pointer()
            && let Some(pointee) = ptr.borrow_inner()
        {
            value = pointee;
        }

        // Unwrap transparent wrappers (e.g., newtype wrappers like IntAsString(String))
        // This matches serialization behavior where we serialize the inner value directly
        let value = value.innermost_peek();
        let shape = value.shape();

        // Record the start of this value
        let value_start = out.position();

        if let Some(prev_type_depth) = visited.insert(value.id(), type_depth) {
            write!(out, "{} {{ ", shape)?;
            write!(
                out,
                "/* cycle detected at {} (first seen at type_depth {}) */",
                value.id(),
                prev_type_depth,
            )?;
            visited.remove(&value.id());
            let value_end = out.position();
            out.record_span(current_path, (value_start, value_end));
            return Ok(());
        }

        // Handle proxy types by converting to the proxy representation and formatting that
        if let Some(proxy_def) = shape.proxy {
            let result = self.format_via_proxy_unified(
                value,
                proxy_def,
                out,
                visited,
                format_depth,
                type_depth,
                short,
                current_path.clone(),
            );

            visited.remove(&value.id());

            // Record span for this value
            let value_end = out.position();
            out.record_span(current_path, (value_start, value_end));

            return result;
        }

        match (shape.def, shape.ty) {
            (_, Type::Primitive(PrimitiveType::Textual(TextualType::Str))) => {
                let s = value.get::<str>().unwrap();
                write!(out, "\"{}\"", s)?;
            }
            (Def::Scalar, _) if value.shape().id == <alloc::string::String as Facet>::SHAPE.id => {
                let s = value.get::<alloc::string::String>().unwrap();
                write!(out, "\"{}\"", s)?;
            }
            (Def::Scalar, _) => {
                self.format_scalar_to_output(value, out)?;
            }
            (Def::Option(_), _) => {
                let option = value.into_option().unwrap();
                if let Some(inner) = option.value() {
                    write!(out, "Some(")?;
                    self.format_unified(
                        inner,
                        out,
                        visited,
                        format_depth,
                        type_depth + 1,
                        short,
                        current_path.clone(),
                    )?;
                    write!(out, ")")?;
                } else {
                    write!(out, "None")?;
                }
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
                write!(out, "{}", shape)?;
                if matches!(ty.kind, StructKind::Struct) {
                    let struct_peek = value.into_struct().unwrap();
                    write!(out, " {{")?;
                    for (i, field) in ty.fields.iter().enumerate() {
                        if !short {
                            writeln!(out)?;
                            self.indent_to_output(out, format_depth + 1)?;
                        }
                        // Record field name span
                        let field_name_start = out.position();
                        write!(out, "{}", field.name)?;
                        let field_name_end = out.position();
                        write!(out, ": ")?;

                        // Build path for this field
                        let mut field_path = current_path.clone();
                        field_path.push(PathSegment::Field(Cow::Borrowed(field.name)));

                        // Record field value span
                        let field_value_start = out.position();
                        if let Ok(field_value) = struct_peek.field(i) {
                            // Check for field-level proxy
                            if let Some(proxy_def) = field.proxy() {
                                self.format_via_proxy_unified(
                                    field_value,
                                    proxy_def,
                                    out,
                                    visited,
                                    format_depth + 1,
                                    type_depth + 1,
                                    short,
                                    field_path.clone(),
                                )?;
                            } else {
                                self.format_unified(
                                    field_value,
                                    out,
                                    visited,
                                    format_depth + 1,
                                    type_depth + 1,
                                    short,
                                    field_path.clone(),
                                )?;
                            }
                        }
                        let field_value_end = out.position();

                        // Record span for this field
                        out.record_field_span(
                            field_path,
                            (field_name_start, field_name_end),
                            (field_value_start, field_value_end),
                        );

                        if !short || i + 1 < ty.fields.len() {
                            write!(out, ",")?;
                        }
                    }
                    if !short {
                        writeln!(out)?;
                        self.indent_to_output(out, format_depth)?;
                    }
                    write!(out, "}}")?;
                }
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
                write!(out, "{}", shape)?;
                if matches!(ty.kind, StructKind::Tuple) {
                    write!(out, " ")?;
                }
                let struct_peek = value.into_struct().unwrap();
                write!(out, "(")?;
                for (i, field) in ty.fields.iter().enumerate() {
                    if i > 0 {
                        write!(out, ", ")?;
                    }
                    let mut elem_path = current_path.clone();
                    elem_path.push(PathSegment::Index(i));

                    let elem_start = out.position();
                    if let Ok(field_value) = struct_peek.field(i) {
                        // Check for field-level proxy
                        if let Some(proxy_def) = field.proxy() {
                            self.format_via_proxy_unified(
                                field_value,
                                proxy_def,
                                out,
                                visited,
                                format_depth + 1,
                                type_depth + 1,
                                short,
                                elem_path.clone(),
                            )?;
                        } else {
                            self.format_unified(
                                field_value,
                                out,
                                visited,
                                format_depth + 1,
                                type_depth + 1,
                                short,
                                elem_path.clone(),
                            )?;
                        }
                    }
                    let elem_end = out.position();
                    out.record_span(elem_path, (elem_start, elem_end));
                }
                write!(out, ")")?;
            }
            (_, Type::User(UserType::Enum(_))) => {
                let enum_peek = value.into_enum().unwrap();
                match enum_peek.active_variant() {
                    Err(_) => {
                        write!(out, "{} {{ /* cannot determine variant */ }}", shape)?;
                    }
                    Ok(variant) => {
                        write!(out, "{}::{}", shape, variant.name)?;

                        match variant.data.kind {
                            StructKind::Unit => {}
                            StructKind::Struct => {
                                write!(out, " {{")?;
                                for (i, field) in variant.data.fields.iter().enumerate() {
                                    if !short {
                                        writeln!(out)?;
                                        self.indent_to_output(out, format_depth + 1)?;
                                    }
                                    let field_name_start = out.position();
                                    write!(out, "{}", field.name)?;
                                    let field_name_end = out.position();
                                    write!(out, ": ")?;

                                    let mut field_path = current_path.clone();
                                    field_path
                                        .push(PathSegment::Variant(Cow::Borrowed(variant.name)));
                                    field_path.push(PathSegment::Field(Cow::Borrowed(field.name)));

                                    let field_value_start = out.position();
                                    if let Ok(Some(field_value)) = enum_peek.field(i) {
                                        // Check for field-level proxy
                                        if let Some(proxy_def) = field.proxy() {
                                            self.format_via_proxy_unified(
                                                field_value,
                                                proxy_def,
                                                out,
                                                visited,
                                                format_depth + 1,
                                                type_depth + 1,
                                                short,
                                                field_path.clone(),
                                            )?;
                                        } else {
                                            self.format_unified(
                                                field_value,
                                                out,
                                                visited,
                                                format_depth + 1,
                                                type_depth + 1,
                                                short,
                                                field_path.clone(),
                                            )?;
                                        }
                                    }
                                    let field_value_end = out.position();

                                    out.record_field_span(
                                        field_path,
                                        (field_name_start, field_name_end),
                                        (field_value_start, field_value_end),
                                    );

                                    if !short || i + 1 < variant.data.fields.len() {
                                        write!(out, ",")?;
                                    }
                                }
                                if !short {
                                    writeln!(out)?;
                                    self.indent_to_output(out, format_depth)?;
                                }
                                write!(out, "}}")?;
                            }
                            _ => {
                                write!(out, "(")?;
                                for (i, field) in variant.data.fields.iter().enumerate() {
                                    if i > 0 {
                                        write!(out, ", ")?;
                                    }
                                    let mut elem_path = current_path.clone();
                                    elem_path
                                        .push(PathSegment::Variant(Cow::Borrowed(variant.name)));
                                    elem_path.push(PathSegment::Index(i));

                                    let elem_start = out.position();
                                    if let Ok(Some(field_value)) = enum_peek.field(i) {
                                        // Check for field-level proxy
                                        if let Some(proxy_def) = field.proxy() {
                                            self.format_via_proxy_unified(
                                                field_value,
                                                proxy_def,
                                                out,
                                                visited,
                                                format_depth + 1,
                                                type_depth + 1,
                                                short,
                                                elem_path.clone(),
                                            )?;
                                        } else {
                                            self.format_unified(
                                                field_value,
                                                out,
                                                visited,
                                                format_depth + 1,
                                                type_depth + 1,
                                                short,
                                                elem_path.clone(),
                                            )?;
                                        }
                                    }
                                    let elem_end = out.position();
                                    out.record_span(elem_path, (elem_start, elem_end));
                                }
                                write!(out, ")")?;
                            }
                        }
                    }
                }
            }
            _ if value.into_list_like().is_ok() => {
                let list = value.into_list_like().unwrap();

                // Check if elements are simple scalars - render inline if so
                let elem_shape = list.def().t();
                let is_simple = Self::shape_chunkiness(elem_shape) <= 1;

                write!(out, "[")?;
                let len = list.len();
                for (i, item) in list.iter().enumerate() {
                    if !short && !is_simple {
                        writeln!(out)?;
                        self.indent_to_output(out, format_depth + 1)?;
                    } else if i > 0 {
                        write!(out, " ")?;
                    }
                    let mut elem_path = current_path.clone();
                    elem_path.push(PathSegment::Index(i));

                    let elem_start = out.position();
                    self.format_unified(
                        item,
                        out,
                        visited,
                        format_depth + 1,
                        type_depth + 1,
                        short || is_simple,
                        elem_path.clone(),
                    )?;
                    let elem_end = out.position();
                    out.record_span(elem_path, (elem_start, elem_end));

                    if (!short && !is_simple) || i + 1 < len {
                        write!(out, ",")?;
                    }
                }
                if !short && !is_simple {
                    writeln!(out)?;
                    self.indent_to_output(out, format_depth)?;
                }
                write!(out, "]")?;
            }
            _ if value.into_map().is_ok() => {
                let map = value.into_map().unwrap();
                write!(out, "{{")?;
                for (i, (key, val)) in map.iter().enumerate() {
                    if !short {
                        writeln!(out)?;
                        self.indent_to_output(out, format_depth + 1)?;
                    }
                    // Format key
                    let key_start = out.position();
                    self.format_unified(
                        key,
                        out,
                        visited,
                        format_depth + 1,
                        type_depth + 1,
                        true, // short for keys
                        vec![],
                    )?;
                    let key_end = out.position();

                    write!(out, ": ")?;

                    // Build path for this entry (use key's string representation)
                    let key_str = self.format_peek(key);
                    let mut entry_path = current_path.clone();
                    entry_path.push(PathSegment::Key(Cow::Owned(key_str)));

                    let val_start = out.position();
                    self.format_unified(
                        val,
                        out,
                        visited,
                        format_depth + 1,
                        type_depth + 1,
                        short,
                        entry_path.clone(),
                    )?;
                    let val_end = out.position();

                    out.record_field_span(entry_path, (key_start, key_end), (val_start, val_end));

                    if !short || i + 1 < map.len() {
                        write!(out, ",")?;
                    }
                }
                if !short && !map.is_empty() {
                    writeln!(out)?;
                    self.indent_to_output(out, format_depth)?;
                }
                write!(out, "}}")?;
            }
            _ => {
                // Fallback: just write the type name
                write!(out, "{} {{ ... }}", shape)?;
            }
        }

        visited.remove(&value.id());

        // Record span for this value
        let value_end = out.position();
        out.record_span(current_path, (value_start, value_end));

        Ok(())
    }

    fn format_scalar_to_output(&self, value: Peek<'_, '_>, out: &mut impl Write) -> fmt::Result {
        // Use Display or Debug trait to format scalar values
        if value.shape().is_display() {
            write!(out, "{}", value)
        } else if value.shape().is_debug() {
            write!(out, "{:?}", value)
        } else {
            write!(out, "{}(…)", value.shape())
        }
    }

    fn indent_to_output(&self, out: &mut impl Write, depth: usize) -> fmt::Result {
        for _ in 0..depth {
            for _ in 0..self.indent_size {
                out.write_char(' ')?;
            }
        }
        Ok(())
    }
}

/// Color mode for the pretty printer.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ColorMode {
    /// Automtically detect whether colors are desired through the `NO_COLOR` environment variable.
    Auto,
    /// Always enable colors.
    Always,
    /// Never enable colors.
    Never,
}

impl ColorMode {
    /// Convert the color mode to an option of a boolean.
    pub fn enabled(&self) -> bool {
        static NO_COLOR: LazyLock<bool> = LazyLock::new(|| std::env::var_os("NO_COLOR").is_some());
        match self {
            ColorMode::Auto => !*NO_COLOR,
            ColorMode::Always => true,
            ColorMode::Never => false,
        }
    }
}

impl From<bool> for ColorMode {
    fn from(value: bool) -> Self {
        if value {
            ColorMode::Always
        } else {
            ColorMode::Never
        }
    }
}

impl From<ColorMode> for Option<bool> {
    fn from(value: ColorMode) -> Self {
        match value {
            ColorMode::Auto => None,
            ColorMode::Always => Some(true),
            ColorMode::Never => Some(false),
        }
    }
}

/// Result of formatting a value with span tracking
#[derive(Debug)]
pub struct FormattedValue {
    /// The formatted text (plain text, no ANSI colors)
    pub text: String,
    /// Map from paths to their byte spans in `text`
    pub spans: BTreeMap<Path, FieldSpan>,
}

/// Trait for output destinations that may optionally track spans.
///
/// This allows a single formatting implementation to work with both
/// simple string output and span-tracking output.
trait FormatOutput: Write {
    /// Get the current byte position in the output (for span tracking)
    fn position(&self) -> usize;

    /// Record a span for a path (value only, key=value)
    fn record_span(&mut self, _path: Path, _span: Span) {}

    /// Record a span with separate key and value spans
    fn record_field_span(&mut self, _path: Path, _key_span: Span, _value_span: Span) {}
}

/// A wrapper around any Write that implements FormatOutput but doesn't track spans.
/// Position tracking is approximated by counting bytes written.
#[allow(dead_code)]
struct NonTrackingOutput<W> {
    inner: W,
    position: usize,
}

#[allow(dead_code)]
impl<W> NonTrackingOutput<W> {
    const fn new(inner: W) -> Self {
        Self { inner, position: 0 }
    }
}

impl<W: Write> Write for NonTrackingOutput<W> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.position += s.len();
        self.inner.write_str(s)
    }
}

impl<W: Write> FormatOutput for NonTrackingOutput<W> {
    fn position(&self) -> usize {
        self.position
    }
    // Uses default no-op implementations for span recording
}

/// Context for tracking spans during value formatting
struct SpanTrackingOutput {
    output: String,
    spans: BTreeMap<Path, FieldSpan>,
}

impl SpanTrackingOutput {
    const fn new() -> Self {
        Self {
            output: String::new(),
            spans: BTreeMap::new(),
        }
    }

    fn into_formatted_value(self) -> FormattedValue {
        FormattedValue {
            text: self.output,
            spans: self.spans,
        }
    }
}

impl Write for SpanTrackingOutput {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.output.push_str(s);
        Ok(())
    }
}

impl FormatOutput for SpanTrackingOutput {
    fn position(&self) -> usize {
        self.output.len()
    }

    fn record_span(&mut self, path: Path, span: Span) {
        self.spans.insert(
            path,
            FieldSpan {
                key: span,
                value: span,
            },
        );
    }

    fn record_field_span(&mut self, path: Path, key_span: Span, value_span: Span) {
        self.spans.insert(
            path,
            FieldSpan {
                key: key_span,
                value: value_span,
            },
        );
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
        // use_colors defaults to true unless NO_COLOR is set
        // In tests, NO_COLOR=1 is set via nextest config for consistent snapshots
        assert_eq!(printer.use_colors(), std::env::var_os("NO_COLOR").is_none());
    }

    #[test]
    fn test_pretty_printer_with_methods() {
        let printer = PrettyPrinter::new()
            .with_indent_size(4)
            .with_max_depth(3)
            .with_colors(ColorMode::Never);

        assert_eq!(printer.indent_size, 4);
        assert_eq!(printer.max_depth, Some(3));
        assert!(!printer.use_colors());
    }

    #[test]
    fn test_format_peek_with_spans() {
        use crate::PathSegment;
        use facet_reflect::Peek;

        // Test with a simple tuple - no need for custom struct
        let value = ("Alice", 30u32);

        let printer = PrettyPrinter::new();
        let formatted = printer.format_peek_with_spans(Peek::new(&value));

        // Check that we got output
        assert!(!formatted.text.is_empty());
        assert!(formatted.text.contains("Alice"));
        assert!(formatted.text.contains("30"));

        // Check that spans were recorded
        assert!(!formatted.spans.is_empty());

        // Check that the root span exists (empty path)
        assert!(formatted.spans.contains_key(&vec![]));

        // Check that index spans exist
        let idx0_path = vec![PathSegment::Index(0)];
        let idx1_path = vec![PathSegment::Index(1)];
        assert!(
            formatted.spans.contains_key(&idx0_path),
            "index 0 span not found"
        );
        assert!(
            formatted.spans.contains_key(&idx1_path),
            "index 1 span not found"
        );
    }

    #[test]
    fn test_max_content_len_string() {
        let printer = PrettyPrinter::new()
            .with_colors(ColorMode::Never)
            .with_max_content_len(20);

        // Short string - no truncation
        let short = "hello";
        let output = printer.format(&short);
        assert_eq!(output, "\"hello\"");

        // Long string - should truncate middle
        let long = "abcdefghijklmnopqrstuvwxyz0123456789";
        let output = printer.format(&long);
        assert!(
            output.contains("..."),
            "should contain ellipsis: {}",
            output
        );
        assert!(output.contains("chars"), "should mention chars: {}", output);
        assert!(
            output.starts_with("\"abc"),
            "should start with beginning: {}",
            output
        );
        assert!(
            output.ends_with("89\""),
            "should end with ending: {}",
            output
        );
    }

    #[test]
    fn test_max_content_len_bytes() {
        let printer = PrettyPrinter::new()
            .with_colors(ColorMode::Never)
            .with_max_content_len(10);

        // Short bytes - no truncation
        let short: Vec<u8> = vec![1, 2, 3];
        let output = printer.format(&short);
        assert!(
            output.contains("01 02 03"),
            "should show all bytes: {}",
            output
        );

        // Long bytes - should truncate middle
        let long: Vec<u8> = (0..50).collect();
        let output = printer.format(&long);
        assert!(
            output.contains("..."),
            "should contain ellipsis: {}",
            output
        );
        assert!(output.contains("bytes"), "should mention bytes: {}", output);
    }
}
