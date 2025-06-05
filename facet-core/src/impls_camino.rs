use alloc::borrow::ToOwned;
use alloc::string::String;

use camino::{Utf8Path, Utf8PathBuf};

use crate::{
    Def, Facet, PtrConst, PtrMut, PtrUninit, ScalarAffinity, ScalarDef, Shape, TryFromError,
    TryIntoInnerError, Type, UserType, ValueVTable, value_vtable, value_vtable_unsized,
};

unsafe impl Facet<'_> for Utf8PathBuf {
    const VTABLE: &'static ValueVTable = &const {
        // Define the functions for transparent conversion between Utf8PathBuf and String
        unsafe fn try_from<'shape, 'dst>(
            src_ptr: PtrConst<'_>,
            src_shape: &'shape Shape<'shape>,
            dst: PtrUninit<'dst>,
        ) -> Result<PtrMut<'dst>, TryFromError<'shape>> {
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
            let vtable = vtable.sized_mut().unwrap();
            vtable.parse =
                || Some(|s, target| Ok(unsafe { target.put(Utf8Path::new(s).to_owned()) }));
            vtable.try_from = || Some(try_from);
            vtable.try_into_inner = || Some(try_into_inner);
        }
        vtable
    };

    const SHAPE: &'static Shape<'static> = &const {
        // Function to return inner type's shape
        fn inner_shape() -> &'static Shape<'static> {
            <String as Facet>::SHAPE
        }

        Shape::builder_for_sized::<Self>()
            .type_identifier("Utf8PathBuf")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar(
                ScalarDef::builder()
                    .affinity(&const { ScalarAffinity::path().build() })
                    .build(),
            ))
            .inner(inner_shape)
            .build()
    };
}

unsafe impl Facet<'_> for Utf8Path {
    const VTABLE: &'static ValueVTable = &const {
        value_vtable_unsized!(Utf8Path, |f, _opts| write!(
            f,
            "{}",
            Self::SHAPE.type_identifier
        ))
    };

    const SHAPE: &'static Shape<'static> = &const {
        Shape::builder_for_unsized::<Self>()
            .type_identifier("Utf8Path")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar(
                ScalarDef::builder()
                    .affinity(&const { ScalarAffinity::path().build() })
                    .build(),
            ))
            .build()
    };
}
