use std::sync::Arc;
use std::time::Duration;

use vox_core::{
    ConnectionAcceptor, FromVoxSession, LinkSource, NoopClient, SessionError, TransportMode,
    initiator,
};

/// Error returned by [`serve()`].
#[derive(Debug)]
pub enum ServeError {
    /// I/O error (bind failure, etc.).
    Io(std::io::Error),
    /// Another healthy process is already serving on this address.
    AddrInUse { addr: String },
    /// Another process holds the lock but is not responding to connections.
    /// It may be deadlocked or hung.
    LockHeldUnhealthy { addr: String },
    /// Unknown or unsupported transport scheme.
    UnsupportedScheme { scheme: String },
    /// Session-level error from the accept loop.
    Session(SessionError),
}

impl std::fmt::Display for ServeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "io error: {e}"),
            Self::AddrInUse { addr } => {
                write!(f, "another healthy process is already serving on {addr}")
            }
            Self::LockHeldUnhealthy { addr } => write!(
                f,
                "another process holds the lock on {addr} but is not responding"
            ),
            Self::UnsupportedScheme { scheme } => {
                write!(f, "unsupported transport scheme: {scheme:?}")
            }
            Self::Session(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ServeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Session(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for ServeError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<SessionError> for ServeError {
    fn from(e: SessionError) -> Self {
        Self::Session(e)
    }
}

/// Connect to a remote vox service, returning a typed client.
///
/// The address string determines the transport:
///
/// - `tcp://host:port` or bare `host:port` — TCP stream transport
/// - `local://path` — Unix socket / Windows named pipe
/// - `ws://host:port/path` — WebSocket transport
/// - `shm:///path/to/control.sock` — Shared-memory transport (Unix only)
///
/// # Examples
///
/// ```no_run
/// # #[vox::service]
/// # trait Hello {
/// #     async fn say_hello(&self) -> String;
/// # }
/// # #[tokio::main(flavor = "current_thread")]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let client: HelloClient = vox::connect("127.0.0.1:9000").await?;
/// let reply = client.say_hello().await?;
/// # Ok(())
/// # }
/// ```
pub async fn connect<Client: FromVoxSession>(
    addr: impl std::fmt::Display,
) -> Result<Client, SessionError> {
    let addr = addr.to_string();
    let (scheme, host) = match addr.split_once("://") {
        Some((scheme, host)) => (scheme.to_string(), host.to_string()),
        None => ("tcp".to_string(), addr),
    };

    match scheme.as_str() {
        #[cfg(feature = "transport-tcp")]
        "tcp" => connect_bare(vox_stream::tcp_link_source(host)).await,
        #[cfg(feature = "transport-local")]
        "local" => connect_bare(vox_stream::local_link_source(host)).await,
        #[cfg(feature = "transport-websocket")]
        "ws" | "wss" => {
            let url = format!("{scheme}://{host}");
            connect_bare(vox_websocket::ws_link_source(url)).await
        }
        #[cfg(all(unix, feature = "transport-shm"))]
        "shm" => connect_bare(vox_shm::bootstrap::shm_link_source(host)).await,
        _ => Err(SessionError::Protocol(format!(
            "unknown transport scheme: {scheme:?}"
        ))),
    }
}

/// Serve a vox service by address string, accepting connections in a loop.
///
/// The address string determines the transport:
///
/// - `tcp://host:port` or bare `host:port` — TCP stream transport
/// - `local://path` — Unix socket / Windows named pipe
/// - `ws://host:port` — WebSocket (accepts TCP, upgrades to WS)
///
/// This function runs forever (or until an I/O error occurs). Each incoming
/// connection is handled in a spawned task.
///
/// # Examples
///
/// ```no_run
/// # #[vox::service]
/// # trait Hello {
/// #     async fn say_hello(&self) -> String;
/// # }
/// # #[derive(Clone)]
/// # struct HelloService;
/// # impl Hello for HelloService {
/// #     async fn say_hello(&self) -> String { "hi".into() }
/// # }
/// # #[tokio::main(flavor = "current_thread")]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// vox::serve("0.0.0.0:9000", HelloDispatcher::new(HelloService)).await?;
/// # Ok(())
/// # }
/// ```
pub async fn serve(
    addr: impl std::fmt::Display,
    acceptor: impl ConnectionAcceptor,
) -> Result<(), ServeError> {
    let addr = addr.to_string();
    let (scheme, host) = match addr.split_once("://") {
        Some((scheme, host)) => (scheme.to_string(), host.to_string()),
        None => ("tcp".to_string(), addr),
    };

    match scheme.as_str() {
        #[cfg(feature = "transport-tcp")]
        "tcp" => {
            let listener = tokio::net::TcpListener::bind(&host).await?;
            Ok(serve_listener(listener, acceptor).await?)
        }
        #[cfg(feature = "transport-local")]
        "local" => {
            let lock = match vox_stream::try_local_lock(&host)? {
                vox_stream::LocalLockOutcome::Acquired(lock) => {
                    // We own it — clean up stale socket.
                    let _ = std::fs::remove_file(&host);
                    lock
                }
                vox_stream::LocalLockOutcome::Held => {
                    // Another process holds the lock. Health-check: try connecting
                    // and doing a handshake. If it responds, it's alive.
                    let health = tokio::time::timeout(Duration::from_secs(5), async {
                        let source = vox_stream::local_link_source(&host);
                        initiator(source, TransportMode::Bare)
                            .establish::<NoopClient>()
                            .await
                    })
                    .await;
                    return match health {
                        Ok(Ok(_client)) => Err(ServeError::AddrInUse { addr: host }),
                        _ => Err(ServeError::LockHeldUnhealthy { addr: host }),
                    };
                }
            };
            let listener = vox_stream::LocalLinkAcceptor::bind(&host)?;
            let _lock = lock;
            Ok(serve_listener(listener, acceptor).await?)
        }
        #[cfg(feature = "transport-websocket")]
        "ws" => {
            let listener = WsListener::bind(&host).await?;
            Ok(serve_listener(listener, acceptor).await?)
        }
        _ => Err(ServeError::UnsupportedScheme { scheme }),
    }
}

/// A listener that accepts incoming connections for [`serve_listener()`].
pub trait VoxListener: Send + 'static {
    /// The link type produced by this listener.
    type Link: vox_types::Link + Send + 'static;

    /// Accept the next incoming connection.
    fn accept(&self) -> impl std::future::Future<Output = std::io::Result<Self::Link>> + Send + '_;
}

#[cfg(feature = "transport-tcp")]
impl VoxListener for tokio::net::TcpListener {
    type Link =
        vox_stream::StreamLink<tokio::net::tcp::OwnedReadHalf, tokio::net::tcp::OwnedWriteHalf>;

    async fn accept(&self) -> std::io::Result<Self::Link> {
        let (stream, _addr) = tokio::net::TcpListener::accept(self).await?;
        Ok(vox_stream::StreamLink::tcp(stream))
    }
}

#[cfg(feature = "transport-local")]
impl VoxListener for vox_stream::LocalLinkAcceptor {
    type Link = vox_stream::LocalLink;

    async fn accept(&self) -> std::io::Result<Self::Link> {
        vox_stream::LocalLinkAcceptor::accept(self).await
    }
}

/// A [`VoxListener`] that accepts TCP connections and upgrades them to WebSocket.
#[cfg(feature = "transport-websocket")]
pub struct WsListener {
    tcp: tokio::net::TcpListener,
}

#[cfg(feature = "transport-websocket")]
impl WsListener {
    /// Bind a WebSocket listener to the given TCP address.
    pub async fn bind(addr: impl tokio::net::ToSocketAddrs) -> std::io::Result<Self> {
        let tcp = tokio::net::TcpListener::bind(addr).await?;
        Ok(Self { tcp })
    }

    /// Wrap an existing `TcpListener` as a WebSocket listener.
    pub fn from_tcp(tcp: tokio::net::TcpListener) -> Self {
        Self { tcp }
    }
}

#[cfg(feature = "transport-websocket")]
impl VoxListener for WsListener {
    type Link = vox_websocket::WsLink<tokio::net::TcpStream>;

    async fn accept(&self) -> std::io::Result<Self::Link> {
        let (stream, _addr) = self.tcp.accept().await?;
        vox_websocket::WsLink::server(stream).await
    }
}

/// A [`VoxListener`] backed by a channel.
///
/// Use this when you control how connections arrive (e.g. from an axum
/// WebSocket upgrade handler) and want to feed them into [`serve_listener()`].
///
/// # Example
///
/// ```ignore
/// let (listener, sender) = vox::ChannelListener::new(16);
///
/// // In your axum handler, push upgraded links:
/// sender.send(WsLink::new(ws_stream)).await;
///
/// // Serve from the channel:
/// vox::serve_listener(listener, dispatcher).await?;
/// ```
pub struct ChannelListener<L> {
    rx: tokio::sync::Mutex<tokio::sync::mpsc::Receiver<L>>,
}

/// Sender half of a [`ChannelListener`].
#[derive(Clone)]
pub struct ChannelListenerSender<L> {
    tx: tokio::sync::mpsc::Sender<L>,
}

impl<L: vox_types::Link + Send + 'static> ChannelListener<L> {
    /// Create a new channel listener with the given buffer capacity.
    pub fn new(buffer: usize) -> (Self, ChannelListenerSender<L>) {
        let (tx, rx) = tokio::sync::mpsc::channel(buffer);
        (
            Self {
                rx: tokio::sync::Mutex::new(rx),
            },
            ChannelListenerSender { tx },
        )
    }
}

impl<L: vox_types::Link + Send + 'static> ChannelListenerSender<L> {
    /// Send a link to the listener.
    pub async fn send(&self, link: L) -> Result<(), tokio::sync::mpsc::error::SendError<L>> {
        self.tx.send(link).await
    }
}

impl<L> VoxListener for ChannelListener<L>
where
    L: vox_types::Link + Send + 'static,
{
    type Link = L;

    async fn accept(&self) -> std::io::Result<Self::Link> {
        self.rx
            .lock()
            .await
            .recv()
            .await
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::BrokenPipe, "channel closed"))
    }
}

/// Serve a vox service on a pre-bound listener.
///
/// Takes a [`VoxListener`] (e.g. `TcpListener`) and a [`ConnectionAcceptor`].
/// Each incoming connection is handled in a spawned task. Runs until an I/O
/// error occurs on the listener.
///
/// # Examples
///
/// ```no_run
/// # #[vox::service]
/// # trait Hello {
/// #     async fn say_hello(&self) -> String;
/// # }
/// # #[derive(Clone)]
/// # struct HelloService;
/// # impl Hello for HelloService {
/// #     async fn say_hello(&self) -> String { "hi".into() }
/// # }
/// # #[tokio::main(flavor = "current_thread")]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let listener = tokio::net::TcpListener::bind("0.0.0.0:9000").await?;
/// vox::serve_listener(listener, HelloDispatcher::new(HelloService)).await?;
/// # Ok(())
/// # }
/// ```
pub async fn serve_listener<L>(
    listener: L,
    acceptor: impl ConnectionAcceptor,
) -> Result<(), SessionError>
where
    L: VoxListener,
    <L::Link as vox_types::Link>::Tx: vox_types::MaybeSend + vox_types::MaybeSync + Send + 'static,
    <<L::Link as vox_types::Link>::Tx as vox_types::LinkTx>::Permit: vox_types::MaybeSend,
    <L::Link as vox_types::Link>::Rx: vox_types::MaybeSend + Send + 'static,
{
    let acceptor: Arc<dyn ConnectionAcceptor> = Arc::new(acceptor);
    loop {
        let link = listener.accept().await.map_err(SessionError::Io)?;
        let acceptor = acceptor.clone();
        tokio::spawn(async move {
            let result = vox_core::acceptor_on(link)
                .on_connection(AcceptorRef(acceptor))
                .establish::<NoopClient>()
                .await;
            if let Ok(client) = result {
                client.caller.closed().await;
            }
        });
    }
}

/// Wrapper that implements `ConnectionAcceptor` by delegating to an `Arc<dyn ConnectionAcceptor>`.
struct AcceptorRef(Arc<dyn ConnectionAcceptor>);

impl ConnectionAcceptor for AcceptorRef {
    fn accept(
        &self,
        request: &vox_core::ConnectionRequest,
        connection: vox_core::PendingConnection,
    ) -> Result<(), vox_types::Metadata<'static>> {
        self.0.accept(request, connection)
    }
}

async fn connect_bare<Client, S>(source: S) -> Result<Client, SessionError>
where
    Client: FromVoxSession,
    S: LinkSource,
    S::Link: vox_types::Link + Send + 'static,
    <S::Link as vox_types::Link>::Tx: vox_types::MaybeSend + vox_types::MaybeSync + Send + 'static,
    <<S::Link as vox_types::Link>::Tx as vox_types::LinkTx>::Permit: vox_types::MaybeSend,
    <S::Link as vox_types::Link>::Rx: vox_types::MaybeSend + Send + 'static,
{
    let client = initiator(source, TransportMode::Bare)
        .connect_timeout(Duration::from_secs(5))
        .establish::<Client>()
        .await?;
    Ok(client)
}
