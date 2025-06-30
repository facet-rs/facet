use super::{Field, Repr};

/// Common fields for union types
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct UnionType<'shape> {
    /// Representation of the union's data
    pub repr: Repr,

    /// all fields
    pub fields: &'shape [Field<'shape>],
}
