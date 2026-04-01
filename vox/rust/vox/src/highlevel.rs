use vox_core::{FromVoxSession, LinkSource, SessionError, TransportMode, initiator};

/// Connect to a remote vox service, returning a typed client.
///
/// The address string determines the transport:
///
/// - `tcp://host:port` or bare `host:port` — TCP stream transport
/// - `local://path` — Unix socket / Windows named pipe
/// - `ws://host:port/path` — WebSocket transport
/// - `shm://name` — Shared-memory transport
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
        "tcp" => {
            use std::net::ToSocketAddrs;
            let addr = host
                .to_socket_addrs()
                .map_err(|e| SessionError::Protocol(format!("invalid TCP address {host:?}: {e}")))?
                .next()
                .ok_or_else(|| {
                    SessionError::Protocol(format!("no addresses found for {host:?}"))
                })?;
            connect_bare(vox_stream::tcp_link_source(addr)).await
        }
        #[cfg(feature = "transport-local")]
        "local" => connect_bare(vox_stream::local_link_source(host)).await,
        #[cfg(feature = "transport-websocket")]
        "ws" | "wss" => {
            let url = format!("{scheme}://{host}");
            connect_bare(vox_websocket::ws_link_source(url)).await
        }
        "shm" => Err(SessionError::Protocol(
            "shared-memory transport is not yet supported by connect()".into(),
        )),
        _ => Err(SessionError::Protocol(format!(
            "unknown transport scheme: {scheme:?}"
        ))),
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
    let (client, _session) = initiator(source, TransportMode::Bare)
        .establish::<Client>(())
        .await?;
    Ok(client)
}
