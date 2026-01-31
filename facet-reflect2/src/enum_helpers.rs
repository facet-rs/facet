//! Helpers for enum construction.

use crate::errors::ReflectErrorKind;
use facet_core::{EnumRepr, EnumType, PtrConst, PtrUninit, Variant};

macro_rules! impl_discriminant_ops {
    ($($repr:ident => $ty:ty),* $(,)?) => {
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
                    $(EnumRepr::$repr => *(data.as_byte_ptr() as *const $ty) as i64,)*
                    EnumRepr::RustNPO => return Err(ReflectErrorKind::UnsupportedEnumRepr),
                };
                Ok(discriminant)
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
                    $(EnumRepr::$repr => {
                        *(data.as_mut_byte_ptr() as *mut $ty) = discriminant as $ty;
                    })*
                    EnumRepr::RustNPO => return Err(ReflectErrorKind::UnsupportedEnumRepr),
                }
            }
            Ok(())
        }
    };
}

impl_discriminant_ops! {
    U8 => u8,
    U16 => u16,
    U32 => u32,
    U64 => u64,
    I8 => i8,
    I16 => i16,
    I32 => i32,
    I64 => i64,
    USize => usize,
    ISize => isize,
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
