use alloc::string::String;
use alloc::string::ToString;

use uuid::Uuid;

use crate::{
    Def, Facet, ParseError, PtrConst, PtrMut, PtrUninit, ScalarAffinity, ScalarDef, Shape,
    TryFromError, TryIntoInnerError, Type, UserType, ValueVTable, value_vtable,
};

unsafe impl Facet<'_> for Uuid {
    const VTABLE: &'static ValueVTable = &const {
        // Functions to transparently convert between Uuid and String
        unsafe fn try_from<'shape, 'dst>(
            src_ptr: PtrConst<'_>,
            src_shape: &'shape Shape,
            dst: PtrUninit<'dst>,
        ) -> Result<PtrMut<'dst>, TryFromError<'shape>> {
            if src_shape.id != <String as Facet>::SHAPE.id {
                return Err(TryFromError::UnsupportedSourceShape {
                    src_shape,
                    expected: &[<String as Facet>::SHAPE],
                });
            }
            let s = unsafe { src_ptr.read::<String>() };
            match Uuid::parse_str(&s) {
                Ok(uuid) => Ok(unsafe { dst.put(uuid) }),
                Err(_) => Err(TryFromError::UnsupportedSourceShape {
                    src_shape,
                    expected: &[<String as Facet>::SHAPE],
                }),
            }
        }

        unsafe fn try_into_inner<'dst>(
            src_ptr: PtrMut<'_>,
            dst: PtrUninit<'dst>,
        ) -> Result<PtrMut<'dst>, TryIntoInnerError> {
            let uuid = unsafe { src_ptr.read::<Uuid>() };
            Ok(unsafe { dst.put(uuid.to_string()) })
        }

        let mut vtable = value_vtable!(Uuid, |f, _opts| write!(f, "Uuid"));
        vtable.parse = Some(|s, target| match Uuid::parse_str(s) {
            Ok(uuid) => Ok(unsafe { target.put(uuid) }),
            Err(_) => Err(ParseError::Generic("UUID parsing failed")),
        });
        vtable.try_from = Some(try_from);
        vtable.try_into_inner = Some(try_into_inner);
        vtable
    };

    const SHAPE: &'static Shape<'static> = &const {
        // Return the Shape of the inner type (String)
        fn inner_shape() -> &'static Shape<'static> {
            <String as Facet>::SHAPE
        }

        Shape::builder_for_sized::<Self>()
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar(
                ScalarDef::builder()
                    .affinity(&const { ScalarAffinity::uuid().build() })
                    .build(),
            ))
            .inner(inner_shape)
            .build()
    };
}
