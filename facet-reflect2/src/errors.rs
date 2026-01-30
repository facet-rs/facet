//! Error types for partial value construction.

use crate::ops::Path;
use facet_core::Shape;

/// Location where an error occurred.
pub struct ErrorLocation {
    pub shape: &'static Shape,
    pub path: Path,
}

/// An error during reflection.
pub struct ReflectError {
    pub location: ErrorLocation,
    pub kind: ReflectErrorKind,
}

/// The kind of reflection error.
pub enum ReflectErrorKind {
    // TODO
}
