//! Error types for partial value construction.

use crate::ops::Path;
use facet_core::Shape;

/// Location where an error occurred.
#[derive(Debug)]
pub struct ErrorLocation {
    pub shape: &'static Shape,
    pub path: Path,
}

/// An error during reflection.
#[derive(Debug)]
pub struct ReflectError {
    pub location: ErrorLocation,
    pub kind: ReflectErrorKind,
}

/// The kind of reflection error.
#[derive(Debug)]
pub enum ReflectErrorKind {
    // TODO
}
