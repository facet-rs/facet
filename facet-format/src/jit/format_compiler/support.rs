use facet_core::{Def, Shape, StructType, Type, UserType};

use crate::jit::Tier2Incompatibility;

// =============================================================================
// Tier-2 Compatibility Check
// =============================================================================

/// Ensure a shape is compatible with Tier-2 format JIT (Map encoding).
///
/// Returns `Ok(())` if compatible, or `Err(Tier2Incompatibility)` with details about why not.
///
/// Note: Tier-2 is only available on 64-bit platforms due to ABI constraints
/// (bit-packing in return values assumes 64-bit pointers).
pub fn ensure_format_jit_compatible(
    shape: &'static Shape,
    type_name: &'static str,
) -> Result<(), Tier2Incompatibility> {
    ensure_format_jit_compatible_with_encoding(shape, crate::jit::StructEncoding::Map, type_name)
}

/// Ensure a shape is compatible with Tier-2 format JIT for a specific struct encoding.
///
/// Returns `Ok(())` if compatible, or `Err(Tier2Incompatibility)` with details about why not.
///
/// # Arguments
/// * `shape` - The shape to check for compatibility
/// * `encoding` - The struct encoding used by the format (Map or Positional)
/// * `type_name` - The type name for error messages (from `std::any::type_name::<T>()`)
///
/// Note: Tier-2 is only available on 64-bit platforms due to ABI constraints.
pub fn ensure_format_jit_compatible_with_encoding(
    shape: &'static Shape,
    encoding: crate::jit::StructEncoding,
    type_name: &'static str,
) -> Result<(), Tier2Incompatibility> {
    // Tier-2 requires 64-bit for ABI (bit-63 packing in return values)
    #[cfg(not(target_pointer_width = "64"))]
    {
        return Err(Tier2Incompatibility::Not64BitPlatform);
    }

    #[cfg(target_pointer_width = "64")]
    {
        use facet_core::ScalarType;

        // Check for Vec<T> types
        if let Def::List(list_def) = &shape.def {
            return ensure_format_jit_element_supported(list_def.t, type_name);
        }

        // Check for HashMap<String, V> types
        if let Def::Map(map_def) = &shape.def {
            // Key must be String
            if map_def.k.scalar_type() != Some(ScalarType::String) {
                return Err(Tier2Incompatibility::MapNonStringKey { type_name });
            }
            // Value must be a supported element type
            return ensure_format_jit_element_supported(map_def.v, type_name);
        }

        // Check for simple struct types
        if let Type::User(UserType::Struct(struct_def)) = &shape.ty {
            return ensure_format_jit_struct_supported_with_encoding(
                struct_def, encoding, type_name,
            );
        }

        // Check for enum types (positional encoding only)
        if let Type::User(UserType::Enum(enum_def)) = &shape.ty {
            if encoding != crate::jit::StructEncoding::Positional {
                return Err(Tier2Incompatibility::EnumRequiresPositionalFormat { type_name });
            }
            return ensure_format_jit_enum_supported(enum_def, type_name);
        }

        Err(Tier2Incompatibility::UnrecognizedShapeType { type_name })
    }
}

/// Ensure a struct type is supported for Tier-2 (simple struct subset).
///
/// Uses Map encoding (conservative default).
fn ensure_format_jit_struct_supported(
    struct_def: &StructType,
    type_name: &'static str,
) -> Result<(), Tier2Incompatibility> {
    ensure_format_jit_struct_supported_with_encoding(
        struct_def,
        crate::jit::StructEncoding::Map,
        type_name,
    )
}

/// Ensure a struct type is supported for Tier-2 with a specific struct encoding.
///
/// Simple struct subset:
/// - Named fields (StructKind::Struct) - supported by both encodings
/// - Tuple structs (StructKind::TupleStruct) - only supported by Positional encoding
/// - Unit structs (StructKind::Unit) - only supported by Positional encoding
/// - Flatten supported for: structs, enums, and `HashMap<String, V>`
/// - ≤64 fields (for bitset tracking)
/// - Fields can be: scalars, `Option<T>`, `Vec<T>`, `HashMap<String, V>`, or nested simple structs
/// - No custom defaults (only Option pre-initialization)
fn ensure_format_jit_struct_supported_with_encoding(
    struct_def: &StructType,
    encoding: crate::jit::StructEncoding,
    type_name: &'static str,
) -> Result<(), Tier2Incompatibility> {
    use facet_core::StructKind;

    // Check struct kind based on encoding
    match encoding {
        crate::jit::StructEncoding::Map => {
            // Map-based formats only support named structs
            if !matches!(struct_def.kind, StructKind::Struct) {
                return Err(Tier2Incompatibility::TupleStructWithMapFormat { type_name });
            }
        }
        crate::jit::StructEncoding::Positional => {
            // Positional formats support all struct kinds
            if !matches!(
                struct_def.kind,
                StructKind::Struct | StructKind::TupleStruct | StructKind::Unit
            ) {
                return Err(Tier2Incompatibility::TupleStructWithMapFormat { type_name });
            }
        }
    }

    // Note: We don't check total field count here because:
    // 1. Flattened structs expand to more fields, so raw count is misleading
    // 2. Only *required* fields need tracking bits, Option fields are free
    // 3. The accurate check happens in compile_struct_format_deserializer
    //    which counts actual tracking bits (required fields + enum seen bits)

    // Check all fields are compatible
    for field in struct_def.fields {
        // Flatten is supported for enums, structs, and HashMap<String, V>
        if field.is_flattened() {
            let field_shape = field.shape();

            // Handle flattened HashMap<String, V>
            if let Def::Map(map_def) = &field_shape.def {
                // Validate key is String
                if map_def.k.scalar_type() != Some(facet_core::ScalarType::String) {
                    return Err(Tier2Incompatibility::FlattenedMapNonStringKey {
                        type_name,
                        field_name: field.name,
                    });
                }
                // Validate value type is supported (same check as map values)
                ensure_format_jit_element_supported(map_def.v, type_name)?;
                // Flattened map is OK - skip normal field type check and continue to next field
                continue;
            }

            // Handle flattened enum or struct
            match &field_shape.ty {
                facet_core::Type::User(facet_core::UserType::Enum(enum_type)) => {
                    // Check if it's a supported flattened enum (stricter than regular enums)
                    ensure_format_jit_flattened_enum_supported(enum_type, type_name)?;
                    // Flattened enum is OK - skip normal field type check and continue to next field
                    continue;
                }
                facet_core::Type::User(facet_core::UserType::Struct(inner_struct)) => {
                    // Recursively check if the inner struct is supported
                    ensure_format_jit_struct_supported(inner_struct, type_name)?;
                    // Flattened struct is OK - skip normal field type check and continue to next field
                    continue;
                }
                _ => {
                    return Err(Tier2Incompatibility::UnsupportedFlattenType {
                        type_name,
                        field_name: field.name,
                    });
                }
            }
        }

        // No custom defaults in simple subset (Option pre-init is OK)
        if field.has_default() {
            return Err(Tier2Incompatibility::FieldHasCustomDefault {
                type_name,
                field_name: field.name,
            });
        }

        // Field type must be supported (for normal, non-flattened fields)
        ensure_format_jit_field_type_supported(field.shape(), type_name, field.name)?;
    }

    Ok(())
}

/// Ensure a flattened enum is supported for Tier-2 JIT compilation.
///
/// Flattened enums have stricter requirements than regular enums:
/// - Unit variants are NOT supported (regular enums can have them)
/// - All variants must have at least one field containing payload data
/// - Otherwise, same requirements as regular enums
fn ensure_format_jit_flattened_enum_supported(
    enum_type: &facet_core::EnumType,
    type_name: &'static str,
) -> Result<(), Tier2Incompatibility> {
    use facet_core::StructKind;

    // First check basic enum requirements
    ensure_format_jit_enum_supported(enum_type, type_name)?;

    // Additional check for flattened enums: no unit variants allowed
    for variant in enum_type.variants {
        if matches!(variant.data.kind, StructKind::Unit) {
            return Err(Tier2Incompatibility::FlattenedEnumUnitVariant {
                type_name,
                variant_name: variant.name,
            });
        }

        // Also reject variants with no fields (shouldn't happen, but be defensive)
        if variant.data.fields.is_empty() {
            return Err(Tier2Incompatibility::FlattenedEnumUnitVariant {
                type_name,
                variant_name: variant.name,
            });
        }
    }

    Ok(())
}

/// Ensure an enum is supported for Tier-2 JIT compilation (MVP).
///
/// MVP requirements:
/// - #[repr(C)] or #[repr(Rust)] with explicit discriminant
/// - All variants must have supported field types
/// - Payload structs must be JIT-compatible
fn ensure_format_jit_enum_supported(
    enum_type: &facet_core::EnumType,
    type_name: &'static str,
) -> Result<(), Tier2Incompatibility> {
    use facet_core::{BaseRepr, EnumRepr, ScalarType, StructKind};

    // Accept #[repr(C)] or #[repr(Rust)] with explicit discriminant (like #[repr(u8)])
    // Both are fine for our needs - we just need known layout and discriminant size
    if !matches!(enum_type.repr.base, BaseRepr::C | BaseRepr::Rust) {
        let repr_name = match enum_type.repr.base {
            BaseRepr::C => "C",
            BaseRepr::Rust => "Rust",
            BaseRepr::Transparent => "transparent",
        };
        return Err(Tier2Incompatibility::UnsupportedEnumRepr {
            type_name,
            repr: repr_name,
        });
    }

    // Verify discriminant representation is known
    // We support any explicit integer representation for the discriminant
    match enum_type.enum_repr {
        EnumRepr::U8
        | EnumRepr::U16
        | EnumRepr::U32
        | EnumRepr::U64
        | EnumRepr::USize
        | EnumRepr::I8
        | EnumRepr::I16
        | EnumRepr::I32
        | EnumRepr::I64
        | EnumRepr::ISize => {
            // All explicit discriminant sizes are supported
        }
        EnumRepr::RustNPO => {
            return Err(Tier2Incompatibility::UnsupportedEnumRepr {
                type_name,
                repr: "niche/NPO (Option-like optimization)",
            });
        }
    }

    // Check all variants have supported field types
    for variant in enum_type.variants {
        // Verify discriminant is present
        if variant.discriminant.is_none() {
            return Err(Tier2Incompatibility::EnumVariantNoDiscriminant {
                type_name,
                variant_name: variant.name,
            });
        }

        // Check variant fields based on kind
        match variant.data.kind {
            StructKind::Unit => {
                // Unit variants are always supported (for non-flattened enums)
            }
            StructKind::TupleStruct | StructKind::Struct | StructKind::Tuple => {
                // Check all variant fields
                for field in variant.data.fields {
                    let field_shape = field.shape();

                    // Check if it's a struct payload (common for flattened enums)
                    if let facet_core::Type::User(facet_core::UserType::Struct(struct_def)) =
                        &field_shape.ty
                    {
                        // Recursively validate the struct
                        ensure_format_jit_struct_supported(struct_def, type_name)?;
                    } else if let Some(scalar_type) = field_shape.scalar_type() {
                        // Scalars are supported
                        if !matches!(
                            scalar_type,
                            ScalarType::Bool
                                | ScalarType::I8
                                | ScalarType::I16
                                | ScalarType::I32
                                | ScalarType::I64
                                | ScalarType::U8
                                | ScalarType::U16
                                | ScalarType::U32
                                | ScalarType::U64
                                | ScalarType::String
                        ) {
                            return Err(Tier2Incompatibility::UnsupportedEnumVariantField {
                                type_name,
                                variant_name: variant.name,
                                field_name: field.name,
                            });
                        }
                    } else {
                        return Err(Tier2Incompatibility::UnsupportedEnumVariantField {
                            type_name,
                            variant_name: variant.name,
                            field_name: field.name,
                        });
                    }
                }
            }
        }
    }

    Ok(())
}

/// Ensure a field type is supported for Tier-2.
///
/// Supported types:
/// - Scalars (bool, integers, floats, String)
/// - `Option<T>` where T is supported
/// - `Result<T, E>` where both T and E are supported
/// - `Vec<T>` where T is a supported element type (scalars, structs, nested Vec/Map)
/// - HashMap<String, V> where V is a supported element type
/// - Nested simple structs (recursive)
pub(crate) fn ensure_format_jit_field_type_supported(
    shape: &'static Shape,
    type_name: &'static str,
    field_name: &'static str,
) -> Result<(), Tier2Incompatibility> {
    use facet_core::ScalarType;

    // Check for Option<T>
    if let Def::Option(opt_def) = &shape.def {
        return ensure_format_jit_field_type_supported(opt_def.t, type_name, field_name);
    }

    // Check for Result<T, E>
    if let Def::Result(result_def) = &shape.def {
        // Both Ok and Err types must be supported
        ensure_format_jit_field_type_supported(result_def.t, type_name, field_name).map_err(
            |_| Tier2Incompatibility::UnsupportedResultType {
                type_name,
                which: "Ok",
            },
        )?;
        ensure_format_jit_field_type_supported(result_def.e, type_name, field_name).map_err(
            |_| Tier2Incompatibility::UnsupportedResultType {
                type_name,
                which: "Err",
            },
        )?;
        return Ok(());
    }

    // Check for Vec<T>
    if let Def::List(list_def) = &shape.def {
        return ensure_format_jit_element_supported(list_def.t, type_name);
    }

    // Check for HashMap<String, V>
    if let Def::Map(map_def) = &shape.def {
        // Key must be String
        if map_def.k.scalar_type() != Some(ScalarType::String) {
            return Err(Tier2Incompatibility::MapNonStringKey { type_name });
        }
        // Value must be a supported element type
        return ensure_format_jit_element_supported(map_def.v, type_name);
    }

    // Check for scalars
    if let Some(scalar_type) = shape.scalar_type() {
        if matches!(
            scalar_type,
            ScalarType::Bool
                | ScalarType::I8
                | ScalarType::I16
                | ScalarType::I32
                | ScalarType::I64
                | ScalarType::U8
                | ScalarType::U16
                | ScalarType::U32
                | ScalarType::U64
                | ScalarType::F32
                | ScalarType::F64
                | ScalarType::String
        ) {
            return Ok(());
        }
    }

    // Check for nested simple structs
    if let Type::User(UserType::Struct(struct_def)) = &shape.ty {
        return ensure_format_jit_struct_supported(struct_def, type_name);
    }

    // Check for enums (non-flattened)
    if let Type::User(UserType::Enum(enum_def)) = &shape.ty {
        return ensure_format_jit_enum_supported(enum_def, type_name);
    }

    // Get the field type name for the error message
    let field_type = shape_type_description(shape);
    Err(Tier2Incompatibility::UnsupportedFieldType {
        type_name,
        field_name,
        field_type,
    })
}

/// Ensure a Vec element type is supported for Tier-2.
pub(crate) fn ensure_format_jit_element_supported(
    elem_shape: &'static Shape,
    type_name: &'static str,
) -> Result<(), Tier2Incompatibility> {
    use facet_core::ScalarType;

    if let Some(scalar_type) = elem_shape.scalar_type() {
        // All scalar types (including String) are supported with Tier-2 JIT.
        if matches!(
            scalar_type,
            ScalarType::Bool
                | ScalarType::I8
                | ScalarType::I16
                | ScalarType::I32
                | ScalarType::I64
                | ScalarType::U8
                | ScalarType::U16
                | ScalarType::U32
                | ScalarType::U64
                | ScalarType::F32
                | ScalarType::F64
                | ScalarType::String
        ) {
            return Ok(());
        }
    }

    // Support Result<T, E> elements (e.g., Vec<Result<i32, String>>)
    if let Def::Result(result_def) = &elem_shape.def {
        // Both Ok and Err types must be supported element types
        ensure_format_jit_element_supported(result_def.t, type_name).map_err(|_| {
            Tier2Incompatibility::UnsupportedResultType {
                type_name,
                which: "Ok",
            }
        })?;
        ensure_format_jit_element_supported(result_def.e, type_name).map_err(|_| {
            Tier2Incompatibility::UnsupportedResultType {
                type_name,
                which: "Err",
            }
        })?;
        return Ok(());
    }

    // Support nested Vec<Vec<T>> by recursively checking the inner element type
    if let Def::List(list_def) = &elem_shape.def {
        return ensure_format_jit_element_supported(list_def.t, type_name);
    }

    // Support nested HashMap<String, V> as Vec element
    if let Def::Map(map_def) = &elem_shape.def {
        // Key must be String
        if map_def.k.scalar_type() != Some(ScalarType::String) {
            return Err(Tier2Incompatibility::MapNonStringKey { type_name });
        }
        // Value must be a supported element type (recursive check)
        return ensure_format_jit_element_supported(map_def.v, type_name);
    }

    // Support struct elements (Vec<struct>) - but only if the struct itself is Tier-2 compatible
    if let Type::User(UserType::Struct(struct_def)) = &elem_shape.ty {
        return ensure_format_jit_struct_supported(struct_def, type_name);
    }

    // Element type not supported
    let elem_type = shape_type_description(elem_shape);
    Err(Tier2Incompatibility::UnsupportedFieldType {
        type_name,
        field_name: "(element)",
        field_type: elem_type,
    })
}

/// Get a human-readable description of a shape's type for error messages.
fn shape_type_description(shape: &'static Shape) -> &'static str {
    match &shape.def {
        Def::Undefined => "undefined",
        Def::Scalar => "scalar",
        Def::List(_) => "Vec<_>",
        Def::Map(_) => "HashMap<_, _>",
        Def::Set(_) => "HashSet<_>",
        Def::Option(_) => "Option<_>",
        Def::Result(_) => "Result<_, _>",
        Def::Pointer(_) => "smart pointer",
        Def::Array(_) => "array",
        Def::NdArray(_) => "nd-array",
        Def::Slice(_) => "slice",
        Def::DynamicValue(_) => "dynamic value",
        // Def is non-exhaustive, so we need a wildcard
        _ => "unknown",
    }
}
