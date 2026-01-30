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

impl ReflectError {
    /// Create a new error at the given shape and path.
    pub fn new(shape: &'static Shape, path: Path, kind: ReflectErrorKind) -> Self {
        Self {
            location: ErrorLocation { shape, path },
            kind,
        }
    }

    /// Create a new error at the root (empty path).
    pub fn at_root(shape: &'static Shape, kind: ReflectErrorKind) -> Self {
        Self::new(shape, Path::default(), kind)
    }
}

/// The kind of reflection error.
#[derive(Debug)]
pub enum ReflectErrorKind {
    /// Shape mismatch during set operation.
    ShapeMismatch {
        expected: &'static Shape,
        actual: &'static Shape,
    },
    /// Tried to build an uninitialized value.
    NotInitialized,
    /// Cannot allocate unsized type.
    Unsized { shape: &'static Shape },
    /// Memory allocation failed.
    AllocFailed { layout: core::alloc::Layout },
    /// Field index out of bounds.
    FieldIndexOutOfBounds { index: u32, field_count: usize },
    /// Type is not a struct (cannot navigate into fields).
    NotAStruct,
    /// Multi-level paths are not yet supported.
    MultiLevelPathNotSupported { depth: usize },
    /// Frame is already initialized.
    AlreadyInitialized,
    /// Expected indexed children but found none.
    NotIndexedChildren,
    /// Arena double-free detected.
    DoubleFree,
    /// Arena slot is empty.
    SlotEmpty,
    /// Partial is poisoned after a previous error.
    Poisoned,
}
