use super::{Field, Repr};

/// Common fields for struct-like types
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct StructType {
    /// Representation of the struct's data
    pub repr: Repr,

    /// the kind of struct (e.g. struct, tuple struct, tuple)
    pub kind: StructKind,

    /// all fields, in declaration order (not necessarily in memory order)
    pub fields: &'static [Field],
}

impl StructType {
    /// A unit struct type with default C representation and no fields.
    ///
    /// This is a pre-built constant for the common case of unit enum variants.
    pub const UNIT: Self = Self {
        repr: Repr::C,
        kind: StructKind::Unit,
        fields: &[],
    };
}

/// Describes the kind of struct (useful for deserializing)
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
pub enum StructKind {
    /// struct UnitStruct;
    Unit,

    /// struct TupleStruct(T0, T1);
    TupleStruct,

    /// struct S { foo: T0, bar: T1 }
    Struct,

    /// (T0, T1)
    Tuple,
}

/// Builder for [`StructType`] to enable shorter derive macro output
///
/// # Example
/// ```
/// use facet_core::{StructTypeBuilder, StructKind, Field, StructType};
/// const FIELDS: &[Field] = &[];
/// const STRUCT_TYPE: StructType =
///     StructTypeBuilder::new(StructKind::Struct, FIELDS).build();
/// ```
#[derive(Clone, Copy, Debug)]
pub struct StructTypeBuilder {
    repr: Repr,
    kind: StructKind,
    fields: &'static [Field],
}

impl StructTypeBuilder {
    /// Create a new StructTypeBuilder with the given kind and fields
    ///
    /// The representation defaults to `Repr::c()` if not explicitly set via [`Self::repr`]
    #[inline]
    pub const fn new(kind: StructKind, fields: &'static [Field]) -> Self {
        Self {
            repr: Repr::c(),
            kind,
            fields,
        }
    }

    /// Set the representation for the struct type
    #[inline]
    pub const fn repr(mut self, repr: Repr) -> Self {
        self.repr = repr;
        self
    }

    /// Build the final StructType
    #[inline]
    pub const fn build(self) -> StructType {
        StructType {
            repr: self.repr,
            kind: self.kind,
            fields: self.fields,
        }
    }
}
