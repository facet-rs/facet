//! Pretty printer for Shape types as Rust-like code
//!
//! This module provides functionality to format a `Shape` as Rust source code,
//! showing the type definition with its attributes.

use alloc::borrow::Cow;
use alloc::collections::BTreeMap;
use alloc::collections::BTreeSet;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt::Write;

use facet_core::{
    Attr, Def, EnumRepr, EnumType, Field, PointerType, Shape, StructKind, StructType, Type,
    UserType, Variant,
};
use owo_colors::OwoColorize;

/// Tokyo Night color scheme for syntax highlighting
pub mod colors {
    use owo_colors::Style;

    /// Keywords: struct, enum, pub, etc. (purple)
    pub fn keyword() -> Style {
        Style::new().fg_rgb::<187, 154, 247>()
    }

    /// Type names and identifiers (light blue)
    pub fn type_name() -> Style {
        Style::new().fg_rgb::<192, 202, 245>()
    }

    /// Field names (cyan)
    pub fn field_name() -> Style {
        Style::new().fg_rgb::<125, 207, 255>()
    }

    /// Primitive types: u8, i32, bool, String, etc. (teal)
    pub fn primitive() -> Style {
        Style::new().fg_rgb::<115, 218, 202>()
    }

    /// Punctuation: {, }, (, ), :, etc. (gray-blue)
    pub fn punctuation() -> Style {
        Style::new().fg_rgb::<154, 165, 206>()
    }

    /// Attribute markers: #[...] (light cyan)
    pub fn attribute() -> Style {
        Style::new().fg_rgb::<137, 221, 255>()
    }

    /// Attribute content: derive, facet, repr (blue)
    pub fn attribute_content() -> Style {
        Style::new().fg_rgb::<122, 162, 247>()
    }

    /// String literals (green)
    pub fn string() -> Style {
        Style::new().fg_rgb::<158, 206, 106>()
    }

    /// Container types: Vec, Option, HashMap (orange)
    pub fn container() -> Style {
        Style::new().fg_rgb::<255, 158, 100>()
    }

    /// Doc comments (muted gray)
    pub fn comment() -> Style {
        Style::new().fg_rgb::<86, 95, 137>()
    }
}

/// Configuration options for shape formatting
#[derive(Clone, Debug, Default)]
pub struct ShapeFormatConfig {
    /// Whether to include doc comments in the output
    pub show_doc_comments: bool,
    /// Whether to include third-party (namespaced) attributes
    pub show_third_party_attrs: bool,
    /// Whether to expand and print nested types (default: true)
    pub expand_nested_types: bool,
}

impl ShapeFormatConfig {
    /// Create a new config with default settings (no doc comments, no third-party attrs, expand nested)
    pub fn new() -> Self {
        Self {
            expand_nested_types: true,
            ..Self::default()
        }
    }

    /// Enable doc comment display
    pub fn with_doc_comments(mut self) -> Self {
        self.show_doc_comments = true;
        self
    }

    /// Enable third-party attribute display
    pub fn with_third_party_attrs(mut self) -> Self {
        self.show_third_party_attrs = true;
        self
    }

    /// Enable all metadata (doc comments and third-party attrs)
    pub fn with_all_metadata(mut self) -> Self {
        self.show_doc_comments = true;
        self.show_third_party_attrs = true;
        self
    }

    /// Disable nested type expansion (only format the root type)
    pub fn without_nested_types(mut self) -> Self {
        self.expand_nested_types = false;
        self
    }
}

/// A segment in a path through a type structure
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PathSegment {
    /// A field name in a struct
    Field(Cow<'static, str>),
    /// A variant name in an enum
    Variant(Cow<'static, str>),
    /// An index in a list/array/tuple
    Index(usize),
    /// A key in a map (stored as formatted string representation)
    Key(Cow<'static, str>),
}

/// A path to a location within a type structure
pub type Path = Vec<PathSegment>;

/// A byte span in formatted output (start, end)
pub type Span = (usize, usize);

/// Spans for a field or variant, tracking both key (name) and value (type) positions
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FieldSpan {
    /// Span of the field/variant name (e.g., "max_retries" in "max_retries: u8")
    pub key: Span,
    /// Span of the type annotation (e.g., "u8" in "max_retries: u8")
    pub value: Span,
}

/// Result of formatting a shape with span tracking
#[derive(Debug)]
pub struct FormattedShape {
    /// The formatted text (plain text, no ANSI colors)
    pub text: String,
    /// Map from paths to their field spans (key + value) in `text`
    pub spans: BTreeMap<Path, FieldSpan>,
    /// Span of the type name (e.g., "Server" in "struct Server {")
    pub type_name_span: Option<Span>,
}

/// Strip ANSI escape codes from a string
pub fn strip_ansi(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Skip escape sequence
            if chars.peek() == Some(&'[') {
                chars.next(); // consume '['
                // Skip until we hit a letter (the terminator)
                while let Some(&next) = chars.peek() {
                    chars.next();
                    if next.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            result.push(c);
        }
    }
    result
}

/// Format a Shape as Rust-like source code (plain text, no colors)
///
/// By default, this includes doc comments and third-party attributes.
pub fn format_shape(shape: &Shape) -> String {
    strip_ansi(&format_shape_colored(shape))
}

/// Format a Shape as Rust-like source code with config options (plain text, no colors)
pub fn format_shape_with_config(shape: &Shape, config: &ShapeFormatConfig) -> String {
    strip_ansi(&format_shape_colored_with_config(shape, config))
}

/// Format a Shape with span tracking for each field/variant
/// Note: spans are computed on the plain text (no ANSI codes)
///
/// By default, this includes doc comments and third-party attributes.
pub fn format_shape_with_spans(shape: &Shape) -> FormattedShape {
    format_shape_with_spans_and_config(shape, &ShapeFormatConfig::default().with_all_metadata())
}

/// Format a Shape with span tracking and config options
/// Note: spans are computed on the plain text (no ANSI codes)
pub fn format_shape_with_spans_and_config(
    shape: &Shape,
    config: &ShapeFormatConfig,
) -> FormattedShape {
    let mut ctx = SpanTrackingContext::new(config);
    format_shape_into_with_spans(shape, &mut ctx).expect("Formatting failed");
    FormattedShape {
        text: ctx.output,
        spans: ctx.spans,
        type_name_span: ctx.type_name_span,
    }
}

/// Format a Shape as Rust-like source code with ANSI colors (Tokyo Night theme)
///
/// By default, this includes doc comments and third-party attributes.
pub fn format_shape_colored(shape: &Shape) -> String {
    format_shape_colored_with_config(shape, &ShapeFormatConfig::default().with_all_metadata())
}

/// Format a Shape as Rust-like source code with ANSI colors and config options
pub fn format_shape_colored_with_config(shape: &Shape, config: &ShapeFormatConfig) -> String {
    let mut output = String::new();
    format_shape_colored_into_with_config(shape, &mut output, config).expect("Formatting failed");
    output
}

/// Format a Shape with ANSI colors into an existing String
pub fn format_shape_colored_into(shape: &Shape, output: &mut String) -> core::fmt::Result {
    format_shape_colored_into_with_config(shape, output, &ShapeFormatConfig::default())
}

/// Format a Shape with ANSI colors into an existing String with config options
pub fn format_shape_colored_into_with_config(
    shape: &Shape,
    output: &mut String,
    config: &ShapeFormatConfig,
) -> core::fmt::Result {
    let mut printed: BTreeSet<&'static str> = BTreeSet::new();
    let mut queue: Vec<&Shape> = Vec::new();
    queue.push(shape);

    while let Some(current) = queue.pop() {
        if !printed.insert(current.type_identifier) {
            continue;
        }

        if printed.len() > 1 {
            writeln!(output)?;
            writeln!(output)?;
        }

        match current.def {
            Def::Map(_) | Def::List(_) | Def::Option(_) | Def::Array(_) => {
                printed.remove(current.type_identifier);
                continue;
            }
            _ => {}
        }

        match &current.ty {
            Type::User(user_type) => match user_type {
                UserType::Struct(struct_type) => {
                    format_struct_colored(current, struct_type, output, config)?;
                    collect_nested_types(struct_type, &mut queue);
                }
                UserType::Enum(enum_type) => {
                    format_enum_colored(current, enum_type, output, config)?;
                    for variant in enum_type.variants {
                        collect_nested_types(&variant.data, &mut queue);
                    }
                }
                UserType::Union(_) | UserType::Opaque => {
                    printed.remove(current.type_identifier);
                }
            },
            _ => {
                printed.remove(current.type_identifier);
            }
        }
    }
    Ok(())
}

fn format_struct_colored(
    shape: &Shape,
    struct_type: &StructType,
    output: &mut String,
    config: &ShapeFormatConfig,
) -> core::fmt::Result {
    // Write doc comments for the struct if enabled
    if config.show_doc_comments {
        write_doc_comments_colored(shape.doc, output, "")?;
    }

    // #[derive(Facet)]
    write!(output, "{}", "#[".style(colors::attribute()))?;
    write!(output, "{}", "derive".style(colors::attribute_content()))?;
    write!(output, "{}", "(".style(colors::attribute()))?;
    write!(output, "{}", "Facet".style(colors::attribute_content()))?;
    writeln!(output, "{}", ")]".style(colors::attribute()))?;

    // Write facet attributes if any
    write_facet_attrs_colored(shape, output)?;

    // Write third-party attributes if enabled
    if config.show_third_party_attrs {
        write_third_party_attrs_colored(shape.attributes, output, "")?;
    }

    match struct_type.kind {
        StructKind::Struct => {
            write!(output, "{} ", "struct".style(colors::keyword()))?;
            write!(
                output,
                "{}",
                shape.type_identifier.style(colors::type_name())
            )?;
            writeln!(output, " {}", "{".style(colors::punctuation()))?;

            for (i, field) in struct_type.fields.iter().enumerate() {
                // Blank line between fields (not before the first one)
                if i > 0 {
                    writeln!(output)?;
                }
                // Write doc comments for the field if enabled
                if config.show_doc_comments {
                    write_doc_comments_colored(field.doc, output, "    ")?;
                }
                // Write third-party attributes for the field if enabled
                if config.show_third_party_attrs {
                    write_field_third_party_attrs_colored(field, output, "    ")?;
                }
                write!(output, "    {}", field.name.style(colors::field_name()))?;
                write!(output, "{} ", ":".style(colors::punctuation()))?;
                write_type_name_colored(field.shape(), output)?;
                writeln!(output, "{}", ",".style(colors::punctuation()))?;
            }
            write!(output, "{}", "}".style(colors::punctuation()))?;
        }
        StructKind::Tuple | StructKind::TupleStruct => {
            write!(output, "{} ", "struct".style(colors::keyword()))?;
            write!(
                output,
                "{}",
                shape.type_identifier.style(colors::type_name())
            )?;
            write!(output, "{}", "(".style(colors::punctuation()))?;
            for (i, field) in struct_type.fields.iter().enumerate() {
                if i > 0 {
                    write!(output, "{} ", ",".style(colors::punctuation()))?;
                }
                write_type_name_colored(field.shape(), output)?;
            }
            write!(
                output,
                "{}{}",
                ")".style(colors::punctuation()),
                ";".style(colors::punctuation())
            )?;
        }
        StructKind::Unit => {
            write!(output, "{} ", "struct".style(colors::keyword()))?;
            write!(
                output,
                "{}",
                shape.type_identifier.style(colors::type_name())
            )?;
            write!(output, "{}", ";".style(colors::punctuation()))?;
        }
    }
    Ok(())
}

fn format_enum_colored(
    shape: &Shape,
    enum_type: &EnumType,
    output: &mut String,
    config: &ShapeFormatConfig,
) -> core::fmt::Result {
    // Write doc comments for the enum if enabled
    if config.show_doc_comments {
        write_doc_comments_colored(shape.doc, output, "")?;
    }

    // #[derive(Facet)]
    write!(output, "{}", "#[".style(colors::attribute()))?;
    write!(output, "{}", "derive".style(colors::attribute_content()))?;
    write!(output, "{}", "(".style(colors::attribute()))?;
    write!(output, "{}", "Facet".style(colors::attribute_content()))?;
    writeln!(output, "{}", ")]".style(colors::attribute()))?;

    // Write repr for the discriminant type
    let repr_str = match enum_type.enum_repr {
        EnumRepr::RustNPO => None,
        EnumRepr::U8 => Some("u8"),
        EnumRepr::U16 => Some("u16"),
        EnumRepr::U32 => Some("u32"),
        EnumRepr::U64 => Some("u64"),
        EnumRepr::USize => Some("usize"),
        EnumRepr::I8 => Some("i8"),
        EnumRepr::I16 => Some("i16"),
        EnumRepr::I32 => Some("i32"),
        EnumRepr::I64 => Some("i64"),
        EnumRepr::ISize => Some("isize"),
    };

    if let Some(repr) = repr_str {
        write!(output, "{}", "#[".style(colors::attribute()))?;
        write!(output, "{}", "repr".style(colors::attribute_content()))?;
        write!(output, "{}", "(".style(colors::attribute()))?;
        write!(output, "{}", repr.style(colors::primitive()))?;
        writeln!(output, "{}", ")]".style(colors::attribute()))?;
    }

    // Write facet attributes if any
    write_facet_attrs_colored(shape, output)?;

    // Write third-party attributes if enabled
    if config.show_third_party_attrs {
        write_third_party_attrs_colored(shape.attributes, output, "")?;
    }

    // enum Name {
    write!(output, "{} ", "enum".style(colors::keyword()))?;
    write!(
        output,
        "{}",
        shape.type_identifier.style(colors::type_name())
    )?;
    writeln!(output, " {}", "{".style(colors::punctuation()))?;

    for (vi, variant) in enum_type.variants.iter().enumerate() {
        // Blank line between variants (not before the first one)
        if vi > 0 {
            writeln!(output)?;
        }
        // Write doc comments for the variant if enabled
        if config.show_doc_comments {
            write_doc_comments_colored(variant.doc, output, "    ")?;
        }
        // Write third-party attributes for the variant if enabled
        if config.show_third_party_attrs {
            write_variant_third_party_attrs_colored(variant, output, "    ")?;
        }

        match variant.data.kind {
            StructKind::Unit => {
                write!(output, "    {}", variant.name.style(colors::type_name()))?;
                writeln!(output, "{}", ",".style(colors::punctuation()))?;
            }
            StructKind::Tuple | StructKind::TupleStruct => {
                write!(output, "    {}", variant.name.style(colors::type_name()))?;
                write!(output, "{}", "(".style(colors::punctuation()))?;
                for (i, field) in variant.data.fields.iter().enumerate() {
                    if i > 0 {
                        write!(output, "{} ", ",".style(colors::punctuation()))?;
                    }
                    write_type_name_colored(field.shape(), output)?;
                }
                write!(output, "{}", ")".style(colors::punctuation()))?;
                writeln!(output, "{}", ",".style(colors::punctuation()))?;
            }
            StructKind::Struct => {
                write!(output, "    {}", variant.name.style(colors::type_name()))?;
                writeln!(output, " {}", "{".style(colors::punctuation()))?;
                for (fi, field) in variant.data.fields.iter().enumerate() {
                    // Blank line between variant fields (not before the first one)
                    if fi > 0 {
                        writeln!(output)?;
                    }
                    // Write doc comments for variant fields if enabled
                    if config.show_doc_comments {
                        write_doc_comments_colored(field.doc, output, "        ")?;
                    }
                    // Write third-party attributes for variant fields if enabled
                    if config.show_third_party_attrs {
                        write_field_third_party_attrs_colored(field, output, "        ")?;
                    }
                    write!(output, "        {}", field.name.style(colors::field_name()))?;
                    write!(output, "{} ", ":".style(colors::punctuation()))?;
                    write_type_name_colored(field.shape(), output)?;
                    writeln!(output, "{}", ",".style(colors::punctuation()))?;
                }
                write!(output, "    {}", "}".style(colors::punctuation()))?;
                writeln!(output, "{}", ",".style(colors::punctuation()))?;
            }
        }
    }

    write!(output, "{}", "}".style(colors::punctuation()))?;
    Ok(())
}

fn write_facet_attrs_colored(shape: &Shape, output: &mut String) -> core::fmt::Result {
    let mut attrs: Vec<String> = Vec::new();

    if let Some(tag) = shape.get_tag_attr() {
        if let Some(content) = shape.get_content_attr() {
            attrs.push(alloc::format!(
                "{}{}{}{}{}{}{}{}{}",
                "tag".style(colors::attribute_content()),
                " = ".style(colors::punctuation()),
                "\"".style(colors::string()),
                tag.style(colors::string()),
                "\"".style(colors::string()),
                ", ".style(colors::punctuation()),
                "content".style(colors::attribute_content()),
                " = ".style(colors::punctuation()),
                format!("\"{content}\"").style(colors::string()),
            ));
        } else {
            attrs.push(alloc::format!(
                "{}{}{}",
                "tag".style(colors::attribute_content()),
                " = ".style(colors::punctuation()),
                format!("\"{tag}\"").style(colors::string()),
            ));
        }
    }

    if shape.is_untagged() {
        attrs.push(alloc::format!(
            "{}",
            "untagged".style(colors::attribute_content())
        ));
    }

    if shape.has_deny_unknown_fields_attr() {
        attrs.push(alloc::format!(
            "{}",
            "deny_unknown_fields".style(colors::attribute_content())
        ));
    }

    if !attrs.is_empty() {
        write!(output, "{}", "#[".style(colors::attribute()))?;
        write!(output, "{}", "facet".style(colors::attribute_content()))?;
        write!(output, "{}", "(".style(colors::attribute()))?;
        write!(
            output,
            "{}",
            attrs.join(&format!("{}", ", ".style(colors::punctuation())))
        )?;
        writeln!(output, "{}", ")]".style(colors::attribute()))?;
    }

    Ok(())
}

/// Write doc comments with the given indentation prefix
fn write_doc_comments_colored(
    doc: &[&str],
    output: &mut String,
    indent: &str,
) -> core::fmt::Result {
    for line in doc {
        write!(output, "{indent}")?;
        writeln!(output, "{}", format!("///{line}").style(colors::comment()))?;
    }
    Ok(())
}

/// Write third-party (namespaced) attributes from a Shape's attributes
/// Groups attributes by namespace, e.g. `#[facet(args::named, args::short)]`
fn write_third_party_attrs_colored(
    attributes: &[Attr],
    output: &mut String,
    indent: &str,
) -> core::fmt::Result {
    // Group attributes by namespace
    let mut by_namespace: BTreeMap<&'static str, Vec<&'static str>> = BTreeMap::new();
    for attr in attributes {
        if let Some(ns) = attr.ns {
            by_namespace.entry(ns).or_default().push(attr.key);
        }
    }

    // Write one line per namespace with all keys
    for (ns, keys) in by_namespace {
        write!(output, "{indent}")?;
        write!(output, "{}", "#[".style(colors::attribute()))?;
        write!(output, "{}", "facet".style(colors::attribute_content()))?;
        write!(output, "{}", "(".style(colors::attribute()))?;

        for (i, key) in keys.iter().enumerate() {
            if i > 0 {
                write!(output, "{}", ", ".style(colors::punctuation()))?;
            }
            write!(output, "{}", ns.style(colors::attribute_content()))?;
            write!(output, "{}", "::".style(colors::punctuation()))?;
            write!(output, "{}", key.style(colors::attribute_content()))?;
        }

        write!(output, "{}", ")".style(colors::attribute()))?;
        writeln!(output, "{}", "]".style(colors::attribute()))?;
    }
    Ok(())
}

/// Write third-party attributes for a field
fn write_field_third_party_attrs_colored(
    field: &Field,
    output: &mut String,
    indent: &str,
) -> core::fmt::Result {
    write_third_party_attrs_colored(field.attributes, output, indent)
}

/// Write third-party attributes for a variant
fn write_variant_third_party_attrs_colored(
    variant: &Variant,
    output: &mut String,
    indent: &str,
) -> core::fmt::Result {
    write_third_party_attrs_colored(variant.attributes, output, indent)
}

fn write_type_name_colored(shape: &Shape, output: &mut String) -> core::fmt::Result {
    match shape.def {
        Def::Scalar => {
            // Check if it's a primitive type
            let id = shape.type_identifier;
            if is_primitive_type(id) {
                write!(output, "{}", id.style(colors::primitive()))?;
            } else {
                write!(output, "{}", id.style(colors::type_name()))?;
            }
        }
        Def::Pointer(_) => {
            if let Type::Pointer(PointerType::Reference(r)) = shape.ty
                && let Def::Array(array_def) = r.target.def
            {
                write!(output, "{}", "&[".style(colors::punctuation()))?;
                write_type_name_colored(array_def.t, output)?;
                write!(
                    output,
                    "{}{}{}",
                    "; ".style(colors::punctuation()),
                    array_def.n.style(colors::primitive()),
                    "]".style(colors::punctuation())
                )?;
                return Ok(());
            }
            write!(
                output,
                "{}",
                shape.type_identifier.style(colors::type_name())
            )?;
        }
        Def::List(list_def) => {
            write!(output, "{}", "Vec".style(colors::container()))?;
            write!(output, "{}", "<".style(colors::punctuation()))?;
            write_type_name_colored(list_def.t, output)?;
            write!(output, "{}", ">".style(colors::punctuation()))?;
        }
        Def::Array(array_def) => {
            write!(output, "{}", "[".style(colors::punctuation()))?;
            write_type_name_colored(array_def.t, output)?;
            write!(
                output,
                "{}{}{}",
                "; ".style(colors::punctuation()),
                array_def.n.style(colors::primitive()),
                "]".style(colors::punctuation())
            )?;
        }
        Def::Map(map_def) => {
            let map_name = if shape.type_identifier.contains("BTreeMap") {
                "BTreeMap"
            } else {
                "HashMap"
            };
            write!(output, "{}", map_name.style(colors::container()))?;
            write!(output, "{}", "<".style(colors::punctuation()))?;
            write_type_name_colored(map_def.k, output)?;
            write!(output, "{} ", ",".style(colors::punctuation()))?;
            write_type_name_colored(map_def.v, output)?;
            write!(output, "{}", ">".style(colors::punctuation()))?;
        }
        Def::Option(option_def) => {
            write!(output, "{}", "Option".style(colors::container()))?;
            write!(output, "{}", "<".style(colors::punctuation()))?;
            write_type_name_colored(option_def.t, output)?;
            write!(output, "{}", ">".style(colors::punctuation()))?;
        }
        _ => {
            let id = shape.type_identifier;
            if is_primitive_type(id) {
                write!(output, "{}", id.style(colors::primitive()))?;
            } else {
                write!(output, "{}", id.style(colors::type_name()))?;
            }
        }
    }
    Ok(())
}

/// Check if a type identifier is a primitive type
fn is_primitive_type(id: &str) -> bool {
    matches!(
        id,
        "u8" | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "isize"
            | "f32"
            | "f64"
            | "bool"
            | "char"
            | "str"
            | "&str"
            | "String"
    )
}

/// Context for tracking spans during formatting
struct SpanTrackingContext<'a> {
    output: String,
    spans: BTreeMap<Path, FieldSpan>,
    /// Span of the type name (struct/enum identifier)
    type_name_span: Option<Span>,
    /// Current path prefix (for nested types)
    current_type: Option<&'static str>,
    /// Configuration for what to include
    config: &'a ShapeFormatConfig,
}

impl<'a> SpanTrackingContext<'a> {
    fn new(config: &'a ShapeFormatConfig) -> Self {
        Self {
            output: String::new(),
            spans: BTreeMap::new(),
            type_name_span: None,
            current_type: None,
            config,
        }
    }

    fn len(&self) -> usize {
        self.output.len()
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

/// Format a Shape with span tracking
fn format_shape_into_with_spans(shape: &Shape, ctx: &mut SpanTrackingContext) -> core::fmt::Result {
    // Track which types we've already printed to avoid duplicates
    let mut printed: BTreeSet<&'static str> = BTreeSet::new();
    // Queue of types to print
    let mut queue: Vec<&Shape> = Vec::new();

    // Start with the root shape
    queue.push(shape);

    while let Some(current) = queue.pop() {
        // Skip if we've already printed this type
        if !printed.insert(current.type_identifier) {
            continue;
        }

        // Add separator between type definitions
        if printed.len() > 1 {
            writeln!(ctx.output)?;
            writeln!(ctx.output)?;
        }

        // First check def for container types (Map, List, Option, Array)
        // These have rich generic info even when ty is Opaque
        match current.def {
            Def::Map(_) | Def::List(_) | Def::Option(_) | Def::Array(_) => {
                // Don't print container types as definitions, they're inline
                printed.remove(current.type_identifier);
                continue;
            }
            _ => {}
        }

        // Then check ty for user-defined types
        match &current.ty {
            Type::User(user_type) => match user_type {
                UserType::Struct(struct_type) => {
                    ctx.current_type = Some(current.type_identifier);
                    format_struct_with_spans(current, struct_type, ctx)?;
                    ctx.current_type = None;
                    // Queue nested types from fields (if expansion enabled)
                    if ctx.config.expand_nested_types {
                        collect_nested_types(struct_type, &mut queue);
                    }
                }
                UserType::Enum(enum_type) => {
                    ctx.current_type = Some(current.type_identifier);
                    format_enum_with_spans(current, enum_type, ctx)?;
                    ctx.current_type = None;
                    // Queue nested types from variants (if expansion enabled)
                    if ctx.config.expand_nested_types {
                        for variant in enum_type.variants {
                            collect_nested_types(&variant.data, &mut queue);
                        }
                    }
                }
                UserType::Union(_) | UserType::Opaque => {
                    // For union/opaque types, just show the type identifier
                    // Don't actually print anything since we can't expand them
                    printed.remove(current.type_identifier);
                }
            },
            _ => {
                // For non-user types (primitives, pointers, etc.), don't print
                printed.remove(current.type_identifier);
            }
        }
    }
    Ok(())
}

fn format_struct_with_spans(
    shape: &Shape,
    struct_type: &StructType,
    ctx: &mut SpanTrackingContext,
) -> core::fmt::Result {
    // Track start of the whole type definition
    let type_start = ctx.len();

    // Write doc comments if enabled
    if ctx.config.show_doc_comments {
        write_doc_comments(shape.doc, &mut ctx.output, "")?;
    }

    // Write #[derive(Facet)]
    writeln!(ctx.output, "#[derive(Facet)]")?;

    // Write facet attributes if any
    write_facet_attrs(shape, &mut ctx.output)?;

    // Write third-party attributes if enabled
    if ctx.config.show_third_party_attrs {
        write_third_party_attrs(shape.attributes, &mut ctx.output, "")?;
    }

    // Write struct definition
    match struct_type.kind {
        StructKind::Struct => {
            write!(ctx.output, "struct ")?;
            let type_name_start = ctx.len();
            write!(ctx.output, "{}", shape.type_identifier)?;
            let type_name_end = ctx.len();
            ctx.type_name_span = Some((type_name_start, type_name_end));
            writeln!(ctx.output, " {{")?;
            for field in struct_type.fields {
                // Write doc comments for the field if enabled
                if ctx.config.show_doc_comments {
                    write_doc_comments(field.doc, &mut ctx.output, "    ")?;
                }
                // Write third-party attributes for the field if enabled
                if ctx.config.show_third_party_attrs {
                    write_field_third_party_attrs(field, &mut ctx.output, "    ")?;
                }
                write!(ctx.output, "    ")?;
                // Track the span of the field name (key)
                let key_start = ctx.len();
                write!(ctx.output, "{}", field.name)?;
                let key_end = ctx.len();
                write!(ctx.output, ": ")?;
                // Track the span of the type annotation (value)
                let value_start = ctx.len();
                write_type_name(field.shape(), &mut ctx.output)?;
                let value_end = ctx.len();
                ctx.record_field_span(
                    vec![PathSegment::Field(Cow::Borrowed(field.name))],
                    (key_start, key_end),
                    (value_start, value_end),
                );
                writeln!(ctx.output, ",")?;
            }
            write!(ctx.output, "}}")?;
        }
        StructKind::Tuple | StructKind::TupleStruct => {
            write!(ctx.output, "struct ")?;
            let type_name_start = ctx.len();
            write!(ctx.output, "{}", shape.type_identifier)?;
            let type_name_end = ctx.len();
            ctx.type_name_span = Some((type_name_start, type_name_end));
            write!(ctx.output, "(")?;
            for (i, field) in struct_type.fields.iter().enumerate() {
                if i > 0 {
                    write!(ctx.output, ", ")?;
                }
                // For tuple structs, key and value span are the same (just the type)
                let type_start = ctx.len();
                write_type_name(field.shape(), &mut ctx.output)?;
                let type_end = ctx.len();
                // Use field name if available, otherwise use index as string
                let field_name = if !field.name.is_empty() {
                    field.name
                } else {
                    // Tuple fields don't have names, skip span tracking
                    continue;
                };
                ctx.record_field_span(
                    vec![PathSegment::Field(Cow::Borrowed(field_name))],
                    (type_start, type_end), // key is the type itself for tuples
                    (type_start, type_end),
                );
            }
            write!(ctx.output, ");")?;
        }
        StructKind::Unit => {
            write!(ctx.output, "struct ")?;
            let type_name_start = ctx.len();
            write!(ctx.output, "{}", shape.type_identifier)?;
            let type_name_end = ctx.len();
            ctx.type_name_span = Some((type_name_start, type_name_end));
            write!(ctx.output, ";")?;
        }
    }

    // Record span for the root (empty path) covering the whole type
    let type_end = ctx.len();
    ctx.record_field_span(vec![], (type_start, type_end), (type_start, type_end));

    Ok(())
}

fn format_enum_with_spans(
    shape: &Shape,
    enum_type: &EnumType,
    ctx: &mut SpanTrackingContext,
) -> core::fmt::Result {
    // Track start of the whole type definition
    let type_start = ctx.len();

    // Write doc comments if enabled
    if ctx.config.show_doc_comments {
        write_doc_comments(shape.doc, &mut ctx.output, "")?;
    }

    // Write #[derive(Facet)]
    writeln!(ctx.output, "#[derive(Facet)]")?;

    // Write repr for the discriminant type
    let repr_str = match enum_type.enum_repr {
        EnumRepr::RustNPO => None,
        EnumRepr::U8 => Some("u8"),
        EnumRepr::U16 => Some("u16"),
        EnumRepr::U32 => Some("u32"),
        EnumRepr::U64 => Some("u64"),
        EnumRepr::USize => Some("usize"),
        EnumRepr::I8 => Some("i8"),
        EnumRepr::I16 => Some("i16"),
        EnumRepr::I32 => Some("i32"),
        EnumRepr::I64 => Some("i64"),
        EnumRepr::ISize => Some("isize"),
    };

    if let Some(repr) = repr_str {
        writeln!(ctx.output, "#[repr({repr})]")?;
    }

    // Write facet attributes if any
    write_facet_attrs(shape, &mut ctx.output)?;

    // Write third-party attributes if enabled
    if ctx.config.show_third_party_attrs {
        write_third_party_attrs(shape.attributes, &mut ctx.output, "")?;
    }

    // Write enum definition
    write!(ctx.output, "enum ")?;
    let type_name_start = ctx.len();
    write!(ctx.output, "{}", shape.type_identifier)?;
    let type_name_end = ctx.len();
    ctx.type_name_span = Some((type_name_start, type_name_end));
    writeln!(ctx.output, " {{")?;

    for variant in enum_type.variants {
        // Write doc comments for the variant if enabled
        if ctx.config.show_doc_comments {
            write_doc_comments(variant.doc, &mut ctx.output, "    ")?;
        }
        // Write third-party attributes for the variant if enabled
        if ctx.config.show_third_party_attrs {
            write_variant_third_party_attrs(variant, &mut ctx.output, "    ")?;
        }

        match variant.data.kind {
            StructKind::Unit => {
                write!(ctx.output, "    ")?;
                // For unit variants, key and value are the same (just the variant name)
                let name_start = ctx.len();
                write!(ctx.output, "{}", variant.name)?;
                let name_end = ctx.len();
                ctx.record_field_span(
                    vec![PathSegment::Variant(Cow::Borrowed(variant.name))],
                    (name_start, name_end),
                    (name_start, name_end),
                );
                writeln!(ctx.output, ",")?;
            }
            StructKind::Tuple | StructKind::TupleStruct => {
                write!(ctx.output, "    ")?;
                let variant_name_start = ctx.len();
                write!(ctx.output, "{}", variant.name)?;
                let variant_name_end = ctx.len();
                write!(ctx.output, "(")?;
                let tuple_start = ctx.len();
                for (i, field) in variant.data.fields.iter().enumerate() {
                    if i > 0 {
                        write!(ctx.output, ", ")?;
                    }
                    let type_start = ctx.len();
                    write_type_name(field.shape(), &mut ctx.output)?;
                    let type_end = ctx.len();
                    // Track span for variant field
                    if !field.name.is_empty() {
                        ctx.record_field_span(
                            vec![
                                PathSegment::Variant(Cow::Borrowed(variant.name)),
                                PathSegment::Field(Cow::Borrowed(field.name)),
                            ],
                            (type_start, type_end),
                            (type_start, type_end),
                        );
                    }
                }
                write!(ctx.output, ")")?;
                let tuple_end = ctx.len();
                // Record variant span: key is the name, value is the tuple contents
                ctx.record_field_span(
                    vec![PathSegment::Variant(Cow::Borrowed(variant.name))],
                    (variant_name_start, variant_name_end),
                    (tuple_start, tuple_end),
                );
                writeln!(ctx.output, ",")?;
            }
            StructKind::Struct => {
                write!(ctx.output, "    ")?;
                let variant_name_start = ctx.len();
                write!(ctx.output, "{}", variant.name)?;
                let variant_name_end = ctx.len();
                writeln!(ctx.output, " {{")?;
                let struct_start = ctx.len();
                for field in variant.data.fields {
                    // Write doc comments for variant fields if enabled
                    if ctx.config.show_doc_comments {
                        write_doc_comments(field.doc, &mut ctx.output, "        ")?;
                    }
                    // Write third-party attributes for variant fields if enabled
                    if ctx.config.show_third_party_attrs {
                        write_field_third_party_attrs(field, &mut ctx.output, "        ")?;
                    }
                    write!(ctx.output, "        ")?;
                    let key_start = ctx.len();
                    write!(ctx.output, "{}", field.name)?;
                    let key_end = ctx.len();
                    write!(ctx.output, ": ")?;
                    let value_start = ctx.len();
                    write_type_name(field.shape(), &mut ctx.output)?;
                    let value_end = ctx.len();
                    ctx.record_field_span(
                        vec![
                            PathSegment::Variant(Cow::Borrowed(variant.name)),
                            PathSegment::Field(Cow::Borrowed(field.name)),
                        ],
                        (key_start, key_end),
                        (value_start, value_end),
                    );
                    writeln!(ctx.output, ",")?;
                }
                write!(ctx.output, "    }}")?;
                let struct_end = ctx.len();
                // Record variant span: key is the name, value is the struct body
                ctx.record_field_span(
                    vec![PathSegment::Variant(Cow::Borrowed(variant.name))],
                    (variant_name_start, variant_name_end),
                    (struct_start, struct_end),
                );
                writeln!(ctx.output, ",")?;
            }
        }
    }

    write!(ctx.output, "}}")?;

    // Record span for the root (empty path) covering the whole type
    let type_end = ctx.len();
    ctx.record_field_span(vec![], (type_start, type_end), (type_start, type_end));

    Ok(())
}

/// Collect nested user-defined types from struct fields
fn collect_nested_types<'a>(struct_type: &'a StructType, queue: &mut Vec<&'a Shape>) {
    for field in struct_type.fields {
        collect_from_shape(field.shape(), queue);
    }
}

/// Recursively collect user-defined types from a shape (handles containers)
fn collect_from_shape<'a>(shape: &'a Shape, queue: &mut Vec<&'a Shape>) {
    match shape.def {
        Def::List(list_def) => collect_from_shape(list_def.t, queue),
        Def::Array(array_def) => collect_from_shape(array_def.t, queue),
        Def::Map(map_def) => {
            collect_from_shape(map_def.k, queue);
            collect_from_shape(map_def.v, queue);
        }
        Def::Option(option_def) => collect_from_shape(option_def.t, queue),
        _ => {
            // Check if it's a user-defined type worth expanding
            if let Type::User(UserType::Struct(_) | UserType::Enum(_)) = &shape.ty {
                queue.push(shape);
            }
        }
    }
}

// Plain text helpers for span tracking (ANSI codes would break byte offsets)
fn write_facet_attrs(shape: &Shape, output: &mut String) -> core::fmt::Result {
    let mut attrs: Vec<String> = Vec::new();

    if let Some(tag) = shape.get_tag_attr() {
        if let Some(content) = shape.get_content_attr() {
            attrs.push(alloc::format!("tag = \"{tag}\", content = \"{content}\""));
        } else {
            attrs.push(alloc::format!("tag = \"{tag}\""));
        }
    }

    if shape.is_untagged() {
        attrs.push("untagged".into());
    }

    if shape.has_deny_unknown_fields_attr() {
        attrs.push("deny_unknown_fields".into());
    }

    if !attrs.is_empty() {
        writeln!(output, "#[facet({})]", attrs.join(", "))?;
    }

    Ok(())
}

/// Write doc comments (plain text) with the given indentation prefix
fn write_doc_comments(doc: &[&str], output: &mut String, indent: &str) -> core::fmt::Result {
    for line in doc {
        write!(output, "{indent}")?;
        writeln!(output, "///{line}")?;
    }
    Ok(())
}

/// Write third-party (namespaced) attributes (plain text)
fn write_third_party_attrs(
    attributes: &[Attr],
    output: &mut String,
    indent: &str,
) -> core::fmt::Result {
    // Group attributes by namespace
    let mut by_namespace: BTreeMap<&'static str, Vec<&'static str>> = BTreeMap::new();
    for attr in attributes {
        if let Some(ns) = attr.ns {
            by_namespace.entry(ns).or_default().push(attr.key);
        }
    }

    // Write one line per namespace with all keys
    for (ns, keys) in by_namespace {
        write!(output, "{indent}")?;
        write!(output, "#[facet(")?;
        for (i, key) in keys.iter().enumerate() {
            if i > 0 {
                write!(output, ", ")?;
            }
            write!(output, "{ns}::{key}")?;
        }
        writeln!(output, ")]")?;
    }
    Ok(())
}

/// Write third-party attributes for a field (plain text)
fn write_field_third_party_attrs(
    field: &Field,
    output: &mut String,
    indent: &str,
) -> core::fmt::Result {
    write_third_party_attrs(field.attributes, output, indent)
}

/// Write third-party attributes for a variant (plain text)
fn write_variant_third_party_attrs(
    variant: &Variant,
    output: &mut String,
    indent: &str,
) -> core::fmt::Result {
    write_third_party_attrs(variant.attributes, output, indent)
}

fn write_type_name(shape: &Shape, output: &mut String) -> core::fmt::Result {
    match shape.def {
        Def::Scalar => {
            write!(output, "{}", shape.type_identifier)?;
        }
        Def::Pointer(_) => {
            if let Type::Pointer(PointerType::Reference(r)) = shape.ty
                && let Def::Array(array_def) = r.target.def
            {
                write!(output, "&[")?;
                write_type_name(array_def.t, output)?;
                write!(output, "; {}]", array_def.n)?;
                return Ok(());
            }
            write!(output, "{}", shape.type_identifier)?;
        }
        Def::List(list_def) => {
            write!(output, "Vec<")?;
            write_type_name(list_def.t, output)?;
            write!(output, ">")?;
        }
        Def::Array(array_def) => {
            write!(output, "[")?;
            write_type_name(array_def.t, output)?;
            write!(output, "; {}]", array_def.n)?;
        }
        Def::Map(map_def) => {
            let map_name = if shape.type_identifier.contains("BTreeMap") {
                "BTreeMap"
            } else {
                "HashMap"
            };
            write!(output, "{map_name}<")?;
            write_type_name(map_def.k, output)?;
            write!(output, ", ")?;
            write_type_name(map_def.v, output)?;
            write!(output, ">")?;
        }
        Def::Option(option_def) => {
            write!(output, "Option<")?;
            write_type_name(option_def.t, output)?;
            write!(output, ">")?;
        }
        _ => {
            write!(output, "{}", shape.type_identifier)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet::Facet;

    #[test]
    fn test_simple_struct() {
        #[derive(Facet)]
        struct Simple {
            name: String,
            count: u32,
        }

        let output = format_shape(Simple::SHAPE);
        assert!(output.contains("struct Simple"));
        assert!(output.contains("name: String"));
        assert!(output.contains("count: u32"));
    }

    #[test]
    fn test_enum_with_tag() {
        #[derive(Facet)]
        #[repr(C)]
        #[facet(tag = "type")]
        #[allow(dead_code)]
        enum Tagged {
            A { x: i32 },
            B { y: String },
        }

        let output = format_shape(Tagged::SHAPE);
        assert!(output.contains("enum Tagged"));
        assert!(output.contains("#[facet(tag = \"type\")]"));
    }

    #[test]
    fn test_nested_types() {
        #[derive(Facet)]
        #[allow(dead_code)]
        struct Inner {
            value: i32,
        }

        #[derive(Facet)]
        #[allow(dead_code)]
        struct Outer {
            inner: Inner,
            name: String,
        }

        let output = format_shape(Outer::SHAPE);
        // Should contain both Outer and Inner definitions
        assert!(output.contains("struct Outer"), "Missing Outer: {output}");
        assert!(
            output.contains("inner: Inner"),
            "Missing inner field: {output}"
        );
        assert!(
            output.contains("struct Inner"),
            "Missing Inner definition: {output}"
        );
        assert!(
            output.contains("value: i32"),
            "Missing value field: {output}"
        );
    }

    #[test]
    fn test_nested_in_vec() {
        #[derive(Facet)]
        #[allow(dead_code)]
        struct Item {
            id: u32,
        }

        #[derive(Facet)]
        #[allow(dead_code)]
        struct Container {
            items: Vec<Item>,
        }

        let output = format_shape(Container::SHAPE);
        // Should contain both Container and Item definitions
        assert!(
            output.contains("struct Container"),
            "Missing Container: {output}"
        );
        assert!(
            output.contains("items: Vec<Item>"),
            "Missing items field: {output}"
        );
        assert!(
            output.contains("struct Item"),
            "Missing Item definition: {output}"
        );
    }

    #[test]
    fn test_format_shape_with_spans() {
        #[derive(Facet)]
        #[allow(dead_code)]
        struct Config {
            name: String,
            max_retries: u8,
            enabled: bool,
        }

        let result = format_shape_with_spans(Config::SHAPE);

        // Check that spans were recorded for each field
        let name_path = vec![PathSegment::Field(Cow::Borrowed("name"))];
        let retries_path = vec![PathSegment::Field(Cow::Borrowed("max_retries"))];
        let enabled_path = vec![PathSegment::Field(Cow::Borrowed("enabled"))];

        assert!(
            result.spans.contains_key(&name_path),
            "Missing span for 'name' field. Spans: {:?}",
            result.spans
        );
        assert!(
            result.spans.contains_key(&retries_path),
            "Missing span for 'max_retries' field. Spans: {:?}",
            result.spans
        );
        assert!(
            result.spans.contains_key(&enabled_path),
            "Missing span for 'enabled' field. Spans: {:?}",
            result.spans
        );

        // Verify the span for max_retries points to "u8"
        let field_span = &result.spans[&retries_path];
        let spanned_text = &result.text[field_span.value.0..field_span.value.1];
        assert_eq!(spanned_text, "u8", "Expected 'u8', got '{spanned_text}'");
    }

    #[test]
    fn test_format_enum_with_spans() {
        #[derive(Facet)]
        #[repr(u8)]
        #[allow(dead_code)]
        enum Status {
            Active,
            Pending,
            Error { code: i32, message: String },
        }

        let result = format_shape_with_spans(Status::SHAPE);

        // Check variant spans
        let active_path = vec![PathSegment::Variant(Cow::Borrowed("Active"))];
        let error_path = vec![PathSegment::Variant(Cow::Borrowed("Error"))];
        let error_code_path = vec![
            PathSegment::Variant(Cow::Borrowed("Error")),
            PathSegment::Field(Cow::Borrowed("code")),
        ];

        assert!(
            result.spans.contains_key(&active_path),
            "Missing span for 'Active' variant. Spans: {:?}",
            result.spans
        );
        assert!(
            result.spans.contains_key(&error_path),
            "Missing span for 'Error' variant. Spans: {:?}",
            result.spans
        );
        assert!(
            result.spans.contains_key(&error_code_path),
            "Missing span for 'Error.code' field. Spans: {:?}",
            result.spans
        );

        // Verify the span for code points to "i32"
        let field_span = &result.spans[&error_code_path];
        let spanned_text = &result.text[field_span.value.0..field_span.value.1];
        assert_eq!(spanned_text, "i32", "Expected 'i32', got '{spanned_text}'");
    }

    #[test]
    fn test_format_with_doc_comments() {
        /// A configuration struct for the application.
        #[derive(Facet)]
        #[allow(dead_code)]
        struct Config {
            /// The name of the configuration.
            name: String,
            /// Maximum number of retries.
            max_retries: u8,
        }

        // With doc comments (default)
        let output = format_shape(Config::SHAPE);
        assert!(
            output.contains("/// A configuration struct"),
            "Should contain struct doc comment: {output}"
        );
        assert!(
            output.contains("/// The name of the configuration"),
            "Should contain field doc comment: {output}"
        );
        assert!(
            output.contains("/// Maximum number of retries"),
            "Should contain field doc comment: {output}"
        );

        // Without doc comments (explicit config)
        let config = ShapeFormatConfig::new();
        let output_without = format_shape_with_config(Config::SHAPE, &config);
        assert!(
            !output_without.contains("///"),
            "Should not contain doc comments when disabled: {output_without}"
        );
    }

    #[test]
    fn test_format_enum_with_doc_comments() {
        /// Status of an operation.
        #[derive(Facet)]
        #[repr(u8)]
        #[allow(dead_code)]
        enum Status {
            /// The operation is active.
            Active,
            /// The operation failed with an error.
            Error {
                /// Error code.
                code: i32,
            },
        }

        let config = ShapeFormatConfig::new().with_doc_comments();
        let output = format_shape_with_config(Status::SHAPE, &config);

        assert!(
            output.contains("/// Status of an operation"),
            "Should contain enum doc comment: {output}"
        );
        assert!(
            output.contains("/// The operation is active"),
            "Should contain variant doc comment: {output}"
        );
        assert!(
            output.contains("/// The operation failed"),
            "Should contain variant doc comment: {output}"
        );
        assert!(
            output.contains("/// Error code"),
            "Should contain variant field doc comment: {output}"
        );
    }
}
