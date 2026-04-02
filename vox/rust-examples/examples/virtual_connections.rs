use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};

use eyre::{Result, WrapErr, eyre};
use vox::transport::tcp::StreamLink;
use vox::{
    ConnectionSettings, Driver, Metadata, MetadataEntry, MetadataFlags, MetadataValue, Parity,
};

#[vox::service]
trait CounterLab {
    async fn bump(&self) -> u32;
    async fn echo(&self, value: String) -> String;
}

#[vox::service]
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

fn lab_acceptor(
    request: &vox::ConnectionRequest,
    connection: vox::PendingConnection,
) -> Result<(), Metadata<'static>> {
    match request.service() {
        Some("counter") => {
            connection.handle_with(CounterLabDispatcher::new(CounterLabService::new()));
            Ok(())
        }
        Some("string") => {
            connection.handle_with(StringLabDispatcher::new(StringLabService));
            Ok(())
        }
        _ => Err(vec![MetadataEntry::str(
            "error",
            "unknown or missing service metadata",
        )]),
    }
}

fn requested_service<'a>(metadata: &'a [MetadataEntry<'a>]) -> Option<&'a str> {
    metadata
        .iter()
        .find(|entry| entry.key == "service")
        .and_then(|entry| match &entry.value {
            MetadataValue::String(value) => Some(value.as_ref()),
            _ => None,
        })
}

fn service_metadata(service: &'static str) -> Metadata<'static> {
    vec![MetadataEntry {
        key: "service".into(),
        value: MetadataValue::String(service.into()),
        flags: MetadataFlags::NONE,
    }]
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
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
        let server_root_guard = vox::acceptor_on(StreamLink::tcp(socket))
            .on_connection(vox::acceptor_fn(lab_acceptor))
            .establish::<vox::NoopClient>(())
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
    let _root_caller_guard = vox::initiator_on(StreamLink::tcp(socket), vox::TransportMode::Bare)
        .establish::<vox::NoopClient>(())
        .await
        .map_err(|e| eyre!("failed to establish initiator session: {e:?}"))?;
    let session_handle = _root_caller_guard.session.clone().unwrap();
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
    let counter_client = CounterLabClient::new(vox::Caller::new(counter_driver.caller()));
    let counter_driver_task = tokio::spawn(async move { counter_driver.run().await });

    println!("[client] opening string virtual connection");
    let string_conn = session_handle
        .open_connection(settings, service_metadata("string"))
        .await
        .map_err(|e| eyre!("open_connection(string) failed: {e:?}"))?;
    let string_conn_id = string_conn.connection_id();
    let mut string_driver = Driver::new(string_conn, ());
    let string_client = StringLabClient::new(vox::Caller::new(string_driver.caller()));
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
