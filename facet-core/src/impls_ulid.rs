use alloc::string::String;
use core::alloc::Layout;

use ulid::Ulid;

use crate::{
    ConstTypeId, Def, Facet, ParseError, PtrConst, PtrMut, PtrUninit, ScalarAffinity, ScalarDef,
    Shape, TryFromError, TryIntoInnerError, value_vtable,
};

unsafe impl Facet<'_> for Ulid {
    const SHAPE: &'static Shape = &const {
        // Functions to transparently convert between Ulid and String
        unsafe fn try_from<'dst>(
            src_ptr: PtrConst<'_>,
            src_shape: &'static Shape,
            dst: PtrUninit<'dst>,
        ) -> Result<PtrMut<'dst>, TryFromError> {
            if src_shape.id != <String as Facet>::SHAPE.id {
                return Err(TryFromError::UnsupportedSourceShape {
                    src_shape,
                    expected: &[<String as Facet>::SHAPE],
                });
            }
            let s = unsafe { src_ptr.get::<String>() };
            match Ulid::from_string(s) {
                Ok(ulid) => Ok(unsafe { dst.put(ulid) }),
                Err(_) => Err(TryFromError::UnsupportedSourceShape {
                    src_shape,
                    expected: &[<String as Facet>::SHAPE],
                }),
            }
        }

        unsafe fn try_into_inner<'dst>(
            src_ptr: PtrConst<'_>,
            dst: PtrUninit<'dst>,
        ) -> Result<PtrMut<'dst>, TryIntoInnerError> {
            let ulid = unsafe { src_ptr.get::<Ulid>() };
            Ok(unsafe { dst.put(ulid.to_string()) })
        }

        // Return the Shape of the inner type (String)
        fn inner_shape() -> &'static Shape {
            <String as Facet>::SHAPE
        }

        Shape::builder()
            .id(ConstTypeId::of::<Self>())
            .layout(Layout::new::<Self>())
            .def(Def::Scalar(
                ScalarDef::builder()
                    .affinity(ScalarAffinity::ulid().build())
                    .build(),
            ))
            .vtable(
                &const {
                    let mut vtable = value_vtable!((), |f, _opts| write!(f, "Ulid"));
                    vtable.parse = Some(|s, target| match Ulid::from_string(s) {
                        Ok(ulid) => Ok(unsafe { target.put(ulid) }),
                        Err(_) => Err(ParseError::Generic("ULID parsing failed")),
                    });
                    vtable.try_from = Some(try_from);
                    vtable.try_into_inner = Some(try_into_inner);
                    vtable
                },
            )
            .inner(inner_shape)
            .build()
    };
}
