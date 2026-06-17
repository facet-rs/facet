use std::sync::{Arc, Mutex};

use vox_rt::task::FutureExt;
use vox_types::{
    ChannelBinder, ConnectionCloseReason, ConnectionSettings, DriverEvent, IncomingChannelMessage,
    LaneId, MethodId, Parity, Payload, ReplySink, RequestCall, RequestResponse, SelfRef,
    VoxObserver, VoxObserverHandle,
};

use super::utils::*;
use crate::Driver;
use crate::connection::{
    ConnectionError, LaneAcceptor, LaneRejectReason, LaneRejection, LaneRequest, PendingLane,
    VOX_LANE_REJECT_REASON_METADATA_KEY, acceptor_conduit, initiator_conduit,
};

#[derive(Clone, Copy)]
struct ConstHandler(u32);

impl vox_types::Handler<crate::DriverReplySink> for ConstHandler {
    async fn handle(
        &self,
        _call: SelfRef<RequestCall<'static>>,
        reply: crate::DriverReplySink,
        _schemas: std::sync::Arc<vox_types::SchemaRecvTracker>,
    ) {
        let result = self.0;
        reply
            .send_reply(RequestResponse {
                ret: Payload::outgoing(&result),
                schemas: Default::default(),
                metadata: Default::default(),
            })
            .await;
    }
}

async fn call_u32(caller: &crate::Caller, value: u32) -> u32 {
    let response = caller
        .call(RequestCall {
            channels: Vec::new(),
            method_id: MethodId(1),
            args: Payload::outgoing(&value),
            schemas: Default::default(),
            metadata: Default::default(),
        })
        .await
        .expect("call should succeed");
    let response = response.get();
    let ret_bytes = match &response.ret {
        Payload::Encoded(bytes) => bytes,
        _ => panic!("expected incoming payload in response"),
    };
    vox_phon::from_slice(ret_bytes).expect("deserialize response")
}

#[derive(Default)]
struct RecordingDriverObserver {
    events: Mutex<Vec<DriverEvent>>,
}

impl RecordingDriverObserver {
    fn saw_lane_closed(&self, lane_id: LaneId) -> bool {
        self.events
            .lock()
            .expect("observer events mutex poisoned")
            .iter()
            .any(|event| {
                matches!(
                    event,
                    DriverEvent::ConnectionClosed { connection_id, .. } if *connection_id == lane_id
                )
            })
    }
}

impl VoxObserver for RecordingDriverObserver {
    fn driver_event(&self, event: DriverEvent) {
        self.events
            .lock()
            .expect("observer events mutex poisoned")
            .push(event);
    }
}

// r[verify lane.open.api]
// r[verify lane.accept.api]
// r[verify lane.open.wire]
// r[verify lane.service.compat]
// r[verify lane]
// r[verify lane.control]
// r[verify lane.open]
// r[verify lane.wire.compat]
// r[verify lane.open.settings]
// r[verify connection.message]
// r[verify connection.message.lane-id]
#[tokio::test]
async fn open_virtual_connection_and_call() {
    let _ = tracing_subscriber::fmt::try_init();
    let (client_conduit, server_conduit) = message_conduit_pair();

    let server_task = vox_rt::task::spawn(
        async move {
            acceptor_conduit(server_conduit, test_acceptor_handshake())
                .on_connection(EchoAcceptor)
                .establish_connection()
                .await
                .expect("server handshake failed")
        }
        .named("server_setup"),
    );

    let _client_caller_guard = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<TestLaneClient>()
        .await
        .expect("client handshake failed");
    let connection_handle = _client_caller_guard.connection.clone().unwrap();

    let _server_caller_guard = server_task.await.expect("server setup failed");

    // Open a service lane.
    let vconn_handle = connection_handle
        .open_lane_handle(
            ConnectionSettings {
                parity: Parity::Odd,
                max_concurrent_requests: 64,
                initial_channel_credit: 16,
            },
            vox_types::metadata().str("vox-service", "Echo").build(),
        )
        .await
        .expect("open service lane");
    assert!(
        !vconn_handle.connection_id().is_root(),
        "service lane id should not be the control lane"
    );

    // Set up a driver on the client side for the service lane.
    let mut vconn_driver = Driver::new(vconn_handle, ());
    let caller = crate::Caller::new(vconn_driver.caller());
    vox_rt::task::spawn(async move { vconn_driver.run().await }.named("vconn_client_driver"));

    // Make a call on the service lane.
    let args_value: u32 = 123;
    let response = caller
        .call(RequestCall {
            channels: Vec::new(),
            method_id: MethodId(1),
            args: Payload::outgoing(&args_value),
            schemas: Default::default(),
            metadata: Default::default(),
        })
        .await
        .expect("call should succeed");

    let response = response.get();
    let ret_bytes = match &response.ret {
        Payload::Encoded(bytes) => bytes,
        _ => panic!("expected incoming payload in response"),
    };
    let result: u32 = vox_phon::from_slice(ret_bytes).expect("deserialize response");
    assert_eq!(result, 123);
}

// r[verify lane]
// r[verify lane.service]
#[tokio::test]
async fn root_and_virtual_connections_bind_separate_services() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    struct ServiceAcceptor;

    impl LaneAcceptor for ServiceAcceptor {
        fn accept(
            &self,
            request: &LaneRequest,
            connection: PendingLane,
        ) -> Result<(), LaneRejection> {
            match request.service() {
                "Noop" => connection.handle_with(ConstHandler(10)),
                "Echo" => connection.handle_with(ConstHandler(20)),
                _ => return Err(LaneRejection::new(LaneRejectReason::UnknownService)),
            }
            Ok(())
        }
    }

    let server_task = vox_rt::task::spawn(
        async move {
            acceptor_conduit(server_conduit, test_acceptor_handshake())
                .on_connection(ServiceAcceptor)
                .establish_connection()
                .await
                .expect("server handshake failed")
        }
        .named("server_setup"),
    );

    let root_caller_guard = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<TestLaneClient>()
        .await
        .expect("client handshake failed");
    let connection_handle = root_caller_guard.connection.clone().unwrap();
    let _server_caller_guard = server_task.await.expect("server setup failed");

    assert_eq!(call_u32(&root_caller_guard.caller, 1).await, 10);

    let vconn_handle = connection_handle
        .open_lane_handle(
            ConnectionSettings {
                parity: Parity::Odd,
                max_concurrent_requests: 64,
                initial_channel_credit: 16,
            },
            vox_types::metadata().str("vox-service", "Echo").build(),
        )
        .await
        .expect("open service lane");

    let mut vconn_driver = Driver::new(vconn_handle, ());
    let vconn_caller = crate::Caller::new(vconn_driver.caller());
    vox_rt::task::spawn(async move { vconn_driver.run().await }.named("vconn_client_driver"));

    assert_eq!(call_u32(&vconn_caller, 2).await, 20);
    assert_eq!(call_u32(&root_caller_guard.caller, 3).await, 10);
}

// r[verify lane.open.wire.rejection]
// r[verify lane.open]
// r[verify lane.open.result]
// r[verify lane.wire.compat]
#[tokio::test]
async fn reject_virtual_connection() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let server_task = vox_rt::task::spawn(
        async move {
            acceptor_conduit(server_conduit, test_acceptor_handshake())
                .on_connection(crate::connection::lane_acceptor_fn(
                    |request: &LaneRequest, connection: PendingLane| match request.service() {
                        "Noop" => {
                            connection.handle_with(EchoHandler);
                            Ok(())
                        }
                        _ => Err(LaneRejection::new(LaneRejectReason::UnknownService)),
                    },
                ))
                .establish_connection()
                .await
                .expect("server handshake failed")
        }
        .named("server_setup"),
    );

    let _client_caller_guard = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<TestLaneClient>()
        .await
        .expect("client handshake failed");
    let connection_handle = _client_caller_guard.connection.clone().unwrap();

    let _server_caller_guard = server_task.await.expect("server setup failed");

    // Try to open an unknown service lane — should be rejected.
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
    assert_eq!(
        vox_types::metadata_get_str(rejection.metadata(), VOX_LANE_REJECT_REASON_METADATA_KEY,),
        Some("unknown-service")
    );
}

// r[verify lane.open.wire.rejection]
// r[verify lane.open]
// r[verify lane.open.result]
// r[verify lane.wire.compat]
#[tokio::test]
async fn open_virtual_connection_without_acceptor_is_rejected() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let server_task = vox_rt::task::spawn(
        async move {
            acceptor_conduit(server_conduit, test_acceptor_handshake())
                .establish_connection()
                .await
                .expect("server handshake failed")
        }
        .named("server_setup"),
    );

    let connection_handle = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish_connection()
        .await
        .expect("client handshake failed");

    let _server_guard = server_task.await.expect("server setup failed");

    // No explicit acceptor means inbound service lanes are rejected.
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

// r[verify lane.close]
// r[verify lane.wire.compat]
#[tokio::test]
async fn close_unknown_virtual_connection_is_rejected() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let server_task = vox_rt::task::spawn(
        async move {
            acceptor_conduit(server_conduit, test_acceptor_handshake())
                .establish_connection()
                .await
                .expect("server handshake failed")
        }
        .named("server_setup"),
    );

    let connection_handle = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish_connection()
        .await
        .expect("client handshake failed");

    let _server_guard = server_task.await.expect("server setup failed");

    let missing_conn_id = vox_types::LaneId(1);
    let result = connection_handle
        .close_lane(missing_conn_id, Default::default())
        .await;
    assert!(
        matches!(result, Err(ConnectionError::Protocol(ref msg)) if msg == "connection not found"),
        "expected missing-connection protocol error, got: {result:?}"
    );
}

// r[verify lane.close]
// r[verify lane.close.semantics]
// r[verify lane.wire.compat]
#[tokio::test]
async fn close_virtual_connection() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let server_task = vox_rt::task::spawn(
        async move {
            acceptor_conduit(server_conduit, test_acceptor_handshake())
                .on_connection(EchoAcceptor)
                .establish_connection()
                .await
                .expect("server handshake failed")
        }
        .named("server_setup"),
    );

    let _client_caller_guard = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<TestLaneClient>()
        .await
        .expect("client handshake failed");
    let connection_handle = _client_caller_guard.connection.clone().unwrap();

    let _server_caller_guard = server_task.await.expect("server setup failed");

    // Open a service lane.
    let vconn_handle = connection_handle
        .open_lane_handle(
            ConnectionSettings {
                parity: Parity::Odd,
                max_concurrent_requests: 64,
                initial_channel_credit: 16,
            },
            vox_types::metadata().str("vox-service", "Echo").build(),
        )
        .await
        .expect("open service lane");

    let conn_id = vconn_handle.connection_id();
    assert!(
        !conn_id.is_root(),
        "service lane should not be the control lane"
    );

    // Set up a driver on the client side.
    let mut vconn_driver = Driver::new(vconn_handle, ());
    let caller = crate::Caller::new(vconn_driver.caller());
    let caller_closed = caller.clone();
    vox_rt::task::spawn(async move { vconn_driver.run().await }.named("vconn_client_driver"));

    // Make a call to confirm the service lane works.
    let args_value: u32 = 42;
    let response = caller
        .call(RequestCall {
            channels: Vec::new(),
            method_id: MethodId(1),
            args: Payload::outgoing(&args_value),
            schemas: Default::default(),
            metadata: Default::default(),
        })
        .await
        .expect("call should succeed before close");

    let response = response.get();
    let ret_bytes = match &response.ret {
        Payload::Encoded(bytes) => bytes,
        _ => panic!("expected incoming payload"),
    };
    let result: u32 = vox_phon::from_slice(ret_bytes).expect("deserialize");
    assert_eq!(result, 42);

    // Close the service lane.
    connection_handle
        .close_lane(conn_id, Default::default())
        .await
        .expect("close service lane");

    tokio::time::timeout(std::time::Duration::from_secs(1), caller_closed.closed())
        .await
        .expect("caller closed() should resolve after service lane close");
    assert!(
        !caller.is_connected(),
        "caller should report disconnected after service lane close"
    );
}

// r[verify rpc.caller.liveness.last-drop-closes-connection]
// r[verify connection.lifecycle.driven]
// r[verify connection.shutdown.explicit]
#[tokio::test]
async fn dropping_last_virtual_caller_does_not_close_virtual_connection() {
    let (client_conduit, server_conduit) = message_conduit_pair();
    let observer = Arc::new(RecordingDriverObserver::default());
    let observer_handle: VoxObserverHandle = observer.clone();

    let server_task = vox_rt::task::spawn(
        async move {
            acceptor_conduit(server_conduit, test_acceptor_handshake())
                .on_connection(EchoAcceptor)
                .establish_connection()
                .await
                .expect("server handshake failed")
        }
        .named("server_setup"),
    );

    let _client_caller_guard = initiator_conduit(client_conduit, test_initiator_handshake())
        .observer_handle(observer_handle)
        .establish::<TestLaneClient>()
        .await
        .expect("client handshake failed");
    let connection_handle = _client_caller_guard.connection.clone().unwrap();

    let _server_caller_guard = server_task.await.expect("server setup failed");

    let vconn_handle = connection_handle
        .open_lane_handle(
            ConnectionSettings {
                parity: Parity::Odd,
                max_concurrent_requests: 64,
                initial_channel_credit: 16,
            },
            vox_types::metadata().str("vox-service", "Echo").build(),
        )
        .await
        .expect("open service lane");
    let conn_id = vconn_handle.connection_id();

    let mut vconn_driver = Driver::new(vconn_handle, ());
    let vconn_caller = crate::Caller::new(vconn_driver.caller());
    vox_rt::task::spawn(async move { vconn_driver.run().await }.named("vconn_client_driver"));

    let response = vconn_caller
        .call(RequestCall {
            channels: Vec::new(),
            method_id: MethodId(1),
            args: Payload::outgoing(&11_u32),
            schemas: Default::default(),
            metadata: Default::default(),
        })
        .await
        .expect("call should succeed before dropping lane caller");
    let response = response.get();
    let ret_bytes = match &response.ret {
        Payload::Encoded(bytes) => bytes,
        _ => panic!("expected incoming payload in response"),
    };
    let echoed: u32 = vox_phon::from_slice(ret_bytes).expect("deserialize response");
    assert_eq!(echoed, 11);

    drop(vconn_caller);
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(
        !observer.saw_lane_closed(conn_id),
        "dropping the last public caller must not close the service lane"
    );

    connection_handle
        .close_lane(conn_id, Default::default())
        .await
        .expect("explicit close should close service lane");
    assert!(
        observer.saw_lane_closed(conn_id),
        "explicit close should emit the lane close event"
    );
}

// r[verify lane.close.semantics]
// r[verify rpc.channel.close]
// r[verify lane.wire.compat]
#[tokio::test]
async fn close_virtual_connection_closes_registered_rx_channels() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let server_task = vox_rt::task::spawn(
        async move {
            acceptor_conduit(server_conduit, test_acceptor_handshake())
                .on_connection(EchoAcceptor)
                .establish_connection()
                .await
                .expect("server handshake failed")
        }
        .named("server_setup"),
    );

    let _client_caller_guard = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<TestLaneClient>()
        .await
        .expect("client handshake failed");
    let connection_handle = _client_caller_guard.connection.clone().unwrap();

    let _server_caller_guard = server_task.await.expect("server setup failed");

    let vconn_handle = connection_handle
        .open_lane_handle(
            ConnectionSettings {
                parity: Parity::Odd,
                max_concurrent_requests: 64,
                initial_channel_credit: 16,
            },
            vox_types::metadata().str("vox-service", "Echo").build(),
        )
        .await
        .expect("open service lane");

    let conn_id = vconn_handle.connection_id();
    let mut vconn_driver = Driver::new(vconn_handle, ());
    let caller = crate::Caller::new(vconn_driver.caller());
    vox_rt::task::spawn(async move { vconn_driver.run().await }.named("vconn_client_driver"));

    let (_channel_id, bound_rx) = caller.driver().create_rx();
    let mut rx_items = bound_rx.receiver;

    connection_handle
        .close_lane(conn_id, Default::default())
        .await
        .expect("close service lane");

    let recv_result = tokio::time::timeout(std::time::Duration::from_millis(200), rx_items.recv())
        .await
        .expect("timed out waiting for channel receiver to close");
    assert!(
        matches!(
            recv_result,
            Some(IncomingChannelMessage::ConnectionClosed(
                ConnectionCloseReason::Local
            ))
        ),
        "registered Rx channel should report connection closure when service lane closes"
    );
}

// r[verify rpc.caller.liveness.root-internal-close]
// r[verify rpc.caller.liveness.root-teardown-condition]
// r[verify connection.lifecycle.driven]
// r[verify connection.shutdown.explicit]
#[tokio::test]
async fn dropping_root_and_virtual_callers_does_not_shutdown_session() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let (client_session_tx, client_session_rx) =
        tokio::sync::oneshot::channel::<vox_rt::task::JoinHandle<()>>();

    struct LocalEchoAcceptor;

    impl LaneAcceptor for LocalEchoAcceptor {
        fn accept(
            &self,
            _request: &LaneRequest,
            connection: PendingLane,
        ) -> Result<(), LaneRejection> {
            connection.handle_with(EchoHandler);
            Ok(())
        }
    }

    let server_task = vox_rt::task::spawn(
        async move {
            acceptor_conduit(server_conduit, test_acceptor_handshake())
                .on_connection(LocalEchoAcceptor)
                .establish_connection()
                .await
                .expect("server handshake failed")
        }
        .named("server_setup"),
    );

    let root_caller = initiator_conduit(client_conduit, test_initiator_handshake())
        .spawn_fn(move |fut| {
            let handle = vox_rt::task::spawn(fut.named("client_session"));
            let _ = client_session_tx.send(handle);
        })
        .establish::<TestLaneClient>()
        .await
        .expect("client handshake failed");
    let connection_handle = root_caller.connection.clone().unwrap();

    let server_connection = server_task.await.expect("server setup failed");
    let client_session = client_session_rx.await.expect("client session handle sent");

    let vconn_handle = connection_handle
        .open_lane_handle(
            ConnectionSettings {
                parity: Parity::Odd,
                max_concurrent_requests: 64,
                initial_channel_credit: 16,
            },
            vox_types::metadata().str("vox-service", "Echo").build(),
        )
        .await
        .expect("open service lane");

    let mut vconn_driver = Driver::new(vconn_handle, ());
    let vconn_caller = crate::Caller::new(vconn_driver.caller());
    vox_rt::task::spawn(async move { vconn_driver.run().await }.named("vconn_client_driver"));

    drop(root_caller);
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(
        !client_session.is_finished(),
        "dropping the root caller must not close the driven connection"
    );

    let response = vconn_caller
        .call(RequestCall {
            channels: Vec::new(),
            method_id: MethodId(1),
            args: Payload::outgoing(&7_u32),
            schemas: Default::default(),
            metadata: Default::default(),
        })
        .await
        .expect("service lane should still be usable after root caller drop");
    let response = response.get();
    let ret_bytes = match &response.ret {
        Payload::Encoded(bytes) => bytes,
        _ => panic!("expected incoming payload in response"),
    };
    let echoed: u32 = vox_phon::from_slice(ret_bytes).expect("deserialize response");
    assert_eq!(echoed, 7);

    drop(vconn_caller);
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(
        !client_session.is_finished(),
        "dropping all public callers must not shut down the connection"
    );

    connection_handle
        .shutdown()
        .expect("client shutdown request");
    let _ = server_connection.shutdown();

    tokio::time::timeout(std::time::Duration::from_millis(500), client_session)
        .await
        .expect("timed out waiting for client session to exit")
        .expect("client session task failed");
}
