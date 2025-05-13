use crate::{
    Characteristic, Def, Facet, PtrConst, PtrMut, PtrUninit, Repr, ScalarAffinity, ScalarDef,
    Shape, StructType, TryBorrowInnerError, TryFromError, TryIntoInnerError, Type, UserType,
    ValueVTable, field_in_type, value_vtable,
};
use ordered_float::{NotNan, OrderedFloat};

unsafe impl<'a, T: Facet<'a>> Facet<'a> for OrderedFloat<T> {
    const VTABLE: &'static ValueVTable = &const {
        // Conversion from inner float type to OrderedFloat<T>
        unsafe fn try_from<'a, 'dst, T: Facet<'a>>(
            src_ptr: PtrConst<'_>,
            src_shape: &'static Shape,
            dst: PtrUninit<'dst>,
        ) -> Result<PtrMut<'dst>, TryFromError> {
            // Only support conversion if shapes match the inner T
            if src_shape.id != <T as Facet>::SHAPE.id {
                return Err(TryFromError::UnsupportedSourceShape {
                    src_shape,
                    expected: &[<T as Facet>::SHAPE],
                });
            }
            let v = unsafe { src_ptr.read::<T>() };
            Ok(unsafe { dst.put(OrderedFloat(v)) })
        }

        // Conversion back to inner float type
        unsafe fn try_into_inner<'a, 'dst, T: Facet<'a>>(
            src_ptr: PtrMut<'_>,
            dst: PtrUninit<'dst>,
        ) -> Result<PtrMut<'dst>, TryIntoInnerError> {
            let v = unsafe { src_ptr.read::<OrderedFloat<T>>() };
            Ok(unsafe { dst.put(v.0) })
        }

        // Borrow inner float type
        unsafe fn try_borrow_inner<'a, T: Facet<'a>>(
            src_ptr: PtrConst<'_>,
        ) -> Result<PtrConst<'_>, TryBorrowInnerError> {
            let v = unsafe { src_ptr.get::<OrderedFloat<T>>() };
            Ok(PtrConst::new((&v.0) as *const T as *const u8))
        }

        let mut vtable = value_vtable!((), |f, _opts| write!(f, "OrderedFloat"));
        if <T as Facet>::SHAPE.is(Characteristic::FromStr) {
            let inner_parse = unsafe { <T as Facet>::SHAPE.vtable.parse.unwrap_unchecked() };
            // `OrderedFloat` is `repr(transparent)`
            vtable.parse = Some(inner_parse);
        }
        vtable.try_from = Some(try_from::<T>);
        vtable.try_into_inner = Some(try_into_inner::<T>);
        vtable.try_borrow_inner = Some(try_borrow_inner::<T>);
        vtable
    };

    const SHAPE: &'static Shape = &const {
        fn inner_shape<'a, T: Facet<'a>>() -> &'static Shape {
            <T as Facet>::SHAPE
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
                    .affinity(ScalarAffinity::opaque().build())
                    .build(),
            ))
            .inner(inner_shape::<T>)
            .build()
    };
}

unsafe impl<'a, T: Facet<'a> + ordered_float::FloatCore + Clone + core::str::FromStr> Facet<'a>
    for NotNan<T>
{
    const VTABLE: &'static ValueVTable = &const {
        // Conversion from inner float type to NotNan<T>
        unsafe fn try_from<'a, 'dst, T: Facet<'a> + ordered_float::FloatCore + Clone>(
            src_ptr: PtrConst<'_>,
            src_shape: &'static Shape,
            dst: PtrUninit<'dst>,
        ) -> Result<PtrMut<'dst>, TryFromError> {
            if src_shape.id != <T as Facet>::SHAPE.id {
                return Err(TryFromError::UnsupportedSourceShape {
                    src_shape,
                    expected: &[<T as Facet>::SHAPE],
                });
            }
            let v = unsafe { src_ptr.read::<T>() };
            match NotNan::new(v) {
                Ok(not_nan) => Ok(unsafe { dst.put(not_nan) }),
                Err(_) => Err(TryFromError::Generic("was NaN")),
            }
        }

        // Conversion back to inner float type
        unsafe fn try_into_inner<'a, 'dst, T: Facet<'a> + ordered_float::FloatCore + Clone>(
            src_ptr: PtrMut<'_>,
            dst: PtrUninit<'dst>,
        ) -> Result<PtrMut<'dst>, TryIntoInnerError> {
            let v = unsafe { src_ptr.read::<NotNan<T>>() };
            Ok(unsafe { dst.put(v.into_inner()) })
        }

        // Borrow inner float type
        unsafe fn try_borrow_inner<'a, T: Facet<'a> + ordered_float::FloatCore + Clone>(
            src_ptr: PtrConst<'_>,
        ) -> Result<PtrConst<'_>, TryBorrowInnerError> {
            let v = unsafe { src_ptr.get::<NotNan<T>>() };
            Ok(PtrConst::new((&v.into_inner()) as *const T as *const u8))
        }

        let mut vtable = value_vtable!((), |f, _opts| write!(f, "NotNan"));
        // Accept parsing as inner T, but enforce NotNan invariant
        vtable.parse = Some(|s, target| match s.parse::<T>() {
            Ok(inner) => match NotNan::new(inner) {
                Ok(not_nan) => Ok(unsafe { target.put(not_nan) }),
                Err(_) => Err(crate::ParseError::Generic("NaN is not allowed for NotNan")),
            },
            Err(_) => Err(crate::ParseError::Generic(
                "Failed to parse inner type for NotNan",
            )),
        });
        vtable.try_from = Some(try_from::<T>);
        vtable.try_into_inner = Some(try_into_inner::<T>);
        vtable.try_borrow_inner = Some(try_borrow_inner::<T>);
        vtable
    };

    const SHAPE: &'static Shape = &const {
        fn inner_shape<'a, T: Facet<'a>>() -> &'static Shape {
            <T as Facet>::SHAPE
        }

        Shape::builder_for_sized::<Self>()
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar(
                ScalarDef::builder()
                    .affinity(ScalarAffinity::opaque().build())
                    .build(),
            ))
            .inner(inner_shape::<T>)
            .build()
    };
}
