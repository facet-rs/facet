use std::sync::Arc;
use std::time::Duration;

use vox_core::{
    ConnectionAcceptor, FromVoxSession, LinkSource, NoopClient, SessionError, TransportMode,
    initiator,
};

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

#[cfg(all(unix, feature = "transport-shm"))]
mod shm;
#[cfg(all(unix, feature = "transport-shm"))]
pub use shm::{ShmListener, ShmListenerConfig};

mod channel;
pub use channel::{ChannelListener, ChannelListenerSender};

/// A listener that accepts incoming connections for [`serve_listener()`].
pub trait VoxListener: Send + 'static {
    /// The link type produced by this listener.
    type Link: vox_types::Link + Send + 'static;

    /// Accept the next incoming connection.
    fn accept(&self) -> impl std::future::Future<Output = std::io::Result<Self::Link>> + Send + '_;
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
/// - `wss://host:port?cert=/path/to/cert.pem&key=/path/to/key.pem` — WebSocket over TLS
/// - `shm:///path/to/control.sock` — Shared-memory transport (Unix only)
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
        #[cfg(all(unix, feature = "transport-shm"))]
        "shm" => {
            let listener = ShmListener::bind(&host)?;
            Ok(serve_listener(listener, acceptor).await?)
        }
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
