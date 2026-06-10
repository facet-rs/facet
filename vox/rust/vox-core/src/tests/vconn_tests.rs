use moire::task::FutureExt;
use vox_types::{
    ChannelBinder, ConnectionCloseReason, ConnectionSettings, IncomingChannelMessage, Metadata,
    MethodId, Parity, Payload, ReplySink, RequestCall, RequestResponse, SelfRef,
};

use super::utils::*;
use crate::session::{
    ConnectionAcceptor, ConnectionRequest, PendingConnection, SessionError, acceptor_conduit,
    initiator_conduit,
};
use crate::{Driver, NoopClient};

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
        Payload::Encoded(bytes) => *bytes,
        _ => panic!("expected incoming payload in response"),
    };
    vox_phon::from_slice(ret_bytes).expect("deserialize response")
}

// r[verify rpc.virtual-connection.open]
// r[verify rpc.virtual-connection.accept]
// r[verify connection.open]
// r[verify connection.virtual]
// r[verify session.connection-settings.open]
// r[verify session.message]
// r[verify session.message.connection-id]
#[tokio::test]
async fn open_virtual_connection_and_call() {
    let _ = tracing_subscriber::fmt::try_init();
    let (client_conduit, server_conduit) = message_conduit_pair();

    let server_task = moire::task::spawn(
        async move {
            acceptor_conduit(server_conduit, test_acceptor_handshake())
                .on_connection(EchoAcceptor)
                .establish::<NoopClient>()
                .await
                .expect("server handshake failed")
        }
        .named("server_setup"),
    );

    let _client_caller_guard = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<NoopClient>()
        .await
        .expect("client handshake failed");
    let session_handle = _client_caller_guard.session.clone().unwrap();

    let _server_caller_guard = server_task.await.expect("server setup failed");

    // Open a virtual connection.
    let vconn_handle = session_handle
        .open_connection(
            ConnectionSettings {
                parity: Parity::Odd,
                max_concurrent_requests: 64,
                initial_channel_credit: 16,
            },
            vox_types::metadata().str("vox-service", "Echo").build(),
        )
        .await
        .expect("open virtual connection");
    assert!(
        !vconn_handle.connection_id().is_root(),
        "virtual connection id should not be root"
    );

    // Set up a driver on the client side for the virtual connection.
    let mut vconn_driver = Driver::new(vconn_handle, ());
    let caller = crate::Caller::new(vconn_driver.caller());
    moire::task::spawn(async move { vconn_driver.run().await }.named("vconn_client_driver"));

    // Make a call on the virtual connection.
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
        Payload::Encoded(bytes) => *bytes,
        _ => panic!("expected incoming payload in response"),
    };
    let result: u32 = vox_phon::from_slice(ret_bytes).expect("deserialize response");
    assert_eq!(result, 123);
}

// r[verify rpc.one-service-per-connection]
#[tokio::test]
async fn root_and_virtual_connections_bind_separate_services() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    struct ServiceAcceptor;

    impl ConnectionAcceptor for ServiceAcceptor {
        fn accept(
            &self,
            request: &ConnectionRequest,
            connection: PendingConnection,
        ) -> Result<(), Metadata> {
            match request.service() {
                "Noop" => connection.handle_with(ConstHandler(10)),
                "Echo" => connection.handle_with(ConstHandler(20)),
                _ => return Err(Metadata::default()),
            }
            Ok(())
        }
    }

    let server_task = moire::task::spawn(
        async move {
            acceptor_conduit(server_conduit, test_acceptor_handshake())
                .on_connection(ServiceAcceptor)
                .establish::<NoopClient>()
                .await
                .expect("server handshake failed")
        }
        .named("server_setup"),
    );

    let root_caller_guard = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<NoopClient>()
        .await
        .expect("client handshake failed");
    let session_handle = root_caller_guard.session.clone().unwrap();
    let _server_caller_guard = server_task.await.expect("server setup failed");

    assert_eq!(call_u32(&root_caller_guard.caller, 1).await, 10);

    let vconn_handle = session_handle
        .open_connection(
            ConnectionSettings {
                parity: Parity::Odd,
                max_concurrent_requests: 64,
                initial_channel_credit: 16,
            },
            vox_types::metadata().str("vox-service", "Echo").build(),
        )
        .await
        .expect("open virtual connection");

    let mut vconn_driver = Driver::new(vconn_handle, ());
    let vconn_caller = crate::Caller::new(vconn_driver.caller());
    moire::task::spawn(async move { vconn_driver.run().await }.named("vconn_client_driver"));

    assert_eq!(call_u32(&vconn_caller, 2).await, 20);
    assert_eq!(call_u32(&root_caller_guard.caller, 3).await, 10);
}

// r[verify connection.open.rejection]
#[tokio::test]
async fn reject_virtual_connection() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let server_task = moire::task::spawn(
        async move {
            acceptor_conduit(server_conduit, test_acceptor_handshake())
                .on_connection(crate::session::acceptor_fn(
                    |request: &ConnectionRequest, connection: PendingConnection| match request
                        .service()
                    {
                        "Noop" => {
                            connection.handle_with(EchoHandler);
                            Ok(())
                        }
                        _ => Err(Default::default()),
                    },
                ))
                .establish::<NoopClient>()
                .await
                .expect("server handshake failed")
        }
        .named("server_setup"),
    );

    let _client_caller_guard = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<NoopClient>()
        .await
        .expect("client handshake failed");
    let session_handle = _client_caller_guard.session.clone().unwrap();

    let _server_caller_guard = server_task.await.expect("server setup failed");

    // Try to open a virtual connection — should be rejected.
    let result = session_handle
        .open_connection(
            ConnectionSettings {
                parity: Parity::Odd,
                max_concurrent_requests: 64,
                initial_channel_credit: 16,
            },
            vox_types::metadata().str("vox-service", "Unknown").build(),
        )
        .await;

    assert!(
        matches!(result, Err(SessionError::Rejected(_))),
        "expected Rejected, got: {result:?}"
    );
}

// r[verify connection.open.rejection]
#[tokio::test]
async fn open_virtual_connection_without_acceptor_is_rejected() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let server_task = moire::task::spawn(
        async move {
            acceptor_conduit(server_conduit, test_acceptor_handshake())
                .establish::<NoopClient>()
                .await
                .expect("server handshake failed")
        }
        .named("server_setup"),
    );

    let _client_caller_guard = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<NoopClient>()
        .await
        .expect("client handshake failed");
    let session_handle = _client_caller_guard.session.clone().unwrap();

    let _server_caller_guard = server_task.await.expect("server setup failed");

    // With the unified acceptor model, no explicit acceptor means the default
    // () acceptor is used, which accepts all connections with a no-op handler.
    let result = session_handle
        .open_connection(
            ConnectionSettings {
                parity: Parity::Odd,
                max_concurrent_requests: 64,
                initial_channel_credit: 16,
            },
            vox_types::metadata().str("vox-service", "Noop").build(),
        )
        .await;

    assert!(
        result.is_ok(),
        "default acceptor should accept connections: {result:?}"
    );
}

// r[verify connection.close]
#[tokio::test]
async fn close_unknown_virtual_connection_is_rejected() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let server_task = moire::task::spawn(
        async move {
            acceptor_conduit(server_conduit, test_acceptor_handshake())
                .on_connection(EchoHandler)
                .establish::<NoopClient>()
                .await
                .expect("server handshake failed")
        }
        .named("server_setup"),
    );

    let _client_caller_guard = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<NoopClient>()
        .await
        .expect("client handshake failed");
    let session_handle = _client_caller_guard.session.clone().unwrap();

    let _server_caller_guard = server_task.await.expect("server setup failed");

    let missing_conn_id = vox_types::ConnectionId(1);
    let result = session_handle
        .close_connection(missing_conn_id, Default::default())
        .await;
    assert!(
        matches!(result, Err(SessionError::Protocol(ref msg)) if msg == "connection not found"),
        "expected missing-connection protocol error, got: {result:?}"
    );
}

// r[verify connection.close]
// r[verify connection.close.semantics]
// r[verify rpc.caller.liveness.last-drop-closes-connection]
#[tokio::test]
async fn close_virtual_connection() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let server_task = moire::task::spawn(
        async move {
            acceptor_conduit(server_conduit, test_acceptor_handshake())
                .on_connection(EchoAcceptor)
                .establish::<NoopClient>()
                .await
                .expect("server handshake failed")
        }
        .named("server_setup"),
    );

    let _client_caller_guard = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<NoopClient>()
        .await
        .expect("client handshake failed");
    let session_handle = _client_caller_guard.session.clone().unwrap();

    let _server_caller_guard = server_task.await.expect("server setup failed");

    // Open a virtual connection.
    let vconn_handle = session_handle
        .open_connection(
            ConnectionSettings {
                parity: Parity::Odd,
                max_concurrent_requests: 64,
                initial_channel_credit: 16,
            },
            vox_types::metadata().str("vox-service", "Echo").build(),
        )
        .await
        .expect("open virtual connection");

    let conn_id = vconn_handle.connection_id();
    assert!(!conn_id.is_root(), "virtual connection should not be root");

    // Set up a driver on the client side.
    let mut vconn_driver = Driver::new(vconn_handle, ());
    let caller = crate::Caller::new(vconn_driver.caller());
    let caller_closed = caller.clone();
    moire::task::spawn(async move { vconn_driver.run().await }.named("vconn_client_driver"));

    // Make a call to confirm the connection works.
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
        Payload::Encoded(bytes) => *bytes,
        _ => panic!("expected incoming payload"),
    };
    let result: u32 = vox_phon::from_slice(ret_bytes).expect("deserialize");
    assert_eq!(result, 42);

    // Close the virtual connection.
    session_handle
        .close_connection(conn_id, Default::default())
        .await
        .expect("close virtual connection");

    tokio::time::timeout(std::time::Duration::from_secs(1), caller_closed.closed())
        .await
        .expect("caller closed() should resolve after virtual connection close");
    assert!(
        !caller.is_connected(),
        "caller should report disconnected after virtual connection close"
    );
}

// r[verify rpc.caller.liveness.last-drop-closes-connection]
#[tokio::test]
async fn dropping_last_virtual_caller_closes_virtual_connection() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let server_task = moire::task::spawn(
        async move {
            acceptor_conduit(server_conduit, test_acceptor_handshake())
                .on_connection(EchoAcceptor)
                .establish::<NoopClient>()
                .await
                .expect("server handshake failed")
        }
        .named("server_setup"),
    );

    let _client_caller_guard = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<NoopClient>()
        .await
        .expect("client handshake failed");
    let session_handle = _client_caller_guard.session.clone().unwrap();

    let _server_caller_guard = server_task.await.expect("server setup failed");

    let vconn_handle = session_handle
        .open_connection(
            ConnectionSettings {
                parity: Parity::Odd,
                max_concurrent_requests: 64,
                initial_channel_credit: 16,
            },
            vox_types::metadata().str("vox-service", "Echo").build(),
        )
        .await
        .expect("open virtual connection");

    let mut vconn_driver = Driver::new(vconn_handle, ());
    let vconn_caller = crate::Caller::new(vconn_driver.caller());
    moire::task::spawn(async move { vconn_driver.run().await }.named("vconn_client_driver"));

    let response = vconn_caller
        .call(RequestCall {
            channels: Vec::new(),
            method_id: MethodId(1),
            args: Payload::outgoing(&11_u32),
            schemas: Default::default(),
            metadata: Default::default(),
        })
        .await
        .expect("call should succeed before dropping virtual caller");
    let response = response.get();
    let ret_bytes = match &response.ret {
        Payload::Encoded(bytes) => *bytes,
        _ => panic!("expected incoming payload in response"),
    };
    let echoed: u32 = vox_phon::from_slice(ret_bytes).expect("deserialize response");
    assert_eq!(echoed, 11);

    drop(vconn_caller);
}

// r[verify connection.close.semantics]
// r[verify rpc.channel.close]
#[tokio::test]
async fn close_virtual_connection_closes_registered_rx_channels() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let server_task = moire::task::spawn(
        async move {
            acceptor_conduit(server_conduit, test_acceptor_handshake())
                .on_connection(EchoAcceptor)
                .establish::<NoopClient>()
                .await
                .expect("server handshake failed")
        }
        .named("server_setup"),
    );

    let _client_caller_guard = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<NoopClient>()
        .await
        .expect("client handshake failed");
    let session_handle = _client_caller_guard.session.clone().unwrap();

    let _server_caller_guard = server_task.await.expect("server setup failed");

    let vconn_handle = session_handle
        .open_connection(
            ConnectionSettings {
                parity: Parity::Odd,
                max_concurrent_requests: 64,
                initial_channel_credit: 16,
            },
            vox_types::metadata().str("vox-service", "Echo").build(),
        )
        .await
        .expect("open virtual connection");

    let conn_id = vconn_handle.connection_id();
    let mut vconn_driver = Driver::new(vconn_handle, ());
    let caller = crate::Caller::new(vconn_driver.caller());
    moire::task::spawn(async move { vconn_driver.run().await }.named("vconn_client_driver"));

    let (_channel_id, bound_rx) = caller.driver().create_rx();
    let mut rx_items = bound_rx.receiver;

    session_handle
        .close_connection(conn_id, Default::default())
        .await
        .expect("close virtual connection");

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
        "registered Rx channel should report connection closure when virtual connection closes"
    );
}

// r[verify rpc.caller.liveness.root-internal-close]
// r[verify rpc.caller.liveness.root-teardown-condition]
#[tokio::test]
async fn dropping_root_caller_waits_for_virtual_connections_before_session_shutdown() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let (client_session_tx, client_session_rx) =
        tokio::sync::oneshot::channel::<moire::task::JoinHandle<()>>();

    struct LocalEchoAcceptor;

    impl ConnectionAcceptor for LocalEchoAcceptor {
        fn accept(
            &self,
            _request: &ConnectionRequest,
            connection: PendingConnection,
        ) -> Result<(), Metadata> {
            connection.handle_with(EchoHandler);
            Ok(())
        }
    }

    let server_task = moire::task::spawn(
        async move {
            acceptor_conduit(server_conduit, test_acceptor_handshake())
                .on_connection(LocalEchoAcceptor)
                .establish::<NoopClient>()
                .await
                .expect("server handshake failed")
        }
        .named("server_setup"),
    );

    let root_caller = initiator_conduit(client_conduit, test_initiator_handshake())
        .spawn_fn(move |fut| {
            let handle = moire::task::spawn(fut.named("client_session"));
            let _ = client_session_tx.send(handle);
        })
        .establish::<NoopClient>()
        .await
        .expect("client handshake failed");
    let session_handle = root_caller.session.clone().unwrap();

    let server_caller_guard = server_task.await.expect("server setup failed");
    let client_session = client_session_rx.await.expect("client session handle sent");

    let vconn_handle = session_handle
        .open_connection(
            ConnectionSettings {
                parity: Parity::Odd,
                max_concurrent_requests: 64,
                initial_channel_credit: 16,
            },
            vox_types::metadata().str("vox-service", "Echo").build(),
        )
        .await
        .expect("open virtual connection");

    let mut vconn_driver = Driver::new(vconn_handle, ());
    let vconn_caller = crate::Caller::new(vconn_driver.caller());
    moire::task::spawn(async move { vconn_driver.run().await }.named("vconn_client_driver"));

    drop(root_caller);
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(
        !client_session.is_finished(),
        "session should remain alive while a virtual connection is still caller-live"
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
        .expect("virtual connection should still be usable after root caller drop");
    let response = response.get();
    let ret_bytes = match &response.ret {
        Payload::Encoded(bytes) => *bytes,
        _ => panic!("expected incoming payload in response"),
    };
    let echoed: u32 = vox_phon::from_slice(ret_bytes).expect("deserialize response");
    assert_eq!(echoed, 7);

    drop(vconn_caller);
    drop(server_caller_guard);

    tokio::time::timeout(std::time::Duration::from_millis(500), client_session)
        .await
        .expect("timed out waiting for client session to exit")
        .expect("client session task failed");
}
