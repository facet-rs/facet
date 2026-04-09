use std::future::IntoFuture;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use vox_core::{
    ConnectionAcceptor, ConnectionRequest, FromVoxSession, NoopClient, PendingConnection,
    SessionError, TransportMode, initiator,
};
use vox_types::{Link, MaybeSend, MaybeSync, Metadata, metadata_into_owned};

mod error;
pub use error::ServeError;

#[cfg(feature = "transport-tcp")]
mod tcp;

#[cfg(feature = "transport-local")]
mod local;

#[cfg(feature = "transport-websocket")]
mod ws;
#[cfg(feature = "transport-websocket")]
pub use ws::WsListener;

#[cfg(feature = "transport-websocket-tls")]
mod wss;
#[cfg(feature = "transport-websocket-tls")]
pub use wss::WssListener;

mod channel;
pub use channel::{ChannelListener, ChannelListenerSender};

/// A listener that accepts incoming connections for [`serve_listener()`].
pub trait VoxListener: MaybeSend + 'static {
    /// The link type produced by this listener.
    type Link: Link + MaybeSend + 'static;

    /// Accept the next incoming connection.
    fn accept(
        &mut self,
    ) -> impl std::future::Future<Output = std::io::Result<Self::Link>> + MaybeSend + '_;
}

/// Connect to a remote vox service, returning a typed client.
///
/// The address string determines the transport:
///
/// - `tcp://host:port` or bare `host:port` — TCP stream transport
/// - `local://path` — Unix socket / Windows named pipe
/// - `ws://host:port/path` — WebSocket transport
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
// r[impl rpc.session-setup]
pub fn connect<Client: FromVoxSession>(
    addr: impl std::fmt::Display,
) -> ConnectBuilder<'static, Client> {
    ConnectBuilder::new(addr.to_string())
}

enum ConnectAddress {
    Tcp(String),
    Local(String),
    #[cfg(feature = "transport-websocket")]
    Ws(String),
}

fn parse_connect_address(addr: String) -> Result<ConnectAddress, SessionError> {
    let (scheme, host) = match addr.split_once("://") {
        Some((scheme, host)) => (scheme.to_string(), host.to_string()),
        None => ("tcp".to_string(), addr),
    };

    match scheme.as_str() {
        #[cfg(feature = "transport-tcp")]
        "tcp" => Ok(ConnectAddress::Tcp(host)),
        #[cfg(feature = "transport-local")]
        "local" => Ok(ConnectAddress::Local(host)),
        #[cfg(feature = "transport-websocket")]
        "ws" | "wss" => Ok(ConnectAddress::Ws(format!("{scheme}://{host}"))),
        _ => Err(SessionError::Protocol(format!(
            "unknown transport scheme: {scheme:?}"
        ))),
    }
}

pub struct ConnectBuilder<'a, Client> {
    addr: String,
    metadata: Metadata<'a>,
    on_connection: Option<Arc<dyn ConnectionAcceptor>>,
    connect_timeout: Option<Duration>,
    resumable: bool,
    wait_for_service: Option<Duration>,
    _client: std::marker::PhantomData<Client>,
}

impl<'a, Client> ConnectBuilder<'a, Client> {
    fn new(addr: String) -> Self {
        Self {
            addr,
            metadata: vec![],
            on_connection: None,
            connect_timeout: Some(Duration::from_secs(5)),
            resumable: false,
            wait_for_service: None,
            _client: std::marker::PhantomData,
        }
    }

    // r[impl rpc.virtual-connection.accept]
    pub fn on_connection(mut self, acceptor: impl ConnectionAcceptor) -> Self {
        self.on_connection = Some(Arc::new(acceptor));
        self
    }

    pub fn metadata(mut self, metadata: Metadata<'a>) -> Self {
        self.metadata = metadata;
        self
    }

    pub fn connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = Some(timeout);
        self
    }

    pub fn resumable(mut self) -> Self {
        self.resumable = true;
        self
    }

    // r[impl session.initial-connect-waiting]
    /// Wait for the service to become reachable, retrying for up to `timeout`.
    ///
    /// Only transient failures (I/O errors, connect timeouts) are retried.
    /// Protocol errors, schema incompatibilities, and explicit rejections fail
    /// immediately without retrying.
    pub fn wait_for_service(mut self, timeout: Duration) -> Self {
        self.wait_for_service = Some(timeout);
        self
    }
}

const INITIAL_CONNECT_BACKOFF_MIN: Duration = Duration::from_millis(100);
const INITIAL_CONNECT_BACKOFF_MAX: Duration = Duration::from_secs(5);

impl<'a, Client> ConnectBuilder<'a, Client>
where
    Client: FromVoxSession,
{
    pub async fn establish(self) -> Result<Client, SessionError> {
        let ConnectBuilder {
            addr,
            metadata,
            on_connection,
            connect_timeout,
            resumable,
            wait_for_service,
            _client: _,
        } = self;

        let parsed = parse_connect_address(addr)?;
        let metadata = metadata_into_owned(metadata);

        match wait_for_service {
            // r[impl session.initial-connect-waiting]
            // r[impl session.initial-connect-waiting.no-session]
            Some(service_timeout) => {
                let deadline = Instant::now() + service_timeout;
                let mut backoff = INITIAL_CONNECT_BACKOFF_MIN;

                loop {
                    // r[impl session.initial-connect-waiting.timeout]
                    // Cap each attempt by the remaining waiting budget so a single
                    // slow attempt cannot exceed the caller-supplied timeout.
                    let now = Instant::now();
                    if now >= deadline {
                        return Err(SessionError::ConnectTimeout);
                    }
                    let remaining = deadline - now;

                    let attempt = Self::establish_once(
                        &parsed,
                        metadata.clone(),
                        on_connection.clone(),
                        connect_timeout,
                        resumable,
                    );
                    let result = match moire::time::timeout(remaining, attempt).await {
                        Ok(r) => r,
                        Err(_) => Err(SessionError::ConnectTimeout),
                    };

                    match result {
                        Ok(client) => return Ok(client),
                        // r[impl session.initial-connect-waiting.non-retryable]
                        Err(e)
                            if !matches!(e, SessionError::Io(_) | SessionError::ConnectTimeout) =>
                        {
                            return Err(e);
                        }
                        // r[impl session.initial-connect-waiting.retryable]
                        // r[impl session.initial-connect-waiting.backoff]
                        Err(e) => {
                            let now = Instant::now();
                            if now >= deadline {
                                return Err(e);
                            }
                            let remaining = deadline - now;
                            let sleep = backoff.min(remaining);
                            moire::time::sleep(sleep).await;
                            backoff = backoff.saturating_mul(2).min(INITIAL_CONNECT_BACKOFF_MAX);
                        }
                    }
                }
            }
            None => {
                Self::establish_once(&parsed, metadata, on_connection, connect_timeout, resumable)
                    .await
            }
        }
    }

    async fn establish_once(
        parsed: &ConnectAddress,
        metadata: vox_types::Metadata<'static>,
        on_connection: Option<Arc<dyn ConnectionAcceptor>>,
        connect_timeout: Option<Duration>,
        resumable: bool,
    ) -> Result<Client, SessionError> {
        match parsed {
            #[cfg(feature = "transport-tcp")]
            ConnectAddress::Tcp(host) => {
                let mut builder = initiator(
                    vox_stream::tcp_link_source(host.clone()),
                    TransportMode::Bare,
                );
                if let Some(acceptor) = on_connection.clone() {
                    builder = builder.on_connection(AcceptorRef(acceptor));
                }
                if let Some(timeout) = connect_timeout {
                    builder = builder.connect_timeout(timeout);
                }
                if resumable {
                    builder = builder.resumable();
                }
                builder.metadata(metadata).establish::<Client>().await
            }
            #[cfg(feature = "transport-local")]
            ConnectAddress::Local(host) => {
                let mut builder = initiator(
                    vox_stream::local_link_source(host.clone()),
                    TransportMode::Bare,
                );
                if let Some(acceptor) = on_connection.clone() {
                    builder = builder.on_connection(AcceptorRef(acceptor));
                }
                if let Some(timeout) = connect_timeout {
                    builder = builder.connect_timeout(timeout);
                }
                if resumable {
                    builder = builder.resumable();
                }
                builder.metadata(metadata).establish::<Client>().await
            }
            #[cfg(feature = "transport-websocket")]
            ConnectAddress::Ws(url) => {
                let mut builder = initiator(
                    vox_websocket::ws_link_source(url.clone()),
                    TransportMode::Bare,
                );
                if let Some(acceptor) = on_connection {
                    builder = builder.on_connection(AcceptorRef(acceptor));
                }
                if let Some(timeout) = connect_timeout {
                    builder = builder.connect_timeout(timeout);
                }
                if resumable {
                    builder = builder.resumable();
                }
                builder.metadata(metadata).establish::<Client>().await
            }
            #[allow(unreachable_patterns)]
            _ => Err(SessionError::Protocol(
                "transport not enabled in this vox build".to_string(),
            )),
        }
    }
}

impl<'a, Client> IntoFuture for ConnectBuilder<'a, Client>
where
    Client: FromVoxSession + 'a,
{
    type Output = Result<Client, SessionError>;
    type IntoFuture = Pin<Box<dyn Future<Output = Self::Output> + 'a>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(self.establish())
    }
}

/// Serve a vox service by address string, accepting connections in a loop.
///
/// The address string determines the transport:
///
/// - `tcp://host:port` or bare `host:port` — TCP stream transport
/// - `local://path` — Unix socket / Windows named pipe
/// - `ws://host:port` — WebSocket (accepts TCP, upgrades to WS)
/// - `wss://host:port?cert=/path/to/cert.pem&key=/path/to/key.pem` — WebSocket over TLS
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
        "local" => local::serve_local(&host, acceptor).await,
        #[cfg(feature = "transport-websocket")]
        "ws" => {
            let listener = WsListener::bind(&host).await?;
            Ok(serve_listener(listener, acceptor).await?)
        }
        #[cfg(feature = "transport-websocket-tls")]
        "wss" => wss::serve_wss(&host, acceptor).await,
        _ => Err(ServeError::UnsupportedScheme { scheme }),
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
    mut listener: L,
    acceptor: impl ConnectionAcceptor,
) -> Result<(), SessionError>
where
    L: VoxListener,
    <L::Link as Link>::Tx: MaybeSend + MaybeSync + 'static,
    <L::Link as Link>::Rx: MaybeSend + 'static,
{
    let acceptor: Arc<dyn ConnectionAcceptor> = Arc::new(acceptor);
    loop {
        let link = listener.accept().await.map_err(SessionError::Io)?;
        let acceptor = acceptor.clone();
        moire::spawn(async move {
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
        request: &ConnectionRequest,
        connection: PendingConnection,
    ) -> Result<(), Metadata<'static>> {
        self.0.accept(request, connection)
    }
}
