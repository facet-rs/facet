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
pub enum Type<'shape> {
    /// Built-in primitive.
    Primitive(PrimitiveType),
    /// Sequence (tuple, array, slice).
    Sequence(SequenceType<'shape>),
    /// User-defined type (struct, enum, union).
    User(UserType<'shape>),
    /// Pointer type (reference, raw, function pointer).
    Pointer(PointerType<'shape>),
}

impl core::fmt::Display for Type<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Type::Primitive(_) => {
                // Defer to `Debug`, which correctly produces the intended formatting.
                write!(f, "{self:?}")?;
            }
            Type::Sequence(SequenceType::Array(ArrayType { t, n })) => {
                write!(f, "Sequence(Array([{t}, {n}]))")?;
            }
            Type::Sequence(SequenceType::Slice(SliceType { t })) => {
                write!(f, "Sequence(Slice(&[{t}]))")?;
            }
            Type::User(UserType::Struct(struct_type)) => {
                write!(f, "User(Struct(«kind: {:?}»))", struct_type.kind)?;
            }
            Type::User(UserType::Enum(_enum_type)) => {
                write!(f, "User(Enum(_))")?;
            }
            Type::User(UserType::Union(_union_type)) => {
                write!(f, "User(Union(_))")?;
            }
            Type::User(UserType::Opaque) => {
                write!(f, "User(Opaque)")?;
            }
            Type::Pointer(PointerType::Reference(ptr_type)) => {
                let show_ref = if ptr_type.mutable { "&mut " } else { "&" };
                let target = ptr_type.target();
                write!(f, "Pointer(Reference({show_ref}{target}))")?;
            }
            Type::Pointer(PointerType::Raw(ptr_type)) => {
                let show_raw = if ptr_type.mutable { "*mut " } else { "*const " };
                let target = ptr_type.target();
                write!(f, "Pointer(Raw({show_raw}{target}))")?;
            }
            Type::Pointer(PointerType::Function(_fn_ptr_def)) => {
                write!(f, "Pointer(Function(_))")?;
            }
        }
        Ok(())
    }
}
