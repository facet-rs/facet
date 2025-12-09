use crate::{
    Def, Facet, PtrConst, Shape, ShapeBuilder, Type, UserType, VTableDirect, VTableIndirect,
    vtable_direct, vtable_indirect,
};

/// Try to convert from &str or String to PathBuf
///
/// # Safety
/// `dst` must be valid for writes, `src` must point to valid data of type described by `src_shape`
unsafe fn pathbuf_try_from(
    dst: *mut std::path::PathBuf,
    src_shape: &'static Shape,
    src: PtrConst,
) -> Result<(), alloc::string::String> {
    // Check if source is &str
    if src_shape.id == <&str as Facet>::SHAPE.id {
        let str_ref: &str = unsafe { src.get::<&str>() };
        unsafe { dst.write(std::path::PathBuf::from(str_ref)) };
        return Ok(());
    }

    // Check if source is String
    if src_shape.id == <alloc::string::String as Facet>::SHAPE.id {
        let string: alloc::string::String = unsafe { src.read::<alloc::string::String>() };
        unsafe { dst.write(std::path::PathBuf::from(string)) };
        return Ok(());
    }

    Err(alloc::format!(
        "cannot convert {} to PathBuf",
        src_shape.type_identifier
    ))
}

unsafe impl Facet<'_> for std::path::PathBuf {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableDirect = vtable_direct!(std::path::PathBuf =>
            Debug,
            Hash,
            PartialEq,
            PartialOrd,
            Ord,
            [try_from = pathbuf_try_from],
        );

        ShapeBuilder::for_sized::<std::path::PathBuf>("PathBuf")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_direct(&VTABLE)
            .eq()
            .send()
            .sync()
            .build()
    };
}

unsafe impl Facet<'_> for std::path::Path {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableIndirect = vtable_indirect!(std::path::Path =>
            Debug,
            Hash,
            PartialEq,
            PartialOrd,
            Ord,
        );

        ShapeBuilder::for_unsized::<std::path::Path>("Path")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_indirect(&VTABLE)
            .eq()
            .send()
            .sync()
            .build()
    };
}
