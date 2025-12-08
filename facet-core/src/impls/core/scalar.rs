//! Scalar type implementations: bool, char, integers, floats
//!
//! Note: ConstTypeId, TypeId are in typeid.rs
//! Note: (), PhantomData are in tuple_empty.rs and phantom.rs
//! Note: str, char are in char_str.rs

use crate::{
    Def, Facet, NumericType, PrimitiveType, Shape, ShapeBuilder, Type, TypeOpsDirect, VTableDirect,
    type_ops_direct, vtable_direct,
};

// TypeOps lifted out of const blocks - shared statics
static BOOL_TYPE_OPS: TypeOpsDirect = type_ops_direct!(bool => Default, Clone);
static U8_TYPE_OPS: TypeOpsDirect = type_ops_direct!(u8 => Default, Clone);
static I8_TYPE_OPS: TypeOpsDirect = type_ops_direct!(i8 => Default, Clone);
static U16_TYPE_OPS: TypeOpsDirect = type_ops_direct!(u16 => Default, Clone);
static I16_TYPE_OPS: TypeOpsDirect = type_ops_direct!(i16 => Default, Clone);
static U32_TYPE_OPS: TypeOpsDirect = type_ops_direct!(u32 => Default, Clone);
static I32_TYPE_OPS: TypeOpsDirect = type_ops_direct!(i32 => Default, Clone);
static U64_TYPE_OPS: TypeOpsDirect = type_ops_direct!(u64 => Default, Clone);
static I64_TYPE_OPS: TypeOpsDirect = type_ops_direct!(i64 => Default, Clone);
static U128_TYPE_OPS: TypeOpsDirect = type_ops_direct!(u128 => Default, Clone);
static I128_TYPE_OPS: TypeOpsDirect = type_ops_direct!(i128 => Default, Clone);
static USIZE_TYPE_OPS: TypeOpsDirect = type_ops_direct!(usize => Default, Clone);
static ISIZE_TYPE_OPS: TypeOpsDirect = type_ops_direct!(isize => Default, Clone);
static F32_TYPE_OPS: TypeOpsDirect = type_ops_direct!(f32 => Default, Clone);
static F64_TYPE_OPS: TypeOpsDirect = type_ops_direct!(f64 => Default, Clone);

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
