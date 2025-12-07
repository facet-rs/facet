use crate::PtrConst;
#[cfg(feature = "alloc")]
use crate::{PtrMut, PtrUninit};

use super::{DefaultInPlaceFn, InvariantsFn, Shape};

/// A reference to a [`Shape`], either direct or lazy.
///
/// Most fields use [`ShapeRef::Static`] for direct `&'static Shape` references,
/// which is more efficient (no function call overhead).
///
/// For recursive types (e.g., a struct containing `Vec<Self>`), use
/// [`ShapeRef::Lazy`] with a closure to break the cycle. Mark such fields
/// with `#[facet(recursive_type)]` in the derive macro.
#[derive(Clone, Copy)]
pub enum ShapeRef {
    /// Direct reference to a shape (default, most efficient)
    Static(&'static Shape),
    /// Lazy reference via closure (for recursive types)
    Lazy(fn() -> &'static Shape),
}

impl ShapeRef {
    /// Get the referenced shape.
    ///
    /// For [`ShapeRef::Static`], returns the reference directly.
    /// For [`ShapeRef::Lazy`], calls the closure to get the shape.
    #[inline]
    pub fn get(&self) -> &'static Shape {
        match self {
            ShapeRef::Static(shape) => shape,
            ShapeRef::Lazy(f) => f(),
        }
    }
}

impl core::fmt::Debug for ShapeRef {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Just debug the shape itself, not the wrapper
        write!(f, "{:?}", self.get())
    }
}

crate::bitflags! {
    /// Bit flags for common field attributes.
    ///
    /// These provide O(1) access to frequently-checked boolean attributes,
    /// avoiding the O(n) linear scan through the attributes slice.
    pub struct FieldFlags: u16 {
        /// Field contains sensitive data (redacted in debug output).
        /// Set by `#[facet(sensitive)]`.
        const SENSITIVE = 1 << 0;

        /// Field is flattened into its parent structure.
        /// Set by `#[facet(flatten)]`.
        const FLATTEN = 1 << 1;

        /// Field is skipped during both serialization and deserialization.
        /// Set by `#[facet(skip)]`.
        const SKIP = 1 << 2;

        /// Field is skipped during serialization only.
        /// Set by `#[facet(skip_serializing)]`.
        const SKIP_SERIALIZING = 1 << 3;

        /// Field is skipped during deserialization only.
        /// Set by `#[facet(skip_deserializing)]`.
        const SKIP_DESERIALIZING = 1 << 4;

        /// Field is a child node (for hierarchical formats like KDL/XML).
        /// Set by `#[facet(child)]`.
        const CHILD = 1 << 5;

        /// Field has a recursive type that needs lazy shape resolution.
        /// Set by `#[facet(recursive_type)]`.
        const RECURSIVE_TYPE = 1 << 6;

        /// Field has a default value (either via Default trait or custom expression).
        /// Set by `#[facet(default)]` or `#[facet(default = expr)]`.
        const HAS_DEFAULT = 1 << 7;
    }
}

/// Describes a field in a struct or tuple
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct Field {
    /// key for the struct field (for tuples and tuple-structs, this is the 0-based index)
    pub name: &'static str,

    /// shape of the inner type
    ///
    /// Use [`ShapeRef::Static`] for most fields (direct reference, more efficient).
    /// Use [`ShapeRef::Lazy`] for recursive types to break cycles.
    pub shape: ShapeRef,

    /// offset of the field in the struct (obtained through `core::mem::offset_of`)
    pub offset: usize,

    /// Bit flags for common boolean attributes.
    ///
    /// Provides O(1) access to frequently-checked attributes like `sensitive`,
    /// `flatten`, `skip`, etc. These are set by the derive macro based on
    /// `#[facet(...)]` attributes with `#[storage(flag)]` in the grammar.
    pub flags: FieldFlags,

    /// Renamed field name for serialization/deserialization.
    ///
    /// Set by `#[facet(rename = "name")]`. When present, serializers/deserializers
    /// should use this name instead of the field's actual name.
    pub rename: Option<&'static str>,

    /// Alternative name(s) accepted during deserialization.
    ///
    /// Set by `#[facet(alias = "name")]`. During deserialization, this name
    /// is accepted in addition to the primary name (or renamed name).
    // TODO: This should probably be `&'static [&'static str]` to support multiple aliases
    pub alias: Option<&'static str>,

    /// arbitrary attributes set via the derive macro
    ///
    /// This slice contains extension attributes that don't have dedicated storage.
    /// Builtin attributes with `#[storage(flag)]` or `#[storage(field)]` are stored
    /// in their dedicated fields instead.
    pub attributes: &'static [FieldAttribute],

    /// doc comments
    pub doc: &'static [&'static str],
}

impl Field {
    /// Returns true if the field should be skipped during serialization.
    ///
    /// This checks the `SKIP` and `SKIP_SERIALIZING` flags (O(1)),
    /// then the `skip_serializing_if` function if present.
    ///
    /// # Safety
    /// The ptr should correspond to a value of the same type as this field
    pub unsafe fn should_skip_serializing(&self, ptr: PtrConst<'_>) -> bool {
        if self.flags.contains(FieldFlags::SKIP)
            || self.flags.contains(FieldFlags::SKIP_SERIALIZING)
        {
            return true;
        }
        if let Some(skip_serializing_if) = self.skip_serializing_if_fn() {
            return unsafe { skip_serializing_if(ptr) };
        }
        false
    }

    /// Returns true if this field should be skipped during deserialization.
    ///
    /// This checks the `SKIP` and `SKIP_DESERIALIZING` flags (O(1)).
    #[inline]
    pub fn should_skip_deserializing(&self) -> bool {
        self.flags.contains(FieldFlags::SKIP) || self.flags.contains(FieldFlags::SKIP_DESERIALIZING)
    }

    /// Returns true if this field is flattened.
    ///
    /// This checks the `FLATTEN` flag (O(1)).
    #[inline]
    pub fn is_flattened(&self) -> bool {
        self.flags.contains(FieldFlags::FLATTEN)
    }

    /// Returns true if this field is marked as sensitive.
    ///
    /// This checks the `SENSITIVE` flag (O(1)).
    #[inline]
    pub fn is_sensitive(&self) -> bool {
        self.flags.contains(FieldFlags::SENSITIVE)
    }

    /// Returns true if this field has a default value.
    ///
    /// This checks the `HAS_DEFAULT` flag (O(1)).
    #[inline]
    pub fn has_default(&self) -> bool {
        self.flags.contains(FieldFlags::HAS_DEFAULT)
    }

    /// Returns true if this field is a child (for KDL/XML formats).
    ///
    /// This checks the `CHILD` flag (O(1)).
    #[inline]
    pub fn is_child(&self) -> bool {
        self.flags.contains(FieldFlags::CHILD)
    }

    /// Returns the effective name for this field during serialization/deserialization.
    ///
    /// Returns `rename` if set, otherwise returns the field's actual name.
    #[inline]
    pub fn effective_name(&self) -> &'static str {
        self.rename.unwrap_or(self.name)
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

#[cfg(feature = "alloc")]
/// Definition of a proxy type for serialization/deserialization.
///
/// This is used when `#[facet(proxy = ProxyType)]` is applied at the container level
/// (struct or enum). It stores everything needed to convert values to/from the proxy type.
///
/// The user must implement:
/// - `TryFrom<ProxyType> for OriginalType` (for deserialization: proxy → original)
/// - `TryFrom<&OriginalType> for ProxyType` (for serialization: original → proxy)
#[derive(Clone, Copy)]
pub struct ProxyDef {
    /// The shape of the proxy type.
    pub shape: &'static super::Shape,

    /// Function to convert FROM proxy type INTO the original type.
    /// Used during deserialization.
    pub convert_in: ProxyConvertInFn,

    /// Function to convert FROM original type OUT TO proxy type.
    /// Used during serialization.
    pub convert_out: ProxyConvertOutFn,
}

impl Field {
    /// Returns the shape of the inner type
    #[inline]
    pub fn shape(&self) -> &'static Shape {
        self.shape.get()
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
    ($container:ty, $field:tt, $field_ty:ty) => {
        $crate::Field {
            name: stringify!($field),
            shape: $crate::ShapeRef::Static(<$field_ty as $crate::Facet>::SHAPE),
            offset: ::core::mem::offset_of!(Self, $field),
            flags: $crate::FieldFlags::empty(),
            rename: None,
            alias: None,
            attributes: &[],
            doc: &[],
        }
    };
}

pub(crate) use field_in_type;

/// Builder for constructing `Field` instances in const contexts.
///
/// This builder is primarily used by derive macros to generate more compact code.
/// All methods are `const fn` to allow usage in static/const contexts.
///
/// # Example
///
/// ```ignore
/// // For normal fields (default, most efficient):
/// const FIELD: Field = FieldBuilder::new(
///     "field_name",
///     <T as Facet>::SHAPE,
///     offset_of!(MyStruct, field_name)
/// ).build();
///
/// // For recursive type fields (use lazy to break cycles):
/// const FIELD: Field = FieldBuilder::new_lazy(
///     "children",
///     || <Vec<Self> as Facet>::SHAPE,
///     offset_of!(MyStruct, children)
/// ).build();
/// ```
pub struct FieldBuilder {
    name: &'static str,
    shape: ShapeRef,
    offset: usize,
    flags: FieldFlags,
    rename: Option<&'static str>,
    alias: Option<&'static str>,
    attributes: &'static [FieldAttribute],
    doc: &'static [&'static str],
}

impl FieldBuilder {
    /// Creates a new `FieldBuilder` with a static shape reference (default, most efficient).
    ///
    /// Use this for most fields. The `attributes` and `doc` fields default to empty slices.
    #[inline]
    pub const fn new(name: &'static str, shape: &'static Shape, offset: usize) -> Self {
        Self {
            name,
            shape: ShapeRef::Static(shape),
            offset,
            flags: FieldFlags::empty(),
            rename: None,
            alias: None,
            attributes: &[],
            doc: &[],
        }
    }

    /// Creates a new `FieldBuilder` with a lazy shape reference (for recursive types).
    ///
    /// Use this for fields with recursive types (e.g., `Vec<Self>`) to break cycles.
    /// Mark such fields with `#[facet(recursive_type)]` in the derive macro.
    #[inline]
    pub const fn new_lazy(
        name: &'static str,
        shape: fn() -> &'static Shape,
        offset: usize,
    ) -> Self {
        Self {
            name,
            shape: ShapeRef::Lazy(shape),
            offset,
            flags: FieldFlags::empty(),
            rename: None,
            alias: None,
            attributes: &[],
            doc: &[],
        }
    }

    /// Sets the attributes for this field.
    #[inline]
    pub const fn attributes(mut self, attributes: &'static [FieldAttribute]) -> Self {
        self.attributes = attributes;
        self
    }

    /// Sets the documentation for this field.
    #[inline]
    pub const fn doc(mut self, doc: &'static [&'static str]) -> Self {
        self.doc = doc;
        self
    }

    /// Sets the flags for this field.
    #[inline]
    pub const fn flags(mut self, flags: FieldFlags) -> Self {
        self.flags = flags;
        self
    }

    /// Sets the rename for this field.
    #[inline]
    pub const fn rename(mut self, rename: &'static str) -> Self {
        self.rename = Some(rename);
        self
    }

    /// Sets the alias for this field.
    #[inline]
    pub const fn alias(mut self, alias: &'static str) -> Self {
        self.alias = Some(alias);
        self
    }

    /// Builds the final `Field` instance.
    #[inline]
    pub const fn build(self) -> Field {
        Field {
            name: self.name,
            shape: self.shape,
            offset: self.offset,
            flags: self.flags,
            rename: self.rename,
            alias: self.alias,
            attributes: self.attributes,
            doc: self.doc,
        }
    }
}
