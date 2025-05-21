use crate::{
    Def, Facet, PtrConst, PtrMut, PtrUninit, Repr, ScalarAffinity, ScalarDef, Shape, StructType,
    TryBorrowInnerError, TryFromError, TryIntoInnerError, Type, UserType, ValueVTable,
    field_in_type, value_vtable,
};
use ordered_float::{NotNan, OrderedFloat};

macro_rules! impl_facet_for_ordered_float_and_notnan {
    ($float:ty) => {
        unsafe impl<'a> Facet<'a> for OrderedFloat<$float> {
            const VTABLE: &'static ValueVTable = &const {
                // Define conversion functions for transparency
                unsafe fn try_from<'shape, 'dst>(
                    src_ptr: PtrConst<'_>,
                    src_shape: &'shape Shape<'shape>,
                    dst: PtrUninit<'dst>,
                ) -> Result<PtrMut<'dst>, TryFromError<'shape>> {
                    if src_shape == <$float as Facet>::SHAPE {
                        // Get the inner value and wrap as OrderedFloat
                        let value = unsafe { src_ptr.get::<$float>() };
                        let ord = OrderedFloat(*value);
                        Ok(unsafe { dst.put(ord) })
                    } else {
                        let inner_try_from = (<$float as Facet>::SHAPE.vtable.try_from)().ok_or(
                            TryFromError::UnsupportedSourceShape {
                                src_shape,
                                expected: &[<$float as Facet>::SHAPE],
                            },
                        )?;
                        // fallback to inner's try_from
                        // This relies on the fact that `dst` is the same size as `OrderedFloat<$float>`
                        // which should be true because `OrderedFloat` is `repr(transparent)`
                        let inner_result = unsafe { (inner_try_from)(src_ptr, src_shape, dst) };
                        match inner_result {
                            Ok(result) => {
                                // After conversion to inner type, wrap as OrderedFloat
                                let value = unsafe { result.read::<$float>() };
                                let ord = OrderedFloat(value);
                                Ok(unsafe { dst.put(ord) })
                            }
                            Err(e) => Err(e),
                        }
                    }
                }

                // Conversion back to inner float type
                unsafe fn try_into_inner<'dst>(
                    src_ptr: PtrMut<'_>,
                    dst: PtrUninit<'dst>,
                ) -> Result<PtrMut<'dst>, TryIntoInnerError> {
                    let v = unsafe { src_ptr.read::<OrderedFloat<$float>>() };
                    Ok(unsafe { dst.put(v.0) })
                }

                // Borrow inner float type
                unsafe fn try_borrow_inner(
                    src_ptr: PtrConst<'_>,
                ) -> Result<PtrConst<'_>, TryBorrowInnerError> {
                    let v = unsafe { src_ptr.get::<OrderedFloat<$float>>() };
                    Ok(PtrConst::new((&v.0) as *const $float as *const u8))
                }

                let mut vtable = value_vtable!((), |f, _opts| write!(f, "OrderedFloat"));
                vtable.parse = || {
                    // `OrderedFloat` is `repr(transparent)`
                    (<$float as Facet>::SHAPE.vtable.parse)()
                };
                vtable.try_from = || Some(try_from);
                vtable.try_into_inner = || Some(try_into_inner);
                vtable.try_borrow_inner = || Some(try_borrow_inner);
                vtable
            };

            const SHAPE: &'static Shape<'static> = &const {
                fn inner_shape() -> &'static Shape<'static> {
                    <$float as Facet>::SHAPE
                }

                Shape::builder_for_sized::<Self>()
                    .ty(Type::User(UserType::Struct(
                        StructType::builder()
                            .repr(Repr::transparent())
                            .fields(&const { [field_in_type!(Self, 0)] })
                            .kind(crate::StructKind::Tuple)
                            .build(),
                    )))
                    .def(Def::Scalar(
                        ScalarDef::builder()
                            // Affinity: use number affinity as inner's
                            .affinity(&const { ScalarAffinity::opaque().build() })
                            .build(),
                    ))
                    .inner(inner_shape)
                    .build()
            };
        }

        unsafe impl<'a> Facet<'a> for NotNan<$float> {
            const VTABLE: &'static ValueVTable = &const {
                // Conversion from inner float type to NotNan<$float>
                unsafe fn try_from<'shape, 'dst>(
                    src_ptr: PtrConst<'_>,
                    src_shape: &'shape Shape<'shape>,
                    dst: PtrUninit<'dst>,
                ) -> Result<PtrMut<'dst>, TryFromError<'shape>> {
                    if src_shape == <$float as Facet>::SHAPE {
                        // Get the inner value and check that it's not NaN
                        let value = unsafe { *src_ptr.get::<$float>() };
                        let nn =
                            NotNan::new(value).map_err(|_| TryFromError::Generic("was NaN"))?;
                        Ok(unsafe { dst.put(nn) })
                    } else {
                        let inner_try_from = (<$float as Facet>::SHAPE.vtable.try_from)().ok_or(
                            TryFromError::UnsupportedSourceShape {
                                src_shape,
                                expected: &[<$float as Facet>::SHAPE],
                            },
                        )?;

                        // fallback to inner's try_from
                        // This relies on the fact that `dst` is the same size as `NotNan<$float>`
                        // which should be true because `NotNan` is `repr(transparent)`
                        let inner_result = unsafe { (inner_try_from)(src_ptr, src_shape, dst) };
                        match inner_result {
                            Ok(result) => {
                                // After conversion to inner type, wrap as NotNan
                                let value = unsafe { *result.get::<$float>() };
                                let nn = NotNan::new(value)
                                    .map_err(|_| TryFromError::Generic("was NaN"))?;
                                Ok(unsafe { dst.put(nn) })
                            }
                            Err(e) => Err(e),
                        }
                    }
                }

                // Conversion back to inner float type
                unsafe fn try_into_inner<'dst>(
                    src_ptr: PtrMut<'_>,
                    dst: PtrUninit<'dst>,
                ) -> Result<PtrMut<'dst>, TryIntoInnerError> {
                    let v = unsafe { src_ptr.read::<NotNan<$float>>() };
                    Ok(unsafe { dst.put(v.into_inner()) })
                }

                // Borrow inner float type
                unsafe fn try_borrow_inner(
                    src_ptr: PtrConst<'_>,
                ) -> Result<PtrConst<'_>, TryBorrowInnerError> {
                    let v = unsafe { src_ptr.get::<NotNan<$float>>() };
                    Ok(PtrConst::new(
                        (&v.into_inner()) as *const $float as *const u8,
                    ))
                }

                let mut vtable = value_vtable!((), |f, _opts| write!(f, "NotNan"));
                // Accept parsing as inner T, but enforce NotNan invariant
                vtable.parse = || {
                    Some(|s, target| match s.parse::<$float>() {
                        Ok(inner) => match NotNan::new(inner) {
                            Ok(not_nan) => Ok(unsafe { target.put(not_nan) }),
                            Err(_) => {
                                Err(crate::ParseError::Generic("NaN is not allowed for NotNan"))
                            }
                        },
                        Err(_) => Err(crate::ParseError::Generic(
                            "Failed to parse inner type for NotNan",
                        )),
                    })
                };
                vtable.try_from = || Some(try_from);
                vtable.try_into_inner = || Some(try_into_inner);
                vtable.try_borrow_inner = || Some(try_borrow_inner);
                vtable
            };

            const SHAPE: &'static Shape<'static> = &const {
                fn inner_shape() -> &'static Shape<'static> {
                    <$float as Facet>::SHAPE
                }

                Shape::builder_for_sized::<Self>()
                    .ty(Type::User(UserType::Opaque))
                    .def(Def::Scalar(
                        ScalarDef::builder()
                            .affinity(&const { ScalarAffinity::opaque().build() })
                            .build(),
                    ))
                    .inner(inner_shape)
                    .build()
            };
        }
    };
}

impl_facet_for_ordered_float_and_notnan!(f32);
impl_facet_for_ordered_float_and_notnan!(f64);
