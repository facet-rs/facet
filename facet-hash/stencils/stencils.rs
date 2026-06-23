//! facet-hash native stencils.
//!
//! These are compiled to an object by `build.rs`, then copied and patched into a
//! native chain at runtime. Per-op immediates ride in `Ctx.prog`; scalar hashing
//! itself stays in monomorphized host calls so the stencil ABI is independent of
//! the concrete `Hasher`.

#![cfg_attr(tailcall, feature(explicit_tail_calls))]
#![allow(clippy::missing_safety_doc)]

#[repr(C)]
pub struct Ctx {
    pub base: *const u8,
    pub hasher: *mut (),
    pub prog: *const u64,
}

pub type HashFn = unsafe extern "C" fn(hasher: *mut (), ptr: *const u8);

#[repr(C)]
pub struct ScalarInfo {
    pub offset: usize,
    pub absolute: *const u8,
    pub hash: HashFn,
}

#[repr(C)]
pub struct ScalarRunInfo {
    pub fields: *const ScalarInfo,
    pub field_count: usize,
}

extern "C" {
    fn facet_hash_cont(cx: *mut Ctx);
}

#[no_mangle]
pub unsafe extern "C" fn facet_hash_stencil_scalar(cx: *mut Ctx) {
    let c = &mut *cx;
    let info = &*(*c.prog as *const ScalarInfo);
    c.prog = c.prog.add(1);

    let ptr = if info.absolute.is_null() {
        c.base.add(info.offset)
    } else {
        info.absolute
    };
    (info.hash)(c.hasher, ptr);

    #[cfg(tailcall)]
    become facet_hash_cont(cx);
    #[cfg(not(tailcall))]
    facet_hash_cont(cx);
}

#[no_mangle]
pub unsafe extern "C" fn facet_hash_stencil_scalar_run(cx: *mut Ctx) {
    let c = &mut *cx;
    let info = &*(*c.prog as *const ScalarRunInfo);
    c.prog = c.prog.add(1);

    let mut index = 0;
    while index < info.field_count {
        let field = &*info.fields.add(index);
        let ptr = if field.absolute.is_null() {
            c.base.add(field.offset)
        } else {
            field.absolute
        };
        (field.hash)(c.hasher, ptr);
        index += 1;
    }

    #[cfg(tailcall)]
    become facet_hash_cont(cx);
    #[cfg(not(tailcall))]
    facet_hash_cont(cx);
}

#[no_mangle]
pub unsafe extern "C" fn facet_hash_stencil_done(_cx: *mut Ctx) {}
