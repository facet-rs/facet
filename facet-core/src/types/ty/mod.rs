use super::*;

mod field;
pub use field::*;

mod struct_;
pub use struct_::*;

mod enum_;
pub use enum_::*;

mod union_;
pub use union_::*;

mod primitive;
pub use primitive::*;

mod sequence;
pub use sequence::*;

mod user;
pub use user::*;

mod pointer;
pub use pointer::*;

/// The definition of a shape in accordance to rust reference:
///
/// See <https://doc.rust-lang.org/reference/types.html>
#[derive(Clone, Copy, Debug)]
#[repr(C)]
#[non_exhaustive]
pub enum Type {
    /// Built-in primitive.
    Primitive(PrimitiveType),
    /// Sequence (tuple, array, slice).
    Sequence(SequenceType),
    /// User-defined type (struct, enum, union).
    User(UserType),
    /// Pointer type (reference, raw, function pointer).
    Pointer(PointerType),
}
