#![cfg(feature = "bytestring")]

use bytestring::ByteString;

use crate::{
    Def, Facet, PtrConst, Shape, ShapeBuilder, TryFromOutcome, Type, UserType, VTableDirect,
    vtable_direct,
};

/// Try to convert from &str or String to ByteString
///
/// # Safety
/// `dst` must be valid for writes, `src` must point to valid data of type described by `src_shape`
unsafe fn bytestring_try_from(
    dst: *mut ByteString,
    src_shape: &'static Shape,
    src: PtrConst,
) -> TryFromOutcome {
    // Check if source is &str (Copy type, use get)
    if src_shape.id == <&str as Facet>::SHAPE.id {
        let str_ref: &str = unsafe { src.get::<&str>() };
        unsafe { dst.write(ByteString::from(str_ref)) };
        return TryFromOutcome::Converted;
    }

    // Check if source is String (consume via read)
    if src_shape.id == <alloc::string::String as Facet>::SHAPE.id {
        let string: alloc::string::String = unsafe { src.read::<alloc::string::String>() };
        unsafe { dst.write(ByteString::from(string)) };
        return TryFromOutcome::Converted;
    }

    TryFromOutcome::Unsupported
}

unsafe impl Facet<'_> for ByteString {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableDirect = vtable_direct!(ByteString =>
            Display,
            Debug,
            Hash,
            PartialEq,
            PartialOrd,
            Ord,
            [try_from = bytestring_try_from],
        );

        ShapeBuilder::for_sized::<ByteString>("ByteString")
            .module_path("bytestring")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .builder_shape(<alloc::string::String as Facet>::SHAPE)
            .vtable_direct(&VTABLE)
            .eq()
            .send()
            .sync()
            .build()
    };
}
