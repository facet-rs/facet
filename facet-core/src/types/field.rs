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
}

impl Field {
    /// Returns the shape of the inner type
    pub fn shape(&self) -> &'static Shape {
        (self.shape)()
    }

    /// Returns a builder for Field
    pub const fn builder() -> FieldBuilder {
        FieldBuilder::new()
    }

    /// See [`FieldAttribute::Sensitive`]
    pub fn has_sensitive_attr(&'static self) -> bool {
        self.attributes.contains(&FieldAttribute::Sensitive)
    }

    /// See [`FieldAttribute::Rename`]
    pub fn get_rename_attr(&'static self) -> Option<&'static str> {
        for attr in self.attributes {
            if let FieldAttribute::Rename(name) = attr {
                return Some(name);
            }
        }
        None
    }

    /// See [`FieldAttribute::Default`]
    pub fn get_default_attr(&'static self) -> Option<Option<DefaultInPlaceFn>> {
        for attr in self.attributes {
            if let FieldAttribute::Default(default_fn) = attr {
                return Some(*default_fn);
            }
        }
        None
    }

    /// Checks if the field has a default value
    pub fn has_default_attr(&'static self) -> bool {
        self.attributes
            .iter()
            .any(|attr| matches!(attr, FieldAttribute::Default(_)))
    }

    /// See [`FieldAttribute::Arbitrary`]
    pub fn get_arbitrary_attr(&'static self) -> Option<&'static str> {
        for attr in self.attributes {
            if let FieldAttribute::Arbitrary(value) = attr {
                return Some(value);
            }
        }
        None
    }

    /// Checks if field is marked as sensitive through attributes or flags
    pub fn is_sensitive(&'static self) -> bool {
        self.has_sensitive_attr() || self.flags.contains(FieldFlags::SENSITIVE)
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
}

/// An attribute that can be set on a field
#[non_exhaustive]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
pub enum FieldAttribute {
    /// Marks field as containing sensitive information
    Sensitive,
    /// Skip serializing this field.
    SkipSerializing,
    /// Skip serializing this field if the function identified by `.0` evaluates to true.
    SkipSerializingIf(SkipSerializingIfFn),
    /// Specifies an alternative name for the field (for serialization/deserialization)
    Rename(&'static str),
    /// Indicates the field has a default value (the value is which fn to call for default, or None for Default::default)
    Default(Option<DefaultInPlaceFn>),
    /// Custom field attribute containing arbitrary text
    Arbitrary(&'static str),
}

/// A function that, if present, determines whether field should be included in the serialization
/// step.
pub type SkipSerializingIfFn = for<'mem> unsafe fn(value: PtrConst<'mem>) -> bool;

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
