use super::{EnumType, StructType, UnionType};

/// User-defined types (structs, enums, unions)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub enum UserType {
    /// Describes a `struct`
    Struct(StructType),
    /// Describes an `enum`
    Enum(EnumType),
    /// Describes a `union`
    Union(UnionType),
    /// Special variant for representing external types with unknown internal representation.
    Opaque,
}

impl UserType {
    /// Retrieves underlying representation of the type
    pub const fn repr(&self) -> Option<Repr> {
        match self {
            Self::Struct(s) => Some(s.repr),
            Self::Enum(e) => Some(e.repr),
            Self::Union(u) => Some(u.repr),
            Self::Opaque => None,
        }
    }
}

/// Describes base representation of the type
///
/// Is the structure packed, is it laid out like a C struct, is it a transparent wrapper?
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
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
}

impl Repr {
    /// Create default representation for a user type
    ///
    /// This will be Rust representation with no packing
    pub const fn default() -> Self {
        Self {
            base: BaseRepr::Rust,
            packed: false,
        }
    }

    /// Build unpacked C representation
    pub const fn c() -> Self {
        Self {
            base: BaseRepr::C,
            packed: false,
        }
    }

    /// Builds transparent representation
    pub const fn transparent() -> Self {
        Self {
            base: BaseRepr::Transparent,
            packed: false,
        }
    }
}

/// Underlying byte layout representation
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
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
