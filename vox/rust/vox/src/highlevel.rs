use vox_core::{FromVoxSession, LinkSource, SessionError, TransportMode, initiator};

/// Connect to a remote vox service, returning a typed client.
///
/// The address string determines the transport:
///
/// - `tcp://host:port` — TCP stream transport
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
/// let client: HelloClient = vox::connect("tcp://127.0.0.1:9000").await?;
/// let reply = client.say_hello().await?;
/// # Ok(())
/// # }
/// ```
pub async fn connect<Client: FromVoxSession>(addr: &str) -> Result<Client, SessionError> {
    let Some((scheme, host)) = addr.split_once("://") else {
        return Err(SessionError::Protocol(format!(
            "invalid address, expected scheme://host: {addr:?}"
        )));
    };

    match scheme {
        #[cfg(feature = "transport-tcp")]
        "tcp" => connect_bare(vox_stream::tcp_connector(host)).await,
        #[cfg(feature = "transport-local")]
        "local" => connect_bare(vox_stream::local_link_source(host)).await,
        _ => Err(SessionError::Protocol(format!(
            "unsupported transport scheme: {scheme:?}"
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
