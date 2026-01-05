#![deny(unsafe_code)]

//! Rust-level schema structs per `docs/content/rust-spec/_index.md`.

use facet::Facet;

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct MethodDetail {
    pub service_name: String,
    pub method_name: String,
    pub args: Vec<ArgDetail>,
    pub return_type: TypeDetail,
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
