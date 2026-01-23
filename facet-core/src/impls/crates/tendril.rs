#![cfg(feature = "tendril")]

use tendril::{Atomic, StrTendril, Tendril, fmt};

use crate::{
    Def, Facet, PtrConst, Shape, ShapeBuilder, TryFromOutcome, Type, UserType, VTableDirect,
    vtable_direct,
};

/// Atomic variant of StrTendril (Send + Sync)
pub type AtomicStrTendril = Tendril<fmt::UTF8, Atomic>;

/// Try to convert from &str or String to `StrTendril`
///
/// # Safety
/// `dst` must be valid for writes, `src` must point to valid data of type described by `src_shape`
unsafe fn str_tendril_try_from(
    dst: *mut StrTendril,
    src_shape: &'static Shape,
    src: PtrConst,
) -> TryFromOutcome {
    // Check if source is &str (Copy type, use get)
    if src_shape.id == <&str as Facet>::SHAPE.id {
        let str_ref: &str = unsafe { src.get::<&str>() };
        unsafe { dst.write(StrTendril::from(str_ref)) };
        return TryFromOutcome::Converted;
    }

    // Check if source is String (consume via read)
    if src_shape.id == <alloc::string::String as Facet>::SHAPE.id {
        let string: alloc::string::String = unsafe { src.read::<alloc::string::String>() };
        unsafe { dst.write(StrTendril::from(string)) };
        return TryFromOutcome::Converted;
    }

    TryFromOutcome::Unsupported
}

/// Try to convert from &str or String to `AtomicStrTendril`
///
/// # Safety
/// `dst` must be valid for writes, `src` must point to valid data of type described by `src_shape`
unsafe fn atomic_str_tendril_try_from(
    dst: *mut AtomicStrTendril,
    src_shape: &'static Shape,
    src: PtrConst,
) -> TryFromOutcome {
    // Check if source is &str (Copy type, use get)
    if src_shape.id == <&str as Facet>::SHAPE.id {
        let str_ref: &str = unsafe { src.get::<&str>() };
        unsafe { dst.write(AtomicStrTendril::from(str_ref)) };
        return TryFromOutcome::Converted;
    }

    // Check if source is String (consume via read)
    if src_shape.id == <alloc::string::String as Facet>::SHAPE.id {
        let string: alloc::string::String = unsafe { src.read::<alloc::string::String>() };
        unsafe { dst.write(AtomicStrTendril::from(string)) };
        return TryFromOutcome::Converted;
    }

    TryFromOutcome::Unsupported
}

unsafe impl Facet<'_> for StrTendril {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableDirect = vtable_direct!(StrTendril =>
            Display,
            Debug,
            Hash,
            PartialEq,
            PartialOrd,
            Ord,
            FromStr,
            [try_from = str_tendril_try_from],
        );

        ShapeBuilder::for_sized::<StrTendril>("StrTendril")
            .module_path("tendril")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_direct(&VTABLE)
            .eq()
            .build()
    };
}

unsafe impl Facet<'_> for AtomicStrTendril {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableDirect = vtable_direct!(AtomicStrTendril =>
            Display,
            Debug,
            Hash,
            PartialEq,
            PartialOrd,
            Ord,
            FromStr,
            [try_from = atomic_str_tendril_try_from],
        );

        ShapeBuilder::for_sized::<AtomicStrTendril>("AtomicStrTendril")
            .module_path("tendril")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_direct(&VTABLE)
            .eq()
            .send()
            .sync()
            .build()
    };
}
