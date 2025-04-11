//! structs and vtable definitions used by Facet

use core::alloc::Layout;
use core::fmt;

mod list;
pub use list::*;

mod map;
pub use map::*;

mod value;
pub use value::*;

mod option;
pub use option::*;

mod scalar_affinities;
pub use scalar_affinities::*;

use crate::{ConstTypeId, Facet};

/// Schema for reflection of a type
#[derive(Clone, Copy, Debug)]
#[repr(C)]
#[non_exhaustive]
pub struct Shape {
    /// Type ID
    pub id: ConstTypeId,

    /// Size, alignment
    pub layout: Layout,

    /// VTable for common operations. This is indirected because the vtable might
    /// have different functions implemented based on generic type parameters:
    /// HashMap<K, V> is not even constructible if `K` is not `Hash` + `Eq`.
    pub vtable: &'static ValueVTable,

    /// Details/contents of the value
    pub def: Def,

    /// Doc comments — if there's multiple lines, they're all concatenated
    pub doc: &'static [&'static str],
}

impl Shape {
    /// Checks if a shape has the given characteristic.
    pub const fn is(&'static self, characteristic: Characteristic) -> bool {
        match characteristic {
            // Marker traits
            Characteristic::Send => self.vtable.marker_traits.contains(MarkerTraits::SEND),
            Characteristic::Sync => self.vtable.marker_traits.contains(MarkerTraits::SYNC),
            Characteristic::Copy => self.vtable.marker_traits.contains(MarkerTraits::COPY),
            Characteristic::Eq => self.vtable.marker_traits.contains(MarkerTraits::EQ),

            // Functionality traits
            Characteristic::Clone => self.vtable.clone_into.is_some(),
            Characteristic::Debug => self.vtable.debug.is_some(),
            Characteristic::PartialEq => self.vtable.eq.is_some(),
            Characteristic::PartialOrd => self.vtable.partial_ord.is_some(),
            Characteristic::Ord => self.vtable.ord.is_some(),
            Characteristic::Hash => self.vtable.hash.is_some(),
            Characteristic::Default => self.vtable.default_in_place.is_some(),
        }
    }

    /// Check if this shape implements the Send trait
    pub const fn is_send(&'static self) -> bool {
        self.is(Characteristic::Send)
    }

    /// Check if this shape implements the Sync trait
    pub const fn is_sync(&'static self) -> bool {
        self.is(Characteristic::Sync)
    }

    /// Check if this shape implements the Copy trait
    pub const fn is_copy(&'static self) -> bool {
        self.is(Characteristic::Copy)
    }

    /// Check if this shape implements the Eq trait
    pub const fn is_eq(&'static self) -> bool {
        self.is(Characteristic::Eq)
    }

    /// Check if this shape implements the Clone trait
    pub const fn is_clone(&'static self) -> bool {
        self.is(Characteristic::Clone)
    }

    /// Check if this shape implements the Debug trait
    pub const fn is_debug(&'static self) -> bool {
        self.is(Characteristic::Debug)
    }

    /// Check if this shape implements the PartialEq trait
    pub const fn is_partial_eq(&'static self) -> bool {
        self.is(Characteristic::PartialEq)
    }

    /// Check if this shape implements the PartialOrd trait
    pub const fn is_partial_ord(&'static self) -> bool {
        self.is(Characteristic::PartialOrd)
    }

    /// Check if this shape implements the Ord trait
    pub const fn is_ord(&'static self) -> bool {
        self.is(Characteristic::Ord)
    }

    /// Check if this shape implements the Hash trait
    pub const fn is_hash(&'static self) -> bool {
        self.is(Characteristic::Hash)
    }

    /// Check if this shape implements the Default trait
    pub const fn is_default(&'static self) -> bool {
        self.is(Characteristic::Default)
    }

    /// Writes the name of this type to the given formatter
    pub fn write_type_name(&self, f: &mut fmt::Formatter<'_>, opts: TypeNameOpts) -> fmt::Result {
        (self.vtable.type_name)(f, opts)
    }

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
}

/// Builder for [`Shape`]
pub struct ShapeBuilder {
    id: Option<ConstTypeId>,
    layout: Option<Layout>,
    vtable: Option<&'static ValueVTable>,
    def: Option<Def>,
    doc: &'static [&'static str],
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
            doc: &[],
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

    /// Sets the `doc` field of the `ShapeBuilder`.
    #[inline]
    pub const fn doc(mut self, doc: &'static [&'static str]) -> Self {
        self.doc = doc;
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
            def: self.def.unwrap(),
            doc: self.doc,
        }
    }
}

impl PartialEq for Shape {
    fn eq(&self, other: &Self) -> bool {
        self.def == other.def && self.layout == other.layout
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
    #[cfg(feature = "std")]
    #[inline]
    pub fn allocate(&self) -> crate::opaque::OpaqueUninit<'static> {
        crate::opaque::OpaqueUninit::new(if self.layout.size() == 0 {
            core::ptr::without_provenance_mut(self.layout.align())
        } else {
            // SAFETY: We have checked that layout's size is non-zero
            unsafe { alloc::alloc::alloc(self.layout) }
        })
    }
}

/// Errors encountered when calling `field_by_index` or `field_by_name`
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum FieldError {
    /// `field_by_index` was called on a dynamic collection, that has no
    /// static fields. a map doesn't have a "first field", it can only
    /// associate by keys.
    NoStaticFields,

    /// `field_by_name` was called on a struct, and there is no static field
    /// with the given key.
    NoSuchStaticField,

    /// `field_by_index` was called on a fixed-size collection (like a tuple,
    /// a struct, or a fixed-size array) and the index was out of bounds.
    IndexOutOfBounds,

    /// `field_by_index` or `field_by_name` was called on a non-struct type.
    NotAStruct,
}

impl core::error::Error for FieldError {}

impl core::fmt::Display for FieldError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            FieldError::NoStaticFields => write!(f, "No static fields available"),
            FieldError::NoSuchStaticField => write!(f, "No such static field"),
            FieldError::IndexOutOfBounds => write!(f, "Index out of bounds"),
            FieldError::NotAStruct => write!(f, "Not a struct"),
        }
    }
}

/// Common fields for struct-like types
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
#[non_exhaustive]
pub struct StructDef {
    /// the kind of struct (e.g. struct, tuple struct, tuple)
    pub kind: StructKind,

    /// all fields, in declaration order (not necessarily in memory order)
    pub fields: &'static [Field],
}

impl StructDef {
    /// Returns a builder for StructDef
    pub const fn builder() -> StructDefBuilder {
        StructDefBuilder::new()
    }
}

/// Builder for StructDef
pub struct StructDefBuilder {
    kind: Option<StructKind>,
    fields: Option<&'static [Field]>,
}

impl StructDefBuilder {
    /// Creates a new StructDefBuilder
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {
            kind: None,
            fields: None,
        }
    }

    /// Sets the kind for the StructDef
    pub const fn kind(mut self, kind: StructKind) -> Self {
        self.kind = Some(kind);
        self
    }

    /// Sets the fields for the StructDef
    pub const fn fields(mut self, fields: &'static [Field]) -> Self {
        self.fields = Some(fields);
        self
    }

    /// Builds the StructDef
    pub const fn build(self) -> StructDef {
        StructDef {
            kind: self.kind.unwrap(),
            fields: self.fields.unwrap(),
        }
    }
}

/// Describes the kind of struct (useful for deserializing)
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
#[non_exhaustive]
pub enum StructKind {
    /// struct S { t0: T0, t1: T1 }
    Struct,

    /// struct TupleStruct(T0, T1);
    TupleStruct,

    /// struct UnitStruct;
    Unit,

    /// (T0, T1)
    Tuple,
}

/// Describes a field in a struct or tuple
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
#[non_exhaustive]
pub struct Field {
    /// key for the struct field (for tuples and tuple-structs, this is the 0-based index)
    pub name: &'static str,

    /// schema of the inner type
    pub shape: &'static Shape,

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
    /// Returns a builder for Field
    pub const fn builder() -> FieldBuilder {
        FieldBuilder::new()
    }
}

/// Builder for Field
pub struct FieldBuilder {
    name: Option<&'static str>,
    shape: Option<&'static Shape>,
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
    /// Custom field attribute containing arbitrary text
    Arbitrary(&'static str),
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
        }
    }

    /// Sets the name for the Field
    pub const fn name(mut self, name: &'static str) -> Self {
        self.name = Some(name);
        self
    }

    /// Sets the shape for the Field
    pub const fn shape(mut self, shape: &'static Shape) -> Self {
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

bitflags::bitflags! {
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

/// Fields for map types
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
#[non_exhaustive]
pub struct MapDef {
    /// vtable for interacting with the map
    pub vtable: &'static MapVTable,
    /// shape of the keys in the map
    pub k: &'static Shape,
    /// shape of the values in the map
    pub v: &'static Shape,
}

impl MapDef {
    /// Returns a builder for MapDef
    pub const fn builder() -> MapDefBuilder {
        MapDefBuilder::new()
    }
}

/// Builder for MapDef
pub struct MapDefBuilder {
    vtable: Option<&'static MapVTable>,
    k: Option<&'static Shape>,
    v: Option<&'static Shape>,
}

impl MapDefBuilder {
    /// Creates a new MapDefBuilder
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {
            vtable: None,
            k: None,
            v: None,
        }
    }

    /// Sets the vtable for the MapDef
    pub const fn vtable(mut self, vtable: &'static MapVTable) -> Self {
        self.vtable = Some(vtable);
        self
    }

    /// Sets the key shape for the MapDef
    pub const fn k(mut self, k: &'static Shape) -> Self {
        self.k = Some(k);
        self
    }

    /// Sets the value shape for the MapDef
    pub const fn v(mut self, v: &'static Shape) -> Self {
        self.v = Some(v);
        self
    }

    /// Builds the MapDef
    pub const fn build(self) -> MapDef {
        MapDef {
            vtable: self.vtable.unwrap(),
            k: self.k.unwrap(),
            v: self.v.unwrap(),
        }
    }
}

/// Fields for list types
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
#[non_exhaustive]
pub struct ListDef {
    /// vtable for interacting with the list
    pub vtable: &'static ListVTable,
    /// shape of the items in the list
    pub t: &'static Shape,
}

impl ListDef {
    /// Returns a builder for ListDef
    pub const fn builder() -> ListDefBuilder {
        ListDefBuilder::new()
    }
}

/// Builder for ListDef
pub struct ListDefBuilder {
    vtable: Option<&'static ListVTable>,
    t: Option<&'static Shape>,
}

impl ListDefBuilder {
    /// Creates a new ListDefBuilder
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {
            vtable: None,
            t: None,
        }
    }

    /// Sets the vtable for the ListDef
    pub const fn vtable(mut self, vtable: &'static ListVTable) -> Self {
        self.vtable = Some(vtable);
        self
    }

    /// Sets the item shape for the ListDef
    pub const fn t(mut self, t: &'static Shape) -> Self {
        self.t = Some(t);
        self
    }

    /// Builds the ListDef
    pub const fn build(self) -> ListDef {
        ListDef {
            vtable: self.vtable.unwrap(),
            t: self.t.unwrap(),
        }
    }
}

/// Fields for option types
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
#[non_exhaustive]
pub struct OptionDef {
    /// vtable for interacting with the option
    pub vtable: &'static OptionVTable,
    /// shape of the inner type of the option
    pub t: &'static Shape,
}

impl OptionDef {
    /// Returns a builder for OptionDef
    pub const fn builder() -> OptionDefBuilder {
        OptionDefBuilder::new()
    }
}

/// Builder for OptionDef
pub struct OptionDefBuilder {
    vtable: Option<&'static OptionVTable>,
    t: Option<&'static Shape>,
}

impl OptionDefBuilder {
    /// Creates a new OptionDefBuilder
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {
            vtable: None,
            t: None,
        }
    }

    /// Sets the vtable for the OptionDef
    pub const fn vtable(mut self, vtable: &'static OptionVTable) -> Self {
        self.vtable = Some(vtable);
        self
    }

    /// Sets the inner type shape for the OptionDef
    pub const fn t(mut self, t: &'static Shape) -> Self {
        self.t = Some(t);
        self
    }

    /// Builds the OptionDef
    pub const fn build(self) -> OptionDef {
        OptionDef {
            vtable: self.vtable.unwrap(),
            t: self.t.unwrap(),
        }
    }
}

/// Fields for enum types
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
#[non_exhaustive]
pub struct EnumDef {
    /// representation of the enum (u8, u16, etc.)
    pub repr: EnumRepr,
    /// all variants for this enum
    pub variants: &'static [Variant],
}

impl EnumDef {
    /// Returns a builder for EnumDef
    pub const fn builder() -> EnumDefBuilder {
        EnumDefBuilder::new()
    }
}

/// Builder for EnumDef
pub struct EnumDefBuilder {
    repr: Option<EnumRepr>,
    variants: Option<&'static [Variant]>,
}

impl EnumDefBuilder {
    /// Creates a new EnumDefBuilder
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {
            repr: None,
            variants: None,
        }
    }

    /// Sets the representation for the EnumDef
    pub const fn repr(mut self, repr: EnumRepr) -> Self {
        self.repr = Some(repr);
        self
    }

    /// Sets the variants for the EnumDef
    pub const fn variants(mut self, variants: &'static [Variant]) -> Self {
        self.variants = Some(variants);
        self
    }

    /// Builds the EnumDef
    pub const fn build(self) -> EnumDef {
        EnumDef {
            repr: self.repr.unwrap(),
            variants: self.variants.unwrap(),
        }
    }
}

/// Describes a variant of an enum
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
#[non_exhaustive]
pub struct Variant {
    /// Name of the variant
    pub name: &'static str,

    /// Discriminant value (if available)
    pub discriminant: Option<i64>,

    /// Kind of variant (unit, tuple, or struct)
    pub kind: VariantKind,

    /// Doc comment for the variant
    pub doc: &'static [&'static str],
}

impl Variant {
    /// Returns a builder for Variant
    pub const fn builder() -> VariantBuilder {
        VariantBuilder::new()
    }
}

/// Builder for Variant
pub struct VariantBuilder {
    name: Option<&'static str>,
    discriminant: Option<Option<i64>>,
    kind: Option<VariantKind>,
    doc: &'static [&'static str],
}

impl VariantBuilder {
    /// Creates a new VariantBuilder
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {
            name: None,
            discriminant: None,
            kind: None,
            doc: &[],
        }
    }

    /// Sets the name for the Variant
    pub const fn name(mut self, name: &'static str) -> Self {
        self.name = Some(name);
        self
    }

    /// Sets the discriminant for the Variant
    pub const fn discriminant(mut self, discriminant: Option<i64>) -> Self {
        self.discriminant = Some(discriminant);
        self
    }

    /// Sets the kind for the Variant
    pub const fn kind(mut self, kind: VariantKind) -> Self {
        self.kind = Some(kind);
        self
    }

    /// Sets the doc comment for the Variant
    pub const fn doc(mut self, doc: &'static [&'static str]) -> Self {
        self.doc = doc;
        self
    }

    /// Builds the Variant
    pub const fn build(self) -> Variant {
        Variant {
            name: self.name.unwrap(),
            discriminant: self.discriminant.unwrap(),
            kind: self.kind.unwrap(),
            doc: self.doc,
        }
    }
}

/// Represents the different kinds of variants that can exist in a Rust enum
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
#[non_exhaustive]
pub enum VariantKind {
    /// Unit variant (e.g., `None` in Option)
    Unit,

    /// Tuple variant with unnamed fields (e.g., `Some(T)` in Option)
    Tuple {
        /// List of fields contained in the tuple variant
        fields: &'static [Field],
    },

    /// Struct variant with named fields (e.g., `Struct { field: T }`)
    Struct {
        /// List of fields contained in the struct variant
        fields: &'static [Field],
    },
}

/// All possible representations for Rust enums
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
#[non_exhaustive]
pub enum EnumRepr {
    /// Default representation (compiler-dependent)
    Default,
    /// u8 representation (#[repr(u8)])
    U8,
    /// u16 representation (#[repr(u16)])
    U16,
    /// u32 representation (#[repr(u32)])
    U32,
    /// u64 representation (#[repr(u64)])
    U64,
    /// usize representation (#[repr(usize)])
    USize,
    /// i8 representation (#[repr(i8)])
    I8,
    /// i16 representation (#[repr(i16)])
    I16,
    /// i32 representation (#[repr(i32)])
    I32,
    /// i64 representation (#[repr(i64)])
    I64,
    /// isize representation (#[repr(isize)])
    ISize,
}

impl Default for EnumRepr {
    fn default() -> Self {
        Self::Default
    }
}

/// Definition for scalar types
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
#[non_exhaustive]
pub struct ScalarDef {
    /// Affinity of the scalar — is spiritually more like a number, more like a string, something else?
    /// example: an IPv4 address is both. good luck.
    pub affinity: ScalarAffinity,
}

impl ScalarDef {
    /// Returns a builder for ScalarDef
    pub const fn builder() -> ScalarDefBuilder {
        ScalarDefBuilder::new()
    }
}

/// Builder for ScalarDef
#[derive(Default)]
pub struct ScalarDefBuilder {
    affinity: Option<ScalarAffinity>,
}

impl ScalarDefBuilder {
    /// Creates a new ScalarDefBuilder
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self { affinity: None }
    }

    /// Sets the affinity for the ScalarDef
    pub const fn affinity(mut self, affinity: ScalarAffinity) -> Self {
        self.affinity = Some(affinity);
        self
    }

    /// Builds the ScalarDef
    pub const fn build(self) -> ScalarDef {
        ScalarDef {
            affinity: self.affinity.unwrap(),
        }
    }
}

/// The definition of a shape: is it more like a struct, a map, a list?
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
#[non_exhaustive]
pub enum Def {
    /// Scalar — those don't have a def, they're not composed of other things.
    /// You can interact with them through [`ValueVTable`].
    ///
    /// e.g. `u32`, `String`, `bool`, `SocketAddr`, etc.
    Scalar(ScalarDef),

    /// Various kinds of structs, see [`StructKind`]
    ///
    /// e.g. `struct Struct { field: u32 }`, `struct TupleStruct(u32, u32);`, `(u32, u32)`
    Struct(StructDef),

    /// Map — keys are dynamic (and strings, sorry), values are homogeneous
    ///
    /// e.g. `Map<String, T>`
    Map(MapDef),

    /// Ordered list of heterogenous values, variable size
    ///
    /// e.g. `Vec<T>`
    List(ListDef),

    /// Enum with variants
    ///
    /// e.g. `enum Enum { Variant1, Variant2 }`
    Enum(EnumDef),

    /// Option
    ///
    /// e.g. `Option<T>`
    Option(OptionDef),
}

/// A characteristic a shape can have
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(C)]
#[non_exhaustive]
pub enum Characteristic {
    // Marker traits
    /// Implements Send
    Send,

    /// Implements Sync
    Sync,

    /// Implements Copy
    Copy,

    /// Implements Eq
    Eq,

    // Functionality traits
    /// Implements Clone
    Clone,

    /// Implements Debug
    Debug,

    /// Implements PartialEq
    PartialEq,

    /// Implements PartialOrd
    PartialOrd,

    /// Implements Ord
    Ord,

    /// Implements Hash
    Hash,

    /// Implements Default
    Default,
}

impl Characteristic {
    /// Checks if all shapes have the given characteristic.
    pub const fn all(self, shapes: &'static [&'static Shape]) -> bool {
        let mut i = 0;
        while i < shapes.len() {
            if !shapes[i].is(self) {
                return false;
            }
            i += 1;
        }
        true
    }

    /// Checks if any shape has the given characteristic.
    pub const fn any(self, shapes: &'static [&'static Shape]) -> bool {
        let mut i = 0;
        while i < shapes.len() {
            if shapes[i].is(self) {
                return true;
            }
            i += 1;
        }
        false
    }

    /// Checks if none of the shapes have the given characteristic.
    pub const fn none(self, shapes: &'static [&'static Shape]) -> bool {
        let mut i = 0;
        while i < shapes.len() {
            if shapes[i].is(self) {
                return false;
            }
            i += 1;
        }
        true
    }
}
