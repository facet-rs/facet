//! Helpers for enum construction.

use crate::errors::ReflectErrorKind;
use facet_core::{EnumRepr, EnumType, PtrUninit, Variant};

/// Write the discriminant for an enum variant.
///
/// # Safety
/// - `data` must point to valid memory for the enum
/// - `variant` must be a valid variant of `enum_type`
pub unsafe fn write_discriminant(
    data: PtrUninit,
    enum_type: &EnumType,
    variant: &Variant,
) -> Result<(), ReflectErrorKind> {
    let Some(discriminant) = variant.discriminant else {
        return Err(ReflectErrorKind::UnsupportedEnumRepr);
    };

    unsafe {
        match enum_type.enum_repr {
            EnumRepr::U8 => {
                let ptr = data.as_mut_byte_ptr();
                *ptr = discriminant as u8;
            }
            EnumRepr::U16 => {
                let ptr = data.as_mut_byte_ptr() as *mut u16;
                *ptr = discriminant as u16;
            }
            EnumRepr::U32 => {
                let ptr = data.as_mut_byte_ptr() as *mut u32;
                *ptr = discriminant as u32;
            }
            EnumRepr::U64 => {
                let ptr = data.as_mut_byte_ptr() as *mut u64;
                *ptr = discriminant as u64;
            }
            EnumRepr::I8 => {
                let ptr = data.as_mut_byte_ptr() as *mut i8;
                *ptr = discriminant as i8;
            }
            EnumRepr::I16 => {
                let ptr = data.as_mut_byte_ptr() as *mut i16;
                *ptr = discriminant as i16;
            }
            EnumRepr::I32 => {
                let ptr = data.as_mut_byte_ptr() as *mut i32;
                *ptr = discriminant as i32;
            }
            EnumRepr::I64 => {
                let ptr = data.as_mut_byte_ptr() as *mut i64;
                *ptr = discriminant;
            }
            EnumRepr::USize => {
                let ptr = data.as_mut_byte_ptr() as *mut usize;
                *ptr = discriminant as usize;
            }
            EnumRepr::ISize => {
                let ptr = data.as_mut_byte_ptr() as *mut isize;
                *ptr = discriminant as isize;
            }
            EnumRepr::RustNPO => {
                return Err(ReflectErrorKind::UnsupportedEnumRepr);
            }
        }
    }
    Ok(())
}
