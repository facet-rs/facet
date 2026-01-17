use alloc::string::String;
use core::time::Duration;

use crate::{
    Def, Facet, OxPtrConst, ProxyDef, PtrConst, PtrMut, PtrUninit, Shape, ShapeBuilder, Type,
    UserType, VTableIndirect,
};

unsafe fn duration_proxy_convert_out(
    target_ptr: PtrConst,
    proxy_ptr: PtrUninit,
) -> Result<PtrMut, String> {
    unsafe {
        let duration = target_ptr.get::<Duration>();
        let secs = duration.as_secs();
        let nanos = duration.subsec_nanos();
        let proxy_mut = proxy_ptr.as_mut_byte_ptr() as *mut (u64, u32);
        proxy_mut.write((secs, nanos));
        Ok(PtrMut::new(proxy_mut as *mut u8))
    }
}

unsafe fn duration_proxy_convert_in(
    proxy_ptr: PtrConst,
    target_ptr: PtrUninit,
) -> Result<PtrMut, String> {
    unsafe {
        let (secs, nanos): (u64, u32) = proxy_ptr.read::<(u64, u32)>();
        let duration = Duration::new(secs, nanos);
        let target_mut = target_ptr.as_mut_byte_ptr() as *mut Duration;
        target_mut.write(duration);
        Ok(PtrMut::new(target_mut as *mut u8))
    }
}

const DURATION_PROXY: ProxyDef = ProxyDef {
    shape: <(u64, u32) as Facet>::SHAPE,
    convert_in: duration_proxy_convert_in,
    convert_out: duration_proxy_convert_out,
};

unsafe fn display_duration(
    source: OxPtrConst,
    f: &mut core::fmt::Formatter<'_>,
) -> Option<core::fmt::Result> {
    unsafe {
        let d = source.get::<Duration>();
        Some(write!(f, "{}s {}ns", d.as_secs(), d.subsec_nanos()))
    }
}

unsafe fn partial_eq_duration(a: OxPtrConst, b: OxPtrConst) -> Option<bool> {
    unsafe { Some(a.get::<Duration>() == b.get::<Duration>()) }
}

const DURATION_VTABLE: VTableIndirect = VTableIndirect {
    display: Some(display_duration),
    partial_eq: Some(partial_eq_duration),
    ..VTableIndirect::EMPTY
};

unsafe impl Facet<'_> for Duration {
    const SHAPE: &'static Shape = &const {
        ShapeBuilder::for_sized::<Duration>("Duration")
            .module_path("core::time")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_indirect(&DURATION_VTABLE)
            .proxy(&DURATION_PROXY)
            .build()
    };
}
