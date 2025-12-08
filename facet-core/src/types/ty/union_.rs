use super::{Field, Repr};

/// Common fields for union types
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct UnionType {
    /// Representation of the union's data
    pub repr: Repr,

    /// all fields
    pub fields: &'static [Field],
}

/// Builder for constructing [`UnionType`] instances in const contexts.
///
/// # Example
///
/// ```
/// use facet_core::{UnionTypeBuilder, Repr, UnionType, Field};
///
/// const FIELDS: &[Field] = &[];
/// const UNION: UnionType = UnionTypeBuilder::new(FIELDS)
///     .repr(Repr::c())
///     .build();
/// ```
#[derive(Clone, Copy, Debug)]
pub struct UnionTypeBuilder {
    repr: Repr,
    fields: &'static [Field],
}

impl UnionTypeBuilder {
    /// Creates a new `UnionTypeBuilder` with the given fields.
    ///
    /// The representation defaults to `Repr::c()` if not explicitly set.
    #[inline]
    pub const fn new(fields: &'static [Field]) -> Self {
        Self {
            repr: Repr::c(),
            fields,
        }
    }

    /// Sets the representation for the union type.
    #[inline]
    pub const fn repr(mut self, repr: Repr) -> Self {
        self.repr = repr;
        self
    }

    /// Builds the final [`UnionType`] instance.
    #[inline]
    pub const fn build(self) -> UnionType {
        UnionType {
            repr: self.repr,
            fields: self.fields,
        }
    }
}
