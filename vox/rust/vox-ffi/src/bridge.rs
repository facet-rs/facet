//! C-ABI bridge for Swift ↔ Rust in-process vox links.
//!
//! Each side owns a receive mailbox. The peer sends into it via a vtable.
//!
//! ## Ownership
//!
//! `vox_ffi_link_create` takes a Swift-provided vtable (for Rust→Swift delivery)
//! and returns an opaque handle. The handle contains:
//!
//! - The Rust rx mailbox (frames from Swift→Rust)
//! - The Swift vtable (for Rust→Swift tx)
//!
//! The caller retrieves the Rust-rx vtable via `vox_ffi_link_rust_vtable` and
//! passes it to Swift so Swift can send frames into Rust.
//!
//! On the Rust side, call `vox_ffi_link_take_link` to extract the vox `Link`
//! for use with a vox session. This can only be called once per handle.

use std::collections::VecDeque;
use std::future::poll_fn;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::task::Waker;

use vox_types::{Backing, Link, LinkRx, LinkTx, LinkTxPermit, WriteSlot};

use crate::{FfiReleaseFn, FfiVtable};

// ---------------------------------------------------------------------------
// Mailbox — receives frames from the foreign side
// ---------------------------------------------------------------------------

struct Mailbox {
    queue: Mutex<VecDeque<Vec<u8>>>,
    waker: Mutex<Option<Waker>>,
    closed: AtomicBool,
}

impl Mailbox {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            queue: Mutex::new(VecDeque::new()),
            waker: Mutex::new(None),
            closed: AtomicBool::new(false),
        })
    }

    fn push(&self, data: Vec<u8>) {
        self.queue.lock().unwrap().push_back(data);
        if let Some(w) = self.waker.lock().unwrap().take() {
            w.wake();
        }
    }

    fn close(&self) {
        self.closed.store(true, Ordering::Release);
        if let Some(w) = self.waker.lock().unwrap().take() {
            w.wake();
        }
    }
}

// ---------------------------------------------------------------------------
// C callbacks that Swift calls to send frames into Rust's mailbox
// ---------------------------------------------------------------------------

/// C callback: Swift sends a frame into the Rust mailbox.
///
/// Copies `buf[..len]` into Rust-owned memory, then immediately calls
/// `release(release_ctx)` to free the Swift side's buffer.
unsafe extern "C" fn rust_rx_recv_fn(
    ctx: *mut (),
    buf: *const u8,
    len: usize,
    release: FfiReleaseFn,
    release_ctx: *mut (),
) {
    let mailbox = unsafe { &*(ctx as *const Mailbox) };
    let data = unsafe { std::slice::from_raw_parts(buf, len) }.to_vec();
    // Release Swift's buffer immediately since we copied
    unsafe { release(release_ctx) };
    mailbox.push(data);
}

/// C callback: Swift is closing its send direction.
unsafe extern "C" fn rust_rx_drop_fn(ctx: *mut ()) {
    let mailbox = unsafe { Arc::from_raw(ctx as *const Mailbox) };
    mailbox.close();
    // Arc drops here, but Rust rx side also holds a clone
}

// ---------------------------------------------------------------------------
// Bridge handle
// ---------------------------------------------------------------------------

/// Opaque handle returned to Swift from `vox_ffi_link_create`.
pub struct VoxFfiBridgeHandle {
    /// Rust's receive mailbox (Swift sends into this via the rust_rx vtable)
    rust_rx: Arc<Mailbox>,
    /// Swift's receive vtable (Rust sends into this)
    swift_vtable: Option<FfiVtable>,
    /// The vox Link, available until taken via `vox_ffi_link_take_link`
    link: Option<BridgeLink>,
}

// ---------------------------------------------------------------------------
// BridgeLink — implements vox Link for use with vox sessions
// ---------------------------------------------------------------------------

pub struct BridgeLink {
    rust_rx: Arc<Mailbox>,
    swift_vtable: *const FfiVtable,
}

// SAFETY: The swift_vtable pointer is valid for the lifetime of the handle,
// and we only access it from the vox session's task.
unsafe impl Send for BridgeLink {}
unsafe impl Sync for BridgeLink {}

pub struct BridgeLinkTx {
    swift_vtable: *const FfiVtable,
}

unsafe impl Send for BridgeLinkTx {}
unsafe impl Sync for BridgeLinkTx {}

pub struct BridgeLinkRx {
    mailbox: Arc<Mailbox>,
}

pub struct BridgeTxPermit {
    swift_vtable: *const FfiVtable,
}

unsafe impl Send for BridgeTxPermit {}

pub struct BridgeWriteSlot {
    buf: Vec<u8>,
    swift_vtable: *const FfiVtable,
}

impl Link for BridgeLink {
    type Tx = BridgeLinkTx;
    type Rx = BridgeLinkRx;

    fn split(self) -> (Self::Tx, Self::Rx) {
        (
            BridgeLinkTx {
                swift_vtable: self.swift_vtable,
            },
            BridgeLinkRx {
                mailbox: self.rust_rx,
            },
        )
    }
}

impl LinkTx for BridgeLinkTx {
    type Permit = BridgeTxPermit;

    async fn reserve(&self) -> std::io::Result<Self::Permit> {
        // No backpressure for now — always ready
        Ok(BridgeTxPermit {
            swift_vtable: self.swift_vtable,
        })
    }

    async fn close(self) -> std::io::Result<()> {
        // Don't drop the vtable here — the handle owns it
        Ok(())
    }
}

impl LinkTxPermit for BridgeTxPermit {
    type Slot = BridgeWriteSlot;

    fn alloc(self, len: usize) -> std::io::Result<Self::Slot> {
        Ok(BridgeWriteSlot {
            buf: vec![0u8; len],
            swift_vtable: self.swift_vtable,
        })
    }
}

impl WriteSlot for BridgeWriteSlot {
    fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.buf
    }

    fn commit(self) {
        let vtable = unsafe { &*self.swift_vtable };
        // We pass a no-op release since Swift will copy the data anyway
        unsafe {
            (vtable.recv_fn)(
                vtable.ctx,
                self.buf.as_ptr(),
                self.buf.len(),
                noop_release,
                std::ptr::null_mut(),
            );
        }
    }
}

unsafe extern "C" fn noop_release(_ctx: *mut ()) {}

#[derive(Debug)]
pub struct BridgeLinkRxError;

impl std::fmt::Display for BridgeLinkRxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "bridge link closed")
    }
}

impl std::error::Error for BridgeLinkRxError {}

impl LinkRx for BridgeLinkRx {
    type Error = BridgeLinkRxError;

    async fn recv(&mut self) -> Result<Option<Backing>, Self::Error> {
        poll_fn(|cx| {
            if let Some(data) = self.mailbox.queue.lock().unwrap().pop_front() {
                return std::task::Poll::Ready(Ok(Some(Backing::Boxed(data.into_boxed_slice()))));
            }
            if self.mailbox.closed.load(Ordering::Acquire) {
                return std::task::Poll::Ready(Ok(None));
            }
            *self.mailbox.waker.lock().unwrap() = Some(cx.waker().clone());
            if let Some(data) = self.mailbox.queue.lock().unwrap().pop_front() {
                return std::task::Poll::Ready(Ok(Some(Backing::Boxed(data.into_boxed_slice()))));
            }
            if self.mailbox.closed.load(Ordering::Acquire) {
                return std::task::Poll::Ready(Ok(None));
            }
            std::task::Poll::Pending
        })
        .await
    }
}

// ---------------------------------------------------------------------------
// C-ABI exports
// ---------------------------------------------------------------------------

/// Create a bridge link.
///
/// `swift_vtable` is the vtable Rust will use to send frames TO Swift.
/// Rust takes ownership of the vtable (will call `drop_fn` on destroy).
///
/// Returns an opaque handle. The caller must:
/// 1. Call `vox_ffi_link_rust_vtable` to get the vtable for Swift→Rust.
/// 2. On the Rust side, call `vox_ffi_link_take_link` to get the vox Link.
/// 3. Call `vox_ffi_link_destroy` when done.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_ffi_link_create(swift_vtable: FfiVtable) -> *mut VoxFfiBridgeHandle {
    let rust_rx = Mailbox::new();

    // Create the BridgeLink — it borrows the swift_vtable pointer,
    // which will live inside the handle.
    let handle = Box::new(VoxFfiBridgeHandle {
        rust_rx: Arc::clone(&rust_rx),
        swift_vtable: Some(swift_vtable),
        link: None, // set after we have a stable pointer to the vtable
    });

    let handle_ptr = Box::into_raw(handle);

    // Now that the handle is at a stable address, create the BridgeLink
    // pointing at the vtable inside the handle.
    let vtable_ptr = unsafe { (*handle_ptr).swift_vtable.as_ref().unwrap() as *const FfiVtable };
    unsafe {
        (*handle_ptr).link = Some(BridgeLink {
            rust_rx,
            swift_vtable: vtable_ptr,
        });
    }

    handle_ptr
}

/// Get the vtable that Swift should use to send frames into Rust.
///
/// The returned vtable is valid until `vox_ffi_link_destroy` is called.
/// Swift must NOT free or drop this — it points into the handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_ffi_link_rust_vtable(handle: *mut VoxFfiBridgeHandle) -> FfiVtable {
    let handle = unsafe { &*handle };
    let mailbox_ptr = Arc::into_raw(Arc::clone(&handle.rust_rx));

    FfiVtable {
        ctx: mailbox_ptr as *mut (),
        recv_fn: rust_rx_recv_fn,
        drop_fn: rust_rx_drop_fn,
    }
}

/// Extract the vox `Link` from the handle for use with a vox session.
///
/// Can only be called once. Returns null if already taken.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_ffi_link_take_link(
    handle: *mut VoxFfiBridgeHandle,
) -> *mut BridgeLink {
    let handle = unsafe { &mut *handle };
    match handle.link.take() {
        Some(link) => Box::into_raw(Box::new(link)),
        None => std::ptr::null_mut(),
    }
}

/// Destroy the bridge handle.
///
/// This drops the Swift vtable (calling its `drop_fn`) and closes
/// the Rust rx mailbox.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vox_ffi_link_destroy(handle: *mut VoxFfiBridgeHandle) {
    if !handle.is_null() {
        let handle = unsafe { Box::from_raw(handle) };
        // Closing the mailbox wakes any pending Rust recv
        handle.rust_rx.close();
        // Drop of swift_vtable calls drop_fn
        drop(handle);
    }
}
