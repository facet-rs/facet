use std::alloc::Layout;

#[derive(facet::Facet, Clone)]
#[repr(C)]
pub enum PrimitiveType {
    Boolean,
    Numeric(NumericType),
    Textual(TextualType),
    Never,
}

#[derive(facet::Facet, Clone)]
#[repr(C)]
pub enum NumericType {
    Integer { signed: bool },
    Float,
}

#[derive(facet::Facet, Clone)]
#[repr(C)]
pub enum TextualType {
    Char,
    Str,
}

/// Wrapper for std::alloc::Layout that can derive Facet
#[derive(facet::Facet, Clone)]
#[repr(C)]
pub struct LayoutLike {
    pub size: u64,
    pub align: u64,
}

impl From<Layout> for LayoutLike {
    fn from(layout: Layout) -> Self {
        Self {
            size: layout.size() as u64,
            align: layout.align() as u64,
        }
    }
}

#[derive(facet::Facet, Clone)]
#[repr(C)]
pub enum ShapeLayout {
    Sized(LayoutLike),
    Unsized,
}

/// Flags to represent various characteristics of pointers
#[derive(facet::Facet, Clone)]
#[repr(C)]
pub struct PointerFlags {
    pub weak: bool,
    pub atomic: bool,
    pub lock: bool,
}

impl PointerFlags {
    pub const EMPTY: Self = Self {
        weak: false,
        atomic: false,
        lock: false,
    };

    pub const fn from_bits_truncate(bits: u8) -> Self {
        Self {
            weak: (bits & (1 << 0)) != 0,
            atomic: (bits & (1 << 1)) != 0,
            lock: (bits & (1 << 2)) != 0,
        }
    }
}

#[derive(facet::Facet, Clone)]
#[repr(C)]
pub enum KnownPointer {
    Box,
    Rc,
    RcWeak,
    Arc,
    ArcWeak,
    Cow,
    Pin,
    Cell,
    RefCell,
    OnceCell,
    Mutex,
    RwLock,
    OnceLock,
    LazyLock,
    NonNull,
    SharedReference,
    ExclusiveReference,
}

#[derive(facet::Facet, Clone)]
#[repr(C)]
pub enum FunctionAbi {
    C,
    Rust,
    Unknown,
}

#[derive(facet::Facet, Clone)]
#[repr(C)]
pub enum BaseRepr {
    Rust,
    C,
    Transparent,
}

#[derive(facet::Facet, Clone)]
#[repr(C)]
pub struct ReprLike {
    pub base: BaseRepr,
    pub packed: bool,
}

impl Default for ReprLike {
    fn default() -> Self {
        Self {
            base: BaseRepr::Rust,
            packed: false,
        }
    }
}

impl From<facet_core::Repr> for ReprLike {
    fn from(repr: facet_core::Repr) -> Self {
        use facet_core::BaseRepr as CoreBaseRepr;
        Self {
            base: match repr.base {
                CoreBaseRepr::Rust => BaseRepr::Rust,
                CoreBaseRepr::C => BaseRepr::C,
                CoreBaseRepr::Transparent => BaseRepr::Transparent,
            },
            packed: repr.packed,
        }
    }
}

#[derive(facet::Facet, Clone)]
#[repr(C)]
pub enum StructKindLike {
    Unit,
    TupleStruct,
    Struct,
    Tuple,
}

impl From<facet_core::StructKind> for StructKindLike {
    fn from(kind: facet_core::StructKind) -> Self {
        use facet_core::StructKind as CoreStructKind;
        match kind {
            CoreStructKind::Unit => StructKindLike::Unit,
            CoreStructKind::TupleStruct => StructKindLike::TupleStruct,
            CoreStructKind::Struct => StructKindLike::Struct,
            CoreStructKind::Tuple => StructKindLike::Tuple,
        }
    }
}

#[derive(facet::Facet, Clone)]
#[repr(C)]
pub enum EnumReprLike {
    Rust,
    RustNPO,
    U8,
    U16,
    U32,
    U64,
    USize,
    I8,
    I16,
    I32,
    I64,
    ISize,
}

impl From<facet_core::EnumRepr> for EnumReprLike {
    fn from(repr: facet_core::EnumRepr) -> Self {
        use facet_core::EnumRepr as CoreEnumRepr;
        match repr {
            CoreEnumRepr::Rust => EnumReprLike::Rust,
            CoreEnumRepr::RustNPO => EnumReprLike::RustNPO,
            CoreEnumRepr::U8 => EnumReprLike::U8,
            CoreEnumRepr::U16 => EnumReprLike::U16,
            CoreEnumRepr::U32 => EnumReprLike::U32,
            CoreEnumRepr::U64 => EnumReprLike::U64,
            CoreEnumRepr::USize => EnumReprLike::USize,
            CoreEnumRepr::I8 => EnumReprLike::I8,
            CoreEnumRepr::I16 => EnumReprLike::I16,
            CoreEnumRepr::I32 => EnumReprLike::I32,
            CoreEnumRepr::I64 => EnumReprLike::I64,
            CoreEnumRepr::ISize => EnumReprLike::ISize,
        }
    }
}
