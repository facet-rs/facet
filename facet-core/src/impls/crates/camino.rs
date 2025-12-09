#![cfg(feature = "camino")]

use camino::{Utf8Path, Utf8PathBuf};

use crate::{
    Def, Facet, PtrConst, Shape, ShapeBuilder, Type, UserType, VTableDirect, VTableIndirect,
    vtable_direct, vtable_indirect,
};

/// Try to convert from &str or String to Utf8PathBuf
///
/// # Safety
/// `dst` must be valid for writes, `src` must point to valid data of type described by `src_shape`
unsafe fn utf8pathbuf_try_from(
    dst: *mut Utf8PathBuf,
    src_shape: &'static Shape,
    src: PtrConst,
) -> Result<(), alloc::string::String> {
    // Check if source is &str
    if src_shape.id == <&str as Facet>::SHAPE.id {
        let str_ref: &str = unsafe { src.get::<&str>() };
        unsafe { dst.write(Utf8PathBuf::from(str_ref)) };
        return Ok(());
    }

    // Check if source is String
    if src_shape.id == <alloc::string::String as Facet>::SHAPE.id {
        let string: alloc::string::String = unsafe { src.read::<alloc::string::String>() };
        unsafe { dst.write(Utf8PathBuf::from(string)) };
        return Ok(());
    }

    Err(alloc::format!(
        "cannot convert {} to Utf8PathBuf",
        src_shape.type_identifier
    ))
}

unsafe impl Facet<'_> for Utf8PathBuf {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableDirect = vtable_direct!(Utf8PathBuf =>
            Display,
            Debug,
            Hash,
            PartialEq,
            PartialOrd,
            Ord,
            FromStr,
            [try_from = utf8pathbuf_try_from],
        );

        ShapeBuilder::for_sized::<Utf8PathBuf>("Utf8PathBuf")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_direct(&VTABLE)
            .eq()
            .send()
            .sync()
            .build()
    };
}

unsafe impl Facet<'_> for Utf8Path {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableIndirect = vtable_indirect!(Utf8Path =>
            Display,
            Debug,
            Hash,
            PartialEq,
            PartialOrd,
            Ord,
        );

        ShapeBuilder::for_unsized::<Utf8Path>("Utf8Path")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_indirect(&VTABLE)
            .eq()
            .send()
            .sync()
            .build()
    };
}
