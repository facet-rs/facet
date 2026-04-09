//! Test utilities for vox-core. Publicly exported for use by integration
//! tests in downstream crates.

use std::collections::VecDeque;

use moire::sync::mpsc;
use vox_types::{Backing, Link, LinkRx, LinkTx};

use crate::{Attachment, LinkSource};

pub struct BreakableLink {
    tx: mpsc::Sender<Option<Vec<u8>>>,
    rx: mpsc::Receiver<Option<Vec<u8>>>,
}

#[derive(Clone)]
pub struct BreakHandle {
    tx: mpsc::Sender<Option<Vec<u8>>>,
}

pub fn breakable_link_pair(
    buffer: usize,
) -> (BreakableLink, BreakHandle, BreakableLink, BreakHandle) {
    let (tx_a, rx_b) = mpsc::channel("breakable_link.a→b", buffer);
    let (tx_b, rx_a) = mpsc::channel("breakable_link.b→a", buffer);

    let a_handle = BreakHandle { tx: tx_b.clone() };
    let b_handle = BreakHandle { tx: tx_a.clone() };

    (
        BreakableLink { tx: tx_a, rx: rx_a },
        a_handle,
        BreakableLink { tx: tx_b, rx: rx_b },
        b_handle,
    )
}

impl BreakHandle {
    pub async fn close(&self) {
        let _ = self.tx.send(None).await;
    }
}

impl Link for BreakableLink {
    type Tx = BreakableLinkTx;
    type Rx = BreakableLinkRx;

    fn split(self) -> (Self::Tx, Self::Rx) {
        (
            BreakableLinkTx { tx: self.tx },
            BreakableLinkRx { rx: self.rx },
        )
    }
}

#[derive(Clone)]
pub struct BreakableLinkTx {
    tx: mpsc::Sender<Option<Vec<u8>>>,
}

impl LinkTx for BreakableLinkTx {
    async fn send(&self, bytes: Vec<u8>) -> std::io::Result<()> {
        let permit = self.tx.clone().reserve_owned().await.map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::ConnectionReset, "receiver dropped")
        })?;
        drop(permit.send(Some(bytes)));
        Ok(())
    }

    async fn close(self) -> std::io::Result<()> {
        drop(self.tx);
        Ok(())
    }
}

pub struct BreakableLinkRx {
    rx: mpsc::Receiver<Option<Vec<u8>>>,
}

#[derive(Debug)]
pub struct BreakableLinkRxError;

impl std::fmt::Display for BreakableLinkRxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "breakable link rx error")
    }
}

impl std::error::Error for BreakableLinkRxError {}

impl LinkRx for BreakableLinkRx {
    type Error = BreakableLinkRxError;

    async fn recv(&mut self) -> Result<Option<Backing>, Self::Error> {
        match self.rx.recv().await {
            Some(Some(bytes)) => Ok(Some(Backing::Boxed(bytes.into_boxed_slice()))),
            Some(None) | None => Ok(None),
        }
    }
}

/// A [`LinkSource`] backed by a fixed list of pre-made links.
///
/// Each call to `next_link()` pops the next attachment. When exhausted,
/// returns an error (which causes the recoverer to give up).
pub struct TestLinkSource {
    attachments: VecDeque<Attachment<BreakableLink>>,
}

impl TestLinkSource {
    pub fn new(attachments: impl IntoIterator<Item = Attachment<BreakableLink>>) -> Self {
        Self {
            attachments: attachments.into_iter().collect(),
        }
    }
}

impl LinkSource for TestLinkSource {
    type Link = BreakableLink;

    async fn next_link(&mut self) -> std::io::Result<Attachment<Self::Link>> {
        self.attachments.pop_front().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "test link source exhausted",
            )
        })
    }
}
