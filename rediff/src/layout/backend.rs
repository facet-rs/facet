//! Color backends for diff rendering.
//!
//! This module provides an abstraction for how semantic colors are rendered.
//! The render code only knows about semantic meanings (deleted, inserted, etc.),
//! and the backend decides how to actually style the text.

use std::fmt::Write;

use owo_colors::OwoColorize;

use crate::DiffTheme;

/// Semantic color meaning for diff elements.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticColor {
    // === Accent colors (full brightness) ===
    /// Deleted content on line background
    Deleted,
    /// Deleted content on highlight background (the actual changed value)
    DeletedHighlight,
    /// Inserted content on line background
    Inserted,
    /// Inserted content on highlight background (the actual changed value)
    InsertedHighlight,
    /// Moved content on line background
    Moved,
    /// Moved content on highlight background
    MovedHighlight,

    // === Syntax colors (context-aware) ===
    /// Key/field name in deleted context (blended toward orange)
    DeletedKey,
    /// Key/field name in inserted context (blended toward blue)
    InsertedKey,
    /// Key/field name in unchanged context
    Key,

    /// Structural element in deleted context
    DeletedStructure,
    /// Structural element in inserted context
    InsertedStructure,
    /// Structural element in unchanged context
    Structure,

    /// Comment/type hint in deleted context
    DeletedComment,
    /// Comment/type hint in inserted context
    InsertedComment,
    /// Comment in unchanged context
    Comment,

    // === Value type colors (context-aware) ===
    /// String value in deleted context (blended)
    DeletedString,
    /// String value in inserted context (blended)
    InsertedString,
    /// String value in unchanged context
    String,

    /// Number value in deleted context (blended)
    DeletedNumber,
    /// Number value in inserted context (blended)
    InsertedNumber,
    /// Number value in unchanged context
    Number,

    /// Boolean value in deleted context (blended)
    DeletedBoolean,
    /// Boolean value in inserted context (blended)
    InsertedBoolean,
    /// Boolean value in unchanged context
    Boolean,

    /// Null/None value in deleted context (blended)
    DeletedNull,
    /// Null/None value in inserted context (blended)
    InsertedNull,
    /// Null/None value in unchanged context
    Null,

    // === Whitespace and separators ===
    /// Whitespace, commas, and other separator characters (no highlight background)
    Whitespace,

    // === Other ===
    /// Unchanged content (neutral)
    Unchanged,
}

/// A backend that decides how to render semantic colors.
pub trait ColorBackend {
    /// Write styled text to the output.
    fn write_styled<W: Write>(
        &self,
        w: &mut W,
        text: &str,
        color: SemanticColor,
    ) -> std::fmt::Result;

    /// Write a diff prefix (-/+/←/→) with appropriate styling.
    fn write_prefix<W: Write>(
        &self,
        w: &mut W,
        prefix: char,
        color: SemanticColor,
    ) -> std::fmt::Result {
        self.write_styled(w, &prefix.to_string(), color)
    }
}

/// Plain backend - no styling, just plain text.
///
/// Use this for tests and non-terminal output.
#[derive(Debug, Clone, Copy, Default)]
pub struct PlainBackend;

impl ColorBackend for PlainBackend {
    fn write_styled<W: Write>(
        &self,
        w: &mut W,
        text: &str,
        _color: SemanticColor,
    ) -> std::fmt::Result {
        write!(w, "{}", text)
    }
}

/// ANSI backend - emits ANSI escape codes for terminal colors.
///
/// Use this for terminal output with a color theme.
#[derive(Debug, Clone)]
pub struct AnsiBackend {
    theme: DiffTheme,
}

impl AnsiBackend {
    /// Create a new ANSI backend with the given theme.
    pub const fn new(theme: DiffTheme) -> Self {
        Self { theme }
    }

    /// Create a new ANSI backend with the default (One Dark Pro) theme.
    pub fn with_default_theme() -> Self {
        Self::new(DiffTheme::default())
    }
}

impl Default for AnsiBackend {
    fn default() -> Self {
        Self::with_default_theme()
    }
}

impl ColorBackend for AnsiBackend {
    fn write_styled<W: Write>(
        &self,
        w: &mut W,
        text: &str,
        color: SemanticColor,
    ) -> std::fmt::Result {
        let (fg, bg) = match color {
            // Accent colors
            SemanticColor::Deleted => {
                (self.theme.deleted, self.theme.desaturated_deleted_line_bg())
            }
            SemanticColor::DeletedHighlight => (
                self.theme.deleted,
                self.theme.desaturated_deleted_highlight_bg(),
            ),
            SemanticColor::Inserted => (
                self.theme.inserted,
                self.theme.desaturated_inserted_line_bg(),
            ),
            SemanticColor::InsertedHighlight => (
                self.theme.inserted,
                self.theme.desaturated_inserted_highlight_bg(),
            ),
            SemanticColor::Moved => (self.theme.moved, self.theme.desaturated_moved_line_bg()),
            SemanticColor::MovedHighlight => (
                self.theme.moved,
                self.theme.desaturated_moved_highlight_bg(),
            ),

            // Context-aware syntax colors
            SemanticColor::DeletedKey => (
                self.theme.deleted_highlight_key(),
                self.theme.desaturated_deleted_highlight_bg(),
            ),
            SemanticColor::InsertedKey => (
                self.theme.inserted_highlight_key(),
                self.theme.desaturated_inserted_highlight_bg(),
            ),
            SemanticColor::Key => (self.theme.key, None),

            SemanticColor::DeletedStructure => (
                self.theme.deleted_structure(),
                self.theme.desaturated_deleted_line_bg(),
            ),
            SemanticColor::InsertedStructure => (
                self.theme.inserted_structure(),
                self.theme.desaturated_inserted_line_bg(),
            ),
            SemanticColor::Structure => (self.theme.structure, None),

            SemanticColor::DeletedComment => (
                self.theme.deleted_highlight_comment(),
                self.theme.desaturated_deleted_highlight_bg(),
            ),
            SemanticColor::InsertedComment => (
                self.theme.inserted_highlight_comment(),
                self.theme.desaturated_inserted_highlight_bg(),
            ),
            SemanticColor::Comment => (self.theme.comment, None),

            // Context-aware value type colors
            SemanticColor::DeletedString => (
                self.theme.deleted_highlight_string(),
                self.theme.desaturated_deleted_highlight_bg(),
            ),
            SemanticColor::InsertedString => (
                self.theme.inserted_highlight_string(),
                self.theme.desaturated_inserted_highlight_bg(),
            ),
            SemanticColor::String => (self.theme.string, None),

            SemanticColor::DeletedNumber => (
                self.theme.deleted_highlight_number(),
                self.theme.desaturated_deleted_highlight_bg(),
            ),
            SemanticColor::InsertedNumber => (
                self.theme.inserted_highlight_number(),
                self.theme.desaturated_inserted_highlight_bg(),
            ),
            SemanticColor::Number => (self.theme.number, None),

            SemanticColor::DeletedBoolean => (
                self.theme.deleted_highlight_boolean(),
                self.theme.desaturated_deleted_highlight_bg(),
            ),
            SemanticColor::InsertedBoolean => (
                self.theme.inserted_highlight_boolean(),
                self.theme.desaturated_inserted_highlight_bg(),
            ),
            SemanticColor::Boolean => (self.theme.boolean, None),

            SemanticColor::DeletedNull => (
                self.theme.deleted_highlight_null(),
                self.theme.desaturated_deleted_highlight_bg(),
            ),
            SemanticColor::InsertedNull => (
                self.theme.inserted_highlight_null(),
                self.theme.desaturated_inserted_highlight_bg(),
            ),
            SemanticColor::Null => (self.theme.null, None),

            // Whitespace and separators (no background, use comment color which is muted)
            SemanticColor::Whitespace => (self.theme.comment, None),

            // Neutral
            SemanticColor::Unchanged => (self.theme.unchanged, None),
        };
        if let Some(bg) = bg {
            write!(w, "{}", text.color(fg).on_color(bg))
        } else {
            write!(w, "{}", text.color(fg))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plain_backend() {
        let backend = PlainBackend;
        let mut out = String::new();

        backend
            .write_styled(&mut out, "hello", SemanticColor::Deleted)
            .unwrap();
        assert_eq!(out, "hello");

        out.clear();
        backend
            .write_styled(&mut out, "world", SemanticColor::Inserted)
            .unwrap();
        assert_eq!(out, "world");
    }

    #[test]
    fn test_ansi_backend() {
        let backend = AnsiBackend::default();
        let mut out = String::new();

        backend
            .write_styled(&mut out, "deleted", SemanticColor::Deleted)
            .unwrap();
        // Should contain ANSI escape codes
        assert!(out.contains("\x1b["));
        assert!(out.contains("deleted"));
    }

    #[test]
    fn test_prefix() {
        let backend = PlainBackend;
        let mut out = String::new();

        backend
            .write_prefix(&mut out, '-', SemanticColor::Deleted)
            .unwrap();
        assert_eq!(out, "-");
    }
}
