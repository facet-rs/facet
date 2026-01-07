#![cfg(feature = "ordered-float")]

use crate::{
    Def, Facet, FieldBuilder, Repr, Shape, ShapeBuilder, StructKind, StructType, TryFromOutcome,
    Type, TypeOpsDirect, UserType, VTableDirect, type_ops_direct, vtable_direct,
};
use ordered_float::{NotNan, OrderedFloat};

macro_rules! impl_facet_for_ordered_float_and_notnan {
    ($float:ty) => {
        unsafe impl<'a> Facet<'a> for OrderedFloat<$float> {
            const SHAPE: &'static Shape = &const {
                // OrderedFloat implements Display, Debug, Hash, PartialEq, Eq, PartialOrd, Ord
                // It also implements Clone, Copy, Default, FromStr
                const VTABLE: VTableDirect = vtable_direct!(OrderedFloat<$float> =>
                    FromStr,
                    Display,
                    Debug,
                    Hash,
                    PartialEq,
                    PartialOrd,
                    Ord,
                );
                const TYPE_OPS: TypeOpsDirect = type_ops_direct!(OrderedFloat<$float> => Default, Clone);

                ShapeBuilder::for_sized::<OrderedFloat<$float>>("OrderedFloat")
                    .ty(Type::User(UserType::Struct(StructType {
                        repr: Repr::transparent(),
                        kind: StructKind::Tuple,
                        fields: &const { [FieldBuilder::new("0", crate::shape_of::<$float>, 0).build()] },
                    })))
                    .def(Def::Scalar)
                    .inner(<$float as Facet>::SHAPE)
                    .vtable_direct(&VTABLE)
                    .type_ops_direct(&TYPE_OPS)
                    .eq()
                    .copy()
                    .send()
                    .sync()
                    .build()
            };
        }

        unsafe impl<'a> Facet<'a> for NotNan<$float> {
            const SHAPE: &'static Shape = &const {
                // Custom parse function that enforces NotNan invariant
                unsafe fn parse_notnan(
                    s: &str,
                    target: *mut NotNan<$float>,
                ) -> Result<(), crate::ParseError> {
                    match s.parse::<$float>() {
                        Ok(inner) => match NotNan::new(inner) {
                            Ok(not_nan) => {
                                unsafe { target.write(not_nan) };
                                Ok(())
                            }
                            Err(_) => Err(crate::ParseError::Str("NaN is not allowed for NotNan")),
                        },
                        Err(_) => Err(crate::ParseError::Str(
                            "Failed to parse inner type for NotNan",
                        )),
                    }
                }

                // try_from implementation to accept inner float type
                unsafe fn try_from_notnan(
                    target: *mut NotNan<$float>,
                    src_shape: &'static Shape,
                    src: crate::PtrConst,
                ) -> TryFromOutcome {
                    // Accept the inner float type (floats are Copy, so we use get)
                    if src_shape.id == <$float as Facet>::SHAPE.id {
                        let float_val = unsafe { *src.get::<$float>() };
                        match NotNan::new(float_val) {
                            Ok(not_nan) => {
                                unsafe { target.write(not_nan) };
                                TryFromOutcome::Converted
                            }
                            Err(_) => TryFromOutcome::Failed("NaN is not allowed for NotNan".into()),
                        }
                    } else if src_shape.id == <f64 as Facet>::SHAPE.id {
                        // Also accept f64 and convert (for ScalarValue::F64)
                        let float_val = unsafe { *src.get::<f64>() } as $float;
                        match NotNan::new(float_val) {
                            Ok(not_nan) => {
                                unsafe { target.write(not_nan) };
                                TryFromOutcome::Converted
                            }
                            Err(_) => TryFromOutcome::Failed("NaN is not allowed for NotNan".into()),
                        }
                    } else {
                        TryFromOutcome::Unsupported
                    }
                }

                // NotNan implements Display, Debug, Hash, PartialEq, Eq, PartialOrd, Ord
                // It also implements Clone, Copy, FromStr (but we override parse)
                // It does NOT implement Default (no default value for NotNan)
                const VTABLE: VTableDirect = vtable_direct!(NotNan<$float> =>
                    Display,
                    Debug,
                    Hash,
                    PartialEq,
                    PartialOrd,
                    Ord,
                    [parse = parse_notnan],
                    [try_from = try_from_notnan],
                );
                const TYPE_OPS: TypeOpsDirect = type_ops_direct!(NotNan<$float> => Clone);

                ShapeBuilder::for_sized::<NotNan<$float>>("NotNan")
                    .ty(Type::User(UserType::Opaque))
                    .def(Def::Scalar)
                    .inner(<$float as Facet>::SHAPE)
                    .vtable_direct(&VTABLE)
                    .type_ops_direct(&TYPE_OPS)
                    .eq()
                    .copy()
                    .send()
                    .sync()
                    .build()
            };
        }
    };
}

impl_facet_for_ordered_float_and_notnan!(f32);
impl_facet_for_ordered_float_and_notnan!(f64);
