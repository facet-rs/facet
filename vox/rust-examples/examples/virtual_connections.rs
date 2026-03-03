use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};

use eyre::{Result, WrapErr, eyre};
use roam::{AcceptedConnection, ConnectionAcceptor, ConnectionSettings, Driver, Parity};
use roam_stream::StreamLink;

#[roam::service]
trait CounterLab {
    async fn bump(&self) -> u32;
    async fn echo(&self, value: String) -> String;
}

#[derive(Clone)]
struct CounterLabService {
    count: Arc<AtomicU32>,
}

impl CounterLabService {
    fn new() -> Self {
        Self {
            count: Arc::new(AtomicU32::new(0)),
        }
    }
}

impl CounterLab for CounterLabService {
    async fn bump(&self) -> u32 {
        self.count.fetch_add(1, Ordering::Relaxed) + 1
    }

    async fn echo(&self, value: String) -> String {
        format!("echo:{value}")
    }
}

struct CounterLabAcceptor;

impl ConnectionAcceptor for CounterLabAcceptor {
    fn accept(
        &self,
        _conn_id: roam_types::ConnectionId,
        peer_settings: &ConnectionSettings,
        _metadata: &[roam_types::MetadataEntry],
    ) -> Result<AcceptedConnection, roam_types::Metadata<'static>> {
        let peer_parity = peer_settings.parity;
        Ok(AcceptedConnection {
            settings: ConnectionSettings {
                parity: peer_parity.other(),
                max_concurrent_requests: 64,
            },
            metadata: vec![],
            setup: Box::new(|handle| {
                let mut driver =
                    Driver::new(handle, CounterLabDispatcher::new(CounterLabService::new()));
                tokio::spawn(async move { driver.run().await });
            }),
        })
    }
}

fn main() -> Result<()> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .wrap_err("building Tokio runtime")?;
    rt.block_on(run_demo())
}

async fn run_demo() -> Result<()> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .wrap_err("binding TCP listener")?;
    let addr = listener.local_addr().wrap_err("reading listener addr")?;

    let server_task = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.expect("accept");
        let _ = roam::acceptor(StreamLink::tcp(socket))
            .on_connection(CounterLabAcceptor)
            .establish::<()>(())
            .await
            .expect("server establish");
    });

    let socket = tokio::net::TcpStream::connect(addr)
        .await
        .wrap_err("connecting client socket")?;
    let ((), session_handle) = roam::initiator(StreamLink::tcp(socket))
        .establish::<()>(())
        .await
        .map_err(|e| eyre!("failed to establish initiator session: {e:?}"))?;
    server_task.await.wrap_err("joining server_task")?;

    let settings = ConnectionSettings {
        parity: Parity::Odd,
        max_concurrent_requests: 64,
    };

    let conn_a = session_handle
        .open_connection(settings.clone(), vec![])
        .await
        .map_err(|e| eyre!("open_connection(conn_a) failed: {e:?}"))?;
    let conn_a_id = conn_a.connection_id();
    let mut driver_a = Driver::new(conn_a, ());
    let client_a = CounterLabClient::from(driver_a.caller());
    let driver_a_task = tokio::spawn(async move { driver_a.run().await });

    let conn_b = session_handle
        .open_connection(settings, vec![])
        .await
        .map_err(|e| eyre!("open_connection(conn_b) failed: {e:?}"))?;
    let conn_b_id = conn_b.connection_id();
    let mut driver_b = Driver::new(conn_b, ());
    let client_b = CounterLabClient::from(driver_b.caller());
    let driver_b_task = tokio::spawn(async move { driver_b.run().await });

    assert_eq!(
        client_a
            .bump()
            .await
            .map_err(|e| eyre!("client_a.bump #1 failed: {e:?}"))?,
        1
    );
    assert_eq!(
        client_a
            .bump()
            .await
            .map_err(|e| eyre!("client_a.bump #2 failed: {e:?}"))?,
        2
    );
    assert_eq!(
        client_b
            .bump()
            .await
            .map_err(|e| eyre!("client_b.bump #1 failed: {e:?}"))?,
        1
    );
    assert_eq!(
        client_a
            .echo("alpha".to_string())
            .await
            .map_err(|e| eyre!("client_a.echo failed: {e:?}"))?,
        "echo:alpha"
    );
    assert_eq!(
        client_b
            .echo("beta".to_string())
            .await
            .map_err(|e| eyre!("client_b.echo failed: {e:?}"))?,
        "echo:beta"
    );

    session_handle
        .close_connection(conn_a_id, vec![])
        .await
        .map_err(|e| eyre!("close_connection(conn_a) failed: {e:?}"))?;
    session_handle
        .close_connection(conn_b_id, vec![])
        .await
        .map_err(|e| eyre!("close_connection(conn_b) failed: {e:?}"))?;

    driver_a_task.abort();
    driver_b_task.abort();

    Ok(())
}
