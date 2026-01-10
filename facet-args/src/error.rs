extern crate alloc;

use crate::span::Span;
use core::fmt;
use facet_core::{Field, Shape, Type, UserType, Variant};
use facet_reflect::ReflectError;
use heck::ToKebabCase;

/// An args parsing error, with input info, so that it can be formatted nicely
#[derive(Debug)]
pub struct ArgsErrorWithInput {
    /// The inner error
    pub(crate) inner: ArgsError,

    /// All CLI arguments joined by a space
    pub(crate) flattened_args: String,
}

impl ArgsErrorWithInput {
    /// Returns true if this is a help request (not a real error)
    pub fn is_help_request(&self) -> bool {
        self.inner.kind.is_help_request()
    }

    /// If this is a help request, returns the help text
    pub fn help_text(&self) -> Option<&str> {
        self.inner.kind.help_text()
    }
}

impl core::fmt::Display for ArgsErrorWithInput {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // For help requests, just display the help text directly
        if let Some(help) = self.help_text() {
            return write!(f, "{}", help);
        }

        // Write the main error message
        write!(f, "error: {}", self.inner.kind.label())?;

        // If we have help text, add it
        if let Some(help) = self.inner.kind.help() {
            write!(f, "\n\n{help}")?;
        }

        Ok(())
    }
}

impl core::error::Error for ArgsErrorWithInput {}

/// An args parsing error (without input info)
#[derive(Debug)]
pub struct ArgsError {
    /// Where the error occurred
    pub span: Span,

    /// The specific error that occurred while parsing arguments.
    pub kind: ArgsErrorKind,
}

/// An error kind for argument parsing.
///
/// Stores references to static shape/field/variant info for lazy formatting.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum ArgsErrorKind {
    /// Help was requested via -h, --help, -help, or /?
    ///
    /// This is not really an "error" but uses the error path to return
    /// help text when the user explicitly requests it.
    HelpRequested {
        /// The generated help text
        help_text: alloc::string::String,
    },

    /// Did not expect a positional argument at this position
    UnexpectedPositionalArgument {
        /// Fields of the struct/variant being parsed (for help text)
        fields: &'static [Field],
    },

    /// Wanted to look up a field, for example `--something` in a struct,
    /// but the current shape was not a struct.
    NoFields {
        /// The shape that was being parsed
        shape: &'static Shape,
    },

    /// Found an enum field without the args::subcommand attribute.
    /// Enums can only be used as subcommands when explicitly marked.
    EnumWithoutSubcommandAttribute {
        /// The field that has the enum type
        field: &'static Field,
    },

    /// Passed `--something` (see span), no such long flag
    UnknownLongFlag {
        /// The flag that was passed
        flag: String,
        /// Fields of the struct/variant being parsed
        fields: &'static [Field],
    },

    /// Passed `-j` (see span), no such short flag
    UnknownShortFlag {
        /// The flag that was passed
        flag: String,
        /// Fields of the struct/variant being parsed
        fields: &'static [Field],
        /// Precise span for the invalid flag (used for chained short flags like `-axc` where `x` is invalid)
        precise_span: Option<Span>,
    },

    /// Struct/type expected a certain argument to be passed and it wasn't
    MissingArgument {
        /// The field that was missing
        field: &'static Field,
    },

    /// Expected a value of type shape, got EOF
    ExpectedValueGotEof {
        /// The type that was expected
        shape: &'static Shape,
    },

    /// Unknown subcommand name
    UnknownSubcommand {
        /// The subcommand that was provided
        provided: String,
        /// Variants of the enum (subcommands)
        variants: &'static [Variant],
    },

    /// Required subcommand was not provided
    MissingSubcommand {
        /// Variants of the enum (available subcommands)
        variants: &'static [Variant],
    },

    /// Generic reflection error: something went wrong
    ReflectError(ReflectError),
}

impl ArgsErrorKind {
    /// Returns a precise span override if the error kind has one.
    /// This is used for errors like `UnknownShortFlag` in chained flags
    /// where we want to highlight just the invalid character, not the whole arg.
    pub fn precise_span(&self) -> Option<Span> {
        match self {
            ArgsErrorKind::UnknownShortFlag { precise_span, .. } => *precise_span,
            _ => None,
        }
    }

    /// Returns an error code for this error kind.
    pub fn code(&self) -> &'static str {
        match self {
            ArgsErrorKind::HelpRequested { .. } => "args::help",
            ArgsErrorKind::UnexpectedPositionalArgument { .. } => "args::unexpected_positional",
            ArgsErrorKind::NoFields { .. } => "args::no_fields",
            ArgsErrorKind::EnumWithoutSubcommandAttribute { .. } => {
                "args::enum_without_subcommand_attribute"
            }
            ArgsErrorKind::UnknownLongFlag { .. } => "args::unknown_long_flag",
            ArgsErrorKind::UnknownShortFlag { .. } => "args::unknown_short_flag",
            ArgsErrorKind::MissingArgument { .. } => "args::missing_argument",
            ArgsErrorKind::ExpectedValueGotEof { .. } => "args::expected_value",
            ArgsErrorKind::UnknownSubcommand { .. } => "args::unknown_subcommand",
            ArgsErrorKind::MissingSubcommand { .. } => "args::missing_subcommand",
            ArgsErrorKind::ReflectError(_) => "args::reflect_error",
        }
    }

    /// Returns a short label for the error (shown inline in the source)
    pub fn label(&self) -> String {
        match self {
            ArgsErrorKind::HelpRequested { .. } => "help requested".to_string(),
            ArgsErrorKind::UnexpectedPositionalArgument { .. } => {
                "unexpected positional argument".to_string()
            }
            ArgsErrorKind::NoFields { shape } => {
                format!("cannot parse arguments into `{}`", shape.type_identifier)
            }
            ArgsErrorKind::EnumWithoutSubcommandAttribute { field } => {
                format!(
                    "enum field `{}` must be marked with `#[facet(args::subcommand)]` to be used as subcommands",
                    field.name
                )
            }
            ArgsErrorKind::UnknownLongFlag { flag, .. } => {
                format!("unknown flag `--{flag}`")
            }
            ArgsErrorKind::UnknownShortFlag { flag, .. } => {
                format!("unknown flag `-{flag}`")
            }
            ArgsErrorKind::ExpectedValueGotEof { shape } => {
                // Unwrap Option to show the inner type
                let inner_type = unwrap_option_type(shape);
                format!("expected `{inner_type}` value")
            }
            ArgsErrorKind::ReflectError(err) => format_reflect_error(err),
            ArgsErrorKind::MissingArgument { field } => {
                let doc_hint = field
                    .doc
                    .first()
                    .map(|d| format!(" ({})", d.trim()))
                    .unwrap_or_default();
                let positional = field.has_attr(Some("args"), "positional");
                let arg_name = if positional {
                    format!("<{}>", field.name.to_kebab_case())
                } else {
                    format!("--{}", field.name.to_kebab_case())
                };
                format!("missing required argument `{arg_name}`{doc_hint}")
            }
            ArgsErrorKind::UnknownSubcommand { provided, .. } => {
                format!("unknown subcommand `{provided}`")
            }
            ArgsErrorKind::MissingSubcommand { .. } => "expected a subcommand".to_string(),
        }
    }

    /// Returns help text for this error
    pub fn help(&self) -> Option<Box<dyn core::fmt::Display + '_>> {
        match self {
            ArgsErrorKind::UnexpectedPositionalArgument { fields } => {
                if fields.is_empty() {
                    return Some(Box::new(
                        "this command does not accept positional arguments",
                    ));
                }

                // Check if any of the fields are enums without subcommand attributes
                if let Some(enum_field) = fields.iter().find(|f| {
                    matches!(f.shape().ty, Type::User(UserType::Enum(_)))
                        && !f.has_attr(Some("args"), "subcommand")
                }) {
                    return Some(Box::new(format!(
                        "available options:\n{}\n\nnote: field `{}` is an enum but missing `#[facet(args::subcommand)]` attribute. Enums must be marked as subcommands to accept positional arguments.",
                        format_available_flags(fields),
                        enum_field.name
                    )));
                }

                let flags = format_available_flags(fields);
                Some(Box::new(format!("available options:\n{flags}")))
            }
            ArgsErrorKind::UnknownLongFlag { flag, fields } => {
                // Try to find a similar flag
                if let Some(suggestion) = find_similar_flag(flag, fields) {
                    return Some(Box::new(format!("did you mean `--{suggestion}`?")));
                }
                if fields.is_empty() {
                    return None;
                }
                let flags = format_available_flags(fields);
                Some(Box::new(format!("available options:\n{flags}")))
            }
            ArgsErrorKind::UnknownShortFlag { flag, fields, .. } => {
                // Try to find what flag the user might have meant
                let short_char = flag.chars().next();
                if let Some(field) = fields.iter().find(|f| get_short_flag(f) == short_char) {
                    return Some(Box::new(format!(
                        "`-{}` is `--{}`",
                        flag,
                        field.name.to_kebab_case()
                    )));
                }
                if fields.is_empty() {
                    return None;
                }
                let flags = format_available_flags(fields);
                Some(Box::new(format!("available options:\n{flags}")))
            }
            ArgsErrorKind::MissingArgument { field } => {
                let kebab = field.name.to_kebab_case();
                let type_name = field.shape().type_identifier;
                let positional = field.has_attr(Some("args"), "positional");
                if positional {
                    Some(Box::new(format!("provide a value for `<{kebab}>`")))
                } else {
                    Some(Box::new(format!(
                        "provide a value with `--{kebab} <{type_name}>`"
                    )))
                }
            }
            ArgsErrorKind::UnknownSubcommand { provided, variants } => {
                if variants.is_empty() {
                    return None;
                }
                // Try to find a similar subcommand
                if let Some(suggestion) = find_similar_subcommand(provided, variants) {
                    return Some(Box::new(format!("did you mean `{suggestion}`?")));
                }
                let cmds = format_available_subcommands(variants);
                Some(Box::new(format!("available subcommands:\n{cmds}")))
            }
            ArgsErrorKind::MissingSubcommand { variants } => {
                if variants.is_empty() {
                    return None;
                }
                let cmds = format_available_subcommands(variants);
                Some(Box::new(format!("available subcommands:\n{cmds}")))
            }
            ArgsErrorKind::ExpectedValueGotEof { .. } => {
                Some(Box::new("provide a value after the flag"))
            }
            ArgsErrorKind::HelpRequested { .. }
            | ArgsErrorKind::NoFields { .. }
            | ArgsErrorKind::EnumWithoutSubcommandAttribute { .. }
            | ArgsErrorKind::ReflectError(_) => None,
        }
    }

    /// Returns true if this is a help request (not a real error)
    pub fn is_help_request(&self) -> bool {
        matches!(self, ArgsErrorKind::HelpRequested { .. })
    }

    /// If this is a help request, returns the help text
    pub fn help_text(&self) -> Option<&str> {
        match self {
            ArgsErrorKind::HelpRequested { help_text } => Some(help_text),
            _ => None,
        }
    }
}

/// Format a two-column list with aligned descriptions
fn format_two_column_list(
    items: impl IntoIterator<Item = (String, Option<&'static str>)>,
) -> String {
    use core::fmt::Write;

    let items: Vec<_> = items.into_iter().collect();

    // Find max width for alignment
    let max_width = items.iter().map(|(name, _)| name.len()).max().unwrap_or(0);

    let mut lines = Vec::new();
    for (name, doc) in items {
        let mut line = String::new();
        write!(line, "  {name}").unwrap();

        // Pad to alignment
        let padding = max_width.saturating_sub(name.len());
        for _ in 0..padding {
            line.push(' ');
        }

        if let Some(doc) = doc {
            write!(line, "  {}", doc.trim()).unwrap();
        }

        lines.push(line);
    }
    lines.join("\n")
}

/// Format available flags for help text (from static field info)
fn format_available_flags(fields: &'static [Field]) -> String {
    let items = fields.iter().filter_map(|field| {
        if field.has_attr(Some("args"), "subcommand") {
            return None;
        }

        let short = get_short_flag(field);
        let positional = field.has_attr(Some("args"), "positional");
        let kebab = field.name.to_kebab_case();

        let name = if positional {
            match short {
                Some(s) => format!("-{s}, <{kebab}>"),
                None => format!("    <{kebab}>"),
            }
        } else {
            match short {
                Some(s) => format!("-{s}, --{kebab}"),
                None => format!("    --{kebab}"),
            }
        };

        Some((name, field.doc.first().copied()))
    });

    format_two_column_list(items)
}

/// Format available subcommands for help text (from static variant info)
fn format_available_subcommands(variants: &'static [Variant]) -> String {
    let items = variants.iter().map(|variant| {
        let name = variant
            .get_builtin_attr("rename")
            .and_then(|attr| attr.get_as::<&str>())
            .map(|s| (*s).to_string())
            .unwrap_or_else(|| variant.name.to_kebab_case());

        (name, variant.doc.first().copied())
    });

    format_two_column_list(items)
}

/// Get the short flag character for a field, if any
fn get_short_flag(field: &Field) -> Option<char> {
    field
        .get_attr(Some("args"), "short")
        .and_then(|attr| attr.get_as::<crate::Attr>())
        .and_then(|attr| {
            if let crate::Attr::Short(c) = attr {
                // If explicit char provided, use it; otherwise use first char of field name
                c.or_else(|| field.name.chars().next())
            } else {
                None
            }
        })
}

/// Find a similar flag name using simple heuristics
fn find_similar_flag(input: &str, fields: &'static [Field]) -> Option<String> {
    for field in fields {
        let kebab = field.name.to_kebab_case();
        if is_similar(input, &kebab) {
            return Some(kebab);
        }
    }
    None
}

/// Find a similar subcommand name using simple heuristics
fn find_similar_subcommand(input: &str, variants: &'static [Variant]) -> Option<String> {
    for variant in variants {
        // Check for rename attribute first
        let name = variant
            .get_builtin_attr("rename")
            .and_then(|attr| attr.get_as::<&str>())
            .map(|s| (*s).to_string())
            .unwrap_or_else(|| variant.name.to_kebab_case());
        if is_similar(input, &name) {
            return Some(name);
        }
    }
    None
}

/// Check if two strings are similar (differ by at most 2 edits)
fn is_similar(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    let len_diff = (a.len() as isize - b.len() as isize).abs();
    if len_diff > 2 {
        return false;
    }

    // Simple check: count character differences
    let mut diffs = 0;
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();

    for (ac, bc) in a_chars.iter().zip(b_chars.iter()) {
        if ac != bc {
            diffs += 1;
        }
    }
    diffs += len_diff as usize;
    diffs <= 2
}

/// Get the inner type identifier, unwrapping Option if present
fn unwrap_option_type(shape: &'static Shape) -> &'static str {
    match shape.def {
        facet_core::Def::Option(opt_def) => opt_def.t.type_identifier,
        _ => shape.type_identifier,
    }
}

/// Format a ReflectError into a user-friendly message
fn format_reflect_error(err: &ReflectError) -> String {
    use facet_reflect::ReflectError::*;
    match err {
        ParseFailed { shape, .. } => {
            // Use the same nice message format as OperationFailed with "Failed to parse"
            let inner_type = unwrap_option_type(shape);
            format!("invalid value for `{inner_type}`")
        }
        OperationFailed { shape, operation } => {
            // Improve common operation failure messages
            // Unwrap Option to show the inner type
            let inner_type = unwrap_option_type(shape);

            // Check for subcommand-specific error message
            if operation.starts_with("Subcommands must be provided") {
                return operation.to_string();
            }

            match *operation {
                "Type does not support parsing from string" => {
                    format!("`{inner_type}` cannot be parsed from a string value")
                }
                "Failed to parse string value" => {
                    format!("invalid value for `{inner_type}`")
                }
                _ => format!("`{inner_type}`: {operation}"),
            }
        }
        UninitializedField { shape, field_name } => {
            format!(
                "field `{}` of `{}` was not provided",
                field_name, shape.type_identifier
            )
        }
        WrongShape { expected, actual } => {
            format!(
                "expected `{}`, got `{}`",
                expected.type_identifier, actual.type_identifier
            )
        }
        _ => format!("{err}"),
    }
}

impl core::fmt::Display for ArgsErrorKind {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.label())
    }
}

impl From<ReflectError> for ArgsErrorKind {
    fn from(error: ReflectError) -> Self {
        ArgsErrorKind::ReflectError(error)
    }
}

impl ArgsError {
    /// Creates a new args error
    pub fn new(kind: ArgsErrorKind, span: Span) -> Self {
        Self { span, kind }
    }
}

impl fmt::Display for ArgsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

/// Extract variants from a shape (if it's an enum)
pub(crate) fn get_variants_from_shape(shape: &'static Shape) -> &'static [Variant] {
    if let Type::User(UserType::Enum(enum_type)) = shape.ty {
        enum_type.variants
    } else {
        &[]
    }
}
