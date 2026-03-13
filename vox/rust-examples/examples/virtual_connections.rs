use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};

use eyre::{Result, WrapErr, eyre};
use roam::{
    AcceptedConnection, ConnectionAcceptor, ConnectionId, ConnectionSettings, Driver, Metadata,
    MetadataEntry, MetadataFlags, MetadataValue, Parity,
};
use roam_stream::StreamLink;

#[roam::service]
trait CounterLab {
    async fn bump(&self) -> u32;
    async fn echo(&self, value: String) -> String;
}

#[roam::service]
trait StringLab {
    async fn shout(&self, value: String) -> String;
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

#[derive(Clone, Copy)]
struct StringLabService;

impl StringLab for StringLabService {
    async fn shout(&self, value: String) -> String {
        value.to_uppercase()
    }
}

struct CounterLabAcceptor;

impl ConnectionAcceptor for CounterLabAcceptor {
    fn accept(
        &self,
        _conn_id: ConnectionId,
        peer_settings: &ConnectionSettings,
        metadata: &[MetadataEntry],
    ) -> Result<AcceptedConnection, Metadata<'static>> {
        let peer_parity = peer_settings.parity;
        let settings = ConnectionSettings {
            parity: peer_parity.other(),
            max_concurrent_requests: 64,
        };

        match requested_service(metadata) {
            Some("counter") => Ok(AcceptedConnection {
                settings,
                metadata: vec![],
                setup: Box::new(|handle| {
                    println!("[server] accepted vconn as CounterLab");
                    let mut driver =
                        Driver::new(handle, CounterLabDispatcher::new(CounterLabService::new()));
                    tokio::spawn(async move { driver.run().await });
                }),
            }),
            Some("string") => Ok(AcceptedConnection {
                settings,
                metadata: vec![],
                setup: Box::new(|handle| {
                    println!("[server] accepted vconn as StringLab");
                    let mut driver =
                        Driver::new(handle, StringLabDispatcher::new(StringLabService));
                    tokio::spawn(async move { driver.run().await });
                }),
            }),
            _ => Err(vec![MetadataEntry {
                key: "error",
                value: MetadataValue::String("unknown or missing service metadata"),
                flags: MetadataFlags::NONE,
            }]),
        }
    }
}

fn requested_service<'a>(metadata: &'a [MetadataEntry<'a>]) -> Option<&'a str> {
    metadata
        .iter()
        .find(|entry| entry.key == "service")
        .and_then(|entry| match entry.value {
            MetadataValue::String(value) => Some(value),
            _ => None,
        })
}

fn service_metadata(service: &'static str) -> Metadata<'static> {
    vec![MetadataEntry {
        key: "service",
        value: MetadataValue::String(service),
        flags: MetadataFlags::NONE,
    }]
}

fn main() -> Result<()> {
    println!("[demo] virtual_connections: starting runtime");
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .wrap_err("building Tokio runtime")?;
    rt.block_on(run_demo())
}

async fn run_demo() -> Result<()> {
    println!("[demo] binding TCP listener");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .wrap_err("binding TCP listener")?;
    let addr = listener.local_addr().wrap_err("reading listener addr")?;
    println!("[demo] listening on {addr}");
    let (server_ready_tx, server_ready_rx) = tokio::sync::oneshot::channel::<()>();

    let server_task = tokio::spawn(async move {
        println!("[server] waiting for client");
        let (socket, _) = listener.accept().await.expect("accept");
        println!("[server] client connected; establishing root session");
        let (server_root_guard, _) = roam::acceptor_on(StreamLink::tcp(socket))
            .on_connection(CounterLabAcceptor)
            .establish::<roam::DriverCaller>(())
            .await
            .expect("server establish");
        let _ = server_ready_tx.send(());
        let _server_root_guard = server_root_guard;
        std::future::pending::<()>().await;
    });

    println!("[client] connecting");
    let socket = tokio::net::TcpStream::connect(addr)
        .await
        .wrap_err("connecting client socket")?;
    let (_root_caller_guard, session_handle) =
        roam::initiator_on(StreamLink::tcp(socket), roam::TransportMode::Bare)
            .establish::<roam::DriverCaller>(())
            .await
            .map_err(|e| eyre!("failed to establish initiator session: {e:?}"))?;
    println!("[client] root session established");
    server_ready_rx
        .await
        .map_err(|_| eyre!("server task ended before signaling readiness"))?;

    let settings = ConnectionSettings {
        parity: Parity::Odd,
        max_concurrent_requests: 64,
    };

    println!("[client] opening counter virtual connection");
    let counter_conn = session_handle
        .open_connection(settings.clone(), service_metadata("counter"))
        .await
        .map_err(|e| eyre!("open_connection(counter) failed: {e:?}"))?;
    let counter_conn_id = counter_conn.connection_id();
    let mut counter_driver = Driver::new(counter_conn, ());
    let counter_client = CounterLabClient::from(counter_driver.caller());
    let counter_driver_task = tokio::spawn(async move { counter_driver.run().await });

    println!("[client] opening string virtual connection");
    let string_conn = session_handle
        .open_connection(settings, service_metadata("string"))
        .await
        .map_err(|e| eyre!("open_connection(string) failed: {e:?}"))?;
    let string_conn_id = string_conn.connection_id();
    let mut string_driver = Driver::new(string_conn, ());
    let string_client = StringLabClient::from(string_driver.caller());
    let string_driver_task = tokio::spawn(async move { string_driver.run().await });

    println!("[client] calling CounterLab::bump twice");
    assert_eq!(
        counter_client
            .bump()
            .await
            .map_err(|e| eyre!("counter_client.bump #1 failed: {e:?}"))?,
        1
    );
    assert_eq!(
        counter_client
            .bump()
            .await
            .map_err(|e| eyre!("counter_client.bump #2 failed: {e:?}"))?,
        2
    );
    println!("[client] CounterLab::bump -> 1, 2");

    println!("[client] calling CounterLab::echo");
    assert_eq!(
        counter_client
            .echo("alpha".to_string())
            .await
            .map_err(|e| eyre!("counter_client.echo failed: {e:?}"))?,
        "echo:alpha"
    );
    println!("[client] CounterLab::echo -> echo:alpha");

    println!("[client] calling StringLab::shout");
    assert_eq!(
        string_client
            .shout("beta".to_string())
            .await
            .map_err(|e| eyre!("string_client.shout failed: {e:?}"))?,
        "BETA"
    );
    println!("[client] StringLab::shout -> BETA");

    println!("[client] closing virtual connections");
    session_handle
        .close_connection(counter_conn_id, vec![])
        .await
        .map_err(|e| eyre!("close_connection(counter) failed: {e:?}"))?;
    session_handle
        .close_connection(string_conn_id, vec![])
        .await
        .map_err(|e| eyre!("close_connection(string) failed: {e:?}"))?;

    counter_driver_task.abort();
    string_driver_task.abort();
    server_task.abort();
    println!("[demo] virtual_connections: complete");

    Ok(())
}
