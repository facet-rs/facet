#![allow(non_camel_case_types)]

use std::collections::VecDeque;
use std::future::poll_fn;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::task::{Poll, Waker};

use vox_types::{Backing, Link, LinkRx, LinkTx, LinkTxPermit, SharedBacking, WriteSlot};

pub type vox_send_fn = unsafe extern "C" fn(buf: *const u8, len: usize);
pub type vox_free_fn = unsafe extern "C" fn(buf: *const u8);
pub type vox_attach_fn = unsafe extern "C" fn(peer: *const vox_link_vtable);

#[repr(C)]
#[derive(Clone, Copy)]
pub struct vox_link_vtable {
    pub send: vox_send_fn,
    pub free: vox_free_fn,
    pub attach: vox_attach_fn,
}

#[derive(Clone, Copy)]
struct IncomingFrame {
    ptr: *const u8,
    len: usize,
}

unsafe impl Send for IncomingFrame {}
unsafe impl Sync for IncomingFrame {}

struct OutboundLoan {
    ptr: *const u8,
    len: usize,
    storage: Box<[u8]>,
}

unsafe impl Send for OutboundLoan {}
unsafe impl Sync for OutboundLoan {}

impl OutboundLoan {
    fn new(bytes: Vec<u8>) -> Self {
        let len = bytes.len();
        let storage = if len == 0 {
            vec![0u8].into_boxed_slice()
        } else {
            bytes.into_boxed_slice()
        };
        let ptr = storage.as_ptr();
        Self { ptr, len, storage }
    }
}

struct FfiBacking {
    frame: IncomingFrame,
    free: vox_free_fn,
}

unsafe impl Send for FfiBacking {}
unsafe impl Sync for FfiBacking {}

impl SharedBacking for FfiBacking {
    fn as_bytes(&self) -> &[u8] {
        if self.frame.len == 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(self.frame.ptr, self.frame.len) }
        }
    }
}

impl Drop for FfiBacking {
    fn drop(&mut self) {
        unsafe { (self.free)(self.frame.ptr) }
    }
}

/// In-process FFI endpoint.
///
/// This owns the callback bridge state. Providers export one static vtable and
/// then use `connect` or `accept` to obtain a normal Vox `Link`.
pub struct Endpoint {
    vtable: fn() -> &'static vox_link_vtable,
    peer: OnceLock<vox_link_vtable>,
    link_taken: AtomicBool,
    inbox: Mutex<VecDeque<IncomingFrame>>,
    outbound: Mutex<Vec<OutboundLoan>>,
    recv_waker: Mutex<Option<Waker>>,
    accept_waker: Mutex<Option<Waker>>,
    send_lock: Mutex<()>,
}

impl Endpoint {
    pub const fn new(vtable: fn() -> &'static vox_link_vtable) -> Self {
        Self {
            vtable,
            peer: OnceLock::new(),
            link_taken: AtomicBool::new(false),
            inbox: Mutex::new(VecDeque::new()),
            outbound: Mutex::new(Vec::new()),
            recv_waker: Mutex::new(None),
            accept_waker: Mutex::new(None),
            send_lock: Mutex::new(()),
        }
    }

    pub fn vtable(&'static self) -> &'static vox_link_vtable {
        (self.vtable)()
    }

    pub fn connect(&'static self, peer: &'static vox_link_vtable) -> io::Result<FfiLink> {
        self.attach_peer(*peer);
        unsafe { (peer.attach)(self.vtable() as *const vox_link_vtable) };
        self.take_link()
    }

    pub async fn accept(&'static self) -> io::Result<FfiLink> {
        poll_fn(|cx| {
            if self.peer.get().is_some() {
                Poll::Ready(())
            } else {
                *self.accept_waker.lock().expect("accept_waker poisoned") =
                    Some(cx.waker().clone());
                Poll::Pending
            }
        })
        .await;

        self.take_link()
    }

    fn take_link(&'static self) -> io::Result<FfiLink> {
        if self
            .link_taken
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "ffi endpoint already connected",
            ));
        }

        Ok(FfiLink { endpoint: self })
    }

    fn attach_peer(&'static self, peer: vox_link_vtable) {
        let _ = self.peer.set(peer);
        if let Some(waker) = self
            .accept_waker
            .lock()
            .expect("accept_waker poisoned")
            .take()
        {
            waker.wake();
        }
    }

    fn peer(&'static self) -> io::Result<vox_link_vtable> {
        self.peer.get().copied().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotConnected,
                "ffi endpoint has no attached peer",
            )
        })
    }

    fn send_bytes(&'static self, bytes: Vec<u8>) -> io::Result<()> {
        let peer = self.peer()?;
        let loan = OutboundLoan::new(bytes);
        let ptr = loan.ptr;
        let len = loan.len;

        let _send_guard = self.send_lock.lock().expect("send_lock poisoned");
        self.outbound.lock().expect("outbound poisoned").push(loan);
        unsafe { (peer.send)(ptr, len) };

        Ok(())
    }

    fn poll_recv(&'static self, cx: &mut std::task::Context<'_>) -> Poll<IncomingFrame> {
        if let Some(frame) = self.inbox.lock().expect("inbox poisoned").pop_front() {
            Poll::Ready(frame)
        } else {
            *self.recv_waker.lock().expect("recv_waker poisoned") = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

#[doc(hidden)]
pub unsafe fn __endpoint_send(endpoint: &'static Endpoint, buf: *const u8, len: usize) {
    endpoint
        .inbox
        .lock()
        .expect("inbox poisoned")
        .push_back(IncomingFrame { ptr: buf, len });
    if let Some(waker) = endpoint
        .recv_waker
        .lock()
        .expect("recv_waker poisoned")
        .take()
    {
        waker.wake();
    }
}

#[doc(hidden)]
pub unsafe fn __endpoint_free(endpoint: &'static Endpoint, buf: *const u8) {
    let mut outbound = endpoint.outbound.lock().expect("outbound poisoned");
    if let Some(index) = outbound.iter().position(|loan| loan.ptr == buf) {
        let loan = outbound.swap_remove(index);
        let _ = loan.storage.len();
    }
}

#[doc(hidden)]
pub unsafe fn __endpoint_attach(endpoint: &'static Endpoint, peer: *const vox_link_vtable) {
    if let Some(peer) = unsafe { peer.as_ref() } {
        endpoint.attach_peer(*peer);
    }
}

// r[impl link]
// r[impl link.message]
// r[impl link.order]
pub struct FfiLink {
    endpoint: &'static Endpoint,
}

pub struct FfiLinkTx {
    endpoint: &'static Endpoint,
}

pub struct FfiLinkRx {
    endpoint: &'static Endpoint,
}

pub struct FfiLinkTxPermit {
    endpoint: &'static Endpoint,
}

pub struct FfiWriteSlot {
    endpoint: &'static Endpoint,
    buf: Vec<u8>,
}

impl Link for FfiLink {
    type Tx = FfiLinkTx;
    type Rx = FfiLinkRx;

    // r[impl link.split]
    fn split(self) -> (Self::Tx, Self::Rx) {
        (
            FfiLinkTx {
                endpoint: self.endpoint,
            },
            FfiLinkRx {
                endpoint: self.endpoint,
            },
        )
    }
}

impl LinkTx for FfiLinkTx {
    type Permit = FfiLinkTxPermit;

    // r[impl link.tx.reserve]
    // r[impl link.tx.cancel-safe]
    async fn reserve(&self) -> io::Result<Self::Permit> {
        self.endpoint.peer()?;
        Ok(FfiLinkTxPermit {
            endpoint: self.endpoint,
        })
    }

    async fn close(self) -> io::Result<()> {
        Ok(())
    }
}

impl LinkTxPermit for FfiLinkTxPermit {
    type Slot = FfiWriteSlot;

    // r[impl link.tx.alloc.limits]
    // r[impl link.message.empty]
    fn alloc(self, len: usize) -> io::Result<Self::Slot> {
        Ok(FfiWriteSlot {
            endpoint: self.endpoint,
            buf: vec![0u8; len],
        })
    }
}

impl WriteSlot for FfiWriteSlot {
    // r[impl link.tx.slot.len]
    fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.buf
    }

    // r[impl link.tx.commit]
    fn commit(self) {
        self.endpoint
            .send_bytes(self.buf)
            .expect("ffi peer must be attached before commit");
    }
}

#[derive(Debug)]
pub struct FfiLinkRxError;

impl std::fmt::Display for FfiLinkRxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ffi link receive error")
    }
}

impl std::error::Error for FfiLinkRxError {}

impl LinkRx for FfiLinkRx {
    type Error = FfiLinkRxError;

    // r[impl link.rx.recv]
    async fn recv(&mut self) -> Result<Option<Backing>, Self::Error> {
        let frame = poll_fn(|cx| self.endpoint.poll_recv(cx)).await;
        let peer = self.endpoint.peer().map_err(|_| FfiLinkRxError)?;
        Ok(Some(Backing::shared(Arc::new(FfiBacking {
            frame,
            free: peer.free,
        }))))
    }
}

#[macro_export]
macro_rules! declare_link_endpoint {
    ($vis:vis mod $module:ident { export = $export:ident; }) => {
        $vis mod $module {
            fn __vox_link_vtable() -> &'static $crate::vox_link_vtable {
                &__VOX_LINK_VTABLE
            }

            static __ENDPOINT: $crate::Endpoint = $crate::Endpoint::new(__vox_link_vtable);

            unsafe extern "C" fn __vox_send(buf: *const u8, len: usize) {
                unsafe { $crate::__endpoint_send(&__ENDPOINT, buf, len) };
            }

            unsafe extern "C" fn __vox_free(buf: *const u8) {
                unsafe { $crate::__endpoint_free(&__ENDPOINT, buf) };
            }

            unsafe extern "C" fn __vox_attach(peer: *const $crate::vox_link_vtable) {
                unsafe { $crate::__endpoint_attach(&__ENDPOINT, peer) };
            }

            static __VOX_LINK_VTABLE: $crate::vox_link_vtable = $crate::vox_link_vtable {
                send: __vox_send,
                free: __vox_free,
                attach: __vox_attach,
            };

            pub fn vtable() -> &'static $crate::vox_link_vtable {
                __vox_link_vtable()
            }

            #[cfg_attr(test, allow(dead_code))]
            pub fn connect(
                peer: &'static $crate::vox_link_vtable,
            ) -> std::io::Result<$crate::FfiLink> {
                __ENDPOINT.connect(peer)
            }

            #[cfg_attr(test, allow(dead_code))]
            pub async fn accept() -> std::io::Result<$crate::FfiLink> {
                __ENDPOINT.accept().await
            }

            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn $export() -> *const $crate::vox_link_vtable {
                vtable() as *const $crate::vox_link_vtable
            }
        }
    };
}
