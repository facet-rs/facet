//! structs and vtable definitions used by Facet

#[cfg(feature = "alloc")]
use crate::PtrMut;

use core::alloc::Layout;

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
#[non_exhaustive]
pub struct Shape<'shape> {
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
    pub vtable: &'shape ValueVTable,

    /// Underlying type: primitive, sequence, user, pointer.
    ///
    /// This follows the [`Rust Reference`](https://doc.rust-lang.org/reference/types.html), but
    /// omits function types, and trait types, as they cannot be represented here.
    pub ty: Type<'shape>,

    /// Functional definition of the value: details for scalars, functions for inserting values into
    /// a map, or fetching a value from a list.
    pub def: Def<'shape>,

    /// Identifier for a type: the type's name without generic parameters. To get the type's full
    /// name with generic parameters, see [`ValueVTable::type_name`].
    pub type_identifier: &'shape str,

    /// Generic parameters for the shape
    pub type_params: &'shape [TypeParam<'shape>],

    /// Doc comment lines, collected by facet-derive. Note that they tend to
    /// start with a space.
    pub doc: &'shape [&'shape str],

    /// Attributes that can be applied to a shape
    pub attributes: &'shape [ShapeAttribute<'shape>],

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
    /// See Wip's `innermost_shape` function (and its support in `put`).
    pub inner: Option<fn() -> &'shape Shape<'shape>>,
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
    pub fn sized_layout(self) -> Result<Layout, UnsizedError> {
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

/// An attribute that can be applied to a shape
#[derive(Debug, PartialEq)]
pub enum ShapeAttribute<'shape> {
    /// Reject deserialization upon encountering an unknown key.
    DenyUnknownFields,
    /// Indicates that, when deserializing, fields from this shape that are
    /// missing in the input should be filled with corresponding field values from
    /// a `T::default()` (where T is this shape)
    Default,
    /// Indicates that this is a transparent wrapper type, like `NewType(T)`
    /// it should not be treated like a struct, but like something that can be built
    /// from `T` and converted back to `T`
    Transparent,
    /// Specifies a case conversion rule for all fields or variants
    RenameAll(&'shape str),
    /// Custom field attribute containing arbitrary text
    Arbitrary(&'shape str),
}

impl<'shape> Shape<'shape> {
    /// Returns a builder for a shape for some type `T`.
    pub const fn builder_for_sized<'a, T: Facet<'a>>() -> ShapeBuilder<'shape> {
        ShapeBuilder::new(T::VTABLE)
            .layout(Layout::new::<T>())
            .id(ConstTypeId::of::<T>())
    }

    /// Returns a builder for a shape for some type `T`.
    pub const fn builder_for_unsized<'a, T: Facet<'a> + ?Sized>() -> ShapeBuilder<'shape> {
        ShapeBuilder::new(T::VTABLE)
            .set_unsized()
            .id(ConstTypeId::of::<T>())
    }

    /// Check if this shape is of the given type
    pub fn is_type<Other: Facet<'shape>>(&self) -> bool {
        let l = self;
        let r = Other::SHAPE;
        l == r
    }

    /// Assert that this shape is of the given type, panicking if it's not
    pub fn assert_type<Other: Facet<'shape>>(&self) {
        assert!(
            self.is_type::<Other>(),
            "Type mismatch: expected {}, found {self}",
            Other::SHAPE,
        );
    }

    /// See [`ShapeAttribute::DenyUnknownFields`]
    pub fn has_deny_unknown_fields_attr(&self) -> bool {
        self.attributes.contains(&ShapeAttribute::DenyUnknownFields)
    }

    /// See [`ShapeAttribute::Default`]
    pub fn has_default_attr(&self) -> bool {
        self.attributes.contains(&ShapeAttribute::Default)
    }

    /// See [`ShapeAttribute::RenameAll`]
    pub fn get_rename_all_attr(&self) -> Option<&str> {
        self.attributes.iter().find_map(|attr| {
            if let ShapeAttribute::RenameAll(rule) = attr {
                Some(*rule)
            } else {
                None
            }
        })
    }
}

/// Builder for [`Shape`]
pub struct ShapeBuilder<'shape> {
    id: Option<ConstTypeId>,
    layout: Option<ShapeLayout>,
    vtable: &'shape ValueVTable,
    def: Def<'shape>,
    ty: Option<Type<'shape>>,
    type_identifier: Option<&'shape str>,
    type_params: &'shape [TypeParam<'shape>],
    doc: &'shape [&'shape str],
    attributes: &'shape [ShapeAttribute<'shape>],
    inner: Option<fn() -> &'shape Shape<'shape>>,
}

impl<'shape> ShapeBuilder<'shape> {
    /// Creates a new `ShapeBuilder` with all fields set to `None`.
    #[allow(clippy::new_without_default)]
    pub const fn new(vtable: &'shape ValueVTable) -> Self {
        Self {
            id: None,
            layout: None,
            vtable,
            def: Def::Undefined,
            ty: None,
            type_identifier: None,
            type_params: &[],
            doc: &[],
            attributes: &[],
            inner: None,
        }
    }

    /// Sets the id field of the `ShapeBuilder`.
    #[inline]
    pub const fn id(mut self, id: ConstTypeId) -> Self {
        self.id = Some(id);
        self
    }

    /// Sets the `layout` field of the `ShapeBuilder`.
    #[inline]
    pub const fn layout(mut self, layout: Layout) -> Self {
        self.layout = Some(ShapeLayout::Sized(layout));
        self
    }

    /// Sets the type as unsized
    #[inline]
    pub const fn set_unsized(mut self) -> Self {
        self.layout = Some(ShapeLayout::Unsized);
        self
    }

    /// Sets the `def` field of the `ShapeBuilder`.
    #[inline]
    pub const fn def(mut self, def: Def<'shape>) -> Self {
        self.def = def;
        self
    }

    /// Sets the `ty` field of the `ShapeBuilder`.
    #[inline]
    pub const fn ty(mut self, ty: Type<'shape>) -> Self {
        self.ty = Some(ty);
        self
    }

    /// Sets the `type_identifier` field of the `ShapeBuilder`.
    #[inline]
    pub const fn type_identifier(mut self, type_identifier: &'shape str) -> Self {
        self.type_identifier = Some(type_identifier);
        self
    }

    /// Sets the `type_params` field of the `ShapeBuilder`.
    #[inline]
    pub const fn type_params(mut self, type_params: &'shape [TypeParam<'shape>]) -> Self {
        self.type_params = type_params;
        self
    }

    /// Sets the `doc` field of the `ShapeBuilder`.
    #[inline]
    pub const fn doc(mut self, doc: &'shape [&'shape str]) -> Self {
        self.doc = doc;
        self
    }

    /// Sets the `attributes` field of the `ShapeBuilder`.
    #[inline]
    pub const fn attributes(mut self, attributes: &'shape [ShapeAttribute]) -> Self {
        self.attributes = attributes;
        self
    }

    /// Sets the `inner` field of the `ShapeBuilder`.
    ///
    /// This indicates that this shape is a transparent wrapper for another shape,
    /// like a newtype or smart pointer, and should be treated as such for serialization
    /// and deserialization.
    ///
    /// The function `inner_fn` should return the static shape of the inner type.
    #[inline]
    pub const fn inner(mut self, inner_fn: fn() -> &'shape Shape<'shape>) -> Self {
        self.inner = Some(inner_fn);
        self
    }

    /// Builds a `Shape` from the `ShapeBuilder`.
    ///
    /// # Panics
    ///
    /// This method will panic if any of the required fields (`layout`, `vtable`, or `def`) are `None`.
    #[inline]
    pub const fn build(self) -> Shape<'shape> {
        Shape {
            id: self.id.unwrap(),
            layout: self.layout.unwrap(),
            vtable: self.vtable,
            type_identifier: self.type_identifier.unwrap(),
            type_params: self.type_params,
            def: self.def,
            ty: self.ty.unwrap(),
            doc: self.doc,
            attributes: self.attributes,
            inner: self.inner,
        }
    }
}

impl PartialEq for Shape<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for Shape<'_> {}

impl core::hash::Hash for Shape<'_> {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
        self.layout.hash(state);
    }
}

impl Shape<'_> {
    /// Check if this shape is of the given type
    pub fn is_shape(&self, other: &Shape<'_>) -> bool {
        self == other
    }

    /// Assert that this shape is equal to the given shape, panicking if it's not
    pub fn assert_shape(&self, other: &Shape<'_>) {
        assert!(
            self.is_shape(other),
            "Shape mismatch: expected {other}, found {self}",
        );
    }
}

// Helper struct to format the name for display
impl core::fmt::Display for Shape<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        (self.vtable.type_name)(f, TypeNameOpts::default())
    }
}

impl core::fmt::Debug for Shape<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let mut debug_struct = f.debug_struct("Shape");

        // Always show the type name
        debug_struct.field("type", &format_args!("{}", self));

        // Show def if it's not Undefined
        if !matches!(self.def, Def::Undefined) {
            debug_struct.field("def", &format_args!("{:?}", self.def));
        }

        // Show inner if present
        if self.inner.is_some() {
            debug_struct.field("inner", &format_args!("Some(..)"));
        }

        debug_struct.finish()
    }
}

impl Shape<'_> {
    /// Heap-allocate a value of this shape
    #[cfg(feature = "alloc")]
    #[inline]
    pub fn allocate(&self) -> Result<crate::ptr::PtrUninit<'static>, UnsizedError> {
        let layout = self.layout.sized_layout()?;

        Ok(crate::ptr::PtrUninit::new(if layout.size() == 0 {
            core::ptr::without_provenance_mut(layout.align())
        } else {
            // SAFETY: We have checked that layout's size is non-zero
            unsafe { alloc::alloc::alloc(layout) }
        }))
    }

    /// Deallocate a heap-allocated value of this shape
    ///
    /// # Safety
    ///
    /// - `ptr` must have been allocated using [`Self::allocate`] and be aligned for this shape.
    /// - `ptr` must point to a region that is not already deallocated.
    #[cfg(feature = "alloc")]
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
pub struct TypeParam<'shape> {
    /// The name of the type parameter (e.g., `T`).
    pub name: &'shape str,

    /// The shape of the type parameter (e.g. `String`)
    pub shape: fn() -> &'shape Shape<'shape>,
}

impl<'shape> TypeParam<'shape> {
    /// Returns the shape of the type parameter.
    pub fn shape(&self) -> &'shape Shape {
        (self.shape)()
    }
}
