//! End-to-end tests for automatic service name routing via vox-service metadata.

use vox::{ConnectionSettings, Parity, memory_link_pair};

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
async fn control_lane_connect_sends_vox_service_and_factory_sees_it() {
    use std::sync::{Arc, Mutex};

    let (client_link, server_link) = memory_link_pair(16);
    let seen_service = Arc::new(Mutex::new(None::<String>));

    // Server uses a factory that records the service name it sees.
    let factory = vox::lane_acceptor_fn({
        let seen_service = seen_service.clone();
        move |request: &vox::LaneRequest,
              connection: vox::PendingLane|
              -> Result<(), vox::LaneRejection> {
            *seen_service.lock().unwrap() = Some(request.service().to_string());
            connection.handle_with(EchoDispatcher::new(EchoService));
            Ok(())
        }
    });

    let server = tokio::spawn(async move {
        vox::acceptor_on(server_link)
            .on_connection(factory)
            .establish_connection()
            .await
            .expect("server establish")
    });

    let connection_guard = vox::initiator_on(client_link)
        .establish_connection()
        .await
        .expect("client establish");

    let _server_guard = server.await.expect("server task");
    let connection = connection_guard.clone();

    // Open a typed Echo service lane — this triggers the factory
    let echo: EchoClient = connection
        .open_lane_with_settings(ConnectionSettings {
            parity: Parity::Odd,
            max_concurrent_requests: 64,
            initial_channel_credit: 16,
        })
        .await
        .expect("open Echo service lane");

    // Verify the factory saw vox-service: "Echo"
    let service = seen_service.lock().unwrap().clone();
    assert_eq!(service.as_deref(), Some("Echo"));

    let result = echo.echo(42).await.expect("echo call");
    assert_eq!(result, 42);
}

#[tokio::test]
async fn service_factory_routes_service_lanes() {
    let (client_link, server_link) = memory_link_pair(16);

    let factory = vox::lane_acceptor_fn(
        |request: &vox::LaneRequest,
         connection: vox::PendingLane|
         -> Result<(), vox::LaneRejection> {
            match request.service() {
                "Echo" => {
                    connection.handle_with(EchoDispatcher::new(EchoService));
                    Ok(())
                }
                "Adder" => {
                    connection.handle_with(AdderDispatcher::new(AdderService));
                    Ok(())
                }
                "Noop" => {
                    connection.handle_with(());
                    Ok(())
                }
                _ => Err(vox::LaneRejection::new(
                    vox::LaneRejectReason::UnknownService,
                )),
            }
        },
    );

    let server = tokio::spawn(async move {
        vox::acceptor_on(server_link)
            .on_connection(factory)
            .establish_connection()
            .await
            .expect("server establish")
    });

    let connection_guard = vox::initiator_on(client_link)
        .establish_connection()
        .await
        .expect("client establish");

    let _server_guard = server.await.expect("server task");
    let connection = connection_guard.clone();

    // Open a typed Echo service lane
    let echo: EchoClient = connection
        .open_lane_with_settings(ConnectionSettings {
            parity: Parity::Odd,
            max_concurrent_requests: 64,
            initial_channel_credit: 16,
        })
        .await
        .expect("open Echo service lane");

    let result = echo.echo(42).await.expect("echo call");
    assert_eq!(result, 42);

    // Open a typed Adder service lane
    let adder: AdderClient = connection
        .open_lane_with_settings(ConnectionSettings {
            parity: Parity::Odd,
            max_concurrent_requests: 64,
            initial_channel_credit: 16,
        })
        .await
        .expect("open Adder service lane");

    let result = adder.add(3, 4).await.expect("add call");
    assert_eq!(result, 7);
}

#[tokio::test]
async fn service_factory_rejects_unknown_service() {
    let (client_link, server_link) = memory_link_pair(16);

    let factory = vox::lane_acceptor_fn(
        |request: &vox::LaneRequest,
         connection: vox::PendingLane|
         -> Result<(), vox::LaneRejection> {
            match request.service() {
                "Echo" => {
                    connection.handle_with(EchoDispatcher::new(EchoService));
                    Ok(())
                }
                "Noop" => {
                    connection.handle_with(());
                    Ok(())
                }
                _ => Err(vox::LaneRejection::new(
                    vox::LaneRejectReason::UnknownService,
                )),
            }
        },
    );

    let server = tokio::spawn(async move {
        vox::acceptor_on(server_link)
            .on_connection(factory)
            .establish_connection()
            .await
            .expect("server establish")
    });

    let connection_guard = vox::initiator_on(client_link)
        .establish_connection()
        .await
        .expect("client establish");

    let _server_guard = server.await.expect("server task");
    let connection = connection_guard.clone();

    // Adder is not in the factory — should be rejected
    let result = connection
        .open_lane_with_settings::<AdderClient>(ConnectionSettings {
            parity: Parity::Odd,
            max_concurrent_requests: 64,
            initial_channel_credit: 16,
        })
        .await;

    let rejection = match result {
        Err(vox::ConnectionError::Rejected(rejection)) => rejection,
        Err(error) => panic!("expected structured rejection, got error: {error:?}"),
        Ok(_) => panic!("expected structured rejection, got client"),
    };
    assert_eq!(rejection.reason(), vox::LaneRejectReason::UnknownService);
}
