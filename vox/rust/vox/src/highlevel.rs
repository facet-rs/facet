use vox_core::{FromVoxSession, SessionError, TransportMode, initiator};

/// Connect to a remote vox service, returning a typed client.
///
/// The address string determines the transport:
///
/// - `tcp://host:port` or `host:port` — TCP stream transport
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
pub async fn connect<Client: FromVoxSession>(addr: &str) -> Result<Client, SessionError> {
    let (scheme, host) = match addr.split_once("://") {
        Some((scheme, host)) => (scheme, host),
        None => ("tcp", addr),
    };

    match scheme {
        #[cfg(feature = "transport-tcp")]
        "tcp" => {
            let (client, _session) =
                initiator(vox_stream::tcp_connector(host), TransportMode::Bare)
                    .establish::<Client>(())
                    .await?;
            Ok(client)
        }
        #[cfg(feature = "transport-local")]
        "local" => {
            let (client, _session) =
                initiator(vox_stream::local_link_source(host), TransportMode::Bare)
                    .establish::<Client>(())
                    .await?;
            Ok(client)
        }
        _ => Err(SessionError::Protocol(format!(
            "unsupported transport scheme: {scheme:?}"
        ))),
    }
}
