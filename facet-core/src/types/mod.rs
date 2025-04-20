//! structs and vtable definitions used by Facet

use core::alloc::Layout;

mod characteristic;
pub use characteristic::*;

mod field;
pub use field::*;

mod struct_;
pub use struct_::*;

mod enum_;
pub use enum_::*;

mod array;
pub use array::*;

mod slice;
pub use slice::*;

mod list;
pub use list::*;

mod map;
pub use map::*;

mod value;
pub use value::*;

mod option;
pub use option::*;

mod smartptr;
pub use smartptr::*;

mod scalar;
pub use scalar::*;

use crate::{ConstTypeId, Facet};

/// Schema for reflection of a type
#[derive(Clone, Copy, Debug)]
#[repr(C)]
#[non_exhaustive]
pub struct Shape {
    /// Unique type identifier, provided by the compiler.
    pub id: ConstTypeId,

    /// Size, alignment — enough to allocate a value of this type
    /// (but not initialize it.)
    pub layout: Layout,

    /// Function pointers to perform various operations: print the full type
    /// name (with generic type parameters), use the Display implementation,
    /// the Debug implementation, build a default value, clone, etc.
    ///
    /// There are more specific vtables in variants of [`Def`]
    pub vtable: &'static ValueVTable,

    /// Further definition of the value: details for structs, enums, scalars,
    /// options, smart pointers, arrays, slices, tuples, etc.
    ///
    /// This typically lists fields (with shapes and offsets), reprs, variants
    /// and contains vtables that let you perform other operations, like inserting
    /// into a map or fetching a value from a list.
    pub def: Def,

    /// Generic parameters for the shape
    pub type_params: &'static [TypeParam],

    /// Doc comment lines, collected by facet-derive. Note that they tend to
    /// start with a space.
    pub doc: &'static [&'static str],

    /// Attributes that can be applied to a shape
    pub attributes: &'static [ShapeAttribute],
}

/// An attribute that can be applied to a shape
#[derive(Debug, PartialEq)]
pub enum ShapeAttribute {
    /// Specifies an alternative name for the field (for serialization/deserialization)
    DenyUnknownFields,
    /// Indicates that, when deserializing, fields from this shape that are
    /// missing in the input should be filled with corresponding field values from
    /// a `T::default()` (where T is this shape)
    Default,
    /// Custom field attribute containing arbitrary text
    Arbitrary(&'static str),
}

impl Shape {
    /// Returns a builder for shape
    pub const fn builder() -> ShapeBuilder {
        ShapeBuilder::new()
    }

    /// Check if this shape is of the given type
    pub fn is_type<Other: Facet>(&'static self) -> bool {
        let l = self;
        let r = Other::SHAPE;
        l == r
    }

    /// Assert that this shape is of the given type, panicking if it's not
    pub fn assert_type<Other: Facet>(&'static self) {
        assert!(
            self.is_type::<Other>(),
            "Type mismatch: expected {}, found {self}",
            Other::SHAPE,
        );
    }

    /// See [`ShapeAttribute::DenyUnknownFields`]
    pub fn has_deny_unknown_fields_attr(&'static self) -> bool {
        self.attributes.contains(&ShapeAttribute::DenyUnknownFields)
    }

    /// See [`ShapeAttribute::Default`]
    pub fn has_default_attr(&'static self) -> bool {
        self.attributes.contains(&ShapeAttribute::Default)
    }
}

/// Builder for [`Shape`]
pub struct ShapeBuilder {
    id: Option<ConstTypeId>,
    layout: Option<Layout>,
    vtable: Option<&'static ValueVTable>,
    def: Option<Def>,
    type_params: &'static [TypeParam],
    doc: &'static [&'static str],
    attributes: &'static [ShapeAttribute],
}

impl ShapeBuilder {
    /// Creates a new `ShapeBuilder` with all fields set to `None`.
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {
            id: None,
            layout: None,
            vtable: None,
            def: None,
            type_params: &[],
            doc: &[],
            attributes: &[],
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
        self.layout = Some(layout);
        self
    }

    /// Sets the `vtable` field of the `ShapeBuilder`.
    #[inline]
    pub const fn vtable(mut self, vtable: &'static ValueVTable) -> Self {
        self.vtable = Some(vtable);
        self
    }

    /// Sets the `def` field of the `ShapeBuilder`.
    #[inline]
    pub const fn def(mut self, def: Def) -> Self {
        self.def = Some(def);
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

    /// Builds a `Shape` from the `ShapeBuilder`.
    ///
    /// # Panics
    ///
    /// This method will panic if any of the required fields (`layout`, `vtable`, or `def`) are `None`.
    #[inline]
    pub const fn build(self) -> Shape {
        Shape {
            id: self.id.unwrap(),
            layout: self.layout.unwrap(),
            vtable: self.vtable.unwrap(),
            type_params: self.type_params,
            def: self.def.unwrap(),
            doc: self.doc,
            attributes: self.attributes,
        }
    }
}

impl PartialEq for Shape {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for Shape {}

impl core::hash::Hash for Shape {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.def.hash(state);
        self.layout.hash(state);
    }
}

impl Shape {
    /// Check if this shape is of the given type
    pub fn is_shape(&'static self, other: &'static Shape) -> bool {
        self == other
    }

    /// Assert that this shape is equal to the given shape, panicking if it's not
    pub fn assert_shape(&'static self, other: &'static Shape) {
        assert!(
            self.is_shape(other),
            "Shape mismatch: expected {other}, found {self}",
        );
    }
}

// Helper struct to format the name for display
impl core::fmt::Display for Shape {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        (self.vtable.type_name)(f, TypeNameOpts::default())
    }
}

impl Shape {
    /// Heap-allocate a value of this shape
    #[cfg(feature = "alloc")]
    #[inline]
    pub fn allocate(&self) -> crate::ptr::PtrUninit<'static> {
        crate::ptr::PtrUninit::new(if self.layout.size() == 0 {
            core::ptr::without_provenance_mut(self.layout.align())
        } else {
            // SAFETY: We have checked that layout's size is non-zero
            unsafe { alloc::alloc::alloc(self.layout) }
        })
    }
}
/// The definition of a shape: is it more like a struct, a map, a list?
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
#[non_exhaustive]
// this enum is only ever going to be owned in static space,
// right?
#[allow(clippy::large_enum_variant)]
pub enum Def {
    /// Scalar — those don't have a def, they're not composed of other things.
    /// You can interact with them through [`ValueVTable`].
    ///
    /// e.g. `u32`, `String`, `bool`, `SocketAddr`, etc.
    Scalar(ScalarDef),

    /// Various kinds of structs, see [`StructKind`]
    ///
    /// e.g. `struct Struct { field: u32 }`, `struct TupleStruct(u32, u32);`, `(u32, u32)`
    Struct(Struct),

    /// Enum with variants
    ///
    /// e.g. `enum Enum { Variant1, Variant2 }`
    Enum(EnumDef),

    /// Map — keys are dynamic (and strings, sorry), values are homogeneous
    ///
    /// e.g. `Map<String, T>`
    Map(MapDef),

    /// Ordered list of heterogenous values, variable size
    ///
    /// e.g. `Vec<T>`
    List(ListDef),

    /// Fixed-size array of heterogenous values
    ///
    /// e.g. `[T; 32]`
    Array(ArrayDef),

    /// Slice — a reference to a contiguous sequence of elements
    ///
    /// e.g. `&[T]`
    Slice(SliceDef),

    /// Option
    ///
    /// e.g. `Option<T>`
    Option(OptionDef),

    /// Smart pointers, like `Arc<T>`, `Rc<T>`, etc.
    SmartPointer(SmartPointerDef),
}

/// Represents a lifetime parameter, e.g., `'a` or `'a: 'b + 'c`.
///
/// Note: these are subject to change — it's a bit too stringly-typed for now.
#[derive(Debug, Clone)]
pub struct TypeParam {
    /// The name of the type parameter (e.g., `T`).
    pub name: &'static str,

    /// The shape of the type parameter (e.g. `String`)
    pub shape: fn() -> &'static Shape,
}

impl TypeParam {
    /// Returns the shape of the type parameter.
    pub fn shape(&self) -> &'static Shape {
        (self.shape)()
    }
}
