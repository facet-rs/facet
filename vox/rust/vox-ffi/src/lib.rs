#![allow(non_camel_case_types)]

use std::collections::VecDeque;
use std::future::poll_fn;
use std::io;
use std::mem::size_of;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Poll, Waker};

use tracing::trace;
use vox_types::{Backing, Link, LinkRx, LinkTx, LinkTxPermit, SharedBacking, WriteSlot};

pub type vox_status_t = i32;
pub type vox_send_fn = unsafe extern "C" fn(buf: *const u8, len: usize);
pub type vox_free_fn = unsafe extern "C" fn(buf: *const u8);
pub type vox_attach_fn = unsafe extern "C" fn(peer: *const vox_link_vtable) -> vox_status_t;

pub const VOX_STATUS_OK: vox_status_t = 0;
pub const VOX_STATUS_ALREADY_ATTACHED: vox_status_t = -1;
pub const VOX_STATUS_BAD_ABI: vox_status_t = -2;
pub const VOX_STATUS_INVALID_PEER: vox_status_t = -3;

pub const VOX_LINK_VTABLE_MAGIC: u64 = 0x564f_584c_494e_4b31;
pub const VOX_LINK_VTABLE_ABI_VERSION: u32 = 1;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct vox_link_vtable {
    pub magic: u64,
    pub abi_version: u32,
    pub size: u32,
    pub send: Option<vox_send_fn>,
    pub free: Option<vox_free_fn>,
    pub attach: Option<vox_attach_fn>,
}

impl vox_link_vtable {
    pub const fn new(send: vox_send_fn, free: vox_free_fn, attach: vox_attach_fn) -> Self {
        Self {
            magic: VOX_LINK_VTABLE_MAGIC,
            abi_version: VOX_LINK_VTABLE_ABI_VERSION,
            size: size_of::<Self>() as u32,
            send: Some(send),
            free: Some(free),
            attach: Some(attach),
        }
    }

    pub fn validate(&self) -> io::Result<()> {
        if self.magic != VOX_LINK_VTABLE_MAGIC {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "ffi vtable magic mismatch",
            ));
        }
        if self.abi_version != VOX_LINK_VTABLE_ABI_VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "ffi vtable abi version mismatch",
            ));
        }
        if self.size != size_of::<Self>() as u32 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "ffi vtable size mismatch",
            ));
        }
        if self.send.is_none() || self.free.is_none() || self.attach.is_none() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "ffi vtable missing callbacks",
            ));
        }
        Ok(())
    }

    pub unsafe fn validate_ptr(peer: *const Self) -> io::Result<&'static Self> {
        let peer = unsafe { peer.as_ref() }.ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "ffi vtable pointer was null")
        })?;
        peer.validate()?;
        Ok(peer)
    }
}

#[derive(Clone, Copy)]
struct IncomingFrame {
    ptr: *const u8,
    len: usize,
}

unsafe impl Send for IncomingFrame {}
unsafe impl Sync for IncomingFrame {}

#[derive(Clone, Copy, PartialEq, Eq)]
struct PeerPtr(*const vox_link_vtable);

unsafe impl Send for PeerPtr {}
unsafe impl Sync for PeerPtr {}

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
    peer: Mutex<Option<PeerPtr>>,
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
            peer: Mutex::new(None),
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
        trace!("ffi endpoint connect: validating peer");
        peer.validate()?;
        trace!("ffi endpoint connect: attaching peer");
        self.attach_peer(PeerPtr(peer as *const vox_link_vtable))?;
        let attach = peer.attach.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "ffi vtable missing attach callback",
            )
        })?;
        trace!("ffi endpoint connect: calling peer attach");
        let status = unsafe { attach(self.vtable() as *const vox_link_vtable) };
        if status != VOX_STATUS_OK {
            trace!("ffi endpoint connect: peer attach failed status={status}");
            self.clear_peer_if(PeerPtr(peer as *const vox_link_vtable));
            return Err(status_error(status));
        }
        trace!("ffi endpoint connect: taking link");
        self.take_link()
    }

    pub async fn accept(&'static self) -> io::Result<FfiLink> {
        trace!("ffi endpoint accept: waiting for peer");
        poll_fn(|cx| {
            if self.peer.lock().expect("peer poisoned").is_some() {
                trace!("ffi endpoint accept: peer present, ready");
                Poll::Ready(())
            } else {
                trace!("ffi endpoint accept: no peer yet, parking");
                *self.accept_waker.lock().expect("accept_waker poisoned") =
                    Some(cx.waker().clone());
                Poll::Pending
            }
        })
        .await;

        trace!("ffi endpoint accept: taking link");
        self.take_link()
    }

    fn take_link(&'static self) -> io::Result<FfiLink> {
        if self
            .link_taken
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            trace!("ffi endpoint take_link: already connected");
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "ffi endpoint already connected",
            ));
        }

        trace!("ffi endpoint take_link: ok");
        Ok(FfiLink { endpoint: self })
    }

    fn attach_peer(&'static self, peer: PeerPtr) -> io::Result<()> {
        let mut slot = self.peer.lock().expect("peer poisoned");
        if let Some(existing) = *slot {
            if existing == peer {
                trace!("ffi endpoint attach_peer: same peer, ok");
                return Ok(());
            }
            trace!("ffi endpoint attach_peer: different peer, already attached");
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "ffi endpoint already attached",
            ));
        }
        trace!("ffi endpoint attach_peer: attached peer={:p}", peer.0);
        *slot = Some(peer);
        if let Some(waker) = self
            .accept_waker
            .lock()
            .expect("accept_waker poisoned")
            .take()
        {
            trace!("ffi endpoint attach_peer: waking accept waiter");
            waker.wake();
        }
        Ok(())
    }

    fn clear_peer_if(&'static self, peer: PeerPtr) {
        let mut slot = self.peer.lock().expect("peer poisoned");
        if slot.as_ref().is_some_and(|existing| *existing == peer) {
            trace!("ffi endpoint clear_peer_if: cleared");
            *slot = None;
        }
    }

    fn peer(&'static self) -> io::Result<&'static vox_link_vtable> {
        let peer = *self.peer.lock().expect("peer poisoned");
        let peer = peer.ok_or_else(|| {
            trace!("ffi endpoint peer: no peer attached");
            io::Error::new(
                io::ErrorKind::NotConnected,
                "ffi endpoint has no attached peer",
            )
        })?;
        unsafe { vox_link_vtable::validate_ptr(peer.0) }
    }

    fn send_bytes(&'static self, bytes: Vec<u8>) -> io::Result<()> {
        let peer = self.peer()?;
        let loan = OutboundLoan::new(bytes);
        let ptr = loan.ptr;
        let len = loan.len;

        let _send_guard = self.send_lock.lock().expect("send_lock poisoned");
        self.outbound.lock().expect("outbound poisoned").push(loan);
        let send = peer.send.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "ffi vtable missing send callback",
            )
        })?;
        trace!("ffi endpoint send_bytes: len={len}");
        unsafe { send(ptr, len) };

        Ok(())
    }

    fn poll_recv(&'static self, cx: &mut std::task::Context<'_>) -> Poll<IncomingFrame> {
        if let Some(frame) = self.inbox.lock().expect("inbox poisoned").pop_front() {
            trace!("ffi endpoint poll_recv: got frame len={}", frame.len);
            Poll::Ready(frame)
        } else {
            *self.recv_waker.lock().expect("recv_waker poisoned") = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

#[doc(hidden)]
pub unsafe fn __endpoint_send(endpoint: &'static Endpoint, buf: *const u8, len: usize) {
    trace!("ffi __endpoint_send: len={len}");
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
        trace!("ffi __endpoint_send: waking recv waiter");
        waker.wake();
    }
}

#[doc(hidden)]
pub unsafe fn __endpoint_free(endpoint: &'static Endpoint, buf: *const u8) {
    let mut outbound = endpoint.outbound.lock().expect("outbound poisoned");
    if let Some(index) = outbound.iter().position(|loan| loan.ptr == buf) {
        let loan = outbound.swap_remove(index);
        trace!("ffi __endpoint_free: freed loan len={}", loan.storage.len());
    } else {
        trace!("ffi __endpoint_free: loan not found for ptr={buf:p}");
    }
}

#[doc(hidden)]
pub unsafe fn __endpoint_attach(
    endpoint: &'static Endpoint,
    peer: *const vox_link_vtable,
) -> vox_status_t {
    trace!("ffi __endpoint_attach: peer={peer:p}");
    let peer = match unsafe { peer.as_ref() } {
        None => {
            trace!("ffi __endpoint_attach: null peer");
            return VOX_STATUS_INVALID_PEER;
        }
        Some(peer) => peer,
    };
    if let Err(error) = peer.validate() {
        trace!("ffi __endpoint_attach: validation failed: {error}");
        return match error.kind() {
            io::ErrorKind::InvalidInput => VOX_STATUS_INVALID_PEER,
            io::ErrorKind::InvalidData => VOX_STATUS_BAD_ABI,
            _ => VOX_STATUS_BAD_ABI,
        };
    }
    let result = match endpoint.attach_peer(PeerPtr(peer as *const vox_link_vtable)) {
        Ok(()) => VOX_STATUS_OK,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => VOX_STATUS_ALREADY_ATTACHED,
        Err(_) => VOX_STATUS_BAD_ABI,
    };
    trace!("ffi __endpoint_attach: result={result}");
    result
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
        trace!("ffi link split");
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
        trace!("ffi link rx recv: got frame len={}", frame.len);
        let peer = self.endpoint.peer().map_err(|e| {
            trace!("ffi link rx recv: peer lookup failed: {e}");
            FfiLinkRxError
        })?;
        Ok(Some(Backing::shared(Arc::new(FfiBacking {
            frame,
            free: peer.free.ok_or(FfiLinkRxError)?,
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

            unsafe extern "C" fn __vox_attach(
                peer: *const $crate::vox_link_vtable,
            ) -> $crate::vox_status_t {
                unsafe { $crate::__endpoint_attach(&__ENDPOINT, peer) }
            }

            static __VOX_LINK_VTABLE: $crate::vox_link_vtable =
                $crate::vox_link_vtable::new(__vox_send, __vox_free, __vox_attach);

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

fn status_error(status: vox_status_t) -> io::Error {
    let kind = match status {
        VOX_STATUS_ALREADY_ATTACHED => io::ErrorKind::AlreadyExists,
        VOX_STATUS_INVALID_PEER | VOX_STATUS_BAD_ABI => io::ErrorKind::InvalidData,
        _ => io::ErrorKind::Other,
    };
    io::Error::new(kind, format!("ffi attach failed with status {status}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe extern "C" fn noop_send(_: *const u8, _: usize) {}
    unsafe extern "C" fn noop_free(_: *const u8) {}
    unsafe extern "C" fn noop_attach(_: *const vox_link_vtable) -> vox_status_t {
        VOX_STATUS_OK
    }

    fn test_endpoint_vtable() -> &'static vox_link_vtable {
        &TEST_ENDPOINT_VTABLE
    }

    static TEST_ENDPOINT_VTABLE: vox_link_vtable =
        vox_link_vtable::new(noop_send, noop_free, noop_attach);
    static TEST_ENDPOINT: Endpoint = Endpoint::new(test_endpoint_vtable);

    #[test]
    fn validates_a_well_formed_vtable() {
        let vtable = vox_link_vtable::new(noop_send, noop_free, noop_attach);
        assert!(vtable.validate().is_ok());
    }

    #[test]
    fn rejects_bad_magic() {
        let mut vtable = vox_link_vtable::new(noop_send, noop_free, noop_attach);
        vtable.magic ^= 1;
        assert!(matches!(
            vtable.validate(),
            Err(error) if error.kind() == io::ErrorKind::InvalidData
        ));
    }

    #[test]
    fn rejects_missing_callbacks() {
        let mut vtable = vox_link_vtable::new(noop_send, noop_free, noop_attach);
        vtable.attach = None;
        assert!(matches!(
            vtable.validate(),
            Err(error) if error.kind() == io::ErrorKind::InvalidData
        ));
    }

    #[test]
    fn attach_rejects_null_peer() {
        assert_eq!(
            unsafe { __endpoint_attach(&TEST_ENDPOINT, std::ptr::null()) },
            VOX_STATUS_INVALID_PEER
        );
    }

    #[test]
    fn attach_allows_same_peer_and_rejects_second_peer() {
        static SECOND_PEER: vox_link_vtable =
            vox_link_vtable::new(noop_send, noop_free, noop_attach);

        assert_eq!(
            unsafe {
                __endpoint_attach(
                    &TEST_ENDPOINT,
                    &TEST_ENDPOINT_VTABLE as *const vox_link_vtable,
                )
            },
            VOX_STATUS_OK
        );
        assert_eq!(
            unsafe {
                __endpoint_attach(
                    &TEST_ENDPOINT,
                    &TEST_ENDPOINT_VTABLE as *const vox_link_vtable,
                )
            },
            VOX_STATUS_OK
        );
        assert_eq!(
            unsafe { __endpoint_attach(&TEST_ENDPOINT, &SECOND_PEER as *const vox_link_vtable) },
            VOX_STATUS_ALREADY_ATTACHED
        );
    }
}
