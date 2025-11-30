use crate::PtrConst;
#[cfg(feature = "alloc")]
use crate::{PtrMut, PtrUninit};

use super::{DefaultInPlaceFn, InvariantsFn, Shape};

/// Describes a field in a struct or tuple
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct Field {
    /// key for the struct field (for tuples and tuple-structs, this is the 0-based index)
    pub name: &'static str,

    /// shape of the inner type
    ///
    /// the layer of indirection allows for cyclic type definitions
    pub shape: fn() -> &'static Shape,

    /// offset of the field in the struct (obtained through `core::mem::offset_of`)
    pub offset: usize,

    /// arbitrary attributes set via the derive macro
    pub attributes: &'static [FieldAttribute],

    /// doc comments
    pub doc: &'static [&'static str],
}

impl Field {
    /// Returns true if the field should be skipped during serialization.
    ///
    /// This checks for `#[facet(skip)]` or `#[facet(skip_serializing)]` attributes,
    /// or if `skip_serializing_if` function returns true.
    ///
    /// # Safety
    /// The ptr should correspond to a value of the same type as this field
    pub unsafe fn should_skip_serializing(&self, ptr: PtrConst<'_>) -> bool {
        if self.has_builtin_attr("skip") || self.has_builtin_attr("skip_serializing") {
            return true;
        }
        if let Some(skip_serializing_if) = self.skip_serializing_if_fn() {
            return unsafe { skip_serializing_if(ptr) };
        }
        false
    }

    /// Returns true if this field should be skipped during deserialization.
    ///
    /// This checks for `#[facet(skip)]` or `#[facet(skip_deserializing)]` attributes.
    #[inline]
    pub fn should_skip_deserializing(&self) -> bool {
        self.has_builtin_attr("skip") || self.has_builtin_attr("skip_deserializing")
    }

    /// Returns true if this field is flattened.
    ///
    /// This checks for `#[facet(flatten)]` attribute.
    #[inline]
    pub fn is_flattened(&self) -> bool {
        self.has_builtin_attr("flatten")
    }

    /// Returns true if this field is marked as sensitive.
    ///
    /// This checks for `#[facet(sensitive)]` attribute.
    #[inline]
    pub fn is_sensitive(&self) -> bool {
        self.has_builtin_attr("sensitive")
    }

    /// Returns true if this field has a default value.
    ///
    /// This checks for `#[facet(default)]` or `#[facet(default = expr)]` attributes.
    #[inline]
    pub fn has_default(&self) -> bool {
        self.has_builtin_attr("default")
    }

    /// Returns true if this field is a child (for KDL/XML formats).
    ///
    /// This checks for `#[facet(child)]` attribute.
    #[inline]
    pub fn is_child(&self) -> bool {
        self.has_builtin_attr("child")
    }
}

/// A function that, if present, determines whether field should be included in the serialization
/// step. Takes a type-erased pointer and returns true if the field should be skipped.
pub type SkipSerializingIfFn = for<'mem> unsafe fn(value: PtrConst<'mem>) -> bool;

#[cfg(feature = "alloc")]
/// Function type for proxy deserialization: converts FROM proxy type INTO field type.
/// Used internally when `#[facet(proxy = Type)]` is specified on a field.
pub type ProxyConvertInFn = for<'mem> unsafe fn(
    proxy_ptr: PtrConst<'mem>,
    field_ptr: PtrUninit<'mem>,
) -> Result<PtrMut<'mem>, alloc::string::String>;

#[cfg(feature = "alloc")]
/// Function type for proxy serialization: converts FROM field type OUT TO proxy type.
/// Used internally when `#[facet(proxy = Type)]` is specified on a field.
pub type ProxyConvertOutFn = for<'mem> unsafe fn(
    field_ptr: PtrConst<'mem>,
    proxy_ptr: PtrUninit<'mem>,
) -> Result<PtrMut<'mem>, alloc::string::String>;

impl Field {
    /// Returns the shape of the inner type
    pub fn shape(&self) -> &'static Shape {
        (self.shape)()
    }

    /// Returns a builder for Field
    pub const fn builder() -> FieldBuilder {
        FieldBuilder::new()
    }

    /// Checks whether the `Field` has an attribute with the given namespace and key.
    ///
    /// Use `None` for builtin attributes, `Some("ns")` for namespaced attributes.
    #[inline]
    pub fn has_attr(&self, ns: Option<&str>, key: &str) -> bool {
        self.attributes
            .iter()
            .any(|attr| attr.ns == ns && attr.key == key)
    }

    /// Gets an attribute by namespace and key.
    ///
    /// Use `None` for builtin attributes, `Some("ns")` for namespaced attributes.
    #[inline]
    pub fn get_attr(&self, ns: Option<&str>, key: &str) -> Option<&super::ExtensionAttr> {
        self.attributes
            .iter()
            .find(|attr| attr.ns == ns && attr.key == key)
    }

    /// Checks whether the `Field` has a builtin attribute with the given key.
    #[inline]
    pub fn has_builtin_attr(&self, key: &str) -> bool {
        self.has_attr(None, key)
    }

    /// Gets a builtin attribute by key.
    #[inline]
    pub fn get_builtin_attr(&self, key: &str) -> Option<&super::ExtensionAttr> {
        self.get_attr(None, key)
    }

    /// Gets the proxy shape stored in the `proxy` attribute, if present.
    ///
    /// This is set when `#[facet(proxy = ProxyType)]` is used. The proxy type
    /// is used for both serialization and deserialization. The user must implement:
    /// - `TryFrom<ProxyType> for FieldType` (for deserialization)
    /// - `TryFrom<&FieldType> for ProxyType` (for serialization)
    #[inline]
    pub fn proxy_shape(&self) -> Option<&'static super::Shape> {
        // Note: shape_type variants store the Shape directly (not wrapped in Attr enum)
        // so we read it as Shape, not &'static Shape
        self.get_builtin_attr("proxy")
            .map(|attr| unsafe { attr.data_ref::<super::Shape>() })
    }

    /// Gets the `skip_serializing_if` function pointer from attributes, if present.
    ///
    /// This is set when `#[facet(skip_serializing_if = fn)]` is used.
    #[inline]
    pub fn skip_serializing_if_fn(&self) -> Option<SkipSerializingIfFn> {
        self.get_builtin_attr("skip_serializing_if")
            .map(|attr| unsafe { *attr.data_ref::<SkipSerializingIfFn>() })
    }

    /// Gets the `default` function pointer from attributes, if present.
    ///
    /// This is set when `#[facet(default = expr)]` is used with a custom expression.
    /// Returns `None` if:
    /// - No `#[facet(default)]` attribute is present, OR
    /// - `#[facet(default)]` is present without an expression (uses Default trait instead)
    #[inline]
    pub fn default_fn(&self) -> Option<DefaultInPlaceFn> {
        self.get_builtin_attr("default")
            .and_then(|attr| unsafe { *attr.data_ref::<Option<DefaultInPlaceFn>>() })
    }

    /// Gets the `invariants` function pointer from attributes, if present.
    ///
    /// This is set when `#[facet(invariants = validate_fn)]` is used.
    #[inline]
    pub fn invariants_fn(&self) -> Option<InvariantsFn> {
        self.get_builtin_attr("invariants")
            .map(|attr| unsafe { *attr.data_ref::<InvariantsFn>() })
    }

    /// Gets the proxy-to-field conversion function, if this field has a proxy attribute.
    ///
    /// This is generated by the derive macro when `#[facet(proxy = Type)]` is used.
    /// The function converts from the proxy type to the field type via TryFrom.
    #[cfg(feature = "alloc")]
    #[inline]
    pub fn proxy_convert_in_fn(&self) -> Option<ProxyConvertInFn> {
        self.get_builtin_attr("__proxy_in")
            .map(|attr| unsafe { *attr.data_ref::<ProxyConvertInFn>() })
    }

    /// Gets the field-to-proxy conversion function, if this field has a proxy attribute.
    ///
    /// This is generated by the derive macro when `#[facet(proxy = Type)]` is used.
    /// The function converts from the field type to the proxy type via TryFrom.
    #[cfg(feature = "alloc")]
    #[inline]
    pub fn proxy_convert_out_fn(&self) -> Option<ProxyConvertOutFn> {
        self.get_builtin_attr("__proxy_out")
            .map(|attr| unsafe { *attr.data_ref::<ProxyConvertOutFn>() })
    }
}

/// An attribute that can be set on a field.
/// This is now just an alias for `ExtensionAttr` - all attributes use the same representation.
pub type FieldAttribute = super::ExtensionAttr;

/// Builder for Field
pub struct FieldBuilder {
    name: Option<&'static str>,
    shape: Option<fn() -> &'static Shape>,
    offset: Option<usize>,
    attributes: &'static [FieldAttribute],
    doc: &'static [&'static str],
}

impl FieldBuilder {
    /// Creates a new FieldBuilder
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {
            name: None,
            shape: None,
            offset: None,
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
            attributes: self.attributes,
            doc: self.doc,
        }
    }
}

/// Errors encountered when calling `field_by_index` or `field_by_name`
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldError {
    /// `field_by_name` was called on a struct, and there is no static field
    /// with the given key.
    NoSuchField,

    /// `field_by_index` was called on a fixed-size collection (like a tuple,
    /// a struct, or a fixed-size array) and the index was out of bounds.
    IndexOutOfBounds {
        /// the index we asked for
        index: usize,

        /// the upper bound of the index
        bound: usize,
    },

    /// `set` or `set_by_name` was called with an mismatched type
    TypeMismatch {
        /// the actual type of the field
        expected: &'static Shape,

        /// what someone tried to write into it / read from it
        actual: &'static Shape,
    },

    /// The type is unsized
    Unsized,
}

impl core::error::Error for FieldError {}

impl core::fmt::Display for FieldError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            FieldError::NoSuchField => write!(f, "no such field"),
            FieldError::IndexOutOfBounds { index, bound } => {
                write!(f, "tried to access field {index} of {bound}")
            }
            FieldError::TypeMismatch { expected, actual } => {
                write!(f, "expected type {expected}, got {actual}")
            }
            FieldError::Unsized => {
                write!(f, "can't access field of !Sized type")
            }
        }
    }
}

macro_rules! field_in_type {
    ($container:ty, $field:tt) => {
        $crate::Field::builder()
            .name(stringify!($field))
            .shape(|| $crate::shape_of(&|t: &Self| &t.$field))
            .offset(::core::mem::offset_of!(Self, $field))
            .build()
    };
}

pub(crate) use field_in_type;
