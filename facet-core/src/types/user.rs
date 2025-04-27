use super::{EnumDef, StructDef, UnionType};

/// User-defined types (structs, enums, unions)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct UserType {
    pub repr: Repr,
    pub subtype: UserSubtype,
}

impl UserType {
    // TODO: remove this after we migrate away from original `Def`
    pub const fn opaque() -> Self {
        Self {
            repr: Repr::default(),
            subtype: UserSubtype::Opaque,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub enum UserSubtype {
    /// Describes a `struct`
    Struct(StructDef),
    /// Describes an `enum`
    Enum(EnumDef),
    /// Describes a `union`
    Union(UnionType),
    /// Special variant for representing external types with unknown internal representation.
    Opaque,
}

/// Describes base representation of the type
///
/// Is the structure packed, is it laid out like a C struct, is it a transparent wrapper?
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(C)]
pub struct Repr {
    /// Describes base layout representation of the type
    pub base: BaseRepr,
    /// Are the values tightly packed?
    ///
    /// Note, that if struct is packed, the underlying values may not be aligned, and it is
    /// undefined behavior to interact with unaligned values - first copy the value to aligned
    /// buffer, before interacting with it (but first, make sure it is `Copy`!)
    pub packed: bool,
    pub align: Option<usize>,
}

impl Repr {
    /// Create default representation for a user type
    ///
    /// This will be Rust representation with no packing
    pub const fn default() -> Self {
        Self {
            base: BaseRepr::Rust,
            packed: false,
            align: None,
        }
    }
}

/// Underlying byte layout representation
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(C)]
pub enum BaseRepr {
    /// `#[repr(C)]`
    C,
    /// `#[repr(Rust)]` / no attribute
    #[default]
    Rust,
    /// `#[repr(transparent)]`
    Transparent,
}
