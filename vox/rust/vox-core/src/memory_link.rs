use moire::sync::mpsc;
use vox_types::{Backing, Link, LinkRx, LinkTx};

/// In-process [`Link`] backed by tokio mpsc channels.
///
/// Each direction is an unbounded channel carrying `Vec<u8>` — raw bytes,
/// no serialization, no IO. Useful for testing Conduits, Session, and
/// anything above the transport layer without real networking.
// r[impl transport.memory]
// r[impl zerocopy.framing.link.memory]
pub struct MemoryLink {
    tx: mpsc::Sender<Vec<u8>>,
    rx: mpsc::Receiver<Vec<u8>>,
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
        (MemoryLinkTx { tx: self.tx }, MemoryLinkRx { rx: self.rx })
    }
}

// ---------------------------------------------------------------------------
// Tx
// ---------------------------------------------------------------------------

/// Sending half of a [`MemoryLink`].
#[derive(Clone)]
pub struct MemoryLinkTx {
    tx: mpsc::Sender<Vec<u8>>,
}

impl LinkTx for MemoryLinkTx {
    async fn send(&self, bytes: Vec<u8>) -> std::io::Result<()> {
        let permit = self.tx.clone().reserve_owned().await.map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::ConnectionReset, "receiver dropped")
        })?;
        drop(permit.send(bytes));
        Ok(())
    }

    async fn close(self) -> std::io::Result<()> {
        drop(self.tx);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Rx
// ---------------------------------------------------------------------------

/// Receiving half of a [`MemoryLink`].
pub struct MemoryLinkRx {
    rx: mpsc::Receiver<Vec<u8>>,
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
            Some(bytes) => Ok(Some(Backing::Boxed(bytes.into_boxed_slice()))),
            None => Ok(None),
        }
    }
}
