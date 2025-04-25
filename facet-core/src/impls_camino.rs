use alloc::borrow::ToOwned;
use alloc::string::String;
use core::alloc::Layout;

use camino::{Utf8Path, Utf8PathBuf};

use crate::{
    ConstTypeId, Def, Facet, PtrConst, PtrMut, PtrUninit, ScalarAffinity, ScalarDef, Shape,
    TryBorrowInnerError, TryFromInnerError, TryIntoInnerError, value_vtable_inner,
};

unsafe impl Facet<'_> for Utf8PathBuf {
    const SHAPE: &'static Shape = &const {
        // Define the functions for transparent conversion between Utf8PathBuf and String
        unsafe fn try_from_inner<'dst>(
            src_ptr: PtrConst<'_>,
            src_shape: &'static Shape,
            dst: PtrUninit<'dst>,
        ) -> Result<PtrMut<'dst>, TryFromInnerError> {
            if src_shape.id != <String as Facet>::SHAPE.id {
                return Err(TryFromInnerError::UnsupportedSourceShape {
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

        // Function to return inner type's shape
        fn inner_shape() -> &'static Shape {
            <String as Facet>::SHAPE
        }

        Shape::builder()
            .id(ConstTypeId::of::<Self>())
            .layout(Layout::new::<Self>())
            .def(Def::Scalar(
                ScalarDef::builder()
                    .affinity(ScalarAffinity::path().build())
                    .build(),
            ))
            .vtable(
                &const {
                    let mut vtable = value_vtable_inner!((), |f, _opts| write!(f, "Utf8PathBuf"));
                    vtable.parse =
                        Some(|s, target| Ok(unsafe { target.put(Utf8Path::new(s).to_owned()) }));
                    vtable.try_from_inner = Some(try_from_inner);
                    vtable.try_into_inner = Some(try_into_inner);
                    vtable.try_borrow_inner = Some(try_borrow_inner);
                    vtable
                },
            )
            .inner(inner_shape)
            .build()
    };
}

unsafe impl<'a> Facet<'a> for &'a Utf8Path {
    const SHAPE: &'static Shape = &const {
        // Implement try_from_inner to allow conversion from &str to &Utf8Path
        unsafe fn try_from_inner<'src, 'dst>(
            src_ptr: PtrConst<'src>,
            src_shape: &'static Shape,
            dst: PtrUninit<'dst>,
        ) -> Result<PtrMut<'dst>, TryFromInnerError> {
            if src_shape.id != <&'src str as Facet>::SHAPE.id {
                return Err(TryFromInnerError::UnsupportedSourceShape {
                    src_shape,
                    expected: &[<&'src str as Facet>::SHAPE],
                });
            }
            let s: &str = unsafe { src_ptr.read::<&str>() };
            let path = Utf8Path::new(s);
            Ok(unsafe { dst.put(path) })
        }

        Shape::builder()
            .id(ConstTypeId::of::<Self>())
            .layout(Layout::new::<Self>())
            .def(Def::Scalar(
                ScalarDef::builder()
                    .affinity(ScalarAffinity::path().build())
                    .build(),
            ))
            .vtable(
                &const {
                    let mut vtable = value_vtable_inner!((), |f, _opts| write!(f, "Utf8Path"));
                    vtable.try_from_inner = Some(try_from_inner);
                    vtable
                },
            )
            .build()
    };
}
