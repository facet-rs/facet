use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use moire::task::FutureExt;
use roam_types::{
    Caller, ChannelBinder, ChannelBody, ChannelClose, ChannelGrantCredit, ChannelId, ChannelItem,
    ChannelMessage, ChannelSink, Conduit, ConduitRx, ConnectionSettings, Handler,
    IncomingChannelMessage, Message, MessageFamily, MessagePayload, Metadata, MethodId, Parity,
    Payload, ReplySink, RequestBody, RequestCall, RequestCancel, RequestMessage, RequestResponse,
    RoamError, SelfRef,
};

use crate::session::{
    AcceptedConnection, ConnectionAcceptor, ConnectionMessage, SessionError,
    SessionKeepaliveConfig, acceptor, initiator,
};
use crate::{BareConduit, Driver, DriverReplySink, memory_link_pair};

type MessageConduit = BareConduit<MessageFamily, crate::MemoryLink>;

fn message_conduit_pair() -> (MessageConduit, MessageConduit) {
    let (a, b) = memory_link_pair(64);
    (BareConduit::new(a), BareConduit::new(b))
}

/// Conduit wrapper used by keepalive tests: drops inbound Pong messages.
struct DropPongConduit<C> {
    inner: C,
}

impl<C> DropPongConduit<C> {
    fn new(inner: C) -> Self {
        Self { inner }
    }
}

impl<C> Conduit for DropPongConduit<C>
where
    C: Conduit<Msg = MessageFamily>,
{
    type Msg = MessageFamily;
    type Tx = C::Tx;
    type Rx = DropPongRx<C::Rx>;

    fn split(self) -> (Self::Tx, Self::Rx) {
        let (tx, rx) = self.inner.split();
        (tx, DropPongRx { inner: rx })
    }
}

struct DropPongRx<Rx> {
    inner: Rx,
}

impl<Rx> ConduitRx for DropPongRx<Rx>
where
    Rx: ConduitRx<Msg = MessageFamily>,
{
    type Msg = MessageFamily;
    type Error = Rx::Error;

    async fn recv(&mut self) -> Result<Option<SelfRef<Message<'static>>>, Self::Error> {
        loop {
            let Some(msg) = self.inner.recv().await? else {
                return Ok(None);
            };
            if matches!(&msg.payload, MessagePayload::Pong(_)) {
                continue;
            }
            return Ok(Some(msg));
        }
    }
}

/// A handler that echoes back the raw args payload as the response.
struct EchoHandler;

impl Handler<DriverReplySink> for EchoHandler {
    async fn handle(&self, call: SelfRef<RequestCall<'static>>, reply: DriverReplySink) {
        let args_bytes = match &call.args {
            Payload::Incoming(bytes) => *bytes,
            _ => panic!("expected incoming payload"),
        };

        let result: u32 = facet_postcard::from_slice(args_bytes).expect("deserialize args");
        reply
            .send_reply(RequestResponse {
                ret: Payload::outgoing(&result),
                channels: vec![],
                metadata: Default::default(),
            })
            .await;
    }
}

/// A handler that blocks forever until its task is cancelled.
/// Tracks whether cancellation occurred via a drop guard.
struct BlockingHandler {
    was_cancelled: Arc<AtomicBool>,
}

impl Handler<DriverReplySink> for BlockingHandler {
    async fn handle(&self, _call: SelfRef<RequestCall<'static>>, reply: DriverReplySink) {
        let was_cancelled = self.was_cancelled.clone();
        // Hold the reply to prevent premature DriverReplySink::drop
        let _reply = reply;
        // Create a drop guard that records cancellation
        struct DropGuard(Arc<AtomicBool>);
        impl Drop for DropGuard {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        let _guard = DropGuard(was_cancelled);
        // Block forever — only cancellation (abort) will stop this
        std::future::pending::<()>().await;
    }
}

// r[verify rpc.cancel]
// r[verify rpc.cancel.channels]
#[tokio::test]
async fn cancel_aborts_in_flight_handler() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let was_cancelled = Arc::new(AtomicBool::new(false));
    let was_cancelled_check = was_cancelled.clone();

    let server_task = moire::task::spawn(
        async move {
            let (mut server_session, server_handle, _sh) = acceptor(server_conduit)
                .establish()
                .await
                .expect("server handshake failed");
            let mut server_driver = Driver::new(
                server_handle,
                BlockingHandler { was_cancelled },
                Parity::Even,
            );
            moire::task::spawn(async move { server_session.run().await }.named("server_session"));
            moire::task::spawn(async move { server_driver.run().await }.named("server_driver"));
        }
        .named("server_setup"),
    );

    // Set up client side. We need both the Caller (for sending the call) and
    // the raw sender (for sending the cancel message with the same request ID).
    let (mut client_session, client_handle, _sh) = initiator(client_conduit)
        .establish()
        .await
        .expect("client handshake failed");
    let client_sender = client_handle.sender.clone();
    let mut client_driver = Driver::new(client_handle, (), Parity::Odd);
    let caller = client_driver.caller();
    moire::task::spawn(async move { client_session.run().await }.named("client_session"));
    moire::task::spawn(async move { client_driver.run().await }.named("client_driver"));

    server_task.await.expect("server setup failed");

    // Spawn the call as a task so we can concurrently send a cancel.
    let call_task = moire::task::spawn(
        async move {
            let args_value: u32 = 99;
            caller
                .call(RequestCall {
                    method_id: MethodId(1),
                    args: Payload::outgoing(&args_value),
                    channels: vec![],
                    metadata: Default::default(),
                })
                .await
        }
        .named("client_call"),
    );

    // Give the call time to reach the server and start the handler.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Send a cancel for request ID 1 (the first request on an Odd-parity
    // connection allocates ID 1).
    let cancel_req_id = roam_types::RequestId(1);
    client_sender
        .send(ConnectionMessage::Request(RequestMessage {
            id: cancel_req_id,
            body: RequestBody::Cancel(RequestCancel {
                metadata: Metadata::default(),
            }),
        }))
        .await
        .expect("send cancel");

    // The call should resolve with an Err(Cancelled) in the wire Result envelope.
    let result = call_task.await.expect("call task join");
    let response = result.expect("call should receive a response");
    let ret_bytes = match &response.ret {
        Payload::Incoming(bytes) => *bytes,
        _ => panic!("expected incoming payload in response"),
    };
    let error: Result<(), RoamError> =
        facet_postcard::from_slice(ret_bytes).expect("deserialize response");
    assert!(
        matches!(error, Err(RoamError::Cancelled)),
        "expected Err(RoamError::Cancelled) in response payload"
    );

    // Wait for the handler abort to propagate (drop guard sets the flag).
    for _ in 0..20 {
        if was_cancelled_check.load(Ordering::SeqCst) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    // Verify the handler was actually cancelled (drop guard fired).
    assert!(
        was_cancelled_check.load(Ordering::SeqCst),
        "handler should have been cancelled"
    );
}

#[tokio::test]
async fn in_flight_call_returns_cancelled_when_peer_closes() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let was_cancelled = Arc::new(AtomicBool::new(false));
    let was_cancelled_check = was_cancelled.clone();

    let server_task = moire::task::spawn(
        async move {
            let (mut server_session, server_handle, _sh) = acceptor(server_conduit)
                .establish()
                .await
                .expect("server handshake failed");
            let mut server_driver = Driver::new(
                server_handle,
                BlockingHandler { was_cancelled },
                Parity::Even,
            );
            let session_task = moire::task::spawn(
                async move { server_session.run().await }.named("server_session"),
            );
            let driver_task =
                moire::task::spawn(async move { server_driver.run().await }.named("server_driver"));
            (session_task, driver_task)
        }
        .named("server_setup"),
    );

    let (mut client_session, client_handle, _sh) = initiator(client_conduit)
        .establish()
        .await
        .expect("client handshake failed");
    let mut client_driver = Driver::new(client_handle, (), Parity::Odd);
    let caller = client_driver.caller();
    moire::task::spawn(async move { client_session.run().await }.named("client_session"));
    moire::task::spawn(async move { client_driver.run().await }.named("client_driver"));

    let (server_session_task, _server_driver_task) =
        server_task.await.expect("server setup failed");

    let call_task = moire::task::spawn(
        async move {
            caller
                .call(RequestCall {
                    method_id: MethodId(1),
                    args: Payload::outgoing(&123_u32),
                    channels: vec![],
                    metadata: Default::default(),
                })
                .await
        }
        .named("client_call"),
    );

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    server_session_task.abort();

    let result = tokio::time::timeout(std::time::Duration::from_millis(500), call_task)
        .await
        .expect("timed out waiting for call to resolve after peer close")
        .expect("call task join");
    assert!(
        matches!(result, Err(RoamError::Cancelled)),
        "expected cancelled after peer close"
    );

    for _ in 0..20 {
        if was_cancelled_check.load(Ordering::SeqCst) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(
        was_cancelled_check.load(Ordering::SeqCst),
        "server handler should be cancelled when peer connection closes"
    );
}

#[tokio::test]
async fn keepalive_timeout_returns_cancelled_when_pongs_are_missing() {
    let (client_link, server_link) = memory_link_pair(64);
    let client_conduit = DropPongConduit::new(BareConduit::new(client_link));
    let server_conduit = BareConduit::new(server_link);

    let server_task = moire::task::spawn(
        async move {
            let (mut server_session, server_handle, _sh) = acceptor(server_conduit)
                .establish()
                .await
                .expect("server handshake failed");
            let mut server_driver = Driver::new(
                server_handle,
                BlockingHandler {
                    was_cancelled: Arc::new(AtomicBool::new(false)),
                },
                Parity::Even,
            );
            moire::task::spawn(async move { server_session.run().await }.named("server_session"));
            moire::task::spawn(async move { server_driver.run().await }.named("server_driver"));
        }
        .named("server_setup"),
    );

    let (mut client_session, client_handle, _sh) = initiator(client_conduit)
        .keepalive(SessionKeepaliveConfig {
            ping_interval: std::time::Duration::from_millis(20),
            pong_timeout: std::time::Duration::from_millis(50),
        })
        .establish()
        .await
        .expect("client handshake failed");
    let mut client_driver = Driver::new(client_handle, (), Parity::Odd);
    let caller = client_driver.caller();
    moire::task::spawn(async move { client_session.run().await }.named("client_session"));
    moire::task::spawn(async move { client_driver.run().await }.named("client_driver"));

    server_task.await.expect("server setup failed");

    let call_task = moire::task::spawn(
        async move {
            caller
                .call(RequestCall {
                    method_id: MethodId(1),
                    args: Payload::outgoing(&123_u32),
                    channels: vec![],
                    metadata: Default::default(),
                })
                .await
        }
        .named("client_call"),
    );

    let result = tokio::time::timeout(std::time::Duration::from_secs(1), call_task)
        .await
        .expect("timed out waiting for call to resolve after keepalive timeout")
        .expect("call task join");
    assert!(
        matches!(result, Err(RoamError::Cancelled)),
        "expected cancelled after keepalive timeout"
    );
}

// ---------------------------------------------------------------------------
// Virtual connection tests
// ---------------------------------------------------------------------------

/// An acceptor that spawns an EchoHandler driver on each accepted connection.
struct EchoAcceptor;

impl ConnectionAcceptor for EchoAcceptor {
    fn accept(
        &self,
        _conn_id: roam_types::ConnectionId,
        peer_settings: &ConnectionSettings,
        _metadata: &[roam_types::MetadataEntry],
    ) -> Result<AcceptedConnection, Metadata<'static>> {
        let peer_parity = peer_settings.parity;
        Ok(AcceptedConnection {
            settings: ConnectionSettings {
                parity: peer_parity.other(),
                max_concurrent_requests: 64,
            },
            metadata: vec![],
            setup: Box::new(move |handle| {
                let mut driver = Driver::new(handle, EchoHandler, peer_parity.other());
                moire::task::spawn(async move { driver.run().await }.named("vconn_server_driver"));
            }),
        })
    }
}

/// An acceptor that rejects every connection.
struct RejectAcceptor;

impl ConnectionAcceptor for RejectAcceptor {
    fn accept(
        &self,
        _conn_id: roam_types::ConnectionId,
        _peer_settings: &ConnectionSettings,
        _metadata: &[roam_types::MetadataEntry],
    ) -> Result<AcceptedConnection, Metadata<'static>> {
        Err(vec![])
    }
}

// r[verify rpc.virtual-connection.open]
// r[verify rpc.virtual-connection.accept]
// r[verify connection.open]
#[tokio::test]
async fn open_virtual_connection_and_call() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let server_task = moire::task::spawn(
        async move {
            let (mut server_session, server_handle, _sh) = acceptor(server_conduit)
                .on_connection(EchoAcceptor)
                .establish()
                .await
                .expect("server handshake failed");
            let mut server_driver = Driver::new(server_handle, (), Parity::Even);
            moire::task::spawn(async move { server_session.run().await }.named("server_session"));
            moire::task::spawn(async move { server_driver.run().await }.named("server_driver"));
        }
        .named("server_setup"),
    );

    let (mut client_session, _client_handle, session_handle) = initiator(client_conduit)
        .establish()
        .await
        .expect("client handshake failed");
    moire::task::spawn(async move { client_session.run().await }.named("client_session"));

    server_task.await.expect("server setup failed");

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
    let mut vconn_driver = Driver::new(vconn_handle, (), Parity::Odd);
    let caller = vconn_driver.caller();
    moire::task::spawn(async move { vconn_driver.run().await }.named("vconn_client_driver"));

    // Make a call on the virtual connection.
    let args_value: u32 = 123;
    let response = caller
        .call(RequestCall {
            method_id: MethodId(1),
            args: Payload::outgoing(&args_value),
            channels: vec![],
            metadata: Default::default(),
        })
        .await
        .expect("call should succeed");

    let ret_bytes = match &response.ret {
        Payload::Incoming(bytes) => *bytes,
        _ => panic!("expected incoming payload in response"),
    };
    let result: u32 = facet_postcard::from_slice(ret_bytes).expect("deserialize response");
    assert_eq!(result, 123);
}

#[tokio::test]
async fn initiator_builder_customization_controls_allocated_connection_parity() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let server_task = moire::task::spawn(
        async move {
            let (mut server_session, server_handle, _sh) = acceptor(server_conduit)
                .root_settings(ConnectionSettings {
                    parity: Parity::Odd,
                    max_concurrent_requests: 8,
                })
                .max_concurrent_requests(32)
                .metadata(vec![])
                .on_connection(EchoAcceptor)
                .establish()
                .await
                .expect("server handshake failed");
            let mut server_driver = Driver::new(server_handle, (), Parity::Even);
            moire::task::spawn(async move { server_session.run().await }.named("server_session"));
            moire::task::spawn(async move { server_driver.run().await }.named("server_driver"));
        }
        .named("server_setup"),
    );

    let (mut client_session, _client_handle, session_handle) = initiator(client_conduit)
        .root_settings(ConnectionSettings {
            parity: Parity::Odd,
            max_concurrent_requests: 4,
        })
        .max_concurrent_requests(64)
        .metadata(vec![])
        .parity(Parity::Even)
        .establish()
        .await
        .expect("client handshake failed");
    moire::task::spawn(async move { client_session.run().await }.named("client_session"));

    server_task.await.expect("server setup failed");

    let vconn_handle = session_handle
        .open_connection(
            ConnectionSettings {
                parity: Parity::Even,
                max_concurrent_requests: 64,
            },
            vec![],
        )
        .await
        .expect("open virtual connection");

    let conn_id = vconn_handle.connection_id();
    assert!(
        conn_id.has_parity(Parity::Even),
        "initiator parity should drive allocated connection ids"
    );
}

#[tokio::test]
async fn acceptor_builder_customization_supports_opening_connections() {
    let (initiator_conduit, acceptor_conduit) = message_conduit_pair();

    let initiator_task = moire::task::spawn(
        async move {
            let (mut initiator_session, initiator_handle, _initiator_session_handle) =
                initiator(initiator_conduit)
                    .parity(Parity::Even)
                    .metadata(vec![])
                    .on_connection(EchoAcceptor)
                    .establish()
                    .await
                    .expect("initiator handshake failed");
            let mut initiator_driver = Driver::new(initiator_handle, (), Parity::Even);
            moire::task::spawn(
                async move { initiator_session.run().await }.named("initiator_session"),
            );
            moire::task::spawn(
                async move { initiator_driver.run().await }.named("initiator_driver"),
            );
        }
        .named("initiator_setup"),
    );

    let (mut acceptor_session, _acceptor_handle, acceptor_session_handle) =
        acceptor(acceptor_conduit)
            .root_settings(ConnectionSettings {
                parity: Parity::Even,
                max_concurrent_requests: 2,
            })
            .max_concurrent_requests(32)
            .metadata(vec![])
            .establish()
            .await
            .expect("acceptor handshake failed");
    moire::task::spawn(async move { acceptor_session.run().await }.named("acceptor_session"));

    initiator_task.await.expect("initiator setup failed");

    let vconn_handle = acceptor_session_handle
        .open_connection(
            ConnectionSettings {
                parity: Parity::Odd,
                max_concurrent_requests: 64,
            },
            vec![],
        )
        .await
        .expect("acceptor opens virtual connection");

    let conn_id = vconn_handle.connection_id();
    assert!(
        conn_id.has_parity(Parity::Odd),
        "acceptor should allocate odd ids when peer initiator parity is even"
    );
}

// r[verify connection.open.rejection]
#[tokio::test]
async fn reject_virtual_connection() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let server_task = moire::task::spawn(
        async move {
            let (mut server_session, server_handle, _sh) = acceptor(server_conduit)
                .on_connection(RejectAcceptor)
                .establish()
                .await
                .expect("server handshake failed");
            let mut server_driver = Driver::new(server_handle, (), Parity::Even);
            moire::task::spawn(async move { server_session.run().await }.named("server_session"));
            moire::task::spawn(async move { server_driver.run().await }.named("server_driver"));
        }
        .named("server_setup"),
    );

    let (mut client_session, _client_handle, session_handle) = initiator(client_conduit)
        .establish()
        .await
        .expect("client handshake failed");
    moire::task::spawn(async move { client_session.run().await }.named("client_session"));

    server_task.await.expect("server setup failed");

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
            let (mut server_session, server_handle, _sh) = acceptor(server_conduit)
                .establish()
                .await
                .expect("server handshake failed");
            let mut server_driver = Driver::new(server_handle, (), Parity::Even);
            moire::task::spawn(async move { server_session.run().await }.named("server_session"));
            moire::task::spawn(async move { server_driver.run().await }.named("server_driver"));
        }
        .named("server_setup"),
    );

    let (mut client_session, _client_handle, session_handle) = initiator(client_conduit)
        .establish()
        .await
        .expect("client handshake failed");
    moire::task::spawn(async move { client_session.run().await }.named("client_session"));

    server_task.await.expect("server setup failed");

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
async fn close_root_connection_is_rejected() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let server_task = moire::task::spawn(
        async move {
            let (mut server_session, server_handle, _sh) = acceptor(server_conduit)
                .establish()
                .await
                .expect("server handshake failed");
            let mut server_driver = Driver::new(server_handle, EchoHandler, Parity::Even);
            moire::task::spawn(async move { server_session.run().await }.named("server_session"));
            moire::task::spawn(async move { server_driver.run().await }.named("server_driver"));
        }
        .named("server_setup"),
    );

    let (mut client_session, _client_handle, session_handle) = initiator(client_conduit)
        .establish()
        .await
        .expect("client handshake failed");
    moire::task::spawn(async move { client_session.run().await }.named("client_session"));

    server_task.await.expect("server setup failed");

    let result = session_handle
        .close_connection(roam_types::ConnectionId::ROOT, vec![])
        .await;
    assert!(
        matches!(result, Err(SessionError::Protocol(ref msg)) if msg == "cannot close root connection"),
        "expected root-close protocol error, got: {result:?}"
    );
}

// r[verify connection.close]
#[tokio::test]
async fn close_unknown_virtual_connection_is_rejected() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let server_task = moire::task::spawn(
        async move {
            let (mut server_session, server_handle, _sh) = acceptor(server_conduit)
                .establish()
                .await
                .expect("server handshake failed");
            let mut server_driver = Driver::new(server_handle, EchoHandler, Parity::Even);
            moire::task::spawn(async move { server_session.run().await }.named("server_session"));
            moire::task::spawn(async move { server_driver.run().await }.named("server_driver"));
        }
        .named("server_setup"),
    );

    let (mut client_session, _client_handle, session_handle) = initiator(client_conduit)
        .establish()
        .await
        .expect("client handshake failed");
    moire::task::spawn(async move { client_session.run().await }.named("client_session"));

    server_task.await.expect("server setup failed");

    let missing_conn_id = roam_types::ConnectionId(1);
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
            _conn_id: roam_types::ConnectionId,
            peer_settings: &ConnectionSettings,
            _metadata: &[roam_types::MetadataEntry],
        ) -> Result<AcceptedConnection, Metadata<'static>> {
            let peer_parity = peer_settings.parity;
            let exited = self.exited.clone();
            Ok(AcceptedConnection {
                settings: ConnectionSettings {
                    parity: peer_parity.other(),
                    max_concurrent_requests: 64,
                },
                metadata: vec![],
                setup: Box::new(move |handle| {
                    let mut driver = Driver::new(handle, EchoHandler, peer_parity.other());
                    moire::task::spawn(
                        async move {
                            driver.run().await;
                            exited.store(true, Ordering::SeqCst);
                        }
                        .named("vconn_server_driver"),
                    );
                }),
            })
        }
    }

    let server_task = moire::task::spawn(
        async move {
            let (mut server_session, server_handle, _sh) = acceptor(server_conduit)
                .on_connection(TrackingAcceptor {
                    exited: server_driver_exited,
                })
                .establish()
                .await
                .expect("server handshake failed");
            let mut server_driver = Driver::new(server_handle, (), Parity::Even);
            moire::task::spawn(async move { server_session.run().await }.named("server_session"));
            moire::task::spawn(async move { server_driver.run().await }.named("server_driver"));
        }
        .named("server_setup"),
    );

    let (mut client_session, _client_handle, session_handle) = initiator(client_conduit)
        .establish()
        .await
        .expect("client handshake failed");
    moire::task::spawn(async move { client_session.run().await }.named("client_session"));

    server_task.await.expect("server setup failed");

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
    let mut vconn_driver = Driver::new(vconn_handle, (), Parity::Odd);
    let caller = vconn_driver.caller();
    moire::task::spawn(async move { vconn_driver.run().await }.named("vconn_client_driver"));

    // Make a call to confirm the connection works.
    let args_value: u32 = 42;
    let response = caller
        .call(RequestCall {
            method_id: MethodId(1),
            args: Payload::outgoing(&args_value),
            channels: vec![],
            metadata: Default::default(),
        })
        .await
        .expect("call should succeed before close");

    let ret_bytes = match &response.ret {
        Payload::Incoming(bytes) => *bytes,
        _ => panic!("expected incoming payload"),
    };
    let result: u32 = facet_postcard::from_slice(ret_bytes).expect("deserialize");
    assert_eq!(result, 42);

    // Close the virtual connection.
    session_handle
        .close_connection(conn_id, vec![])
        .await
        .expect("close virtual connection");

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

// r[verify connection.close.semantics]
// r[verify rpc.channel.close]
#[tokio::test]
async fn close_virtual_connection_closes_registered_rx_channels() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let server_task = moire::task::spawn(
        async move {
            let (mut server_session, server_handle, _sh) = acceptor(server_conduit)
                .on_connection(EchoAcceptor)
                .establish()
                .await
                .expect("server handshake failed");
            let mut server_driver = Driver::new(server_handle, (), Parity::Even);
            moire::task::spawn(async move { server_session.run().await }.named("server_session"));
            moire::task::spawn(async move { server_driver.run().await }.named("server_driver"));
        }
        .named("server_setup"),
    );

    let (mut client_session, _client_handle, session_handle) = initiator(client_conduit)
        .establish()
        .await
        .expect("client handshake failed");
    moire::task::spawn(async move { client_session.run().await }.named("client_session"));

    server_task.await.expect("server setup failed");

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
    let mut vconn_driver = Driver::new(vconn_handle, (), Parity::Odd);
    let caller = vconn_driver.caller();
    moire::task::spawn(async move { vconn_driver.run().await }.named("vconn_client_driver"));

    let (_channel_id, mut rx_items) = caller.create_rx();

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

#[tokio::test]
async fn echo_call_across_memory_link() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    // Server and client handshakes must run concurrently — both sides exchange
    // settings before either can proceed.
    let server_task = moire::task::spawn(
        async move {
            let (mut server_session, server_handle, _sh) = acceptor(server_conduit)
                .establish()
                .await
                .expect("server handshake failed");
            let mut server_driver = Driver::new(server_handle, EchoHandler, Parity::Even);
            moire::task::spawn(async move { server_session.run().await }.named("server_session"));
            moire::task::spawn(async move { server_driver.run().await }.named("server_driver"));
        }
        .named("server_setup"),
    );

    // Set up client side (runs concurrently with server_task above).
    let (mut client_session, client_handle, _sh) = initiator(client_conduit)
        .establish()
        .await
        .expect("client handshake failed");
    let mut client_driver = Driver::new(client_handle, (), Parity::Odd);
    let caller = client_driver.caller();
    moire::task::spawn(async move { client_session.run().await }.named("client_session"));
    moire::task::spawn(async move { client_driver.run().await }.named("client_driver"));

    server_task.await.expect("server setup failed");

    // Make a call: serialize a u32 as the args payload.
    let args_value: u32 = 42;
    let response = caller
        .call(RequestCall {
            method_id: MethodId(1),
            args: Payload::outgoing(&args_value),
            channels: vec![],
            metadata: Default::default(),
        })
        .await
        .expect("call should succeed");

    // The echo handler sends back the same bytes. Deserialize the response.
    let ret_bytes = match &response.ret {
        Payload::Incoming(bytes) => *bytes,
        _ => panic!("expected incoming payload in response"),
    };
    let result: u32 = facet_postcard::from_slice(ret_bytes).expect("deserialize response");
    assert_eq!(result, 42);
}

#[tokio::test]
async fn buffers_inbound_channel_items_until_rx_is_registered() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let server_task = moire::task::spawn(
        async move {
            let (mut server_session, server_handle, _sh) = acceptor(server_conduit)
                .establish()
                .await
                .expect("server handshake failed");
            let mut server_driver = Driver::new(server_handle, (), Parity::Even);
            let server_caller = server_driver.caller();
            moire::task::spawn(async move { server_session.run().await }.named("server_session"));
            moire::task::spawn(async move { server_driver.run().await }.named("server_driver"));
            server_caller
        }
        .named("server_setup"),
    );

    let (mut client_session, client_handle, _sh) = initiator(client_conduit)
        .establish()
        .await
        .expect("client handshake failed");
    let client_sender = client_handle.sender.clone();
    moire::task::spawn(async move { client_session.run().await }.named("client_session"));

    let server_caller = server_task.await.expect("server setup failed");

    let channel_id = ChannelId(99);
    let value = 123_u32;
    client_sender
        .send(crate::session::ConnectionMessage::Channel(ChannelMessage {
            id: channel_id,
            body: ChannelBody::Item(ChannelItem {
                item: Payload::outgoing(&value),
            }),
        }))
        .await
        .expect("send channel item");

    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    let mut rx = server_caller.register_rx_channel(channel_id);
    let msg = tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv())
        .await
        .expect("timed out waiting for buffered channel item")
        .expect("channel receiver closed unexpectedly");

    let IncomingChannelMessage::Item(item) = msg else {
        panic!("expected buffered item");
    };
    let bytes = match item.item {
        Payload::Incoming(bytes) => bytes,
        _ => panic!("expected incoming payload"),
    };
    let decoded: u32 = facet_postcard::from_slice(bytes).expect("deserialize buffered item");
    assert_eq!(decoded, 123);
}

#[tokio::test]
async fn grant_credit_unblocks_driver_created_tx_channel() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let server_task = moire::task::spawn(
        async move {
            let (mut server_session, server_handle, _sh) = acceptor(server_conduit)
                .establish()
                .await
                .expect("server handshake failed");
            let mut server_driver = Driver::new(server_handle, (), Parity::Even);
            let server_caller = server_driver.caller();
            moire::task::spawn(async move { server_session.run().await }.named("server_session"));
            moire::task::spawn(async move { server_driver.run().await }.named("server_driver"));
            server_caller
        }
        .named("server_setup"),
    );

    let (mut client_session, client_handle, _sh) = initiator(client_conduit)
        .establish()
        .await
        .expect("client handshake failed");
    let client_sender = client_handle.sender.clone();
    moire::task::spawn(async move { client_session.run().await }.named("client_session"));

    let server_caller = server_task.await.expect("server setup failed");
    let (channel_id, sink) = server_caller.create_tx_channel(0);

    let send_task = moire::task::spawn(async move {
        let value = 42_u32;
        sink.send_payload(Payload::outgoing(&value)).await
    });

    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    assert!(
        !send_task.is_finished(),
        "send should block when initial credit is zero"
    );

    client_sender
        .send(crate::session::ConnectionMessage::Channel(ChannelMessage {
            id: channel_id,
            body: ChannelBody::GrantCredit(ChannelGrantCredit { additional: 1 }),
        }))
        .await
        .expect("send grant credit");

    let send_result = tokio::time::timeout(std::time::Duration::from_millis(200), send_task)
        .await
        .expect("timed out waiting for send to unblock")
        .expect("send task join");
    assert!(
        send_result.is_ok(),
        "send should succeed after credit grant"
    );
}

#[tokio::test]
async fn buffered_close_before_registration_keeps_channel_terminal() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let server_task = moire::task::spawn(
        async move {
            let (mut server_session, server_handle, _sh) = acceptor(server_conduit)
                .establish()
                .await
                .expect("server handshake failed");
            let mut server_driver = Driver::new(server_handle, (), Parity::Even);
            let server_caller = server_driver.caller();
            moire::task::spawn(async move { server_session.run().await }.named("server_session"));
            moire::task::spawn(async move { server_driver.run().await }.named("server_driver"));
            server_caller
        }
        .named("server_setup"),
    );

    let (mut client_session, client_handle, _sh) = initiator(client_conduit)
        .establish()
        .await
        .expect("client handshake failed");
    let client_sender = client_handle.sender.clone();
    moire::task::spawn(async move { client_session.run().await }.named("client_session"));

    let server_caller = server_task.await.expect("server setup failed");
    let channel_id = ChannelId(77);

    client_sender
        .send(crate::session::ConnectionMessage::Channel(ChannelMessage {
            id: channel_id,
            body: ChannelBody::Close(ChannelClose {
                metadata: Metadata::default(),
            }),
        }))
        .await
        .expect("send buffered close");

    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    let mut rx = server_caller.register_rx_channel(channel_id);
    let close = tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv())
        .await
        .expect("timed out waiting for buffered close")
        .expect("channel receiver closed before buffered close");
    assert!(
        matches!(close, IncomingChannelMessage::Close(_)),
        "expected buffered close first"
    );

    let value = 999_u32;
    client_sender
        .send(crate::session::ConnectionMessage::Channel(ChannelMessage {
            id: channel_id,
            body: ChannelBody::Item(ChannelItem {
                item: Payload::outgoing(&value),
            }),
        }))
        .await
        .expect("send post-close item");

    let next = tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv())
        .await
        .expect("timed out waiting for channel termination");
    assert!(
        next.is_none(),
        "channel should be terminal after buffered close"
    );
}

#[tokio::test]
async fn unsolicited_response_id_is_ignored_and_does_not_break_calls() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let server_task = moire::task::spawn(
        async move {
            let (mut server_session, server_handle, _sh) = acceptor(server_conduit)
                .establish()
                .await
                .expect("server handshake failed");
            let server_sender = server_handle.sender.clone();
            let mut server_driver = Driver::new(server_handle, EchoHandler, Parity::Even);
            moire::task::spawn(async move { server_session.run().await }.named("server_session"));
            moire::task::spawn(async move { server_driver.run().await }.named("server_driver"));
            server_sender
        }
        .named("server_setup"),
    );

    let (mut client_session, client_handle, _sh) = initiator(client_conduit)
        .establish()
        .await
        .expect("client handshake failed");
    let mut client_driver = Driver::new(client_handle, (), Parity::Odd);
    let caller = client_driver.caller();
    moire::task::spawn(async move { client_session.run().await }.named("client_session"));
    moire::task::spawn(async move { client_driver.run().await }.named("client_driver"));

    let server_sender = server_task.await.expect("server setup failed");

    server_sender
        .send(crate::session::ConnectionMessage::Request(RequestMessage {
            id: roam_types::RequestId(9999),
            body: RequestBody::Response(RequestResponse {
                ret: Payload::outgoing(&123_u32),
                channels: vec![],
                metadata: Default::default(),
            }),
        }))
        .await
        .expect("send unsolicited response");

    let args_value: u32 = 42;
    let response = caller
        .call(RequestCall {
            method_id: MethodId(1),
            args: Payload::outgoing(&args_value),
            channels: vec![],
            metadata: Default::default(),
        })
        .await
        .expect("call should still succeed after unsolicited response");

    let ret_bytes = match &response.ret {
        Payload::Incoming(bytes) => *bytes,
        _ => panic!("expected incoming payload"),
    };
    let result: u32 = facet_postcard::from_slice(ret_bytes).expect("deserialize response");
    assert_eq!(result, 42);
}
