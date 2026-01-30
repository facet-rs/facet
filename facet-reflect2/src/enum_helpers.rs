//! Helpers for enum construction.

use crate::errors::ReflectErrorKind;
use facet_core::{EnumRepr, EnumType, PtrConst, PtrUninit, Variant};

/// Read the discriminant from an initialized enum value.
///
/// # Safety
/// - `data` must point to a valid, initialized enum value of type `enum_type`
pub unsafe fn read_discriminant(
    data: PtrConst,
    enum_type: &EnumType,
) -> Result<i64, ReflectErrorKind> {
    unsafe {
        let discriminant = match enum_type.enum_repr {
            EnumRepr::U8 => *data.as_byte_ptr() as i64,
            EnumRepr::U16 => *(data.as_byte_ptr() as *const u16) as i64,
            EnumRepr::U32 => *(data.as_byte_ptr() as *const u32) as i64,
            EnumRepr::U64 => *(data.as_byte_ptr() as *const u64) as i64,
            EnumRepr::I8 => *(data.as_byte_ptr() as *const i8) as i64,
            EnumRepr::I16 => *(data.as_byte_ptr() as *const i16) as i64,
            EnumRepr::I32 => *(data.as_byte_ptr() as *const i32) as i64,
            EnumRepr::I64 => *(data.as_byte_ptr() as *const i64),
            EnumRepr::USize => *(data.as_byte_ptr() as *const usize) as i64,
            EnumRepr::ISize => *(data.as_byte_ptr() as *const isize) as i64,
            EnumRepr::RustNPO => return Err(ReflectErrorKind::UnsupportedEnumRepr),
        };
        Ok(discriminant)
    }
}

/// Find the variant index for a given discriminant value.
pub fn variant_index_from_discriminant(enum_type: &EnumType, discriminant: i64) -> Option<u32> {
    for (i, variant) in enum_type.variants.iter().enumerate() {
        if variant.discriminant == Some(discriminant) {
            return Some(i as u32);
        }
    }
    None
}

/// Drop the fields of an enum variant in place.
///
/// # Safety
/// - `data` must point to a valid enum value with the given variant active
/// - The variant's fields must be initialized
pub unsafe fn drop_variant_fields(data: PtrConst, variant: &Variant) {
    use facet_core::PtrMut;
    for field in variant.data.fields.iter() {
        let field_ptr = unsafe { data.as_byte_ptr().add(field.offset) };
        let field_shape = field.shape();
        // SAFETY: field_ptr points to an initialized field of the correct type
        // We cast to PtrMut because drop_in_place needs mutable access
        unsafe {
            field_shape.call_drop_in_place(PtrMut::new(field_ptr as *mut u8));
        }
    }
}

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
