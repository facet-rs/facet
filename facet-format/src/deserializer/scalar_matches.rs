use crate::ScalarValue;
use facet_core::Def;

/// Check if a scalar value matches a target shape.
///
/// This is a non-generic function to avoid unnecessary monomorphization.
/// It determines whether a parsed scalar value can be deserialized into a given shape.
pub(crate) fn scalar_matches_shape(
    scalar: &ScalarValue<'_>,
    shape: &'static facet_core::Shape,
) -> bool {
    use facet_core::ScalarType;

    let Some(scalar_type) = shape.scalar_type() else {
        // Not a scalar type - check for Option wrapping null
        if matches!(scalar, ScalarValue::Null) {
            return matches!(shape.def, Def::Option(_));
        }
        return false;
    };

    match scalar {
        ScalarValue::Bool(_) => matches!(scalar_type, ScalarType::Bool),
        ScalarValue::Char(_) => matches!(scalar_type, ScalarType::Char),
        ScalarValue::I64(val) => {
            // I64 matches signed types directly
            if matches!(
                scalar_type,
                ScalarType::I8
                    | ScalarType::I16
                    | ScalarType::I32
                    | ScalarType::I64
                    | ScalarType::I128
                    | ScalarType::ISize
            ) {
                return true;
            }

            // I64 can also match unsigned types if the value is non-negative and in range
            // This handles TOML's requirement to represent all integers as i64
            if *val >= 0 {
                let uval = *val as u64;
                match scalar_type {
                    ScalarType::U8 => uval <= u8::MAX as u64,
                    ScalarType::U16 => uval <= u16::MAX as u64,
                    ScalarType::U32 => uval <= u32::MAX as u64,
                    ScalarType::U64 | ScalarType::U128 | ScalarType::USize => true,
                    _ => false,
                }
            } else {
                false
            }
        }
        ScalarValue::U64(val) => {
            // U64 matches unsigned types directly
            if matches!(
                scalar_type,
                ScalarType::U8
                    | ScalarType::U16
                    | ScalarType::U32
                    | ScalarType::U64
                    | ScalarType::U128
                    | ScalarType::USize
            ) {
                return true;
            }

            // U64 can also match signed types if the value fits in the signed range
            // This handles JSON's representation of positive integers as u64
            if *val <= i64::MAX as u64 {
                match scalar_type {
                    ScalarType::I8 => *val <= i8::MAX as u64,
                    ScalarType::I16 => *val <= i16::MAX as u64,
                    ScalarType::I32 => *val <= i32::MAX as u64,
                    ScalarType::I64 | ScalarType::I128 | ScalarType::ISize => true,
                    _ => false,
                }
            } else {
                false
            }
        }
        ScalarValue::U128(_) => matches!(scalar_type, ScalarType::U128 | ScalarType::I128),
        ScalarValue::I128(_) => matches!(scalar_type, ScalarType::I128 | ScalarType::U128),
        ScalarValue::F64(_) => matches!(scalar_type, ScalarType::F32 | ScalarType::F64),
        ScalarValue::Str(s) => {
            // String scalars match string types directly
            if matches!(
                scalar_type,
                ScalarType::String | ScalarType::Str | ScalarType::CowStr | ScalarType::Char
            ) {
                return true;
            }
            // For other scalar types, check if the shape has a parse function
            // and if so, try parsing the string to see if it would succeed.
            // This enables untagged enums to correctly match string values like "4.625"
            // to the appropriate variant (f64 vs i64).
            // See #1615 for discussion of this double-parse pattern.
            const PARSE_PROBE_SIZE: usize = 128;
            #[repr(align(64))]
            struct ParseProbeStorage([u8; PARSE_PROBE_SIZE]);
            #[allow(unsafe_code)]
            if shape.vtable.has_parse()
                && shape.layout.sized_layout().is_ok_and(|layout| {
                    layout.size() <= PARSE_PROBE_SIZE
                        && layout.align() <= core::mem::align_of::<ParseProbeStorage>()
                })
            {
                // Attempt to parse - this is a probe, not the actual deserialization
                let mut temp = core::mem::MaybeUninit::<ParseProbeStorage>::uninit();
                let temp_bytes_ptr = unsafe { core::ptr::addr_of_mut!((*temp.as_mut_ptr()).0) };
                let temp_ptr = facet_core::PtrUninit::new(temp_bytes_ptr.cast::<u8>());
                // SAFETY: temp buffer is properly aligned and sized for this shape
                if let Some(Ok(())) = unsafe { shape.call_parse(s.as_ref(), temp_ptr) } {
                    // Parse succeeded - drop the temp value
                    // SAFETY: we just successfully parsed into temp_ptr
                    unsafe { shape.call_drop_in_place(temp_ptr.assume_init()) };
                    return true;
                }
            }
            false
        }
        ScalarValue::Bytes(_) => {
            // Bytes don't have a ScalarType - would need to check for Vec<u8> or [u8]
            false
        }
        ScalarValue::Null => {
            // Null matches Unit type
            matches!(scalar_type, ScalarType::Unit)
        }
        ScalarValue::Unit => {
            // Unit matches Unit type
            matches!(scalar_type, ScalarType::Unit)
        }
    }
}

/// Return how strong a scalar-to-shape match is.
///
/// - `0`: direct type match (preferred)
/// - `1`: coercive/parsed match (fallback)
///
/// `None` means the scalar cannot be deserialized into the shape.
pub(crate) fn scalar_match_quality(
    scalar: &ScalarValue<'_>,
    shape: &'static facet_core::Shape,
) -> Option<u8> {
    if !scalar_matches_shape(scalar, shape) {
        return None;
    }

    if is_exact_scalar_match(scalar, shape) {
        Some(0)
    } else {
        Some(1)
    }
}

fn is_exact_scalar_match(scalar: &ScalarValue<'_>, shape: &'static facet_core::Shape) -> bool {
    use facet_core::ScalarType;

    let scalar_type = shape.scalar_type();

    match scalar {
        ScalarValue::Bool(_) => matches!(scalar_type, Some(ScalarType::Bool)),
        ScalarValue::Char(_) => matches!(scalar_type, Some(ScalarType::Char)),
        ScalarValue::I64(_) | ScalarValue::U64(_) | ScalarValue::U128(_) | ScalarValue::I128(_) => {
            matches!(
                scalar_type,
                Some(
                    ScalarType::U8
                        | ScalarType::U16
                        | ScalarType::U32
                        | ScalarType::U64
                        | ScalarType::U128
                        | ScalarType::USize
                        | ScalarType::I8
                        | ScalarType::I16
                        | ScalarType::I32
                        | ScalarType::I64
                        | ScalarType::I128
                        | ScalarType::ISize
                )
            )
        }
        ScalarValue::F64(_) => matches!(scalar_type, Some(ScalarType::F32 | ScalarType::F64)),
        ScalarValue::Str(_) => {
            // The string-shaped scalars are exactly these four. This used to sniff
            // `shape.type_identifier` for `"::String"` and `"Cow<str"`, which could
            // never match: `type_identifier` is the *bare* name — no module path and
            // no generics — so it reads `"String"` and `"Cow"`, never `"Cow<str>"`.
            // (`"Cow<str>"` is what `format!("{shape}")` renders; the two are not the
            // same thing.) The upshot was that `Cow<'_, str>` fell through to `false`
            // and got ranked a *coercive* rather than exact match, losing untagged
            // enum variant selection to anything that scored exact.
            matches!(
                scalar_type,
                Some(
                    ScalarType::Str
                        | ScalarType::Char
                        | ScalarType::String
                        | ScalarType::CowStr
                )
            )
        }
        ScalarValue::Null => {
            matches!(scalar_type, Some(ScalarType::Unit))
                || (scalar_type.is_none() && matches!(shape.def, Def::Option(_)))
        }
        ScalarValue::Unit => matches!(scalar_type, Some(ScalarType::Unit)),
        ScalarValue::Bytes(_) => false,
    }
}

#[cfg(test)]
mod tests {
    extern crate alloc;

    use super::*;
    use crate::ScalarValue;
    use alloc::borrow::Cow;
    use facet_core::Facet;

    /// Every string-shaped type must rank as an *exact* match for a string
    /// scalar — not merely a coercive one.
    ///
    /// `Cow<'_, str>` used to fail this. The check sniffed `type_identifier` for
    /// `"Cow<str"`, but `type_identifier` is the *bare* name and reads `"Cow"`;
    /// `"Cow<str>"` is only what `Display` renders. So `Cow<'_, str>` scored 1
    /// (coercive) instead of 0 (exact) and lost untagged enum variant selection
    /// to any sibling that scored 0.
    ///
    /// Asserted as a table so a fifth string type cannot be added without a row.
    #[test]
    fn string_shaped_types_are_exact_matches_for_str_scalars() {
        let rows: &[(&str, &'static facet_core::Shape)] = &[
            ("String", <alloc::string::String as Facet>::SHAPE),
            ("Cow<'_, str>", <Cow<'_, str> as Facet>::SHAPE),
            ("&str", <&str as Facet>::SHAPE),
            ("char", <char as Facet>::SHAPE),
        ];

        let scalar = ScalarValue::Str(Cow::Borrowed("hello"));

        let failures: alloc::vec::Vec<&str> = rows
            .iter()
            .filter(|(_, shape)| !is_exact_scalar_match(&scalar, shape))
            .map(|(label, _)| *label)
            .collect();

        assert!(
            failures.is_empty(),
            "these string-shaped types were not treated as exact matches for a \
             string scalar, so they lose untagged-enum selection to anything that \
             is: {failures:?}"
        );
    }

    /// The guard above is only meaningful if non-string types still fail it.
    #[test]
    fn non_string_types_are_not_exact_matches_for_str_scalars() {
        let scalar = ScalarValue::Str(Cow::Borrowed("hello"));
        assert!(!is_exact_scalar_match(&scalar, <u32 as Facet>::SHAPE));
        assert!(!is_exact_scalar_match(&scalar, <bool as Facet>::SHAPE));
    }
}
