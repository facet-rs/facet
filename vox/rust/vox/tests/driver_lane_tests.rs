//! Tests for service lanes through the driver.

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use vox::{
    ConnectionError, ConnectionHandle, ConnectionSettings, Driver, LaneRejectReason, LaneRejection,
    Parity, memory_link_pair,
};

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

async fn lane_server(server_link: impl vox::Link + Send + 'static) -> vox::ConnectionHandle {
    vox::acceptor_on(server_link)
        .on_connection(EchoDispatcher::new(EchoService))
        .establish_connection()
        .await
        .expect("server establish")
}

async fn open_echo_lane(connection: &ConnectionHandle) -> EchoClient {
    connection
        .open_lane_with_settings::<EchoClient>(ConnectionSettings {
            parity: Parity::Odd,
            max_concurrent_requests: 64,
            initial_channel_credit: 16,
        })
        .await
        .expect("open service lane")
}

#[tokio::test]
async fn open_service_lane_and_call() {
    let (client_link, server_link) = memory_link_pair(16);

    let server = tokio::spawn(async move { lane_server(server_link).await });

    let connection_guard = vox::initiator_on(client_link)
        .establish_connection()
        .await
        .expect("client establish");
    let connection = connection_guard.clone();

    let _server_guard = server.await.expect("server task");
    let lane_client = open_echo_lane(&connection).await;

    let result = lane_client.echo(123).await.expect("service lane echo");
    assert_eq!(result, 123);

    let _ = connection.shutdown();
    let _ = _server_guard.shutdown();
    drop(lane_client);
    drop(connection_guard);
}

// r[verify rpc.caller.liveness.public-handle-drop]
// r[verify rpc.caller.liveness.explicit-shutdown-required]
#[tokio::test]
async fn dropping_control_client_and_lane_clients_does_not_shutdown_connection() {
    let (client_link, server_link) = memory_link_pair(16);

    let (connection_task_tx, connection_task_rx) =
        tokio::sync::oneshot::channel::<tokio::task::JoinHandle<()>>();

    let server = tokio::spawn(async move { lane_server(server_link).await });

    let connection_guard = vox::initiator_on(client_link)
        .spawn_fn(move |fut| {
            let handle = tokio::spawn(fut);
            let _ = connection_task_tx.send(handle);
        })
        .establish_connection()
        .await
        .expect("client establish");
    let connection = connection_guard.clone();

    let _server_guard = server.await.expect("server task");
    let client_connection_task = connection_task_rx.await.expect("connection handle");

    let connection_handle = connection.clone();
    let lane_client = open_echo_lane(&connection_handle).await;

    // Drop the connection handle: ordinary handle drop is inert.
    drop(connection_guard);
    drop(connection_handle);
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        !client_connection_task.is_finished(),
        "connection should remain alive after dropping control-lane handles"
    );

    // service lane still works.
    let result = lane_client
        .echo(7)
        .await
        .expect("service lane echo after control-lane client drop");
    assert_eq!(result, 7);

    // Drop service lane too: shutdown still remains explicit.
    drop(lane_client);
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        !client_connection_task.is_finished(),
        "connection should remain alive after dropping all public clients"
    );

    let _ = _server_guard.shutdown();
    let _ = connection.shutdown();

    tokio::time::timeout(Duration::from_millis(500), client_connection_task)
        .await
        .expect("connection exit timeout")
        .expect("connection failed");
}

#[tokio::test]
async fn schema_tracker_is_per_lane() {
    let (client_link, server_link) = memory_link_pair(16);

    let server = tokio::spawn(async move {
        vox::acceptor_on(server_link)
            .on_connection(EchoDispatcher::new(EchoService))
            .establish_connection()
            .await
            .expect("server establish")
    });

    let control_client = vox::initiator_on(client_link)
        .establish::<EchoClient>()
        .await
        .expect("client establish");
    let connection = control_client.connection.clone().unwrap();

    let _server_guard = server.await.expect("server task");
    let connection_handle = connection;

    // Call on control lane.
    let r1 = control_client.echo(100).await.expect("control-lane echo");
    assert_eq!(r1, 100);

    // Open service lane and call; schemas should not conflict with the control lane.
    let lane_client = open_echo_lane(&connection_handle).await;
    let r2 = lane_client.echo(200).await.expect("service lane echo");
    assert_eq!(r2, 200);
}

#[vox::service]
trait Counter {
    async fn increment(&self) -> u32;
}

#[derive(Clone)]
struct CounterService {
    count: std::sync::Arc<AtomicU32>,
}

impl Counter for CounterService {
    async fn increment(&self) -> u32 {
        self.count.fetch_add(1, Ordering::SeqCst) + 1
    }
}

#[tokio::test]
async fn reject_service_lane() {
    let (client_link, server_link) = memory_link_pair(16);

    let server = tokio::spawn(async move {
        vox::acceptor_on(server_link)
            .on_connection(vox::lane_acceptor_fn(
                |req: &vox::LaneRequest, conn: vox::PendingLane| match req.service() {
                    "Noop" => {
                        conn.handle_with(());
                        Ok(())
                    }
                    _ => Err(LaneRejection::new(LaneRejectReason::UnknownService)),
                },
            ))
            .establish_connection()
            .await
            .expect("server establish")
    });

    let _connection_guard = vox::initiator_on(client_link)
        .establish_connection()
        .await
        .expect("client establish");
    let connection = _connection_guard.clone();

    let _server_guard = server.await.expect("server task");
    let connection_handle = connection;

    let result = connection_handle
        .open_lane_handle(
            ConnectionSettings {
                parity: Parity::Odd,
                max_concurrent_requests: 64,
                initial_channel_credit: 16,
            },
            vox_types::metadata().str("vox-service", "Unknown").build(),
        )
        .await;

    let Err(ConnectionError::Rejected(rejection)) = result else {
        panic!("expected structured rejection, got: {result:?}");
    };
    assert_eq!(rejection.reason(), LaneRejectReason::UnknownService);
}

#[tokio::test]
async fn open_service_lane_without_acceptor_is_rejected() {
    let (client_link, server_link) = memory_link_pair(16);

    // Server with NO lane acceptor.
    let server = tokio::spawn(async move {
        vox::acceptor_on(server_link)
            .establish_connection()
            .await
            .expect("server establish")
    });

    let _connection_guard = vox::initiator_on(client_link)
        .establish_connection()
        .await
        .expect("client establish");
    let connection = _connection_guard.clone();

    let _server_guard = server.await.expect("server task");
    let connection_handle = connection;

    let result = connection_handle
        .open_lane_handle(
            ConnectionSettings {
                parity: Parity::Odd,
                max_concurrent_requests: 64,
                initial_channel_credit: 16,
            },
            vox_types::metadata().str("vox-service", "Noop").build(),
        )
        .await;

    let Err(ConnectionError::Rejected(rejection)) = result else {
        panic!("expected structured rejection, got: {result:?}");
    };
    assert_eq!(rejection.reason(), LaneRejectReason::NotReady);
}

#[tokio::test]
async fn close_service_lane() {
    let (client_link, server_link) = memory_link_pair(16);

    let server = tokio::spawn(async move {
        vox::acceptor_on(server_link)
            .on_connection(CounterDispatcher::new(CounterService {
                count: std::sync::Arc::new(AtomicU32::new(0)),
            }))
            .establish_connection()
            .await
            .expect("server establish")
    });

    let _connection_guard = vox::initiator_on(client_link)
        .establish_connection()
        .await
        .expect("client establish");
    let connection = _connection_guard.clone();

    let _server_guard = server.await.expect("server task");
    let connection_handle = connection;

    let lane_handle = connection_handle
        .open_lane_handle(
            ConnectionSettings {
                parity: Parity::Odd,
                max_concurrent_requests: 64,
                initial_channel_credit: 16,
            },
            vox_types::metadata().str("vox-service", "Counter").build(),
        )
        .await
        .expect("open service lane");

    let conn_id = lane_handle.connection_id();
    let mut lane_driver = Driver::new(lane_handle, ());
    let caller = vox::Caller::new(lane_driver.caller());
    tokio::spawn(async move { lane_driver.run().await });

    let client = CounterClient::new(caller);
    let r = client.increment().await.expect("increment before close");
    assert_eq!(r, 1);

    connection_handle
        .close_lane(conn_id, Default::default())
        .await
        .expect("close service lane");

    // Call after close should fail.
    tokio::time::sleep(Duration::from_millis(50)).await;
    let result = client.increment().await;
    assert!(result.is_err(), "call after close should fail");
}

#[tokio::test]
async fn close_control_lane_is_rejected() {
    let (client_link, server_link) = memory_link_pair(16);

    let server = tokio::spawn(async move {
        vox::acceptor_on(server_link)
            .establish_connection()
            .await
            .expect("server establish")
    });

    let _connection_guard = vox::initiator_on(client_link)
        .establish_connection()
        .await
        .expect("client establish");
    let connection = _connection_guard.clone();

    let _server_guard = server.await.expect("server task");
    let connection_handle = connection;

    // Lane ID 0 is the control lane.
    let result = connection_handle
        .close_lane(vox::LaneId(0), Default::default())
        .await;
    assert!(result.is_err(), "closing control lane should fail");
}
