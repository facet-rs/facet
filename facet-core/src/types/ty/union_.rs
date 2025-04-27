use super::{Field, Repr};

/// Common fields for union types
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
#[non_exhaustive]
pub struct UnionType {
    /// Representation of the union's data
    pub repr: Repr,

    /// all fields
    pub fields: &'static [Field],
}
