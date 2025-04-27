use super::Field;

/// Common fields for union types
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
#[non_exhaustive]
pub struct UnionType {
    /// all fields
    pub fields: &'static [Field],
}
