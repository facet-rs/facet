#![deny(unsafe_code)]

use std::{io, sync::Arc};

use iroh::{
    Endpoint, EndpointAddr, EndpointId,
    endpoint::{Connection, RecvStream, SendStream},
};
use tokio::task::JoinSet;
use vox_core::{Attachment, LinkSource};
use vox_stream::{StreamLink, StreamLinkRx, StreamLinkTx};
use vox_types::{
    Backing, Link, LinkRx, LinkTx, PeerEvidence, PeerEvidenceItem, PublicKeyAlgorithm,
};

/// Versioned application protocol negotiated before any Vox bytes are sent.
// r[impl transport.iroh.alpn]
pub const ALPN: &[u8] = b"vox/iroh/1";

type InnerLink = StreamLink<RecvStream, SendStream>;
type InnerRx = StreamLinkRx<tokio::io::BufReader<RecvStream>>;

/// One Vox link carried by one Iroh bidirectional stream.
// r[impl transport.iroh.link]
// r[impl transport.iroh.path-equivalence]
pub struct IrohLink {
    inner: InnerLink,
    connection: Arc<Connection>,
}

impl IrohLink {
    fn new(connection: Connection, send: SendStream, recv: RecvStream) -> Self {
        Self {
            inner: StreamLink::new(recv, send),
            connection: Arc::new(connection),
        }
    }
}

impl Link for IrohLink {
    type Tx = IrohLinkTx;
    type Rx = IrohLinkRx;

    fn split(self) -> (Self::Tx, Self::Rx) {
        let (inner_tx, inner_rx) = self.inner.split();
        (
            IrohLinkTx {
                inner: inner_tx,
                connection: self.connection.clone(),
            },
            IrohLinkRx {
                inner: inner_rx,
                connection: self.connection,
            },
        )
    }
}

/// Transmit half of an [`IrohLink`].
pub struct IrohLinkTx {
    inner: StreamLinkTx,
    connection: Arc<Connection>,
}

impl LinkTx for IrohLinkTx {
    // r[impl transport.iroh.cancel-safe]
    async fn send(&self, bytes: Vec<u8>) -> io::Result<()> {
        self.inner.send(bytes).await
    }

    // r[impl transport.iroh.close]
    async fn close(self) -> io::Result<()> {
        let Self { inner, connection } = self;
        let result = inner.close().await;
        drop(connection);
        result
    }
}

/// Receive half of an [`IrohLink`].
pub struct IrohLinkRx {
    inner: InnerRx,
    connection: Arc<Connection>,
}

impl LinkRx for IrohLinkRx {
    type Error = io::Error;

    async fn recv(&mut self) -> io::Result<Option<Backing>> {
        let _keep_connection_alive = &self.connection;
        self.inner.recv().await
    }
}

/// Reusable outbound source of authenticated Vox-over-Iroh links.
pub struct IrohLinkSource {
    endpoint: Endpoint,
    remote: EndpointAddr,
}

impl IrohLinkSource {
    #[must_use]
    pub fn new(endpoint: Endpoint, remote: impl Into<EndpointAddr>) -> Self {
        Self {
            endpoint,
            remote: remote.into(),
        }
    }
}

impl LinkSource for IrohLinkSource {
    type Link = IrohLink;

    async fn next_link(&mut self) -> io::Result<Attachment<Self::Link>> {
        let remote = self.remote.id;
        tracing::debug!(remote = %remote.fmt_short(), "dialing Vox-over-Iroh peer");
        let connection = self
            .endpoint
            .connect(self.remote.clone(), ALPN)
            .await
            .map_err(io_error)?;
        let (send, recv) = connection.open_bi().await.map_err(io_error)?;
        tracing::debug!(remote = %remote.fmt_short(), "opened Vox-over-Iroh link");
        Ok(attachment(connection, send, recv))
    }
}

/// Incoming source for [`vox::serve_listener`].
///
/// Each accepted QUIC connection is allowed to wait for its first
/// bidirectional stream independently, so one stalled peer cannot stop the
/// endpoint from accepting other peers.
pub struct IrohListener {
    endpoint: Endpoint,
    pending: JoinSet<io::Result<Attachment<IrohLink>>>,
    endpoint_closed: bool,
}

impl IrohListener {
    #[must_use]
    pub fn new(endpoint: Endpoint) -> Self {
        Self {
            endpoint,
            pending: JoinSet::new(),
            endpoint_closed: false,
        }
    }

    #[must_use]
    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }
}

impl vox::VoxListener for IrohListener {
    type Link = IrohLink;

    async fn accept(&mut self) -> io::Result<Attachment<Self::Link>> {
        loop {
            tokio::select! {
                incoming = self.endpoint.accept(), if !self.endpoint_closed => {
                    let Some(incoming) = incoming else {
                        self.endpoint_closed = true;
                        continue;
                    };
                    self.pending.spawn(async move {
                        let connection = incoming.accept().map_err(io_error)?.await.map_err(io_error)?;
                        let remote = connection.remote_id();
                        let (send, recv) = connection.accept_bi().await.map_err(io_error)?;
                        tracing::debug!(remote = %remote.fmt_short(), "accepted Vox-over-Iroh link");
                        Ok(attachment(connection, send, recv))
                    });
                }
                completed = self.pending.join_next(), if !self.pending.is_empty() => {
                    match completed {
                        Some(Ok(Ok(attachment))) => return Ok(attachment),
                        Some(Ok(Err(error))) => {
                            tracing::debug!(?error, "discarding failed Vox-over-Iroh connection");
                        }
                        Some(Err(error)) => {
                            tracing::warn!(?error, "Vox-over-Iroh accept task failed");
                        }
                        None => {}
                    }
                }
                else => {
                    return Err(io::Error::new(io::ErrorKind::BrokenPipe, "Iroh endpoint closed"));
                }
            }
        }
    }
}

fn attachment(connection: Connection, send: SendStream, recv: RecvStream) -> Attachment<IrohLink> {
    let remote_id = connection.remote_id();
    let evidence = endpoint_evidence(remote_id);
    Attachment::initiator(IrohLink::new(connection, send, recv)).with_runtime_evidence(evidence)
}

// r[impl transport.iroh.evidence]
fn endpoint_evidence(remote_id: EndpointId) -> PeerEvidence {
    // `remote_id` is produced by Iroh only after its mutually authenticated TLS
    // handshake. This module is trusted transport code and does not accept the
    // value from Vox metadata or application payloads.
    #[allow(unsafe_code)]
    unsafe {
        PeerEvidence::from_runtime_asserted(vec![PeerEvidenceItem::PublicKey {
            algorithm: PublicKeyAlgorithm::Ed25519,
            bytes: *remote_id.as_bytes(),
        }])
    }
}

fn io_error(error: impl std::fmt::Display) -> io::Error {
    io::Error::other(error.to_string())
}
