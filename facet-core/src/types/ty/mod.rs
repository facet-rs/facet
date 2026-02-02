use core::fmt;

use super::*;

mod field;
pub use field::*;

mod proxy;
pub use proxy::*;

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
pub enum Type {
    /// Undefined type - used as default in ShapeBuilder.
    Undefined,
    /// Built-in primitive.
    Primitive(PrimitiveType),
    /// Sequence (array, slice).
    Sequence(SequenceType),
    /// User-defined type (struct, enum, union).
    User(UserType),
    /// Pointer type (reference, raw, function pointer).
    Pointer(PointerType),
}

impl Type {
    /// Returns the kind of this type as a string, for use in `DeclId` computation.
    ///
    /// This is used when auto-computing `DeclId` for foreign generic types.
    #[inline]
    pub const fn kind_str(&self) -> &'static str {
        match self {
            Type::Undefined => "undefined",
            Type::Primitive(_) => "primitive",
            Type::Sequence(s) => match s {
                SequenceType::Array(_) => "array",
                SequenceType::Slice(_) => "slice",
            },
            Type::User(u) => match u {
                UserType::Struct(_) => "struct",
                UserType::Enum(_) => "enum",
                UserType::Union(_) => "union",
                UserType::Opaque => "opaque",
            },
            Type::Pointer(p) => match p {
                PointerType::Reference(_) => "ref",
                PointerType::Raw(_) => "ptr",
                PointerType::Function(_) => "fn",
            },
        }
    }

    /// Create a builder for a struct type.
    ///
    /// # Example
    /// ```ignore
    /// let ty = Type::struct_builder(StructKind::Struct, &FIELDS)
    ///     .repr(Repr::c())
    ///     .build();
    /// ```
    #[inline]
    pub const fn struct_builder(kind: StructKind, fields: &'static [Field]) -> TypeStructBuilder {
        TypeStructBuilder(StructTypeBuilder::new(kind, fields))
    }

    /// Create a builder for an enum type.
    ///
    /// # Example
    /// ```ignore
    /// let ty = Type::enum_builder(EnumRepr::U8, &VARIANTS)
    ///     .repr(Repr::c())
    ///     .build();
    /// ```
    #[inline]
    pub const fn enum_builder(
        enum_repr: EnumRepr,
        variants: &'static [Variant],
    ) -> TypeEnumBuilder {
        TypeEnumBuilder(EnumTypeBuilder::new(enum_repr, variants))
    }

    /// Create a builder for a union type.
    ///
    /// # Example
    /// ```ignore
    /// let ty = Type::union_builder(&FIELDS)
    ///     .repr(Repr::c())
    ///     .build();
    /// ```
    #[inline]
    pub const fn union_builder(fields: &'static [Field]) -> TypeUnionBuilder {
        TypeUnionBuilder(UnionTypeBuilder::new(fields))
    }
}

/// Builder that produces `Type::User(UserType::Struct(...))`.
#[derive(Clone, Copy, Debug)]
pub struct TypeStructBuilder(StructTypeBuilder);

impl TypeStructBuilder {
    /// Set the representation for the struct type.
    #[inline]
    pub const fn repr(self, repr: Repr) -> Self {
        Self(self.0.repr(repr))
    }

    /// Build the final `Type`.
    #[inline]
    pub const fn build(self) -> Type {
        Type::User(UserType::Struct(self.0.build()))
    }
}

/// Builder that produces `Type::User(UserType::Enum(...))`.
#[derive(Clone, Copy, Debug)]
pub struct TypeEnumBuilder(EnumTypeBuilder);

impl TypeEnumBuilder {
    /// Set the representation for the enum type.
    #[inline]
    pub const fn repr(self, repr: Repr) -> Self {
        Self(self.0.repr(repr))
    }

    /// Build the final `Type`.
    #[inline]
    pub const fn build(self) -> Type {
        Type::User(UserType::Enum(self.0.build()))
    }
}

/// Builder that produces `Type::User(UserType::Union(...))`.
#[derive(Clone, Copy, Debug)]
pub struct TypeUnionBuilder(UnionTypeBuilder);

impl TypeUnionBuilder {
    /// Set the representation for the union type.
    #[inline]
    pub const fn repr(self, repr: Repr) -> Self {
        Self(self.0.repr(repr))
    }

    /// Build the final `Type`.
    #[inline]
    pub const fn build(self) -> Type {
        Type::User(UserType::Union(self.0.build()))
    }
}

// This implementation of `Display` is user-facing output, where the users are developers.
// It is intended to show structure up to a certain depth, but for readability and brevity,
// some complicated types have custom formatting surrounded by guillemet characters
// (`«` and `»`) to indicate divergence from AST.
impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Undefined => {
                write!(f, "Undefined")?;
            }
            Type::Primitive(_) => {
                // Defer to `Debug`, which correctly produces the intended formatting.
                write!(f, "{self:?}")?;
            }
            Type::Sequence(SequenceType::Array(ArrayType { t, n })) => {
                write!(f, "Sequence(Array(«[{t}; {n}]»))")?;
            }
            Type::Sequence(SequenceType::Slice(SliceType { t })) => {
                write!(f, "Sequence(Slice(«&[{t}]»))")?;
            }
            Type::User(UserType::Struct(struct_type)) => {
                struct __Display<'a>(&'a StructType);
                impl fmt::Display for __Display<'_> {
                    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                        write!(f, "«")?;
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
                        // Only show the `#[repr(_)]` if it's not `Rust` (unless it's `repr(packed)`).
                        if let BaseRepr::C = self.0.repr.base {
                            if self.0.repr.packed {
                                // If there are multiple `repr` hints, display as a parenthesized list.
                                write!(f, ", repr: (C, packed)")?;
                            } else {
                                write!(f, ", repr: C")?;
                            }
                        } else if let BaseRepr::Transparent = self.0.repr.base {
                            write!(f, ", repr: transparent")?;
                            // Verbatim compiler error:
                            assert!(
                                !self.0.repr.packed,
                                "transparent struct cannot have other repr hints"
                            );
                        } else if self.0.repr.packed {
                            // This is potentially meaningless, but we'll show it anyway.
                            // In this circumstance, you can assume it's `repr(Rust)`.
                            write!(f, ", repr: packed")?;
                        }
                        write!(f, "»")
                    }
                }
                let show_struct = __Display(struct_type);
                write!(f, "User(Struct({show_struct}))")?;
            }
            Type::User(UserType::Enum(enum_type)) => {
                struct __Display<'a>(&'a EnumType);
                impl<'a> fmt::Display for __Display<'a> {
                    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                        write!(f, "«")?;
                        write!(f, "variants: (")?;
                        let mut variants_iter = self.0.variants.iter();
                        if let Some(variant) = variants_iter.next() {
                            write!(f, "{}", variant.name)?;
                            for variant in variants_iter {
                                write!(f, ", {}", variant.name)?;
                            }
                        }
                        write!(f, ")")?;
                        // Only show the `#[repr(_)]` if it's not `Rust`.
                        if let BaseRepr::C = self.0.repr.base {
                            // TODO: `EnumRepr` should probably be optional, and contain the fields of `Repr`.
                            // I think it is wrong to have both `Repr` and `EnumRepr` in the same type,
                            // since that allows constructing impossible states.
                            let repr_ty = match self.0.enum_repr {
                                EnumRepr::Rust => unreachable!(
                                    "default Rust repr is not valid for `repr(C)` enums"
                                ),
                                EnumRepr::RustNPO => unreachable!(
                                    "null-pointer optimization is only valid for `repr(Rust)`"
                                ),
                                EnumRepr::U8 => "u8",
                                EnumRepr::U16 => "u16",
                                EnumRepr::U32 => "u32",
                                EnumRepr::U64 => "u64",
                                EnumRepr::USize => "usize",
                                EnumRepr::I8 => "i8",
                                EnumRepr::I16 => "i16",
                                EnumRepr::I32 => "i32",
                                EnumRepr::I64 => "i64",
                                EnumRepr::ISize => "isize",
                            };
                            // If there are multiple `repr` hints, display as a parenthesized list.
                            write!(f, ", repr: (C, {repr_ty})")?;
                        } else if let BaseRepr::Transparent = self.0.repr.base {
                            // Extra variant hints are not supported for `repr(transparent)`.
                            write!(f, ", repr: transparent")?;
                        }
                        // Verbatim compiler error:
                        assert!(
                            !self.0.repr.packed,
                            "attribute should be applied to a struct or union"
                        );
                        write!(f, "»")
                    }
                }
                let show_enum = __Display(enum_type);
                write!(f, "User(Enum({show_enum}))")?;
            }
            Type::User(UserType::Union(union_type)) => {
                struct __Display<'a>(&'a UnionType);
                impl<'a> fmt::Display for __Display<'a> {
                    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                        write!(f, "«")?;
                        write!(f, "fields: (")?;
                        let mut fields_iter = self.0.fields.iter();
                        if let Some(field) = fields_iter.next() {
                            write!(f, "{}", field.name)?;
                            for field in fields_iter {
                                write!(f, ", {}", field.name)?;
                            }
                        }
                        write!(f, ")")?;
                        // Only show the `#[repr(_)]` if it's not `Rust` (unless it's `repr(packed)`).
                        if let BaseRepr::C = self.0.repr.base {
                            if self.0.repr.packed {
                                // If there are multiple `repr` hints, display as a parenthesized list.
                                write!(f, ", repr: (C, packed)")?;
                            } else {
                                write!(f, ", repr: C")?;
                            }
                        } else if let BaseRepr::Transparent = self.0.repr.base {
                            // Nothing needs to change if `transparent_unions` is stabilized.
                            // <https://github.com/rust-lang/rust/issues/60405>
                            write!(f, ", repr: transparent")?;
                            // Verbatim compiler error:
                            assert!(
                                !self.0.repr.packed,
                                "transparent union cannot have other repr hints"
                            );
                        } else if self.0.repr.packed {
                            // Here `Rust` is displayed because a lint asks you to specify explicitly,
                            // despite the fact that `repr(Rust)` is the default.
                            write!(f, ", repr: (Rust, packed)")?;
                        }
                        write!(f, "»")?;
                        Ok(())
                    }
                }
                let show_union = __Display(union_type);
                write!(f, "User(Union({show_union}))")?;
            }
            Type::User(UserType::Opaque) => {
                write!(f, "User(Opaque)")?;
            }
            Type::Pointer(PointerType::Reference(ptr_type)) => {
                let show_ref = if ptr_type.mutable { "&mut " } else { "&" };
                let target = ptr_type.target();
                write!(f, "Pointer(Reference(«{show_ref}{target}»))")?;
            }
            Type::Pointer(PointerType::Raw(ptr_type)) => {
                let show_raw = if ptr_type.mutable { "*mut " } else { "*const " };
                let target = ptr_type.target();
                write!(f, "Pointer(Raw(«{show_raw}{target}»))")?;
            }
            Type::Pointer(PointerType::Function(fn_ptr_def)) => {
                struct __Display<'a>(&'a FunctionPointerDef);
                impl fmt::Display for __Display<'_> {
                    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                        write!(f, "«")?;
                        write!(f, "fn(")?;
                        let mut args_iter = self.0.parameters.iter();
                        if let Some(arg) = args_iter.next() {
                            write!(f, "{arg}")?;
                            for arg in args_iter {
                                write!(f, ", {arg}")?;
                            }
                        }
                        let ret_ty = self.0.return_type;
                        write!(f, ") -> {ret_ty}")?;
                        write!(f, "»")?;
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
