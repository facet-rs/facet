#![cfg(feature = "smartstring")]

use smartstring::{LazyCompact, SmartString};

use crate::{
    Def, Facet, PtrConst, Shape, ShapeBuilder, Type, UserType, VTableDirect, vtable_direct,
};

/// Try to convert from &str or String to SmartString<LazyCompact>
///
/// # Safety
/// `dst` must be valid for writes, `src` must point to valid data of type described by `src_shape`
unsafe fn smartstring_try_from(
    dst: *mut SmartString<LazyCompact>,
    src_shape: &'static Shape,
    src: PtrConst,
) -> Result<(), alloc::string::String> {
    // Check if source is &str
    if src_shape.id == <&str as Facet>::SHAPE.id {
        let str_ref: &str = unsafe { src.get::<&str>() };
        unsafe { dst.write(SmartString::from(str_ref)) };
        return Ok(());
    }

    // Check if source is String
    if src_shape.id == <alloc::string::String as Facet>::SHAPE.id {
        let string: alloc::string::String = unsafe { src.read::<alloc::string::String>() };
        unsafe { dst.write(SmartString::from(string)) };
        return Ok(());
    }

    Err(alloc::format!(
        "cannot convert {} to SmartString",
        src_shape.type_identifier
    ))
}

unsafe impl Facet<'_> for SmartString<LazyCompact> {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableDirect = vtable_direct!(SmartString<LazyCompact> =>
            Display,
            Debug,
            Hash,
            PartialEq,
            PartialOrd,
            Ord,
            FromStr,
            [try_from = smartstring_try_from],
        );

        ShapeBuilder::for_sized::<SmartString<LazyCompact>>("SmartString")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_direct(&VTABLE)
            .eq()
            .send()
            .sync()
            .build()
    };
}
