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
                struct __Display<'a>(&'a StructType<'a>);
                impl core::fmt::Display for __Display<'_> {
                    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                        write!(f, "«")?; // Guillemet indicates some kind of fake syntax.
                        write!(f, "kind: {:?}", self.0.kind)?;
                        // Field count for `TupleStruct` and `Tuple`, and field names for `Struct`.
                        // For `Unit`, we don't show anything.
                        if let StructKind::Struct = self.0.kind {
                            write!(f, ", fields: (")?;
                            let mut fields_iter = self.0.fields.iter();
                            if let Some(field) = fields_iter.next() {
                                write!(f, "{}", field.name)?;
                                for field in fields_iter {
                                    write!(f, ", {}", field.name)?;
                                }
                            }
                            write!(f, ")")?;
                        } else if let StructKind::TupleStruct | StructKind::Tuple = self.0.kind {
                            write!(f, ", fields: {}", self.0.fields.len())?;
                        }
                        // Only show the `#[repr(_)]` if it's not `Rust`.
                        if let BaseRepr::C = self.0.repr.base {
                            write!(f, ", repr: C")?;
                        } else if let BaseRepr::Transparent = self.0.repr.base {
                            write!(f, ", repr: transparent")?;
                        }
                        // Display as a "flag" if the type is packed.
                        if self.0.repr.packed {
                            write!(f, ", packed")?;
                        }
                        write!(f, "»")
                    }
                }
                let show_struct = __Display(struct_type);
                write!(f, "User(Struct({show_struct}))")?;
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
            Type::Pointer(PointerType::Function(fn_ptr_def)) => {
                struct __Display<'a>(&'a FunctionPointerDef);
                impl core::fmt::Display for __Display<'_> {
                    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                        write!(f, "fn(")?;
                        let mut args_iter = self.0.parameters.iter().map(|f| f());
                        if let Some(arg) = args_iter.next() {
                            write!(f, "{arg}")?;
                            for arg in args_iter {
                                write!(f, ", {arg}")?;
                            }
                        }
                        let ret_ty = (self.0.return_type)();
                        write!(f, ") -> {ret_ty}")?;
                        Ok(())
                    }
                }
                let show_fn = __Display(fn_ptr_def);
                write!(f, "Pointer(Function({show_fn}))")?;
            }
        }
        Ok(())
    }
}
