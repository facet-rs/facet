use const_oid::ObjectIdentifier;

use crate::{
    Def, Facet, ParseError, PtrConst, PtrMut, PtrUninit, ScalarAffinity, ScalarDef, Shape,
    TryFromError, TryIntoInnerError, Type, UserType, ValueVTable, value_vtable,
};

unsafe impl<'a, const L: usize> Facet<'a> for ObjectIdentifier<L> {
    const VTABLE: &'static ValueVTable = &const {
        unsafe fn try_from<'shape, 'dst>(
            src_ptr: PtrConst<'_>,
            src_shape: &'shape Shape,
            dst: PtrUninit<'dst>,
        ) -> Result<PtrMut<'dst>, TryFromError<'shape>> {
            if src_shape.id == <String as Facet>::SHAPE.id {
                let s = unsafe { src_ptr.read::<String>() };
                return match ObjectIdentifier::new(&s) {
                    Ok(oid) => Ok(unsafe { dst.put(oid) }),
                    Err(_) => Err(TryFromError::UnsupportedSourceShape {
                        src_shape,
                        expected: &[<&[u8] as Facet>::SHAPE, <String as Facet>::SHAPE],
                    }),
                };
            }
            if src_shape.id == <&[u8] as Facet>::SHAPE.id {
                let b = unsafe { src_ptr.read::<&[u8]>() };
                return match ObjectIdentifier::from_bytes(b) {
                    Ok(oid) => Ok(unsafe { dst.put(oid) }),
                    Err(_) => Err(TryFromError::UnsupportedSourceShape {
                        src_shape,
                        expected: &[<&[u8] as Facet>::SHAPE, <String as Facet>::SHAPE],
                    }),
                };
            }
            Err(TryFromError::UnsupportedSourceShape {
                src_shape,
                expected: &[<&[u8] as Facet>::SHAPE, <String as Facet>::SHAPE],
            })
        }

        unsafe fn try_into_inner<'dst>(
            src_ptr: PtrMut<'_>,
            dst: PtrUninit<'dst>,
        ) -> Result<PtrMut<'dst>, TryIntoInnerError> {
            let oid = unsafe { src_ptr.read::<ObjectIdentifier>() };
            Ok(unsafe { dst.put(oid.as_bytes()) })
        }

        let mut vtable = value_vtable!(ObjectIdentifier, |f, _opts| write!(
            f,
            "{}",
            Self::SHAPE.type_identifier
        ));
        {
            let vtable = vtable.sized_mut().unwrap();
            vtable.parse = || {
                Some(|s, target| match ObjectIdentifier::new(s) {
                    Ok(oid) => Ok(unsafe { target.put(oid) }),
                    Err(_) => Err(ParseError::Generic("OID parsing failed")),
                })
            };
            vtable.try_from = || Some(try_from);
            vtable.try_into_inner = || Some(try_into_inner);
        }
        vtable
    };

    const SHAPE: &'static Shape<'static> = &const {
        fn inner_shape<const L: usize>() -> &'static Shape<'static> {
            <&[u8] as Facet>::SHAPE
        }

        Shape::builder_for_sized::<Self>()
            .type_identifier("ObjectIdentifier")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar(
                ScalarDef::builder()
                    .affinity(&const { ScalarAffinity::oid().build() })
                    .build(),
            ))
            .inner(inner_shape::<L>)
            .build()
    };
}
