//! C bindings for vox shared-memory primitives.
//!
//! Exposes BipBuffer operations, VarSlotPool management, and atomic helpers
//! through a C ABI so that Swift (and other FFI consumers) can use the Rust
//! implementations as the single source of truth.
//!
//! This crate is Unix-only; on other platforms it compiles to an empty library.
#![cfg(unix)]

use core::ffi::c_void;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::collections::HashMap;
use std::sync::Mutex;

use shm_primitives::MmapRegion;
use shm_primitives::Region;
use shm_primitives::bipbuf::{BIPBUF_HEADER_SIZE, BipBufHeader, BipBufRaw};
use shm_primitives::bootstrap::{
    self, BOOTSTRAP_REQUEST_HEADER_LEN, BOOTSTRAP_RESPONSE_HEADER_LEN, BootstrapStatus,
};
use shm_primitives::{SizeClassConfig, SlotRef, VarSlotPool};
use std::io::ErrorKind;
use std::io::{self, Error};
use std::os::fd::{FromRawFd, IntoRawFd, OwnedFd, RawFd};

// ─── FFI types ──────────────────────────────────────────────────────────────

/// A size class descriptor for variable-size slot pools.
#[repr(C)]
pub struct VoxSizeClass {
    pub slot_size: u32,
    pub count: u32,
}

/// Handle to an allocated variable-size slot.
#[repr(C)]
pub struct VoxVarSlotHandle {
    pub class_idx: u8,
    pub extent_idx: u8,
    pub slot_idx: u32,
    pub generation: u32,
}

/// Key: (class_idx, extent_idx, slot_idx). Value: (generation, refcount, state).
type SlotStateMap = Mutex<HashMap<(u8, u8, u32), (u32, i32, u8)>>;

/// Opaque wrapper around the Rust VarSlotPool (heap-allocated, Box'd).
pub struct VoxVarSlotPool {
    inner: VarSlotPool,
    region_ptr: *mut u8,
    region_len: usize,
    base_offset: usize,
    configs: Vec<SizeClassConfig>,
    states: SlotStateMap,
}

#[cfg(unix)]
struct VoxMmapAttachment {
    region: MmapRegion,
}

/// Guest-side mmap attachments resolved by (map_id, map_generation).
pub struct VoxMmapAttachments {
    control_fd: RawFd,
    mappings: Mutex<HashMap<(u32, u32), VoxMmapAttachment>>,
}

// ─── BipBuf FFI ─────────────────────────────────────────────────────────────
//
// All functions take an opaque `void*` for the BipBuf header. Internally this
// is cast to `*mut BipBufHeader`, which is layout-identical to the C
// `vox_bipbuf_header_t` (128 bytes, same field offsets, cache-line aligned).
//
// The data region is expected to immediately follow the header at +128.

#[unsafe(no_mangle)]
pub extern "C" fn vox_bipbuf_header_size() -> u32 {
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
pub unsafe extern "C" fn vox_bipbuf_init(header_ptr: *mut c_void, capacity: u32) {
    let header = header_ptr as *mut BipBufHeader;
    unsafe { (*header).init(capacity) };
}

/// # Safety
///
/// `header_ptr` must point to a valid, initialized `BipBufHeader`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_bipbuf_capacity(header_ptr: *const c_void) -> u32 {
    let header = header_ptr as *const BipBufHeader;
    unsafe { (*header).capacity }
}

/// # Safety
///
/// `header_ptr` must point to a valid, initialized `BipBufHeader`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_bipbuf_load_write_acquire(header_ptr: *const c_void) -> u32 {
    let header = header_ptr as *const BipBufHeader;
    unsafe { (*header).write.load(Ordering::Acquire) }
}

/// # Safety
///
/// `header_ptr` must point to a valid, initialized `BipBufHeader`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_bipbuf_load_read_acquire(header_ptr: *const c_void) -> u32 {
    let header = header_ptr as *const BipBufHeader;
    unsafe { (*header).read.load(Ordering::Acquire) }
}

/// # Safety
///
/// `header_ptr` must point to a valid, initialized `BipBufHeader`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_bipbuf_load_watermark_acquire(header_ptr: *const c_void) -> u32 {
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
pub unsafe extern "C" fn vox_bipbuf_try_grant(
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
pub unsafe extern "C" fn vox_bipbuf_commit(header_ptr: *mut c_void, len: u32) -> i32 {
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
pub unsafe extern "C" fn vox_bipbuf_try_read(
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
pub unsafe extern "C" fn vox_bipbuf_release(header_ptr: *mut c_void, len: u32) -> i32 {
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
/// Does NOT initialize the pool — call `vox_var_slot_pool_init` for that.
/// Returns a heap-allocated opaque handle, or null on failure.
///
/// # Safety
///
/// - `region_ptr` must point to a valid shared-memory region of at least
///   `region_len` bytes, and must remain valid for the lifetime of the pool.
/// - `classes` must point to a valid array of `num_classes` `VoxSizeClass` entries.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_var_slot_pool_attach(
    region_ptr: *mut u8,
    region_len: usize,
    base_offset: u64,
    classes: *const VoxSizeClass,
    num_classes: usize,
) -> *mut VoxVarSlotPool {
    if region_ptr.is_null() || classes.is_null() || num_classes == 0 {
        return core::ptr::null_mut();
    }

    let region = unsafe { Region::from_raw(region_ptr, region_len) };
    let ffi_classes = unsafe { core::slice::from_raw_parts(classes, num_classes) };
    let size_classes: Vec<SizeClassConfig> = ffi_classes
        .iter()
        .map(|c| SizeClassConfig {
            slot_size: c.slot_size,
            slot_count: c.count,
        })
        .collect();

    let base_offset = base_offset as usize;
    let pool = unsafe { VarSlotPool::attach(region, base_offset, &size_classes) };
    Box::into_raw(Box::new(VoxVarSlotPool {
        inner: pool,
        region_ptr,
        region_len,
        base_offset,
        configs: size_classes,
        states: Mutex::new(HashMap::new()),
    }))
}

/// Initialize all extent-0 slots and free lists. Call once during segment creation.
///
/// # Safety
///
/// `pool` must be a valid pointer returned by `vox_var_slot_pool_attach`.
/// The underlying region must be writable and large enough for the configured
/// size classes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_var_slot_pool_init(pool: *mut VoxVarSlotPool) {
    let pool = unsafe { &mut *pool };
    let region = unsafe { Region::from_raw(pool.region_ptr, pool.region_len) };
    pool.inner = unsafe { VarSlotPool::init(region, pool.base_offset, &pool.configs) };
    if let Ok(mut states) = pool.states.lock() {
        states.clear();
    }
}

/// Update the region pointer after a resize/remap.
///
/// # Safety
///
/// - `pool` must be a valid pointer returned by `vox_var_slot_pool_attach`.
/// - `region_ptr` must point to a valid region of at least `region_len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_var_slot_pool_update_region(
    pool: *mut VoxVarSlotPool,
    region_ptr: *mut u8,
    region_len: usize,
) {
    let pool = unsafe { &mut *pool };
    pool.region_ptr = region_ptr;
    pool.region_len = region_len;
    let region = unsafe { Region::from_raw(region_ptr, region_len) };
    pool.inner = unsafe { VarSlotPool::attach(region, pool.base_offset, &pool.configs) };
}

/// Allocate a slot that can hold `size` bytes.
///
/// Returns 1 on success (handle written to `*out_handle`), 0 if exhausted.
///
/// # Safety
///
/// - `pool` must be a valid pointer returned by `vox_var_slot_pool_attach`.
/// - `out_handle` must be non-null and writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_var_slot_pool_alloc(
    pool: *const VoxVarSlotPool,
    size: u32,
    owner: u8,
    out_handle: *mut VoxVarSlotHandle,
) -> i32 {
    let pool = unsafe { &*pool };
    match pool.inner.allocate(size, owner) {
        Some(h) => {
            unsafe {
                *out_handle = VoxVarSlotHandle {
                    class_idx: h.class_idx,
                    extent_idx: h.extent_idx,
                    slot_idx: h.slot_idx,
                    generation: h.generation,
                };
            }
            if let Ok(mut states) = pool.states.lock() {
                states.insert(
                    (h.class_idx, h.extent_idx, h.slot_idx),
                    (h.generation, 1, owner),
                );
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
/// `pool` must be a valid pointer returned by `vox_var_slot_pool_attach`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_var_slot_pool_mark_in_flight(
    pool: *const VoxVarSlotPool,
    handle: VoxVarSlotHandle,
) -> i32 {
    let pool = unsafe { &*pool };
    if let Ok(mut states) = pool.states.lock()
        && let Some((generation, state, _)) =
            states.get_mut(&(handle.class_idx, handle.extent_idx, handle.slot_idx))
        && *generation == handle.generation
        && *state == 1
    {
        *state = 2;
    }
    0
}

/// Free an in-flight slot back to its pool.
///
/// Returns 0 on success, -1 on error.
///
/// # Safety
///
/// `pool` must be a valid pointer returned by `vox_var_slot_pool_attach`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_var_slot_pool_free(
    pool: *const VoxVarSlotPool,
    handle: VoxVarSlotHandle,
) -> i32 {
    let pool = unsafe { &*pool };
    let h = to_handle(&handle);
    match pool.inner.free(h) {
        Ok(()) => {
            if let Ok(mut states) = pool.states.lock() {
                states.remove(&(handle.class_idx, handle.extent_idx, handle.slot_idx));
            }
            0
        }
        Err(_) => -1,
    }
}

/// Free an allocated (never sent) slot back to its pool.
///
/// Returns 0 on success, -1 on error.
///
/// # Safety
///
/// `pool` must be a valid pointer returned by `vox_var_slot_pool_attach`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_var_slot_pool_free_allocated(
    pool: *const VoxVarSlotPool,
    handle: VoxVarSlotHandle,
) -> i32 {
    unsafe { vox_var_slot_pool_free(pool, handle) }
}

/// Get a pointer to the slot's payload data area.
///
/// Returns null if the handle is invalid.
///
/// # Safety
///
/// `pool` must be a valid pointer returned by `vox_var_slot_pool_attach`.
/// The returned pointer is only valid while the pool and its region remain alive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_var_slot_pool_payload_ptr(
    pool: *const VoxVarSlotPool,
    handle: VoxVarSlotHandle,
) -> *mut u8 {
    let pool = unsafe { &*pool };
    let h = to_handle(&handle);
    if usize::from(h.class_idx) >= pool.inner.class_count() {
        return core::ptr::null_mut();
    }
    unsafe { pool.inner.slot_data_mut(&h) }.as_mut_ptr()
}

/// Get the current state of a slot.
///
/// Returns 0 = Free, 1 = Allocated, 2 = InFlight, -1 = invalid handle.
///
/// # Safety
///
/// `pool` must be a valid pointer returned by `vox_var_slot_pool_attach`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_var_slot_pool_slot_state(
    pool: *const VoxVarSlotPool,
    handle: VoxVarSlotHandle,
) -> i32 {
    let pool = unsafe { &*pool };
    if let Ok(states) = pool.states.lock()
        && let Some((generation, state, _)) =
            states.get(&(handle.class_idx, handle.extent_idx, handle.slot_idx))
    {
        if *generation == handle.generation {
            return *state;
        }
        return -1;
    }
    0
}

/// Get the slot size for a given class index.
///
/// Returns 0 if the class index is out of range.
///
/// # Safety
///
/// `pool` must be a valid pointer returned by `vox_var_slot_pool_attach`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_var_slot_pool_slot_size(
    pool: *const VoxVarSlotPool,
    class_idx: u8,
) -> u32 {
    let pool = unsafe { &*pool };
    let class_idx = usize::from(class_idx);
    if class_idx >= pool.inner.class_count() {
        return 0;
    }
    pool.inner.slot_size(class_idx)
}

/// Recover all slots owned by a crashed peer.
///
/// # Safety
///
/// `pool` must be a valid pointer returned by `vox_var_slot_pool_attach`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_var_slot_pool_recover_peer(pool: *const VoxVarSlotPool, peer_id: u8) {
    let pool = unsafe { &*pool };
    pool.inner.reclaim_peer_slots(peer_id);
    if let Ok(mut states) = pool.states.lock() {
        states.retain(|_, (_, _, owner)| *owner != peer_id);
    }
}

/// Calculate the total size needed for a variable slot pool (extent 0 only).
///
/// # Safety
///
/// `classes` must point to a valid array of `num_classes` `VoxSizeClass` entries.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_var_slot_pool_calculate_size(
    classes: *const VoxSizeClass,
    num_classes: usize,
) -> u64 {
    if classes.is_null() || num_classes == 0 {
        return 0;
    }
    let ffi_classes = unsafe { core::slice::from_raw_parts(classes, num_classes) };
    let size_classes: Vec<SizeClassConfig> = ffi_classes
        .iter()
        .map(|c| SizeClassConfig {
            slot_size: c.slot_size,
            slot_count: c.count,
        })
        .collect();
    VarSlotPool::required_size(&size_classes) as u64
}

/// Destroy a VarSlotPool, freeing its heap allocation.
///
/// # Safety
///
/// `pool` must be either null or a valid pointer previously returned by
/// `vox_var_slot_pool_attach`. After this call, `pool` is dangling and must
/// not be used again.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_var_slot_pool_destroy(pool: *mut VoxVarSlotPool) {
    if !pool.is_null() {
        drop(unsafe { Box::from_raw(pool) });
    }
}

// ─── bootstrap wire FFI ────────────────────────────────────────────────────

#[repr(C)]
pub struct VoxShmBootstrapResponseInfo {
    pub status: u8,
    pub peer_id: u32,
    pub payload_len: u16,
}

/// Encode a bootstrap request frame (`RSH0` + sid length + sid bytes).
///
/// Returns:
/// - 0: success
/// - -1: invalid arguments
/// - -2: output buffer too small
/// - -3: encoding failure
///
/// # Safety
///
/// - `sid_ptr` must point to `sid_len` readable bytes.
/// - `out_buf` must point to `out_buf_len` writable bytes.
/// - `out_written` must be non-null and writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_shm_bootstrap_request_encode(
    sid_ptr: *const u8,
    sid_len: usize,
    out_buf: *mut u8,
    out_buf_len: usize,
    out_written: *mut usize,
) -> i32 {
    if (sid_len > 0 && sid_ptr.is_null()) || out_buf.is_null() || out_written.is_null() {
        return -1;
    }

    let sid = if sid_len == 0 {
        &[][..]
    } else {
        // SAFETY: validated by caller contract and null checks above.
        unsafe { std::slice::from_raw_parts(sid_ptr, sid_len) }
    };
    let frame = match bootstrap::encode_request(sid) {
        Ok(frame) => frame,
        Err(_) => return -3,
    };

    if frame.len() > out_buf_len {
        return -2;
    }

    // SAFETY: output buffer is valid for frame.len() bytes.
    unsafe {
        std::ptr::copy_nonoverlapping(frame.as_ptr(), out_buf, frame.len());
        *out_written = frame.len();
    }
    0
}

/// Decode a bootstrap request frame and expose SID location in the input buffer.
///
/// Returns:
/// - 0: success
/// - -1: invalid arguments
/// - -2: malformed request frame
///
/// # Safety
///
/// - `buf_ptr` must point to `buf_len` readable bytes.
/// - `out_sid_ptr` and `out_sid_len` must be non-null and writable.
/// - `out_sid_ptr` points into `buf_ptr`; keep input alive while using it.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_shm_bootstrap_request_decode(
    buf_ptr: *const u8,
    buf_len: usize,
    out_sid_ptr: *mut *const u8,
    out_sid_len: *mut u16,
) -> i32 {
    if (buf_len > 0 && buf_ptr.is_null()) || out_sid_ptr.is_null() || out_sid_len.is_null() {
        return -1;
    }

    let frame = if buf_len == 0 {
        &[][..]
    } else {
        // SAFETY: validated by caller contract and null checks above.
        unsafe { std::slice::from_raw_parts(buf_ptr, buf_len) }
    };
    let req = match bootstrap::decode_request(frame) {
        Ok(req) => req,
        Err(_) => return -2,
    };

    if req.sid.len() > u16::MAX as usize {
        return -2;
    }

    // SAFETY: output pointers are valid and writable.
    unsafe {
        *out_sid_ptr = req.sid.as_ptr();
        *out_sid_len = req.sid.len() as u16;
    }
    0
}

/// Encode a bootstrap response frame (`RSP0` + status + peer + payload length + payload).
///
/// Returns:
/// - 0: success
/// - -1: invalid arguments
/// - -2: output buffer too small
/// - -3: malformed response fields
///
/// # Safety
///
/// - `payload_ptr` must point to `payload_len` readable bytes when `payload_len > 0`.
/// - `out_buf` must point to `out_buf_len` writable bytes.
/// - `out_written` must be non-null and writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_shm_bootstrap_response_encode(
    status: u8,
    peer_id: u32,
    payload_ptr: *const u8,
    payload_len: usize,
    out_buf: *mut u8,
    out_buf_len: usize,
    out_written: *mut usize,
) -> i32 {
    if (payload_len > 0 && payload_ptr.is_null()) || out_buf.is_null() || out_written.is_null() {
        return -1;
    }

    let status = match BootstrapStatus::try_from(status) {
        Ok(status) => status,
        Err(_) => return -3,
    };
    let payload = if payload_len == 0 {
        &[][..]
    } else {
        // SAFETY: validated by caller contract and null checks above.
        unsafe { std::slice::from_raw_parts(payload_ptr, payload_len) }
    };
    let frame = match bootstrap::encode_response(status, peer_id, payload) {
        Ok(frame) => frame,
        Err(_) => return -3,
    };

    if frame.len() > out_buf_len {
        return -2;
    }

    // SAFETY: output buffer is valid for frame.len() bytes.
    unsafe {
        std::ptr::copy_nonoverlapping(frame.as_ptr(), out_buf, frame.len());
        *out_written = frame.len();
    }
    0
}

/// Decode a bootstrap response frame and expose payload location in the input buffer.
///
/// Returns:
/// - 0: success
/// - -1: invalid arguments
/// - -2: malformed response frame
///
/// # Safety
///
/// - `buf_ptr` must point to `buf_len` readable bytes.
/// - `out_info` and `out_payload_ptr` must be non-null and writable.
/// - `out_payload_ptr` points into `buf_ptr`; keep input alive while using it.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_shm_bootstrap_response_decode(
    buf_ptr: *const u8,
    buf_len: usize,
    out_info: *mut VoxShmBootstrapResponseInfo,
    out_payload_ptr: *mut *const u8,
) -> i32 {
    if (buf_len > 0 && buf_ptr.is_null()) || out_info.is_null() || out_payload_ptr.is_null() {
        return -1;
    }

    let frame = if buf_len == 0 {
        &[][..]
    } else {
        // SAFETY: validated by caller contract and null checks above.
        unsafe { std::slice::from_raw_parts(buf_ptr, buf_len) }
    };
    let resp = match bootstrap::decode_response(frame) {
        Ok(resp) => resp,
        Err(_) => return -2,
    };

    if resp.payload.len() > u16::MAX as usize {
        return -2;
    }

    // SAFETY: output pointers are valid and writable.
    unsafe {
        (*out_info).status = resp.status as u8;
        (*out_info).peer_id = resp.peer_id;
        (*out_info).payload_len = resp.payload.len() as u16;
        *out_payload_ptr = resp.payload.as_ptr();
    }
    0
}

/// Send one bootstrap response on a Unix control socket.
///
/// On success status (`0`), `doorbell_fd` and `segment_fd` are required and
/// `mmap_control_fd` is also required. On error status (`1`), all fd
/// parameters must be `-1`.
///
/// Returns 0 on success, -1 on error.
///
/// # Safety
///
/// - `control_fd` must be a valid Unix-domain socket descriptor.
/// - `payload_ptr` must be valid for `payload_len` bytes when `payload_len > 0`.
/// - Any provided fds must stay valid through the send call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_shm_bootstrap_response_send_unix(
    control_fd: i32,
    status: u8,
    peer_id: u32,
    payload_ptr: *const u8,
    payload_len: usize,
    doorbell_fd: i32,
    segment_fd: i32,
    mmap_control_fd: i32,
) -> i32 {
    #[cfg(unix)]
    {
        if control_fd < 0 || (payload_len > 0 && payload_ptr.is_null()) {
            return -1;
        }

        let status = match BootstrapStatus::try_from(status) {
            Ok(status) => status,
            Err(_) => return -1,
        };
        let payload = if payload_len == 0 {
            &[][..]
        } else {
            // SAFETY: validated by caller contract and null checks above.
            unsafe { std::slice::from_raw_parts(payload_ptr, payload_len) }
        };

        let result = if status == BootstrapStatus::Success {
            if doorbell_fd < 0 || segment_fd < 0 || mmap_control_fd < 0 {
                return -1;
            }
            let fds = bootstrap::BootstrapSuccessFds {
                doorbell_fd,
                segment_fd,
                mmap_control_fd,
            };
            bootstrap::send_response_unix(control_fd, status, peer_id, payload, Some(&fds))
        } else {
            if doorbell_fd >= 0 || segment_fd >= 0 || mmap_control_fd >= 0 {
                return -1;
            }
            bootstrap::send_response_unix(control_fd, status, peer_id, payload, None)
        };

        if result.is_ok() { 0 } else { -1 }
    }

    #[cfg(not(unix))]
    {
        let _ = (
            control_fd,
            status,
            peer_id,
            payload_ptr,
            payload_len,
            doorbell_fd,
            segment_fd,
            mmap_control_fd,
        );
        -1
    }
}

/// Receive one bootstrap response on a Unix control socket.
///
/// `payload_buf` receives the response payload bytes and returned fds become
/// owned by the caller (`-1` values are used for error responses).
///
/// Returns:
/// - 0: success
/// - -1: invalid arguments / malformed frame / fd mismatch / io error
/// - -2: payload buffer too small
///
/// # Safety
///
/// - `control_fd` must be a valid Unix-domain socket descriptor.
/// - `payload_buf` must be writable for `payload_buf_len` bytes.
/// - output pointers must be non-null and writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_shm_bootstrap_response_recv_unix(
    control_fd: i32,
    payload_buf: *mut u8,
    payload_buf_len: usize,
    out_info: *mut VoxShmBootstrapResponseInfo,
    out_doorbell_fd: *mut i32,
    out_segment_fd: *mut i32,
    out_mmap_control_fd: *mut i32,
) -> i32 {
    #[cfg(unix)]
    {
        if control_fd < 0
            || payload_buf.is_null()
            || out_info.is_null()
            || out_doorbell_fd.is_null()
            || out_segment_fd.is_null()
            || out_mmap_control_fd.is_null()
        {
            return -1;
        }

        let received = match bootstrap::recv_response_unix(control_fd) {
            Ok(received) => received,
            Err(_) => return -1,
        };

        if received.response.payload.len() > payload_buf_len {
            return -2;
        }

        // SAFETY: pointers validated above.
        unsafe {
            std::ptr::copy_nonoverlapping(
                received.response.payload.as_ptr(),
                payload_buf,
                received.response.payload.len(),
            );
            (*out_info).status = received.response.status as u8;
            (*out_info).peer_id = received.response.peer_id;
            (*out_info).payload_len = received.response.payload.len() as u16;
            *out_doorbell_fd = -1;
            *out_segment_fd = -1;
            *out_mmap_control_fd = -1;
        }

        if let Some(fds) = received.fds {
            // SAFETY: pointers validated above.
            unsafe {
                *out_doorbell_fd = fds.doorbell_fd.into_raw_fd();
                *out_segment_fd = fds.segment_fd.into_raw_fd();
                *out_mmap_control_fd = fds.mmap_control_fd.into_raw_fd();
            }
        }

        0
    }

    #[cfg(not(unix))]
    {
        let _ = (
            control_fd,
            payload_buf,
            payload_buf_len,
            out_info,
            out_doorbell_fd,
            out_segment_fd,
            out_mmap_control_fd,
        );
        -1
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn vox_shm_bootstrap_request_header_size() -> u32 {
    BOOTSTRAP_REQUEST_HEADER_LEN as u32
}

#[unsafe(no_mangle)]
pub extern "C" fn vox_shm_bootstrap_response_header_size() -> u32 {
    BOOTSTRAP_RESPONSE_HEADER_LEN as u32
}

// ─── mmap attachment FFI ────────────────────────────────────────────────────

#[cfg(unix)]
#[derive(Clone, Copy)]
#[repr(C)]
struct VoxMmapAttachMessage {
    map_id: u32,
    map_generation: u32,
    mapping_length: u64,
}

#[cfg(unix)]
impl VoxMmapAttachMessage {
    const LEN: usize = 16;

    fn from_le_bytes(buf: [u8; Self::LEN]) -> Self {
        Self {
            map_id: u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]),
            map_generation: u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]),
            mapping_length: u64::from_le_bytes([
                buf[8], buf[9], buf[10], buf[11], buf[12], buf[13], buf[14], buf[15],
            ]),
        }
    }
}

#[cfg(unix)]
fn set_nonblocking(fd: RawFd) -> i32 {
    // SAFETY: fcntl is thread-safe for querying/modifying fd flags.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 {
        return -1;
    }
    // SAFETY: same as above.
    if unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return -1;
    }
    0
}

#[cfg(unix)]
fn recv_mmap_attach(fd: RawFd) -> std::io::Result<Option<(OwnedFd, VoxMmapAttachMessage)>> {
    let mut payload = [0_u8; VoxMmapAttachMessage::LEN];
    let mut iov = libc::iovec {
        iov_base: payload.as_mut_ptr().cast(),
        iov_len: payload.len(),
    };

    let cmsg_space = unsafe { libc::CMSG_SPACE(core::mem::size_of::<RawFd>() as u32) as usize };
    let mut control = vec![0_u8; cmsg_space];

    // SAFETY: zeroed msghdr is valid before field initialization.
    let mut msghdr: libc::msghdr = unsafe { core::mem::zeroed() };
    msghdr.msg_iov = &mut iov;
    msghdr.msg_iovlen = 1;
    msghdr.msg_control = control.as_mut_ptr().cast();
    msghdr.msg_controllen = control.len() as _;

    // SAFETY: msghdr points to valid buffers for recvmsg.
    let n = unsafe { libc::recvmsg(fd, &mut msghdr, 0) };
    if n == 0 {
        return Ok(None);
    }
    if n < 0 {
        let err = std::io::Error::last_os_error();
        if err.kind() == ErrorKind::WouldBlock {
            return Ok(None);
        }
        return Err(err);
    }
    if n < VoxMmapAttachMessage::LEN as isize {
        return Err(std::io::Error::new(
            ErrorKind::InvalidData,
            "short mmap attach payload",
        ));
    }
    if (msghdr.msg_flags & libc::MSG_CTRUNC) != 0 {
        return Err(std::io::Error::new(
            ErrorKind::InvalidData,
            "truncated mmap attach control message",
        ));
    }

    let mut received_fd: Option<RawFd> = None;
    // SAFETY: control buffer ownership is local and valid for traversal.
    unsafe {
        let mut cmsg = libc::CMSG_FIRSTHDR(&msghdr);
        while !cmsg.is_null() {
            if (*cmsg).cmsg_level == libc::SOL_SOCKET && (*cmsg).cmsg_type == libc::SCM_RIGHTS {
                let cmsg_len = (*cmsg).cmsg_len as usize;
                let base_len = libc::CMSG_LEN(0) as usize;
                if cmsg_len >= base_len + core::mem::size_of::<RawFd>() {
                    let data_ptr = libc::CMSG_DATA(cmsg).cast::<RawFd>();
                    received_fd = Some(*data_ptr);
                    break;
                }
            }
            cmsg = libc::CMSG_NXTHDR(&msghdr, cmsg);
        }
    }

    let raw_fd = received_fd.ok_or_else(|| {
        std::io::Error::new(
            ErrorKind::InvalidData,
            "missing mmap attach fd in control message",
        )
    })?;
    // SAFETY: recvmsg gives ownership of the received fd to the receiver.
    let owned_fd = unsafe { OwnedFd::from_raw_fd(raw_fd) };
    Ok(Some((
        owned_fd,
        VoxMmapAttachMessage::from_le_bytes(payload),
    )))
}

#[cfg(unix)]
fn send_mmap_attach(
    control_fd: RawFd,
    mapping_fd: RawFd,
    msg: VoxMmapAttachMessage,
) -> std::io::Result<()> {
    let mut payload = [0_u8; VoxMmapAttachMessage::LEN];
    payload[0..4].copy_from_slice(&msg.map_id.to_le_bytes());
    payload[4..8].copy_from_slice(&msg.map_generation.to_le_bytes());
    payload[8..16].copy_from_slice(&msg.mapping_length.to_le_bytes());
    let mut iov = libc::iovec {
        iov_base: payload.as_ptr() as *mut libc::c_void,
        iov_len: payload.len(),
    };

    let fds = [mapping_fd];
    let cmsg_space = unsafe { libc::CMSG_SPACE(core::mem::size_of_val(&fds) as u32) as usize };
    let mut control = vec![0_u8; cmsg_space];
    let cmsg_len = unsafe { libc::CMSG_LEN(core::mem::size_of_val(&fds) as u32) as usize };

    // SAFETY: zeroed msghdr is valid before field initialization.
    let mut msghdr: libc::msghdr = unsafe { core::mem::zeroed() };
    msghdr.msg_iov = &mut iov;
    msghdr.msg_iovlen = 1;
    msghdr.msg_control = control.as_mut_ptr().cast();
    msghdr.msg_controllen = cmsg_len as _;

    // SAFETY: cmsg buffer belongs to this stack frame and is sized via CMSG_SPACE.
    let cmsg = unsafe { libc::CMSG_FIRSTHDR(&msghdr) };
    if cmsg.is_null() {
        return Err(std::io::Error::new(
            ErrorKind::InvalidData,
            "failed to allocate SCM_RIGHTS header",
        ));
    }

    unsafe {
        (*cmsg).cmsg_level = libc::SOL_SOCKET;
        (*cmsg).cmsg_type = libc::SCM_RIGHTS;
        (*cmsg).cmsg_len = cmsg_len as _;
        let data_ptr = libc::CMSG_DATA(cmsg).cast::<RawFd>();
        core::ptr::copy_nonoverlapping(fds.as_ptr(), data_ptr, 1);
    }

    // SAFETY: msghdr points to valid payload/control data.
    let n = sendmsg_no_sigpipe(control_fd, &msghdr)?;
    if n < 0 {
        return Err(std::io::Error::last_os_error());
    }
    if n == 0 {
        return Err(std::io::Error::new(
            ErrorKind::WriteZero,
            "sendmsg wrote zero bytes for mmap attach",
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn sendmsg_no_sigpipe(fd: RawFd, msghdr: &libc::msghdr) -> io::Result<isize> {
    #[cfg(target_vendor = "apple")]
    ensure_socket_no_sigpipe(fd)?;

    #[cfg(any(target_os = "linux", target_os = "android"))]
    let flags = libc::MSG_NOSIGNAL;
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    let flags = 0;

    // SAFETY: caller guarantees `msghdr` points to valid iov/cmsg buffers.
    let n = unsafe { libc::sendmsg(fd, msghdr, flags) };
    if n < 0 {
        return Err(Error::last_os_error());
    }
    Ok(n)
}

#[cfg(all(unix, target_vendor = "apple"))]
fn ensure_socket_no_sigpipe(fd: RawFd) -> io::Result<()> {
    let one: libc::c_int = 1;
    // SAFETY: setsockopt reads `one` for the provided length.
    let rc = unsafe {
        libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_NOSIGPIPE,
            (&one as *const libc::c_int).cast(),
            core::mem::size_of_val(&one) as libc::socklen_t,
        )
    };
    if rc < 0 {
        return Err(Error::last_os_error());
    }
    Ok(())
}

/// Send one mmap attach message (fd + map metadata) over a Unix control socket.
///
/// Returns 0 on success, -1 on error.
///
/// # Safety
///
/// `control_fd` and `mapping_fd` must be valid, open Unix file descriptors.
/// `control_fd` must refer to a Unix domain socket configured to pass
/// `SCM_RIGHTS`, and ownership/lifetime of `mapping_fd` must outlive the send.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_mmap_control_send(
    control_fd: i32,
    mapping_fd: i32,
    map_id: u32,
    map_generation: u32,
    mapping_length: u64,
) -> i32 {
    #[cfg(unix)]
    {
        if control_fd < 0 || mapping_fd < 0 {
            return -1;
        }
        let msg = VoxMmapAttachMessage {
            map_id,
            map_generation,
            mapping_length,
        };
        if send_mmap_attach(control_fd, mapping_fd, msg).is_ok() {
            0
        } else {
            -1
        }
    }

    #[cfg(not(unix))]
    {
        let _ = (
            control_fd,
            mapping_fd,
            map_id,
            map_generation,
            mapping_length,
        );
        -1
    }
}

/// Create a guest-side mmap attachment registry from a control socket fd.
///
/// Returns null on invalid fd or setup failure.
///
/// # Safety
///
/// `control_fd` must be a valid Unix domain socket file descriptor owned by the
/// caller for at least as long as the returned attachments object lives.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_mmap_attachments_create(control_fd: i32) -> *mut VoxMmapAttachments {
    if control_fd < 0 {
        return core::ptr::null_mut();
    }
    if set_nonblocking(control_fd) != 0 {
        return core::ptr::null_mut();
    }
    Box::into_raw(Box::new(VoxMmapAttachments {
        control_fd,
        mappings: Mutex::new(HashMap::new()),
    }))
}

/// Drain all pending mmap attach messages from the control socket.
///
/// Returns number of mappings attached, or -1 on error.
///
/// # Safety
///
/// `attachments` must be a valid pointer returned by `vox_mmap_attachments_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_mmap_attachments_drain_control(
    attachments: *mut VoxMmapAttachments,
) -> i32 {
    if attachments.is_null() {
        return -1;
    }

    let attachments = unsafe { &*attachments };
    let mut attached_count = 0_i32;

    loop {
        let result = recv_mmap_attach(attachments.control_fd);
        match result {
            Ok(Some((fd, msg))) => {
                let mapping_len = msg.mapping_length as usize;
                let region = match MmapRegion::attach_fd(fd, mapping_len) {
                    Ok(region) => region,
                    Err(_) => return -1,
                };
                let mut mappings = match attachments.mappings.lock() {
                    Ok(mappings) => mappings,
                    Err(_) => return -1,
                };
                mappings.insert(
                    (msg.map_id, msg.map_generation),
                    VoxMmapAttachment { region },
                );
                attached_count += 1;
            }
            Ok(None) => break,
            Err(_) => return -1,
        }
    }

    attached_count
}

/// Resolve an mmap-ref tuple to a direct payload pointer.
///
/// Return codes:
/// - 0: success
/// - -1: invalid arguments / internal error
/// - -2: unknown mapping (map_id, map_generation not attached)
/// - -3: offset+len overflow
/// - -4: out of bounds for mapping length
///
/// # Safety
///
/// - `attachments` must be valid and created by `vox_mmap_attachments_create`.
/// - `out_ptr` must be non-null and writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_mmap_attachments_resolve_ptr(
    attachments: *const VoxMmapAttachments,
    map_id: u32,
    map_generation: u32,
    map_offset: u64,
    payload_len: u32,
    out_ptr: *mut *const u8,
) -> i32 {
    if attachments.is_null() || out_ptr.is_null() {
        return -1;
    }

    let attachments = unsafe { &*attachments };
    let mappings = match attachments.mappings.lock() {
        Ok(mappings) => mappings,
        Err(_) => return -1,
    };
    let Some(mapping) = mappings.get(&(map_id, map_generation)) else {
        return -2;
    };

    let start = map_offset as usize;
    let Some(end) = start.checked_add(payload_len as usize) else {
        return -3;
    };
    if end > mapping.region.len() {
        return -4;
    }

    // SAFETY: bounds checked against mapping length above.
    let ptr = unsafe { mapping.region.region().as_ptr().add(start) } as *const u8;
    unsafe { *out_ptr = ptr };
    0
}

/// Destroy mmap attachments and free all attached mapping resources.
///
/// # Safety
///
/// `attachments` must be null or a valid pointer returned by
/// `vox_mmap_attachments_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_mmap_attachments_destroy(attachments: *mut VoxMmapAttachments) {
    if !attachments.is_null() {
        drop(unsafe { Box::from_raw(attachments) });
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
pub unsafe extern "C" fn vox_atomic_load_u32_acquire(ptr: *const u32) -> u32 {
    let a = ptr as *const AtomicU32;
    unsafe { (*a).load(Ordering::Acquire) }
}

/// # Safety
///
/// `ptr` must point to a naturally-aligned `u32` in valid shared memory.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_atomic_store_u32_release(ptr: *mut u32, value: u32) {
    let a = ptr as *const AtomicU32;
    unsafe { (*a).store(value, Ordering::Release) };
}

/// # Safety
///
/// - `ptr` must point to a naturally-aligned `u32` in valid shared memory.
/// - `expected` must be non-null and writable (updated with the actual value on
///   failure).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_atomic_compare_exchange_u32(
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
pub unsafe extern "C" fn vox_atomic_fetch_add_u32(ptr: *mut u32, value: u32) -> u32 {
    let a = ptr as *const AtomicU32;
    unsafe { (*a).fetch_add(value, Ordering::AcqRel) }
}

/// # Safety
///
/// `ptr` must point to a naturally-aligned `u64` in valid shared memory.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_atomic_load_u64_acquire(ptr: *const u64) -> u64 {
    let a = ptr as *const AtomicU64;
    unsafe { (*a).load(Ordering::Acquire) }
}

/// # Safety
///
/// `ptr` must point to a naturally-aligned `u64` in valid shared memory.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_atomic_store_u64_release(ptr: *mut u64, value: u64) {
    let a = ptr as *const AtomicU64;
    unsafe { (*a).store(value, Ordering::Release) };
}

/// # Safety
///
/// - `ptr` must point to a naturally-aligned `u64` in valid shared memory.
/// - `expected` must be non-null and writable (updated with the actual value on
///   failure).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_atomic_compare_exchange_u64(
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

fn to_handle(ffi: &VoxVarSlotHandle) -> SlotRef {
    SlotRef {
        class_idx: ffi.class_idx,
        extent_idx: ffi.extent_idx,
        slot_idx: ffi.slot_idx,
        generation: ffi.generation,
    }
}
