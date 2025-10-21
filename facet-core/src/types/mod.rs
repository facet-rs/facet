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

/// An attribute that can be applied to a shape
#[derive(Debug, PartialEq)]
pub enum ShapeAttribute {
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
    RenameAll(&'static str),
    /// Custom field attribute containing arbitrary text
    Arbitrary(&'static str),
}

impl Shape {
    /// Returns a builder for a shape for some type `T`.
    pub const fn builder_for_sized<T>() -> ShapeBuilder {
        ShapeBuilder::new()
            .layout(Layout::new::<T>())
            .id(ConstTypeId::of::<T>())
    }

    /// Returns a builder for a shape for some type `T`.
    pub const fn builder_for_unsized<T: ?Sized>() -> ShapeBuilder {
        ShapeBuilder::new().set_unsized().id(ConstTypeId::of::<T>())
    }

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

    /// See [`ShapeAttribute::DenyUnknownFields`]
    #[inline]
    pub fn has_deny_unknown_fields_attr(&self) -> bool {
        self.attributes.contains(&ShapeAttribute::DenyUnknownFields)
    }

    /// See [`ShapeAttribute::Default`]
    #[inline]
    pub fn has_default_attr(&self) -> bool {
        self.attributes.contains(&ShapeAttribute::Default)
    }

    /// See [`ShapeAttribute::RenameAll`]
    #[inline]
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
pub struct ShapeBuilder {
    id: Option<ConstTypeId>,
    layout: Option<ShapeLayout>,
    vtable: Option<ValueVTable>,
    def: Def,
    ty: Option<Type>,
    type_identifier: Option<&'static str>,
    type_params: &'static [TypeParam],
    doc: &'static [&'static str],
    attributes: &'static [ShapeAttribute],
    type_tag: Option<&'static str>,
    inner: Option<&'static Shape>,
}

impl ShapeBuilder {
    /// Creates a new `ShapeBuilder` with all fields set to `None`.
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {
            id: None,
            layout: None,
            vtable: None,
            def: Def::Undefined,
            ty: None,
            type_identifier: None,
            type_params: &[],
            doc: &[],
            attributes: &[],
            type_tag: None,
            inner: None,
        }
    }

    /// Sets the `id` field of the `ShapeBuilder`.
    #[inline]
    pub const fn id(mut self, id: ConstTypeId) -> Self {
        self.id = Some(id);
        self
    }

    /// Sets the `vtable` field of the `ShapeBuilder`.
    #[inline]
    pub const fn vtable(mut self, vtable: ValueVTable) -> Self {
        self.vtable = Some(vtable);
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
    pub const fn def(mut self, def: Def) -> Self {
        self.def = def;
        self
    }

    /// Sets the `ty` field of the `ShapeBuilder`.
    #[inline]
    pub const fn ty(mut self, ty: Type) -> Self {
        self.ty = Some(ty);
        self
    }

    /// Sets the `type_identifier` field of the `ShapeBuilder`.
    #[inline]
    pub const fn type_identifier(mut self, type_identifier: &'static str) -> Self {
        self.type_identifier = Some(type_identifier);
        self
    }

    /// Sets the `type_params` field of the `ShapeBuilder`.
    #[inline]
    pub const fn type_params(mut self, type_params: &'static [TypeParam]) -> Self {
        self.type_params = type_params;
        self
    }

    /// Sets the `doc` field of the `ShapeBuilder`.
    #[inline]
    pub const fn doc(mut self, doc: &'static [&'static str]) -> Self {
        self.doc = doc;
        self
    }

    /// Sets the `attributes` field of the `ShapeBuilder`.
    #[inline]
    pub const fn attributes(mut self, attributes: &'static [ShapeAttribute]) -> Self {
        self.attributes = attributes;
        self
    }

    /// Sets the `type_tag` field of the `ShapeBuilder`.
    #[inline]
    pub const fn type_tag(mut self, type_tag: &'static str) -> Self {
        self.type_tag = Some(type_tag);
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
    pub const fn inner(mut self, inner_fn: &'static Shape) -> Self {
        self.inner = Some(inner_fn);
        self
    }

    /// Builds a `Shape` from the `ShapeBuilder`.
    ///
    /// # Panics
    ///
    /// This method will panic if any of the required fields (`id`, `layout`, `type_identifier`, or `ty`) are `None`.
    #[inline]
    pub const fn build(self) -> Shape {
        Shape {
            id: self.id.unwrap(),
            layout: self.layout.unwrap(),
            vtable: self.vtable.unwrap(),
            type_identifier: self.type_identifier.unwrap(),
            type_params: self.type_params,
            def: self.def,
            ty: self.ty.unwrap(),
            doc: self.doc,
            attributes: self.attributes,
            type_tag: self.type_tag,
            inner: self.inner,
        }
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
