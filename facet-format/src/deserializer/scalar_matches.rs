extern crate alloc;

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
            #[allow(unsafe_code)]
            if shape.vtable.has_parse()
                && shape
                    .layout
                    .sized_layout()
                    .is_ok_and(|layout| layout.size() <= 128)
            {
                // Attempt to parse - this is a probe, not the actual deserialization
                let mut temp = [0u8; 128];
                let temp_ptr = facet_core::PtrUninit::new(temp.as_mut_ptr());
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
