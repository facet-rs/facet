//! Stable extern "C" ABI types and runtime helpers for the vox Cranelift JIT.
//!
//! All generated stubs and runtime helpers use the types defined here.
//! This crate has no Cranelift dependency — it is safe to import from both
//! the JIT backend and the runtime call site.
//!
//! # Safety contract
//!
//! Generated stubs are `unsafe extern "C" fn`. Callers must:
//! - Pass a valid, non-null `DecodeCtx` pointer.
//! - Pass `out_ptr` pointing to at least `size_of::<T>()` bytes of writable,
//!   suitably aligned uninitialized memory (zeroed before the call).
//! - Not read from `out_ptr` if the return status is not `Ok`.
//! - On partial list/array decode failure: drop only `ctx.init_count` elements
//!   and free the backing allocation using `helper_vec_drop_partial`.
//!
//! # No unwind
//!
//! No generated frame may propagate a Rust panic. All helpers are non-panicking
//! and return `DecodeStatus` error codes instead. The `#[no_panic]` guarantee
//! is enforced by implementation convention — every path returns a status code.

#![allow(unsafe_code)]

pub use vox_jit_cal::{
    BorrowMode, CalibrationRegistry, ContainerKind, DescriptorHandle, OFFSET_ABSENT,
    OpaqueDescriptor,
};

// ---------------------------------------------------------------------------
// Status codes
// ---------------------------------------------------------------------------

/// Status code returned by all generated decode stubs and runtime helpers.
///
/// Discriminant values are stable and part of the ABI. Do not reorder.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeStatus {
    /// Decode succeeded. `out_ptr` holds a fully initialized value.
    Ok = 0,
    /// Input ended before the value was complete.
    UnexpectedEof = 1,
    /// A varint exceeded the maximum encoded width.
    VarintOverflow = 2,
    /// A bool byte was neither 0x00 nor 0x01.
    InvalidBool = 3,
    /// A string/char byte sequence failed UTF-8 validation.
    InvalidUtf8 = 4,
    /// An option tag was neither 0x00 nor 0x01.
    InvalidOptionTag = 5,
    /// An enum discriminant was out of the range known to the local type.
    InvalidEnumDiscriminant = 6,
    /// The remote enum variant has no local equivalent.
    UnknownVariant = 7,
    /// A memory allocation inside a helper failed (OOM).
    AllocFailed = 8,
}

impl DecodeStatus {
    #[inline]
    pub fn is_ok(self) -> bool {
        self == DecodeStatus::Ok
    }
}

// ---------------------------------------------------------------------------
// Decode context
// ---------------------------------------------------------------------------

/// Context block threaded through all generated decode stubs via pointer.
///
/// Layout is stable and part of the ABI. Fields must not be reordered.
#[repr(C)]
pub struct DecodeCtx {
    /// Pointer to the start of the input slice.
    pub input_ptr: *const u8,
    /// Total length of the input slice in bytes.
    pub input_len: usize,
    /// Number of input bytes consumed so far. Updated by generated stubs.
    pub consumed: usize,
    /// On failure: the byte position at which the error occurred.
    pub error_pos: usize,
    /// On partial-init failure of a list/array: number of elements successfully
    /// initialized before the failure. Generated code must write this field
    /// before returning any non-Ok status from an aggregate decode path, so the
    /// caller can drop only the initialized prefix.
    pub init_count: usize,
}

impl DecodeCtx {
    #[inline]
    pub fn new(input: &[u8]) -> Self {
        DecodeCtx {
            input_ptr: input.as_ptr(),
            input_len: input.len(),
            consumed: 0,
            error_pos: 0,
            init_count: 0,
        }
    }

    /// Return the slice of bytes not yet consumed.
    ///
    /// # Safety
    /// `input_ptr`/`input_len` must come from a valid slice; generated stubs
    /// must maintain `consumed <= input_len`.
    #[inline]
    pub unsafe fn remaining(&self) -> &[u8] {
        unsafe {
            core::slice::from_raw_parts(
                self.input_ptr.add(self.consumed),
                self.input_len - self.consumed,
            )
        }
    }
}

// SAFETY: DecodeCtx contains raw pointers; callers are responsible for
// single-threaded access during a stub call.
unsafe impl Send for DecodeCtx {}

// ---------------------------------------------------------------------------
// Generated stub function pointer types
// ---------------------------------------------------------------------------

/// Owned-decode stub type: reads postcard bytes, writes a fully initialized
/// value into `out_ptr`.
pub type OwnedDecodeFn =
    unsafe extern "C" fn(ctx: *mut DecodeCtx, out_ptr: *mut u8) -> DecodeStatus;

/// Borrowed-decode stub type: same as owned but the written value may contain
/// pointers into `ctx.input_ptr`. Lifetime correctness is the caller's
/// responsibility via surrounding Rust wrapper types.
pub type BorrowedDecodeFn =
    unsafe extern "C" fn(ctx: *mut DecodeCtx, out_ptr: *mut u8) -> DecodeStatus;

// ---------------------------------------------------------------------------
// Runtime helpers — called by generated stubs via stable extern "C" symbols
// ---------------------------------------------------------------------------
//
// All helpers:
//   - are non-panicking (every error path returns a DecodeStatus)
//   - are ABI-stable within the process
//   - are explicit about ownership transfer and cleanup responsibility

/// Allocate backing storage for `cap` elements of a Vec-family container
/// (`Vec<T>` or `String`) and return the data pointer.
///
/// This helper does allocation only. The generated code is responsible for
/// writing ptr/len/cap into the container header using calibrated offsets.
///
/// Returns:
/// - non-null data pointer on success
/// - alignment sentinel for ZST elements or `cap == 0`
/// - null on OOM or invalid layout
///
/// # Safety
/// - `desc` must be a valid calibrated descriptor with `kind == Vec` or
///   `kind == String`. Do not call with `BoxOwned` or `BoxSlice` descriptors.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_jit_vec_alloc(desc: *const OpaqueDescriptor, cap: usize) -> *mut u8 {
    let desc = unsafe { &*desc };

    if desc.elem_size == 0 || cap == 0 {
        return desc.elem_align as *mut u8;
    }

    let layout = match std::alloc::Layout::from_size_align(cap * desc.elem_size, desc.elem_align) {
        Ok(l) => l,
        Err(_) => return core::ptr::null_mut(),
    };

    let data_ptr = unsafe { std::alloc::alloc(layout) };
    if data_ptr.is_null() {
        core::ptr::null_mut()
    } else {
        data_ptr
    }
}

/// `String`-specific alloc — identical semantics to `vox_jit_vec_alloc`
/// but under a distinct symbol so the call site is unambiguous about which
/// calibrated type it is operating on.
///
/// # Safety
/// Same as `vox_jit_vec_alloc`. `desc.kind` must be `String`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_jit_string_alloc(
    desc: *const OpaqueDescriptor,
    cap: usize,
) -> *mut u8 {
    // Implementation is identical; the symbol distinction is the contract.
    unsafe { vox_jit_vec_alloc(desc, cap) }
}

/// Drop the first `init_count` initialized elements of a partially initialized
/// Vec-family container, then free the backing allocation.
///
/// This is the failure-path cleanup. The caller is responsible for not calling
/// drop on any element index >= `init_count`.
///
/// `drop_glue` is called once per initialized element pointer. If `None`, only
/// the backing allocation is freed (suitable for `Copy` element types).
///
/// Non-panicking: if the backing layout cannot be reconstructed (should not
/// happen with calibrated values), the allocation is leaked rather than causing
/// UB or a panic.
///
/// # Safety
/// - `desc` valid, `container_ptr` initialized with ptr/cap fields matching the
///   allocation returned by `vox_jit_vec_alloc`.
/// - Elements `[0, init_count)` are fully initialized T values.
/// - Elements `[init_count, cap)` are uninitialized and must not be dropped.
/// - `drop_glue`, if provided, must correctly drop a single T at the given ptr.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_jit_vec_drop_partial(
    desc: *const OpaqueDescriptor,
    container_ptr: *mut u8,
    init_count: usize,
    drop_glue: Option<unsafe extern "C" fn(*mut u8)>,
) {
    let desc = unsafe { &*desc };

    // ZSTs: no backing allocation, nothing to free.
    if desc.elem_size == 0 {
        return;
    }

    let data_ptr =
        unsafe { (container_ptr.add(desc.ptr_offset as usize) as *const *mut u8).read_unaligned() };

    // Drop each initialized element.
    if let Some(drop_fn) = drop_glue {
        for i in 0..init_count {
            let elem_ptr = unsafe { data_ptr.add(i * desc.elem_size) };
            unsafe { drop_fn(elem_ptr) };
        }
    }

    // Free the backing allocation. Read cap from the header.
    let cap = if desc.cap_offset != OFFSET_ABSENT {
        unsafe { (container_ptr.add(desc.cap_offset as usize) as *const usize).read_unaligned() }
    } else {
        // BoxSlice: no cap field — use init_count as the allocated length
        // (for BoxSlice the allocation is exactly init_count elements).
        init_count
    };

    if cap == 0 || data_ptr as usize == desc.elem_align {
        // Zero-cap or alignment-sentinel: no real allocation to free.
        return;
    }

    if let Ok(layout) = std::alloc::Layout::from_size_align(cap * desc.elem_size, desc.elem_align) {
        unsafe { std::alloc::dealloc(data_ptr, layout) };
    }
    // If layout reconstruction fails (calibration bug), we leak. Non-panicking.
}

/// Allocate a single `T`-sized heap slot for a `Box<T>` and write the pointer
/// into `out_ptr`.
///
/// On success: writes the heap pointer into `out_ptr` at `desc.ptr_offset`,
/// returns `Ok`. The allocation is uninitialized — the caller must initialize
/// the slot before the Box is considered valid.
///
/// On OOM: returns `AllocFailed` without writing `out_ptr`.
///
/// # Safety
/// - `desc` valid with `kind == BoxOwned`.
/// - `out_ptr` writable for `desc.size` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_jit_box_alloc(
    desc: *const OpaqueDescriptor,
    out_ptr: *mut u8,
) -> DecodeStatus {
    let desc = unsafe { &*desc };

    if desc.elem_size == 0 {
        // ZST Box: pointer is the alignment sentinel — no allocation.
        let sentinel = desc.elem_align as *mut u8;
        unsafe { write_ptr_field(out_ptr, desc.ptr_offset, sentinel) };
        return DecodeStatus::Ok;
    }

    let layout = match std::alloc::Layout::from_size_align(desc.elem_size, desc.elem_align) {
        Ok(l) => l,
        Err(_) => return DecodeStatus::AllocFailed,
    };

    let data_ptr = unsafe { std::alloc::alloc(layout) };
    if data_ptr.is_null() {
        return DecodeStatus::AllocFailed;
    }

    unsafe { write_ptr_field(out_ptr, desc.ptr_offset, data_ptr) };
    DecodeStatus::Ok
}

/// Allocate a `Box<[T]>` backing store for `len` elements and write the fat
/// pointer (data ptr + len) into `out_ptr`.
///
/// On success: returns `Ok`; the data pointer points to `len * desc.elem_size`
/// bytes of uninitialized memory.
///
/// # Safety
/// - `desc` valid with `kind == BoxSlice`.
/// - `out_ptr` writable for `desc.size` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_jit_box_slice_alloc(
    desc: *const OpaqueDescriptor,
    len: usize,
    out_ptr: *mut u8,
) -> DecodeStatus {
    let desc = unsafe { &*desc };

    if desc.elem_size == 0 || len == 0 {
        // ZST or empty: dangling sentinel + len.
        let sentinel = if desc.elem_size == 0 {
            desc.elem_align as *mut u8
        } else {
            // len == 0 non-ZST: use align sentinel (same as std).
            desc.elem_align as *mut u8
        };
        unsafe {
            write_ptr_field(out_ptr, desc.ptr_offset, sentinel);
            if desc.len_offset != OFFSET_ABSENT {
                write_usize_field(out_ptr, desc.len_offset, len);
            }
        }
        return DecodeStatus::Ok;
    }

    let layout = match std::alloc::Layout::from_size_align(len * desc.elem_size, desc.elem_align) {
        Ok(l) => l,
        Err(_) => return DecodeStatus::AllocFailed,
    };

    let data_ptr = unsafe { std::alloc::alloc(layout) };
    if data_ptr.is_null() {
        return DecodeStatus::AllocFailed;
    }

    unsafe {
        write_ptr_field(out_ptr, desc.ptr_offset, data_ptr);
        if desc.len_offset != OFFSET_ABSENT {
            write_usize_field(out_ptr, desc.len_offset, len);
        }
    }
    DecodeStatus::Ok
}

/// Validate that `bytes[..len]` is valid UTF-8.
///
/// Returns `Ok` on success, `InvalidUtf8` on failure.
///
/// # Safety
/// `bytes` must point to at least `len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_jit_utf8_validate(bytes: *const u8, len: usize) -> DecodeStatus {
    let slice = unsafe { core::slice::from_raw_parts(bytes, len) };
    match core::str::from_utf8(slice) {
        Ok(_) => DecodeStatus::Ok,
        Err(_) => DecodeStatus::InvalidUtf8,
    }
}

// ---------------------------------------------------------------------------
// Encode context
// ---------------------------------------------------------------------------

/// Context block threaded through all generated encode stubs via pointer.
///
/// The output buffer is a simple growable byte array. Generated stubs write
/// directly into `buf_ptr[buf_len..]` when space is available, calling
/// `vox_jit_buf_grow` when the buffer is full.
///
/// Layout is stable and part of the ABI. Fields must not be reordered.
#[repr(C)]
pub struct EncodeCtx {
    /// Pointer to the start of the output buffer allocation.
    pub buf_ptr: *mut u8,
    /// Number of bytes written so far (logical length).
    pub buf_len: usize,
    /// Total capacity of the current allocation.
    pub buf_cap: usize,
}

impl EncodeCtx {
    /// Create a new encode context with the given initial capacity.
    ///
    /// Allocates a heap buffer; must be freed via `into_vec` or `vox_jit_buf_free`.
    pub fn with_capacity(cap: usize) -> Self {
        let cap = cap.max(64);
        let layout = std::alloc::Layout::from_size_align(cap, 1).expect("valid layout");
        let buf_ptr = unsafe { std::alloc::alloc(layout) };
        assert!(!buf_ptr.is_null(), "initial EncodeCtx allocation failed");
        EncodeCtx {
            buf_ptr,
            buf_len: 0,
            buf_cap: cap,
        }
    }

    /// Consume the context and return the written bytes as a `Vec<u8>`.
    ///
    /// The returned `Vec` owns the allocation.
    pub fn into_vec(mut self) -> Vec<u8> {
        let len = self.buf_len;
        let cap = self.buf_cap;
        let ptr = self.buf_ptr;
        // Prevent drop from freeing (we hand ownership to Vec).
        self.buf_ptr = std::ptr::null_mut();
        self.buf_len = 0;
        self.buf_cap = 0;
        unsafe { Vec::from_raw_parts(ptr, len, cap) }
    }
}

impl Drop for EncodeCtx {
    fn drop(&mut self) {
        if !self.buf_ptr.is_null() && self.buf_cap > 0 {
            let layout =
                std::alloc::Layout::from_size_align(self.buf_cap, 1).expect("valid layout in drop");
            unsafe { std::alloc::dealloc(self.buf_ptr, layout) };
        }
    }
}

// SAFETY: EncodeCtx contains raw pointers; callers are responsible for
// single-threaded access during a stub call.
unsafe impl Send for EncodeCtx {}

/// Generated encode stub type.
///
/// Reads from `src_ptr` (pointing to the value being encoded) and writes
/// postcard bytes into `ctx`. Returns `true` on success, `false` on OOM.
pub type EncodeFn = unsafe extern "C" fn(ctx: *mut EncodeCtx, src_ptr: *const u8) -> bool;

/// Grow the encode buffer to fit at least `needed` additional bytes.
///
/// Doubles the capacity (or sets it to `buf_len + needed` if larger).
/// Returns `true` on success, `false` on OOM. On OOM the buffer is unchanged.
///
/// # Safety
/// - `ctx` must be a valid, non-null `EncodeCtx`.
/// - The new allocation replaces `ctx.buf_ptr`; any previously cached pointer
///   into the buffer is invalidated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_jit_buf_grow(ctx: *mut EncodeCtx, needed: usize) -> bool {
    let ctx = unsafe { &mut *ctx };
    let new_cap = (ctx.buf_cap * 2).max(ctx.buf_len + needed).max(64);
    let new_layout = match std::alloc::Layout::from_size_align(new_cap, 1) {
        Ok(l) => l,
        Err(_) => return false,
    };
    let new_ptr = if ctx.buf_ptr.is_null() || ctx.buf_cap == 0 {
        unsafe { std::alloc::alloc(new_layout) }
    } else {
        let old_layout =
            std::alloc::Layout::from_size_align(ctx.buf_cap, 1).expect("valid old layout");
        unsafe { std::alloc::realloc(ctx.buf_ptr, old_layout, new_cap) }
    };
    if new_ptr.is_null() {
        return false;
    }
    ctx.buf_ptr = new_ptr;
    ctx.buf_cap = new_cap;
    true
}

/// Write a single byte to the encode buffer.
///
/// Grows the buffer if needed. Returns `true` on success, `false` on OOM.
///
/// # Safety
/// `ctx` must be a valid, non-null `EncodeCtx`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_jit_buf_push_byte(ctx: *mut EncodeCtx, byte: u8) -> bool {
    let ctx_ref = unsafe { &mut *ctx };
    if ctx_ref.buf_len >= ctx_ref.buf_cap {
        if !unsafe { vox_jit_buf_grow(ctx, 1) } {
            return false;
        }
    }
    let ctx_ref = unsafe { &mut *ctx };
    unsafe { ctx_ref.buf_ptr.add(ctx_ref.buf_len).write(byte) };
    ctx_ref.buf_len += 1;
    true
}

/// Write a slice of bytes to the encode buffer.
///
/// Grows the buffer if needed. Returns `true` on success, `false` on OOM.
///
/// # Safety
/// - `ctx` must be a valid, non-null `EncodeCtx`.
/// - `data` must point to at least `len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_jit_buf_push_bytes(
    ctx: *mut EncodeCtx,
    data: *const u8,
    len: usize,
) -> bool {
    if len == 0 {
        return true;
    }
    let ctx_ref = unsafe { &mut *ctx };
    let needed = len.saturating_sub(ctx_ref.buf_cap.saturating_sub(ctx_ref.buf_len));
    if needed > 0 && !unsafe { vox_jit_buf_grow(ctx, len) } {
        return false;
    }
    let ctx_ref = unsafe { &mut *ctx };
    unsafe {
        core::ptr::copy_nonoverlapping(data, ctx_ref.buf_ptr.add(ctx_ref.buf_len), len);
    }
    ctx_ref.buf_len += len;
    true
}

/// Write a u64 as a postcard varint to the encode buffer.
///
/// Returns `true` on success, `false` on OOM.
///
/// # Safety
/// `ctx` must be a valid, non-null `EncodeCtx`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_jit_buf_write_varint(ctx: *mut EncodeCtx, mut value: u64) -> bool {
    while value >= 0x80 {
        if !unsafe { vox_jit_buf_push_byte(ctx, (value as u8) | 0x80) } {
            return false;
        }
        value >>= 7;
    }
    unsafe { vox_jit_buf_push_byte(ctx, value as u8) }
}

/// Write an i64 as a zigzag-encoded postcard varint to the encode buffer.
///
/// Returns `true` on success, `false` on OOM.
///
/// # Safety
/// `ctx` must be a valid, non-null `EncodeCtx`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_jit_buf_write_varint_signed(ctx: *mut EncodeCtx, value: i64) -> bool {
    let zigzag = ((value << 1) ^ (value >> 63)) as u64;
    unsafe { vox_jit_buf_write_varint(ctx, zigzag) }
}

/// Write a Vec-family opaque type as postcard bytes into the buffer.
///
/// For `Vec<u8>` / `String`: writes varint length then the raw bytes.
/// For other `Vec<T>`: writes varint element count, then each element via
/// `elem_encode_fn`.
///
/// The function pointer `elem_encode_fn`, if non-null, is called once per
/// element with `(ctx, elem_ptr)`. It must not be null for non-byte vectors.
///
/// # Safety
/// - `desc` must be a valid calibrated Vec or String descriptor.
/// - `src_ptr` must point to a valid, initialized container value.
/// - `elem_encode_fn`, if non-null, must be a valid encode stub for the element type.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_jit_buf_write_opaque_vec(
    ctx: *mut EncodeCtx,
    desc: *const OpaqueDescriptor,
    src_ptr: *const u8,
    elem_encode_fn: Option<EncodeFn>,
) -> bool {
    let desc = unsafe { &*desc };

    // Read the data ptr and length from the container.
    let data_ptr =
        unsafe { (src_ptr.add(desc.ptr_offset as usize) as *const *const u8).read_unaligned() };
    let len = unsafe { (src_ptr.add(desc.len_offset as usize) as *const usize).read_unaligned() };

    if desc.elem_size == 1 && elem_encode_fn.is_none() {
        // String / Vec<u8>: write varint len + raw bytes.
        if !unsafe { vox_jit_buf_write_varint(ctx, len as u64) } {
            return false;
        }
        if len > 0 {
            return unsafe { vox_jit_buf_push_bytes(ctx, data_ptr, len) };
        }
        return true;
    }

    // Generic Vec<T>: write varint element count, then encode each element.
    if !unsafe { vox_jit_buf_write_varint(ctx, len as u64) } {
        return false;
    }
    if let Some(encode_fn) = elem_encode_fn {
        for i in 0..len {
            let elem_ptr = unsafe { data_ptr.add(i * desc.elem_size) };
            if !unsafe { encode_fn(ctx, elem_ptr) } {
                return false;
            }
        }
    }
    true
}

// ---------------------------------------------------------------------------
// Cache key types
// ---------------------------------------------------------------------------

/// Identifies a compiled decode stub in the process-local stub cache.
///
/// `local_shape: &'static Shape` uses Shape's own `Hash`/`Eq` via the blanket
/// `impl<T: Hash> Hash for &T` — not the pointer address.
///
/// Do not cache a stub when `descriptor_handle` is `None`; fall back to the IR
/// interpreter instead.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DecodeCacheKey {
    pub remote_schema_id: u64,
    pub local_shape: &'static facet_core::Shape,
    pub borrow_mode: BorrowMode,
    pub target_isa: &'static str,
    pub descriptor_handle: Option<DescriptorHandle>,
}

/// Identifies a compiled encode stub in the process-local stub cache.
///
/// `local_shape: &'static Shape` uses Shape's own `Hash`/`Eq` via the blanket
/// `impl<T: Hash> Hash for &T` — not the pointer address.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EncodeCacheKey {
    pub local_shape: &'static facet_core::Shape,
    pub borrow_mode: BorrowMode,
    pub target_isa: &'static str,
    pub descriptor_handle: Option<DescriptorHandle>,
}

// ---------------------------------------------------------------------------
// Internal write helpers (not extern "C" — used only within this crate)
// ---------------------------------------------------------------------------

#[inline]
unsafe fn write_ptr_field(base: *mut u8, offset: u8, val: *mut u8) {
    let field = unsafe { base.add(offset as usize) as *mut *mut u8 };
    // write_unaligned: out_ptr is an opaque byte buffer; alignment not guaranteed.
    unsafe { field.write_unaligned(val) };
}

#[inline]
unsafe fn write_usize_field(base: *mut u8, offset: u8, val: usize) {
    let field = unsafe { base.add(offset as usize) as *mut usize };
    // write_unaligned: out_ptr is an opaque byte buffer; alignment not guaranteed.
    unsafe { field.write_unaligned(val) };
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use vox_jit_cal::{CalibrationRegistry, ContainerKind, OFFSET_ABSENT, OpaqueDescriptor};

    use crate::{
        DecodeStatus, vox_jit_box_alloc, vox_jit_string_alloc, vox_jit_utf8_validate,
        vox_jit_vec_alloc, vox_jit_vec_drop_partial, write_ptr_field, write_usize_field,
    };

    fn desc_for_vec_u32() -> OpaqueDescriptor {
        let mut reg = CalibrationRegistry::new();
        let h = reg.calibrate_vec::<u32>().expect("Vec<u32> calibration");
        reg.get(h).expect("handle valid").clone()
    }

    fn desc_for_string() -> OpaqueDescriptor {
        let mut reg = CalibrationRegistry::new();
        let h = reg.calibrate_string().expect("String calibration");
        reg.get(h).expect("handle valid").clone()
    }

    fn desc_for_box_u32() -> OpaqueDescriptor {
        let mut reg = CalibrationRegistry::new();
        let h = reg.calibrate_box_t::<u32>().expect("Box<u32> calibration");
        reg.get(h).expect("handle valid").clone()
    }

    // -----------------------------------------------------------------------
    // vox_jit_vec_alloc
    // -----------------------------------------------------------------------

    #[test]
    fn vec_alloc_nonzero_cap_allocates() {
        let desc = desc_for_vec_u32();
        let data_ptr = unsafe { vox_jit_vec_alloc(&desc as *const _, 4) as *mut u32 };

        assert!(!data_ptr.is_null(), "data ptr must be non-null after alloc");

        // Clean up: reconstruct and drop the vec to avoid leak.
        unsafe {
            let _ = Vec::from_raw_parts(data_ptr, 0, 4);
        }
    }

    #[test]
    fn vec_alloc_zero_cap_returns_alignment_sentinel() {
        let desc = desc_for_vec_u32();
        let data_ptr = unsafe { vox_jit_vec_alloc(&desc as *const _, 0) };
        assert_eq!(data_ptr, desc.elem_align as *mut u8);
    }

    // -----------------------------------------------------------------------
    // vox_jit_vec_drop_partial
    // -----------------------------------------------------------------------

    #[test]
    fn vec_drop_partial_copy_type_no_glue() {
        let desc = desc_for_vec_u32();
        let mut buf = vec![0u8; desc.size];
        let data_ptr = unsafe { vox_jit_vec_alloc(&desc as *const _, 4) as *mut u32 };
        unsafe {
            write_ptr_field(buf.as_mut_ptr(), desc.ptr_offset, data_ptr as *mut u8);
            write_usize_field(buf.as_mut_ptr(), desc.len_offset, 2);
            write_usize_field(buf.as_mut_ptr(), desc.cap_offset, 4);
        }

        // Write 2 u32 values (they're Copy — no destructor needed).
        unsafe {
            data_ptr.write(10);
            data_ptr.add(1).write(20);
        }

        // Simulate failure after 2 elements: drop_partial with no glue.
        // This should free the backing allocation without calling any destructor.
        unsafe { vox_jit_vec_drop_partial(&desc as *const _, buf.as_mut_ptr(), 2, None) };
        // If we get here without ASAN/Miri reporting a double-free, the test passes.
    }

    // -----------------------------------------------------------------------
    // vox_jit_string_alloc
    // -----------------------------------------------------------------------

    #[test]
    fn string_alloc_nonzero_cap() {
        let desc = desc_for_string();
        let str_ptr = unsafe { vox_jit_string_alloc(&desc as *const _, 16) };
        assert!(!str_ptr.is_null());

        // Clean up: reconstruct the String backing to avoid leak.
        unsafe {
            drop(String::from_raw_parts(str_ptr, 0, 16));
        }
    }

    // -----------------------------------------------------------------------
    // vox_jit_box_alloc
    // -----------------------------------------------------------------------

    #[test]
    fn box_alloc_u32_sets_nonnull_ptr() {
        let desc = desc_for_box_u32();
        let mut buf = vec![0u8; desc.size];
        let status = unsafe { vox_jit_box_alloc(&desc as *const _, buf.as_mut_ptr()) };
        assert_eq!(status, DecodeStatus::Ok);

        // Read back the pointer with full provenance by using *mut u32 read_unaligned.
        let box_ptr = unsafe {
            (buf.as_ptr().add(desc.ptr_offset as usize) as *const *mut u32).read_unaligned()
        };
        assert!(!box_ptr.is_null(), "Box<u32> ptr must be non-null");

        // Write a value and free via Box to avoid leak.
        unsafe {
            box_ptr.write(42);
            drop(Box::from_raw(box_ptr));
        }
    }

    // -----------------------------------------------------------------------
    // vox_jit_utf8_validate
    // -----------------------------------------------------------------------

    #[test]
    fn utf8_validate_valid_ascii() {
        let s = b"hello";
        let status = unsafe { vox_jit_utf8_validate(s.as_ptr(), s.len()) };
        assert_eq!(status, DecodeStatus::Ok);
    }

    #[test]
    fn utf8_validate_valid_unicode() {
        let s = "こんにちは".as_bytes();
        let status = unsafe { vox_jit_utf8_validate(s.as_ptr(), s.len()) };
        assert_eq!(status, DecodeStatus::Ok);
    }

    #[test]
    fn utf8_validate_invalid_bytes() {
        let bad = b"\xff\xfe";
        let status = unsafe { vox_jit_utf8_validate(bad.as_ptr(), bad.len()) };
        assert_eq!(status, DecodeStatus::InvalidUtf8);
    }

    #[test]
    fn utf8_validate_empty() {
        let status = unsafe { vox_jit_utf8_validate(b"".as_ptr(), 0) };
        assert_eq!(status, DecodeStatus::Ok);
    }

    // -----------------------------------------------------------------------
    // ContainerKind / cap_offset guard
    // -----------------------------------------------------------------------

    #[test]
    fn string_desc_has_absent_cap_sentinel_respected() {
        // String has cap_offset present (not ABSENT).
        let desc = desc_for_string();
        assert_ne!(
            desc.cap_offset, OFFSET_ABSENT,
            "String must have a cap slot"
        );
        assert_eq!(desc.kind, ContainerKind::String);
    }

    #[test]
    fn box_t_desc_has_absent_len_and_cap() {
        let desc = desc_for_box_u32();
        assert_eq!(desc.kind, ContainerKind::BoxOwned);
        assert_eq!(desc.len_offset, OFFSET_ABSENT);
        assert_eq!(desc.cap_offset, OFFSET_ABSENT);
    }

    // -----------------------------------------------------------------------
    // vox_jit_vec_drop_partial: counting drop_glue verifies exact boundary
    // -----------------------------------------------------------------------

    // Global mutex serializes the two counting-glue tests so they can't
    // interfere with each other's DROP_COUNT resets.
    // Using a global + extern "C" fn is the only way to pass a counter
    // through the `unsafe extern "C" fn(*mut u8)` glue signature.
    static DROP_COUNT_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    static DROP_COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    unsafe extern "C" fn counting_drop_glue(_ptr: *mut u8) {
        DROP_COUNT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }

    #[test]
    fn vec_drop_partial_counting_glue_respects_init_count() {
        let _guard = DROP_COUNT_LOCK.lock().unwrap();
        DROP_COUNT.store(0, std::sync::atomic::Ordering::SeqCst);

        let desc = desc_for_vec_u32();
        let mut buf = vec![0u8; desc.size];

        // Allocate 4 slots and write the container header.
        let data_ptr = unsafe { vox_jit_vec_alloc(&desc as *const _, 4) as *mut u32 };
        unsafe {
            write_ptr_field(buf.as_mut_ptr(), desc.ptr_offset, data_ptr as *mut u8);
            write_usize_field(buf.as_mut_ptr(), desc.len_offset, 2);
            write_usize_field(buf.as_mut_ptr(), desc.cap_offset, 4);
        }

        // Write elements at indices 0 and 1 only.
        unsafe {
            data_ptr.write(10);
            data_ptr.add(1).write(20);
            // indices 2 and 3 are intentionally left uninitialized (simulate mid-decode failure)
        }

        // Drop with init_count=2: glue must be called exactly twice.
        unsafe {
            vox_jit_vec_drop_partial(
                &desc as *const _,
                buf.as_mut_ptr(),
                2,
                Some(counting_drop_glue),
            )
        };

        let count = DROP_COUNT.load(std::sync::atomic::Ordering::SeqCst);
        assert_eq!(
            count, 2,
            "drop_glue must be called exactly init_count=2 times, got {count}"
        );
    }

    #[test]
    fn vec_drop_partial_zero_init_count_calls_no_glue() {
        let _guard = DROP_COUNT_LOCK.lock().unwrap();
        DROP_COUNT.store(0, std::sync::atomic::Ordering::SeqCst);

        let desc = desc_for_vec_u32();
        let mut buf = vec![0u8; desc.size];

        // Allocate 4 slots but "fail" before writing any element.
        let data_ptr = unsafe { vox_jit_vec_alloc(&desc as *const _, 4) };
        unsafe {
            write_ptr_field(buf.as_mut_ptr(), desc.ptr_offset, data_ptr);
            write_usize_field(buf.as_mut_ptr(), desc.len_offset, 0);
            write_usize_field(buf.as_mut_ptr(), desc.cap_offset, 4);
        }

        unsafe {
            vox_jit_vec_drop_partial(
                &desc as *const _,
                buf.as_mut_ptr(),
                0,
                Some(counting_drop_glue),
            )
        };

        let count = DROP_COUNT.load(std::sync::atomic::Ordering::SeqCst);
        assert_eq!(count, 0, "no drops expected for init_count=0, got {count}");
    }
}
