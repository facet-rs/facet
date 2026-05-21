//! `Fd` — a file descriptor that travels across a vox connection.
//!
//! On the wire an [`Fd`] is encoded as a small varint *index* into a
//! per-message fd table; the descriptors themselves travel out-of-band in
//! `SCM_RIGHTS` ancillary data on a Unix-domain socket. This mirrors the
//! `Tx<T>` → [`ChannelId`](crate::ChannelId) indirection: the bytes on the
//! wire are just an index, the real resource is carried out-of-band and
//! re-associated at the peer.
//!
//! The send/recv side-channel is plumbed through two thread-locals
//! ([`collect_fds`] / [`provide_fds`]), installed around (de)serialization —
//! exactly the shape of the channel binder in [`mod@crate::channel`].
//!
//! [`Fd`] itself is Unix-only (codegen refuses it for non-local / non-Rust
//! targets). [`FrameFds`], [`collect_fds`] and [`provide_fds`] are portable
//! — on non-Unix targets `FrameFds` is `()` and the helpers are pass-throughs
//! — so the message-routing rail and generated client code need no `cfg`.

/// The descriptors carried with one frame. `Vec<OwnedFd>` on Unix; `()`
/// elsewhere (no transport can pass descriptors off-Unix).
#[cfg(unix)]
pub type FrameFds = Vec<std::os::fd::OwnedFd>;
/// The descriptors carried with one frame (none off-Unix).
#[cfg(not(unix))]
pub type FrameFds = ();

// ===========================================================================
// Non-Unix: portable no-op surface.
// ===========================================================================

#[cfg(not(unix))]
mod portable {
    /// No collector off-Unix: run `f`, gather nothing.
    pub fn collect_fds<R>(f: impl FnOnce() -> R) -> (R, super::FrameFds) {
        (f(), ())
    }

    /// No source off-Unix: descriptors cannot arrive, just run `f`.
    pub fn provide_fds<R>(_fds: super::FrameFds, f: impl FnOnce() -> R) -> R {
        f()
    }
}

#[cfg(not(unix))]
pub use portable::{collect_fds, provide_fds};

/// Number of descriptors in a frame's fd set (0 off-Unix).
#[cfg(unix)]
pub fn frame_fds_len(fds: &FrameFds) -> usize {
    fds.len()
}
/// Number of descriptors in a frame's fd set (0 off-Unix).
#[cfg(not(unix))]
pub fn frame_fds_len(_fds: &FrameFds) -> usize {
    0
}

// ===========================================================================
// Unix: the real implementation.
// ===========================================================================

#[cfg(unix)]
mod unix {
    use std::cell::{Cell, RefCell};
    use std::os::fd::{AsFd, AsRawFd, BorrowedFd, IntoRawFd, OwnedFd, RawFd};

    use facet::{Facet, FacetOpaqueAdapter, OpaqueDeserialize, OpaqueSerialize, PtrConst};

    use super::FrameFds;

    /// Maximum number of descriptors carried in a single `SCM_RIGHTS` control
    /// message. The kernel hard-caps this (`SCM_MAX_FD`); we surface a clean
    /// error rather than silently chunking across `sendmsg` calls.
    pub const SCM_MAX_FD: usize = 253;

    /// `wire_index` sentinel: this `Fd` has not been pushed into a collector.
    /// Also the value encoded when an outgoing `Fd` carries no descriptor, so
    /// the peer's `take_fd` fails cleanly instead of the encoder aborting
    /// across `extern "C"`.
    const NOT_COLLECTED: u32 = u32::MAX;

    /// A file descriptor that can be sent across a vox connection.
    ///
    /// Construct one with [`Fd::new`] from anything that owns a descriptor
    /// (`OwnedFd`, `File`, `UnixStream`, …). After the value has been
    /// received on the far side, take ownership with [`Fd::into_owned_fd`]
    /// or borrow it with [`Fd::as_raw_fd`].
    ///
    /// Serializing an `Fd` *duplicates* its descriptor into the transport's
    /// `SCM_RIGHTS` batch (the source `Fd` keeps ownership), so a response
    /// may be encoded more than once — the operation store's replay-seal
    /// pass and the wire pass — and the encoder's size/write double-call is
    /// deduped by the `Fd` value's address.
    #[derive(Facet)]
    #[facet(opaque = FdAdapter, traits(Debug))]
    pub struct Fd {
        /// The descriptor. `Some` for an outgoing `Fd`; `Some` for an
        /// incoming `Fd` built by `deserialize_build`; `None` once taken.
        inner: Cell<Option<OwnedFd>>,
        /// Scratch the adapter points `OpaqueSerialize` at. Holds
        /// `NOT_COLLECTED` until the first `serialize_map`, then the index
        /// assigned by the send collector. Serialized as a postcard varint.
        wire_index: Cell<u32>,
    }

    impl Fd {
        /// Wrap an owned descriptor for sending.
        pub fn new(fd: impl Into<OwnedFd>) -> Self {
            Self {
                inner: Cell::new(Some(fd.into())),
                wire_index: Cell::new(NOT_COLLECTED),
            }
        }

        /// Borrow the raw descriptor without taking ownership. `None` if it
        /// has already been taken.
        pub fn as_raw_fd(&self) -> Option<RawFd> {
            let taken = self.inner.take();
            let raw = taken.as_ref().map(|f| f.as_raw_fd());
            self.inner.set(taken);
            raw
        }

        /// Take the owned descriptor out of this `Fd`.
        pub fn into_owned_fd(self) -> Option<OwnedFd> {
            self.inner.take()
        }

        /// Take the descriptor as a raw, *owned* `RawFd`. The caller is
        /// responsible for closing it.
        pub fn into_raw_fd(self) -> Option<RawFd> {
            self.inner.take().map(IntoRawFd::into_raw_fd)
        }
    }

    impl std::fmt::Debug for Fd {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self.as_raw_fd() {
                Some(raw) => f.debug_tuple("Fd").field(&raw).finish(),
                None => f.debug_tuple("Fd").field(&"<consumed>").finish(),
            }
        }
    }

    // SAFETY: `Cell<Option<OwnedFd>>` / `Cell<u32>` are `Send` (contents are
    // `Send`); `Fd` is moved, never shared, so `!Sync` is fine — matching
    // `Payload`'s send-only stance.
    unsafe impl Send for Fd {}

    // -----------------------------------------------------------------------
    // Thread-local fd side-channel — same install-around-(de)serialize shape
    // as `CHANNEL_BINDER` (`channel.rs`).
    // -----------------------------------------------------------------------

    /// Descriptors gathered while encoding one message, with a dedup map so
    /// the encoder's size pass + write pass (which both run `serialize_map`
    /// for the same value) duplicate each descriptor exactly once.
    struct FdCollector {
        fds: Vec<OwnedFd>,
        /// `Fd` value address → assigned index, scoped to this collector.
        seen: std::collections::HashMap<usize, u32>,
    }

    std::thread_local! {
        static FD_COLLECTOR: RefCell<Option<FdCollector>> = const { RefCell::new(None) };
        static FD_SOURCE: RefCell<Option<Vec<Option<OwnedFd>>>> = const { RefCell::new(None) };
    }

    /// Install an empty fd collector for the duration of `f`, returning what
    /// `f` produced together with the descriptors it gathered.
    ///
    /// Descriptors are *duplicated* into the collector, so the source `Fd`
    /// keeps ownership and the same response can be encoded more than once
    /// (the operation store's replay-seal pass and the wire pass).
    pub fn collect_fds<R>(f: impl FnOnce() -> R) -> (R, FrameFds) {
        struct Restore(Option<FdCollector>);
        impl Drop for Restore {
            fn drop(&mut self) {
                FD_COLLECTOR.with(|c| *c.borrow_mut() = self.0.take());
            }
        }
        let fresh = FdCollector {
            fds: Vec::new(),
            seen: std::collections::HashMap::new(),
        };
        let _restore = Restore(FD_COLLECTOR.with(|c| c.borrow_mut().replace(fresh)));
        let out = f();
        let fds = FD_COLLECTOR
            .with(|c| {
                c.borrow_mut()
                    .as_mut()
                    .map(|col| std::mem::take(&mut col.fds))
            })
            .unwrap_or_default();
        (out, fds)
    }

    /// Provide the descriptors received with a frame for the duration of `f`
    /// (typed payload decoding). Each [`Fd`] decoded inside claims one by
    /// index.
    pub fn provide_fds<R>(fds: FrameFds, f: impl FnOnce() -> R) -> R {
        struct Restore(Option<Vec<Option<OwnedFd>>>);
        impl Drop for Restore {
            fn drop(&mut self) {
                FD_SOURCE.with(|c| *c.borrow_mut() = self.0.take());
            }
        }
        let slots = fds.into_iter().map(Some).collect();
        let _restore = Restore(FD_SOURCE.with(|c| c.borrow_mut().replace(slots)));
        f()
    }

    /// Duplicate `fd` into the active collector, returning its stable index.
    ///
    /// `key` is the source `Fd`'s value address: repeated calls for the same
    /// value within one collector (size pass then write pass) return the
    /// same index and duplicate the descriptor only once. Returns
    /// `NOT_COLLECTED` — never panics — when no collector is installed (e.g.
    /// the operation store's seal pre-encode: an fd response is inherently
    /// non-replayable) or if `dup` fails; a panic here would abort the
    /// process across the `extern "C"` encoder trampolines.
    fn collect_fd(key: usize, fd: BorrowedFd<'_>) -> u32 {
        FD_COLLECTOR.with(|c| {
            let mut slot = c.borrow_mut();
            let Some(col) = slot.as_mut() else {
                return NOT_COLLECTED;
            };
            if let Some(&idx) = col.seen.get(&key) {
                return idx;
            }
            let Ok(dup) = fd.try_clone_to_owned() else {
                return NOT_COLLECTED;
            };
            let idx = col.fds.len() as u32;
            col.fds.push(dup);
            col.seen.insert(key, idx);
            idx
        })
    }

    /// Claim descriptor `index` from the active source.
    fn take_fd(index: u32) -> Result<OwnedFd, String> {
        if index == NOT_COLLECTED {
            return Err("Fd was sent without a descriptor".to_string());
        }
        FD_SOURCE.with(|c| {
            let mut slot = c.borrow_mut();
            let vec = slot
                .as_mut()
                .ok_or_else(|| "Fd decoded with no fd source installed".to_string())?;
            let len = vec.len();
            let cell = vec
                .get_mut(index as usize)
                .ok_or_else(|| format!("Fd wire index {index} out of range ({len})"))?;
            cell.take()
                .ok_or_else(|| format!("Fd wire index {index} already claimed"))
        })
    }

    /// Adapter that bridges [`Fd`] through the opaque field contract
    /// (modelled on `PayloadAdapter` in `message.rs`).
    pub struct FdAdapter;

    impl FacetOpaqueAdapter for FdAdapter {
        type Error = String;
        type SendValue<'a> = Fd;
        type RecvValue<'de> = Fd;

        fn serialize_map(value: &Self::SendValue<'_>) -> OpaqueSerialize {
            // Borrow (don't consume): `collect_fd` dups, so the same response
            // can be encoded more than once (seal pass + wire pass) and the
            // size/write double-call is deduped by the `Fd` value address.
            // `Cell` has no `&` accessor for non-`Copy` contents, so swap
            // out and back.
            let taken = value.inner.take();
            let idx = match taken.as_ref() {
                Some(owned) => collect_fd(value as *const Fd as usize, owned.as_fd()),
                None => NOT_COLLECTED,
            };
            value.inner.set(taken);
            value.wire_index.set(idx);
            OpaqueSerialize {
                ptr: PtrConst::new(value.wire_index.as_ptr().cast::<u8>()),
                shape: <u32 as Facet>::SHAPE,
            }
        }

        fn deserialize_build<'de>(
            input: OpaqueDeserialize<'de>,
        ) -> Result<Self::RecvValue<'de>, Self::Error> {
            let bytes = match &input {
                OpaqueDeserialize::Borrowed(b) => *b,
                OpaqueDeserialize::Owned(b) => b.as_slice(),
            };
            let mut cursor = vox_postcard::decode::Cursor::new(bytes);
            let index = cursor
                .read_varint()
                .map_err(|e| format!("Fd index varint: {e}"))? as u32;
            let owned = take_fd(index)?;
            Ok(Fd {
                inner: Cell::new(Some(owned)),
                wire_index: Cell::new(index),
            })
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::io::{Read, Seek, Write};

        fn temp_file_with(seed: &[u8]) -> std::fs::File {
            let mut path = std::env::temp_dir();
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            path.push(format!("vox-fd-test-{}-{nanos}", std::process::id()));
            let mut f = std::fs::OpenOptions::new()
                .create(true)
                .read(true)
                .write(true)
                .truncate(true)
                .open(&path)
                .unwrap();
            let _ = std::fs::remove_file(&path);
            f.write_all(seed).unwrap();
            f.rewind().unwrap();
            f
        }

        #[test]
        fn fd_round_trips_through_postcard() {
            let file = temp_file_with(b"vox-fd-payload");
            let msg = Fd::new(OwnedFd::from(file));

            let (bytes, collected) = collect_fds(|| vox_postcard::to_vec(&msg).expect("encode"));
            assert_eq!(collected.len(), 1, "one fd collected");
            assert_eq!(&bytes[..4], &1u32.to_le_bytes());
            assert_eq!(bytes[4], 0);

            let decoded: Fd = provide_fds(collected, || {
                vox_postcard::from_slice(&bytes).expect("decode")
            });

            let mut f = std::fs::File::from(decoded.into_owned_fd().expect("owned fd"));
            let mut got = String::new();
            f.read_to_string(&mut got).unwrap();
            assert_eq!(got, "vox-fd-payload");
        }

        #[test]
        fn missing_source_is_a_clean_error() {
            let msg = Fd::new(OwnedFd::from(temp_file_with(b"x")));
            let (bytes, _fds) = collect_fds(|| vox_postcard::to_vec(&msg).unwrap());
            let r = std::panic::catch_unwind(|| vox_postcard::from_slice::<Fd>(&bytes));
            assert!(
                r.is_err() || r.unwrap().is_err(),
                "decoding an Fd with no source must fail"
            );
        }
    }
}

#[cfg(unix)]
pub use unix::{Fd, FdAdapter, SCM_MAX_FD, collect_fds, provide_fds};
