//! C bindings for roam shared-memory primitives.
//!
//! Exposes BipBuffer operations, VarSlotPool management, and atomic helpers
//! through a C ABI so that Swift (and other FFI consumers) can use the Rust
//! implementations as the single source of truth.

use core::ffi::c_void;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use roam_shm::layout::SizeClass;
use roam_shm::var_slot_pool::{VarSlotHandle, VarSlotPool};
use shm_primitives::Region;
use shm_primitives::bipbuf::{BIPBUF_HEADER_SIZE, BipBufHeader, BipBufRaw};

// ─── FFI types ──────────────────────────────────────────────────────────────

/// A size class descriptor for variable-size slot pools.
#[repr(C)]
pub struct RoamSizeClass {
    pub slot_size: u32,
    pub count: u32,
}

/// Handle to an allocated variable-size slot.
#[repr(C)]
pub struct RoamVarSlotHandle {
    pub class_idx: u8,
    pub extent_idx: u8,
    pub slot_idx: u32,
    pub generation: u32,
}

/// Opaque wrapper around the Rust VarSlotPool (heap-allocated, Box'd).
pub struct RoamVarSlotPool {
    inner: VarSlotPool,
}

// ─── BipBuf FFI ─────────────────────────────────────────────────────────────
//
// All functions take an opaque `void*` for the BipBuf header. Internally this
// is cast to `*mut BipBufHeader`, which is layout-identical to the C
// `roam_bipbuf_header_t` (128 bytes, same field offsets, cache-line aligned).
//
// The data region is expected to immediately follow the header at +128.

#[unsafe(no_mangle)]
pub extern "C" fn roam_bipbuf_header_size() -> u32 {
    BIPBUF_HEADER_SIZE as u32
}

/// Initialize a BipBuffer header. The caller must provide a zeroed 128-byte
/// region at `header_ptr` followed by `capacity` bytes of data space.
///
/// # Safety
///
/// `header_ptr` must point to a valid, zeroed, 128-byte-aligned region followed
/// by at least `capacity` bytes of writable data space.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn roam_bipbuf_init(header_ptr: *mut c_void, capacity: u32) {
    let header = header_ptr as *mut BipBufHeader;
    unsafe { (*header).init(capacity) };
}

/// # Safety
///
/// `header_ptr` must point to a valid, initialized `BipBufHeader`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn roam_bipbuf_capacity(header_ptr: *const c_void) -> u32 {
    let header = header_ptr as *const BipBufHeader;
    unsafe { (*header).capacity }
}

/// # Safety
///
/// `header_ptr` must point to a valid, initialized `BipBufHeader`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn roam_bipbuf_load_write_acquire(header_ptr: *const c_void) -> u32 {
    let header = header_ptr as *const BipBufHeader;
    unsafe { (*header).write.load(Ordering::Acquire) }
}

/// # Safety
///
/// `header_ptr` must point to a valid, initialized `BipBufHeader`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn roam_bipbuf_load_read_acquire(header_ptr: *const c_void) -> u32 {
    let header = header_ptr as *const BipBufHeader;
    unsafe { (*header).read.load(Ordering::Acquire) }
}

/// # Safety
///
/// `header_ptr` must point to a valid, initialized `BipBufHeader`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn roam_bipbuf_load_watermark_acquire(header_ptr: *const c_void) -> u32 {
    let header = header_ptr as *const BipBufHeader;
    unsafe { (*header).watermark.load(Ordering::Acquire) }
}

/// Try to reserve `len` bytes for writing.
///
/// Returns 1 on success (offset written to `*out_offset`),
/// 0 if there isn't enough contiguous space (would block),
/// -1 if `len` exceeds the buffer capacity (error).
///
/// # Safety
///
/// - `header_ptr` must point to a valid, initialized `BipBufHeader` followed by
///   its data region.
/// - `out_offset` must be non-null and writable.
/// - Only one writer may call this concurrently.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn roam_bipbuf_try_grant(
    header_ptr: *mut c_void,
    len: u32,
    out_offset: *mut u32,
) -> i32 {
    let header = header_ptr as *mut BipBufHeader;

    if len == 0 {
        unsafe { *out_offset = 0 };
        return 1;
    }

    let capacity = unsafe { (*header).capacity };
    if len > capacity {
        return -1;
    }

    let data = unsafe { (header as *mut u8).add(BIPBUF_HEADER_SIZE) };
    let raw = unsafe { BipBufRaw::from_raw(header, data) };

    match raw.try_grant(len) {
        Some(slice) => {
            let offset = unsafe { slice.as_ptr().offset_from(data) } as u32;
            unsafe { *out_offset = offset };
            1
        }
        None => 0,
    }
}

/// Commit `len` previously granted bytes, making them visible to the consumer.
///
/// Returns 0 on success, -1 on overflow.
///
/// # Safety
///
/// - `header_ptr` must point to a valid, initialized `BipBufHeader`.
/// - `len` must not exceed the previously granted region.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn roam_bipbuf_commit(header_ptr: *mut c_void, len: u32) -> i32 {
    let header = header_ptr as *mut BipBufHeader;
    let write = unsafe { (*header).write.load(Ordering::Relaxed) };

    match write.checked_add(len) {
        Some(new_write) if new_write <= unsafe { (*header).capacity } => {
            unsafe { (*header).write.store(new_write, Ordering::Release) };
            0
        }
        _ => -1,
    }
}

/// Try to read contiguous bytes from the buffer.
///
/// On success, writes the readable region's offset and length to the out
/// pointers and returns 1. Returns 0 if the buffer is empty.
///
/// # Safety
///
/// - `header_ptr` must point to a valid, initialized `BipBufHeader` followed by
///   its data region.
/// - `out_offset` and `out_len` must be non-null and writable.
/// - Only one reader may call this concurrently.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn roam_bipbuf_try_read(
    header_ptr: *mut c_void,
    out_offset: *mut u32,
    out_len: *mut u32,
) -> i32 {
    let header = header_ptr as *mut BipBufHeader;
    let data = unsafe { (header as *mut u8).add(BIPBUF_HEADER_SIZE) };
    let raw = unsafe { BipBufRaw::from_raw(header, data) };

    match raw.try_read() {
        Some(slice) => {
            let offset = unsafe { slice.as_ptr().offset_from(data as *const u8) } as u32;
            unsafe {
                *out_offset = offset;
                *out_len = slice.len() as u32;
            }
            1
        }
        None => 0,
    }
}

/// Release `len` bytes from the consumer side.
///
/// Returns 0 on success, -1 on overflow.
///
/// # Safety
///
/// - `header_ptr` must point to a valid, initialized `BipBufHeader`.
/// - `len` must not exceed the previously read region.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn roam_bipbuf_release(header_ptr: *mut c_void, len: u32) -> i32 {
    let header = header_ptr as *mut BipBufHeader;
    let read = unsafe { (*header).read.load(Ordering::Relaxed) };

    match read.checked_add(len) {
        Some(new_read) if new_read <= unsafe { (*header).capacity } => {
            let watermark = unsafe { (*header).watermark.load(Ordering::Acquire) };
            if watermark != 0 && new_read >= watermark {
                unsafe {
                    (*header).read.store(0, Ordering::Release);
                    (*header).watermark.store(0, Ordering::Release);
                }
            } else {
                unsafe { (*header).read.store(new_read, Ordering::Release) };
            }
            0
        }
        _ => -1,
    }
}

// ─── VarSlotPool FFI ────────────────────────────────────────────────────────
//
// The pool is heap-allocated (Box'd) and returned as an opaque pointer.
// The caller passes a raw pointer + length for the shared memory region.

/// Create a VarSlotPool view over an existing shared memory region.
///
/// Does NOT initialize the pool — call `roam_var_slot_pool_init` for that.
/// Returns a heap-allocated opaque handle, or null on failure.
///
/// # Safety
///
/// - `region_ptr` must point to a valid shared-memory region of at least
///   `region_len` bytes, and must remain valid for the lifetime of the pool.
/// - `classes` must point to a valid array of `num_classes` `RoamSizeClass` entries.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn roam_var_slot_pool_attach(
    region_ptr: *mut u8,
    region_len: usize,
    base_offset: u64,
    classes: *const RoamSizeClass,
    num_classes: usize,
) -> *mut RoamVarSlotPool {
    if region_ptr.is_null() || classes.is_null() || num_classes == 0 {
        return core::ptr::null_mut();
    }

    let region = unsafe { Region::from_raw(region_ptr, region_len) };
    let ffi_classes = unsafe { core::slice::from_raw_parts(classes, num_classes) };
    let size_classes: Vec<SizeClass> = ffi_classes
        .iter()
        .map(|c| SizeClass::new(c.slot_size, c.count))
        .collect();

    let pool = VarSlotPool::new(region, base_offset, size_classes);
    Box::into_raw(Box::new(RoamVarSlotPool { inner: pool }))
}

/// Initialize all extent-0 slots and free lists. Call once during segment creation.
///
/// # Safety
///
/// `pool` must be a valid pointer returned by `roam_var_slot_pool_attach`.
/// The underlying region must be writable and large enough for the configured
/// size classes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn roam_var_slot_pool_init(pool: *mut RoamVarSlotPool) {
    let pool = unsafe { &mut *pool };
    unsafe { pool.inner.init() };
}

/// Update the region pointer after a resize/remap.
///
/// # Safety
///
/// - `pool` must be a valid pointer returned by `roam_var_slot_pool_attach`.
/// - `region_ptr` must point to a valid region of at least `region_len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn roam_var_slot_pool_update_region(
    pool: *mut RoamVarSlotPool,
    region_ptr: *mut u8,
    region_len: usize,
) {
    let pool = unsafe { &mut *pool };
    let region = unsafe { Region::from_raw(region_ptr, region_len) };
    pool.inner.update_region(region);
}

/// Allocate a slot that can hold `size` bytes.
///
/// Returns 1 on success (handle written to `*out_handle`), 0 if exhausted.
///
/// # Safety
///
/// - `pool` must be a valid pointer returned by `roam_var_slot_pool_attach`.
/// - `out_handle` must be non-null and writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn roam_var_slot_pool_alloc(
    pool: *const RoamVarSlotPool,
    size: u32,
    owner: u8,
    out_handle: *mut RoamVarSlotHandle,
) -> i32 {
    let pool = unsafe { &*pool };
    match pool.inner.alloc(size, owner) {
        Some(h) => {
            unsafe {
                *out_handle = RoamVarSlotHandle {
                    class_idx: h.class_idx,
                    extent_idx: h.extent_idx,
                    slot_idx: h.slot_idx,
                    generation: h.generation,
                };
            }
            1
        }
        None => 0,
    }
}

/// Transition a slot from Allocated to InFlight.
///
/// Returns 0 on success, -1 on error (generation mismatch or wrong state).
///
/// # Safety
///
/// `pool` must be a valid pointer returned by `roam_var_slot_pool_attach`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn roam_var_slot_pool_mark_in_flight(
    pool: *const RoamVarSlotPool,
    handle: RoamVarSlotHandle,
) -> i32 {
    let pool = unsafe { &*pool };
    let h = to_handle(&handle);
    match pool.inner.mark_in_flight(h) {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

/// Free an in-flight slot back to its pool.
///
/// Returns 0 on success, -1 on error.
///
/// # Safety
///
/// `pool` must be a valid pointer returned by `roam_var_slot_pool_attach`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn roam_var_slot_pool_free(
    pool: *const RoamVarSlotPool,
    handle: RoamVarSlotHandle,
) -> i32 {
    let pool = unsafe { &*pool };
    let h = to_handle(&handle);
    match pool.inner.free(h) {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

/// Free an allocated (never sent) slot back to its pool.
///
/// Returns 0 on success, -1 on error.
///
/// # Safety
///
/// `pool` must be a valid pointer returned by `roam_var_slot_pool_attach`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn roam_var_slot_pool_free_allocated(
    pool: *const RoamVarSlotPool,
    handle: RoamVarSlotHandle,
) -> i32 {
    let pool = unsafe { &*pool };
    let h = to_handle(&handle);
    match pool.inner.free_allocated(h) {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

/// Get a pointer to the slot's payload data area.
///
/// Returns null if the handle is invalid.
///
/// # Safety
///
/// `pool` must be a valid pointer returned by `roam_var_slot_pool_attach`.
/// The returned pointer is only valid while the pool and its region remain alive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn roam_var_slot_pool_payload_ptr(
    pool: *const RoamVarSlotPool,
    handle: RoamVarSlotHandle,
) -> *mut u8 {
    let pool = unsafe { &*pool };
    let h = to_handle(&handle);
    pool.inner.payload_ptr(h).unwrap_or(core::ptr::null_mut())
}

/// Get the current state of a slot.
///
/// Returns 0 = Free, 1 = Allocated, 2 = InFlight, -1 = invalid handle.
///
/// # Safety
///
/// `pool` must be a valid pointer returned by `roam_var_slot_pool_attach`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn roam_var_slot_pool_slot_state(
    pool: *const RoamVarSlotPool,
    handle: RoamVarSlotHandle,
) -> i32 {
    let pool = unsafe { &*pool };
    let h = to_handle(&handle);
    match pool.inner.slot_state(&h) {
        Some(state) => state as i32,
        None => -1,
    }
}

/// Get the slot size for a given class index.
///
/// Returns 0 if the class index is out of range.
///
/// # Safety
///
/// `pool` must be a valid pointer returned by `roam_var_slot_pool_attach`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn roam_var_slot_pool_slot_size(
    pool: *const RoamVarSlotPool,
    class_idx: u8,
) -> u32 {
    let pool = unsafe { &*pool };
    pool.inner.slot_size(class_idx).unwrap_or(0)
}

/// Recover all slots owned by a crashed peer.
///
/// # Safety
///
/// `pool` must be a valid pointer returned by `roam_var_slot_pool_attach`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn roam_var_slot_pool_recover_peer(
    pool: *const RoamVarSlotPool,
    peer_id: u8,
) {
    let pool = unsafe { &*pool };
    pool.inner.recover_peer(peer_id);
}

/// Calculate the total size needed for a variable slot pool (extent 0 only).
///
/// # Safety
///
/// `classes` must point to a valid array of `num_classes` `RoamSizeClass` entries.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn roam_var_slot_pool_calculate_size(
    classes: *const RoamSizeClass,
    num_classes: usize,
) -> u64 {
    if classes.is_null() || num_classes == 0 {
        return 0;
    }
    let ffi_classes = unsafe { core::slice::from_raw_parts(classes, num_classes) };
    let size_classes: Vec<SizeClass> = ffi_classes
        .iter()
        .map(|c| SizeClass::new(c.slot_size, c.count))
        .collect();
    VarSlotPool::calculate_size(&size_classes)
}

/// Destroy a VarSlotPool, freeing its heap allocation.
///
/// # Safety
///
/// `pool` must be either null or a valid pointer previously returned by
/// `roam_var_slot_pool_attach`. After this call, `pool` is dangling and must
/// not be used again.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn roam_var_slot_pool_destroy(pool: *mut RoamVarSlotPool) {
    if !pool.is_null() {
        drop(unsafe { Box::from_raw(pool) });
    }
}

// ─── Atomic helpers ─────────────────────────────────────────────────────────
//
// Thin wrappers so Swift can perform atomic operations on shared memory
// without needing C stdatomic interop.

/// # Safety
///
/// `ptr` must point to a naturally-aligned `u32` in valid shared memory.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn roam_atomic_load_u32_acquire(ptr: *const u32) -> u32 {
    let a = ptr as *const AtomicU32;
    unsafe { (*a).load(Ordering::Acquire) }
}

/// # Safety
///
/// `ptr` must point to a naturally-aligned `u32` in valid shared memory.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn roam_atomic_store_u32_release(ptr: *mut u32, value: u32) {
    let a = ptr as *const AtomicU32;
    unsafe { (*a).store(value, Ordering::Release) };
}

/// # Safety
///
/// - `ptr` must point to a naturally-aligned `u32` in valid shared memory.
/// - `expected` must be non-null and writable (updated with the actual value on
///   failure).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn roam_atomic_compare_exchange_u32(
    ptr: *mut u32,
    expected: *mut u32,
    desired: u32,
) -> i32 {
    let a = ptr as *const AtomicU32;
    let exp = unsafe { *expected };
    match unsafe { (*a).compare_exchange_weak(exp, desired, Ordering::AcqRel, Ordering::Acquire) } {
        Ok(_) => 1,
        Err(actual) => {
            unsafe { *expected = actual };
            0
        }
    }
}

/// # Safety
///
/// `ptr` must point to a naturally-aligned `u32` in valid shared memory.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn roam_atomic_fetch_add_u32(ptr: *mut u32, value: u32) -> u32 {
    let a = ptr as *const AtomicU32;
    unsafe { (*a).fetch_add(value, Ordering::AcqRel) }
}

/// # Safety
///
/// `ptr` must point to a naturally-aligned `u64` in valid shared memory.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn roam_atomic_load_u64_acquire(ptr: *const u64) -> u64 {
    let a = ptr as *const AtomicU64;
    unsafe { (*a).load(Ordering::Acquire) }
}

/// # Safety
///
/// `ptr` must point to a naturally-aligned `u64` in valid shared memory.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn roam_atomic_store_u64_release(ptr: *mut u64, value: u64) {
    let a = ptr as *const AtomicU64;
    unsafe { (*a).store(value, Ordering::Release) };
}

/// # Safety
///
/// - `ptr` must point to a naturally-aligned `u64` in valid shared memory.
/// - `expected` must be non-null and writable (updated with the actual value on
///   failure).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn roam_atomic_compare_exchange_u64(
    ptr: *mut u64,
    expected: *mut u64,
    desired: u64,
) -> i32 {
    let a = ptr as *const AtomicU64;
    let exp = unsafe { *expected };
    match unsafe { (*a).compare_exchange_weak(exp, desired, Ordering::AcqRel, Ordering::Acquire) } {
        Ok(_) => 1,
        Err(actual) => {
            unsafe { *expected = actual };
            0
        }
    }
}

// ─── Internal helpers ───────────────────────────────────────────────────────

fn to_handle(ffi: &RoamVarSlotHandle) -> VarSlotHandle {
    VarSlotHandle {
        class_idx: ffi.class_idx,
        extent_idx: ffi.extent_idx,
        slot_idx: ffi.slot_idx,
        generation: ffi.generation,
    }
}
