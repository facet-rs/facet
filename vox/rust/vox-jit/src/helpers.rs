//! JIT runtime helpers that require both `vox-jit-abi` and `vox-postcard`.
//!
//! These are registered in the JITBuilder and called via `call_indirect` from
//! generated stubs.

use facet::PtrConst;
use vox_jit_abi::{
    DecodeCtx, DecodeStatus, EncodeCtx, vox_jit_buf_push_bytes, vox_jit_buf_write_varint,
};
use vox_postcard::{TranslationPlan, ir::slow_path_decode_raw};

/// SlowPath helper: decode one field via the reflective interpreter and update
/// `ctx.consumed`. Called by generated stubs when a `SlowPath` IR op is hit.
///
/// # Safety
/// - `ctx` must be a valid, non-null `DecodeCtx`.
/// - `shape` must be a valid `&'static Shape`.
/// - `plan` must be a valid `*const TranslationPlan`.
/// - `dst_base.add(dst_offset)` must be writable for `shape.layout.size()` bytes.
#[unsafe(no_mangle)]
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

/// Opaque decode helper: read a u32le-length-prefixed byte payload and
/// initialize the destination via the shape's opaque adapter.
#[unsafe(no_mangle)]
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
#[unsafe(no_mangle)]
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

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
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
                let inner = result.ok()?;
                let mut out = Vec::with_capacity(4 + inner.len());
                out.extend_from_slice(&(inner.len() as u32).to_le_bytes());
                out.extend_from_slice(&inner);
                out
            } else {
                vox_postcard::serialize::to_vec_dynamic(PtrConst::new(src_ptr), shape).ok()?
            }
        } else {
            vox_postcard::serialize::to_vec_dynamic(PtrConst::new(src_ptr), shape).ok()?
        };
        unsafe { vox_jit_buf_push_bytes(ctx, bytes.as_ptr(), bytes.len()) }.then_some(())
    }));

    matches!(result, Ok(Some(())))
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
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_jit_encode_opaque(
    ctx: *mut EncodeCtx,
    src_ptr: *const u8,
    shape: &'static facet_core::Shape,
) -> bool {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let adapter = shape.opaque_adapter?;
        let mapped = unsafe { (adapter.serialize)(PtrConst::new(src_ptr)) };

        if let Some(bytes) =
            unsafe { vox_postcard::raw::try_decode_passthrough_bytes(mapped.ptr, mapped.shape) }
        {
            unsafe { vox_jit_buf_push_bytes(ctx, (bytes.len() as u32).to_le_bytes().as_ptr(), 4) }
                .then_some(())?;
            return unsafe { vox_jit_buf_push_bytes(ctx, bytes.as_ptr(), bytes.len()) }
                .then_some(());
        }

        if let Some(result) = crate::global_runtime().try_encode_ptr(mapped.ptr, mapped.shape) {
            let inner = result.ok()?;
            unsafe { vox_jit_buf_push_bytes(ctx, (inner.len() as u32).to_le_bytes().as_ptr(), 4) }
                .then_some(())?;
            return unsafe { vox_jit_buf_push_bytes(ctx, inner.as_ptr(), inner.len()) }
                .then_some(());
        }

        handle_pure_jit_encode_miss("opaque", shape)?;

        let bytes = vox_postcard::serialize::to_vec_dynamic(PtrConst::new(src_ptr), shape).ok()?;
        unsafe { vox_jit_buf_push_bytes(ctx, bytes.as_ptr(), bytes.len()) }.then_some(())
    }));

    matches!(result, Ok(Some(())))
}

/// Encode a proxy field by converting to the proxy value and delegating the
/// proxy shape back through the JIT runtime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_jit_encode_proxy(
    ctx: *mut EncodeCtx,
    src_ptr: *const u8,
    shape: &'static facet_core::Shape,
) -> bool {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let proxy_def = shape.proxy?;
        let proxy_shape = proxy_def.shape;
        let proxy_layout = proxy_shape.layout.sized_layout().ok()?;
        let proxy_uninit = facet_core::alloc_for_layout(proxy_layout);
        let proxy_ptr =
            unsafe { (proxy_def.convert_out)(PtrConst::new(src_ptr), proxy_uninit) }.ok()?;

        let encode_result = if let Some(result) =
            crate::global_runtime().try_encode_ptr(proxy_ptr.as_const(), proxy_shape)
        {
            result.ok().and_then(|bytes| {
                unsafe { vox_jit_buf_push_bytes(ctx, bytes.as_ptr(), bytes.len()) }.then_some(())
            })
        } else {
            handle_pure_jit_encode_miss("proxy", shape)?;
            let bytes =
                vox_postcard::serialize::to_vec_dynamic(proxy_ptr.as_const(), proxy_shape).ok()?;
            unsafe { vox_jit_buf_push_bytes(ctx, bytes.as_ptr(), bytes.len()) }.then_some(())
        };

        unsafe {
            let _ = proxy_shape.call_drop_in_place(proxy_ptr);
            facet_core::dealloc_for_layout(proxy_ptr, proxy_layout);
        }

        encode_result
    }));

    matches!(result, Ok(Some(())))
}

/// Initialize a `Cow<[u8]>` with owned bytes.
///
/// # Safety
/// - `dst` must point to writable storage for `Cow<[u8]>`.
/// - `data` must be valid for `len` bytes.
#[unsafe(no_mangle)]
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
#[unsafe(no_mangle)]
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
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_jit_init_byte_slice_ref(dst: *mut u8, data: *const u8, len: usize) {
    let bytes: &'static [u8] =
        unsafe { std::mem::transmute(std::slice::from_raw_parts(data, len)) };
    unsafe {
        std::ptr::write(dst as *mut &'static [u8], bytes);
    }
}

/// Initialize a `Cow<str>` with owned bytes after UTF-8 validation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_jit_init_cow_str_owned(dst: *mut u8, data: *const u8, len: usize) {
    let bytes = unsafe { std::slice::from_raw_parts(data, len) };
    let s = std::str::from_utf8(bytes).expect("JIT emitted invalid UTF-8 for Cow<str>");
    let value = std::borrow::Cow::<'static, str>::Owned(s.to_owned());
    unsafe {
        std::ptr::write(dst as *mut std::borrow::Cow<'static, str>, value);
    }
}

/// Initialize a `Cow<str>` borrowing from the input buffer.
#[unsafe(no_mangle)]
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
#[unsafe(no_mangle)]
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
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_jit_encode_string_like(
    ctx: *mut EncodeCtx,
    src_ptr: *const u8,
    shape: &'static facet_core::Shape,
) -> bool {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        use facet_core::ScalarType;

        let bytes: &[u8] = match shape.scalar_type() {
            Some(ScalarType::String) if shape.is_type::<String>() => unsafe {
                (&*(src_ptr as *const String)).as_bytes()
            },
            Some(ScalarType::Str) => unsafe { (&*(src_ptr as *const &str)).as_bytes() },
            Some(ScalarType::CowStr) => unsafe {
                (&*(src_ptr as *const std::borrow::Cow<'static, str>)).as_bytes()
            },
            _ => return None,
        };

        if !unsafe { vox_jit_buf_write_varint(ctx, bytes.len() as u64) } {
            return None;
        }
        unsafe { vox_jit_buf_push_bytes(ctx, bytes.as_ptr(), bytes.len()) }.then_some(())
    }));

    matches!(result, Ok(Some(())))
}

/// Encode a field by delegating to a nested encoder for the exact shape.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_jit_encode_shape(
    ctx: *mut EncodeCtx,
    src_ptr: *const u8,
    shape: &'static facet_core::Shape,
) -> bool {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if let Some(result) = crate::global_runtime().try_encode_ptr(PtrConst::new(src_ptr), shape)
        {
            let bytes = result.ok()?;
            return unsafe { vox_jit_buf_push_bytes(ctx, bytes.as_ptr(), bytes.len()) }
                .then_some(());
        }

        handle_pure_jit_encode_miss("nested", shape)?;

        let bytes = vox_postcard::serialize::to_vec_dynamic(PtrConst::new(src_ptr), shape).ok()?;
        unsafe { vox_jit_buf_push_bytes(ctx, bytes.as_ptr(), bytes.len()) }.then_some(())
    }));

    matches!(result, Ok(Some(())))
}

/// Encode a bytes-like shape (`Cow<[u8]>`, `&[u8]`) without using the
/// reflective walker.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_jit_encode_bytes_like(
    ctx: *mut EncodeCtx,
    src_ptr: *const u8,
    shape: &'static facet_core::Shape,
) -> bool {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let bytes: &[u8] = match shape.def {
            facet_core::Def::Pointer(ptr_def)
                if ptr_def.known == Some(facet_core::KnownPointer::Cow) =>
            unsafe { (&*(src_ptr as *const std::borrow::Cow<'static, [u8]>)).as_ref() },
            facet_core::Def::Pointer(ptr_def)
                if ptr_def.known == Some(facet_core::KnownPointer::SharedReference) =>
            unsafe { &*(src_ptr as *const &[u8]) },
            _ => return None,
        };

        if !unsafe { vox_jit_buf_write_varint(ctx, bytes.len() as u64) } {
            return None;
        }
        unsafe { vox_jit_buf_push_bytes(ctx, bytes.as_ptr(), bytes.len()) }.then_some(())
    }));

    matches!(result, Ok(Some(())))
}

/// Encode `vox_types::MetadataEntry` without the reflective walker.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_jit_encode_metadata_entry(
    ctx: *mut EncodeCtx,
    src_ptr: *const u8,
) -> bool {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        use vox_types::MetadataValue;

        let entry = unsafe { &*(src_ptr as *const vox_types::MetadataEntry<'static>) };

        if !unsafe { vox_jit_buf_write_varint(ctx, entry.key.len() as u64) } {
            return None;
        }
        if !unsafe { vox_jit_buf_push_bytes(ctx, entry.key.as_bytes().as_ptr(), entry.key.len()) } {
            return None;
        }

        match &entry.value {
            MetadataValue::String(s) => {
                if !unsafe { vox_jit_buf_write_varint(ctx, 0) } {
                    return None;
                }
                let s = s.as_ref();
                if !unsafe { vox_jit_buf_write_varint(ctx, s.len() as u64) } {
                    return None;
                }
                if !unsafe { vox_jit_buf_push_bytes(ctx, s.as_bytes().as_ptr(), s.len()) } {
                    return None;
                }
            }
            MetadataValue::Bytes(b) => {
                if !unsafe { vox_jit_buf_write_varint(ctx, 1) } {
                    return None;
                }
                let b = b.as_ref();
                if !unsafe { vox_jit_buf_write_varint(ctx, b.len() as u64) } {
                    return None;
                }
                if !unsafe { vox_jit_buf_push_bytes(ctx, b.as_ptr(), b.len()) } {
                    return None;
                }
            }
            MetadataValue::U64(n) => {
                if !unsafe { vox_jit_buf_write_varint(ctx, 2) } {
                    return None;
                }
                if !unsafe { vox_jit_buf_write_varint(ctx, *n) } {
                    return None;
                }
            }
        }

        let flags = unsafe { *(std::ptr::addr_of!(entry.flags) as *const u64) };
        unsafe { vox_jit_buf_write_varint(ctx, flags) }.then_some(())
    }));

    matches!(result, Ok(Some(())))
}
