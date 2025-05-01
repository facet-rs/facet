use crate::PtrConst;

use super::{DefaultInPlaceFn, Shape};
use bitflags::bitflags;

/// Describes a field in a struct or tuple
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
#[non_exhaustive]
pub struct Field {
    /// key for the struct field (for tuples and tuple-structs, this is the 0-based index)
    pub name: &'static str,

    /// shape of the inner type
    pub shape: fn() -> &'static Shape,

    /// offset of the field in the struct (obtained through `core::mem::offset_of`)
    pub offset: usize,

    /// flags for the field (e.g. sensitive, etc.)
    pub flags: FieldFlags,

    /// arbitrary attributes set via the derive macro
    pub attributes: &'static [FieldAttribute],

    /// doc comments
    pub doc: &'static [&'static str],

    /// vtable for fields
    pub vtable: &'static FieldVTable,

    /// true if returned from `fields_for_serialize` and it was flattened - which
    /// means, if it's an enum, the outer variant shouldn't be written.
    pub flattened: bool,
}

impl Field {
    /// Returns true if the field has the skip-serializing unconditionally flag or if it has the
    /// skip-serializing-if function in its vtable and it returns true on the given data.
    ///
    /// # Safety
    /// The peek should correspond to a value of the same type as this field
    pub unsafe fn should_skip_serializing(&self, ptr: PtrConst<'_>) -> bool {
        if self.flags.contains(FieldFlags::SKIP_SERIALIZING) {
            return true;
        }
        if let Some(skip_serializing_if) = self.vtable.skip_serializing_if {
            return unsafe { skip_serializing_if(ptr) };
        }
        false
    }
}

/// Vtable for field-specific operations
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
#[non_exhaustive]
pub struct FieldVTable {
    /// Function to determine if serialization should be skipped for this field
    pub skip_serializing_if: Option<SkipSerializingIfFn>,

    /// Function to get the default value for this field
    pub default_fn: Option<DefaultInPlaceFn>,
}

/// A function that, if present, determines whether field should be included in the serialization
/// step.
pub type SkipSerializingIfFn = for<'mem> unsafe fn(value: PtrConst<'mem>) -> bool;

impl Field {
    /// Returns the shape of the inner type
    pub fn shape(&self) -> &'static Shape {
        (self.shape)()
    }

    /// Returns a builder for Field
    pub const fn builder() -> FieldBuilder {
        FieldBuilder::new()
    }

    /// Checks if field is marked as sensitive through attributes or flags
    pub fn is_sensitive(&'static self) -> bool {
        self.flags.contains(FieldFlags::SENSITIVE)
    }
}

/// An attribute that can be set on a field
#[non_exhaustive]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
pub enum FieldAttribute {
    /// Custom field attribute containing arbitrary text
    Arbitrary(&'static str),
}

/// Builder for FieldVTable
pub struct FieldVTableBuilder {
    skip_serializing_if: Option<SkipSerializingIfFn>,
    default_fn: Option<DefaultInPlaceFn>,
}

impl FieldVTableBuilder {
    /// Creates a new FieldVTableBuilder
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {
            skip_serializing_if: None,
            default_fn: None,
        }
    }

    /// Sets the skip_serializing_if function for the FieldVTable
    pub const fn skip_serializing_if(mut self, func: SkipSerializingIfFn) -> Self {
        self.skip_serializing_if = Some(func);
        self
    }

    /// Sets the default_fn function for the FieldVTable
    pub const fn default_fn(mut self, func: DefaultInPlaceFn) -> Self {
        self.default_fn = Some(func);
        self
    }

    /// Builds the FieldVTable
    pub const fn build(self) -> FieldVTable {
        FieldVTable {
            skip_serializing_if: self.skip_serializing_if,
            default_fn: self.default_fn,
        }
    }
}

impl FieldVTable {
    /// Returns a builder for FieldVTable
    pub const fn builder() -> FieldVTableBuilder {
        FieldVTableBuilder::new()
    }
}

/// Builder for Field
pub struct FieldBuilder {
    name: Option<&'static str>,
    shape: Option<fn() -> &'static Shape>,
    offset: Option<usize>,
    flags: Option<FieldFlags>,
    attributes: &'static [FieldAttribute],
    doc: &'static [&'static str],
    vtable: &'static FieldVTable,
}

impl FieldBuilder {
    /// Creates a new FieldBuilder
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {
            name: None,
            shape: None,
            offset: None,
            flags: None,
            attributes: &[],
            doc: &[],
            vtable: &const {
                FieldVTable {
                    skip_serializing_if: None,
                    default_fn: None,
                }
            },
        }
    }

    /// Sets the name for the Field
    pub const fn name(mut self, name: &'static str) -> Self {
        self.name = Some(name);
        self
    }

    /// Sets the shape for the Field
    pub const fn shape(mut self, shape: fn() -> &'static Shape) -> Self {
        self.shape = Some(shape);
        self
    }

    /// Sets the offset for the Field
    pub const fn offset(mut self, offset: usize) -> Self {
        self.offset = Some(offset);
        self
    }

    /// Sets the flags for the Field
    pub const fn flags(mut self, flags: FieldFlags) -> Self {
        self.flags = Some(flags);
        self
    }

    /// Sets the attributes for the Field
    pub const fn attributes(mut self, attributes: &'static [FieldAttribute]) -> Self {
        self.attributes = attributes;
        self
    }

    /// Sets the doc comments for the Field
    pub const fn doc(mut self, doc: &'static [&'static str]) -> Self {
        self.doc = doc;
        self
    }

    /// Sets the vtable for the Field
    pub const fn vtable(mut self, vtable: &'static FieldVTable) -> Self {
        self.vtable = vtable;
        self
    }

    /// Builds the Field
    pub const fn build(self) -> Field {
        Field {
            name: self.name.unwrap(),
            shape: self.shape.unwrap(),
            offset: self.offset.unwrap(),
            flags: match self.flags {
                Some(flags) => flags,
                None => FieldFlags::EMPTY,
            },
            attributes: self.attributes,
            doc: self.doc,
            vtable: self.vtable,
            flattened: false,
        }
    }
}

bitflags! {
    /// Flags that can be applied to fields to modify their behavior
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct FieldFlags: u64 {
        /// An empty set of flags
        const EMPTY = 0;

        /// Flag indicating this field contains sensitive data that should not be displayed
        const SENSITIVE = 1 << 0;

        /// Flag indicating this field should be skipped during serialization
        const SKIP_SERIALIZING = 1 << 1;

        /// Flag indicating that this field should be flattened: if it's a struct, all its
        /// fields should be apparent on the parent structure, etc.
        const FLATTEN = 1 << 2;

        /// For KDL/XML formats, indicates that this field is a child, not an attribute
        const CHILD = 1 << 3;

        /// When deserializing, if this field is missing, use its default value. If
        /// `FieldVTable::default_fn` is set, use that.
        const DEFAULT = 1 << 4;
    }
}

impl Default for FieldFlags {
    #[inline(always)]
    fn default() -> Self {
        Self::EMPTY
    }
}

impl core::fmt::Display for FieldFlags {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.is_empty() {
            return write!(f, "none");
        }

        // Define a vector of flag entries: (flag, name)
        let flags = [
            (FieldFlags::SENSITIVE, "sensitive"),
            // Future flags can be easily added here:
            // (FieldFlags::SOME_FLAG, "some_flag"),
            // (FieldFlags::ANOTHER_FLAG, "another_flag"),
        ];

        // Write all active flags with proper separators
        let mut is_first = true;
        for (flag, name) in flags {
            if self.contains(flag) {
                if !is_first {
                    write!(f, ", ")?;
                }
                is_first = false;
                write!(f, "{}", name)?;
            }
        }

        Ok(())
    }
}

/// Errors encountered when calling `field_by_index` or `field_by_name`
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum FieldError {
    /// `field_by_name` was called on a struct, and there is no static field
    /// with the given key.
    NoSuchField,

    /// `field_by_index` was called on a fixed-size collection (like a tuple,
    /// a struct, or a fixed-size array) and the index was out of bounds.
    IndexOutOfBounds,

    /// `set` or `set_by_name` was called with an mismatched type
    TypeMismatch {
        /// the actual type of the field
        expected: &'static Shape,

        /// what someone tried to write into it / read from it
        actual: &'static Shape,
    },
}

impl core::error::Error for FieldError {}

impl core::fmt::Display for FieldError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            FieldError::NoSuchField => write!(f, "No such static field"),
            FieldError::IndexOutOfBounds => write!(f, "Index out of bounds"),
            FieldError::TypeMismatch { expected, actual } => {
                write!(f, "Type mismatch: expected {}, got {}", expected, actual)
            }
        }
    }
}
