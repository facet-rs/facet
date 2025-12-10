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
    /// Deleted/removed content (typically red)
    Deleted,
    /// Inserted/added content (typically green)
    Inserted,
    /// Moved content (typically blue)
    Moved,
    /// Unchanged content (normal/white)
    Unchanged,
    /// Structural elements like tags, brackets (typically neutral)
    Structure,
    /// Comments and type hints (muted/gray)
    Comment,
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
    pub fn new(theme: DiffTheme) -> Self {
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
        let rgb = match color {
            SemanticColor::Deleted => self.theme.deleted,
            SemanticColor::Inserted => self.theme.inserted,
            SemanticColor::Moved => self.theme.moved,
            SemanticColor::Unchanged => self.theme.unchanged,
            SemanticColor::Structure => self.theme.structure,
            SemanticColor::Comment => self.theme.comment,
        };
        write!(w, "{}", text.color(rgb))
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
