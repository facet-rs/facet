use alloc::borrow::ToOwned;
use alloc::string::String;

use camino::{Utf8Path, Utf8PathBuf};

use crate::{
    Def, Facet, PtrConst, PtrMut, PtrUninit, Shape, TryFromError, TryIntoInnerError, Type,
    UserType, value_vtable,
};

unsafe impl Facet<'_> for Utf8PathBuf {
    const SHAPE: &'static Shape = &const {
        Shape {
            id: Shape::id_of::<Self>(),
            layout: Shape::layout_of::<Self>(),
            vtable: {
                // Define the functions for transparent conversion between Utf8PathBuf and String
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
                    Ok(unsafe { dst.put(Utf8PathBuf::from(s)) })
                }

                unsafe fn try_into_inner<'dst>(
                    src_ptr: PtrMut<'_>,
                    dst: PtrUninit<'dst>,
                ) -> Result<PtrMut<'dst>, TryIntoInnerError> {
                    let path = unsafe { src_ptr.read::<Utf8PathBuf>() };
                    Ok(unsafe { dst.put(path.into_string()) })
                }

                let mut vtable = value_vtable!(Utf8PathBuf, |f, _opts| write!(
                    f,
                    "{}",
                    Self::SHAPE.type_identifier
                ));

                {
                    vtable.parse =
                        Some(|s, target| Ok(unsafe { target.put(Utf8Path::new(s).to_owned()) }));
                    vtable.try_from = Some(try_from);
                    vtable.try_into_inner = Some(try_into_inner);
                }
                vtable
            },
            ty: Type::User(UserType::Opaque),
            def: Def::Scalar,
            type_identifier: "Utf8PathBuf",
            type_params: &[],
            doc: &[],
            attributes: &[],
            type_tag: None,
            inner: Some(<String as Facet>::SHAPE),
        }
    };
}

unsafe impl Facet<'_> for Utf8Path {
    const SHAPE: &'static Shape = &const {
        Shape {
            id: Shape::id_of::<Self>(),
            layout: Shape::UNSIZED_LAYOUT,
            vtable: value_vtable!(Utf8Path, |f, _opts| write!(
                f,
                "{}",
                Self::SHAPE.type_identifier
            )),
            ty: Type::User(UserType::Opaque),
            def: Def::Scalar,
            type_identifier: "Utf8Path",
            type_params: &[],
            doc: &[],
            attributes: &[],
            type_tag: None,
            inner: None,
        }
    };
}
