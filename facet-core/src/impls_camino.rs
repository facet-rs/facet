use alloc::borrow::ToOwned;
use alloc::string::String;

use camino::{Utf8Path, Utf8PathBuf};

use crate::{
    Def, Facet, PtrConst, PtrMut, PtrUninit, ScalarAffinity, ScalarDef, Shape, TryBorrowInnerError,
    TryFromError, TryIntoInnerError, Type, UserType, ValueVTable, value_vtable,
};

unsafe impl Facet<'_> for Utf8PathBuf {
    const VTABLE: &'static ValueVTable = &const {
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
            let s = unsafe { src_ptr.get::<String>() };
            Ok(unsafe { dst.put(Utf8PathBuf::from(s.clone())) })
        }

        unsafe fn try_into_inner<'dst>(
            src_ptr: PtrConst<'_>,
            dst: PtrUninit<'dst>,
        ) -> Result<PtrMut<'dst>, TryIntoInnerError> {
            let path = unsafe { src_ptr.get::<Utf8PathBuf>() };
            Ok(unsafe { dst.put(path.as_str().to_owned()) })
        }

        unsafe fn try_borrow_inner(
            src_ptr: PtrConst<'_>,
        ) -> Result<PtrConst<'_>, TryBorrowInnerError> {
            let path = unsafe { src_ptr.get::<Utf8PathBuf>() };
            Ok(PtrConst::new(path.as_str().as_ptr()))
        }

        let mut vtable = value_vtable!(Utf8PathBuf, |f, _opts| write!(f, "Utf8PathBuf"));
        vtable.parse = Some(|s, target| Ok(unsafe { target.put(Utf8Path::new(s).to_owned()) }));
        vtable.try_from = Some(try_from);
        vtable.try_into_inner = Some(try_into_inner);
        vtable.try_borrow_inner = Some(try_borrow_inner);
        vtable
    };

    const SHAPE: &'static Shape = &const {
        // Function to return inner type's shape
        fn inner_shape() -> &'static Shape {
            <String as Facet>::SHAPE
        }

        Shape::builder_for_sized::<Self>()
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar(
                ScalarDef::builder()
                    .affinity(ScalarAffinity::path().build())
                    .build(),
            ))
            .inner(inner_shape)
            .build()
    };
}

unsafe impl Facet<'_> for Utf8Path {
    const VTABLE: &'static ValueVTable = &const {
        // Allows conversion from &str to &Utf8Path
        unsafe fn try_from<'src, 'dst>(
            src_ptr: PtrConst<'src>,
            src_shape: &'static Shape,
            dst: PtrUninit<'dst>,
        ) -> Result<PtrMut<'dst>, TryFromError> {
            if src_shape.id != <&'src str as Facet>::SHAPE.id {
                return Err(TryFromError::UnsupportedSourceShape {
                    src_shape,
                    expected: &[<&'src str as Facet>::SHAPE],
                });
            }
            let s: &str = unsafe { src_ptr.read::<&str>() };
            let path = Utf8Path::new(s);
            Ok(unsafe { dst.put(path) })
        }

        let mut vtable = value_vtable!(&Utf8Path, |f, _opts| write!(f, "Utf8Path"));
        vtable.try_from = Some(try_from);
        vtable
    };

    const SHAPE: &'static Shape = &const {
        Shape::builder_for_unsized::<Self>()
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar(
                ScalarDef::builder()
                    .affinity(ScalarAffinity::path().build())
                    .build(),
            ))
            .build()
    };
}
