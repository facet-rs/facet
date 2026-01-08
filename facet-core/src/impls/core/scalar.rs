//! Scalar type implementations: bool, char, integers, floats
//!
//! Note: ConstTypeId, TypeId are in typeid.rs
//! Note: (), PhantomData are in tuple_empty.rs and phantom.rs
//! Note: str, char are in char_str.rs

extern crate alloc;

use crate::{
    Def, Facet, NumericType, PrimitiveType, PtrConst, Shape, ShapeBuilder, TryFromOutcome, Type,
    TypeOpsDirect, VTableDirect, type_ops_direct, vtable_direct,
};

/// Generate a try_from function for an integer type that converts from any other integer.
macro_rules! integer_try_from {
    ($target:ty) => {{
        /// # Safety
        /// `dst` must be valid for writes, `src` must point to valid data of type described by `src_shape`
        unsafe fn try_from_any(
            dst: *mut $target,
            src_shape: &'static Shape,
            src: PtrConst,
        ) -> TryFromOutcome {
            use core::convert::TryInto;

            // Helper macro to handle the conversion with proper error handling
            // Note: integers are Copy, so reading doesn't consume in a meaningful way
            macro_rules! convert {
                ($src_ty:ty) => {{
                    let src_val = unsafe { *(src.as_byte_ptr() as *const $src_ty) };
                    <$src_ty as TryInto<$target>>::try_into(src_val).map_err(|_| {
                        alloc::format!(
                            "conversion from {} to {} failed: value {} out of range",
                            src_shape.type_identifier,
                            stringify!($target),
                            src_val
                        )
                    })
                }};
            }

            // Match on the source shape to determine its type
            let result: Result<$target, alloc::string::String> = match src_shape.type_identifier {
                "i8" => convert!(i8),
                "i16" => convert!(i16),
                "i32" => convert!(i32),
                "i64" => convert!(i64),
                "i128" => convert!(i128),
                "isize" => convert!(isize),
                "u8" => convert!(u8),
                "u16" => convert!(u16),
                "u32" => convert!(u32),
                "u64" => convert!(u64),
                "u128" => convert!(u128),
                "usize" => convert!(usize),
                _ => return TryFromOutcome::Unsupported,
            };

            match result {
                Ok(value) => {
                    unsafe { dst.write(value) };
                    TryFromOutcome::Converted
                }
                Err(e) => TryFromOutcome::Failed(e.into()),
            }
        }
        try_from_any
    }};
}

// Truthiness helpers + TypeOps lifted out of const blocks - shared statics

#[inline(always)]
unsafe fn bool_truthy(value: PtrConst) -> bool {
    *unsafe { value.get::<bool>() }
}

macro_rules! define_int_type_ops {
    ($const_name:ident, $ty:ty, $fn_name:ident) => {
        #[inline(always)]
        unsafe fn $fn_name(value: PtrConst) -> bool {
            *unsafe { value.get::<$ty>() } != 0
        }

        static $const_name: TypeOpsDirect = TypeOpsDirect {
            is_truthy: Some($fn_name),
    ..type_ops_direct!($ty => Default, Clone)
        };
    };
}

macro_rules! define_float_type_ops {
    ($const_name:ident, $ty:ty, $fn_name:ident) => {
        #[inline(always)]
        unsafe fn $fn_name(value: PtrConst) -> bool {
            let v = *unsafe { value.get::<$ty>() };
            v != 0.0 && !v.is_nan()
        }

        static $const_name: TypeOpsDirect = TypeOpsDirect {
            is_truthy: Some($fn_name),
    ..type_ops_direct!($ty => Default, Clone)
        };
    };
}

static BOOL_TYPE_OPS: TypeOpsDirect = TypeOpsDirect {
    is_truthy: Some(bool_truthy),
    ..type_ops_direct!(bool => Default, Clone)
};

define_int_type_ops!(U8_TYPE_OPS, u8, u8_truthy);
define_int_type_ops!(I8_TYPE_OPS, i8, i8_truthy);
define_int_type_ops!(U16_TYPE_OPS, u16, u16_truthy);
define_int_type_ops!(I16_TYPE_OPS, i16, i16_truthy);
define_int_type_ops!(U32_TYPE_OPS, u32, u32_truthy);
define_int_type_ops!(I32_TYPE_OPS, i32, i32_truthy);
define_int_type_ops!(U64_TYPE_OPS, u64, u64_truthy);
define_int_type_ops!(I64_TYPE_OPS, i64, i64_truthy);
define_int_type_ops!(U128_TYPE_OPS, u128, u128_truthy);
define_int_type_ops!(I128_TYPE_OPS, i128, i128_truthy);
define_int_type_ops!(USIZE_TYPE_OPS, usize, usize_truthy);
define_int_type_ops!(ISIZE_TYPE_OPS, isize, isize_truthy);

define_float_type_ops!(F32_TYPE_OPS, f32, f32_truthy);
define_float_type_ops!(F64_TYPE_OPS, f64, f64_truthy);

unsafe impl Facet<'_> for bool {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableDirect = vtable_direct!(bool =>
            FromStr,
            Display,
            Debug,
            Hash,
            PartialEq,
            PartialOrd,
            Ord,
        );

        ShapeBuilder::for_sized::<bool>("bool")
            .ty(Type::Primitive(PrimitiveType::Boolean))
            .def(Def::Scalar)
            .vtable_direct(&VTABLE)
            .type_ops_direct(&BOOL_TYPE_OPS)
            .eq()
            .copy()
            .send()
            .sync()
            .build()
    };
}

macro_rules! impl_facet_for_integer {
    ($type:ty, $type_ops:expr) => {
        unsafe impl<'a> Facet<'a> for $type {
            const SHAPE: &'static Shape = &const {
                const VTABLE: VTableDirect = vtable_direct!($type =>
                    FromStr,
                    Display,
                    Debug,
                    Hash,
                    PartialEq,
                    PartialOrd,
                    Ord,
                    [try_from = integer_try_from!($type)],
                );

                ShapeBuilder::for_sized::<$type>(stringify!($type))
                    .ty(Type::Primitive(PrimitiveType::Numeric(
                        NumericType::Integer {
                            signed: (1 as $type).checked_neg().is_some(),
                        },
                    )))
                    .def(Def::Scalar)
                    .vtable_direct(&VTABLE)
                    .type_ops_direct($type_ops)
                    .eq()
                    .copy()
                    .send()
                    .sync()
                    .build()
            };
        }
    };
}

impl_facet_for_integer!(u8, &U8_TYPE_OPS);
impl_facet_for_integer!(i8, &I8_TYPE_OPS);
impl_facet_for_integer!(u16, &U16_TYPE_OPS);
impl_facet_for_integer!(i16, &I16_TYPE_OPS);
impl_facet_for_integer!(u32, &U32_TYPE_OPS);
impl_facet_for_integer!(i32, &I32_TYPE_OPS);
impl_facet_for_integer!(u64, &U64_TYPE_OPS);
impl_facet_for_integer!(i64, &I64_TYPE_OPS);
impl_facet_for_integer!(u128, &U128_TYPE_OPS);
impl_facet_for_integer!(i128, &I128_TYPE_OPS);
impl_facet_for_integer!(usize, &USIZE_TYPE_OPS);
impl_facet_for_integer!(isize, &ISIZE_TYPE_OPS);

unsafe impl Facet<'_> for f32 {
    const SHAPE: &'static Shape = &const {
        // f32 implements Debug, Display, Clone, Copy, Default, PartialEq, PartialOrd
        // but NOT Eq, Ord, or Hash (because of NaN)
        const VTABLE: VTableDirect = vtable_direct!(f32 =>
            FromStr,
            Display,
            Debug,
            PartialEq,
            PartialOrd,
        );

        ShapeBuilder::for_sized::<f32>("f32")
            .ty(Type::Primitive(PrimitiveType::Numeric(NumericType::Float)))
            .def(Def::Scalar)
            .vtable_direct(&VTABLE)
            .type_ops_direct(&F32_TYPE_OPS)
            .copy()
            .send()
            .sync()
            .build()
    };
}

unsafe impl Facet<'_> for f64 {
    const SHAPE: &'static Shape = &const {
        // f64 implements Debug, Display, Clone, Copy, Default, PartialEq, PartialOrd
        // but NOT Eq, Ord, or Hash (because of NaN)
        const VTABLE: VTableDirect = vtable_direct!(f64 =>
            FromStr,
            Display,
            Debug,
            PartialEq,
            PartialOrd,
        );

        ShapeBuilder::for_sized::<f64>("f64")
            .ty(Type::Primitive(PrimitiveType::Numeric(NumericType::Float)))
            .def(Def::Scalar)
            .vtable_direct(&VTABLE)
            .type_ops_direct(&F64_TYPE_OPS)
            .copy()
            .send()
            .sync()
            .build()
    };
}

#[cfg(test)]
mod tests {
    use crate::{Facet, TypeOps};

    #[test]
    fn test_scalar_shapes() {
        assert!(bool::SHAPE.vtable.has_debug());
        assert!(bool::SHAPE.vtable.has_display());
        // Default is now in type_ops
        assert!(
            matches!(bool::SHAPE.type_ops, Some(TypeOps::Direct(ops)) if ops.default_in_place.is_some())
        );

        assert!(u32::SHAPE.vtable.has_debug());
        assert!(u32::SHAPE.vtable.has_display());
        assert!(u32::SHAPE.vtable.has_hash());

        assert!(f64::SHAPE.vtable.has_debug());
        assert!(f64::SHAPE.vtable.has_display());
        assert!(!f64::SHAPE.vtable.has_hash()); // floats don't have Hash
    }
}
