//! End-to-end tests for automatic service name routing via vox-service metadata.

use vox::{ConnectionSettings, Driver, Parity, memory_link_pair, metadata_get_str};

#[vox::service]
trait Echo {
    async fn echo(&self, value: u32) -> u32;
}

#[derive(Clone)]
struct EchoService;

impl Echo for EchoService {
    async fn echo(&self, value: u32) -> u32 {
        value
    }
}

#[vox::service]
trait Adder {
    async fn add(&self, a: u32, b: u32) -> u32;
}

#[derive(Clone)]
struct AdderService;

impl Adder for AdderService {
    async fn add(&self, a: u32, b: u32) -> u32 {
        a + b
    }
}

#[tokio::test]
async fn root_connect_sends_vox_service_and_factory_sees_it() {
    use std::sync::{Arc, Mutex};

    let (client_link, server_link) = memory_link_pair(16);
    let seen_service = Arc::new(Mutex::new(None::<String>));

    // Server uses a factory that records the service name it sees.
    let factory = {
        let seen_service = seen_service.clone();
        move |metadata: &[vox::MetadataEntry]| -> Option<Box<dyn FnOnce(vox::ConnectionHandle) + Send>> {
            let service = metadata_get_str(metadata, "vox-service");
            *seen_service.lock().unwrap() = service.map(String::from);
            // Accept and serve Echo regardless
            Some(Box::new(|handle| {
                let mut driver = Driver::new(handle, EchoDispatcher::new(EchoService));
                tokio::spawn(async move { driver.run().await });
            })
                as Box<dyn FnOnce(vox::ConnectionHandle) + Send>)
        }
    };

    let server = tokio::spawn(async move {
        let s = vox::acceptor_on(server_link)
            .on_connection(factory)
            .establish::<vox::NoopClient>(())
            .await
            .expect("server establish");
        s
    });

    let root = vox::initiator_on(client_link, vox::TransportMode::Bare)
        .establish::<vox::NoopClient>(())
        .await
        .expect("client establish");

    let _server_guard = server.await.expect("server task");
    let session = root.session.clone().unwrap();

    // Open a typed Echo vconn — this triggers the factory
    let echo: EchoClient = session
        .open(ConnectionSettings {
            parity: Parity::Odd,
            max_concurrent_requests: 64,
        })
        .await
        .expect("open Echo vconn");

    // Verify the factory saw vox-service: "Echo"
    let service = seen_service.lock().unwrap().clone();
    assert_eq!(service.as_deref(), Some("Echo"));

    let result = echo.echo(42).await.expect("echo call");
    assert_eq!(result, 42);
}

#[tokio::test]
async fn service_factory_routes_virtual_connections() {
    let (client_link, server_link) = memory_link_pair(16);

    let factory =
        |metadata: &[vox::MetadataEntry]| -> Option<Box<dyn FnOnce(vox::ConnectionHandle) + Send>> {
            let service = metadata_get_str(metadata, "vox-service")?;
            match service {
                "Echo" => Some(Box::new(|handle| {
                    let mut driver = Driver::new(handle, EchoDispatcher::new(EchoService));
                    tokio::spawn(async move { driver.run().await });
                })
                    as Box<dyn FnOnce(vox::ConnectionHandle) + Send>),
                "Adder" => Some(Box::new(|handle| {
                    let mut driver = Driver::new(handle, AdderDispatcher::new(AdderService));
                    tokio::spawn(async move { driver.run().await });
                })
                    as Box<dyn FnOnce(vox::ConnectionHandle) + Send>),
                _ => None,
            }
        };

    let server = tokio::spawn(async move {
        let s = vox::acceptor_on(server_link)
            .on_connection(factory)
            .establish::<vox::NoopClient>(())
            .await
            .expect("server establish");
        s
    });

    let root = vox::initiator_on(client_link, vox::TransportMode::Bare)
        .establish::<vox::NoopClient>(())
        .await
        .expect("client establish");

    let _server_guard = server.await.expect("server task");
    let session = root.session.clone().unwrap();

    // Open a typed Echo vconn
    let echo: EchoClient = session
        .open(ConnectionSettings {
            parity: Parity::Odd,
            max_concurrent_requests: 64,
        })
        .await
        .expect("open Echo vconn");

    let result = echo.echo(42).await.expect("echo call");
    assert_eq!(result, 42);

    // Open a typed Adder vconn
    let adder: AdderClient = session
        .open(ConnectionSettings {
            parity: Parity::Odd,
            max_concurrent_requests: 64,
        })
        .await
        .expect("open Adder vconn");

    let result = adder.add(3, 4).await.expect("add call");
    assert_eq!(result, 7);
}

#[tokio::test]
async fn service_factory_rejects_unknown_service() {
    let (client_link, server_link) = memory_link_pair(16);

    let factory =
        |metadata: &[vox::MetadataEntry]| -> Option<Box<dyn FnOnce(vox::ConnectionHandle) + Send>> {
            let service = metadata_get_str(metadata, "vox-service")?;
            match service {
                "Echo" => Some(Box::new(|handle| {
                    let mut driver = Driver::new(handle, EchoDispatcher::new(EchoService));
                    tokio::spawn(async move { driver.run().await });
                })
                    as Box<dyn FnOnce(vox::ConnectionHandle) + Send>),
                _ => None,
            }
        };

    let server = tokio::spawn(async move {
        let s = vox::acceptor_on(server_link)
            .on_connection(factory)
            .establish::<vox::NoopClient>(())
            .await
            .expect("server establish");
        s
    });

    let root = vox::initiator_on(client_link, vox::TransportMode::Bare)
        .establish::<vox::NoopClient>(())
        .await
        .expect("client establish");

    let _server_guard = server.await.expect("server task");
    let session = root.session.clone().unwrap();

    // Adder is not in the factory — should be rejected
    let result = session
        .open::<AdderClient>(ConnectionSettings {
            parity: Parity::Odd,
            max_concurrent_requests: 64,
        })
        .await;

    assert!(result.is_err(), "unknown service should be rejected");
}
