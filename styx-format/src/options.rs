//! Formatting options for Styx serialization.

use std::borrow::Cow;

/// Restrict formatting to a kind.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ForceStyle {
    None,

    /// Force all objects to use comma separators
    Inline,

    /// Force all objects to use newline separators
    Multiline,
}

/// Options for Styx serialization.
#[derive(Debug, Clone)]
pub struct FormatOptions {
    /// Indentation string (default: "    " - 4 spaces)
    pub indent: Cow<'static, str>,

    /// Max line width before wrapping (default: 80)
    pub max_width: usize,

    /// Minimum available width to even consider inline (default: 30)
    /// If depth eats into max_width below this, force multi-line
    pub min_inline_width: usize,

    /// Inline objects with ≤ N entries (default: 4)
    pub inline_object_threshold: usize,

    /// Inline sequences with ≤ N items (default: 8)
    pub inline_sequence_threshold: usize,

    /// Use heredocs for strings with > N lines (default: 2)
    pub heredoc_line_threshold: usize,

    pub force_style: ForceStyle,
    /// Whether pretty printing is enabled
    pub pretty_printing: bool,

    /// When serializing a Facet value, omit struct fields whose value is `None`
    /// (an absent `Option`) instead of writing them as the unit key `@`. Off by
    /// default; opt in for clean, round-trippable output. Only affects value
    /// serialization, not text reformatting.
    pub omit_none: bool,
}

impl Default for FormatOptions {
    fn default() -> Self {
        Self {
            indent: Cow::Borrowed("    "),
            max_width: 80,
            min_inline_width: 30,
            inline_object_threshold: 4,
            inline_sequence_threshold: 8,
            heredoc_line_threshold: 2,
            force_style: ForceStyle::None,
            pretty_printing: false,
            omit_none: false,
        }
    }
}

impl FormatOptions {
    /// Create new default options.
    pub fn new() -> Self {
        Self::default()
    }

    /// Force all output to be multi-line (newline separators).
    pub fn multiline(mut self) -> Self {
        self.force_style = ForceStyle::Multiline;
        self
    }

    /// Force all output to be inline (comma separators, single line).
    pub fn inline(mut self) -> Self {
        self.force_style = ForceStyle::Inline;
        self
    }

    /// Omit `None` (absent `Option`) struct fields during value serialization.
    pub fn omit_none(mut self) -> Self {
        self.omit_none = true;
        self
    }

    /// Set a custom indentation string.
    pub fn indent(mut self, indent: impl Into<Cow<'static, str>>) -> Self {
        self.indent = indent.into();
        self
    }

    /// Set max line width.
    pub fn max_width(mut self, width: usize) -> Self {
        self.max_width = width;
        self
    }

    /// Enable pretty printing with optional line length override.
    pub fn pretty(mut self, line_length: usize) -> Self {
        self.max_width = line_length;
        self.pretty_printing = true;
        self
    }

    /// Check if pretty printing is enabled.
    pub fn pretty_printing_enabled(&self) -> bool {
        self.pretty_printing
    }
}
