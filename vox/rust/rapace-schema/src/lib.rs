#![deny(unsafe_code)]

//! Rust-level schema structs per `docs/content/rust-spec/_index.md`.

use facet::Facet;

/// A complete service definition with all its methods.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct ServiceDetail {
    pub name: String,
    pub methods: Vec<MethodDetail>,
    pub doc: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct MethodDetail {
    pub service_name: String,
    pub method_name: String,
    pub args: Vec<ArgDetail>,
    pub return_type: TypeDetail,
    pub doc: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct ArgDetail {
    pub name: String,
    pub type_info: TypeDetail,
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct ServiceSummary {
    pub name: String,
    pub method_count: u32,
    pub doc: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct MethodSummary {
    pub name: String,
    pub method_id: u64,
    pub doc: Option<String>,
}

#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub enum MismatchExplanation {
    /// Service doesn't exist.
    UnknownService { closest: Option<String> } = 0,
    /// Service exists but method doesn't.
    UnknownMethod {
        service: String,
        closest: Option<String>,
    } = 1,
    /// Method exists but signature differs.
    SignatureMismatch {
        service: String,
        method: String,
        expected: MethodDetail,
    } = 2,
}

/// Describes a type's structure for introspection and diffing.
///
/// Mirrors the signature-hash encoding but in a structured form.
#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub enum TypeDetail {
    // Primitives
    Bool = 0,
    U8 = 1,
    U16 = 2,
    U32 = 3,
    U64 = 4,
    U128 = 5,
    I8 = 6,
    I16 = 7,
    I32 = 8,
    I64 = 9,
    I128 = 10,
    F32 = 11,
    F64 = 12,
    Char = 13,
    String = 14,
    Unit = 15,
    Bytes = 16,

    // Containers
    List(Box<TypeDetail>) = 32,
    Option(Box<TypeDetail>) = 33,
    Array {
        element: Box<TypeDetail>,
        len: u32,
    } = 34,
    Map {
        key: Box<TypeDetail>,
        value: Box<TypeDetail>,
    } = 35,
    Set(Box<TypeDetail>) = 36,
    Tuple(Vec<TypeDetail>) = 37,
    Stream(Box<TypeDetail>) = 38,

    // Composite
    Struct {
        fields: Vec<FieldDetail>,
    } = 48,
    Enum {
        variants: Vec<VariantDetail>,
    } = 49,
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct FieldDetail {
    pub name: String,
    pub type_info: TypeDetail,
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct VariantDetail {
    pub name: String,
    pub payload: VariantPayload,
}

#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub enum VariantPayload {
    Unit = 0,
    Newtype(TypeDetail) = 1,
    Struct(Vec<FieldDetail>) = 2,
}

impl TypeDetail {
    /// Visit all TypeDetails in this type tree, including this one.
    /// Returns true to continue visiting, false to stop early.
    pub fn visit<F>(&self, visitor: &mut F) -> bool
    where
        F: FnMut(&TypeDetail) -> bool,
    {
        // Visit self first
        if !visitor(self) {
            return false;
        }

        // Then visit children
        match self {
            TypeDetail::List(inner)
            | TypeDetail::Set(inner)
            | TypeDetail::Option(inner)
            | TypeDetail::Stream(inner) => inner.visit(visitor),
            TypeDetail::Array { element, .. } => element.visit(visitor),
            TypeDetail::Map { key, value } => key.visit(visitor) && value.visit(visitor),
            TypeDetail::Tuple(items) => items.iter().all(|item| item.visit(visitor)),
            TypeDetail::Struct { fields } => fields.iter().all(|f| f.type_info.visit(visitor)),
            TypeDetail::Enum { variants } => variants.iter().all(|v| match &v.payload {
                VariantPayload::Unit => true,
                VariantPayload::Newtype(inner) => inner.visit(visitor),
                VariantPayload::Struct(fields) => fields.iter().all(|f| f.type_info.visit(visitor)),
            }),
            // Primitives have no children
            TypeDetail::Bool
            | TypeDetail::U8
            | TypeDetail::U16
            | TypeDetail::U32
            | TypeDetail::U64
            | TypeDetail::U128
            | TypeDetail::I8
            | TypeDetail::I16
            | TypeDetail::I32
            | TypeDetail::I64
            | TypeDetail::I128
            | TypeDetail::F32
            | TypeDetail::F64
            | TypeDetail::Char
            | TypeDetail::String
            | TypeDetail::Bytes
            | TypeDetail::Unit => true,
        }
    }
}
