// =============================================================================
// Tier-2 Compatibility Check
// =============================================================================

/// Check if a shape is compatible with Tier-2 format JIT.
///
/// For MVP, supports:
/// - `Vec<T>` where T is bool
///
/// Note: Tier-2 is only available on 64-bit platforms due to ABI constraints
/// (bit-packing in return values assumes 64-bit pointers).
pub fn is_format_jit_compatible(shape: &'static Shape) -> bool {
    // Use conservative default (Map encoding) for backward compatibility
    is_format_jit_compatible_with_encoding(shape, crate::jit::StructEncoding::Map)
}

/// Check if a shape is compatible with Tier-2 format JIT for a specific struct encoding.
///
/// This is the format-aware version that knows whether tuple structs are supported.
///
/// # Arguments
/// * `shape` - The shape to check for compatibility
/// * `encoding` - The struct encoding used by the format (Map or Positional)
///
/// Note: Tier-2 is only available on 64-bit platforms due to ABI constraints
/// (bit-packing in return values assumes 64-bit pointers).
pub fn is_format_jit_compatible_with_encoding(
    shape: &'static Shape,
    encoding: crate::jit::StructEncoding,
) -> bool {
    // Tier-2 requires 64-bit for ABI (bit-63 packing in return values)
    #[cfg(not(target_pointer_width = "64"))]
    {
        return false;
    }

    #[cfg(target_pointer_width = "64")]
    {
        use facet_core::ScalarType;

        // Check for Vec<T> types
        if let Def::List(list_def) = &shape.def {
            return is_format_jit_element_supported(list_def.t);
        }

        // Check for HashMap<String, V> types
        if let Def::Map(map_def) = &shape.def {
            // Key must be String
            if map_def.k.scalar_type() != Some(ScalarType::String) {
                return false;
            }
            // Value must be a supported element type
            return is_format_jit_element_supported(map_def.v);
        }

        // Check for simple struct types
        if let Type::User(UserType::Struct(struct_def)) = &shape.ty {
            let supported = is_format_jit_struct_supported_with_encoding(struct_def, encoding);
            if !supported {
                jit_diag!("Struct incompatible (see earlier field diagnostics)");
            }
            return supported;
        }

        // Check for enum types (positional encoding only)
        if let Type::User(UserType::Enum(enum_def)) = &shape.ty {
            if encoding != crate::jit::StructEncoding::Positional {
                jit_diag!("Enum types only supported with positional encoding (e.g., postcard)");
                return false;
            }
            let supported = is_format_jit_enum_supported(enum_def);
            if !supported {
                jit_diag!("Enum incompatible (see earlier diagnostics)");
            }
            return supported;
        }

        jit_diag!("Shape type not recognized as compatible");
        false
    }
}

/// Check if a struct type is supported for Tier-2 (simple struct subset).
///
/// Uses Map encoding (conservative default).
///
/// Simple struct subset:
/// - Named fields only (StructKind::Struct) for map-based formats
/// - Tuple structs and unit structs for positional formats only
/// - Flatten supported for: structs, enums, and `HashMap<String, V>`
/// - ≤64 fields (for bitset tracking)
/// - Fields can be: scalars, `Option<T>`, `Vec<T>`, `HashMap<String, V>`, or nested simple structs
/// - No custom defaults (only Option pre-initialization)
fn is_format_jit_struct_supported(struct_def: &StructType) -> bool {
    is_format_jit_struct_supported_with_encoding(struct_def, crate::jit::StructEncoding::Map)
}

/// Check if a struct type is supported for Tier-2 with a specific struct encoding.
///
/// Simple struct subset:
/// - Named fields (StructKind::Struct) - supported by both encodings
/// - Tuple structs (StructKind::TupleStruct) - only supported by Positional encoding
/// - Unit structs (StructKind::Unit) - only supported by Positional encoding
/// - Flatten supported for: structs, enums, and `HashMap<String, V>`
/// - ≤64 fields (for bitset tracking)
/// - Fields can be: scalars, `Option<T>`, `Vec<T>`, `HashMap<String, V>`, or nested simple structs
/// - No custom defaults (only Option pre-initialization)
fn is_format_jit_struct_supported_with_encoding(
    struct_def: &StructType,
    encoding: crate::jit::StructEncoding,
) -> bool {
    use facet_core::StructKind;

    // Check struct kind based on encoding
    match encoding {
        crate::jit::StructEncoding::Map => {
            // Map-based formats only support named structs
            if !matches!(struct_def.kind, StructKind::Struct) {
                jit_diag!(
                    "Map-based formats do not support {:?} (only named structs)",
                    struct_def.kind
                );
                return false;
            }
        }
        crate::jit::StructEncoding::Positional => {
            // Positional formats support all struct kinds
            if !matches!(
                struct_def.kind,
                StructKind::Struct | StructKind::TupleStruct | StructKind::Unit
            ) {
                return false;
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
                    jit_diag!(
                        "Field '{}' is flattened map but key type is not String",
                        field.name
                    );
                    return false;
                }
                // Validate value type is supported (same check as map values)
                if !is_format_jit_element_supported(map_def.v) {
                    jit_diag!(
                        "Field '{}' is flattened map but value type not supported",
                        field.name
                    );
                    return false;
                }
                // Flattened map is OK - skip normal field type check and continue to next field
                continue;
            }

            // Handle flattened enum or struct
            match &field_shape.ty {
                facet_core::Type::User(facet_core::UserType::Enum(enum_type)) => {
                    // Check if it's a supported flattened enum (stricter than regular enums)
                    if !is_format_jit_flattened_enum_supported(enum_type) {
                        jit_diag!("Field '{}' is flattened enum but not supported", field.name);
                        return false;
                    }
                    // Flattened enum is OK - skip normal field type check and continue to next field
                    continue;
                }
                facet_core::Type::User(facet_core::UserType::Struct(inner_struct)) => {
                    // Recursively check if the inner struct is supported
                    if !is_format_jit_struct_supported(inner_struct) {
                        jit_diag!(
                            "Field '{}' is flattened struct but inner struct not supported",
                            field.name
                        );
                        return false;
                    }
                    // Flattened struct is OK - skip normal field type check and continue to next field
                    continue;
                }
                _ => {
                    jit_diag!(
                        "Field '{}' is flattened but type is not enum, struct, or HashMap (not supported)",
                        field.name
                    );
                    return false;
                }
            }
        }

        // No custom defaults in simple subset (Option pre-init is OK)
        if field.has_default() {
            jit_diag!("Field '{}' has custom default (not supported)", field.name);
            return false;
        }

        // Field type must be supported (for normal, non-flattened fields)
        if !is_format_jit_field_type_supported(field.shape()) {
            jit_diag!(
                "Field '{}' has unsupported type: {:?}",
                field.name,
                field.shape().def
            );
            return false;
        }
    }

    true
}

/// Check if a flattened enum is supported for Tier-2 JIT compilation.
///
/// Flattened enums have stricter requirements than regular enums:
/// - Unit variants are NOT supported (regular enums can have them)
/// - All variants must have at least one field containing payload data
/// - Otherwise, same requirements as regular enums
fn is_format_jit_flattened_enum_supported(enum_type: &facet_core::EnumType) -> bool {
    use facet_core::StructKind;

    // First check basic enum requirements
    if !is_format_jit_enum_supported(enum_type) {
        return false;
    }

    // Additional check for flattened enums: no unit variants allowed
    for variant in enum_type.variants {
        if matches!(variant.data.kind, StructKind::Unit) {
            jit_diag!(
                "Flattened enum variant '{}' is a unit variant (not yet supported for flattened enums)",
                variant.name
            );
            return false;
        }

        // Also reject variants with no fields (shouldn't happen, but be defensive)
        if variant.data.fields.is_empty() {
            jit_diag!(
                "Flattened enum variant '{}' has no fields (not yet supported)",
                variant.name
            );
            return false;
        }
    }

    true
}

/// Check if an enum is supported for Tier-2 JIT compilation (MVP).
///
/// MVP requirements:
/// - #[repr(C)] only
/// - All variants must be tuple variants with exactly one field
/// - Payload structs must be JIT-compatible
fn is_format_jit_enum_supported(enum_type: &facet_core::EnumType) -> bool {
    use facet_core::{BaseRepr, EnumRepr, ScalarType, StructKind};

    // Accept #[repr(C)] or #[repr(Rust)] with explicit discriminant (like #[repr(u8)])
    // Both are fine for our needs - we just need known layout and discriminant size
    if !matches!(enum_type.repr.base, BaseRepr::C | BaseRepr::Rust) {
        jit_diag!("Enum repr {:?} not supported", enum_type.repr.base);
        return false;
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
            jit_diag!("Enum with niche/NPO optimization (Option-like) not supported");
            return false;
        }
    }

    // Check all variants have supported field types
    for variant in enum_type.variants {
        // Verify discriminant is present
        if variant.discriminant.is_none() {
            jit_diag!("Enum variant '{}' has no discriminant value", variant.name);
            return false;
        }

        // Check variant fields based on kind
        match variant.data.kind {
            StructKind::Unit => {
                // Unit variants are always supported (for non-flattened enums)
            }
            StructKind::TupleStruct => {
                // Tuple variants: check field types
                // Most common pattern: single struct field like Password(AuthPassword)
                // But also support: multiple scalar fields or mixed types
                for field in variant.data.fields {
                    let field_shape = field.shape();

                    // Check if it's a struct payload (common for flattened enums)
                    if let facet_core::Type::User(facet_core::UserType::Struct(struct_def)) =
                        &field_shape.ty
                    {
                        // Recursively validate the struct
                        if !is_format_jit_struct_supported(struct_def) {
                            jit_diag!(
                                "Enum variant '{}' field '{}' has unsupported struct type",
                                variant.name,
                                field.name
                            );
                            return false;
                        }
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
                            jit_diag!(
                                "Enum variant '{}' field '{}' scalar type {:?} not supported",
                                variant.name,
                                field.name,
                                scalar_type
                            );
                            return false;
                        }
                    } else {
                        jit_diag!(
                            "Enum variant '{}' field '{}' type not yet supported (only structs, scalars, and strings)",
                            variant.name,
                            field.name
                        );
                        return false;
                    }
                }
            }
            StructKind::Struct | StructKind::Tuple => {
                // Named struct variants or standalone tuples - check field types same as TupleStruct
                for field in variant.data.fields {
                    let field_shape = field.shape();

                    if let facet_core::Type::User(facet_core::UserType::Struct(struct_def)) =
                        &field_shape.ty
                    {
                        if !is_format_jit_struct_supported(struct_def) {
                            jit_diag!(
                                "Enum variant '{}' field '{}' has unsupported struct type",
                                variant.name,
                                field.name
                            );
                            return false;
                        }
                    } else if let Some(scalar_type) = field_shape.scalar_type() {
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
                            jit_diag!(
                                "Enum variant '{}' field '{}' scalar type {:?} not supported",
                                variant.name,
                                field.name,
                                scalar_type
                            );
                            return false;
                        }
                    } else {
                        jit_diag!(
                            "Enum variant '{}' field '{}' type not yet supported",
                            variant.name,
                            field.name
                        );
                        return false;
                    }
                }
            }
        }
    }

    true
}

/// Check if a field type is supported for Tier-2.
///
/// Supported types:
/// - Scalars (bool, integers, floats, String)
/// - `Option<T>` where T is supported
/// - `Result<T, E>` where both T and E are supported
/// - `Vec<T>` where T is a supported element type (scalars, structs, nested Vec/Map)
/// - HashMap<String, V> where V is a supported element type
/// - Nested simple structs (recursive)
fn is_format_jit_field_type_supported(shape: &'static Shape) -> bool {
    use facet_core::ScalarType;

    // Check for Option<T>
    if let Def::Option(opt_def) = &shape.def {
        return is_format_jit_field_type_supported(opt_def.t);
    }

    // Check for Result<T, E>
    if let Def::Result(result_def) = &shape.def {
        // Both Ok and Err types must be supported
        let t_supported = is_format_jit_field_type_supported(result_def.t);
        let e_supported = is_format_jit_field_type_supported(result_def.e);

        if !t_supported {
            jit_diag!(
                "Result<T, E> not supported: Ok type ({}) is not JIT-compatible",
                std::any::type_name_of_val(&result_def.t)
            );
        }
        if !e_supported {
            jit_diag!(
                "Result<T, E> not supported: Err type ({}) is not JIT-compatible",
                std::any::type_name_of_val(&result_def.e)
            );
        }

        return t_supported && e_supported;
    }

    // Check for Vec<T>
    if let Def::List(list_def) = &shape.def {
        return is_format_jit_element_supported(list_def.t);
    }

    // Check for HashMap<String, V>
    if let Def::Map(map_def) = &shape.def {
        // Key must be String
        if map_def.k.scalar_type() != Some(ScalarType::String) {
            return false;
        }
        // Value must be a supported element type
        return is_format_jit_element_supported(map_def.v);
    }

    // Check for scalars
    if let Some(scalar_type) = shape.scalar_type() {
        return matches!(
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
        );
    }

    // Check for nested simple structs
    if let Type::User(UserType::Struct(struct_def)) = &shape.ty {
        return is_format_jit_struct_supported(struct_def);
    }

    // Check for enums (non-flattened)
    if let Type::User(UserType::Enum(enum_def)) = &shape.ty {
        return is_format_jit_enum_supported(enum_def);
    }

    false
}

/// Check if a Vec element type is supported for Tier-2.
fn is_format_jit_element_supported(elem_shape: &'static Shape) -> bool {
    use facet_core::ScalarType;

    if let Some(scalar_type) = elem_shape.scalar_type() {
        // All scalar types (including String) are supported with Tier-2 JIT.
        return matches!(
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
        );
    }

    // Support Result<T, E> elements (e.g., Vec<Result<i32, String>>)
    if let Def::Result(result_def) = &elem_shape.def {
        // Both Ok and Err types must be supported element types
        let t_supported = is_format_jit_element_supported(result_def.t);
        let e_supported = is_format_jit_element_supported(result_def.e);

        if !t_supported {
            jit_diag!(
                "Result<T, E> element not supported: Ok type ({}) is not JIT-compatible",
                std::any::type_name_of_val(&result_def.t)
            );
        }
        if !e_supported {
            jit_diag!(
                "Result<T, E> element not supported: Err type ({}) is not JIT-compatible",
                std::any::type_name_of_val(&result_def.e)
            );
        }

        return t_supported && e_supported;
    }

    // Support nested Vec<Vec<T>> by recursively checking the inner element type
    if let Def::List(list_def) = &elem_shape.def {
        return is_format_jit_element_supported(list_def.t);
    }

    // Support nested HashMap<String, V> as Vec element
    if let Def::Map(map_def) = &elem_shape.def {
        // Key must be String
        if map_def.k.scalar_type() != Some(ScalarType::String) {
            return false;
        }
        // Value must be a supported element type (recursive check)
        return is_format_jit_element_supported(map_def.v);
    }

    // Support struct elements (Vec<struct>) - but only if the struct itself is Tier-2 compatible
    if let Type::User(UserType::Struct(struct_def)) = &elem_shape.ty {
        return is_format_jit_struct_supported(struct_def);
    }

    false
}
