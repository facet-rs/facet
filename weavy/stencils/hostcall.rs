//! Shared Weavy copy-and-patch stencils.
//!
//! Consumers keep ownership of their state ABI and intrinsics. The common stencil
//! shape here is only "load one host-call record, call it, then continue", so
//! format crates can share code layout while supplying JSON/PHON/postcard/etc.
//! operations as host functions.

#![allow(clippy::missing_safety_doc)]

#[repr(C)]
pub struct HostCallInfo {
    pub info: *const (),
    pub call: unsafe extern "C" fn(cx: *mut (), info: *const ()) -> bool,
}

#[repr(C)]
pub struct HostCallCtx {
    pub prog: *const u64,
    pub inner: *mut (),
}

extern "C" {
    fn weavy_cont(cx: *mut HostCallCtx);
}

#[no_mangle]
pub unsafe extern "C" fn weavy_stencil_hostcall(cx: *mut HostCallCtx) {
    let c = &mut *cx;
    let info = *(c.prog as *const *const HostCallInfo);
    c.prog = c.prog.add(1);
    if !((*info).call)(c.inner, (*info).info) {
        return;
    }

    weavy_cont(cx);
}

#[no_mangle]
pub unsafe extern "C" fn weavy_stencil_done(_cx: *mut HostCallCtx) {}
