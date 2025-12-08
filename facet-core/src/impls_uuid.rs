use alloc::string::String;
use alloc::string::ToString;

use uuid::Uuid;

use crate::{
    Def, Facet, ParseError, PtrConst, PtrMut, PtrUninit, Shape, TryFromError, TryIntoInnerError,
    Type, UserType, Variance, value_vtable,
};

unsafe impl Facet<'_> for Uuid {
    const SHAPE: &'static Shape = &const {
        Shape {
            id: Shape::id_of::<Self>(),
            layout: Shape::layout_of::<Self>(),
            vtable: {
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

                let mut vtable = value_vtable!(Uuid, |f, _opts| write!(
                    f,
                    "{}",
                    Self::SHAPE.type_identifier
                ));
                {
                    vtable.parse = {
                        Some(|s, target| match Uuid::parse_str(s) {
                            Ok(uuid) => Ok(unsafe { target.put(uuid) }),
                            Err(_) => Err(ParseError::Generic("UUID parsing failed")),
                        })
                    };
                    vtable.try_from = Some(try_from);
                    vtable.try_into_inner = Some(try_into_inner);
                }
                vtable
            },
            ty: Type::User(UserType::Opaque),
            def: Def::Scalar,
            type_identifier: "Uuid",
            type_params: &[],
            doc: &[],
            attributes: &[],
            type_tag: None,
            inner: Some(<String as Facet>::SHAPE),
            proxy: None,
            variance: Variance::Invariant,
        }
    };
}
