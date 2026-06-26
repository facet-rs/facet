#[repr(C)]
pub struct Ctx {
    pub base: *const u8,
    pub hasher: *mut (),
    pub prog: *const u64,
}

#[repr(C)]
pub struct EqCtx {
    pub left: *const u8,
    pub right: *const u8,
    pub prog: *const u64,
    pub equal: bool,
}

pub type HashFn = unsafe extern "C" fn(hasher: *mut (), ptr: *const u8);
pub type EqFn = unsafe extern "C" fn(left: *const u8, right: *const u8) -> bool;

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

#[repr(C)]
pub struct EqScalarInfo {
    pub offset: usize,
    pub eq: EqFn,
}

#[repr(C)]
pub struct EqScalarRunInfo {
    pub fields: *const EqScalarInfo,
    pub field_count: usize,
}

extern "C" {
    fn facet_hash_cont(cx: *mut Ctx);
    fn facet_hash_eq_cont(cx: *mut EqCtx);
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

    continue_to!(facet_hash_cont, cx);
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

    continue_to!(facet_hash_cont, cx);
}

#[no_mangle]
pub unsafe extern "C" fn facet_hash_stencil_done(_cx: *mut Ctx) {}

#[no_mangle]
pub unsafe extern "C" fn facet_hash_stencil_eq_scalar(cx: *mut EqCtx) {
    let c = &mut *cx;
    if !c.equal {
        return;
    }

    let info = &*(*c.prog as *const EqScalarInfo);
    c.prog = c.prog.add(1);

    c.equal = (info.eq)(c.left.add(info.offset), c.right.add(info.offset));
    if !c.equal {
        return;
    }

    continue_to!(facet_hash_eq_cont, cx);
}

#[no_mangle]
pub unsafe extern "C" fn facet_hash_stencil_eq_scalar_run(cx: *mut EqCtx) {
    let c = &mut *cx;
    if !c.equal {
        return;
    }

    let info = &*(*c.prog as *const EqScalarRunInfo);
    c.prog = c.prog.add(1);

    let mut index = 0;
    while index < info.field_count {
        let field = &*info.fields.add(index);
        if !(field.eq)(c.left.add(field.offset), c.right.add(field.offset)) {
            c.equal = false;
            return;
        }
        index += 1;
    }

    continue_to!(facet_hash_eq_cont, cx);
}

#[no_mangle]
pub unsafe extern "C" fn facet_hash_stencil_eq_done(_cx: *mut EqCtx) {}
