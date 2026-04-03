#![allow(unsafe_code)]

use std::collections::VecDeque;
use std::future::poll_fn;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::Waker;

use vox_types::{Backing, Link, LinkRx, LinkTx, LinkTxPermit, SharedBacking, WriteSlot};

// ---------------------------------------------------------------------------
// C-ABI types (Swift-compatible)
// ---------------------------------------------------------------------------

/// Called by the receiver to release a loaned buffer back to the sender.
pub type FfiReleaseFn = unsafe extern "C" fn(*mut ());

/// Called by the sender to deliver a message to the receiver.
///
/// - `ctx`: receiver context from [`FfiVtable::ctx`]
/// - `buf`: pointer to message bytes (valid until `release(release_ctx)` is called)
/// - `len`: byte count
/// - `release`: function to call when the receiver is done with the buffer
/// - `release_ctx`: opaque context forwarded to `release`
pub type FfiRecvFn = unsafe extern "C" fn(
    ctx: *mut (),
    buf: *const u8,
    len: usize,
    release: FfiReleaseFn,
    release_ctx: *mut (),
);

/// Called when the vtable itself is destroyed (peer closed).
pub type FfiDropFn = unsafe extern "C" fn(*mut ());

/// C-ABI vtable for one direction of the FFI link.
///
/// The sender holds a `FfiVtable` pointing at the receiver.  When the sender
/// wants to deliver a message it calls `recv_fn`.  When the sender closes
/// permanently it drops the vtable, triggering `drop_fn`.
#[repr(C)]
pub struct FfiVtable {
    pub ctx: *mut (),
    pub recv_fn: FfiRecvFn,
    pub drop_fn: FfiDropFn,
}

// Safety: the ctx pointer and the functions are always accessed in a
// coordinated fashion via the Arc<FfiRxInner> that backs them.
unsafe impl Send for FfiVtable {}
unsafe impl Sync for FfiVtable {}

impl Drop for FfiVtable {
    fn drop(&mut self) {
        unsafe { (self.drop_fn)(self.ctx) }
    }
}

// ---------------------------------------------------------------------------
// In-flight tracker — backpressure for the sender
// ---------------------------------------------------------------------------

struct InFlightTracker {
    count: AtomicUsize,
    max: usize,
    waker: Mutex<Option<Waker>>,
}

impl InFlightTracker {
    fn new(max: usize) -> Arc<Self> {
        Arc::new(Self {
            count: AtomicUsize::new(0),
            max,
            waker: Mutex::new(None),
        })
    }

    /// Try to increment the in-flight count.  Returns `true` on success.
    fn try_acquire(&self) -> bool {
        let mut current = self.count.load(Ordering::Relaxed);
        loop {
            if current >= self.max {
                return false;
            }
            match self.count.compare_exchange_weak(
                current,
                current + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return true,
                Err(actual) => current = actual,
            }
        }
    }

    /// Decrement the in-flight count and wake a pending sender.
    fn release(&self) {
        self.count.fetch_sub(1, Ordering::AcqRel);
        if let Ok(mut guard) = self.waker.lock() {
            if let Some(w) = guard.take() {
                w.wake();
            }
        }
    }
}

// ---------------------------------------------------------------------------
// FfiRxInner — the shared state owned by the receiver side
// ---------------------------------------------------------------------------

struct FfiRxInner {
    queue: Mutex<VecDeque<Arc<FfiFrameBacking>>>,
    waker: Mutex<Option<Waker>>,
    closed: AtomicBool,
}

impl FfiRxInner {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            queue: Mutex::new(VecDeque::new()),
            waker: Mutex::new(None),
            closed: AtomicBool::new(false),
        })
    }

    /// Push a frame and wake the receiver.
    fn push(&self, frame: Arc<FfiFrameBacking>) {
        self.queue.lock().unwrap().push_back(frame);
        if let Ok(mut guard) = self.waker.lock() {
            if let Some(w) = guard.take() {
                w.wake();
            }
        }
    }
}

// ---------------------------------------------------------------------------
// FfiFrameBacking — the loan handle held by the receiver
// ---------------------------------------------------------------------------

/// Zero-copy backing for one received FFI frame.
///
/// Holds an `Arc<[u8]>` (the sender's buffer) and the in-flight tracker.
/// `SharedBacking::as_bytes` borrows the bytes; `Drop` releases the
/// in-flight slot so the sender can proceed.
struct FfiFrameBacking {
    data: Arc<[u8]>,
    tracker: Arc<InFlightTracker>,
}

impl SharedBacking for FfiFrameBacking {
    fn as_bytes(&self) -> &[u8] {
        &self.data
    }
}

impl Drop for FfiFrameBacking {
    fn drop(&mut self) {
        self.tracker.release();
    }
}

// ---------------------------------------------------------------------------
// Public Link / Tx / Rx types
// ---------------------------------------------------------------------------

/// An in-process FFI link.
///
/// Use [`ffi_link_pair`] to create two connected ends.
pub struct FfiLink {
    tx_tracker: Arc<InFlightTracker>,
    tx_peer: Arc<FfiRxInner>,
    rx_inner: Arc<FfiRxInner>,
}

/// Create a pair of connected [`FfiLink`]s.
///
/// `max_in_flight` is the maximum number of frames that may be outstanding
/// (loaned to the receiver but not yet released) per direction at once.
/// This is the backpressure limit for the sender.
pub fn ffi_link_pair(max_in_flight: usize) -> (FfiLink, FfiLink) {
    let rx_a = FfiRxInner::new();
    let rx_b = FfiRxInner::new();
    let tracker_a = InFlightTracker::new(max_in_flight);
    let tracker_b = InFlightTracker::new(max_in_flight);

    let a = FfiLink {
        tx_tracker: Arc::clone(&tracker_a),
        tx_peer: Arc::clone(&rx_b),
        rx_inner: Arc::clone(&rx_a),
    };
    let b = FfiLink {
        tx_tracker: Arc::clone(&tracker_b),
        tx_peer: Arc::clone(&rx_a),
        rx_inner: Arc::clone(&rx_b),
    };
    (a, b)
}

impl Link for FfiLink {
    type Tx = FfiLinkTx;
    type Rx = FfiLinkRx;

    fn split(self) -> (Self::Tx, Self::Rx) {
        (
            FfiLinkTx {
                tracker: self.tx_tracker,
                peer: self.tx_peer,
            },
            FfiLinkRx {
                inner: self.rx_inner,
            },
        )
    }
}

// ---------------------------------------------------------------------------
// Tx
// ---------------------------------------------------------------------------

/// Sending half of a [`FfiLink`].
pub struct FfiLinkTx {
    tracker: Arc<InFlightTracker>,
    peer: Arc<FfiRxInner>,
}

/// Permit to send exactly one frame.
pub struct FfiLinkTxPermit {
    tracker: Arc<InFlightTracker>,
    peer: Arc<FfiRxInner>,
}

impl LinkTx for FfiLinkTx {
    type Permit = FfiLinkTxPermit;

    async fn reserve(&self) -> std::io::Result<Self::Permit> {
        poll_fn(|cx| {
            if self.tracker.try_acquire() {
                return std::task::Poll::Ready(Ok(FfiLinkTxPermit {
                    tracker: Arc::clone(&self.tracker),
                    peer: Arc::clone(&self.peer),
                }));
            }
            // Register waker before rechecking to avoid a wake-then-miss race.
            {
                let mut guard = self.tracker.waker.lock().unwrap();
                *guard = Some(cx.waker().clone());
            }
            // Recheck after registering the waker.
            if self.tracker.try_acquire() {
                self.tracker.waker.lock().unwrap().take();
                return std::task::Poll::Ready(Ok(FfiLinkTxPermit {
                    tracker: Arc::clone(&self.tracker),
                    peer: Arc::clone(&self.peer),
                }));
            }
            std::task::Poll::Pending
        })
        .await
    }

    async fn close(self) -> std::io::Result<()> {
        self.peer.closed.store(true, Ordering::Release);
        if let Ok(mut guard) = self.peer.waker.lock() {
            if let Some(w) = guard.take() {
                w.wake();
            }
        }
        Ok(())
    }
}

impl LinkTxPermit for FfiLinkTxPermit {
    type Slot = FfiWriteSlot;

    fn alloc(self, len: usize) -> std::io::Result<Self::Slot> {
        Ok(FfiWriteSlot {
            buf: Some(vec![0u8; len].into_boxed_slice()),
            tracker: self.tracker,
            peer: self.peer,
        })
    }
}

/// Write slot for [`FfiLinkTx`].
pub struct FfiWriteSlot {
    /// `None` after `commit` so that `Drop` knows not to release the permit.
    buf: Option<Box<[u8]>>,
    tracker: Arc<InFlightTracker>,
    peer: Arc<FfiRxInner>,
}

impl WriteSlot for FfiWriteSlot {
    fn as_mut_slice(&mut self) -> &mut [u8] {
        self.buf.as_mut().expect("buf present before commit")
    }

    fn commit(mut self) {
        let buf = self.buf.take().expect("buf present before commit");
        let frame = Arc::new(FfiFrameBacking {
            data: Arc::from(buf),
            tracker: Arc::clone(&self.tracker),
        });
        self.peer.push(frame);
        // Drop of self will see buf == None and skip the release.
    }
}

impl Drop for FfiWriteSlot {
    fn drop(&mut self) {
        if self.buf.is_some() {
            // Slot was dropped without committing — release the reservation.
            self.tracker.release();
        }
    }
}

// ---------------------------------------------------------------------------
// Rx
// ---------------------------------------------------------------------------

/// Receiving half of a [`FfiLink`].
pub struct FfiLinkRx {
    inner: Arc<FfiRxInner>,
}

/// Error type for [`FfiLinkRx`].  Currently infallible (only EOF is possible).
#[derive(Debug)]
pub struct FfiLinkRxError;

impl std::fmt::Display for FfiLinkRxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ffi link rx error")
    }
}

impl std::error::Error for FfiLinkRxError {}

impl LinkRx for FfiLinkRx {
    type Error = FfiLinkRxError;

    async fn recv(&mut self) -> Result<Option<Backing>, Self::Error> {
        poll_fn(|cx| {
            if let Some(frame) = self.inner.queue.lock().unwrap().pop_front() {
                return std::task::Poll::Ready(Ok(Some(Backing::shared(frame))));
            }
            if self.inner.closed.load(Ordering::Acquire) {
                return std::task::Poll::Ready(Ok(None));
            }
            // Register waker, then recheck both queue and closed flag.
            {
                let mut guard = self.inner.waker.lock().unwrap();
                *guard = Some(cx.waker().clone());
            }
            if let Some(frame) = self.inner.queue.lock().unwrap().pop_front() {
                return std::task::Poll::Ready(Ok(Some(Backing::shared(frame))));
            }
            if self.inner.closed.load(Ordering::Acquire) {
                return std::task::Poll::Ready(Ok(None));
            }
            std::task::Poll::Pending
        })
        .await
    }
}
