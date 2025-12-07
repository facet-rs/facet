//! structs and vtable definitions used by Facet

use crate::PtrConst;
#[cfg(feature = "alloc")]
use crate::PtrMut;

use core::alloc::Layout;

mod attr_grammar;
pub use attr_grammar::*;

mod characteristic;
pub use characteristic::*;

mod value;
pub use value::*;

mod def;
pub use def::*;

mod ty;
pub use ty::*;

use crate::{ConstTypeId, Facet};

/// Schema for reflection of a type
#[derive(Clone, Copy)]
#[repr(C)]
pub struct Shape {
    /// Unique type identifier, provided by the compiler.
    pub id: ConstTypeId,

    /// Size, alignment — enough to allocate a value of this type
    /// (but not initialize it.)
    pub layout: ShapeLayout,

    /// Function pointers to perform various operations: print the full type
    /// name (with generic type parameters), use the Display implementation,
    /// the Debug implementation, build a default value, clone, etc.
    ///
    /// If the shape has `ShapeLayout::Unsized`, then the parent pointer needs to be passed.
    ///
    /// There are more specific vtables in variants of [`Def`]
    pub vtable: ValueVTable,

    /// Underlying type: primitive, sequence, user, pointer.
    ///
    /// This follows the [`Rust Reference`](https://doc.rust-lang.org/reference/types.html), but
    /// omits function types, and trait types, as they cannot be represented here.
    pub ty: Type,

    /// Functional definition of the value: details for scalars, functions for inserting values into
    /// a map, or fetching a value from a list.
    pub def: Def,

    /// Identifier for a type: the type's name without generic parameters. To get the type's full
    /// name with generic parameters, see [`ValueVTable::type_name`].
    pub type_identifier: &'static str,

    /// Generic parameters for the shape
    pub type_params: &'static [TypeParam],

    /// Doc comment lines, collected by facet-macros. Note that they tend to
    /// start with a space.
    pub doc: &'static [&'static str],

    /// Attributes that can be applied to a shape
    pub attributes: &'static [ShapeAttribute],

    /// Shape type tag, used to identify the type in self describing formats.
    ///
    /// For some formats, this is a fully or partially qualified name.
    /// For other formats, this is a simple string or integer type.
    pub type_tag: Option<&'static str>,

    /// As far as serialization and deserialization goes, we consider that this shape is a wrapper
    /// for that shape This is true for "newtypes" like `NonZero`, wrappers like `Utf8PathBuf`,
    /// smart pointers like `Arc<T>`, etc.
    ///
    /// When this is set, deserialization takes that into account. For example, facet-json
    /// doesn't expect:
    ///
    ///   { "NonZero": { "value": 128 } }
    ///
    /// It expects just
    ///
    ///   128
    ///
    /// Same for `Utf8PathBuf`, which is parsed from and serialized to "just a string".
    ///
    /// See Partial's `innermost_shape` function (and its support in `put`).
    pub inner: Option<&'static Shape>,

    /// Default proxy type for this shape, if any.
    ///
    /// When `#[facet(proxy = ProxyType)]` is applied at the container level (struct/enum),
    /// this stores a reference to the proxy definition. This allows any value of this type
    /// to be automatically converted through the proxy during serialization/deserialization,
    /// even when nested inside generic containers like `Vec<T>` or `Option<T>`.
    ///
    /// Field-level `#[facet(proxy = ...)]` takes precedence over this container-level proxy.
    ///
    /// This is stored as an opaque pointer to avoid conditional compilation on all Shape
    /// constructors. Use the `proxy()` method to access it with proper typing.
    pub proxy: Option<&'static ()>,
}

impl PartialOrd for Shape {
    #[allow(clippy::non_canonical_partial_ord_impl)]
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        self.id.get().partial_cmp(&other.id.get())
    }
}

impl Ord for Shape {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.id.get().cmp(&other.id.get())
    }
}

/// Layout of the shape
#[derive(Clone, Copy, Debug, Hash)]
pub enum ShapeLayout {
    /// `Sized` type
    Sized(Layout),
    /// `!Sized` type
    Unsized,
}

impl ShapeLayout {
    /// `Layout` if this type is `Sized`
    #[inline]
    pub const fn sized_layout(self) -> Result<Layout, UnsizedError> {
        match self {
            ShapeLayout::Sized(layout) => Ok(layout),
            ShapeLayout::Unsized => Err(UnsizedError),
        }
    }
}

/// Tried to get the `Layout` of an unsized type
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Hash)]
pub struct UnsizedError;

impl core::fmt::Display for UnsizedError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Not a Sized type")
    }
}

impl core::error::Error for UnsizedError {}

/// An extension attribute for third-party crates to attach metadata.
///
/// Attributes use syntax like `#[facet(sensitive)]` for builtins or
/// `#[facet(orm::primary_key)]` for namespaced extension attributes.
///
/// The derive macro expands attributes to macro invocations that
/// return `ExtensionAttr` values with typed data.
pub struct ExtensionAttr {
    /// The namespace (e.g., Some("orm") in `#[facet(orm::primary_key)]`).
    /// None for builtin attributes like `#[facet(sensitive)]`.
    pub ns: Option<&'static str>,

    /// The key (e.g., "primary_key" in `#[facet(orm::primary_key)]`)
    pub key: &'static str,

    /// Pointer to the static data stored by the attribute.
    pub data: *const (),

    /// Shape of the data, enabling full introspection via facet's reflection.
    pub shape: &'static Shape,
}

// SAFETY: ExtensionAttr only holds static data, which is inherently Send + Sync
unsafe impl Send for ExtensionAttr {}
unsafe impl Sync for ExtensionAttr {}

impl ExtensionAttr {
    /// Create a new attribute from static data.
    ///
    /// The type must implement `Facet` so we can store its shape for introspection.
    #[inline]
    pub const fn new<'a, T: Facet<'a>>(
        ns: Option<&'static str>,
        key: &'static str,
        data: &'static T,
    ) -> Self {
        Self {
            ns,
            key,
            data: data as *const T as *const (),
            shape: T::SHAPE,
        }
    }

    /// Create a new attribute storing a Shape reference.
    ///
    /// This is used for `shape_type` variants (like `proxy = ProxyType`) where
    /// the attribute data is a Shape reference. Since `Shape` doesn't implement
    /// `Facet`, we use this specialized constructor.
    #[inline]
    pub const fn new_shape(
        ns: Option<&'static str>,
        key: &'static str,
        shape_data: &'static Shape,
    ) -> Self {
        Self {
            ns,
            key,
            data: shape_data as *const Shape as *const (),
            // Use the shape of the stored data for introspection
            // (the Shape's own shape - which is itself)
            shape: shape_data,
        }
    }

    /// Returns true if this is a builtin attribute (no namespace).
    #[inline]
    pub const fn is_builtin(&self) -> bool {
        self.ns.is_none()
    }

    /// Get the typed data if the shape matches.
    ///
    /// Returns `None` if the type doesn't match.
    #[inline]
    pub fn get_as<'a, T: Facet<'a>>(&self) -> Option<&'static T> {
        if self.shape == T::SHAPE {
            Some(unsafe { &*(self.data as *const T) })
        } else {
            None
        }
    }

    /// Get the typed data, panicking if the shape doesn't match.
    ///
    /// Use this when you know the expected type and want a simpler API.
    #[inline]
    pub fn must_get_as<'a, T: Facet<'a>>(&self) -> &'static T {
        self.get_as().unwrap_or_else(|| {
            let ns_str = self.ns.unwrap_or("<builtin>");
            panic!(
                "ExtensionAttr {}::{} - expected shape {}, got {}",
                ns_str,
                self.key,
                T::SHAPE,
                self.shape
            )
        })
    }

    /// Get the data pointer and shape for creating a `Peek` value.
    ///
    /// Use with `facet_reflect::Peek::unchecked_new()` for full introspection:
    /// ```ignore
    /// let (ptr, shape) = ext_attr.data_and_shape();
    /// let peek = unsafe { Peek::unchecked_new(PtrConst::new(ptr), shape) };
    /// ```
    #[inline]
    pub const fn data_and_shape(&self) -> (*const (), &'static Shape) {
        (self.data, self.shape)
    }

    /// Get the data as a raw pointer to a specific type.
    ///
    /// This is useful for types that don't implement `Facet` (like `Shape` itself).
    /// No type checking is performed - use with care.
    ///
    /// # Safety
    /// The caller must ensure that `T` is the correct type for this attribute's data.
    #[inline]
    pub const unsafe fn data_ptr<T>(&self) -> *const T {
        self.data as *const T
    }

    /// Get the data as a reference to a specific type.
    ///
    /// This is useful for types that don't implement `Facet` (like `Shape` itself).
    /// No type checking is performed - use with care.
    ///
    /// # Safety
    /// The caller must ensure that `T` is the correct type for this attribute's data.
    #[inline]
    pub unsafe fn data_ref<T>(&self) -> &'static T {
        // SAFETY: caller guarantees T is the correct type
        unsafe { &*self.data_ptr::<T>() }
    }
}

impl core::fmt::Debug for ExtensionAttr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Write the attribute name (with or without namespace)
        match self.ns {
            Some(ns) => write!(f, "{}::{}", ns, self.key)?,
            None => write!(f, "{}", self.key)?,
        };

        // Try to use the shape's debug function if available
        if let Some(debug_fn) = self.shape.vtable.format.debug {
            write!(f, " = ")?;
            // SAFETY: self.data is a valid pointer to static data of the correct shape
            unsafe {
                let ptr = core::ptr::NonNull::new_unchecked(self.data as *mut ());
                debug_fn(PtrConst::new(ptr), f)
            }
        } else {
            Ok(())
        }
    }
}

impl PartialEq for ExtensionAttr {
    fn eq(&self, other: &Self) -> bool {
        // Compare by namespace and key only (args don't impl PartialEq, and we don't need to compare them)
        self.ns == other.ns && self.key == other.key
    }
}

/// An attribute that can be applied to a shape.
/// This is now just an alias for `ExtensionAttr` - all attributes use the same representation.
pub type ShapeAttribute = ExtensionAttr;

impl Shape {
    /// Returns a const type ID for type `T`.
    #[inline]
    pub const fn id_of<T: ?Sized>() -> ConstTypeId {
        ConstTypeId::of::<T>()
    }

    /// Returns the sized layout for type `T`.
    #[inline]
    pub const fn layout_of<T>() -> ShapeLayout {
        ShapeLayout::Sized(Layout::new::<T>())
    }

    /// Returns the unsized layout marker.
    pub const UNSIZED_LAYOUT: ShapeLayout = ShapeLayout::Unsized;

    /// Check if this shape is of the given type
    pub fn is_type<'facet, Other: Facet<'facet>>(&self) -> bool {
        let l = self;
        let r = Other::SHAPE;
        l == r
    }

    /// Assert that this shape is of the given type, panicking if it's not
    pub fn assert_type<'facet, Other: Facet<'facet>>(&self) {
        assert!(
            self.is_type::<Other>(),
            "Type mismatch: expected {}, found {self}",
            Other::SHAPE,
        );
    }

    /// Returns true if this shape has the `deny_unknown_fields` builtin attribute.
    #[inline]
    pub fn has_deny_unknown_fields_attr(&self) -> bool {
        self.has_builtin_attr("deny_unknown_fields")
    }

    /// Returns true if this shape has the `default` builtin attribute.
    #[inline]
    pub fn has_default_attr(&self) -> bool {
        self.has_builtin_attr("default")
    }

    /// Returns the `rename_all` case conversion rule if present.
    #[inline]
    pub fn get_rename_all_attr(&self) -> Option<&'static str> {
        self.get_builtin_attr_value::<&'static str>("rename_all")
    }

    /// Returns the `alias`ed name if present.
    #[inline]
    pub fn get_alias_attr(&self) -> Option<&'static str> {
        self.get_builtin_attr_value::<&'static str>("alias")
    }

    /// Returns true if this enum is untagged.
    #[inline]
    pub fn is_untagged(&self) -> bool {
        self.has_builtin_attr("untagged")
    }

    /// Returns the tag field name for internally/adjacently tagged enums.
    #[inline]
    pub fn get_tag_attr(&self) -> Option<&'static str> {
        self.get_builtin_attr_value::<&'static str>("tag")
    }

    /// Returns the content field name for adjacently tagged enums.
    #[inline]
    pub fn get_content_attr(&self) -> Option<&'static str> {
        self.get_builtin_attr_value::<&'static str>("content")
    }

    /// Returns true if this shape has a builtin attribute with the given key.
    #[inline]
    pub fn has_builtin_attr(&self, key: &str) -> bool {
        self.attributes
            .iter()
            .any(|attr| attr.ns.is_none() && attr.key == key)
    }

    /// Returns true if this shape has a transparent attribute.
    #[inline]
    pub fn is_transparent(&self) -> bool {
        self.has_builtin_attr("transparent")
    }

    /// Gets the value of a builtin attribute, if the attribute data can be interpreted as type T.
    ///
    /// This is a helper for attributes with simple payload types like `&'static str`.
    #[inline]
    pub fn get_builtin_attr_value<'a, T: Facet<'a> + Copy + 'static>(
        &self,
        key: &str,
    ) -> Option<T> {
        self.attributes.iter().find_map(|attr| {
            if attr.ns.is_none() && attr.key == key {
                // Try to get the data as the requested type
                attr.get_as::<T>().copied()
            } else {
                None
            }
        })
    }

    /// Returns the container-level proxy definition, if any.
    ///
    /// This is set when `#[facet(proxy = ProxyType)]` is applied at the struct/enum level.
    /// When present, values of this type will automatically be converted through the proxy
    /// during serialization/deserialization.
    #[cfg(feature = "alloc")]
    #[inline]
    pub fn proxy(&self) -> Option<&'static ProxyDef> {
        // SAFETY: When alloc is enabled, the proxy field is only set via
        // ShapeBuilder::proxy() which takes &'static ProxyDef
        self.proxy
            .map(|p| unsafe { &*(p as *const () as *const ProxyDef) })
    }
}

impl PartialEq for Shape {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for Shape {}

impl core::hash::Hash for Shape {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
        self.layout.hash(state);
    }
}

impl Shape {
    /// Check if this shape is of the given type
    #[inline]
    pub fn is_shape(&self, other: &Shape) -> bool {
        self == other
    }

    /// Assert that this shape is equal to the given shape, panicking if it's not
    pub fn assert_shape(&self, other: &Shape) {
        assert!(
            self.is_shape(other),
            "Shape mismatch: expected {other}, found {self}",
        );
    }

    /// Returns true if this shape requires eager materialization.
    ///
    /// Shapes that require eager materialization cannot have their construction
    /// deferred because they need all their data available at once. Examples include:
    ///
    /// - `Arc<[T]>`, `Box<[T]>`, `Rc<[T]>` - slice-based smart pointers that need
    ///   all elements to compute the final allocation
    ///
    /// This is used by deferred validation mode in `Partial` to determine which
    /// shapes must be fully materialized before proceeding.
    #[inline]
    pub fn requires_eager_materialization(&self) -> bool {
        // Check if this is a pointer type with slice_builder_vtable
        // (indicates Arc<[T]>, Box<[T]>, Rc<[T]>, etc.)
        if let Ok(ptr_def) = self.def.into_pointer() {
            if ptr_def.vtable.slice_builder_vtable.is_some() {
                return true;
            }
        }
        false
    }
}

// Helper struct to format the name for display
impl core::fmt::Display for Shape {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        (self.vtable.type_name())(f, TypeNameOpts::default())
    }
}

impl core::fmt::Debug for Shape {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        // NOTE:
        // This dummy destructuring is present to ensure that if fields are added,
        // developers will get a compiler error in this function, reminding them
        // to carefully consider whether it should be shown when debug formatting.
        let Self {
            id: _, // omit by default
            layout: _,
            vtable: _, // omit by default
            ty: _,
            def: _,
            type_identifier: _,
            type_params: _,
            doc: _,
            attributes: _,
            type_tag: _,
            inner: _,
            proxy: _, // omit by default
        } = self;

        if f.alternate() {
            f.debug_struct("Shape")
                .field("id", &self.id)
                .field("layout", &format_args!("{:?}", self.layout))
                .field("vtable", &format_args!("ValueVTable {{ .. }}"))
                .field("ty", &self.ty)
                .field("def", &self.def)
                .field("type_identifier", &self.type_identifier)
                .field("type_params", &self.type_params)
                .field("doc", &self.doc)
                .field("attributes", &self.attributes)
                .field("type_tag", &self.type_tag)
                .field("inner", &self.inner)
                .finish()
        } else {
            let mut debug_struct = f.debug_struct("Shape");

            macro_rules! field {
                ( $field:literal, $( $fmt_args:tt )* ) => {{
                    debug_struct.field($field, &format_args!($($fmt_args)*));
                }};
            }

            field!("type_identifier", "{:?}", self.type_identifier);

            if !self.type_params.is_empty() {
                // Use `[]` to indicate empty `type_params` (a real empty slice),
                // and `«(...)»` to show custom-formatted parameter sets when present.
                // Avoids visual conflict with array types like `[T; N]` in other fields.
                field!("type_params", "{}", {
                    struct TypeParams<'shape>(&'shape [TypeParam]);
                    impl core::fmt::Display for TypeParams<'_> {
                        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                            let mut iter = self.0.iter();
                            if let Some(first) = iter.next() {
                                write!(f, "«({}: {}", first.name, first.shape)?;
                                for next in iter {
                                    write!(f, ", {}: {}", next.name, next.shape)?;
                                }
                                write!(f, ")»")?;
                            } else {
                                write!(f, "[]")?;
                            }
                            Ok(())
                        }
                    }
                    TypeParams(self.type_params)
                });
            }

            if let Some(type_tag) = self.type_tag {
                field!("type_tag", "{:?}", type_tag);
            }

            if !self.attributes.is_empty() {
                field!("attributes", "{:?}", self.attributes);
            }

            // Omit the `inner` field if this shape is not a transparent wrapper.
            if let Some(inner) = self.inner {
                field!("inner", "{:?}", inner);
            }

            // Uses `Display` to potentially format with shorthand syntax.
            field!("ty", "{}", self.ty);

            // For sized layouts, display size and alignment in shorthand.
            // NOTE: If you wish to display the bitshift for alignment, please open an issue.
            if let ShapeLayout::Sized(layout) = self.layout {
                field!(
                    "layout",
                    "Sized(«{} align {}»)",
                    layout.size(),
                    layout.align()
                );
            } else {
                field!("layout", "{:?}", self.layout);
            }

            // If `def` is `Undefined`, the information in `ty` would be more useful.
            if !matches!(self.def, Def::Undefined) {
                field!("def", "{:?}", self.def);
            }

            if !self.doc.is_empty() {
                // TODO: Should these be called "strings"? Because `#[doc]` can contain newlines.
                field!("doc", "«{} lines»", self.doc.len());
            }

            debug_struct.finish_non_exhaustive()
        }
    }
}

impl Shape {
    /// Heap-allocate a value of this shape
    #[cfg(feature = "alloc")]
    #[inline]
    pub fn allocate(&self) -> Result<crate::ptr::PtrUninit<'static>, UnsizedError> {
        let layout = self.layout.sized_layout()?;

        Ok(crate::ptr::PtrUninit::new(if layout.size() == 0 {
            use core::ptr::NonNull;

            unsafe {
                NonNull::new_unchecked(
                    core::ptr::null_mut::<u8>().wrapping_byte_add(layout.align()),
                )
            }
        } else {
            // SAFETY: We have checked that layout's size is non-zero

            use core::ptr::NonNull;
            let ptr = unsafe { alloc::alloc::alloc(layout) };
            let Some(ptr) = NonNull::new(ptr) else {
                alloc::alloc::handle_alloc_error(layout)
            };
            ptr
        }))
    }

    /// Deallocate a heap-allocated value of this shape
    ///
    /// # Safety
    ///
    /// - `ptr` must have been allocated using [`Self::allocate`] and be aligned for this shape.
    /// - `ptr` must point to a region that is not already deallocated.
    #[cfg(feature = "alloc")]
    #[inline]
    pub unsafe fn deallocate_mut(&self, ptr: PtrMut) -> Result<(), UnsizedError> {
        use alloc::alloc::dealloc;

        let layout = self.layout.sized_layout()?;

        if layout.size() == 0 {
            // Nothing to deallocate
            return Ok(());
        }
        // SAFETY: The user guarantees ptr is valid and from allocate, we checked size isn't 0
        unsafe { dealloc(ptr.as_mut_byte_ptr(), layout) }

        Ok(())
    }

    /// Deallocate a heap-allocated, uninitialized value of this shape.
    ///
    /// # Safety
    ///
    /// - `ptr` must have been allocated using [`Self::allocate`] (or equivalent) for this shape.
    /// - `ptr` must not have been already deallocated.
    /// - `ptr` must be properly aligned for this shape.
    #[cfg(feature = "alloc")]
    #[inline]
    pub unsafe fn deallocate_uninit(
        &self,
        ptr: crate::ptr::PtrUninit<'static>,
    ) -> Result<(), UnsizedError> {
        use alloc::alloc::dealloc;

        let layout = self.layout.sized_layout()?;

        if layout.size() == 0 {
            // Nothing to deallocate
            return Ok(());
        }
        // SAFETY: The user guarantees ptr is valid and from allocate; layout is nonzero
        unsafe { dealloc(ptr.as_mut_byte_ptr(), layout) };

        Ok(())
    }
}

/// Represents a lifetime parameter, e.g., `'a` or `'a: 'b + 'c`.
///
/// Note: these are subject to change — it's a bit too stringly-typed for now.
#[derive(Debug, Clone)]
pub struct TypeParam {
    /// The name of the type parameter (e.g., `T`).
    pub name: &'static str,

    /// The shape of the type parameter (e.g. `String`)
    pub shape: &'static Shape,
}

impl TypeParam {
    /// Returns the shape of the type parameter.
    #[inline]
    pub const fn shape(&self) -> &'static Shape {
        self.shape
    }
}

/// Builder for creating [`Shape`] instances.
///
/// This builder provides a convenient way to construct Shape values with
/// sensible defaults. Many fields can be inferred or have reasonable defaults:
///
/// ```ignore
/// Shape::builder::<MyType>(|f, _| write!(f, "MyType"))
///     .def(Def::Scalar)
///     .build()
/// ```
pub struct ShapeBuilder {
    id: ConstTypeId,
    layout: ShapeLayout,
    vtable: ValueVTable,
    ty: Option<Type>,
    def: Def,
    type_identifier: &'static str,
    type_params: &'static [TypeParam],
    doc: &'static [&'static str],
    attributes: &'static [ShapeAttribute],
    type_tag: Option<&'static str>,
    inner: Option<&'static Shape>,
    proxy: Option<&'static ()>,
}

impl ShapeBuilder {
    /// Create a new builder for a sized type.
    ///
    /// The `id` and `layout` are derived from the type parameter.
    /// The `type_name` function is used for the vtable.
    #[inline]
    pub const fn for_sized<T>(type_name: TypeNameFn, type_identifier: &'static str) -> Self {
        Self {
            id: ConstTypeId::of::<T>(),
            layout: ShapeLayout::Sized(Layout::new::<T>()),
            vtable: ValueVTable::new(type_name),
            ty: None,
            def: Def::Scalar,
            type_identifier,
            type_params: &[],
            doc: &[],
            attributes: &[],
            type_tag: None,
            inner: None,
            proxy: None,
        }
    }

    /// Create a new builder for an unsized type.
    #[inline]
    pub const fn for_unsized<T: ?Sized>(
        type_name: TypeNameFn,
        type_identifier: &'static str,
    ) -> Self {
        Self {
            id: ConstTypeId::of::<T>(),
            layout: ShapeLayout::Unsized,
            vtable: ValueVTable::new(type_name),
            ty: None,
            def: Def::Scalar,
            type_identifier,
            type_params: &[],
            doc: &[],
            attributes: &[],
            type_tag: None,
            inner: None,
            proxy: None,
        }
    }

    /// Set the vtable.
    #[inline]
    pub const fn vtable(mut self, vtable: ValueVTable) -> Self {
        self.vtable = vtable;
        self
    }

    /// Set the type.
    #[inline]
    pub const fn ty(mut self, ty: Type) -> Self {
        self.ty = Some(ty);
        self
    }

    /// Set the definition.
    #[inline]
    pub const fn def(mut self, def: Def) -> Self {
        self.def = def;
        self
    }

    /// Set the type parameters.
    #[inline]
    pub const fn type_params(mut self, type_params: &'static [TypeParam]) -> Self {
        self.type_params = type_params;
        self
    }

    /// Set the documentation.
    #[inline]
    pub const fn doc(mut self, doc: &'static [&'static str]) -> Self {
        self.doc = doc;
        self
    }

    /// Set the attributes.
    #[inline]
    pub const fn attributes(mut self, attributes: &'static [ShapeAttribute]) -> Self {
        self.attributes = attributes;
        self
    }

    /// Set the type tag.
    #[inline]
    pub const fn type_tag(mut self, type_tag: &'static str) -> Self {
        self.type_tag = Some(type_tag);
        self
    }

    /// Set the inner shape (for transparent/newtype wrappers).
    #[inline]
    pub const fn inner(mut self, inner: &'static Shape) -> Self {
        self.inner = Some(inner);
        self
    }

    /// Set the container-level proxy definition.
    ///
    /// When set, values of this type will automatically be converted through
    /// the proxy during serialization/deserialization, even when nested inside
    /// generic containers like `Vec<T>` or `Option<T>`.
    #[cfg(feature = "alloc")]
    #[inline]
    pub const fn proxy(mut self, proxy: &'static ProxyDef) -> Self {
        // Store as opaque pointer - will be cast back in Shape::proxy()
        self.proxy = Some(unsafe { &*(proxy as *const ProxyDef as *const ()) });
        self
    }

    // ========== Direct vtable field setters ==========
    // These allow modifying the vtable without creating a separate ValueVTableBuilder,
    // reusing the type_name that was already set in for_sized/for_unsized.
    //
    // Each setter has two variants:
    // - `foo(f)` takes the function directly (ergonomic for unconditional use)
    // - `foo_opt(f)` takes Option<Fn> (for conditional availability based on inner type)

    /// Set the drop_in_place function. Use `ValueVTable::drop_in_place_for::<T>()`.
    #[inline]
    pub const fn drop_in_place(mut self, f: Option<DropInPlaceFn>) -> Self {
        self.vtable.drop_in_place = f;
        self
    }

    /// Set the invariants function.
    #[inline]
    pub const fn invariants(mut self, f: InvariantsFn) -> Self {
        self.vtable.invariants = Some(f);
        self
    }

    /// Set the default_in_place function.
    #[inline]
    pub const fn default_in_place(mut self, f: DefaultInPlaceFn) -> Self {
        self.vtable.default_in_place = Some(f);
        self
    }

    /// Conditionally set default_in_place.
    #[inline]
    pub const fn default_in_place_opt(mut self, f: Option<DefaultInPlaceFn>) -> Self {
        self.vtable.default_in_place = f;
        self
    }

    /// Set the clone_into function.
    #[inline]
    pub const fn clone_into(mut self, f: CloneIntoFn) -> Self {
        self.vtable.clone_into = Some(f);
        self
    }

    /// Conditionally set clone_into.
    #[inline]
    pub const fn clone_into_opt(mut self, f: Option<CloneIntoFn>) -> Self {
        self.vtable.clone_into = f;
        self
    }

    /// Set the parse function.
    #[inline]
    pub const fn parse(mut self, f: ParseFn) -> Self {
        self.vtable.parse = Some(f);
        self
    }

    /// Set the try_from function.
    #[inline]
    pub const fn try_from(mut self, f: TryFromFn) -> Self {
        self.vtable.try_from = Some(f);
        self
    }

    /// Set the try_into_inner function.
    #[inline]
    pub const fn try_into_inner(mut self, f: TryIntoInnerFn) -> Self {
        self.vtable.try_into_inner = Some(f);
        self
    }

    /// Set the try_borrow_inner function.
    #[inline]
    pub const fn try_borrow_inner(mut self, f: TryBorrowInnerFn) -> Self {
        self.vtable.try_borrow_inner = Some(f);
        self
    }

    /// Set the display function.
    #[inline]
    pub const fn display(mut self, f: DisplayFn) -> Self {
        self.vtable.format.display = Some(f);
        self
    }

    /// Conditionally set display.
    #[inline]
    pub const fn display_opt(mut self, f: Option<DisplayFn>) -> Self {
        self.vtable.format.display = f;
        self
    }

    /// Set the debug function.
    #[inline]
    pub const fn debug(mut self, f: DebugFn) -> Self {
        self.vtable.format.debug = Some(f);
        self
    }

    /// Conditionally set debug.
    #[inline]
    pub const fn debug_opt(mut self, f: Option<DebugFn>) -> Self {
        self.vtable.format.debug = f;
        self
    }

    /// Set the partial_eq function.
    #[inline]
    pub const fn partial_eq(mut self, f: PartialEqFn) -> Self {
        self.vtable.cmp.partial_eq = Some(f);
        self
    }

    /// Conditionally set partial_eq.
    #[inline]
    pub const fn partial_eq_opt(mut self, f: Option<PartialEqFn>) -> Self {
        self.vtable.cmp.partial_eq = f;
        self
    }

    /// Set the partial_ord function.
    #[inline]
    pub const fn partial_ord(mut self, f: PartialOrdFn) -> Self {
        self.vtable.cmp.partial_ord = Some(f);
        self
    }

    /// Conditionally set partial_ord.
    #[inline]
    pub const fn partial_ord_opt(mut self, f: Option<PartialOrdFn>) -> Self {
        self.vtable.cmp.partial_ord = f;
        self
    }

    /// Set the ord function.
    #[inline]
    pub const fn ord(mut self, f: CmpFn) -> Self {
        self.vtable.cmp.ord = Some(f);
        self
    }

    /// Conditionally set ord.
    #[inline]
    pub const fn ord_opt(mut self, f: Option<CmpFn>) -> Self {
        self.vtable.cmp.ord = f;
        self
    }

    /// Set the hash function.
    #[inline]
    pub const fn hash(mut self, f: HashFn) -> Self {
        self.vtable.hash.hash = Some(f);
        self
    }

    /// Conditionally set hash.
    #[inline]
    pub const fn hash_opt(mut self, f: Option<HashFn>) -> Self {
        self.vtable.hash.hash = f;
        self
    }

    /// Set the marker traits.
    #[inline]
    pub const fn markers(mut self, m: MarkerTraits) -> Self {
        self.vtable.markers = m;
        self
    }

    /// Build the Shape.
    ///
    /// If `ty` was not explicitly set, it will be inferred from `def`.
    #[inline]
    pub const fn build(self) -> Shape {
        let ty = match self.ty {
            Some(ty) => ty,
            None => self.def.default_type(),
        };
        Shape {
            id: self.id,
            layout: self.layout,
            vtable: self.vtable,
            ty,
            def: self.def,
            type_identifier: self.type_identifier,
            type_params: self.type_params,
            doc: self.doc,
            attributes: self.attributes,
            type_tag: self.type_tag,
            inner: self.inner,
            proxy: self.proxy,
        }
    }
}
