//! The schema model — phon's wire-shape vocabulary.
//!
//! These are direct transcriptions of the type system in `docs/content/spec.md`.
//! They are the *resolved* form: every `SchemaRef::Concrete` carries the real,
//! content-derived [`SchemaId`] of its target (see [`crate::identity`]).
//!
//! No `#[derive(Facet)]` here — this crate is reflection-free
//! (`r[crates.engine-is-binding-free]`); the `phon` front door owns the facet
//! bridge.

use core::fmt;

/// A content-derived schema identifier: the first 8 bytes of the BLAKE3 hash of
/// the schema's canonical structural encoding, read as a little-endian `u64`.
///
/// Spec: `r[schema-identity.content-hash]`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SchemaId(pub u64);

impl fmt::Debug for SchemaId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SchemaId({:#018x})", self.0)
    }
}

impl fmt::Display for SchemaId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:016x}", self.0)
    }
}

/// A schema: a content-derived id, an optional list of type-parameter names if
/// the schema is parametric, and a kind describing what it represents.
///
/// Spec: "Type system" — `Schema`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Schema {
    pub id: SchemaId,
    pub type_params: Vec<String>,
    pub kind: SchemaKind,
}

/// What a schema represents.
///
/// Spec: "Type system" — `SchemaKind`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SchemaKind {
    Primitive(Primitive),
    Struct { name: String, fields: Vec<Field> },
    Enum { name: String, variants: Vec<Variant> },
    Tuple { elements: Vec<SchemaRef> },
    List { element: SchemaRef },
    Set { element: SchemaRef },
    Map { key: SchemaRef, value: SchemaRef },
    Array { element: SchemaRef, dimensions: Vec<u64> },
    Tensor { element: SchemaRef, rank: Option<u32> },
    Option { element: SchemaRef },
    Channel { direction: ChannelDirection, element: SchemaRef },
    Dynamic,
    External { kind: String, metadata: Option<SchemaRef> },
}

/// The direction of a streaming channel.
///
/// Spec: `r[type-system.channel]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ChannelDirection {
    /// The sending end.
    Tx,
    /// The receiving end.
    Rx,
}

/// A reference to another schema.
///
/// `Concrete` names a schema by id and supplies arguments for its type
/// parameters (empty for a non-generic reference). `Var` names a type parameter
/// declared by an enclosing schema's `type_params`.
///
/// Spec: "Type system" — `SchemaRef`, `r[type-system.generics]`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SchemaRef {
    Concrete { id: SchemaId, args: Vec<SchemaRef> },
    Var { name: String },
}

impl SchemaRef {
    /// A concrete reference with no type arguments.
    #[must_use]
    pub fn concrete(id: SchemaId) -> Self {
        SchemaRef::Concrete {
            id,
            args: Vec::new(),
        }
    }

    /// A concrete reference binding type parameters with `args`.
    #[must_use]
    pub fn generic(id: SchemaId, args: Vec<SchemaRef>) -> Self {
        SchemaRef::Concrete { id, args }
    }

    /// A reference to an enclosing schema's type parameter.
    #[must_use]
    pub fn var(name: impl Into<String>) -> Self {
        SchemaRef::Var { name: name.into() }
    }
}

/// The primitive types.
///
/// Spec: "Type system" — `Primitive`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Primitive {
    Bool,
    U8,
    U16,
    U32,
    U64,
    U128,
    I8,
    I16,
    I32,
    I64,
    I128,
    F32,
    F64,
    Char,
    String,
    Bytes,
    /// An instant or civil time, carried as its RFC 3339 / ISO 8601 canonical
    /// string. A binding with a native date/time type parses and formats it; one
    /// without holds the string.
    DateTime,
    /// A UUID, carried as its lowercase hyphenated canonical string.
    Uuid,
    /// A qualified name (optional namespace + local name), carried as its James
    /// Clark `{namespace}local` canonical string.
    QName,
    Unit,
    Never,
}

impl Primitive {
    /// Every primitive, in declaration order.
    pub const ALL: [Primitive; 21] = [
        Primitive::Bool,
        Primitive::U8,
        Primitive::U16,
        Primitive::U32,
        Primitive::U64,
        Primitive::U128,
        Primitive::I8,
        Primitive::I16,
        Primitive::I32,
        Primitive::I64,
        Primitive::I128,
        Primitive::F32,
        Primitive::F64,
        Primitive::Char,
        Primitive::String,
        Primitive::Bytes,
        Primitive::DateTime,
        Primitive::Uuid,
        Primitive::QName,
        Primitive::Unit,
        Primitive::Never,
    ];

    /// The canonical tag string fed to the identity hash for this primitive.
    ///
    /// Spec: `r[schema-identity.canonical-encoding]` (primitive tags).
    #[must_use]
    pub fn tag(self) -> &'static str {
        match self {
            Primitive::Bool => "bool",
            Primitive::U8 => "u8",
            Primitive::U16 => "u16",
            Primitive::U32 => "u32",
            Primitive::U64 => "u64",
            Primitive::U128 => "u128",
            Primitive::I8 => "i8",
            Primitive::I16 => "i16",
            Primitive::I32 => "i32",
            Primitive::I64 => "i64",
            Primitive::I128 => "i128",
            Primitive::F32 => "f32",
            Primitive::F64 => "f64",
            Primitive::Char => "char",
            Primitive::String => "string",
            Primitive::Bytes => "bytes",
            Primitive::DateTime => "datetime",
            Primitive::Uuid => "uuid",
            Primitive::QName => "qname",
            Primitive::Unit => "unit",
            Primitive::Never => "never",
        }
    }

    /// The inverse of [`Primitive::tag`]: parse a primitive from its tag string.
    #[must_use]
    pub fn from_tag(tag: &str) -> Option<Primitive> {
        Some(match tag {
            "bool" => Primitive::Bool,
            "u8" => Primitive::U8,
            "u16" => Primitive::U16,
            "u32" => Primitive::U32,
            "u64" => Primitive::U64,
            "u128" => Primitive::U128,
            "i8" => Primitive::I8,
            "i16" => Primitive::I16,
            "i32" => Primitive::I32,
            "i64" => Primitive::I64,
            "i128" => Primitive::I128,
            "f32" => Primitive::F32,
            "f64" => Primitive::F64,
            "char" => Primitive::Char,
            "string" => Primitive::String,
            "bytes" => Primitive::Bytes,
            "datetime" => Primitive::DateTime,
            "uuid" => Primitive::Uuid,
            "qname" => Primitive::QName,
            "unit" => Primitive::Unit,
            "never" => Primitive::Never,
            _ => return None,
        })
    }
}

/// A struct field: a name, the field's schema (which may be parametric), and a
/// `required` flag (no default; must be present).
///
/// Spec: "Type system" — `Field`, `r[compat.reader-only-fields]`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Field {
    pub name: String,
    pub schema: SchemaRef,
    pub required: bool,
}

/// An enum variant: a name, a stable index, and a payload shape.
///
/// Spec: "Type system" — `Variant`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Variant {
    pub name: String,
    pub index: u32,
    pub payload: VariantPayload,
}

/// The four payload shapes an enum variant can hold.
///
/// Spec: `r[type-system.variant-payloads]`.
// r[impl type-system.variant-payloads]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum VariantPayload {
    Unit,
    Newtype(SchemaRef),
    Tuple(Vec<SchemaRef>),
    Struct(Vec<Field>),
}
