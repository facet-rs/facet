//! JIT runtime helpers that require both `vox-jit-abi` and `vox-postcard`.
//!
//! These are registered in the JITBuilder and called via `call_indirect` from
//! generated encoders/decoders.

use facet::{PtrConst, PtrMut, PtrUninit};
use vox_jit_abi::{
    DecodeCtx, DecodeStatus, EncodeCtx, vox_jit_buf_push_bytes, vox_jit_buf_write_varint,
};
use vox_postcard::{TranslationPlan, ir::slow_path_decode_raw};

pub unsafe extern "C" fn vox_jit_result_is_ok_raw(
    src_ptr: *const u8,
    is_ok_fn: facet_core::ResultIsOkFn,
) -> bool {
    unsafe { is_ok_fn(PtrConst::new(src_ptr)) }
}

pub unsafe extern "C" fn vox_jit_result_get_payload_raw(
    src_ptr: *const u8,
    get_fn: facet_core::ResultGetOkFn,
) -> *const u8 {
    unsafe { get_fn(PtrConst::new(src_ptr)) }
}

pub unsafe extern "C" fn vox_jit_result_init_raw(
    dst_ptr: *mut u8,
    payload_ptr: *mut u8,
    init_fn: facet_core::ResultInitOkFn,
) {
    unsafe {
        init_fn(PtrUninit::new(dst_ptr), PtrMut::new(payload_ptr));
    }
}

/// SlowPath helper: decode one field via the reflective interpreter and update
/// `ctx.consumed`. Called by generated decoders when a `SlowPath` IR op is hit.
///
/// # Safety
/// - `ctx` must be a valid, non-null `DecodeCtx`.
/// - `shape` must be a valid `&'static Shape`.
/// - `plan` must be a valid `*const TranslationPlan`.
/// - `dst_base.add(dst_offset)` must be writable for `shape.layout.size()` bytes.
pub unsafe extern "C" fn vox_jit_slow_path(
    ctx: *mut DecodeCtx,
    shape: &'static facet_core::Shape,
    plan: *const TranslationPlan,
    dst_base: *mut u8,
    dst_offset: usize,
) -> DecodeStatus {
    if crate::abort_on_slow_path() {
        eprintln!(
            "VOX_JIT_ABORT_ON_SLOW_PATH=1: decode slow path reached for '{}'",
            shape
        );
        std::process::abort();
    }

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let ctx_ref = unsafe { &mut *ctx };
        slow_path_decode_raw(
            ctx_ref.input_ptr,
            ctx_ref.input_len,
            ctx_ref.consumed,
            shape,
            plan,
            dst_base,
            dst_offset,
        )
        .map(|new_consumed| {
            ctx_ref.consumed = new_consumed;
        })
    }));

    match result {
        Ok(Some(())) => DecodeStatus::Ok,
        Ok(None) => DecodeStatus::UnexpectedEof,
        Err(_) => DecodeStatus::UnexpectedEof,
    }
}

/// Default-fill helper: invoke a shape's `call_default_in_place` vtable on
/// `dst_base.add(dst_offset)`. Used for local struct fields that have no
/// corresponding remote field on the wire (schema evolution: fill-defaults).
///
/// # Safety
/// - `shape` must be a valid `&'static Shape`.
/// - `dst_base.add(dst_offset)` must point to writable, properly-aligned,
///   uninitialized memory of at least `shape.layout.size()` bytes.
pub unsafe extern "C" fn vox_jit_write_default(
    shape: &'static facet_core::Shape,
    dst_base: *mut u8,
    dst_offset: usize,
) -> DecodeStatus {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
        shape.call_default_in_place(PtrUninit::new(dst_base.add(dst_offset)))
    }));
    match result {
        Ok(Some(())) => DecodeStatus::Ok,
        // No default available — this field should have been marked required
        // during plan-build, so reaching this branch means the plan was built
        // incorrectly. Surface as a generic failure.
        Ok(None) => DecodeStatus::AllocFailed,
        Err(_) => DecodeStatus::AllocFailed,
    }
}

/// Opaque decode helper: read a u32le-length-prefixed byte payload and
/// initialize the destination via the shape's opaque adapter.
pub unsafe extern "C" fn vox_jit_decode_opaque(
    ctx: *mut DecodeCtx,
    shape: &'static facet_core::Shape,
    dst_base: *mut u8,
    dst_offset: usize,
) -> DecodeStatus {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let adapter = shape.opaque_adapter?;
        let ctx_ref = unsafe { &mut *ctx };
        let remaining = unsafe { ctx_ref.remaining() };
        if remaining.len() < 4 {
            return None;
        }

        let len =
            u32::from_le_bytes([remaining[0], remaining[1], remaining[2], remaining[3]]) as usize;
        let total = 4usize.checked_add(len)?;
        if remaining.len() < total {
            return None;
        }

        let bytes = &remaining[4..total];
        let input = facet::OpaqueDeserialize::Borrowed(bytes);
        unsafe {
            (adapter.deserialize)(input, facet_core::PtrUninit::new(dst_base.add(dst_offset)))
        }
        .ok()?;
        ctx_ref.consumed = ctx_ref.consumed.checked_add(total)?;
        Some(())
    }));

    match result {
        Ok(Some(())) => DecodeStatus::Ok,
        Ok(None) => DecodeStatus::UnexpectedEof,
        Err(_) => DecodeStatus::UnexpectedEof,
    }
}

/// SlowPath helper: encode one field reflectively and append its bytes to the
/// current encode buffer.
///
/// # Safety
/// - `ctx` must be a valid, non-null `EncodeCtx`.
/// - `src_ptr` must point to a valid value matching `shape`.
/// - `shape` must be a valid `&'static Shape`.
pub unsafe extern "C" fn vox_jit_encode_slow_path(
    ctx: *mut EncodeCtx,
    src_ptr: *const u8,
    shape: &'static facet_core::Shape,
) -> bool {
    if crate::abort_on_slow_path() {
        eprintln!(
            "VOX_JIT_ABORT_ON_SLOW_PATH=1: encode slow path reached for '{}'",
            shape
        );
        std::process::abort();
    }

    let bytes = if let Some(adapter) = shape.opaque_adapter {
        let mapped = unsafe { (adapter.serialize)(PtrConst::new(src_ptr)) };
        if let Some(bytes) =
            unsafe { vox_postcard::raw::try_decode_passthrough_bytes(mapped.ptr, mapped.shape) }
        {
            let mut out = Vec::with_capacity(4 + bytes.len());
            out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
            out.extend_from_slice(bytes);
            out
        } else if let Some(result) =
            crate::global_runtime().try_encode_ptr(mapped.ptr, mapped.shape)
        {
            let Ok(inner) = result else { return false };
            let mut out = Vec::with_capacity(4 + inner.len());
            out.extend_from_slice(&(inner.len() as u32).to_le_bytes());
            out.extend_from_slice(&inner);
            out
        } else {
            match vox_postcard::serialize::to_vec_dynamic(PtrConst::new(src_ptr), shape) {
                Ok(v) => v,
                Err(_) => return false,
            }
        }
    } else {
        match vox_postcard::serialize::to_vec_dynamic(PtrConst::new(src_ptr), shape) {
            Ok(v) => v,
            Err(_) => return false,
        }
    };
    unsafe { vox_jit_buf_push_bytes(ctx, bytes.as_ptr(), bytes.len()) }
}

fn handle_pure_jit_encode_miss(kind: &str, shape: &'static facet_core::Shape) -> Option<()> {
    if crate::abort_on_slow_path() {
        eprintln!(
            "VOX_JIT_ABORT_ON_SLOW_PATH=1: {kind} encode fell back for '{}'",
            shape
        );
        std::process::abort();
    }
    if crate::require_pure_jit() {
        panic!(
            "VOX_JIT_REQUIRE_PURE=1 and {kind} encode for '{}' could not stay on JIT",
            shape
        );
    }
    None
}

/// Encode an opaque-adapter field without using the reflective walker when the
/// mapped inner value can be encoded by the JIT runtime.
pub unsafe extern "C" fn vox_jit_encode_opaque(
    ctx: *mut EncodeCtx,
    src_ptr: *const u8,
    shape: &'static facet_core::Shape,
) -> bool {
    let Some(adapter) = shape.opaque_adapter else {
        return false;
    };
    let mapped = unsafe { (adapter.serialize)(PtrConst::new(src_ptr)) };

    if let Some(bytes) =
        unsafe { vox_postcard::raw::try_decode_passthrough_bytes(mapped.ptr, mapped.shape) }
    {
        if !unsafe { vox_jit_buf_push_bytes(ctx, (bytes.len() as u32).to_le_bytes().as_ptr(), 4) } {
            return false;
        }
        return unsafe { vox_jit_buf_push_bytes(ctx, bytes.as_ptr(), bytes.len()) };
    }

    if let Some(result) = crate::global_runtime().try_encode_ptr(mapped.ptr, mapped.shape) {
        let Ok(inner) = result else { return false };
        if !unsafe { vox_jit_buf_push_bytes(ctx, (inner.len() as u32).to_le_bytes().as_ptr(), 4) } {
            return false;
        }
        return unsafe { vox_jit_buf_push_bytes(ctx, inner.as_ptr(), inner.len()) };
    }

    if handle_pure_jit_encode_miss("opaque", shape).is_none() {
        return false;
    }

    let Ok(bytes) = vox_postcard::serialize::to_vec_dynamic(PtrConst::new(src_ptr), shape) else {
        return false;
    };
    unsafe { vox_jit_buf_push_bytes(ctx, bytes.as_ptr(), bytes.len()) }
}

/// Encode a proxy field by converting to the proxy value and delegating the
/// proxy shape back through the JIT runtime.
pub unsafe extern "C" fn vox_jit_encode_proxy(
    ctx: *mut EncodeCtx,
    src_ptr: *const u8,
    shape: &'static facet_core::Shape,
) -> bool {
    let Some(proxy_def) = shape.proxy else {
        return false;
    };
    let proxy_shape = proxy_def.shape;
    let Ok(proxy_layout) = proxy_shape.layout.sized_layout() else {
        return false;
    };
    let proxy_uninit = facet_core::alloc_for_layout(proxy_layout);
    let Ok(proxy_ptr) = (unsafe { (proxy_def.convert_out)(PtrConst::new(src_ptr), proxy_uninit) })
    else {
        return false;
    };

    let ok = if let Some(result) =
        crate::global_runtime().try_encode_ptr(proxy_ptr.as_const(), proxy_shape)
    {
        match result {
            Ok(bytes) => unsafe { vox_jit_buf_push_bytes(ctx, bytes.as_ptr(), bytes.len()) },
            Err(_) => false,
        }
    } else if handle_pure_jit_encode_miss("proxy", shape).is_none() {
        false
    } else {
        match vox_postcard::serialize::to_vec_dynamic(proxy_ptr.as_const(), proxy_shape) {
            Ok(bytes) => unsafe { vox_jit_buf_push_bytes(ctx, bytes.as_ptr(), bytes.len()) },
            Err(_) => false,
        }
    };

    unsafe {
        let _ = proxy_shape.call_drop_in_place(proxy_ptr);
        facet_core::dealloc_for_layout(proxy_ptr, proxy_layout);
    }

    ok
}

/// Encode a `Result<T, E>` by selecting the active arm via the result vtable,
/// writing postcard discriminant `0`/`1`, then delegating the inner value back
/// through the JIT runtime.
pub unsafe extern "C" fn vox_jit_encode_result(
    ctx: *mut EncodeCtx,
    src_ptr: *const u8,
    shape: &'static facet_core::Shape,
) -> bool {
    let facet_core::Def::Result(result_def) = shape.def else {
        return false;
    };
    let result_ptr = PtrConst::new(src_ptr);

    if unsafe { (result_def.vtable.is_ok)(result_ptr) } {
        if !unsafe { vox_jit_buf_write_varint(ctx, 0) } {
            return false;
        }
        let ok_ptr = unsafe { (result_def.vtable.get_ok)(result_ptr) };
        if ok_ptr.is_null() {
            return false;
        }
        if let Some(result) =
            crate::global_runtime().try_encode_ptr(PtrConst::new(ok_ptr), result_def.t)
        {
            let Ok(inner) = result else { return false };
            return unsafe { vox_jit_buf_push_bytes(ctx, inner.as_ptr(), inner.len()) };
        }
        let _ = handle_pure_jit_encode_miss("result Ok", result_def.t);
        false
    } else {
        if !unsafe { vox_jit_buf_write_varint(ctx, 1) } {
            return false;
        }
        let err_ptr = unsafe { (result_def.vtable.get_err)(result_ptr) };
        if err_ptr.is_null() {
            return false;
        }
        if let Some(result) =
            crate::global_runtime().try_encode_ptr(PtrConst::new(err_ptr), result_def.e)
        {
            let Ok(inner) = result else { return false };
            return unsafe { vox_jit_buf_push_bytes(ctx, inner.as_ptr(), inner.len()) };
        }
        let _ = handle_pure_jit_encode_miss("result Err", result_def.e);
        false
    }
}

/// Initialize a `Cow<[u8]>` with owned bytes.
///
/// # Safety
/// - `dst` must point to writable storage for `Cow<[u8]>`.
/// - `data` must be valid for `len` bytes.
pub unsafe extern "C" fn vox_jit_init_cow_byte_slice_owned(
    dst: *mut u8,
    data: *const u8,
    len: usize,
) {
    let bytes = unsafe { std::slice::from_raw_parts(data, len) };
    let value = std::borrow::Cow::<'static, [u8]>::Owned(bytes.to_vec());
    unsafe {
        std::ptr::write(dst as *mut std::borrow::Cow<'static, [u8]>, value);
    }
}

/// Initialize a `Cow<[u8]>` borrowing from the input buffer.
///
/// # Safety
/// - `dst` must point to writable storage for `Cow<[u8]>`.
/// - `data` must be valid for `len` bytes and outlive the surrounding decode result.
pub unsafe extern "C" fn vox_jit_init_cow_byte_slice_borrowed(
    dst: *mut u8,
    data: *const u8,
    len: usize,
) {
    let bytes: &'static [u8] =
        unsafe { std::mem::transmute(std::slice::from_raw_parts(data, len)) };
    let value = std::borrow::Cow::<'static, [u8]>::Borrowed(bytes);
    unsafe {
        std::ptr::write(dst as *mut std::borrow::Cow<'static, [u8]>, value);
    }
}

/// Initialize a `&[u8]` borrowing from the input buffer.
///
/// # Safety
/// - `dst` must point to writable storage for `&[u8]`.
/// - `data` must be valid for `len` bytes and outlive the surrounding decode result.
pub unsafe extern "C" fn vox_jit_init_byte_slice_ref(dst: *mut u8, data: *const u8, len: usize) {
    let bytes: &'static [u8] =
        unsafe { std::mem::transmute(std::slice::from_raw_parts(data, len)) };
    unsafe {
        std::ptr::write(dst as *mut &'static [u8], bytes);
    }
}

/// Initialize a `Cow<str>` with owned bytes after UTF-8 validation.
pub unsafe extern "C" fn vox_jit_init_cow_str_owned(dst: *mut u8, data: *const u8, len: usize) {
    let bytes = unsafe { std::slice::from_raw_parts(data, len) };
    let s = std::str::from_utf8(bytes).expect("JIT emitted invalid UTF-8 for Cow<str>");
    let value = std::borrow::Cow::<'static, str>::Owned(s.to_owned());
    unsafe {
        std::ptr::write(dst as *mut std::borrow::Cow<'static, str>, value);
    }
}

/// Initialize a `Cow<str>` borrowing from the input buffer.
pub unsafe extern "C" fn vox_jit_init_cow_str_borrowed(dst: *mut u8, data: *const u8, len: usize) {
    let bytes = unsafe { std::slice::from_raw_parts(data, len) };
    let s = std::str::from_utf8(bytes).expect("JIT emitted invalid UTF-8 for Cow<str>");
    let s: &'static str = unsafe { std::mem::transmute(s) };
    let value = std::borrow::Cow::<'static, str>::Borrowed(s);
    unsafe {
        std::ptr::write(dst as *mut std::borrow::Cow<'static, str>, value);
    }
}

/// Initialize a `&str` borrowing from the input buffer.
pub unsafe extern "C" fn vox_jit_init_str_ref(dst: *mut u8, data: *const u8, len: usize) {
    let bytes = unsafe { std::slice::from_raw_parts(data, len) };
    let s = std::str::from_utf8(bytes).expect("JIT emitted invalid UTF-8 for &str");
    let s: &'static str = unsafe { std::mem::transmute(s) };
    unsafe {
        std::ptr::write(dst as *mut &'static str, s);
    }
}

/// Encode a string-like shape (`String`, `&str`, `Cow<str>`) without using
/// the reflective walker.
pub unsafe extern "C" fn vox_jit_encode_string_like(
    ctx: *mut EncodeCtx,
    src_ptr: *const u8,
    shape: &'static facet_core::Shape,
) -> bool {
    use facet_core::ScalarType;

    let bytes: &[u8] = match shape.scalar_type() {
        Some(ScalarType::String) if shape.is_type::<String>() => unsafe {
            (&*(src_ptr as *const String)).as_bytes()
        },
        Some(ScalarType::Str) => unsafe { (&*(src_ptr as *const &str)).as_bytes() },
        Some(ScalarType::CowStr) => unsafe {
            (&*(src_ptr as *const std::borrow::Cow<'static, str>)).as_bytes()
        },
        _ => return false,
    };

    if !unsafe { vox_jit_buf_write_varint(ctx, bytes.len() as u64) } {
        return false;
    }
    unsafe { vox_jit_buf_push_bytes(ctx, bytes.as_ptr(), bytes.len()) }
}

/// Encode a field by delegating to a nested encoder for the exact shape.
pub unsafe extern "C" fn vox_jit_encode_shape(
    ctx: *mut EncodeCtx,
    src_ptr: *const u8,
    shape: &'static facet_core::Shape,
) -> bool {
    if let Some(result) = crate::global_runtime().try_encode_ptr(PtrConst::new(src_ptr), shape) {
        let Ok(bytes) = result else { return false };
        return unsafe { vox_jit_buf_push_bytes(ctx, bytes.as_ptr(), bytes.len()) };
    }

    if handle_pure_jit_encode_miss("nested", shape).is_none() {
        return false;
    }

    let Ok(bytes) = vox_postcard::serialize::to_vec_dynamic(PtrConst::new(src_ptr), shape) else {
        return false;
    };
    unsafe { vox_jit_buf_push_bytes(ctx, bytes.as_ptr(), bytes.len()) }
}

/// Encode a bytes-like shape (`Cow<[u8]>`, `&[u8]`) without using the
/// reflective walker.
pub unsafe extern "C" fn vox_jit_encode_bytes_like(
    ctx: *mut EncodeCtx,
    src_ptr: *const u8,
    shape: &'static facet_core::Shape,
) -> bool {
    let bytes: &[u8] = match shape.def {
        facet_core::Def::Pointer(ptr_def)
            if ptr_def.known == Some(facet_core::KnownPointer::Cow) =>
        unsafe { (&*(src_ptr as *const std::borrow::Cow<'static, [u8]>)).as_ref() },
        facet_core::Def::Pointer(ptr_def)
            if ptr_def.known == Some(facet_core::KnownPointer::SharedReference) =>
        unsafe { &*(src_ptr as *const &[u8]) },
        _ => return false,
    };

    if !unsafe { vox_jit_buf_write_varint(ctx, bytes.len() as u64) } {
        return false;
    }
    unsafe { vox_jit_buf_push_bytes(ctx, bytes.as_ptr(), bytes.len()) }
}
