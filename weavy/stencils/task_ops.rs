//! Task-lane copy-and-patch stencils: typed three-address ops over
//! FRAME offsets (tooth 2 of the substrate — frames as declared
//! records in a per-task arena; see the ruled ABI in the vixen repo,
//! docs/design/tooth-2-frames-abi.md).
//!
//! No operand stack and no tags: every op addresses the current frame
//! at statically known byte offsets carried in the immediate stream —
//! the await-spill rule holds by construction because values are
//! always frame-resident (constitution A6: typed instructions over
//! untagged operands).
//!
//! Calls and returns EXIT to the driver in this slice (the driver owns
//! frame allocation — tested logic — and trampolines into the callee's
//! chain). Direct-threaded cross-chain calls are a later optimization;
//! the ABI does not change when they arrive, because the frame layout
//! and the immediate vocabulary stay identical.
//!
//! build.rs compiles this with `rustc --emit=obj` (guaranteed tail
//! calls via `become` where available) and extracts each op's machine
//! code plus its `weavy_cont` relocation, exactly like the async lane.

#![cfg_attr(tailcall, feature(explicit_tail_calls))]
#![allow(clippy::missing_safety_doc)]
#![allow(incomplete_features)]

/// Threaded state — MUST match `Ctx` in src/jit/task_lane.rs
/// (repr(C), same field order).
#[repr(C)]
pub struct Ctx {
    /// Immediate stream (frame offsets, values, await descriptors).
    prog: *const u64,
    /// Current frame base. Stable for the duration of one chain entry:
    /// the driver performs all allocation between entries.
    frame: *mut u8,
    /// Host readiness array: `ready[i] != 0` ⇒ await #i's value is present.
    ready: *mut i64,
    /// Host value array, indexed by await index.
    awaited: *const i64,
    /// On any driver exit (park/call/ret), the chain offset to re-enter.
    resume: *mut u64,
    /// On park, which await parked the task.
    await_index: *mut u64,
    /// Exit code: 0 = chain fell through (bug — RET is mandatory),
    /// 1 = parked on an await, 2 = call (driver enters callee),
    /// 3 = ret (driver pops the frame), 4 = sync host call (driver
    /// invokes the host over the frame, re-enters at the continuation).
    exit: *mut i64,
    /// Read-only value payload table for native store-backed loads.
    store_value_memories: *const RawValueMemory,
    store_value_memory_count: usize,
    /// Molten payloads lent by an external owner; read-only.
    lent_molten_value_memories: *const RawValueMemory,
    lent_molten_value_memory_count: usize,
    /// The task's private molten arena, reached only through the two ABI
    /// functions below so both lanes share one arena semantics.
    molten: *mut core::ffi::c_void,
    molten_bytes: unsafe extern "C" fn(*const core::ffi::c_void, i64, *mut usize) -> *const u8,
    array_new:
        unsafe extern "C" fn(*mut core::ffi::c_void, i64, usize, i64, *mut i64) -> i64,
    array_store: unsafe extern "C" fn(
        *mut core::ffi::c_void,
        i64,
        i64,
        *const u8,
        usize,
        i64,
    ) -> i64,
    array_load: unsafe extern "C" fn(
        *const RawValueMemory,
        usize,
        *const RawValueMemory,
        usize,
        *mut core::ffi::c_void,
        i64,
        i64,
        *mut u8,
        usize,
        i64,
    ) -> i64,
    array_len: unsafe extern "C" fn(
        *const RawValueMemory,
        usize,
        *const RawValueMemory,
        usize,
        *mut core::ffi::c_void,
        i64,
        i64,
        *mut i64,
    ) -> i64,
    ordered_begin_probe:
        unsafe extern "C" fn(*mut core::ffi::c_void, i64, i64, *mut i64, *mut i64) -> i64,
    ordered_probe_key: unsafe extern "C" fn(
        *mut core::ffi::c_void,
        i64,
        i64,
        i64,
        usize,
        *mut i64,
        *mut i64,
        *mut i64,
        *mut u8,
    ) -> i64,
    ordered_probe_value: unsafe extern "C" fn(
        *mut core::ffi::c_void,
        i64,
        i64,
        i64,
        usize,
        *mut i64,
        *mut u8,
    ) -> i64,
    ordered_begin_insert:
        unsafe extern "C" fn(*mut core::ffi::c_void, i64, i64, *mut i64, *mut i64) -> i64,
    ordered_insert_inspect: unsafe extern "C" fn(
        *mut core::ffi::c_void,
        i64,
        i64,
        i64,
        usize,
        *mut i64,
        *mut u8,
    ) -> i64,
    ordered_insert_advance: unsafe extern "C" fn(
        *mut core::ffi::c_void,
        i64,
        i64,
        i64,
        i64,
        *mut i64,
    ) -> i64,
    ordered_insert_commit: unsafe extern "C" fn(
        *mut core::ffi::c_void,
        i64,
        i64,
        i64,
        *const u8,
        usize,
        *const u8,
        usize,
        i64,
        i64,
        *mut i64,
    ) -> i64,
    ordered_begin_iterate:
        unsafe extern "C" fn(*mut core::ffi::c_void, i64, i64, *mut i64, *mut i64) -> i64,
    ordered_iterate_row: unsafe extern "C" fn(
        *mut core::ffi::c_void,
        i64,
        i64,
        i64,
        usize,
        *mut i64,
        *mut u8,
    ) -> i64,
    ordered_len:
        unsafe extern "C" fn(*mut core::ffi::c_void, i64, i64, *mut i64) -> i64,
    string_concat: unsafe extern "C" fn(
        *const RawValueMemory,
        usize,
        *const RawValueMemory,
        usize,
        *mut core::ffi::c_void,
        i64,
        i64,
        *mut i64,
    ) -> i64,
    byte_project: unsafe extern "C" fn(
        *const RawValueMemory,
        usize,
        *const RawValueMemory,
        usize,
        *mut core::ffi::c_void,
        i64,
        *mut i64,
    ) -> i64,
    /// The task's append-only publication log, reached only through the ABI
    /// function below so both lanes share one log semantics.
    publications: *mut core::ffi::c_void,
    publish: unsafe extern "C" fn(*mut core::ffi::c_void, u64, i64, *const u8, usize) -> i64,
}

/// Raw ABI descriptor; MUST match `crate::task::RawValueMemory`.
#[repr(C)]
#[derive(Clone, Copy)]
struct RawValueMemory {
    ptr: *const u8,
    len: usize,
}

const EXIT_AWAIT_PARKED: i64 = 1;
const EXIT_CALL: i64 = 2;
const EXIT_RET: i64 = 3;
const EXIT_HOST_CALL: i64 = 4;
const EXIT_TRACE_MARK: i64 = 5;
const EXIT_HOST_CALL_YIELD: i64 = 6;
const EXIT_COMPARE_LEFT_UNRESIDENT: i64 = 7;
const EXIT_COMPARE_RIGHT_UNRESIDENT: i64 = 8;
const EXIT_INVALID_ENUM_SELECTOR: i64 = 9;
const EXIT_ENUM_PROJECTION_MISMATCH: i64 = 10;
const EXIT_INVALID_ARRAY_STATUS: i64 = 11;
const EXIT_INVALID_ORDERED_STATUS: i64 = 12;
const EXIT_STRING_CONCAT_LEFT_UNRESIDENT: i64 = 13;
const EXIT_STRING_CONCAT_RIGHT_UNRESIDENT: i64 = 14;
const EXIT_STRING_CONCAT_ALLOCATION: i64 = 15;
const EXIT_PUBLICATION_ALLOCATION: i64 = 16;
const EXIT_BYTE_PROJECT_SOURCE_UNRESIDENT: i64 = 17;
const EXIT_BYTE_PROJECT_ALLOCATION: i64 = 18;

const LENT_MOLTEN_MIN: i64 = i64::MIN / 2;

extern "C" {
    /// The continuation hole (patched to the next stencil, or DONE).
    fn weavy_cont(cx: *mut Ctx);
    /// Conditional continuation patched to the zero/taken target.
    fn weavy_zero(cx: *mut Ctx);
    /// Conditional continuation patched to the nonzero/fallthrough target.
    fn weavy_nonzero(cx: *mut Ctx);
}

macro_rules! cont {
    ($cx:ident) => {{
        #[cfg(tailcall)]
        {
            become weavy_cont($cx);
        }
        #[cfg(not(tailcall))]
        {
            weavy_cont($cx);
        }
    }};
}

macro_rules! branch_to {
    ($target:ident, $cx:ident) => {{
        #[cfg(tailcall)]
        {
            become $target($cx);
        }
        #[cfg(not(tailcall))]
        {
            $target($cx);
        }
    }};
}

#[inline(always)]
unsafe fn read_i64(frame: *mut u8, off: u64) -> i64 {
    (frame.add(off as usize) as *const i64).read_unaligned()
}

#[inline(always)]
unsafe fn write_i64(frame: *mut u8, off: u64, value: i64) {
    (frame.add(off as usize) as *mut i64).write_unaligned(value);
}

#[inline(always)]
unsafe fn copy_bytes(frame: *mut u8, dst: u64, src: u64, len: u64) {
    let mut index = 0u64;
    while index < len {
        let byte = unsafe { frame.add((src + index) as usize).read() };
        unsafe { frame.add((dst + index) as usize).write(byte) };
        index += 1;
    }
}

/// Complete structural copy — immediates: [dst, src, len].
#[no_mangle]
pub unsafe extern "C" fn weavy_task_copy_value(cx: *mut Ctx) {
    let c = unsafe { &mut *cx };
    let p = c.prog;
    unsafe { copy_bytes(c.frame, *p, *p.add(1), *p.add(2)) };
    c.prog = unsafe { p.add(3) };
    unsafe { cont!(cx) }
}

/// Complete product construction — immediates: [count, dst, src, len] * count.
#[no_mangle]
pub unsafe extern "C" fn weavy_task_product_construct(cx: *mut Ctx) {
    let c = unsafe { &mut *cx };
    let p = c.prog;
    let count = unsafe { *p } as usize;
    let mut index = 0usize;
    while index < count {
        let q = unsafe { p.add(1 + index * 3) };
        unsafe { copy_bytes(c.frame, *q, *q.add(1), *q.add(2)) };
        index += 1;
    }
    c.prog = unsafe { p.add(1 + count * 3) };
    unsafe { cont!(cx) }
}

/// Compact enum construction — immediates:
/// [dst, len, selector, variant, count, (dst, src, len) * count].
#[no_mangle]
pub unsafe extern "C" fn weavy_task_enum_construct(cx: *mut Ctx) {
    let c = unsafe { &mut *cx };
    let p = c.prog;
    let dst = unsafe { *p };
    let len = unsafe { *p.add(1) };
    let selector = unsafe { *p.add(2) };
    let variant = unsafe { *p.add(3) } as i64;
    let count = unsafe { *p.add(4) } as usize;
    let mut byte = 0u64;
    while byte < len {
        unsafe { c.frame.add((dst + byte) as usize).write(0) };
        byte += 1;
    }
    unsafe { write_i64(c.frame, dst + selector, variant) };
    let mut index = 0usize;
    while index < count {
        let q = unsafe { p.add(5 + index * 3) };
        unsafe { copy_bytes(c.frame, *q, *q.add(1), *q.add(2)) };
        index += 1;
    }
    c.prog = unsafe { p.add(5 + count * 3) };
    unsafe { cont!(cx) }
}

/// Checked enum variant test — immediates:
/// [dst, value, selector, requested, variant_count, pc].
#[no_mangle]
pub unsafe extern "C" fn weavy_task_enum_is_variant(cx: *mut Ctx) {
    let c = unsafe { &mut *cx };
    let p = c.prog;
    let actual = unsafe { read_i64(c.frame, *p.add(1) + *p.add(2)) };
    let count = unsafe { *p.add(4) };
    if actual < 0 || actual as u64 >= count {
        unsafe {
            *c.await_index = *p.add(5);
            *c.resume = actual as u64;
            *c.exit = EXIT_INVALID_ENUM_SELECTOR;
        }
        return;
    }
    unsafe { write_i64(c.frame, *p, i64::from(actual == *p.add(3) as i64)) };
    c.prog = unsafe { p.add(6) };
    unsafe { cont!(cx) }
}

/// Checked enum projection — immediates:
/// [dst, value, selector, requested, variant_count, field, len, pc].
#[no_mangle]
pub unsafe extern "C" fn weavy_task_enum_project_checked(cx: *mut Ctx) {
    let c = unsafe { &mut *cx };
    let p = c.prog;
    let actual = unsafe { read_i64(c.frame, *p.add(1) + *p.add(2)) };
    let count = unsafe { *p.add(4) };
    if actual < 0 || actual as u64 >= count {
        unsafe {
            *c.await_index = *p.add(7);
            *c.resume = actual as u64;
            *c.exit = EXIT_INVALID_ENUM_SELECTOR;
        }
        return;
    }
    if actual != unsafe { *p.add(3) } as i64 {
        unsafe {
            *c.await_index = *p.add(7);
            *c.resume = actual as u64;
            *c.exit = EXIT_ENUM_PROJECTION_MISMATCH;
        }
        return;
    }
    unsafe { copy_bytes(c.frame, *p, *p.add(1) + *p.add(5), *p.add(6)) };
    c.prog = unsafe { p.add(8) };
    unsafe { cont!(cx) }
}

#[inline(always)]
unsafe fn handle_bytes(c: &Ctx, handle: i64) -> Option<(*const u8, usize)> {
    let memory = if handle < 0 {
        let mut len = 0usize;
        let ptr = (c.molten_bytes)(c.molten as *const core::ffi::c_void, handle, &raw mut len);
        if !ptr.is_null() {
            return Some((ptr, len));
        }
        if !(LENT_MOLTEN_MIN..0).contains(&handle) {
            return None;
        }
        let index = usize::try_from((-1i64).checked_sub(handle)?).ok()?;
        if index >= c.lent_molten_value_memory_count {
            return None;
        }
        *c.lent_molten_value_memories.add(index)
    } else {
        let index = handle as usize;
        if index >= c.store_value_memory_count {
            return None;
        }
        *c.store_value_memories.add(index)
    };
    if memory.ptr.is_null() {
        return None;
    }
    Some((memory.ptr, memory.len))
}

#[inline(always)]
unsafe fn compare_value_bytes(c: &mut Ctx, pc: u64, a: i64, b: i64) -> Option<i64> {
    let a_handle = a;
    let b_handle = b;
    let Some(a) = handle_bytes(c, a_handle) else {
        *c.await_index = pc;
        *c.resume = a_handle as u64;
        *c.exit = EXIT_COMPARE_LEFT_UNRESIDENT;
        return None;
    };
    if a_handle == b_handle {
        return Some(1);
    }
    let Some(b) = handle_bytes(c, b_handle) else {
        *c.await_index = pc;
        *c.resume = b_handle as u64;
        *c.exit = EXIT_COMPARE_RIGHT_UNRESIDENT;
        return None;
    };
    let a = RawValueMemory {
        ptr: a.0,
        len: a.1,
    };
    let b = RawValueMemory {
        ptr: b.0,
        len: b.1,
    };
    let shared = if a.len < b.len { a.len } else { b.len };
    let mut index = 0usize;
    while index < shared {
        let left = a.ptr.add(index).read();
        let right = b.ptr.add(index).read();
        if left < right {
            return Some(0);
        }
        if left > right {
            return Some(2);
        }
        index += 1;
    }
    Some(if a.len < b.len {
        0
    } else if a.len > b.len {
        2
    } else {
        1
    })
}

/// `frame[dst] = value` — immediates: [dst, value].
#[no_mangle]
pub unsafe extern "C" fn weavy_task_const(cx: *mut Ctx) {
    let c = &mut *cx;
    let dst = *c.prog;
    let value = *c.prog.add(1) as i64;
    c.prog = c.prog.add(2);
    write_i64(c.frame, dst, value);
    cont!(cx);
}

/// `frame[dst] = frame[a] + frame[b]` — immediates: [dst, a, b].
#[no_mangle]
pub unsafe extern "C" fn weavy_task_add(cx: *mut Ctx) {
    let c = &mut *cx;
    let dst = *c.prog;
    let a = *c.prog.add(1);
    let b = *c.prog.add(2);
    c.prog = c.prog.add(3);
    write_i64(
        c.frame,
        dst,
        read_i64(c.frame, a).wrapping_add(read_i64(c.frame, b)),
    );
    cont!(cx);
}

/// `frame[dst] = frame[a] * frame[b]` — immediates: [dst, a, b].
#[no_mangle]
pub unsafe extern "C" fn weavy_task_mul(cx: *mut Ctx) {
    let c = &mut *cx;
    let dst = *c.prog;
    let a = *c.prog.add(1);
    let b = *c.prog.add(2);
    c.prog = c.prog.add(3);
    write_i64(
        c.frame,
        dst,
        read_i64(c.frame, a).wrapping_mul(read_i64(c.frame, b)),
    );
    cont!(cx);
}

/// `frame[dst] = frame[a] - frame[b]` (i64, wrapping) — immediates: [dst, a, b].
#[no_mangle]
pub unsafe extern "C" fn weavy_task_sub(cx: *mut Ctx) {
    let c = &mut *cx;
    let dst = *c.prog;
    let a = *c.prog.add(1);
    let b = *c.prog.add(2);
    c.prog = c.prog.add(3);
    write_i64(
        c.frame,
        dst,
        read_i64(c.frame, a).wrapping_sub(read_i64(c.frame, b)),
    );
    cont!(cx);
}

/// Total wrapping division — immediates: [dst, a, b].
#[no_mangle]
pub unsafe extern "C" fn weavy_task_div(cx: *mut Ctx) {
    let c = &mut *cx;
    let dst = *c.prog;
    let a = *c.prog.add(1);
    let b = *c.prog.add(2);
    c.prog = c.prog.add(3);
    let a = read_i64(c.frame, a);
    let b = read_i64(c.frame, b);
    let value = if b == 0 {
        0
    } else if a == i64::MIN && b == -1 {
        i64::MIN
    } else {
        a / b
    };
    write_i64(c.frame, dst, value);
    cont!(cx);
}

/// `frame[dst] = frame[src]` — immediates: [dst, src].
#[no_mangle]
pub unsafe extern "C" fn weavy_task_copy(cx: *mut Ctx) {
    let c = &mut *cx;
    let dst = *c.prog;
    let src = *c.prog.add(1);
    c.prog = c.prog.add(2);
    let v = read_i64(c.frame, src);
    write_i64(c.frame, dst, v);
    cont!(cx);
}

macro_rules! cmp_i64_stencil {
    ($name:ident, $op:tt) => {
        /// i64 comparison — immediates: [dst, a, b]. Writes 0/1.
        #[no_mangle]
        pub unsafe extern "C" fn $name(cx: *mut Ctx) {
            let c = &mut *cx;
            let dst = *c.prog;
            let a = *c.prog.add(1);
            let b = *c.prog.add(2);
            c.prog = c.prog.add(3);
            let result = i64::from(read_i64(c.frame, a) $op read_i64(c.frame, b));
            write_i64(c.frame, dst, result);
            cont!(cx);
        }
    };
}

cmp_i64_stencil!(weavy_task_eq, ==);
cmp_i64_stencil!(weavy_task_ne, !=);
cmp_i64_stencil!(weavy_task_lt, <);
cmp_i64_stencil!(weavy_task_le, <=);
cmp_i64_stencil!(weavy_task_gt, >);
cmp_i64_stencil!(weavy_task_ge, >=);

/// Unconditional branch — immediates: [target_prog_delta]. The continuation
/// hole is patched to the target stencil instead of the lexical successor.
#[no_mangle]
pub unsafe extern "C" fn weavy_task_jump(cx: *mut Ctx) {
    let c = &mut *cx;
    let delta = *c.prog as i64 as isize;
    c.prog = c.prog.offset(delta);
    cont!(cx);
}

/// Branch to `target` when `frame[value] == 0`, otherwise fall through —
/// immediates: [value, taken_prog_delta, fallthrough_prog_delta]. The two
/// continuation holes are patched separately.
#[no_mangle]
pub unsafe extern "C" fn weavy_task_jump_if_zero(cx: *mut Ctx) {
    let c = &mut *cx;
    let value = *c.prog;
    let taken_delta = *c.prog.add(1) as i64 as isize;
    let fallthrough_delta = *c.prog.add(2) as i64 as isize;
    if read_i64(c.frame, value) == 0 {
        c.prog = c.prog.offset(taken_delta);
        branch_to!(weavy_zero, cx);
    } else {
        c.prog = c.prog.offset(fallthrough_delta);
        branch_to!(weavy_nonzero, cx);
    }
}

/// `frame[dst] = frame[base + frame[index]*stride]` — immediates:
/// [dst, base, index, stride]. Bounds are the checker's obligation.
#[no_mangle]
pub unsafe extern "C" fn weavy_task_load_ix(cx: *mut Ctx) {
    let c = &mut *cx;
    let dst = *c.prog;
    let base = *c.prog.add(1);
    let index = *c.prog.add(2);
    let stride = *c.prog.add(3);
    c.prog = c.prog.add(4);
    let ix = read_i64(c.frame, index) as u64;
    let v = read_i64(c.frame, base + ix * stride);
    write_i64(c.frame, dst, v);
    cont!(cx);
}

/// `frame[base + frame[index]*stride] = frame[src]` — immediates:
/// [base, index, stride, src].
#[no_mangle]
pub unsafe extern "C" fn weavy_task_store_ix(cx: *mut Ctx) {
    let c = &mut *cx;
    let base = *c.prog;
    let index = *c.prog.add(1);
    let stride = *c.prog.add(2);
    let src = *c.prog.add(3);
    c.prog = c.prog.add(4);
    let ix = read_i64(c.frame, index) as u64;
    let v = read_i64(c.frame, src);
    write_i64(c.frame, base + ix * stride, v);
    cont!(cx);
}

/// Checked store-backed array word read — immediates:
/// [dst, present, array, index, elem_schema_ref].
#[no_mangle]
pub unsafe extern "C" fn weavy_task_load_array_word(cx: *mut Ctx) {
    let c = &mut *cx;
    let dst = *c.prog;
    let present = *c.prog.add(1);
    let array = *c.prog.add(2);
    let index = *c.prog.add(3);
    let elem_schema_ref = *c.prog.add(4) as i64;
    c.prog = c.prog.add(5);
    let mut value = [0u8; 8];
    let status = (c.array_load)(
        c.store_value_memories,
        c.store_value_memory_count,
        c.lent_molten_value_memories,
        c.lent_molten_value_memory_count,
        c.molten,
        read_i64(c.frame, array),
        read_i64(c.frame, index),
        value.as_mut_ptr(),
        8,
        elem_schema_ref,
    );
    write_i64(c.frame, dst, i64::from_le_bytes(value));
    write_i64(c.frame, present, i64::from(status == 1));
    cont!(cx);
}

/// Reserve a molten array — immediates:
/// [dst, status, count_slot, elem_width, elem_schema_ref].
#[no_mangle]
pub unsafe extern "C" fn weavy_task_array_new(cx: *mut Ctx) {
    let c = &mut *cx;
    let dst = *c.prog;
    let status = *c.prog.add(1);
    let count_slot = *c.prog.add(2);
    let elem_width = *c.prog.add(3) as usize;
    let elem_schema_ref = *c.prog.add(4) as i64;
    c.prog = c.prog.add(5);
    let mut handle = i64::MIN;
    let op_status = (c.array_new)(
        c.molten,
        read_i64(c.frame, count_slot),
        elem_width,
        elem_schema_ref,
        &raw mut handle,
    );
    write_i64(c.frame, dst, handle);
    write_i64(c.frame, status, op_status);
    cont!(cx);
}

/// Begin a probe cursor over an ordered collection — immediates:
/// [cursor, status, collection, schema]. Writes the two-word opaque cursor
/// token (arena index, task generation) at `cursor`/`cursor + 8` and the
/// operation status at `status`.
#[no_mangle]
pub unsafe extern "C" fn weavy_task_ordered_empty(cx: *mut Ctx) {
    let c = &mut *cx;
    let dst = *c.prog;
    c.prog = c.prog.add(2);
    write_i64(c.frame, dst, 0);
    cont!(cx);
}

#[no_mangle]
pub unsafe extern "C" fn weavy_task_ordered_begin_probe(cx: *mut Ctx) {
    let c = &mut *cx;
    let cursor = *c.prog;
    let status = *c.prog.add(1);
    let collection = *c.prog.add(2);
    let schema = *c.prog.add(3) as i64;
    c.prog = c.prog.add(4);
    let mut index = -1i64;
    let mut generation = 0i64;
    let op_status = (c.ordered_begin_probe)(
        c.molten,
        read_i64(c.frame, collection),
        schema,
        &raw mut index,
        &raw mut generation,
    );
    write_i64(c.frame, cursor, index);
    write_i64(c.frame, cursor + 8, generation);
    write_i64(c.frame, status, op_status);
    cont!(cx);
}

/// Consume a Probe cursor and expose one probe step — immediates:
/// [cursor, present, key, left, right, status, key_width, schema]. The ABI
/// clears and fills the `key_width` key bytes in the frame; this stencil
/// writes the present flag, the child collection handles, and the status.
#[no_mangle]
pub unsafe extern "C" fn weavy_task_ordered_probe_key(cx: *mut Ctx) {
    let c = &mut *cx;
    let cursor = *c.prog;
    let present = *c.prog.add(1);
    let key = *c.prog.add(2);
    let left = *c.prog.add(3);
    let right = *c.prog.add(4);
    let status = *c.prog.add(5);
    let key_width = *c.prog.add(6) as usize;
    let schema = *c.prog.add(7) as i64;
    c.prog = c.prog.add(8);
    let index = read_i64(c.frame, cursor);
    let generation = read_i64(c.frame, cursor + 8);
    let mut present_out = 0i64;
    let mut left_out = 0i64;
    let mut right_out = 0i64;
    let op_status = (c.ordered_probe_key)(
        c.molten,
        index,
        generation,
        schema,
        key_width,
        &raw mut present_out,
        &raw mut left_out,
        &raw mut right_out,
        c.frame.add(key as usize),
    );
    write_i64(c.frame, present, present_out);
    write_i64(c.frame, left, left_out);
    write_i64(c.frame, right, right_out);
    write_i64(c.frame, status, op_status);
    cont!(cx);
}

/// Consume a Probe cursor and expose the current node's Map value — immediates:
/// [cursor, present, value, status, value_width, schema]. The ABI clears and
/// fills the `value_width` value bytes in the frame; this stencil writes the
/// present flag and the status.
#[no_mangle]
pub unsafe extern "C" fn weavy_task_ordered_probe_value(cx: *mut Ctx) {
    let c = &mut *cx;
    let cursor = *c.prog;
    let present = *c.prog.add(1);
    let value = *c.prog.add(2);
    let status = *c.prog.add(3);
    let value_width = *c.prog.add(4) as usize;
    let schema = *c.prog.add(5) as i64;
    c.prog = c.prog.add(6);
    let index = read_i64(c.frame, cursor);
    let generation = read_i64(c.frame, cursor + 8);
    let mut present_out = 0i64;
    let op_status = (c.ordered_probe_value)(
        c.molten,
        index,
        generation,
        schema,
        value_width,
        &raw mut present_out,
        c.frame.add(value as usize),
    );
    write_i64(c.frame, present, present_out);
    write_i64(c.frame, status, op_status);
    cont!(cx);
}

#[no_mangle]
pub unsafe extern "C" fn weavy_task_ordered_begin_insert(cx: *mut Ctx) {
    let c = &mut *cx;
    let cursor = *c.prog;
    let status = *c.prog.add(1);
    let collection = *c.prog.add(2);
    let schema = *c.prog.add(3) as i64;
    c.prog = c.prog.add(4);
    let mut index = -1i64;
    let mut generation = 0i64;
    let op_status = (c.ordered_begin_insert)(
        c.molten,
        read_i64(c.frame, collection),
        schema,
        &raw mut index,
        &raw mut generation,
    );
    write_i64(c.frame, cursor, index);
    write_i64(c.frame, cursor + 8, generation);
    write_i64(c.frame, status, op_status);
    cont!(cx);
}

#[no_mangle]
pub unsafe extern "C" fn weavy_task_ordered_insert_inspect(cx: *mut Ctx) {
    let c = &mut *cx;
    let cursor = *c.prog;
    let present = *c.prog.add(1);
    let key = *c.prog.add(2);
    let status = *c.prog.add(3);
    let key_width = *c.prog.add(4) as usize;
    let schema = *c.prog.add(5) as i64;
    c.prog = c.prog.add(6);
    let mut present_out = 0i64;
    let op_status = (c.ordered_insert_inspect)(
        c.molten,
        read_i64(c.frame, cursor),
        read_i64(c.frame, cursor + 8),
        schema,
        key_width,
        &raw mut present_out,
        c.frame.add(key as usize),
    );
    write_i64(c.frame, present, present_out);
    write_i64(c.frame, status, op_status);
    cont!(cx);
}

#[no_mangle]
pub unsafe extern "C" fn weavy_task_ordered_insert_advance(cx: *mut Ctx) {
    let c = &mut *cx;
    let cursor = *c.prog;
    let ordering = *c.prog.add(1);
    let ready = *c.prog.add(2);
    let status = *c.prog.add(3);
    let schema = *c.prog.add(4) as i64;
    c.prog = c.prog.add(5);
    let mut ready_out = 0i64;
    let op_status = (c.ordered_insert_advance)(
        c.molten,
        read_i64(c.frame, cursor),
        read_i64(c.frame, cursor + 8),
        schema,
        read_i64(c.frame, ordering),
        &raw mut ready_out,
    );
    write_i64(c.frame, ready, ready_out);
    write_i64(c.frame, status, op_status);
    cont!(cx);
}

#[no_mangle]
pub unsafe extern "C" fn weavy_task_ordered_insert_commit(cx: *mut Ctx) {
    let c = &mut *cx;
    let dst = *c.prog;
    let cursor = *c.prog.add(1);
    let key = *c.prog.add(2);
    let value = *c.prog.add(3);
    let status = *c.prog.add(4);
    let key_width = *c.prog.add(5) as usize;
    let value_width = *c.prog.add(6) as usize;
    let schema = *c.prog.add(7) as i64;
    let replace = *c.prog.add(8) as i64;
    c.prog = c.prog.add(9);
    let has_value = i64::from(value != u64::MAX);
    let value_ptr = if has_value == 0 {
        core::ptr::null()
    } else {
        c.frame.add(value as usize).cast_const()
    };
    let mut collection = -1i64;
    let op_status = (c.ordered_insert_commit)(
        c.molten,
        read_i64(c.frame, cursor),
        read_i64(c.frame, cursor + 8),
        schema,
        c.frame.add(key as usize).cast_const(),
        key_width,
        value_ptr,
        value_width,
        has_value,
        replace,
        &raw mut collection,
    );
    write_i64(c.frame, dst, collection);
    write_i64(c.frame, status, op_status);
    cont!(cx);
}

#[no_mangle]
pub unsafe extern "C" fn weavy_task_ordered_begin_iterate(cx: *mut Ctx) {
    let c = &mut *cx;
    let cursor = *c.prog;
    let status = *c.prog.add(1);
    let collection = *c.prog.add(2);
    let schema = *c.prog.add(3) as i64;
    c.prog = c.prog.add(4);
    let mut index = -1i64;
    let mut generation = 0i64;
    let op_status = (c.ordered_begin_iterate)(
        c.molten,
        read_i64(c.frame, collection),
        schema,
        &raw mut index,
        &raw mut generation,
    );
    write_i64(c.frame, cursor, index);
    write_i64(c.frame, cursor + 8, generation);
    write_i64(c.frame, status, op_status);
    cont!(cx);
}

#[no_mangle]
pub unsafe extern "C" fn weavy_task_ordered_iterate_row(cx: *mut Ctx) {
    let c = &mut *cx;
    let cursor = *c.prog;
    let present = *c.prog.add(1);
    let row = *c.prog.add(2);
    let status = *c.prog.add(3);
    let row_width = *c.prog.add(4) as usize;
    let schema = *c.prog.add(5) as i64;
    c.prog = c.prog.add(6);
    let mut present_out = 0i64;
    let op_status = (c.ordered_iterate_row)(
        c.molten,
        read_i64(c.frame, cursor),
        read_i64(c.frame, cursor + 8),
        schema,
        row_width,
        &raw mut present_out,
        c.frame.add(row as usize),
    );
    write_i64(c.frame, present, present_out);
    write_i64(c.frame, status, op_status);
    cont!(cx);
}

#[no_mangle]
pub unsafe extern "C" fn weavy_task_ordered_len(cx: *mut Ctx) {
    let c = &mut *cx;
    let dst = *c.prog;
    let status = *c.prog.add(1);
    let collection = *c.prog.add(2);
    let schema = *c.prog.add(3) as i64;
    c.prog = c.prog.add(4);
    let mut len = 0i64;
    let op_status = (c.ordered_len)(
        c.molten,
        read_i64(c.frame, collection),
        schema,
        &raw mut len,
    );
    write_i64(c.frame, dst, len);
    write_i64(c.frame, status, op_status);
    cont!(cx);
}

#[no_mangle]
pub unsafe extern "C" fn weavy_task_ordered_status_is(cx: *mut Ctx) {
    let c = &mut *cx;
    let dst = *c.prog;
    let status = *c.prog.add(1);
    let expected = *c.prog.add(2) as i64;
    let pc = *c.prog.add(3);
    let actual = read_i64(c.frame, status);
    if !(1..=8).contains(&actual) {
        *c.await_index = pc;
        *c.resume = actual as u64;
        *c.exit = EXIT_INVALID_ORDERED_STATUS;
        return;
    }
    c.prog = c.prog.add(4);
    write_i64(c.frame, dst, i64::from(actual == expected));
    cont!(cx);
}

/// Fill one whole element of a molten array — immediates:
/// [status, array, index, src, elem_width, elem_schema_ref].
#[no_mangle]
pub unsafe extern "C" fn weavy_task_array_store_word(cx: *mut Ctx) {
    let c = &mut *cx;
    let status = *c.prog;
    let array = *c.prog.add(1);
    let index = *c.prog.add(2);
    let src = *c.prog.add(3);
    let elem_width = *c.prog.add(4) as usize;
    let elem_schema_ref = *c.prog.add(5) as i64;
    c.prog = c.prog.add(6);
    let array = read_i64(c.frame, array);
    let index = read_i64(c.frame, index);
    let op_status = (c.array_store)(
        c.molten,
        array,
        index,
        c.frame.add(src as usize),
        elem_width,
        elem_schema_ref,
    );
    write_i64(c.frame, status, op_status);
    cont!(cx);
}

/// Checked whole-element array read — immediates:
/// [dst, status, array, index, elem_width, elem_schema_ref].
#[no_mangle]
pub unsafe extern "C" fn weavy_task_load_array(cx: *mut Ctx) {
    let c = &mut *cx;
    let dst = *c.prog;
    let status = *c.prog.add(1);
    let array = *c.prog.add(2);
    let index = *c.prog.add(3);
    let elem_width = *c.prog.add(4) as usize;
    let elem_schema_ref = *c.prog.add(5) as i64;
    c.prog = c.prog.add(6);
    let op_status = (c.array_load)(
        c.store_value_memories,
        c.store_value_memory_count,
        c.lent_molten_value_memories,
        c.lent_molten_value_memory_count,
        c.molten,
        read_i64(c.frame, array),
        read_i64(c.frame, index),
        c.frame.add(dst as usize),
        elem_width,
        elem_schema_ref,
    );
    write_i64(c.frame, status, op_status);
    cont!(cx);
}

/// Array element count — immediates:
/// [dst, status, array, elem_schema_ref].
#[no_mangle]
pub unsafe extern "C" fn weavy_task_load_array_len(cx: *mut Ctx) {
    let c = &mut *cx;
    let dst = *c.prog;
    let status = *c.prog.add(1);
    let array = *c.prog.add(2);
    let elem_schema_ref = *c.prog.add(3) as i64;
    c.prog = c.prog.add(4);
    let mut count = 0i64;
    let op_status = (c.array_len)(
        c.store_value_memories,
        c.store_value_memory_count,
        c.lent_molten_value_memories,
        c.lent_molten_value_memory_count,
        c.molten,
        read_i64(c.frame, array),
        elem_schema_ref,
        &raw mut count,
    );
    write_i64(c.frame, dst, count);
    write_i64(c.frame, status, op_status);
    cont!(cx);
}

/// Validate one checked status and compare it with a closed expected status.
/// Immediates: [dst, status, expected, pc].
#[no_mangle]
pub unsafe extern "C" fn weavy_task_array_status_is(cx: *mut Ctx) {
    let c = &mut *cx;
    let dst = *c.prog;
    let status = *c.prog.add(1);
    let expected = *c.prog.add(2) as i64;
    let pc = *c.prog.add(3);
    let actual = read_i64(c.frame, status);
    if !(1..=9).contains(&actual) {
        *c.await_index = pc;
        *c.resume = actual as u64;
        *c.exit = EXIT_INVALID_ARRAY_STATUS;
        return;
    }
    c.prog = c.prog.add(4);
    write_i64(c.frame, dst, i64::from(actual == expected));
    cont!(cx);
}

/// Lexicographic resident value-byte comparison — immediates: [dst, a, b].
/// Writes the closed three-way ordinal 0=less, 1=equal, 2=greater.
#[no_mangle]
pub unsafe extern "C" fn weavy_task_compare_value_bytes(cx: *mut Ctx) {
    let c = &mut *cx;
    let dst = *c.prog;
    let a = *c.prog.add(1);
    let b = *c.prog.add(2);
    let pc = *c.prog.add(3);
    c.prog = c.prog.add(4);
    if let Some(ordering) = compare_value_bytes(c, pc, read_i64(c.frame, a), read_i64(c.frame, b))
    {
        write_i64(c.frame, dst, ordering);
        cont!(cx);
    }
}

/// Join two resident value-byte runs into a fresh molten string — immediates:
/// [dst, a, b, pc]. Writes the result handle to `frame[dst]` on success; a
/// non-resident operand or an unsatisfiable allocation exits to the driver with
/// the precise fault code and the offending handle in `resume`.
#[no_mangle]
pub unsafe extern "C" fn weavy_task_string_concat(cx: *mut Ctx) {
    let c = &mut *cx;
    let dst = *c.prog;
    let a = *c.prog.add(1);
    let b = *c.prog.add(2);
    let pc = *c.prog.add(3);
    c.prog = c.prog.add(4);
    let a_handle = read_i64(c.frame, a);
    let b_handle = read_i64(c.frame, b);
    let mut handle = i64::MIN;
    let status = (c.string_concat)(
        c.store_value_memories,
        c.store_value_memory_count,
        c.lent_molten_value_memories,
        c.lent_molten_value_memory_count,
        c.molten,
        a_handle,
        b_handle,
        &raw mut handle,
    );
    match status {
        0 => {
            write_i64(c.frame, dst, handle);
            cont!(cx);
        }
        1 => {
            *c.await_index = pc;
            *c.resume = a_handle as u64;
            *c.exit = EXIT_STRING_CONCAT_LEFT_UNRESIDENT;
        }
        2 => {
            *c.await_index = pc;
            *c.resume = b_handle as u64;
            *c.exit = EXIT_STRING_CONCAT_RIGHT_UNRESIDENT;
        }
        _ => {
            *c.await_index = pc;
            *c.exit = EXIT_STRING_CONCAT_ALLOCATION;
        }
    }
}

/// Copy one resident byte run into a fresh molten value — immediates:
/// [dst, source, pc]. The verifier separately witnesses both source and
/// destination opaque-byte schemas, so this stencil only performs by-value
/// byte residency and allocation.
#[no_mangle]
pub unsafe extern "C" fn weavy_task_byte_project(cx: *mut Ctx) {
    let c = &mut *cx;
    let dst = *c.prog;
    let source = *c.prog.add(1);
    let pc = *c.prog.add(2);
    c.prog = c.prog.add(3);
    let source_handle = read_i64(c.frame, source);
    let mut handle = i64::MIN;
    let status = (c.byte_project)(
        c.store_value_memories,
        c.store_value_memory_count,
        c.lent_molten_value_memories,
        c.lent_molten_value_memory_count,
        c.molten,
        source_handle,
        &raw mut handle,
    );
    match status {
        0 => {
            write_i64(c.frame, dst, handle);
            cont!(cx);
        }
        1 => {
            *c.await_index = pc;
            *c.resume = source_handle as u64;
            *c.exit = EXIT_BYTE_PROJECT_SOURCE_UNRESIDENT;
        }
        _ => {
            *c.await_index = pc;
            *c.exit = EXIT_BYTE_PROJECT_ALLOCATION;
        }
    }
}

/// PUBLISH — immediates: [site, record_off, record_width, schema_ref, pc].
/// Copies the `record_width`-byte record at `frame[record_off]` into the task's
/// append-only log under the opaque provenance `site` and the record schema
/// witness. An allocation the log cannot satisfy exits to the driver with the
/// publication-allocation fault; nothing partial is written.
#[no_mangle]
pub unsafe extern "C" fn weavy_task_publish(cx: *mut Ctx) {
    let c = &mut *cx;
    let site = *c.prog;
    let record_off = *c.prog.add(1);
    let record_width = *c.prog.add(2) as usize;
    let schema_ref = *c.prog.add(3) as i64;
    let pc = *c.prog.add(4);
    c.prog = c.prog.add(5);
    let src = c.frame.add(record_off as usize) as *const u8;
    let status = (c.publish)(c.publications, site, schema_ref, src, record_width);
    if status == 0 {
        cont!(cx);
    } else {
        *c.await_index = pc;
        *c.exit = EXIT_PUBLICATION_ALLOCATION;
    }
}

/// AWAIT — immediates: [resume_off, index, dst], NOT consumed on the
/// pending path so a resume re-reads the same descriptor. The ready token is
/// consumed on the successful read path.
#[no_mangle]
pub unsafe extern "C" fn weavy_task_await(cx: *mut Ctx) {
    let c = &mut *cx;
    let resume_off = *c.prog;
    let index = *c.prog.add(1) as usize;
    let dst = *c.prog.add(2);
    let ready = c.ready.add(index);
    if *ready != 0 {
        *ready = 0;
        c.prog = c.prog.add(3);
        write_i64(c.frame, dst, *c.awaited.add(index));
        cont!(cx);
    } else {
        *c.resume = resume_off;
        *c.await_index = index as u64;
        *c.exit = EXIT_AWAIT_PARKED;
    }
}

/// CALL SITE — immediates: [resume_off] (the caller's continuation,
/// where the driver re-enters after the callee returns). The call
/// descriptor (callee, arg copies, return slot) lives in the driver's
/// side table keyed by this same resume offset. Exit code 2.
#[no_mangle]
pub unsafe extern "C" fn weavy_task_call(cx: *mut Ctx) {
    let c = &mut *cx;
    let resume_off = *c.prog;
    c.prog = c.prog.add(1);
    *c.resume = resume_off;
    *c.exit = EXIT_CALL;
}

/// RET SITE — immediates: [src, size]. Exit code 3. The `resume` and
/// `await_index` fields double as exit-payload registers here (src
/// and size respectively) — the driver reads them, copies the return
/// bytes into the caller's designated slot, and pops the frame. A
/// function may have several return sites; each carries its own
/// descriptor.
#[no_mangle]
pub unsafe extern "C" fn weavy_task_ret(cx: *mut Ctx) {
    let c = &mut *cx;
    let src = *c.prog;
    let size = *c.prog.add(1);
    *c.resume = src;
    *c.await_index = size;
    *c.exit = EXIT_RET;
}

/// `frame[dst] = frame[a] + frame[b]` (f64, IEEE) — immediates: [dst, a, b].
#[no_mangle]
pub unsafe extern "C" fn weavy_task_add_f64(cx: *mut Ctx) {
    let c = &mut *cx;
    let dst = *c.prog;
    let a = *c.prog.add(1);
    let b = *c.prog.add(2);
    c.prog = c.prog.add(3);
    let va = f64::from_bits(read_i64(c.frame, a) as u64);
    let vb = f64::from_bits(read_i64(c.frame, b) as u64);
    write_i64(c.frame, dst, (va + vb).to_bits() as i64);
    cont!(cx);
}

/// `frame[dst] = frame[a] * frame[b]` (f64, IEEE) — immediates: [dst, a, b].
#[no_mangle]
pub unsafe extern "C" fn weavy_task_mul_f64(cx: *mut Ctx) {
    let c = &mut *cx;
    let dst = *c.prog;
    let a = *c.prog.add(1);
    let b = *c.prog.add(2);
    c.prog = c.prog.add(3);
    let va = f64::from_bits(read_i64(c.frame, a) as u64);
    let vb = f64::from_bits(read_i64(c.frame, b) as u64);
    write_i64(c.frame, dst, (va * vb).to_bits() as i64);
    cont!(cx);
}

/// SYNC HOST CALL — immediates: [continuation, host_index], consumed
/// before exit (unlike await: a host call always completes, so
/// re-entry happens at the continuation, never here). Exit code 4;
/// `resume` carries the continuation, `await_index` carries the host
/// index. No park path exists on this op by construction — that is
/// the ruled sync/async distinction in machine code.
#[no_mangle]
pub unsafe extern "C" fn weavy_task_hostcall(cx: *mut Ctx) {
    let c = &mut *cx;
    let continuation = *c.prog;
    let host = *c.prog.add(1);
    c.prog = c.prog.add(2);
    *c.resume = continuation;
    *c.await_index = host;
    *c.exit = EXIT_HOST_CALL;
}

/// SYNC HOST CALL YIELD — same immediates as HOST CALL, but exit code
/// 6 tells the driver to return after invoking the host so native
/// provenance tables can be rebuilt before re-entry.
#[no_mangle]
pub unsafe extern "C" fn weavy_task_hostcall_yield(cx: *mut Ctx) {
    let c = &mut *cx;
    let continuation = *c.prog;
    let host = *c.prog.add(1);
    c.prog = c.prog.add(2);
    *c.resume = continuation;
    *c.await_index = host;
    *c.exit = EXIT_HOST_CALL_YIELD;
}

/// TRACE MARK — immediates: [continuation, id], consumed before exit
/// (a mark always completes). Exit code 5; `resume` carries the
/// continuation, `await_index` carries the id. Only Innards-mode
/// compilation emits this stencil at all — Production strips the op
/// from the chain entirely (zero instructions), which is the
/// unified-trace ruling's "weavy strips by mode" made literal.
#[no_mangle]
pub unsafe extern "C" fn weavy_task_trace(cx: *mut Ctx) {
    let c = &mut *cx;
    let continuation = *c.prog;
    let id = *c.prog.add(1);
    c.prog = c.prog.add(2);
    *c.resume = continuation;
    *c.await_index = id;
    *c.exit = EXIT_TRACE_MARK;
}

/// End of chain — reaching this is a lowering bug (RET is mandatory);
/// the driver panics on exit code 0.
#[no_mangle]
pub unsafe extern "C" fn weavy_task_done(_cx: *mut Ctx) {}
