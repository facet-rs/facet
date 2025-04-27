use alloc::string::String;
use alloc::string::ToString;
use core::alloc::Layout;

use uuid::Uuid;

use crate::{
    ConstTypeId, Def, Facet, ParseError, PtrConst, PtrMut, PtrUninit, ScalarAffinity, ScalarDef,
    Shape, TryBorrowInnerError, TryFromError, TryIntoInnerError, value_vtable,
};

unsafe impl Facet<'_> for Uuid {
    const SHAPE: &'static Shape = &const {
        // Functions to transparently convert between Uuid and String
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
            match Uuid::parse_str(s) {
                Ok(uuid) => Ok(unsafe { dst.put(uuid) }),
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
            let uuid = unsafe { src_ptr.get::<Uuid>() };
            Ok(unsafe { dst.put(uuid.to_string()) })
        }

        unsafe fn try_borrow_inner(
            src_ptr: PtrConst<'_>,
        ) -> Result<PtrConst<'_>, TryBorrowInnerError> {
            let uuid = unsafe { src_ptr.get::<Uuid>() };
            Ok(PtrConst::new(uuid.as_bytes().as_ptr()))
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
                    .affinity(ScalarAffinity::uuid().build())
                    .build(),
            ))
            .vtable(
                &const {
                    let mut vtable = value_vtable!((), |f, _opts| write!(f, "Uuid"));
                    vtable.parse = Some(|s, target| match Uuid::parse_str(s) {
                        Ok(uuid) => Ok(unsafe { target.put(uuid) }),
                        Err(_) => Err(ParseError::Generic("UUID parsing failed")),
                    });
                    vtable.try_from = Some(try_from);
                    vtable.try_into_inner = Some(try_into_inner);
                    vtable.try_borrow_inner = Some(try_borrow_inner);
                    vtable
                },
            )
            .inner(inner_shape)
            .build()
    };
}
