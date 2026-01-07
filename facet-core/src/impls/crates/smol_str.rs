#![cfg(feature = "smol_str")]

use smol_str::SmolStr;

use crate::{
    Def, Facet, PtrConst, Shape, ShapeBuilder, TryFromOutcome, Type, UserType, VTableDirect,
    vtable_direct,
};

/// Try to convert from &str or String to SmolStr
///
/// # Safety
/// `dst` must be valid for writes, `src` must point to valid data of type described by `src_shape`
unsafe fn smol_str_try_from(
    dst: *mut SmolStr,
    src_shape: &'static Shape,
    src: PtrConst,
) -> TryFromOutcome {
    // Check if source is &str (Copy type, use get)
    if src_shape.id == <&str as Facet>::SHAPE.id {
        let str_ref: &str = unsafe { *src.get::<&str>() };
        unsafe { dst.write(SmolStr::from(str_ref)) };
        return TryFromOutcome::Converted;
    }

    // Check if source is String (consume via read)
    if src_shape.id == <alloc::string::String as Facet>::SHAPE.id {
        let string: alloc::string::String = unsafe { src.read::<alloc::string::String>() };
        unsafe { dst.write(SmolStr::from(string)) };
        return TryFromOutcome::Converted;
    }

    TryFromOutcome::Unsupported
}

unsafe impl Facet<'_> for SmolStr {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableDirect = vtable_direct!(SmolStr =>
            Display,
            Debug,
            Hash,
            PartialEq,
            PartialOrd,
            Ord,
            FromStr,
            [try_from = smol_str_try_from],
        );

        ShapeBuilder::for_sized::<SmolStr>("SmolStr")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_direct(&VTABLE)
            .eq()
            .send()
            .sync()
            .build()
    };
}
