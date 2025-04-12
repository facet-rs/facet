use super::Field;

/// Fields for enum types
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
#[non_exhaustive]
pub struct EnumDef {
    /// representation of the enum (u8, u16, etc.)
    pub repr: EnumRepr,
    /// all variants for this enum
    pub variants: &'static [Variant],
}

impl EnumDef {
    /// Returns a builder for EnumDef
    pub const fn builder() -> EnumDefBuilder {
        EnumDefBuilder::new()
    }
}

/// Builder for EnumDef
pub struct EnumDefBuilder {
    repr: Option<EnumRepr>,
    variants: Option<&'static [Variant]>,
}

impl EnumDefBuilder {
    /// Creates a new EnumDefBuilder
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {
            repr: None,
            variants: None,
        }
    }

    /// Sets the representation for the EnumDef
    pub const fn repr(mut self, repr: EnumRepr) -> Self {
        self.repr = Some(repr);
        self
    }

    /// Sets the variants for the EnumDef
    pub const fn variants(mut self, variants: &'static [Variant]) -> Self {
        self.variants = Some(variants);
        self
    }

    /// Builds the EnumDef
    pub const fn build(self) -> EnumDef {
        EnumDef {
            repr: self.repr.unwrap(),
            variants: self.variants.unwrap(),
        }
    }
}

/// Describes a variant of an enum
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
#[non_exhaustive]
pub struct Variant {
    /// Name of the variant
    pub name: &'static str,

    /// Discriminant value (if available)
    pub discriminant: Option<i64>,

    /// Kind of variant (unit, tuple, or struct)
    pub kind: VariantKind,

    /// Offset of the variant in the enum layout
    pub offset: usize,

    /// Doc comment for the variant
    pub doc: &'static [&'static str],
}

impl Variant {
    /// Returns a builder for Variant
    pub const fn builder() -> VariantBuilder {
        VariantBuilder::new()
    }
}

/// Builder for Variant
pub struct VariantBuilder {
    name: Option<&'static str>,
    discriminant: Option<Option<i64>>,
    kind: Option<VariantKind>,
    offset: Option<usize>,
    doc: &'static [&'static str],
}

impl VariantBuilder {
    /// Creates a new VariantBuilder
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {
            name: None,
            discriminant: None,
            kind: None,
            offset: None,
            doc: &[],
        }
    }

    /// Sets the name for the Variant
    pub const fn name(mut self, name: &'static str) -> Self {
        self.name = Some(name);
        self
    }

    /// Sets the discriminant for the Variant
    pub const fn discriminant(mut self, discriminant: Option<i64>) -> Self {
        self.discriminant = Some(discriminant);
        self
    }

    /// Sets the kind for the Variant
    pub const fn kind(mut self, kind: VariantKind) -> Self {
        self.kind = Some(kind);
        self
    }

    /// Sets the offset for the Variant
    pub const fn offset(mut self, offset: usize) -> Self {
        self.offset = Some(offset);
        self
    }

    /// Sets the doc comment for the Variant
    pub const fn doc(mut self, doc: &'static [&'static str]) -> Self {
        self.doc = doc;
        self
    }

    /// Builds the Variant
    pub const fn build(self) -> Variant {
        Variant {
            name: self.name.unwrap(),
            discriminant: self.discriminant.unwrap(),
            kind: self.kind.unwrap(),
            offset: self.offset.unwrap(),
            doc: self.doc,
        }
    }
}

/// Represents the different kinds of variants that can exist in a Rust enum
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
#[non_exhaustive]
pub enum VariantKind {
    /// Unit variant (e.g., `None` in Option)
    Unit,

    /// Tuple variant with unnamed fields (e.g., `Some(T)` in Option)
    Tuple {
        /// List of fields contained in the tuple variant
        fields: &'static [Field],
    },

    /// Struct variant with named fields (e.g., `Struct { field: T }`)
    Struct {
        /// List of fields contained in the struct variant
        fields: &'static [Field],
    },
}

/// All possible representations for Rust enums
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
#[non_exhaustive]
pub enum EnumRepr {
    /// u8 representation (#[repr(u8)])
    U8,
    /// u16 representation (#[repr(u16)])
    U16,
    /// u32 representation (#[repr(u32)])
    U32,
    /// u64 representation (#[repr(u64)])
    U64,
    /// usize representation (#[repr(usize)])
    USize,
    /// i8 representation (#[repr(i8)])
    I8,
    /// i16 representation (#[repr(i16)])
    I16,
    /// i32 representation (#[repr(i32)])
    I32,
    /// i64 representation (#[repr(i64)])
    I64,
    /// isize representation (#[repr(isize)])
    ISize,
}

impl EnumRepr {
    /// Returns the enum representation for the given discriminant type
    ///
    /// NOTE: only supports unsigned discriminants
    ///
    /// # Panics
    ///
    /// Panics if the size of the discriminant size is not 1, 2, 4, or 8 bytes.
    pub const fn from_discriminant_size<T>() -> Self {
        match core::mem::size_of::<T>() {
            1 => EnumRepr::U8,
            2 => EnumRepr::U16,
            4 => EnumRepr::U32,
            8 => EnumRepr::U64,
            _ => panic!("Invalid enum size"),
        }
    }
}
