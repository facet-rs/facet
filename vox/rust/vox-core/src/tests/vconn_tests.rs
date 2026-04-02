use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use moire::sync::mpsc;
use moire::task::FutureExt;
use vox_types::{
    Backing, ChannelBinder, ChannelBody, ChannelClose, ChannelGrantCredit, ChannelId, ChannelItem,
    ChannelMessage, ChannelSink, Conduit, ConduitRx, ConnectionSettings, Handler,
    IncomingChannelMessage, Link, LinkRx, LinkTx, LinkTxPermit, Message, MessageFamily,
    MessagePayload, Metadata, MethodId, Parity, Payload, ReplySink, RequestBody, RequestCall,
    RequestCancel, RequestMessage, RequestResponse, RetryPolicy, SelfRef, Tx, VoxError, WriteSlot,
    channel, ensure_operation_id, metadata_operation_id,
};
use vox_types::{HandshakeResult, SessionResumeKey, SessionRole};

use super::utils::*;
use crate::session::{
    ConnectionAcceptor, ConnectionHandle, ConnectionMessage, ConnectionRequest, PendingConnection,
    SessionAcceptOutcome, SessionError, SessionHandle, SessionKeepaliveConfig, SessionRegistry,
    acceptor_conduit, acceptor_on, initiator_conduit, initiator_on, proxy_connections,
};
use crate::{
    Attachment, BareConduit, Caller, Driver, DriverCaller, DriverReplySink, InMemoryOperationStore,
    LinkSource, NoopClient, OperationStore, TransportMode, initiate_transport, memory_link_pair,
};

// r[verify rpc.virtual-connection.open]
// r[verify rpc.virtual-connection.accept]
// r[verify connection.open]
#[tokio::test]
async fn open_virtual_connection_and_call() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let server_task = moire::task::spawn(
        async move {
            let server_caller = acceptor_conduit(server_conduit, test_acceptor_handshake())
                .on_connection(EchoAcceptor)
                .establish::<NoopClient>(())
                .await
                .expect("server handshake failed");
            server_caller
        }
        .named("server_setup"),
    );

    let _client_caller_guard = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<NoopClient>(())
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
            },
            vec![],
        )
        .await
        .expect("open virtual connection");

    // Set up a driver on the client side for the virtual connection.
    let mut vconn_driver = Driver::new(vconn_handle, ());
    let caller = crate::Caller::new(vconn_driver.caller());
    moire::task::spawn(async move { vconn_driver.run().await }.named("vconn_client_driver"));

    // Make a call on the virtual connection.
    let args_value: u32 = 123;
    let response = caller
        .call(RequestCall {
            method_id: MethodId(1),
            args: Payload::outgoing(&args_value),
            schemas: Default::default(),
            metadata: Default::default(),
        })
        .await
        .expect("call should succeed");

    let ret_bytes = match &response.ret {
        Payload::PostcardBytes(bytes) => *bytes,
        _ => panic!("expected incoming payload in response"),
    };
    let result: u32 = vox_postcard::from_slice(ret_bytes).expect("deserialize response");
    assert_eq!(result, 123);
}

// r[verify connection.open.rejection]
#[tokio::test]
async fn reject_virtual_connection() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let server_task = moire::task::spawn(
        async move {
            let server_caller = acceptor_conduit(server_conduit, test_acceptor_handshake())
                .on_connection(RejectAcceptor)
                .establish::<NoopClient>(())
                .await
                .expect("server handshake failed");
            server_caller
        }
        .named("server_setup"),
    );

    let _client_caller_guard = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<NoopClient>(())
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
            },
            vec![],
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
            let server_caller = acceptor_conduit(server_conduit, test_acceptor_handshake())
                .establish::<NoopClient>(())
                .await
                .expect("server handshake failed");
            server_caller
        }
        .named("server_setup"),
    );

    let _client_caller_guard = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<NoopClient>(())
        .await
        .expect("client handshake failed");
    let session_handle = _client_caller_guard.session.clone().unwrap();

    let _server_caller_guard = server_task.await.expect("server setup failed");

    let result = session_handle
        .open_connection(
            ConnectionSettings {
                parity: Parity::Odd,
                max_concurrent_requests: 64,
            },
            vec![],
        )
        .await;

    assert!(
        matches!(result, Err(SessionError::Rejected(_))),
        "expected Rejected, got: {result:?}"
    );
}

// r[verify connection.close]
#[tokio::test]
async fn close_unknown_virtual_connection_is_rejected() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let server_task = moire::task::spawn(
        async move {
            let server_caller = acceptor_conduit(server_conduit, test_acceptor_handshake())
                .establish::<NoopClient>(EchoHandler)
                .await
                .expect("server handshake failed");
            server_caller
        }
        .named("server_setup"),
    );

    let _client_caller_guard = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<NoopClient>(())
        .await
        .expect("client handshake failed");
    let session_handle = _client_caller_guard.session.clone().unwrap();

    let _server_caller_guard = server_task.await.expect("server setup failed");

    let missing_conn_id = vox_types::ConnectionId(1);
    let result = session_handle
        .close_connection(missing_conn_id, vec![])
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

    // Track whether the server-side virtual connection driver has exited.
    let server_driver_exited = Arc::new(AtomicBool::new(false));
    let server_driver_exited_check = server_driver_exited.clone();

    /// An acceptor that tracks server driver exit.
    struct TrackingAcceptor {
        exited: Arc<AtomicBool>,
    }

    impl ConnectionAcceptor for TrackingAcceptor {
        fn accept(
            &self,
            _request: &ConnectionRequest,
            connection: PendingConnection,
        ) -> Result<(), Metadata<'static>> {
            let exited = self.exited.clone();
            let handle = connection.into_handle();
            let mut driver = Driver::new(handle, EchoHandler);
            moire::task::spawn(
                async move {
                    driver.run().await;
                    exited.store(true, Ordering::SeqCst);
                }
                .named("vconn_server_driver"),
            );
            Ok(())
        }
    }

    let server_task = moire::task::spawn(
        async move {
            let server_caller = acceptor_conduit(server_conduit, test_acceptor_handshake())
                .on_connection(TrackingAcceptor {
                    exited: server_driver_exited,
                })
                .establish::<NoopClient>(())
                .await
                .expect("server handshake failed");
            server_caller
        }
        .named("server_setup"),
    );

    let _client_caller_guard = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<NoopClient>(())
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
            },
            vec![],
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
            method_id: MethodId(1),
            args: Payload::outgoing(&args_value),
            schemas: Default::default(),
            metadata: Default::default(),
        })
        .await
        .expect("call should succeed before close");

    let ret_bytes = match &response.ret {
        Payload::PostcardBytes(bytes) => *bytes,
        _ => panic!("expected incoming payload"),
    };
    let result: u32 = vox_postcard::from_slice(ret_bytes).expect("deserialize");
    assert_eq!(result, 42);

    // Close the virtual connection.
    session_handle
        .close_connection(conn_id, vec![])
        .await
        .expect("close virtual connection");

    tokio::time::timeout(std::time::Duration::from_secs(1), caller_closed.closed())
        .await
        .expect("caller closed() should resolve after virtual connection close");
    assert!(
        !caller.is_connected(),
        "caller should report disconnected after virtual connection close"
    );

    // The server-side driver should exit because `ConnectionClose` causes the
    // peer session to drop the connection slot, which drops conn_tx, causing
    // the driver's rx to return None.
    for _ in 0..20 {
        if server_driver_exited_check.load(Ordering::SeqCst) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    assert!(
        server_driver_exited_check.load(Ordering::SeqCst),
        "server-side driver should have exited after close"
    );
}

// r[verify rpc.caller.liveness.last-drop-closes-connection]
#[tokio::test]
async fn dropping_last_virtual_caller_closes_virtual_connection() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let server_driver_exited = Arc::new(AtomicBool::new(false));
    let server_driver_exited_check = server_driver_exited.clone();

    struct TrackingAcceptor {
        exited: Arc<AtomicBool>,
    }

    impl ConnectionAcceptor for TrackingAcceptor {
        fn accept(
            &self,
            _request: &ConnectionRequest,
            connection: PendingConnection,
        ) -> Result<(), Metadata<'static>> {
            let exited = self.exited.clone();
            let handle = connection.into_handle();
            let mut driver = Driver::new(handle, EchoHandler);
            moire::task::spawn(
                async move {
                    driver.run().await;
                    exited.store(true, Ordering::SeqCst);
                }
                .named("vconn_server_driver"),
            );
            Ok(())
        }
    }

    let server_task = moire::task::spawn(
        async move {
            let server_caller = acceptor_conduit(server_conduit, test_acceptor_handshake())
                .on_connection(TrackingAcceptor {
                    exited: server_driver_exited,
                })
                .establish::<NoopClient>(())
                .await
                .expect("server handshake failed");
            server_caller
        }
        .named("server_setup"),
    );

    let _client_caller_guard = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<NoopClient>(())
        .await
        .expect("client handshake failed");
    let session_handle = _client_caller_guard.session.clone().unwrap();

    let _server_caller_guard = server_task.await.expect("server setup failed");

    let vconn_handle = session_handle
        .open_connection(
            ConnectionSettings {
                parity: Parity::Odd,
                max_concurrent_requests: 64,
            },
            vec![],
        )
        .await
        .expect("open virtual connection");

    let mut vconn_driver = Driver::new(vconn_handle, ());
    let vconn_caller = crate::Caller::new(vconn_driver.caller());
    moire::task::spawn(async move { vconn_driver.run().await }.named("vconn_client_driver"));

    let response = vconn_caller
        .call(RequestCall {
            method_id: MethodId(1),
            args: Payload::outgoing(&11_u32),
            schemas: Default::default(),
            metadata: Default::default(),
        })
        .await
        .expect("call should succeed before dropping virtual caller");
    let ret_bytes = match &response.ret {
        Payload::PostcardBytes(bytes) => *bytes,
        _ => panic!("expected incoming payload in response"),
    };
    let echoed: u32 = vox_postcard::from_slice(ret_bytes).expect("deserialize response");
    assert_eq!(echoed, 11);

    drop(vconn_caller);

    for _ in 0..20 {
        if server_driver_exited_check.load(Ordering::SeqCst) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    assert!(
        server_driver_exited_check.load(Ordering::SeqCst),
        "server-side virtual driver should exit after last virtual caller drops"
    );
}

// r[verify connection.close.semantics]
// r[verify rpc.channel.close]
#[tokio::test]
async fn close_virtual_connection_closes_registered_rx_channels() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let server_task = moire::task::spawn(
        async move {
            let server_caller = acceptor_conduit(server_conduit, test_acceptor_handshake())
                .on_connection(EchoAcceptor)
                .establish::<NoopClient>(())
                .await
                .expect("server handshake failed");
            server_caller
        }
        .named("server_setup"),
    );

    let _client_caller_guard = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<NoopClient>(())
        .await
        .expect("client handshake failed");
    let session_handle = _client_caller_guard.session.clone().unwrap();

    let _server_caller_guard = server_task.await.expect("server setup failed");

    let vconn_handle = session_handle
        .open_connection(
            ConnectionSettings {
                parity: Parity::Odd,
                max_concurrent_requests: 64,
            },
            vec![],
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
        .close_connection(conn_id, vec![])
        .await
        .expect("close virtual connection");

    let recv_result = tokio::time::timeout(std::time::Duration::from_millis(200), rx_items.recv())
        .await
        .expect("timed out waiting for channel receiver to close");
    assert!(
        recv_result.is_none(),
        "registered Rx channel should close when virtual connection closes"
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
        ) -> Result<(), Metadata<'static>> {
            connection.handle_with(EchoHandler);
            Ok(())
        }
    }

    let server_task = moire::task::spawn(
        async move {
            let server_caller = acceptor_conduit(server_conduit, test_acceptor_handshake())
                .on_connection(LocalEchoAcceptor)
                .establish::<NoopClient>(())
                .await
                .expect("server handshake failed");
            server_caller
        }
        .named("server_setup"),
    );

    let root_caller = initiator_conduit(client_conduit, test_initiator_handshake())
        .spawn_fn(move |fut| {
            let handle = moire::task::spawn(fut.named("client_session"));
            let _ = client_session_tx.send(handle);
        })
        .establish::<NoopClient>(())
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
            },
            vec![],
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
            method_id: MethodId(1),
            args: Payload::outgoing(&7_u32),
            schemas: Default::default(),
            metadata: Default::default(),
        })
        .await
        .expect("virtual connection should still be usable after root caller drop");
    let ret_bytes = match &response.ret {
        Payload::PostcardBytes(bytes) => *bytes,
        _ => panic!("expected incoming payload in response"),
    };
    let echoed: u32 = vox_postcard::from_slice(ret_bytes).expect("deserialize response");
    assert_eq!(echoed, 7);

    drop(vconn_caller);
    drop(server_caller_guard);

    tokio::time::timeout(std::time::Duration::from_millis(500), client_session)
        .await
        .expect("timed out waiting for client session to exit")
        .expect("client session task failed");
}
