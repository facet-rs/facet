//! JIT runtime helpers that require both `vox-jit-abi` and `vox-postcard`.
//!
//! These are registered in the JITBuilder and called via `call_indirect` from
//! generated stubs.

use vox_jit_abi::{DecodeCtx, DecodeStatus};
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
