use moire::sync::mpsc;
use vox_types::{Backing, Link, LinkRx, LinkTx};

/// One in-process frame: bytes, plus any descriptors moving with it.
///
/// In-process fd "passing" is just an ownership move through the same
/// channel as the bytes — no `SCM_RIGHTS`, and no separate stream that
/// could desync.
#[cfg(unix)]
type MemItem = (Vec<u8>, Vec<std::os::fd::OwnedFd>);
#[cfg(not(unix))]
type MemItem = Vec<u8>;

/// In-process [`Link`] backed by tokio mpsc channels.
///
/// Each direction is an unbounded channel carrying raw bytes (and, on Unix,
/// any `Fd`s travelling with them) — no serialization, no IO. Useful for
/// testing Conduits, Session, and anything above the transport layer
/// without real networking.
// r[impl transport.memory]
pub struct MemoryLink {
    tx: mpsc::Sender<MemItem>,
    rx: mpsc::Receiver<MemItem>,
}

/// Create a pair of connected [`MemoryLink`]s.
///
/// Returns `(a, b)` where sending on `a` delivers to `b` and vice versa.
pub fn memory_link_pair(buffer: usize) -> (MemoryLink, MemoryLink) {
    let (tx_a, rx_b) = mpsc::channel("memory_link.a→b", buffer);
    let (tx_b, rx_a) = mpsc::channel("memory_link.b→a", buffer);

    let a = MemoryLink { tx: tx_a, rx: rx_a };
    let b = MemoryLink { tx: tx_b, rx: rx_b };

    (a, b)
}

impl Link for MemoryLink {
    type Tx = MemoryLinkTx;
    type Rx = MemoryLinkRx;

    fn split(self) -> (Self::Tx, Self::Rx) {
        (
            MemoryLinkTx { tx: self.tx },
            MemoryLinkRx {
                rx: self.rx,
                #[cfg(unix)]
                last_fds: Vec::new(),
            },
        )
    }
}

// ---------------------------------------------------------------------------
// Tx
// ---------------------------------------------------------------------------

/// Sending half of a [`MemoryLink`].
#[derive(Clone)]
pub struct MemoryLinkTx {
    tx: mpsc::Sender<MemItem>,
}

impl MemoryLinkTx {
    async fn send_item(&self, item: MemItem) -> std::io::Result<()> {
        let permit = self.tx.clone().reserve_owned().await.map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::ConnectionReset, "receiver dropped")
        })?;
        drop(permit.send(item));
        Ok(())
    }
}

impl LinkTx for MemoryLinkTx {
    async fn send(&self, bytes: Vec<u8>) -> std::io::Result<()> {
        #[cfg(unix)]
        {
            self.send_item((bytes, Vec::new())).await
        }
        #[cfg(not(unix))]
        {
            self.send_item(bytes).await
        }
    }

    async fn close(self) -> std::io::Result<()> {
        drop(self.tx);
        Ok(())
    }

    #[cfg(unix)]
    fn supports_fd_passing(&self) -> bool {
        true
    }

    #[cfg(unix)]
    async fn send_with_fds(
        &self,
        bytes: Vec<u8>,
        fds: Vec<std::os::fd::OwnedFd>,
    ) -> std::io::Result<()> {
        self.send_item((bytes, fds)).await
    }
}

// ---------------------------------------------------------------------------
// Rx
// ---------------------------------------------------------------------------

/// Receiving half of a [`MemoryLink`].
pub struct MemoryLinkRx {
    rx: mpsc::Receiver<MemItem>,
    #[cfg(unix)]
    last_fds: Vec<std::os::fd::OwnedFd>,
}

/// MemoryLink never fails on recv — the only "error" is channel closed (returns None).
#[derive(Debug)]
pub struct MemoryLinkRxError;

impl std::fmt::Display for MemoryLinkRxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "memory link rx error (unreachable)")
    }
}

impl std::error::Error for MemoryLinkRxError {}

impl LinkRx for MemoryLinkRx {
    type Error = MemoryLinkRxError;

    async fn recv(&mut self) -> Result<Option<Backing>, Self::Error> {
        match self.rx.recv().await {
            #[cfg(unix)]
            Some((bytes, fds)) => {
                self.last_fds = fds;
                Ok(Some(Backing::Boxed(bytes.into_boxed_slice())))
            }
            #[cfg(not(unix))]
            Some(bytes) => Ok(Some(Backing::Boxed(bytes.into_boxed_slice()))),
            None => Ok(None),
        }
    }

    #[cfg(unix)]
    fn take_frame_fds(&mut self) -> Vec<std::os::fd::OwnedFd> {
        std::mem::take(&mut self.last_fds)
    }
}

#[cfg(test)]
mod tests {
    use vox_types::{Backing, Link, LinkRx, LinkTx};

    use super::memory_link_pair;

    fn payload(backing: &Backing) -> &[u8] {
        match backing {
            Backing::Boxed(bytes) => bytes,
            Backing::Shared(bytes) => bytes.as_bytes(),
        }
    }

    // r[verify transport.memory]
    #[tokio::test]
    async fn memory_link_preserves_boundaries_order_and_close() {
        let (a, b) = memory_link_pair(4);
        let (tx_a, _rx_a) = a.split();
        let (_tx_b, mut rx_b) = b.split();

        let payloads: &[&[u8]] = &[b"one", b"", b"three"];
        for bytes in payloads {
            tx_a.send(bytes.to_vec()).await.unwrap();
        }
        tx_a.close().await.unwrap();

        for expected in payloads {
            let received = rx_b.recv().await.unwrap().unwrap();
            assert_eq!(payload(&received), *expected);
        }
        assert!(rx_b.recv().await.unwrap().is_none());
    }
}
