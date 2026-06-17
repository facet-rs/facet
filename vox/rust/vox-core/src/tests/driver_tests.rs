use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use facet::Facet;
use vox_rt::task::FutureExt;
use vox_types::{
    Backing, ChannelBody, ChannelClose, ChannelDirection, ChannelGrantCredit, ChannelId,
    ChannelItem, ChannelMessage, ChannelSink, ConnectionRole, ConnectionSettings, Handler,
    HandshakeResult, IncomingChannelMessage, Message, MessagePayload, Metadata, MethodId, Parity,
    Payload, ReplySink, RequestBody, RequestCall, RequestCancel, RequestId, RequestMessage,
    RequestResponse, RequestTerminationReason, Rx, SelfRef, Tx, VoxError, channel,
};

use super::utils::*;
use crate::connection::{
    ConnectionError, ConnectionHandle, ConnectionKeepaliveConfig, ConnectionMessage, LaneAcceptor,
    LaneRejection, LaneRequest, PendingLane, acceptor_conduit, acceptor_on, initiator_conduit,
    initiator_on, proxy_lanes,
};
use crate::{BareConduit, Driver, RequestTimeoutPolicy, memory_link_pair};

fn acceptor_handshake_with_request_limits(our_max: u32, peer_max: u32) -> HandshakeResult {
    let mut handshake = test_acceptor_handshake();
    handshake.our_settings.max_concurrent_requests = our_max;
    handshake.peer_settings.max_concurrent_requests = peer_max;
    handshake
}

fn initiator_handshake_with_request_limits(our_max: u32, peer_max: u32) -> HandshakeResult {
    let mut handshake = test_initiator_handshake();
    handshake.our_settings.max_concurrent_requests = our_max;
    handshake.peer_settings.max_concurrent_requests = peer_max;
    handshake
}

struct CaptureClientAcceptor<H> {
    handler: H,
    accepted: Mutex<Option<tokio::sync::oneshot::Sender<TestLaneClient>>>,
}

impl<H> CaptureClientAcceptor<H> {
    fn new(handler: H, accepted: tokio::sync::oneshot::Sender<TestLaneClient>) -> Self {
        Self {
            handler,
            accepted: Mutex::new(Some(accepted)),
        }
    }
}

impl<H> LaneAcceptor for CaptureClientAcceptor<H>
where
    H: Handler<crate::DriverReplySink> + Clone + Send + Sync + 'static,
{
    fn accept(&self, _request: &LaneRequest, lane: PendingLane) -> Result<(), LaneRejection> {
        let client = lane.handle_with_client::<TestLaneClient>(self.handler.clone());
        if let Some(accepted) = self
            .accepted
            .lock()
            .expect("accepted lane mutex poisoned")
            .take()
        {
            let _ = accepted.send(client);
        }
        Ok(())
    }
}

async fn captured_test_lane_pair_with<H>(
    client_conduit: MessageConduit,
    server_conduit: MessageConduit,
    client_handshake: HandshakeResult,
    server_handshake: HandshakeResult,
    handler: H,
) -> (
    TestLaneClient,
    TestLaneClient,
    crate::connection::ConnectionHandle,
)
where
    H: Handler<crate::DriverReplySink> + Clone + Send + Sync + 'static,
{
    captured_test_lane_pair_with_settings(
        client_conduit,
        server_conduit,
        client_handshake,
        server_handshake,
        ConnectionSettings {
            parity: Parity::Odd,
            max_concurrent_requests: 64,
            initial_channel_credit: vox_types::DEFAULT_INITIAL_CHANNEL_CREDIT,
        },
        handler,
    )
    .await
}

async fn captured_test_lane_pair_with_settings<H>(
    client_conduit: MessageConduit,
    server_conduit: MessageConduit,
    client_handshake: HandshakeResult,
    server_handshake: HandshakeResult,
    lane_settings: ConnectionSettings,
    handler: H,
) -> (
    TestLaneClient,
    TestLaneClient,
    crate::connection::ConnectionHandle,
)
where
    H: Handler<crate::DriverReplySink> + Clone + Send + Sync + 'static,
{
    let (accepted_tx, accepted_rx) = tokio::sync::oneshot::channel();
    let server_task = vox_rt::task::spawn(
        async move {
            let server_connection = acceptor_conduit(server_conduit, server_handshake)
                .on_connection(CaptureClientAcceptor::new(handler, accepted_tx))
                .establish_connection()
                .await
                .expect("server handshake failed");
            let server_client = accepted_rx.await.expect("server lane accepted");
            (server_connection, server_client)
        }
        .named("server_setup"),
    );

    let client_connection = initiator_conduit(client_conduit, client_handshake)
        .establish_connection()
        .await
        .expect("client handshake failed");
    let client = client_connection
        .open_lane_with_settings::<TestLaneClient>(lane_settings)
        .await
        .expect("client handshake failed");
    let (server_connection, server_client) = server_task.await.expect("server setup failed");
    (client, server_client, server_connection)
}

async fn captured_test_lane_pair_with_client_timeout<H>(
    idle_timeout: Duration,
    handler: H,
) -> (
    TestLaneClient,
    TestLaneClient,
    crate::connection::ConnectionHandle,
)
where
    H: Handler<crate::DriverReplySink> + Clone + Send + Sync + 'static,
{
    let (client_conduit, server_conduit) = message_conduit_pair();
    let lane_settings = ConnectionSettings {
        parity: Parity::Odd,
        max_concurrent_requests: 64,
        initial_channel_credit: vox_types::DEFAULT_INITIAL_CHANNEL_CREDIT,
    };

    let (accepted_tx, accepted_rx) = tokio::sync::oneshot::channel();
    let server_task = vox_rt::task::spawn(
        async move {
            let server_connection = acceptor_conduit(server_conduit, test_acceptor_handshake())
                .on_connection(CaptureClientAcceptor::new(handler, accepted_tx))
                .establish_connection()
                .await
                .expect("server handshake failed");
            let server_client = accepted_rx.await.expect("server lane accepted");
            (server_connection, server_client)
        }
        .named("server_setup"),
    );

    let client_connection = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish_connection()
        .await
        .expect("client handshake failed");
    let metadata = vox_types::metadata()
        .str(
            crate::VOX_SERVICE_METADATA_KEY,
            <TestLaneClient as crate::FromVoxLane>::SERVICE_NAME,
        )
        .build();
    let handle = client_connection
        .open_lane_handle(lane_settings, metadata)
        .await
        .expect("client lane open failed");
    let mut driver =
        Driver::with_request_timeout_policy(handle, (), RequestTimeoutPolicy::idle(idle_timeout));
    let client_caller = crate::Caller::new(driver.caller());
    tokio::spawn(async move { driver.run().await });
    let client = <TestLaneClient as crate::FromVoxLane>::from_vox_lane(
        client_caller,
        Some(client_connection),
    );
    let (server_connection, server_client) = server_task.await.expect("server setup failed");
    (client, server_client, server_connection)
}

#[derive(Clone, Copy)]
struct ImmediateReplyHandler;

impl Handler<crate::DriverReplySink> for ImmediateReplyHandler {
    async fn handle(
        &self,
        _call: SelfRef<RequestCall<'static>>,
        reply: crate::DriverReplySink,
        _schemas: std::sync::Arc<vox_types::SchemaRecvTracker>,
    ) {
        let result = 7_u32;
        reply
            .send_reply(RequestResponse {
                ret: Payload::outgoing(&result),
                schemas: Default::default(),
                metadata: Default::default(),
            })
            .await;
    }
}

#[derive(Facet)]
struct ReceiveArgs {
    input: Rx<u32>,
}

fn request_arg_bytes(call: &RequestCall<'_>) -> (Vec<ChannelId>, Vec<u8>) {
    let Payload::Encoded(bytes) = &call.args else {
        panic!("expected encoded request args");
    };
    (call.channels.clone(), bytes.to_vec())
}

fn decode_request_args<T: Facet<'static>>(
    channels: Vec<ChannelId>,
    bytes: &[u8],
    binder: &dyn vox_types::ChannelBinder,
) -> T {
    vox_types::channel::provide_channels(channels, || {
        vox_types::channel::with_channel_binder(binder, || {
            vox_phon::from_slice(bytes).expect("decode request args")
        })
    })
}

#[derive(Clone)]
struct CaptureRxBlockingHandler {
    captured_rx: Arc<Mutex<Option<tokio::sync::oneshot::Sender<Rx<u32>>>>>,
    was_cancelled: Arc<AtomicBool>,
}

impl Handler<crate::DriverReplySink> for CaptureRxBlockingHandler {
    async fn handle(
        &self,
        call: SelfRef<RequestCall<'static>>,
        reply: crate::DriverReplySink,
        _schemas: std::sync::Arc<vox_types::SchemaRecvTracker>,
    ) {
        let args = {
            let (channels, bytes) = request_arg_bytes(call.get());
            let binder = reply
                .channel_binder()
                .expect("reply sink should expose channel binder");
            decode_request_args::<ReceiveArgs>(channels, &bytes, binder)
        };
        if let Some(sender) = self
            .captured_rx
            .lock()
            .expect("captured rx mutex poisoned")
            .take()
        {
            let _ = sender.send(args.input);
        }

        let was_cancelled = self.was_cancelled.clone();
        let _reply = reply;
        struct DropGuard(Arc<AtomicBool>);
        impl Drop for DropGuard {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        let _guard = DropGuard(was_cancelled);
        std::future::pending::<()>().await;
    }
}

#[derive(Clone, Copy)]
struct ChannelProgressReplyHandler;

impl Handler<crate::DriverReplySink> for ChannelProgressReplyHandler {
    async fn handle(
        &self,
        call: SelfRef<RequestCall<'static>>,
        reply: crate::DriverReplySink,
        _schemas: std::sync::Arc<vox_types::SchemaRecvTracker>,
    ) {
        let mut input = {
            let (channels, bytes) = request_arg_bytes(call.get());
            let binder = reply
                .channel_binder()
                .expect("reply sink should expose channel binder");
            decode_request_args::<ReceiveArgs>(channels, &bytes, binder).input
        };

        let mut total = 0_u32;
        for _ in 0..3 {
            let item = input
                .recv()
                .await
                .expect("progress channel should stay open")
                .expect("progress channel should yield an item");
            total += *item.get();
        }

        reply
            .send_reply(RequestResponse {
                ret: Payload::outgoing(&total),
                schemas: Default::default(),
                metadata: Default::default(),
            })
            .await;
    }
}

async fn captured_test_lane_pair<H>(
    handler: H,
) -> (
    TestLaneClient,
    TestLaneClient,
    crate::connection::ConnectionHandle,
)
where
    H: Handler<crate::DriverReplySink> + Clone + Send + Sync + 'static,
{
    let (client_conduit, server_conduit) = message_conduit_pair();
    captured_test_lane_pair_with(
        client_conduit,
        server_conduit,
        test_initiator_handshake(),
        test_acceptor_handshake(),
        handler,
    )
    .await
}

async fn wait_for_outstanding_requests(caller: &crate::Caller, expected: usize) {
    for _ in 0..50 {
        let outstanding = caller
            .debug_snapshot()
            .connections
            .iter()
            .map(|connection| connection.outstanding_requests)
            .sum::<usize>();
        if outstanding == expected {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timed out waiting for {expected} outstanding requests");
}

async fn send_raw_root_call(
    sender: &crate::connection::ConnectionSender,
    request_id: RequestId,
    value: u32,
) {
    sender
        .send(ConnectionMessage::Request(RequestMessage {
            id: request_id,
            body: RequestBody::Call(RequestCall {
                channels: Vec::new(),
                method_id: MethodId(1),
                args: Payload::outgoing(&value),
                schemas: Default::default(),
                metadata: Default::default(),
            }),
        }))
        .await
        .expect("send raw call");
}

async fn expect_protocol_close(caller: &crate::Caller, label: &str) {
    tokio::time::timeout(Duration::from_millis(500), caller.closed())
        .await
        .unwrap_or_else(|_| panic!("{label} connection should close after protocol violation"));
    let snapshot = caller.debug_snapshot();
    assert_eq!(
        snapshot.connections[0].close_reason,
        Some(vox_types::ConnectionCloseReason::Protocol),
        "{label} close reason"
    );
}

// r[verify rpc.caller.liveness.refcounted]
// r[verify rpc.caller.liveness.root-internal-close]
// r[verify rpc.caller.liveness.root-teardown-condition]
// r[verify connection.lifecycle.driven]
// r[verify connection.shutdown.explicit]
#[tokio::test]
async fn dropping_root_callers_does_not_shutdown_connection() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let (client_session_tx, client_session_rx) =
        tokio::sync::oneshot::channel::<vox_rt::task::JoinHandle<()>>();
    let (server_session_tx, server_session_rx) =
        tokio::sync::oneshot::channel::<vox_rt::task::JoinHandle<()>>();

    let server_task = vox_rt::task::spawn(
        async move {
            acceptor_conduit(server_conduit, test_acceptor_handshake())
                .spawn_fn(move |fut| {
                    let handle = vox_rt::task::spawn(fut.named("server_session"));
                    let _ = server_session_tx.send(handle);
                })
                .on_connection(EchoHandler)
                .establish_connection()
                .await
                .expect("server handshake failed")
        }
        .named("server_setup"),
    );

    let caller = initiator_conduit(client_conduit, test_initiator_handshake())
        .spawn_fn(move |fut| {
            let handle = vox_rt::task::spawn(fut.named("client_session"));
            let _ = client_session_tx.send(handle);
        })
        .establish::<TestLaneClient>()
        .await
        .expect("client handshake failed");

    let server_connection = server_task.await.expect("server setup failed");
    let client_session = client_session_rx.await.expect("client session handle sent");
    let server_session = server_session_rx.await.expect("server session handle sent");
    let client_connection = caller.connection.clone().expect("client connection handle");

    let caller_clone = caller.clone();
    drop(caller_clone);

    let response = caller
        .caller
        .call(RequestCall {
            channels: Vec::new(),
            method_id: MethodId(1),
            args: Payload::outgoing(&42_u32),
            schemas: Default::default(),
            metadata: Default::default(),
        })
        .await
        .expect("call should still succeed while one root caller remains");
    let response = response.get();
    let ret_bytes = match &response.ret {
        Payload::Encoded(bytes) => bytes,
        _ => panic!("expected incoming payload in response"),
    };
    let echoed: u32 = vox_phon::from_slice(ret_bytes).expect("deserialize response");
    assert_eq!(echoed, 42);

    drop(caller);
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(
        !client_session.is_finished(),
        "dropping the last client caller must not shut down the client connection"
    );
    assert!(
        !server_session.is_finished(),
        "dropping the peer handle must not be needed to keep the server connection alive"
    );

    client_connection
        .shutdown()
        .expect("client shutdown request");
    let _ = server_connection.shutdown();

    tokio::time::timeout(std::time::Duration::from_millis(500), client_session)
        .await
        .expect("timed out waiting for client session to exit")
        .expect("client session task failed");
    tokio::time::timeout(std::time::Duration::from_millis(500), server_session)
        .await
        .expect("timed out waiting for server session to exit")
        .expect("server session task failed");
}

// r[verify rpc]
// r[verify rpc.channel]
// r[verify rpc.channel.allocation]
// r[verify rpc.channel.direction]
// r[verify rpc.channel.lifecycle]
// r[verify rpc.channel.binding.caller-args.tx]
#[tokio::test]
async fn bound_stream_rx_works_after_public_caller_drop_when_connection_is_driven() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let (client_session_tx, client_session_rx) =
        tokio::sync::oneshot::channel::<vox_rt::task::JoinHandle<()>>();
    let (accepted_tx, accepted_rx) = tokio::sync::oneshot::channel();

    let server_task = vox_rt::task::spawn(
        async move {
            let server_connection = acceptor_conduit(server_conduit, test_acceptor_handshake())
                .on_connection(CaptureClientAcceptor::new((), accepted_tx))
                .establish_connection()
                .await
                .expect("server handshake failed");
            let server_caller = accepted_rx.await.expect("server lane accepted");
            (server_connection, server_caller)
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

    let (server_connection, server_caller) = server_task.await.expect("server setup failed");
    let client_session = client_session_rx.await.expect("client session handle sent");
    let client_connection = root_caller
        .connection
        .clone()
        .expect("client connection handle");

    let (updates_tx, mut updates_rx) = channel::<u32>();
    let args = SubscribeArgs {
        updates: updates_tx,
    };
    // Serializing the args binds the Tx's paired Rx via the thread-local binder.
    let _bytes = vox_types::channel::with_channel_binder(root_caller.caller.driver(), || {
        vox_phon::to_vec(&args).expect("serialize args")
    });
    // The first allocated channel ID is 1 (odd parity).
    let channel_id = ChannelId(1);
    drop(args);
    drop(root_caller);

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(
        !client_session.is_finished(),
        "dropping the public caller must not close the driven connection"
    );

    let value = 123_u32;
    server_caller
        .caller
        .driver()
        .connection_sender()
        .send(ConnectionMessage::Channel(ChannelMessage {
            id: channel_id,
            body: ChannelBody::Item(ChannelItem {
                item: Payload::outgoing(&value),
            }),
        }))
        .await
        .expect("send channel item");

    let received = updates_rx
        .recv()
        .await
        .expect("stream should remain usable")
        .expect("channel should yield one item");
    let received = received.get();
    assert_eq!(*received, 123);

    server_caller
        .caller
        .driver()
        .connection_sender()
        .send(ConnectionMessage::Channel(ChannelMessage {
            id: channel_id,
            body: ChannelBody::Close(ChannelClose {
                metadata: Default::default(),
            }),
        }))
        .await
        .expect("send channel close");

    assert!(
        updates_rx
            .recv()
            .await
            .expect("close should be delivered")
            .is_none(),
        "stream should end after explicit close"
    );

    drop(updates_rx);
    drop(server_caller);
    client_connection
        .shutdown()
        .expect("client shutdown request");
    let _ = server_connection.shutdown();

    tokio::time::timeout(std::time::Duration::from_millis(500), client_session)
        .await
        .expect("timed out waiting for client session to exit")
        .expect("client session task failed");
}

// r[verify rpc.request.scope]
// r[verify rpc.request.scope.terminal]
// r[verify rpc.request.scope.channels]
// r[verify rpc.channel.lifecycle]
#[tokio::test]
async fn response_delivery_terminalizes_request_channels() {
    let (client_caller, _server_caller, _server_connection) =
        captured_test_lane_pair(ImmediateReplyHandler).await;

    let (updates_tx, mut updates_rx) = channel::<u32>();
    let args = SubscribeArgs {
        updates: updates_tx,
    };

    let response = client_caller
        .caller
        .call(RequestCall {
            channels: Vec::new(),
            method_id: MethodId(1),
            args: Payload::outgoing(&args),
            schemas: Default::default(),
            metadata: Default::default(),
        })
        .await
        .expect("call should receive response");
    let response = response.get();
    let ret_bytes = match &response.ret {
        Payload::Encoded(bytes) => bytes,
        _ => panic!("expected incoming payload in response"),
    };
    let result: u32 = vox_phon::from_slice(ret_bytes).expect("deserialize response");
    assert_eq!(result, 7);

    let err = match updates_rx.recv().await {
        Ok(_) => panic!("response delivery should terminate live request channel"),
        Err(err) => err,
    };
    assert!(
        matches!(
            err,
            vox_types::RxError::RequestTerminated(RequestTerminationReason::ResponseDelivered)
        ),
        "expected response-delivered request termination, got {err:?}"
    );
}

// r[verify rpc.timeout.idle-progress]
#[tokio::test]
async fn request_idle_timeout_wakes_caller_with_timeout() {
    let was_cancelled = Arc::new(AtomicBool::new(false));
    let was_cancelled_check = was_cancelled.clone();
    let (client_caller, _server_caller, _server_connection) =
        captured_test_lane_pair_with_client_timeout(
            Duration::from_millis(40),
            BlockingHandler { was_cancelled },
        )
        .await;

    let result = tokio::time::timeout(
        Duration::from_millis(500),
        client_caller.caller.call(RequestCall {
            channels: Vec::new(),
            method_id: MethodId(1),
            args: Payload::outgoing(&123_u32),
            schemas: Default::default(),
            metadata: Default::default(),
        }),
    )
    .await
    .expect("call should resolve via request idle timeout");

    assert!(
        matches!(result, Err(VoxError::TimedOut)),
        "expected TimedOut call error, got {result:?}"
    );

    for _ in 0..20 {
        if was_cancelled_check.load(Ordering::SeqCst) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(
        was_cancelled_check.load(Ordering::SeqCst),
        "request idle timeout should cancel the peer handler"
    );
}

// r[verify rpc.timeout.idle-progress]
#[tokio::test]
async fn request_associated_channel_items_reset_idle_timeout() {
    let (client_caller, _server_caller, _server_connection) =
        captured_test_lane_pair_with_client_timeout(
            Duration::from_millis(50),
            ChannelProgressReplyHandler,
        )
        .await;

    let (updates_tx, updates_rx) = channel::<u32>();
    let call_task = {
        let caller = client_caller.caller.clone();
        vox_rt::task::spawn(
            async move {
                let args = ReceiveArgs { input: updates_rx };
                caller
                    .call(RequestCall {
                        channels: Vec::new(),
                        method_id: MethodId(1),
                        args: Payload::outgoing(&args),
                        schemas: Default::default(),
                        metadata: Default::default(),
                    })
                    .await
            }
            .named("progress_reset_call"),
        )
    };

    tokio::time::sleep(Duration::from_millis(30)).await;
    updates_tx.send(1).await.expect("send first progress item");
    tokio::time::sleep(Duration::from_millis(30)).await;
    updates_tx.send(2).await.expect("send second progress item");
    tokio::time::sleep(Duration::from_millis(30)).await;
    updates_tx.send(3).await.expect("send third progress item");

    let response = tokio::time::timeout(Duration::from_millis(500), call_task)
        .await
        .expect("call should not idle-timeout while channel items flow")
        .expect("call task join")
        .expect("call should receive response");
    let response = response.get();
    let ret_bytes = match &response.ret {
        Payload::Encoded(bytes) => bytes,
        _ => panic!("expected incoming payload in response"),
    };
    let total: u32 = vox_phon::from_slice(ret_bytes).expect("deserialize response");
    assert_eq!(total, 6);
}

// r[verify rpc.cancel]
// r[verify rpc.cancel.channels]
#[tokio::test]
async fn cancel_aborts_in_flight_handler() {
    facet_testhelpers::setup();
    let (client_conduit, server_conduit) = message_conduit_pair();

    let was_cancelled = Arc::new(AtomicBool::new(false));
    let was_cancelled_check = was_cancelled.clone();

    let server_task = vox_rt::task::spawn(
        async move {
            acceptor_conduit(server_conduit, test_acceptor_handshake())
                .on_connection(BlockingHandler { was_cancelled })
                .establish_connection()
                .await
                .expect("server handshake failed")
        }
        .named("server_setup"),
    );

    // Set up client side. We need both the Caller (for sending the call) and
    // the raw sender (for sending the cancel message with the same request ID).
    let caller = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<TestLaneClient>()
        .await
        .expect("client handshake failed");
    let client_sender = caller.caller.driver().connection_sender().clone();

    let _server_caller_guard = server_task.await.expect("server setup failed");

    // Spawn the call as a task so we can concurrently send a cancel.
    let call_task = vox_rt::task::spawn(
        async move {
            let args_value: u32 = 99;
            caller
                .caller
                .call(RequestCall {
                    channels: Vec::new(),
                    method_id: MethodId(1),
                    args: Payload::outgoing(&args_value),
                    schemas: Default::default(),
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
    let cancel_req_id = vox_types::RequestId(1);
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
    let response = response.get();
    let ret_bytes = match &response.ret {
        Payload::Encoded(bytes) => bytes,
        _ => panic!("expected incoming payload in response"),
    };
    let error: Result<(), VoxError> =
        vox_phon::from_slice(ret_bytes).expect("deserialize response");
    assert!(
        matches!(error, Err(VoxError::Cancelled)),
        "expected Err(VoxError::Cancelled) in response payload"
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

// r[verify rpc.request.scope]
// r[verify rpc.request.scope.terminal]
// r[verify rpc.request.scope.channels]
// r[verify rpc.cancel.channels]
#[tokio::test]
async fn cancel_terminalizes_request_channels_as_cancelled() {
    let (client_conduit, server_conduit) = message_conduit_pair();
    let (rx_sender, rx_receiver) = tokio::sync::oneshot::channel();
    let captured_rx = Arc::new(Mutex::new(Some(rx_sender)));
    let was_cancelled = Arc::new(AtomicBool::new(false));
    let was_cancelled_check = was_cancelled.clone();

    let server_task = vox_rt::task::spawn(
        {
            let captured_rx = captured_rx.clone();
            async move {
                acceptor_conduit(server_conduit, test_acceptor_handshake())
                    .on_connection(CaptureRxBlockingHandler {
                        captured_rx,
                        was_cancelled,
                    })
                    .establish_connection()
                    .await
                    .expect("server handshake failed")
            }
        }
        .named("server_setup"),
    );

    let caller = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<TestLaneClient>()
        .await
        .expect("client handshake failed");
    let client_sender = caller.caller.driver().connection_sender().clone();

    let _server_caller_guard = server_task.await.expect("server setup failed");

    let (updates_tx, updates_rx) = channel::<u32>();
    let call_task = vox_rt::task::spawn(
        async move {
            let args = ReceiveArgs { input: updates_rx };
            caller
                .caller
                .call(RequestCall {
                    channels: Vec::new(),
                    method_id: MethodId(1),
                    args: Payload::outgoing(&args),
                    schemas: Default::default(),
                    metadata: Default::default(),
                })
                .await
        }
        .named("client_call_with_rx_channel"),
    );

    let mut server_rx = tokio::time::timeout(Duration::from_millis(500), rx_receiver)
        .await
        .expect("timed out waiting for server to decode Rx")
        .expect("server handler dropped before sending Rx");

    client_sender
        .send(ConnectionMessage::Request(RequestMessage {
            id: RequestId(1),
            body: RequestBody::Cancel(RequestCancel {
                metadata: Metadata::default(),
            }),
        }))
        .await
        .expect("send cancel");

    let recv = tokio::time::timeout(Duration::from_millis(500), server_rx.recv())
        .await
        .expect("timed out waiting for server Rx termination");
    let err = match recv {
        Ok(_) => panic!("cancel should terminate live request channel"),
        Err(err) => err,
    };
    assert!(
        matches!(
            err,
            vox_types::RxError::RequestTerminated(RequestTerminationReason::Cancelled)
        ),
        "expected cancelled request termination, got {err:?}"
    );

    drop(updates_tx);
    let result = call_task.await.expect("call task join");
    let response = result.expect("call should receive a cancellation response");
    let response = response.get();
    let ret_bytes = match &response.ret {
        Payload::Encoded(bytes) => bytes,
        _ => panic!("expected incoming payload in response"),
    };
    let error: Result<(), VoxError> =
        vox_phon::from_slice(ret_bytes).expect("deserialize response");
    assert!(
        matches!(error, Err(VoxError::Cancelled)),
        "expected Err(VoxError::Cancelled) in response payload"
    );

    for _ in 0..20 {
        if was_cancelled_check.load(Ordering::SeqCst) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(
        was_cancelled_check.load(Ordering::SeqCst),
        "handler should have been cancelled"
    );
}

/// Verify that a `MessagePlan` built from identical schemas (the schema-identical
/// degenerate of the envelope compat path) can round-trip a message.
// r[verify session.handshake.protocol-schema.session-scoped]
#[test]
fn message_plan_from_identical_schemas_round_trips() {
    // The handshake carries the peer's Message schema as phon bytes; here it is
    // our own (identical), so the compat program takes the schema-identical path.
    let our_schema = vox_phon::schema_bytes::<Message<'static>>().expect("schema bytes");
    let handshake_result = HandshakeResult {
        role: ConnectionRole::Initiator,
        our_settings: ConnectionSettings {
            parity: Parity::Odd,
            max_concurrent_requests: 64,
            initial_channel_credit: 16,
        },
        peer_settings: ConnectionSettings {
            parity: Parity::Even,
            max_concurrent_requests: 64,
            initial_channel_credit: 16,
        },
        our_schema: our_schema.clone(),
        peer_schema: our_schema,
        peer_metadata: vox_types::metadata().str("vox-service", "Noop").build(),
    };
    let plan = crate::MessagePlan::from_handshake(&handshake_result)
        .expect("should build message plan from identical schemas");

    // Build the compat decode program from the plan's writer schema.
    let writer = vox_phon::parse_schema_bytes(&plan.writer_schema).expect("parse writer schema");
    let program =
        vox_phon::build_decode_program::<Message<'static>>(&writer).expect("build decode program");

    // Encode a Ping and decode it back through the program, borrowing via SelfRef.
    let msg = Message {
        lane_id: vox_types::LaneId::ROOT,
        payload: MessagePayload::Ping(vox_types::Ping { nonce: 42 }),
    };
    let bytes = vox_phon::to_vec(&msg).expect("serialize message");
    let backing = Backing::Boxed(bytes.into());
    let decoded: SelfRef<Message<'static>> = SelfRef::try_new(backing, |b| {
        vox_phon::decode_with_program::<Message<'static>>(&program, b)
    })
    .expect("should decode with identical-schema program");
    let decoded = decoded.get();
    assert_eq!(decoded.lane_id, vox_types::LaneId::ROOT);
    match &decoded.payload {
        MessagePayload::Ping(ping) => assert_eq!(ping.nonce, 42),
        other => panic!("expected Ping, got {other:?}"),
    }
}

/// Minimal test: establish via real phon handshake, send one call, verify handler runs.
// r[verify rpc]
// r[verify session]
// r[verify session.role]
// r[verify session.connection-settings]
// r[verify session.message]
// r[verify session.message.connection-id]
// r[verify connection]
// r[verify connection.model]
// r[verify connection.root]
#[tokio::test]
async fn call_through_phon_handshake_reaches_handler() {
    let (client_link, server_link) = memory_link_pair(64);

    let (server_result, client_result) = tokio::try_join!(
        tokio::time::timeout(
            Duration::from_secs(1),
            acceptor_on(server_link)
                .on_connection(EchoHandler)
                .establish_connection(),
        ),
        tokio::time::timeout(
            Duration::from_secs(1),
            initiator_on(client_link).establish::<TestLaneClient>(),
        ),
    )
    .expect("session establishment timed out");

    let _server_caller = server_result.expect("server establish failed");
    let caller = client_result.expect("client establish failed");
    assert!(
        !caller.caller.debug_snapshot().connections[0]
            .connection_id
            .is_root()
    );

    let response = tokio::time::timeout(
        Duration::from_secs(1),
        caller.caller.call(RequestCall {
            channels: Vec::new(),
            method_id: MethodId(1),
            args: Payload::outgoing(&42_u32),
            schemas: Default::default(),
            metadata: Default::default(),
        }),
    )
    .await
    .expect("call timed out")
    .expect("call should succeed");

    let response = response.get();
    let ret_bytes = match &response.ret {
        Payload::Encoded(bytes) => bytes,
        _ => panic!("expected incoming payload in response"),
    };
    let value: u32 = vox_phon::from_slice(ret_bytes).expect("deserialize response");
    assert_eq!(value, 42);
}

#[derive(Clone, Copy)]
struct PanicHandler;

impl vox_types::Handler<crate::DriverReplySink> for PanicHandler {
    async fn handle(
        &self,
        _call: SelfRef<RequestCall<'static>>,
        _reply: crate::DriverReplySink,
        _schemas: std::sync::Arc<vox_types::SchemaRecvTracker>,
    ) {
        panic!("intentional handler panic");
    }
}

#[derive(Clone, Copy)]
struct PipeliningHandler;

impl vox_types::Handler<crate::DriverReplySink> for PipeliningHandler {
    async fn handle(
        &self,
        call: SelfRef<RequestCall<'static>>,
        reply: crate::DriverReplySink,
        _schemas: std::sync::Arc<vox_types::SchemaRecvTracker>,
    ) {
        let method_id = call.get().method_id;
        if method_id == MethodId(1) {
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        let result = if method_id == MethodId(1) {
            1_u32
        } else {
            2_u32
        };
        reply
            .send_reply(RequestResponse {
                ret: Payload::outgoing(&result),
                schemas: Default::default(),
                metadata: Default::default(),
            })
            .await;
    }
}

#[derive(Clone, Copy)]
struct ScopedErrorHandler;

impl vox_types::Handler<crate::DriverReplySink> for ScopedErrorHandler {
    async fn handle(
        &self,
        call: SelfRef<RequestCall<'static>>,
        reply: crate::DriverReplySink,
        _schemas: std::sync::Arc<vox_types::SchemaRecvTracker>,
    ) {
        let method_id = call.get().method_id;
        if method_id == MethodId(1) {
            reply
                .send_error::<std::convert::Infallible>(VoxError::InvalidPayload(
                    "scoped failure".into(),
                ))
                .await;
            return;
        }

        tokio::time::sleep(Duration::from_millis(50)).await;
        let result = 2_u32;
        reply
            .send_reply(RequestResponse {
                ret: Payload::outgoing(&result),
                schemas: Default::default(),
                metadata: Default::default(),
            })
            .await;
    }
}

// r[verify rpc.pipelining]
#[tokio::test]
async fn slow_incoming_request_does_not_block_later_request() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let server_task = vox_rt::task::spawn(
        async move {
            acceptor_conduit(server_conduit, test_acceptor_handshake())
                .on_connection(PipeliningHandler)
                .establish_connection()
                .await
                .expect("server handshake failed")
        }
        .named("server_setup"),
    );

    let caller = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<TestLaneClient>()
        .await
        .expect("client handshake failed");

    let _server_caller_guard = server_task.await.expect("server setup failed");

    let first_caller = caller.caller.clone();
    let first_call = tokio::spawn(async move {
        first_caller
            .call(RequestCall {
                channels: Vec::new(),
                method_id: MethodId(1),
                args: Payload::outgoing(&1_u32),
                schemas: Default::default(),
                metadata: Default::default(),
            })
            .await
    });

    let second_caller = caller.caller.clone();
    let second_call = tokio::spawn(async move {
        second_caller
            .call(RequestCall {
                channels: Vec::new(),
                method_id: MethodId(2),
                args: Payload::outgoing(&2_u32),
                schemas: Default::default(),
                metadata: Default::default(),
            })
            .await
    });

    let second_response = tokio::time::timeout(Duration::from_millis(100), second_call)
        .await
        .expect("second request should complete before delayed first request")
        .expect("second call task join")
        .expect("second call should succeed");
    let second_response = second_response.get();
    let second_ret_bytes = match &second_response.ret {
        Payload::Encoded(bytes) => bytes,
        _ => panic!("expected incoming payload in response"),
    };
    let second_value: u32 = vox_phon::from_slice(second_ret_bytes).expect("deserialize response");
    assert_eq!(second_value, 2);

    assert!(
        !first_call.is_finished(),
        "first delayed request should still be pending after second response"
    );

    let first_response = tokio::time::timeout(Duration::from_millis(500), first_call)
        .await
        .expect("first delayed request should eventually complete")
        .expect("first call task join")
        .expect("first call should succeed");
    let first_response = first_response.get();
    let first_ret_bytes = match &first_response.ret {
        Payload::Encoded(bytes) => bytes,
        _ => panic!("expected incoming payload in response"),
    };
    let first_value: u32 = vox_phon::from_slice(first_ret_bytes).expect("deserialize response");
    assert_eq!(first_value, 1);
}

// r[verify rpc.error.scope]
#[tokio::test]
async fn call_error_does_not_close_connection_or_cancel_other_requests() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let server_task = vox_rt::task::spawn(
        async move {
            acceptor_conduit(server_conduit, test_acceptor_handshake())
                .on_connection(ScopedErrorHandler)
                .establish_connection()
                .await
                .expect("server handshake failed")
        }
        .named("server_setup"),
    );

    let caller = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<TestLaneClient>()
        .await
        .expect("client handshake failed");

    let _server_caller_guard = server_task.await.expect("server setup failed");

    let error_caller = caller.caller.clone();
    let error_call = tokio::spawn(async move {
        error_caller
            .call(RequestCall {
                channels: Vec::new(),
                method_id: MethodId(1),
                args: Payload::outgoing(&1_u32),
                schemas: Default::default(),
                metadata: Default::default(),
            })
            .await
    });

    let ok_caller = caller.caller.clone();
    let ok_call = tokio::spawn(async move {
        ok_caller
            .call(RequestCall {
                channels: Vec::new(),
                method_id: MethodId(2),
                args: Payload::outgoing(&2_u32),
                schemas: Default::default(),
                metadata: Default::default(),
            })
            .await
    });

    let error_response = tokio::time::timeout(Duration::from_millis(250), error_call)
        .await
        .expect("error response should arrive")
        .expect("error call task join")
        .expect("error call should receive a response");
    let error_response = error_response.get();
    let error_ret_bytes = match &error_response.ret {
        Payload::Encoded(bytes) => bytes,
        _ => panic!("expected incoming payload in error response"),
    };
    let error: Result<u32, VoxError<std::convert::Infallible>> =
        vox_phon::from_slice(error_ret_bytes).expect("deserialize error response");
    assert!(
        matches!(error, Err(VoxError::InvalidPayload(ref message)) if message == "scoped failure"),
        "expected scoped InvalidPayload response, got {error:?}"
    );
    assert!(
        !ok_call.is_finished(),
        "the concurrent successful call should remain in flight after the error response"
    );
    assert!(
        caller.caller.is_connected(),
        "call-scoped errors must not close the connection"
    );

    let ok_response = tokio::time::timeout(Duration::from_millis(500), ok_call)
        .await
        .expect("concurrent ok call should finish")
        .expect("ok call task join")
        .expect("ok call should succeed");
    let ok_response = ok_response.get();
    let ok_ret_bytes = match &ok_response.ret {
        Payload::Encoded(bytes) => bytes,
        _ => panic!("expected incoming payload in ok response"),
    };
    let ok_value: u32 = vox_phon::from_slice(ok_ret_bytes).expect("deserialize ok response");
    assert_eq!(ok_value, 2);

    let followup = caller
        .caller
        .call(RequestCall {
            channels: Vec::new(),
            method_id: MethodId(2),
            args: Payload::outgoing(&3_u32),
            schemas: Default::default(),
            metadata: Default::default(),
        })
        .await
        .expect("connection should remain usable after scoped call error");
    let followup = followup.get();
    let followup_ret_bytes = match &followup.ret {
        Payload::Encoded(bytes) => bytes,
        _ => panic!("expected incoming payload in followup response"),
    };
    let followup_value: u32 =
        vox_phon::from_slice(followup_ret_bytes).expect("deserialize followup response");
    assert_eq!(followup_value, 2);
}

#[tokio::test]
async fn handler_panic_returns_error_response_instead_of_hanging() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let server_task = vox_rt::task::spawn(
        async move {
            acceptor_conduit(server_conduit, test_acceptor_handshake())
                .on_connection(PanicHandler)
                .establish_connection()
                .await
                .expect("server handshake failed")
        }
        .named("server_setup"),
    );

    let caller = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<TestLaneClient>()
        .await
        .expect("client handshake failed");

    let _server_caller_guard = server_task.await.expect("server setup failed");

    let response = tokio::time::timeout(
        Duration::from_millis(500),
        caller.caller.call(RequestCall {
            channels: Vec::new(),
            method_id: MethodId(1),
            args: Payload::outgoing(&123_u32),
            schemas: Default::default(),
            metadata: Default::default(),
        }),
    )
    .await
    .expect("call hung after handler panic")
    .expect("driver should deliver a terminal response");

    let response = response.get();
    let ret_bytes = match &response.ret {
        Payload::Encoded(bytes) => bytes,
        _ => panic!("expected incoming payload in response"),
    };
    let error: Result<(), VoxError<std::convert::Infallible>> =
        vox_phon::from_slice(ret_bytes).expect("deserialize error response");
    assert!(
        matches!(error, Err(VoxError::Cancelled)),
        "expected Cancelled error response, got {error:?}"
    );
}

#[tokio::test]
async fn in_flight_call_returns_cancelled_when_peer_closes() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let was_cancelled = Arc::new(AtomicBool::new(false));
    let was_cancelled_check = was_cancelled.clone();

    let (session_tx, session_rx) = tokio::sync::oneshot::channel::<vox_rt::task::JoinHandle<()>>();
    let server_task = vox_rt::task::spawn(
        async move {
            acceptor_conduit(server_conduit, test_acceptor_handshake())
                .spawn_fn(move |fut| {
                    let handle = vox_rt::task::spawn(fut);
                    let _ = session_tx.send(handle);
                })
                .on_connection(BlockingHandler { was_cancelled })
                .establish_connection()
                .await
                .expect("server handshake failed")
        }
        .named("server_setup"),
    );

    let caller = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<TestLaneClient>()
        .await
        .expect("client handshake failed");

    let server_caller_guard = server_task.await.expect("server setup failed");
    let server_session_task = session_rx.await.expect("session handle sent");

    let call_task = vox_rt::task::spawn(
        async move {
            caller
                .caller
                .call(RequestCall {
                    channels: Vec::new(),
                    method_id: MethodId(1),
                    args: Payload::outgoing(&123_u32),
                    schemas: Default::default(),
                    metadata: Default::default(),
                })
                .await
        }
        .named("client_call"),
    );

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    drop(server_caller_guard);
    server_session_task.abort();

    let result = tokio::time::timeout(std::time::Duration::from_millis(500), call_task)
        .await
        .expect("timed out waiting for call to resolve after peer close")
        .expect("call task join");
    assert!(
        matches!(
            result,
            Err(VoxError::ConnectionClosed) | Err(VoxError::SessionShutdown)
        ),
        "expected ConnectionClosed or SessionShutdown after peer close, got: {result:?}"
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

// r[verify rpc.flow-control.max-concurrent-requests]
// r[verify rpc.flow-control.max-concurrent-requests.outbound]
// r[verify rpc.flow-control.max-concurrent-requests.counting]
// r[verify rpc.flow-control.max-concurrent-requests.session-failure]
// r[verify rpc.flow-control]
#[tokio::test]
async fn outbound_max_concurrent_requests_waits_for_peer_limit() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let was_cancelled = Arc::new(AtomicBool::new(false));
    let (session_tx, session_rx) = tokio::sync::oneshot::channel::<vox_rt::task::JoinHandle<()>>();
    let (accepted_tx, accepted_rx) = tokio::sync::oneshot::channel();
    let server_task = vox_rt::task::spawn(
        {
            let was_cancelled = Arc::clone(&was_cancelled);
            async move {
                let server_connection = acceptor_conduit(
                    server_conduit,
                    acceptor_handshake_with_request_limits(1, 64),
                )
                .spawn_fn(move |fut| {
                    let handle = vox_rt::task::spawn(fut);
                    let _ = session_tx.send(handle);
                })
                .on_connection(CaptureClientAcceptor::new(
                    BlockingHandler { was_cancelled },
                    accepted_tx,
                ))
                .establish_connection()
                .await
                .expect("server handshake failed");
                let server_guard = accepted_rx.await.expect("server lane accepted");
                (server_connection, server_guard)
            }
        }
        .named("server_setup"),
    );

    let client_connection = initiator_conduit(
        client_conduit,
        initiator_handshake_with_request_limits(64, 1),
    )
    .establish_connection()
    .await
    .expect("client handshake failed");
    let client_guard = client_connection
        .open_lane_with_settings::<TestLaneClient>(ConnectionSettings {
            parity: Parity::Odd,
            max_concurrent_requests: 1,
            initial_channel_credit: 16,
        })
        .await
        .expect("client lane open failed");
    let (server_connection, server_guard) = server_task.await.expect("server setup failed");
    let server_session_task = session_rx.await.expect("session handle sent");

    let first_caller = client_guard.caller.clone();
    let first_call = tokio::spawn(async move {
        first_caller
            .call(RequestCall {
                channels: Vec::new(),
                method_id: MethodId(1),
                args: Payload::outgoing(&1_u32),
                schemas: Default::default(),
                metadata: Default::default(),
            })
            .await
    });

    wait_for_outstanding_requests(&server_guard.caller, 1).await;

    let second_caller = client_guard.caller.clone();
    let second_call = tokio::spawn(async move {
        second_caller
            .call(RequestCall {
                channels: Vec::new(),
                method_id: MethodId(1),
                args: Payload::outgoing(&2_u32),
                schemas: Default::default(),
                metadata: Default::default(),
            })
            .await
    });

    tokio::time::sleep(Duration::from_millis(50)).await;
    wait_for_outstanding_requests(&server_guard.caller, 1).await;
    assert!(
        !second_call.is_finished(),
        "second call should wait for request capacity instead of being sent"
    );

    drop(server_guard);
    drop(server_connection);
    server_session_task.abort();

    let first_result = tokio::time::timeout(Duration::from_millis(500), first_call)
        .await
        .expect("first call should resolve after server closes")
        .expect("first call task join");
    assert!(
        matches!(
            first_result,
            Err(VoxError::ConnectionClosed) | Err(VoxError::SessionShutdown)
        ),
        "expected first call to fail with connection/session closure, got {first_result:?}"
    );

    let second_result = tokio::time::timeout(Duration::from_millis(500), second_call)
        .await
        .expect("second call should resolve after request limiter closes")
        .expect("second call task join");
    assert!(
        matches!(
            second_result,
            Err(VoxError::ConnectionClosed) | Err(VoxError::SessionShutdown)
        ),
        "expected queued second call to fail with connection/session closure, got {second_result:?}"
    );
}

// r[verify rpc.flow-control.max-concurrent-requests]
// r[verify rpc.flow-control.max-concurrent-requests.inbound]
// r[verify rpc.flow-control]
#[tokio::test]
async fn inbound_max_concurrent_requests_violation_closes_connection() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let was_cancelled = Arc::new(AtomicBool::new(false));
    let (client_guard, server_guard, _server_connection) = captured_test_lane_pair_with_settings(
        client_conduit,
        server_conduit,
        initiator_handshake_with_request_limits(64, 1),
        acceptor_handshake_with_request_limits(1, 64),
        ConnectionSettings {
            parity: Parity::Odd,
            max_concurrent_requests: 1,
            initial_channel_credit: 16,
        },
        BlockingHandler { was_cancelled },
    )
    .await;
    let client_sender = client_guard.caller.driver().connection_sender().clone();

    let first_arg = 1_u32;
    client_sender
        .send(ConnectionMessage::Request(RequestMessage {
            id: RequestId(1),
            body: RequestBody::Call(RequestCall {
                channels: Vec::new(),
                method_id: MethodId(1),
                args: Payload::outgoing(&first_arg),
                schemas: Default::default(),
                metadata: Default::default(),
            }),
        }))
        .await
        .expect("send first raw call");
    wait_for_outstanding_requests(&server_guard.caller, 1).await;

    let second_arg = 2_u32;
    client_sender
        .send(ConnectionMessage::Request(RequestMessage {
            id: RequestId(3),
            body: RequestBody::Call(RequestCall {
                channels: Vec::new(),
                method_id: MethodId(1),
                args: Payload::outgoing(&second_arg),
                schemas: Default::default(),
                metadata: Default::default(),
            }),
        }))
        .await
        .expect("send second raw call");

    tokio::time::timeout(Duration::from_millis(500), server_guard.caller.closed())
        .await
        .expect("server connection should close after inbound limit violation");
    let server_snapshot = server_guard.caller.debug_snapshot();
    assert_eq!(
        server_snapshot.connections[0].close_reason,
        Some(vox_types::ConnectionCloseReason::Protocol)
    );

    drop(server_guard);
}

// r[verify rpc.request.id-allocation]
// r[verify session.protocol-error]
#[tokio::test]
async fn wrong_parity_request_id_closes_with_protocol_error() {
    let (client_guard, server_guard, _server_connection) =
        captured_test_lane_pair(EchoHandler).await;
    let client_sender = client_guard.caller.driver().connection_sender().clone();

    send_raw_root_call(&client_sender, RequestId(2), 1).await;

    expect_protocol_close(&server_guard.caller, "server").await;
    expect_protocol_close(&client_guard.caller, "client").await;

    drop(server_guard);
    drop(client_guard);
}

// r[verify rpc.request.id-allocation]
// r[verify session.protocol-error]
#[tokio::test]
async fn duplicate_inflight_request_id_closes_with_protocol_error() {
    let was_cancelled = Arc::new(AtomicBool::new(false));
    let (client_guard, server_guard, _server_connection) =
        captured_test_lane_pair(BlockingHandler { was_cancelled }).await;
    let client_sender = client_guard.caller.driver().connection_sender().clone();

    send_raw_root_call(&client_sender, RequestId(1), 1).await;
    wait_for_outstanding_requests(&server_guard.caller, 1).await;
    send_raw_root_call(&client_sender, RequestId(1), 2).await;

    expect_protocol_close(&server_guard.caller, "server").await;
    expect_protocol_close(&client_guard.caller, "client").await;

    drop(server_guard);
    drop(client_guard);
}

// r[verify session.keepalive]
#[tokio::test]
async fn keepalive_timeout_returns_cancelled_when_pongs_are_missing() {
    let (client_link, server_link) = memory_link_pair(64);
    let client_conduit = DropPongConduit::new(BareConduit::new(client_link));
    let server_conduit = BareConduit::new(server_link);

    let server_task = vox_rt::task::spawn(
        async move {
            acceptor_conduit(server_conduit, test_acceptor_handshake())
                .on_connection(BlockingHandler {
                    was_cancelled: Arc::new(AtomicBool::new(false)),
                })
                .establish_connection()
                .await
                .expect("server handshake failed")
        }
        .named("server_setup"),
    );

    let caller = initiator_conduit(client_conduit, test_initiator_handshake())
        .keepalive(ConnectionKeepaliveConfig {
            ping_interval: std::time::Duration::from_millis(20),
            pong_timeout: std::time::Duration::from_millis(50),
        })
        .establish::<TestLaneClient>()
        .await
        .expect("client handshake failed");

    let _server_caller_guard = server_task.await.expect("server setup failed");

    let call_task = vox_rt::task::spawn(
        async move {
            caller
                .caller
                .call(RequestCall {
                    channels: Vec::new(),
                    method_id: MethodId(1),
                    args: Payload::outgoing(&123_u32),
                    schemas: Default::default(),
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
        matches!(
            result,
            Err(VoxError::ConnectionClosed) | Err(VoxError::SessionShutdown)
        ),
        "expected ConnectionClosed or SessionShutdown after keepalive timeout, got: {result:?}"
    );
}

// r[verify rpc.caller.liveness.root-internal-close]
// r[verify rpc.caller.liveness.root-teardown-condition]
#[tokio::test]
async fn dropping_root_caller_does_not_shut_down_session() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let (client_session_tx, client_session_rx) =
        tokio::sync::oneshot::channel::<vox_rt::task::JoinHandle<()>>();
    let (server_session_tx, server_session_rx) =
        tokio::sync::oneshot::channel::<vox_rt::task::JoinHandle<()>>();

    let server_task = vox_rt::task::spawn(
        async move {
            acceptor_conduit(server_conduit, test_acceptor_handshake())
                .spawn_fn(move |fut| {
                    let handle = vox_rt::task::spawn(fut.named("server_session"));
                    let _ = server_session_tx.send(handle);
                })
                .on_connection(EchoHandler)
                .establish_connection()
                .await
                .expect("server handshake failed")
        }
        .named("server_setup"),
    );

    let caller = initiator_conduit(client_conduit, test_initiator_handshake())
        .spawn_fn(move |fut| {
            let handle = vox_rt::task::spawn(fut.named("client_session"));
            let _ = client_session_tx.send(handle);
        })
        .establish::<TestLaneClient>()
        .await
        .expect("client handshake failed");

    let server_connection = server_task.await.expect("server setup failed");

    let client_session = client_session_rx.await.expect("client session handle sent");
    let server_session = server_session_rx.await.expect("server session handle sent");
    let client_connection = caller.connection.clone().expect("client connection handle");

    drop(caller);

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(
        !client_session.is_finished(),
        "dropping the root caller must not shut down the client connection"
    );
    assert!(
        !server_session.is_finished(),
        "dropping the root caller must not shut down the server connection"
    );

    client_connection
        .shutdown()
        .expect("client shutdown request");
    let _ = server_connection.shutdown();

    tokio::time::timeout(std::time::Duration::from_millis(500), client_session)
        .await
        .expect("timed out waiting for client session to exit")
        .expect("client session task failed");
    tokio::time::timeout(std::time::Duration::from_millis(500), server_session)
        .await
        .expect("timed out waiting for server session to exit")
        .expect("server session task failed");
}

// ---------------------------------------------------------------------------
// Virtual connection tests
// ---------------------------------------------------------------------------

/// Regression test: schema recv tracker must be per-connection.
/// If it were per-session, the second call (on the virtual connection) would
/// fail because the response schemas overlap with the root connection's.
// r[verify schema.type-id.per-connection]
#[tokio::test]
async fn schema_tracker_is_per_connection_not_per_session() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let server_task = vox_rt::task::spawn(
        async move {
            acceptor_conduit(server_conduit, test_acceptor_handshake())
                .on_connection(EchoAcceptor)
                .on_connection(EchoHandler)
                .establish_connection()
                .await
                .expect("server handshake failed")
        }
        .named("server_setup"),
    );

    let root_caller = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<TestLaneClient>()
        .await
        .expect("client handshake failed");
    let connection_handle = root_caller.connection.clone().unwrap();

    let _server_caller_guard = server_task.await.expect("server setup failed");

    // Call on the root connection — this sends and receives schemas.
    let args_value: u32 = 100;
    let response = root_caller
        .caller
        .call(RequestCall {
            channels: Vec::new(),
            method_id: MethodId(1),
            args: Payload::outgoing(&args_value),
            schemas: Default::default(),
            metadata: Default::default(),
        })
        .await
        .expect("root call should succeed");
    let response = response.get();
    let ret_bytes = match &response.ret {
        Payload::Encoded(bytes) => bytes,
        _ => panic!("expected incoming payload"),
    };
    let result: u32 = vox_phon::from_slice(ret_bytes).expect("deserialize root response");
    assert_eq!(result, 100);

    // Open a virtual connection and call on it.
    // The same schema types (u32, Result, etc.) appear on both connections.
    // If the recv tracker were shared, recording the virtual connection's
    // schemas would hit a duplicate and panic.
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
        .expect("open virtual connection");

    let mut vconn_driver = Driver::new(vconn_handle, ());
    let vconn_caller = crate::Caller::new(vconn_driver.caller());
    vox_rt::task::spawn(async move { vconn_driver.run().await }.named("vconn_driver"));

    let args_value: u32 = 200;
    let response = vconn_caller
        .call(RequestCall {
            channels: Vec::new(),
            method_id: MethodId(1),
            args: Payload::outgoing(&args_value),
            schemas: Default::default(),
            metadata: Default::default(),
        })
        .await
        .expect("virtual connection call should succeed");
    let response = response.get();
    let ret_bytes = match &response.ret {
        Payload::Encoded(bytes) => bytes,
        _ => panic!("expected incoming payload"),
    };
    let result: u32 = vox_phon::from_slice(ret_bytes).expect("deserialize vconn response");
    assert_eq!(result, 200);
}

#[tokio::test]
async fn initiator_builder_customization_controls_allocated_connection_parity() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let server_task = vox_rt::task::spawn(
        async move {
            acceptor_conduit(
                server_conduit,
                HandshakeResult {
                    role: ConnectionRole::Acceptor,
                    our_settings: ConnectionSettings {
                        parity: Parity::Odd,
                        max_concurrent_requests: 32,
                        initial_channel_credit: 16,
                    },
                    peer_settings: ConnectionSettings {
                        parity: Parity::Even,
                        max_concurrent_requests: 64,
                        initial_channel_credit: 16,
                    },
                    our_schema: vec![],
                    peer_schema: vec![],
                    peer_metadata: vox_types::metadata().str("vox-service", "Noop").build(),
                },
            )
            .on_connection(EchoAcceptor)
            .establish_connection()
            .await
            .expect("server handshake failed")
        }
        .named("server_setup"),
    );

    let _client_caller_guard = initiator_conduit(
        client_conduit,
        HandshakeResult {
            role: ConnectionRole::Initiator,
            our_settings: ConnectionSettings {
                parity: Parity::Even,
                max_concurrent_requests: 64,
                initial_channel_credit: 16,
            },
            peer_settings: ConnectionSettings {
                parity: Parity::Odd,
                max_concurrent_requests: 32,
                initial_channel_credit: 16,
            },
            our_schema: vec![],
            peer_schema: vec![],
            peer_metadata: vox_types::metadata().str("vox-service", "Noop").build(),
        },
    )
    .establish::<TestLaneClient>()
    .await
    .expect("client handshake failed");
    let connection_handle = _client_caller_guard.connection.clone().unwrap();

    let _server_caller_guard = server_task.await.expect("server setup failed");

    let vconn_handle = connection_handle
        .open_lane_handle(
            ConnectionSettings {
                parity: Parity::Even,
                max_concurrent_requests: 64,
                initial_channel_credit: 16,
            },
            vox_types::metadata().str("vox-service", "Echo").build(),
        )
        .await
        .expect("open virtual connection");

    let conn_id = vconn_handle.connection_id();
    assert!(
        conn_id.has_parity(Parity::Even),
        "initiator parity should drive allocated connection ids"
    );
}

// r[verify session.symmetry]
#[tokio::test]
async fn acceptor_builder_customization_supports_opening_connections() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let initiator_task = vox_rt::task::spawn(
        async move {
            initiator_conduit(
                client_conduit,
                HandshakeResult {
                    role: ConnectionRole::Initiator,
                    our_settings: ConnectionSettings {
                        parity: Parity::Even,
                        max_concurrent_requests: 64,
                        initial_channel_credit: 16,
                    },
                    peer_settings: ConnectionSettings {
                        parity: Parity::Odd,
                        max_concurrent_requests: 32,
                        initial_channel_credit: 16,
                    },
                    our_schema: vec![],
                    peer_schema: vec![],
                    peer_metadata: vox_types::metadata().str("vox-service", "Noop").build(),
                },
            )
            .on_connection(EchoAcceptor)
            .establish_connection()
            .await
            .expect("initiator handshake failed")
        }
        .named("initiator_setup"),
    );

    let acceptor_session_handle = acceptor_conduit(
        server_conduit,
        HandshakeResult {
            role: ConnectionRole::Acceptor,
            our_settings: ConnectionSettings {
                parity: Parity::Odd,
                max_concurrent_requests: 32,
                initial_channel_credit: 16,
            },
            peer_settings: ConnectionSettings {
                parity: Parity::Even,
                max_concurrent_requests: 64,
                initial_channel_credit: 16,
            },
            our_schema: vec![],
            peer_schema: vec![],
            peer_metadata: vox_types::metadata().str("vox-service", "Noop").build(),
        },
    )
    .establish_connection()
    .await
    .expect("acceptor handshake failed");

    let _initiator_session_handle = initiator_task.await.expect("initiator setup failed");

    let vconn_handle = acceptor_session_handle
        .open_lane_handle(
            ConnectionSettings {
                parity: Parity::Odd,
                max_concurrent_requests: 64,
                initial_channel_credit: 16,
            },
            vox_types::metadata().str("vox-service", "Echo").build(),
        )
        .await
        .expect("acceptor opens virtual connection");

    let conn_id = vconn_handle.connection_id();
    assert!(
        conn_id.has_parity(Parity::Odd),
        "acceptor should allocate odd ids when peer initiator parity is even"
    );
}

// r[verify connection.parity]
// r[verify session.parity]
// r[verify session.connection-settings.open]
#[tokio::test]
async fn virtual_connection_request_ids_use_connection_parity() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let was_cancelled = Arc::new(AtomicBool::new(false));
    let server_task = vox_rt::task::spawn(
        {
            let was_cancelled = Arc::clone(&was_cancelled);
            async move {
                acceptor_conduit(server_conduit, test_acceptor_handshake())
                    .on_connection(crate::connection::lane_acceptor_fn(
                        move |_request: &LaneRequest, connection: PendingLane| {
                            connection.handle_with(BlockingHandler {
                                was_cancelled: Arc::clone(&was_cancelled),
                            });
                            Ok(())
                        },
                    ))
                    .establish_connection()
                    .await
                    .expect("server handshake failed")
            }
        }
        .named("server_setup"),
    );

    let root_caller = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<TestLaneClient>()
        .await
        .expect("client handshake failed");
    let connection_handle = root_caller.connection.clone().unwrap();

    let _server_caller_guard = server_task.await.expect("server setup failed");

    let vconn_handle = connection_handle
        .open_lane_handle(
            ConnectionSettings {
                parity: Parity::Even,
                max_concurrent_requests: 64,
                initial_channel_credit: 16,
            },
            vox_types::metadata().str("vox-service", "Echo").build(),
        )
        .await
        .expect("open virtual connection");
    let vconn_id = vconn_handle.connection_id();
    assert!(
        vconn_id.has_parity(Parity::Odd),
        "session parity should allocate the virtual connection id"
    );

    let mut vconn_driver = Driver::new(vconn_handle, ());
    let vconn_caller = crate::Caller::new(vconn_driver.caller());
    vox_rt::task::spawn(async move { vconn_driver.run().await }.named("vconn_client_driver"));

    let call_caller = vconn_caller.clone();
    let call_task = tokio::spawn(async move {
        call_caller
            .call(RequestCall {
                channels: Vec::new(),
                method_id: MethodId(1),
                args: Payload::outgoing(&7_u32),
                schemas: Default::default(),
                metadata: Default::default(),
            })
            .await
    });

    wait_for_outstanding_requests(&vconn_caller, 1).await;
    let snapshot = vconn_caller.debug_snapshot();
    assert_eq!(snapshot.connections[0].connection_id, vconn_id);
    assert_eq!(snapshot.connections[0].requests[0].request_id, RequestId(2));

    connection_handle
        .close_lane(vconn_id, Default::default())
        .await
        .expect("close virtual connection");
    let result = tokio::time::timeout(Duration::from_millis(500), call_task)
        .await
        .expect("call should finish after virtual connection closes")
        .expect("call task should join");
    assert!(
        matches!(
            result,
            Err(VoxError::ConnectionClosed) | Err(VoxError::SessionShutdown)
        ),
        "expected virtual call to fail after close, got {result:?}"
    );
}

// r[verify connection.close]
// r[verify connection.root]
// r[verify lane.control]
#[tokio::test]
async fn close_root_connection_is_rejected() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let server_task = vox_rt::task::spawn(
        async move {
            acceptor_conduit(server_conduit, test_acceptor_handshake())
                .on_connection(EchoHandler)
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

    let result = connection_handle
        .close_lane(vox_types::LaneId::ROOT, Default::default())
        .await;
    assert!(
        matches!(result, Err(ConnectionError::Protocol(ref msg)) if msg == "cannot close root connection"),
        "expected root-close protocol error, got: {result:?}"
    );
}

#[tokio::test]
async fn echo_call_across_memory_link() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    // Server and client handshakes must run concurrently — both sides exchange
    // settings before either can proceed.
    let server_task = vox_rt::task::spawn(
        async move {
            acceptor_conduit(server_conduit, test_acceptor_handshake())
                .on_connection(EchoHandler)
                .establish_connection()
                .await
                .expect("server handshake failed")
        }
        .named("server_setup"),
    );

    // Set up client side (runs concurrently with server_task above).
    let caller = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<TestLaneClient>()
        .await
        .expect("client handshake failed");

    let _server_caller_guard = server_task.await.expect("server setup failed");

    // Make a call: serialize a u32 as the args payload.
    let args_value: u32 = 42;
    let response = caller
        .caller
        .call(RequestCall {
            channels: Vec::new(),
            method_id: MethodId(1),
            args: Payload::outgoing(&args_value),
            schemas: Default::default(),
            metadata: Default::default(),
        })
        .await
        .expect("call should succeed");

    // The echo handler sends back the same bytes. Deserialize the response.
    let response = response.get();
    let ret_bytes = match &response.ret {
        Payload::Encoded(bytes) => bytes,
        _ => panic!("expected incoming payload in response"),
    };
    let result: u32 = vox_phon::from_slice(ret_bytes).expect("deserialize response");
    assert_eq!(result, 42);
}

#[tokio::test]
async fn buffers_inbound_channel_items_until_rx_is_registered() {
    let (client_caller, server_caller, _server_connection) = captured_test_lane_pair(()).await;
    let client_sender = client_caller.caller.driver().connection_sender().clone();

    let channel_id = ChannelId(99);
    let value = 123_u32;
    client_sender
        .send(crate::connection::ConnectionMessage::Channel(
            ChannelMessage {
                id: channel_id,
                body: ChannelBody::Item(ChannelItem {
                    item: Payload::outgoing(&value),
                }),
            },
        ))
        .await
        .expect("send channel item");

    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    let mut rx = server_caller
        .caller
        .driver()
        .register_rx_channel(channel_id)
        .receiver;
    let msg = tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv())
        .await
        .expect("timed out waiting for buffered channel item")
        .expect("channel receiver closed unexpectedly");

    let IncomingChannelMessage::Item(item) = msg else {
        panic!("expected buffered item");
    };
    let item = item.get();
    let bytes = match item.item {
        Payload::Encoded(bytes) => bytes,
        _ => panic!("expected incoming payload"),
    };
    let decoded: u32 = vox_phon::from_slice(bytes).expect("deserialize buffered item");
    assert_eq!(decoded, 123);
}

#[tokio::test]
async fn grant_credit_unblocks_driver_created_tx_channel() {
    let (client_caller, server_caller, _server_connection) = captured_test_lane_pair(()).await;
    let client_sender = client_caller.caller.driver().connection_sender().clone();

    let (channel_id, sink) = server_caller.caller.driver().create_tx_channel();

    // Exhaust the default 16 credits.
    for _ in 0..16 {
        let value = 0_u32;
        sink.send_payload(Payload::outgoing(&value))
            .await
            .expect("send within initial credit");
    }

    let send_task = vox_rt::task::spawn(async move {
        let value = 42_u32;
        sink.send_payload(Payload::outgoing(&value)).await
    });

    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    assert!(
        !send_task.is_finished(),
        "send should block when credit is exhausted"
    );

    client_sender
        .send(crate::connection::ConnectionMessage::Channel(
            ChannelMessage {
                id: channel_id,
                body: ChannelBody::GrantCredit(ChannelGrantCredit { additional: 1 }),
            },
        ))
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

// r[verify rpc.debug.snapshot]
#[tokio::test]
async fn debug_snapshot_reports_driver_channel_credit_state() {
    let (_client_caller, server_caller, _server_connection) = captured_test_lane_pair(()).await;
    let (channel_id, sink) = server_caller.caller.driver().create_tx_channel();
    let mut tx = Tx::<u32>::unbound();
    tx.bind(sink);
    tx.try_send(7).expect("try_send should use one credit");

    let snapshot = server_caller.caller.debug_snapshot();
    let connection = snapshot
        .connections
        .iter()
        .find(|connection| {
            connection.connection_id
                == server_caller
                    .caller
                    .driver()
                    .connection_sender()
                    .connection_id()
        })
        .expect("snapshot should include caller connection");
    let channel = connection
        .open_channels
        .iter()
        .find(|channel| channel.channel_id == channel_id)
        .expect("snapshot should include created channel");

    assert_eq!(channel.direction, ChannelDirection::Tx);
    assert_eq!(channel.connection_id, connection.connection_id);
    assert_eq!(channel.initial_credit, 16);
    assert_eq!(channel.available_send_credit, Some(15));
    assert_eq!(channel.current_permit_count, Some(15));
    assert_eq!(channel.sent, 1);
    assert!(channel.last_item_sent_at.is_some());
    assert_eq!(connection.outbound_queue_capacity, Some(256));
}

// r[verify rpc.flow-control.credit.initial]
// r[verify rpc.flow-control.credit.exhaustion]
#[tokio::test]
async fn configured_channel_capacity_controls_initial_credit() {
    let (client_conduit, server_conduit) = message_conduit_pair();
    let mut server_handshake = test_acceptor_handshake();
    server_handshake.our_settings.initial_channel_credit = 2;
    server_handshake.peer_settings.initial_channel_credit = 2;
    let mut client_handshake = test_initiator_handshake();
    client_handshake.our_settings.initial_channel_credit = 2;
    client_handshake.peer_settings.initial_channel_credit = 2;

    let (client_caller, server_caller, _server_connection) = captured_test_lane_pair_with_settings(
        client_conduit,
        server_conduit,
        client_handshake,
        server_handshake,
        ConnectionSettings {
            parity: Parity::Odd,
            max_concurrent_requests: 64,
            initial_channel_credit: 2,
        },
        (),
    )
    .await;
    let client_sender = client_caller.caller.driver().connection_sender().clone();

    let (channel_id, sink) = server_caller.caller.driver().create_tx_channel();

    for _ in 0..2 {
        let value = 0_u32;
        sink.send_payload(Payload::outgoing(&value))
            .await
            .expect("send within configured initial credit");
    }

    let send_task = vox_rt::task::spawn(async move {
        let value = 42_u32;
        sink.send_payload(Payload::outgoing(&value)).await
    });

    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    assert!(
        !send_task.is_finished(),
        "third send should block when configured credit is exhausted"
    );

    client_sender
        .send(crate::connection::ConnectionMessage::Channel(
            ChannelMessage {
                id: channel_id,
                body: ChannelBody::GrantCredit(ChannelGrantCredit { additional: 1 }),
            },
        ))
        .await
        .expect("send grant credit");

    let send_result = tokio::time::timeout(std::time::Duration::from_millis(200), send_task)
        .await
        .expect("timed out waiting for send to unblock")
        .expect("send task join");
    assert!(
        send_result.is_ok(),
        "send should succeed after configured credit is replenished"
    );
}

// r[verify rpc.channel.reset]
#[tokio::test]
async fn dropping_bound_rx_makes_peer_tx_send_fail() {
    let (client_caller, server_caller, _server_connection) = captured_test_lane_pair(()).await;
    let (channel_id, sink) = server_caller.caller.driver().create_tx_channel();

    let mut server_tx = Tx::<u32>::unbound();
    let sink: Arc<dyn ChannelSink> = sink;
    server_tx.bind(sink);

    let mut client_rx: Rx<u32> =
        vox_types::channel::with_channel_binder(client_caller.caller.driver(), || {
            channel_id.try_into().expect("bind client rx")
        });

    server_tx
        .send(1_u32)
        .await
        .expect("initial send should succeed");
    let received = client_rx
        .recv()
        .await
        .expect("recv should succeed")
        .expect("expected initial item");
    assert_eq!(*received.get(), 1);

    drop(client_rx);

    let mut observed_error = false;
    for i in 0_u32..100 {
        match tokio::time::timeout(Duration::from_millis(500), server_tx.send(i)).await {
            Ok(Ok(())) => {}
            Ok(Err(_)) => {
                observed_error = true;
                break;
            }
            Err(_) => panic!("send timed out instead of observing dropped Rx"),
        }
    }

    assert!(
        observed_error,
        "server Tx should fail after peer Rx is dropped"
    );
}

#[tokio::test]
async fn buffered_close_before_registration_keeps_channel_terminal() {
    let (client_caller, server_caller, _server_connection) = captured_test_lane_pair(()).await;
    let client_sender = client_caller.caller.driver().connection_sender().clone();

    let channel_id = ChannelId(77);

    client_sender
        .send(crate::connection::ConnectionMessage::Channel(
            ChannelMessage {
                id: channel_id,
                body: ChannelBody::Close(ChannelClose {
                    metadata: Metadata::default(),
                }),
            },
        ))
        .await
        .expect("send buffered close");

    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    let mut rx = server_caller
        .caller
        .driver()
        .register_rx_channel(channel_id)
        .receiver;
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
        .send(crate::connection::ConnectionMessage::Channel(
            ChannelMessage {
                id: channel_id,
                body: ChannelBody::Item(ChannelItem {
                    item: Payload::outgoing(&value),
                }),
            },
        ))
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
    let (caller, server_caller, _server_connection) = captured_test_lane_pair(EchoHandler).await;
    let server_sender = server_caller.caller.driver().connection_sender().clone();

    server_sender
        .send(crate::connection::ConnectionMessage::Request(
            RequestMessage {
                id: vox_types::RequestId(9999),
                body: RequestBody::Response(RequestResponse {
                    ret: Payload::outgoing(&123_u32),
                    schemas: Default::default(),
                    metadata: Default::default(),
                }),
            },
        ))
        .await
        .expect("send unsolicited response");

    let args_value: u32 = 42;
    let response = caller
        .caller
        .call(RequestCall {
            channels: Vec::new(),
            method_id: MethodId(1),
            args: Payload::outgoing(&args_value),
            schemas: Default::default(),
            metadata: Default::default(),
        })
        .await
        .expect("call should still succeed after unsolicited response");

    let response = response.get();
    let ret_bytes = match &response.ret {
        Payload::Encoded(bytes) => bytes,
        _ => panic!("expected incoming payload"),
    };
    let result: u32 = vox_phon::from_slice(ret_bytes).expect("deserialize response");
    assert_eq!(result, 42);
}

#[tokio::test]
async fn proxy_connections_forwards_calls_without_service_specific_proxy_code() {
    let (host_a_conduit, guest_a_conduit) = message_conduit_pair();
    let (host_b_conduit, guest_b_conduit) = message_conduit_pair();

    struct ProxyHostAcceptor {
        upstream_session: ConnectionHandle,
    }
    impl LaneAcceptor for ProxyHostAcceptor {
        fn accept(
            &self,
            request: &LaneRequest,
            connection: PendingLane,
        ) -> Result<(), LaneRejection> {
            if request.service() == "Noop" {
                connection.handle_with(());
                return Ok(());
            }
            // Virtual connections — proxy to upstream.
            let upstream_session = self.upstream_session.clone();
            let incoming = connection.into_handle();
            vox_rt::task::spawn(
                async move {
                    let upstream = upstream_session
                        .open_lane_handle(
                            ConnectionSettings {
                                parity: Parity::Odd,
                                max_concurrent_requests: 64,
                                initial_channel_credit: 16,
                            },
                            vox_types::metadata().str("vox-service", "Echo").build(),
                        )
                        .await
                        .expect("host->guest-b open_lane_handle");
                    let _ = proxy_lanes(incoming, upstream).await;
                }
                .named("host_proxy_vconn"),
            );
            Ok(())
        }
    }

    let guest_b_task = vox_rt::task::spawn(
        async move {
            let guard = acceptor_conduit(guest_b_conduit, test_acceptor_handshake())
                .on_connection(EchoAcceptor)
                .establish::<TestLaneClient>()
                .await
                .expect("guest-b establish");
            let _guard = guard;
            std::future::pending::<()>().await;
        }
        .named("guest_b_root"),
    );

    let _host_to_b_guard = initiator_conduit(host_b_conduit, test_initiator_handshake())
        .establish::<TestLaneClient>()
        .await
        .expect("host<->guest-b establish");
    let host_to_b_session = _host_to_b_guard.connection.clone().unwrap();

    let host_for_a_task = vox_rt::task::spawn(
        async move {
            let guard = acceptor_conduit(host_a_conduit, test_acceptor_handshake())
                .on_connection(ProxyHostAcceptor {
                    upstream_session: host_to_b_session,
                })
                .establish::<TestLaneClient>()
                .await
                .expect("host<->guest-a establish");
            let _guard = guard;
            std::future::pending::<()>().await;
        }
        .named("host_for_guest_a_root"),
    );

    let _guest_a_root_guard = initiator_conduit(guest_a_conduit, test_initiator_handshake())
        .establish::<TestLaneClient>()
        .await
        .expect("guest-a<->host establish");
    let guest_a_session = _guest_a_root_guard.connection.clone().unwrap();

    let proxy_conn = guest_a_session
        .open_lane_handle(
            ConnectionSettings {
                parity: Parity::Odd,
                max_concurrent_requests: 64,
                initial_channel_credit: 16,
            },
            vox_types::metadata().str("vox-service", "Echo").build(),
        )
        .await
        .expect("guest-a open proxy connection");
    let proxy_conn_id = proxy_conn.connection_id();

    let mut proxy_driver = Driver::new(proxy_conn, ());
    let proxy_caller = crate::Caller::new(proxy_driver.caller());
    let proxy_driver_task =
        vox_rt::task::spawn(async move { proxy_driver.run().await }.named("guest_a_proxy_driver"));

    let args_value: u32 = 777;
    let response = proxy_caller
        .call(RequestCall {
            channels: Vec::new(),
            method_id: MethodId(1),
            args: Payload::outgoing(&args_value),
            schemas: Default::default(),
            metadata: Default::default(),
        })
        .await
        .expect("proxied call should succeed");
    let response = response.get();
    let ret_bytes = match &response.ret {
        Payload::Encoded(bytes) => bytes,
        _ => panic!("expected incoming payload"),
    };
    let result: u32 = vox_phon::from_slice(ret_bytes).expect("deserialize proxied response");
    assert_eq!(result, args_value);

    guest_a_session
        .close_lane(proxy_conn_id, Default::default())
        .await
        .expect("close proxy connection");

    proxy_driver_task.abort();
    guest_b_task.abort();
    host_for_a_task.abort();
}
