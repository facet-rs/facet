//! Standard path representation for navigating schemas and ConfigValue trees.
//!
//! This is intentionally small and stable: a `Path` is a thin wrapper over
//! `Vec<String>`, where each segment is a name. Indices are stringified numbers.
//!
//! Use this for diagnostics and for navigation helpers in `schema` and `config_value`.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

/// A path into a schema or ConfigValue tree.
pub type Path = Vec<String>;

/// Convenience helpers for Path.
pub trait PathExt {
    /// Create an empty root path.
    fn root() -> Self;

    /// Push a raw segment.
    fn push_segment(&mut self, segment: impl Into<String>);

    /// Push a field segment.
    fn push_field(&mut self, name: impl Into<String>);

    /// Push a variant segment.
    fn push_variant(&mut self, name: impl Into<String>);

    /// Push an index segment (stringified number).
    fn push_index(&mut self, index: usize);

    /// Push a key segment.
    fn push_key(&mut self, key: impl Into<String>);
}

impl PathExt for Path {
    fn root() -> Self {
        Vec::new()
    }

    fn push_segment(&mut self, segment: impl Into<String>) {
        self.push(segment.into());
    }

    fn push_field(&mut self, name: impl Into<String>) {
        self.push_segment(name);
    }

    fn push_variant(&mut self, name: impl Into<String>) {
        self.push_segment(name);
    }

    fn push_index(&mut self, index: usize) {
        self.push_segment(index.to_string());
    }

    fn push_key(&mut self, key: impl Into<String>) {
        self.push_segment(key);
    }
}

/// Display wrapper for a Path without relying on orphan impls.
pub struct PathDisplay<'a>(pub &'a Path);

impl fmt::Display for PathDisplay<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;
        for seg in self.0 {
            if !first {
                write!(f, ".")?;
            }
            write!(f, "{seg}")?;
            first = false;
        }
        Ok(())
    }
}

/// Convenience helper for formatting paths.
pub fn display_path(path: &Path) -> PathDisplay<'_> {
    PathDisplay(path)
}
