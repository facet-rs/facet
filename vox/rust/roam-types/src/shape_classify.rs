use facet_core::{Def, ScalarType, Shape, StructKind, Type, UserType};

/// Classification of a `Shape` for code generation.
#[derive(Debug, Clone, Copy)]
pub enum ShapeKind<'a> {
    Scalar(ScalarType),
    List {
        element: &'static Shape,
    },
    Array {
        element: &'static Shape,
        len: usize,
    },
    Slice {
        element: &'static Shape,
    },
    Option {
        inner: &'static Shape,
    },
    Map {
        key: &'static Shape,
        value: &'static Shape,
    },
    Set {
        element: &'static Shape,
    },
    Struct(StructInfo<'a>),
    Enum(EnumInfo<'a>),
    Tuple {
        elements: &'a [facet_core::TypeParam],
    },
    TupleStruct {
        fields: &'a [facet_core::Field],
    },
    Tx {
        inner: &'static Shape,
    },
    Rx {
        inner: &'static Shape,
    },
    Pointer {
        pointee: &'static Shape,
    },
    Result {
        ok: &'static Shape,
        err: &'static Shape,
    },
    Opaque,
}

/// Information about a struct type.
#[derive(Debug, Clone, Copy)]
pub struct StructInfo<'a> {
    pub name: Option<&'static str>,
    pub kind: StructKind,
    pub fields: &'a [facet_core::Field],
}

/// Information about an enum type.
#[derive(Debug, Clone, Copy)]
pub struct EnumInfo<'a> {
    pub name: Option<&'static str>,
    pub variants: &'a [facet_core::Variant],
}

/// Information about an enum variant for code generation.
#[derive(Debug, Clone, Copy)]
pub enum VariantKind<'a> {
    Unit,
    Newtype { inner: &'static Shape },
    Tuple { fields: &'a [facet_core::Field] },
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

/// Classify a `Shape` into a higher-level semantic kind.
pub fn classify_shape(shape: &'static Shape) -> ShapeKind<'static> {
    if crate::is_tx(shape)
        && let Some(inner) = shape.type_params.first()
    {
        return ShapeKind::Tx { inner: inner.shape };
    }
    if crate::is_rx(shape)
        && let Some(inner) = shape.type_params.first()
    {
        return ShapeKind::Rx { inner: inner.shape };
    }

    if shape.is_transparent()
        && let Some(inner) = shape.inner
    {
        return classify_shape(inner);
    }

    if let Some(scalar) = shape.scalar_type() {
        return ShapeKind::Scalar(scalar);
    }

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
        Def::Option(opt_def) => return ShapeKind::Option { inner: opt_def.t() },
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

    match shape.ty {
        Type::User(UserType::Struct(struct_type)) => {
            if struct_type.kind == StructKind::Tuple {
                return ShapeKind::TupleStruct {
                    fields: struct_type.fields,
                };
            }
            return ShapeKind::Struct(StructInfo {
                name: extract_type_name(shape.type_identifier),
                kind: struct_type.kind,
                fields: struct_type.fields,
            });
        }
        Type::User(UserType::Enum(enum_type)) => {
            return ShapeKind::Enum(EnumInfo {
                name: extract_type_name(shape.type_identifier),
                variants: enum_type.variants,
            });
        }
        Type::Pointer(_) => {
            if let Some(inner) = shape.type_params.first() {
                return classify_shape(inner.shape);
            }
        }
        _ => {}
    }

    ShapeKind::Opaque
}

/// Check if a shape represents bytes (`Vec<u8>` or `&[u8]`).
pub fn is_bytes(shape: &Shape) -> bool {
    match shape.def {
        Def::List(list_def) => matches!(list_def.t().scalar_type(), Some(ScalarType::U8)),
        Def::Slice(slice_def) => matches!(slice_def.t().scalar_type(), Some(ScalarType::U8)),
        _ => false,
    }
}

fn extract_type_name(type_identifier: &'static str) -> Option<&'static str> {
    if type_identifier.is_empty()
        || type_identifier.starts_with('(')
        || type_identifier.starts_with('[')
    {
        return None;
    }
    Some(type_identifier)
}
