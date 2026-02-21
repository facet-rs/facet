#![deny(unsafe_code)]

//! Schema types for roam RPC service definitions.
//!
//! # Design Philosophy
//!
//! This crate uses `facet::Shape` directly for type information rather than
//! defining a parallel type system. This means:
//!
//! - **No `TypeDetail`** — We use `&'static Shape` from facet instead
//! - **Full type introspection** — Shape provides complete type information
//! - **Zero conversion overhead** — Types are described by their Shape directly
//!
//! For type-specific queries (is this a stream? what are the struct fields?),
//! use the `facet_core` API to inspect the `Shape`.

use std::borrow::Cow;

use facet::Facet;
use facet_core::Shape;

/// A complete service definition with all its methods.
#[derive(Debug, Clone, Facet)]
pub struct ServiceDetail {
    /// Service name (e.g., "Calculator").
    pub name: Cow<'static, str>,

    /// Methods defined on this service.
    pub methods: Vec<MethodDetail>,

    /// Documentation string, if any.
    pub doc: Option<Cow<'static, str>>,
}

/// A single method in a service definition.
#[derive(Debug, Clone, Facet)]
pub struct MethodDetail {
    /// The service this method belongs to.
    pub service_name: Cow<'static, str>,

    /// Method name (e.g., "add").
    pub method_name: Cow<'static, str>,

    /// Method arguments (excluding `&self`).
    pub args: Vec<ArgDetail>,

    /// Return type shape.
    ///
    /// Use `facet_core` to inspect the shape:
    /// - `shape.def` reveals if it's a struct, enum, primitive, etc.
    /// - `shape.type_params` gives generic parameters
    /// - Check for `#[facet(roam = "tx")]` attribute for streaming types
    pub return_type: &'static Shape,

    /// Documentation string, if any.
    pub doc: Option<Cow<'static, str>>,
}

/// A single argument in a method signature.
#[derive(Debug, Clone, Facet)]
pub struct ArgDetail {
    /// Argument name.
    pub name: Cow<'static, str>,

    /// Argument type shape.
    pub ty: &'static Shape,
}

/// Summary information about a service (for listings/discovery).
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct ServiceSummary {
    pub name: Cow<'static, str>,
    pub method_count: u32,
    pub doc: Option<Cow<'static, str>>,
}

/// Summary information about a method (for listings/discovery).
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct MethodSummary {
    pub name: Cow<'static, str>,
    pub method_id: u64,
    pub doc: Option<Cow<'static, str>>,
}

/// Explanation of why a method call mismatched.
#[repr(u8)]
#[derive(Debug, Clone, Facet)]
pub enum MismatchExplanation {
    /// Service doesn't exist.
    UnknownService { closest: Option<Cow<'static, str>> } = 0,

    /// Service exists but method doesn't.
    UnknownMethod {
        service: Cow<'static, str>,
        closest: Option<Cow<'static, str>>,
    } = 1,

    /// Method exists but signature differs.
    ///
    /// The `expected` field contains the server's method signature.
    /// Compare with the client's signature to diagnose the mismatch.
    SignatureMismatch {
        service: Cow<'static, str>,
        method: Cow<'static, str>,
        expected: MethodDetail,
    } = 2,
}

// ============================================================================
// Helper functions for working with Shape
// ============================================================================

/// Check if a shape represents a Tx (caller→callee) stream.
pub fn is_tx(shape: &Shape) -> bool {
    shape.decl_id == roam_session::Tx::<()>::SHAPE.decl_id
}

/// Check if a shape represents an Rx (callee→caller) stream.
pub fn is_rx(shape: &Shape) -> bool {
    shape.decl_id == roam_session::Rx::<()>::SHAPE.decl_id
}

/// Check if a shape represents any channel type (Tx or Rx).
pub fn is_channel(shape: &Shape) -> bool {
    is_tx(shape) || is_rx(shape)
}

/// Recursively check if a shape or any of its type parameters contains a channel.
pub fn contains_channels(shape: &Shape) -> bool {
    if is_channel(shape) {
        return true;
    }

    // Check type parameters recursively
    for param in shape.type_params {
        if contains_channels(param.shape) {
            return true;
        }
    }

    false
}

// ============================================================================
// Shape classification for codegen
// ============================================================================

use facet_core::{Def, ScalarType, StructKind, Type, UserType};

/// Classification of a Shape for codegen purposes.
///
/// This provides a higher-level view than raw `Shape.ty` and `Shape.def`,
/// combining both to give the semantic type category needed for code generation.
#[derive(Debug, Clone, Copy)]
pub enum ShapeKind<'a> {
    /// Scalar/primitive type
    Scalar(ScalarType),
    /// List/Vec of elements
    List { element: &'static Shape },
    /// Fixed-size array
    Array { element: &'static Shape, len: usize },
    /// Slice (treated like list for codegen)
    Slice { element: &'static Shape },
    /// Optional value
    Option { inner: &'static Shape },
    /// Map/HashMap
    Map {
        key: &'static Shape,
        value: &'static Shape,
    },
    /// Set/HashSet
    Set { element: &'static Shape },
    /// Named or anonymous struct
    Struct(StructInfo<'a>),
    /// Named or anonymous enum
    Enum(EnumInfo<'a>),
    /// Tuple (including unit) - from type_params
    Tuple {
        elements: &'a [facet_core::TypeParam],
    },
    /// Tuple struct - from struct fields (anonymous tuple like (i32, String))
    TupleStruct { fields: &'a [facet_core::Field] },
    /// Tx stream (caller → callee)
    Tx { inner: &'static Shape },
    /// Rx stream (callee → caller)
    Rx { inner: &'static Shape },
    /// Smart pointer (Box, Arc, etc.) - transparent
    Pointer { pointee: &'static Shape },
    /// Result type
    Result {
        ok: &'static Shape,
        err: &'static Shape,
    },
    /// Unknown/opaque type
    Opaque,
}

/// Information about a struct type.
#[derive(Debug, Clone, Copy)]
pub struct StructInfo<'a> {
    /// Type name (e.g., "MyStruct"), or None for tuples/anonymous
    pub name: Option<&'static str>,
    /// Struct kind (unit, tuple struct, named struct)
    pub kind: StructKind,
    /// Fields in declaration order
    pub fields: &'a [facet_core::Field],
}

/// Information about an enum type.
#[derive(Debug, Clone, Copy)]
pub struct EnumInfo<'a> {
    /// Type name (e.g., "MyEnum")
    pub name: Option<&'static str>,
    /// Variants in declaration order
    pub variants: &'a [facet_core::Variant],
}

/// Classify a Shape into a ShapeKind for codegen.
pub fn classify_shape(shape: &'static Shape) -> ShapeKind<'static> {
    // Check for roam streaming types first
    if is_tx(shape)
        && let Some(inner) = shape.type_params.first()
    {
        return ShapeKind::Tx { inner: inner.shape };
    }
    if is_rx(shape)
        && let Some(inner) = shape.type_params.first()
    {
        return ShapeKind::Rx { inner: inner.shape };
    }

    // Check for transparent wrappers
    if shape.is_transparent()
        && let Some(inner) = shape.inner
    {
        return classify_shape(inner);
    }

    // Check scalars first
    if let Some(scalar) = shape.scalar_type() {
        return ShapeKind::Scalar(scalar);
    }

    // Check semantic definitions (containers)
    match shape.def {
        Def::List(list_def) => {
            return ShapeKind::List {
                element: list_def.t(),
            };
        }
        Def::Array(array_def) => {
            return ShapeKind::Array {
                element: array_def.t(),
                len: array_def.n,
            };
        }
        Def::Slice(slice_def) => {
            return ShapeKind::Slice {
                element: slice_def.t(),
            };
        }
        Def::Option(opt_def) => {
            return ShapeKind::Option { inner: opt_def.t() };
        }
        Def::Map(map_def) => {
            return ShapeKind::Map {
                key: map_def.k(),
                value: map_def.v(),
            };
        }
        Def::Set(set_def) => {
            return ShapeKind::Set {
                element: set_def.t(),
            };
        }
        Def::Result(result_def) => {
            return ShapeKind::Result {
                ok: result_def.t(),
                err: result_def.e(),
            };
        }
        Def::Pointer(ptr_def) => {
            if let Some(pointee) = ptr_def.pointee {
                return ShapeKind::Pointer { pointee };
            }
        }
        _ => {}
    }

    // Check user-defined types (structs, enums)
    match shape.ty {
        Type::User(UserType::Struct(struct_type)) => {
            // Check for tuple structs first - tuple element shapes are in fields, not type_params
            if struct_type.kind == StructKind::Tuple {
                return ShapeKind::TupleStruct {
                    fields: struct_type.fields,
                };
            }
            // Extract name from type_identifier (e.g., "my_crate::MyStruct" -> "MyStruct")
            let name = extract_type_name(shape.type_identifier);
            return ShapeKind::Struct(StructInfo {
                name,
                kind: struct_type.kind,
                fields: struct_type.fields,
            });
        }
        Type::User(UserType::Enum(enum_type)) => {
            let name = extract_type_name(shape.type_identifier);
            return ShapeKind::Enum(EnumInfo {
                name,
                variants: enum_type.variants,
            });
        }
        Type::Pointer(_) => {
            // Reference types - get inner from type_params
            if let Some(inner) = shape.type_params.first() {
                return classify_shape(inner.shape);
            }
        }
        _ => {}
    }

    ShapeKind::Opaque
}

/// Get the type name if this is a named type.
/// Returns None for anonymous types (tuples, arrays, primitives).
fn extract_type_name(type_identifier: &'static str) -> Option<&'static str> {
    // Skip anonymous/primitive patterns
    if type_identifier.is_empty()
        || type_identifier.starts_with('(')
        || type_identifier.starts_with('[')
    {
        return None;
    }

    // type_identifier is already the simple name (e.g., "MyStruct", "Vec")
    Some(type_identifier)
}

/// Information about an enum variant for codegen.
#[derive(Debug, Clone, Copy)]
pub enum VariantKind<'a> {
    /// Unit variant: `Foo`
    Unit,
    /// Newtype/tuple variant with single field: `Foo(T)`
    Newtype { inner: &'static Shape },
    /// Tuple variant with multiple fields: `Foo(T1, T2)`
    Tuple { fields: &'a [facet_core::Field] },
    /// Struct variant: `Foo { x: T1, y: T2 }`
    Struct { fields: &'a [facet_core::Field] },
}

/// Classify an enum variant.
pub fn classify_variant(variant: &facet_core::Variant) -> VariantKind<'_> {
    match variant.data.kind {
        StructKind::Unit => VariantKind::Unit,
        StructKind::TupleStruct | StructKind::Tuple => {
            if variant.data.fields.len() == 1 {
                VariantKind::Newtype {
                    inner: variant.data.fields[0].shape(),
                }
            } else {
                VariantKind::Tuple {
                    fields: variant.data.fields,
                }
            }
        }
        StructKind::Struct => VariantKind::Struct {
            fields: variant.data.fields,
        },
    }
}

/// Check if a shape represents bytes (`Vec<u8>` or `&[u8]`).
pub fn is_bytes(shape: &Shape) -> bool {
    match shape.def {
        Def::List(list_def) => matches!(list_def.t().scalar_type(), Some(ScalarType::U8)),
        Def::Slice(slice_def) => matches!(slice_def.t().scalar_type(), Some(ScalarType::U8)),
        _ => false,
    }
}
