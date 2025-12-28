use crate::{DefaultInPlaceFn, InvariantsFn, PtrConst};

use super::Shape;

/// Source of a field's default value.
///
/// Used by the `#[facet(default)]` attribute.
#[derive(Clone, Copy, Debug)]
pub enum DefaultSource {
    /// Use the type's Default trait via shape vtable.
    /// Set when `#[facet(default)]` is used without an expression.
    FromTrait,
    /// Custom default expression wrapped in a function.
    /// Set when `#[facet(default = expr)]` is used.
    Custom(DefaultInPlaceFn),
}

/// A lazy reference to a [`Shape`] via a function pointer.
///
/// All shape references use function pointers to enable lazy evaluation,
/// which moves const evaluation overhead from compile time to runtime.
/// This significantly improves compile times for large codebases.
///
/// The function is typically a monomorphized generic function like:
/// ```ignore
/// fn shape_of<T: Facet>() -> &'static Shape { T::SHAPE }
/// ```
#[derive(Clone, Copy)]
pub struct ShapeRef(pub fn() -> &'static Shape);

impl ShapeRef {
    /// Get the referenced shape by calling the function.
    #[inline]
    pub fn get(&self) -> &'static Shape {
        (self.0)()
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
    /// [`ShapeRef`] wraps a function that returns the shape, enabling lazy evaluation
    /// for recursive types while still being simple to use.
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

    /// Default value source for this field.
    /// Set by `#[facet(default)]` or `#[facet(default = expr)]`.
    pub default: Option<DefaultSource>,

    /// Predicate to conditionally skip serialization of this field.
    /// Set by `#[facet(skip_serializing_if = fn_name)]`.
    pub skip_serializing_if: Option<SkipSerializingIfFn>,

    /// Type invariant validation function for this field.
    /// Set by `#[facet(invariants = fn_name)]`.
    pub invariants: Option<InvariantsFn>,

    /// Proxy definition for custom serialization/deserialization.
    /// Set by `#[facet(proxy = ProxyType)]`.
    #[cfg(feature = "alloc")]
    pub proxy: Option<&'static super::ProxyDef>,

    /// Metadata kind for this field, if it stores metadata.
    /// Set by `#[facet(metadata = kind)]` (e.g., `#[facet(metadata = span)]`).
    ///
    /// Metadata fields are:
    /// - Excluded from structural hashing (`Peek::structural_hash`)
    /// - Excluded from structural equality comparisons
    /// - Excluded from tree diffing
    /// - Populated by deserializers that support the metadata kind
    ///
    /// Common metadata kinds:
    /// - `"span"`: Source byte offset and length (for `Spanned<T>`)
    /// - `"line"`: Source line number
    /// - `"column"`: Source column number
    pub metadata: Option<&'static str>,
}

impl Field {
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
    /// This returns true for both `#[facet(default)]` (uses the type's Default impl)
    /// and `#[facet(default = expr)]` (uses a custom expression).
    #[inline]
    pub fn has_default(&self) -> bool {
        self.default.is_some()
    }

    /// Returns the default source for this field, if any.
    #[inline]
    pub fn default_source(&self) -> Option<&DefaultSource> {
        self.default.as_ref()
    }

    /// Returns true if this field is a child (for KDL/XML/HTML formats).
    ///
    /// This checks the `CHILD` flag (O(1)).
    #[inline]
    pub fn is_child(&self) -> bool {
        self.flags.contains(FieldFlags::CHILD)
    }

    /// Returns true if this field is marked as text content (for XML/HTML formats).
    ///
    /// Checks for `xml::text` or `html::text` attributes.
    #[inline]
    pub fn is_text(&self) -> bool {
        self.has_attr(Some("xml"), "text") || self.has_attr(Some("html"), "text")
    }

    /// Returns true if this field collects multiple child elements (for XML/HTML/KDL formats).
    ///
    /// Checks for `xml::elements`, `html::elements`, `kdl::children` attributes.
    #[inline]
    pub fn is_elements(&self) -> bool {
        self.has_attr(Some("xml"), "elements")
            || self.has_attr(Some("html"), "elements")
            || self.has_attr(Some("kdl"), "children")
    }

    /// Returns true if this field is a single child element (for XML/HTML/KDL formats).
    ///
    /// Checks for `xml::element`, `html::element`, `kdl::child` attributes.
    #[inline]
    pub fn is_element(&self) -> bool {
        self.has_attr(Some("xml"), "element")
            || self.has_attr(Some("html"), "element")
            || self.has_attr(Some("kdl"), "child")
    }

    /// Returns true if this field is an attribute on the element tag (for XML/HTML formats).
    ///
    /// Checks for `xml::attribute` or `html::attribute` attributes.
    #[inline]
    pub fn is_attribute(&self) -> bool {
        self.has_attr(Some("xml"), "attribute") || self.has_attr(Some("html"), "attribute")
    }

    /// Returns true if this field is a KDL argument (positional value).
    ///
    /// Checks for `kdl::argument` or `kdl::arguments` attributes.
    #[inline]
    pub fn is_argument(&self) -> bool {
        self.has_attr(Some("kdl"), "argument") || self.has_attr(Some("kdl"), "arguments")
    }

    /// Returns true if this field is a KDL property (named value).
    ///
    /// Checks for `kdl::property` attribute.
    #[inline]
    pub fn is_property(&self) -> bool {
        self.has_attr(Some("kdl"), "property")
    }

    /// Returns true if this field stores metadata.
    ///
    /// Metadata fields are excluded from structural hashing and equality.
    /// Use `metadata_kind()` to get the specific kind of metadata.
    #[inline]
    pub fn is_metadata(&self) -> bool {
        self.metadata.is_some()
    }

    /// Returns the metadata kind if this field stores metadata.
    ///
    /// Common values: `"span"`, `"line"`, `"column"`
    #[inline]
    pub fn metadata_kind(&self) -> Option<&'static str> {
        self.metadata
    }

    /// Returns true if this field should be skipped during deserialization.
    ///
    /// This checks the `SKIP` and `SKIP_DESERIALIZING` flags (O(1)).
    #[inline]
    pub fn should_skip_deserializing(&self) -> bool {
        !self
            .flags
            .intersection(FieldFlags::SKIP.union(FieldFlags::SKIP_DESERIALIZING))
            .is_empty()
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
pub type SkipSerializingIfFn = unsafe fn(value: PtrConst) -> bool;

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
    pub fn get_attr(&self, ns: Option<&str>, key: &str) -> Option<&super::Attr> {
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
    pub fn get_builtin_attr(&self, key: &str) -> Option<&super::Attr> {
        self.get_attr(None, key)
    }

    /// Gets the proxy definition, if present.
    ///
    /// This is set when `#[facet(proxy = ProxyType)]` is used. The proxy type
    /// is used for both serialization and deserialization. The user must implement:
    /// - `TryFrom<ProxyType> for FieldType` (for deserialization)
    /// - `TryFrom<&FieldType> for ProxyType` (for serialization)
    #[cfg(feature = "alloc")]
    #[inline]
    pub fn proxy(&self) -> Option<&'static super::ProxyDef> {
        self.proxy
    }

    /// Gets the proxy shape, if present.
    ///
    /// Convenience method that returns just the shape from the proxy definition.
    #[cfg(feature = "alloc")]
    #[inline]
    pub fn proxy_shape(&self) -> Option<&'static super::Shape> {
        self.proxy.map(|p| p.shape)
    }

    /// Checks if this field should be skipped during serialization.
    ///
    /// Returns `true` if:
    /// - The field has `SKIP_SERIALIZING` flag set, or
    /// - `skip_serializing_if` is set and the predicate returns true
    ///
    /// # Safety
    ///
    /// `field_ptr` must point to a valid value of this field's type.
    #[inline]
    pub unsafe fn should_skip_serializing(&self, field_ptr: PtrConst) -> bool {
        // Check the SKIP flag (which means skip both serialization and deserialization)
        if self.flags.contains(FieldFlags::SKIP) {
            return true;
        }
        // Check the SKIP_SERIALIZING flag (which means skip serialization only)
        if self.flags.contains(FieldFlags::SKIP_SERIALIZING) {
            return true;
        }
        // Then check the predicate if set
        if let Some(predicate) = self.skip_serializing_if {
            unsafe { predicate(field_ptr) }
        } else {
            false
        }
    }

    /// Returns true if this field has a proxy for custom ser/de.
    ///
    /// When true, use `proxy()` to get the proxy definition which contains
    /// the proxy shape and conversion functions.
    #[cfg(feature = "alloc")]
    #[inline]
    pub fn has_proxy(&self) -> bool {
        self.proxy.is_some()
    }

    /// Gets the proxy convert_in function, if present.
    ///
    /// This converts from proxy type to target type (deserialization).
    #[cfg(feature = "alloc")]
    #[inline]
    pub fn proxy_convert_in_fn(&self) -> Option<super::ProxyConvertInFn> {
        self.proxy.map(|p| p.convert_in)
    }

    /// Gets the proxy convert_out function, if present.
    ///
    /// This converts from target type to proxy type (serialization).
    #[cfg(feature = "alloc")]
    #[inline]
    pub fn proxy_convert_out_fn(&self) -> Option<super::ProxyConvertOutFn> {
        self.proxy.map(|p| p.convert_out)
    }
}

/// An attribute that can be set on a field.
/// This is now just an alias for `ExtensionAttr` - all attributes use the same representation.
pub type FieldAttribute = super::Attr;

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
    default: Option<DefaultSource>,
    skip_serializing_if: Option<SkipSerializingIfFn>,
    invariants: Option<InvariantsFn>,
    #[cfg(feature = "alloc")]
    proxy: Option<&'static super::ProxyDef>,
    metadata: Option<&'static str>,
}

impl FieldBuilder {
    /// Creates a new `FieldBuilder` with a shape function.
    ///
    /// The shape is provided as a function pointer to enable lazy evaluation,
    /// which improves compile times by deferring const evaluation to runtime.
    ///
    /// Use the `shape_of::<T>` helper function for the common case:
    /// ```ignore
    /// FieldBuilder::new("field", shape_of::<i32>, offset)
    /// ```
    #[inline]
    pub const fn new(name: &'static str, shape: fn() -> &'static Shape, offset: usize) -> Self {
        Self {
            name,
            shape: ShapeRef(shape),
            offset,
            flags: FieldFlags::empty(),
            rename: None,
            alias: None,
            attributes: &[],
            doc: &[],
            default: None,
            skip_serializing_if: None,
            invariants: None,
            #[cfg(feature = "alloc")]
            proxy: None,
            metadata: None,
        }
    }

    /// Alias for `new` - kept for backward compatibility.
    ///
    /// Previously used for recursive types, but now all fields use lazy shape references.
    #[inline]
    pub const fn new_lazy(
        name: &'static str,
        shape: fn() -> &'static Shape,
        offset: usize,
    ) -> Self {
        Self::new(name, shape, offset)
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

    /// Sets the default to use the type's Default trait.
    #[inline]
    pub const fn default_from_trait(mut self) -> Self {
        self.default = Some(DefaultSource::FromTrait);
        self
    }

    /// Sets a custom default function.
    #[inline]
    pub const fn default_custom(mut self, f: DefaultInPlaceFn) -> Self {
        self.default = Some(DefaultSource::Custom(f));
        self
    }

    /// Sets the skip_serializing_if predicate.
    #[inline]
    pub const fn skip_serializing_if(mut self, f: SkipSerializingIfFn) -> Self {
        self.skip_serializing_if = Some(f);
        self
    }

    /// Sets the invariants validation function.
    #[inline]
    pub const fn invariants(mut self, f: InvariantsFn) -> Self {
        self.invariants = Some(f);
        self
    }

    /// Sets the proxy definition for custom ser/de.
    #[cfg(feature = "alloc")]
    #[inline]
    pub const fn proxy(mut self, proxy: &'static super::ProxyDef) -> Self {
        self.proxy = Some(proxy);
        self
    }

    /// Marks this field as storing metadata of the given kind.
    ///
    /// Metadata fields are excluded from structural hashing and equality,
    /// and are populated by deserializers that support the metadata kind.
    ///
    /// Common metadata kinds:
    /// - `"span"`: Source byte offset and length
    /// - `"line"`: Source line number
    /// - `"column"`: Source column number
    #[inline]
    pub const fn metadata(mut self, kind: &'static str) -> Self {
        self.metadata = Some(kind);
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
            default: self.default,
            skip_serializing_if: self.skip_serializing_if,
            invariants: self.invariants,
            #[cfg(feature = "alloc")]
            proxy: self.proxy,
            metadata: self.metadata,
        }
    }
}
