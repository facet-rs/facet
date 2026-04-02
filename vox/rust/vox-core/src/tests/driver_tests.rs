use facet::Facet;
use std::collections::VecDeque;
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
    ConnectionAcceptor, ConnectionMessage, ConnectionRequest, PendingConnection,
    SessionAcceptOutcome, SessionError, SessionHandle, SessionKeepaliveConfig, SessionRegistry,
    acceptor_conduit, acceptor_on, initiator_conduit, initiator_on, proxy_connections,
};
use crate::{
    Attachment, BareConduit, Driver, DriverCaller, DriverReplySink, InMemoryOperationStore,
    LinkSource, NoopClient, OperationStore, TransportMode, initiate_transport, memory_link_pair,
};

// r[verify rpc.caller.liveness.refcounted]
#[tokio::test]
async fn dropping_one_root_caller_clone_keeps_session_alive_until_last_drop() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let (client_session_tx, client_session_rx) =
        tokio::sync::oneshot::channel::<moire::task::JoinHandle<()>>();
    let (server_session_tx, server_session_rx) =
        tokio::sync::oneshot::channel::<moire::task::JoinHandle<()>>();

    let server_task = moire::task::spawn(
        async move {
            let server_caller = acceptor_conduit(server_conduit, test_acceptor_handshake())
                .spawn_fn(move |fut| {
                    let handle = moire::task::spawn(fut.named("server_session"));
                    let _ = server_session_tx.send(handle);
                })
                .on_connection(EchoHandler)
                .establish::<NoopClient>()
                .await
                .expect("server handshake failed");
            server_caller
        }
        .named("server_setup"),
    );

    let caller = initiator_conduit(client_conduit, test_initiator_handshake())
        .spawn_fn(move |fut| {
            let handle = moire::task::spawn(fut.named("client_session"));
            let _ = client_session_tx.send(handle);
        })
        .establish::<NoopClient>()
        .await
        .expect("client handshake failed");

    let server_caller_guard = server_task.await.expect("server setup failed");
    let client_session = client_session_rx.await.expect("client session handle sent");
    let server_session = server_session_rx.await.expect("server session handle sent");

    let caller_clone = caller.clone();
    drop(caller_clone);

    let response = caller
        .caller
        .call(RequestCall {
            method_id: MethodId(1),
            args: Payload::outgoing(&42_u32),
            schemas: Default::default(),
            metadata: Default::default(),
        })
        .await
        .expect("call should still succeed while one root caller remains");
    let ret_bytes = match &response.ret {
        Payload::PostcardBytes(bytes) => *bytes,
        _ => panic!("expected incoming payload in response"),
    };
    let echoed: u32 = vox_postcard::from_slice(ret_bytes).expect("deserialize response");
    assert_eq!(echoed, 42);

    drop(caller);
    drop(server_caller_guard);

    tokio::time::timeout(std::time::Duration::from_millis(500), client_session)
        .await
        .expect("timed out waiting for client session to exit")
        .expect("client session task failed");
    tokio::time::timeout(std::time::Duration::from_millis(500), server_session)
        .await
        .expect("timed out waiting for server session to exit")
        .expect("server session task failed");
}

// r[verify rpc.channel.binding.caller-args.tx]
#[tokio::test]
async fn dropping_root_caller_keeps_session_alive_while_bound_stream_rx_exists() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let (client_session_tx, client_session_rx) =
        tokio::sync::oneshot::channel::<moire::task::JoinHandle<()>>();

    let server_task = moire::task::spawn(
        async move {
            let server_caller = acceptor_conduit(server_conduit, test_acceptor_handshake())
                .establish::<NoopClient>()
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
        .establish::<NoopClient>()
        .await
        .expect("client handshake failed");

    let server_caller = server_task.await.expect("server setup failed");
    let client_session = client_session_rx.await.expect("client session handle sent");

    let (updates_tx, mut updates_rx) = channel::<u32>();
    let args = SubscribeArgs {
        updates: updates_tx,
    };
    // Serializing the args binds the Tx's paired Rx via the thread-local binder.
    let _bytes = vox_types::channel::with_channel_binder(root_caller.caller.driver(), || {
        vox_postcard::to_vec(&args).expect("serialize args")
    });
    // The first allocated channel ID is 1 (odd parity).
    let channel_id = ChannelId(1);
    drop(args);
    drop(root_caller);

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(
        !client_session.is_finished(),
        "session should remain alive while a bound stream handle still exists"
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

    tokio::time::timeout(std::time::Duration::from_millis(500), client_session)
        .await
        .expect("timed out waiting for client session to exit")
        .expect("client session task failed");
}

// r[verify rpc.cancel]
// r[verify rpc.cancel.channels]
#[tokio::test]
async fn cancel_aborts_in_flight_handler() {
    facet_testhelpers::setup();
    let (client_conduit, server_conduit) = message_conduit_pair();

    let was_cancelled = Arc::new(AtomicBool::new(false));
    let was_cancelled_check = was_cancelled.clone();

    let server_task = moire::task::spawn(
        async move {
            let server_caller = acceptor_conduit(server_conduit, test_acceptor_handshake())
                .establish::<NoopClient>(BlockingHandler {
                    was_cancelled,
                    retry: RetryPolicy::VOLATILE,
                })
                .await
                .expect("server handshake failed");
            server_caller
        }
        .named("server_setup"),
    );

    // Set up client side. We need both the Caller (for sending the call) and
    // the raw sender (for sending the cancel message with the same request ID).
    let caller = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<NoopClient>()
        .await
        .expect("client handshake failed");
    let client_sender = caller.caller.driver().connection_sender().clone();

    let _server_caller_guard = server_task.await.expect("server setup failed");

    // Spawn the call as a task so we can concurrently send a cancel.
    let call_task = moire::task::spawn(
        async move {
            let args_value: u32 = 99;
            caller
                .caller
                .call(RequestCall {
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
    let ret_bytes = match &response.ret {
        Payload::PostcardBytes(bytes) => *bytes,
        _ => panic!("expected incoming payload in response"),
    };
    let error: Result<(), VoxError> =
        vox_postcard::from_slice(ret_bytes).expect("deserialize response");
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

#[tokio::test]
async fn cancel_does_not_abort_persist_handler() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let was_cancelled = Arc::new(AtomicBool::new(false));
    let was_cancelled_check = Arc::clone(&was_cancelled);
    let release = Arc::new(tokio::sync::Notify::new());
    let release_server = Arc::clone(&release);

    let server_task = moire::task::spawn(
        async move {
            let server_caller = acceptor_conduit(server_conduit, test_acceptor_handshake())
                .establish::<NoopClient>(PersistentReplyingHandler {
                    was_cancelled,
                    release: release_server,
                })
                .await
                .expect("server handshake failed");
            server_caller
        }
        .named("server_setup"),
    );

    let caller = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<NoopClient>()
        .await
        .expect("client handshake failed");
    let client_sender = caller.caller.driver().connection_sender().clone();

    let _server_caller_guard = server_task.await.expect("server setup failed");

    let call_task = moire::task::spawn(
        async move {
            caller
                .caller
                .call(RequestCall {
                    method_id: MethodId(1),
                    args: Payload::outgoing(&99_u32),
                    schemas: Default::default(),
                    metadata: Default::default(),
                })
                .await
        }
        .named("client_call_persist"),
    );

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    client_sender
        .send(ConnectionMessage::Request(RequestMessage {
            id: vox_types::RequestId(1),
            body: RequestBody::Cancel(RequestCancel {
                metadata: Metadata::default(),
            }),
        }))
        .await
        .expect("send cancel");

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(
        !was_cancelled_check.load(Ordering::SeqCst),
        "persist handler should not be cancelled by explicit cancel"
    );

    release.notify_waiters();

    let response = tokio::time::timeout(std::time::Duration::from_millis(500), call_task)
        .await
        .expect("timed out waiting for persist handler to finish")
        .expect("call task join")
        .expect("persist call should still receive a response");
    let ret_bytes = match &response.ret {
        Payload::PostcardBytes(bytes) => *bytes,
        _ => panic!("expected incoming payload in response"),
    };
    let value: u32 = vox_postcard::from_slice(ret_bytes).expect("deserialize response");
    assert_eq!(value, 123);
}

#[tokio::test]
async fn caller_injects_operation_id_when_peer_supports_retry() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let server_task = moire::task::spawn(
        async move {
            let server_caller = acceptor_conduit(server_conduit, test_acceptor_handshake())
                .on_connection(OperationIdHandler)
                .establish::<NoopClient>()
                .await
                .expect("server handshake failed");
            server_caller
        }
        .named("server_setup"),
    );

    let caller = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<NoopClient>()
        .await
        .expect("client handshake failed");

    let _server_caller_guard = server_task.await.expect("server setup failed");

    let response = caller
        .caller
        .call(RequestCall {
            method_id: MethodId(1),
            args: Payload::outgoing(&7_u32),
            schemas: Default::default(),
            metadata: Default::default(),
        })
        .await
        .expect("call should succeed");

    let ret_bytes = match &response.ret {
        Payload::PostcardBytes(bytes) => *bytes,
        _ => panic!("expected incoming payload in response"),
    };
    let operation_id: u64 = vox_postcard::from_slice(ret_bytes).expect("deserialize response");
    assert_ne!(operation_id, 0);
}

#[tokio::test]
async fn builder_uses_custom_operation_store() {
    let (client_conduit, server_conduit) = message_conduit_pair();
    let store = Arc::new(CountingOperationStore::new());
    let store_check = Arc::clone(&store);

    let server_task = moire::task::spawn(
        async move {
            let server_caller = acceptor_conduit(server_conduit, test_acceptor_handshake())
                .operation_store(store)
                .on_connection(OperationIdHandler)
                .establish::<NoopClient>()
                .await
                .expect("server handshake failed");
            server_caller
        }
        .named("server_setup"),
    );

    let caller = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<NoopClient>()
        .await
        .expect("client handshake failed");

    let _server_caller_guard = server_task.await.expect("server setup failed");

    let _response = caller
        .caller
        .call(RequestCall {
            method_id: MethodId(1),
            args: Payload::outgoing(&7_u32),
            schemas: Default::default(),
            metadata: Default::default(),
        })
        .await
        .expect("call should succeed");

    assert_ne!(store_check.admits.load(Ordering::SeqCst), 0);
}

/// After disconnect + resume, replaying a sealed operation with the same
/// operation ID must succeed — the schema recv tracker is reset on the new
/// connection so re-sent schemas are accepted.
#[tokio::test]
async fn operation_replay_after_resume_delivers_sealed_outcome() {
    let (client_link1, client_break1, server_link1, server_break1) = breakable_link_pair(64);

    let runs = Arc::new(AtomicUsize::new(0));
    let runs_check = Arc::clone(&runs);
    let release = Arc::new(tokio::sync::Notify::new());

    let (server_established, client_established) = tokio::try_join!(
        tokio::time::timeout(
            Duration::from_secs(2),
            acceptor_conduit(BareConduit::new(server_link1), test_acceptor_handshake())
                .resumable()
                .establish::<NoopClient>(ReplayHandler {
                    runs,
                    release: Arc::clone(&release),
                }),
        ),
        tokio::time::timeout(
            Duration::from_secs(2),
            initiator_conduit(BareConduit::new(client_link1), test_initiator_handshake())
                .resumable()
                .establish::<NoopClient>(),
        ),
    )
    .expect("initial session establishment timed out");
    let _server_caller = server_established.expect("server handshake failed");
    let server_sh = _server_caller.session.clone().unwrap();
    let caller = client_established.expect("client handshake failed");
    let client_sh = caller.session.clone().unwrap();

    // First call — handler runs, response is sealed in the operation store.
    let mut metadata = Metadata::default();
    ensure_operation_id(&mut metadata, vox_types::OperationId(99));

    let call_task = moire::task::spawn(
        {
            let caller = caller.clone();
            let metadata = metadata.clone();
            async move {
                caller
                    .caller
                    .call(RequestCall {
                        method_id: MethodId(1),
                        args: Payload::outgoing(&11_u32),
                        schemas: Default::default(),
                        metadata,
                    })
                    .await
            }
        }
        .named("first_call"),
    );

    // Give the handler time to start, then release it.
    tokio::time::sleep(Duration::from_millis(50)).await;
    release.notify_waiters();

    let response = call_task
        .await
        .expect("join")
        .expect("first call should succeed");
    let ret_bytes = match &response.ret {
        Payload::PostcardBytes(bytes) => *bytes,
        _ => panic!("expected incoming payload"),
    };
    let value: u32 = vox_postcard::from_slice(ret_bytes).expect("deserialize");
    assert_eq!(value, 11);
    assert_eq!(runs_check.load(Ordering::SeqCst), 1);

    // Disconnect and resume on a new link.
    client_break1.close().await;
    server_break1.close().await;
    tokio::time::sleep(Duration::from_millis(25)).await;

    let (client_link2, client_break2, server_link2, server_break2) = breakable_link_pair(64);
    tokio::try_join!(
        client_sh.resume(BareConduit::new(client_link2), test_initiator_handshake()),
        server_sh.resume(BareConduit::new(server_link2), test_acceptor_handshake()),
    )
    .expect("session resume should succeed");

    // Replay the same operation ID — should get the sealed response without
    // running the handler again, and without duplicate schema errors.
    let replayed = caller
        .caller
        .call(RequestCall {
            method_id: MethodId(1),
            args: Payload::outgoing(&11_u32),
            schemas: Default::default(),
            metadata,
        })
        .await
        .expect("replay after resume should succeed");
    let ret_bytes = match &replayed.ret {
        Payload::PostcardBytes(bytes) => *bytes,
        _ => panic!("expected incoming payload"),
    };
    let value: u32 = vox_postcard::from_slice(ret_bytes).expect("deserialize");
    assert_eq!(value, 11);
    assert_eq!(
        runs_check.load(Ordering::SeqCst),
        1,
        "handler should only run once"
    );

    let _ = client_sh.shutdown();
    let _ = server_sh.shutdown();
    client_break2.close().await;
    server_break2.close().await;
}

/// Sending the same operation ID twice on the same connection (without
/// disconnect) is a protocol error — the second call should be rejected.
#[tokio::test]
async fn duplicate_operation_id_on_same_connection_is_rejected() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let release = Arc::new(tokio::sync::Notify::new());
    let release_server = Arc::clone(&release);

    let server_task = moire::task::spawn(
        async move {
            let server_caller = acceptor_conduit(server_conduit, test_acceptor_handshake())
                .establish::<NoopClient>(ReplayHandler {
                    runs: Arc::new(AtomicUsize::new(0)),
                    release: release_server,
                })
                .await
                .expect("server handshake failed");
            server_caller
        }
        .named("server_setup"),
    );

    let caller = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<NoopClient>()
        .await
        .expect("client handshake failed");

    let _server_caller_guard = server_task.await.expect("server setup failed");

    let mut metadata = Metadata::default();
    ensure_operation_id(&mut metadata, vox_types::OperationId(99));

    // First call — blocks in the handler until release.
    let first = moire::task::spawn(
        {
            let caller = caller.clone();
            let metadata = metadata.clone();
            async move {
                caller
                    .caller
                    .call(RequestCall {
                        method_id: MethodId(1),
                        args: Payload::outgoing(&11_u32),
                        schemas: Default::default(),
                        metadata,
                    })
                    .await
            }
        }
        .named("first_call"),
    );

    // Give it time to reach the server.
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Second call with the same operation ID on the same connection — should
    // attach to the live operation and both get the same response.
    let second = moire::task::spawn(
        {
            let caller = caller.clone();
            let metadata = metadata.clone();
            async move {
                caller
                    .caller
                    .call(RequestCall {
                        method_id: MethodId(1),
                        args: Payload::outgoing(&11_u32),
                        schemas: Default::default(),
                        metadata,
                    })
                    .await
            }
        }
        .named("second_call"),
    );

    // Release the handler.
    release.notify_waiters();

    // Both should succeed with the same value.
    let r1 = first
        .await
        .expect("first join")
        .expect("first call should succeed");
    let r2 = second
        .await
        .expect("second join")
        .expect("second call should succeed");

    for response in [r1, r2] {
        let ret_bytes = match &response.ret {
            Payload::PostcardBytes(bytes) => *bytes,
            _ => panic!("expected incoming payload"),
        };
        let value: u32 = vox_postcard::from_slice(ret_bytes).expect("deserialize");
        assert_eq!(value, 11);
    }
}

/// Verify that MessagePlan built from identical schemas can round-trip a message.
#[test]
fn message_plan_from_identical_schemas_round_trips() {
    let schemas = vox_types::extract_schemas(<Message<'static> as Facet<'static>>::SHAPE)
        .expect("schema extraction")
        .schemas;
    let handshake_result = HandshakeResult {
        role: SessionRole::Initiator,
        our_settings: ConnectionSettings {
            parity: Parity::Odd,
            max_concurrent_requests: 64,
        },
        peer_settings: ConnectionSettings {
            parity: Parity::Even,
            max_concurrent_requests: 64,
        },
        peer_supports_retry: false,
        session_resume_key: None,
        peer_resume_key: None,
        our_schema: schemas.clone(),
        peer_schema: schemas,
        peer_metadata: vec![],
    };
    let plan = crate::MessagePlan::from_handshake(&handshake_result)
        .expect("should build message plan from identical schemas");

    // Serialize a simple Ping message
    let msg = Message {
        connection_id: vox_types::ConnectionId::ROOT,
        payload: MessagePayload::Ping(vox_types::Ping { nonce: 42 }),
    };
    let bytes = vox_postcard::to_vec(&msg).expect("serialize message");
    let backing = Backing::Boxed(bytes.into());

    // Deserialize with the plan
    let decoded: SelfRef<Message<'static>> =
        crate::deserialize_postcard_with_plan(backing, &plan.plan, &plan.registry)
            .expect("should deserialize with identical-schema plan");
    assert_eq!(decoded.connection_id, vox_types::ConnectionId::ROOT);
    match &decoded.payload {
        MessagePayload::Ping(ping) => assert_eq!(ping.nonce, 42),
        other => panic!("expected Ping, got {other:?}"),
    }
}

/// Minimal test: establish via real CBOR handshake, send one call, verify handler runs.
#[tokio::test]
async fn call_through_cbor_handshake_reaches_handler() {
    let (client_link, server_link) = memory_link_pair(64);

    let (server_result, client_result) = tokio::try_join!(
        tokio::time::timeout(
            Duration::from_secs(1),
            acceptor_on(server_link)
                .on_connection(EchoHandler)
                .establish::<NoopClient>(),
        ),
        tokio::time::timeout(
            Duration::from_secs(1),
            initiator_on(client_link, TransportMode::Bare).establish::<NoopClient>(),
        ),
    )
    .expect("session establishment timed out");

    let _server_caller = server_result.expect("server establish failed");
    let caller = client_result.expect("client establish failed");

    let response = tokio::time::timeout(
        Duration::from_secs(1),
        caller.caller.call(RequestCall {
            method_id: MethodId(1),
            args: Payload::outgoing(&42_u32),
            schemas: Default::default(),
            metadata: Default::default(),
        }),
    )
    .await
    .expect("call timed out")
    .expect("call should succeed");

    let ret_bytes = match &response.ret {
        Payload::PostcardBytes(bytes) => *bytes,
        _ => panic!("expected incoming payload in response"),
    };
    let value: u32 = vox_postcard::from_slice(ret_bytes).expect("deserialize response");
    assert_eq!(value, 42);
}

/// Same as above but through the stable conduit path.
#[tokio::test]
async fn call_through_stable_conduit_reaches_handler() {
    let (client_link, server_link) = memory_link_pair(64);

    let (server_result, client_result) = tokio::try_join!(
        tokio::time::timeout(
            Duration::from_secs(1),
            acceptor_on(server_link)
                .on_connection(EchoHandler)
                .establish::<NoopClient>(),
        ),
        tokio::time::timeout(
            Duration::from_secs(1),
            initiator_on(client_link, TransportMode::Stable).establish::<NoopClient>(),
        ),
    )
    .expect("session establishment timed out");

    let _server_caller = server_result.expect("server establish failed");
    let caller = client_result.expect("client establish failed");

    let response = tokio::time::timeout(
        Duration::from_secs(1),
        caller.caller.call(RequestCall {
            method_id: MethodId(1),
            args: Payload::outgoing(&42_u32),
            schemas: Default::default(),
            metadata: Default::default(),
        }),
    )
    .await
    .expect("call timed out")
    .expect("call should succeed");

    let ret_bytes = match &response.ret {
        Payload::PostcardBytes(bytes) => *bytes,
        _ => panic!("expected incoming payload in response"),
    };
    let value: u32 = vox_postcard::from_slice(ret_bytes).expect("deserialize response");
    assert_eq!(value, 42);
}

/// Multiple calls through stable conduit to verify seq/ack tracking works.
#[tokio::test]
async fn multiple_calls_through_stable_conduit() {
    let (client_link, server_link) = memory_link_pair(64);

    let (server_result, client_result) = tokio::try_join!(
        tokio::time::timeout(
            Duration::from_secs(1),
            acceptor_on(server_link)
                .on_connection(EchoHandler)
                .establish::<NoopClient>(),
        ),
        tokio::time::timeout(
            Duration::from_secs(1),
            initiator_on(client_link, TransportMode::Stable).establish::<NoopClient>(),
        ),
    )
    .expect("session establishment timed out");

    let _server_caller = server_result.expect("server establish failed");
    let caller = client_result.expect("client establish failed");

    for i in 0_u32..10 {
        let response = tokio::time::timeout(
            Duration::from_secs(1),
            caller.caller.call(RequestCall {
                method_id: MethodId(1),
                args: Payload::outgoing(&i),
                schemas: Default::default(),
                metadata: Default::default(),
            }),
        )
        .await
        .expect("call timed out")
        .expect("call should succeed");

        let ret_bytes = match &response.ret {
            Payload::PostcardBytes(bytes) => *bytes,
            _ => panic!("expected incoming payload in response"),
        };
        let value: u32 = vox_postcard::from_slice(ret_bytes).expect("deserialize response");
        assert_eq!(value, i);
    }
}

#[tokio::test]
async fn in_flight_call_returns_cancelled_when_peer_closes() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let was_cancelled = Arc::new(AtomicBool::new(false));
    let was_cancelled_check = was_cancelled.clone();

    let (session_tx, session_rx) = tokio::sync::oneshot::channel::<moire::task::JoinHandle<()>>();
    let server_task = moire::task::spawn(
        async move {
            let server_caller = acceptor_conduit(server_conduit, test_acceptor_handshake())
                .spawn_fn(move |fut| {
                    let handle = moire::task::spawn(fut);
                    let _ = session_tx.send(handle);
                })
                .establish::<NoopClient>(BlockingHandler {
                    was_cancelled,
                    retry: RetryPolicy::VOLATILE,
                })
                .await
                .expect("server handshake failed");
            server_caller
        }
        .named("server_setup"),
    );

    let caller = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<NoopClient>()
        .await
        .expect("client handshake failed");

    let server_caller_guard = server_task.await.expect("server setup failed");
    let server_session_task = session_rx.await.expect("session handle sent");

    let call_task = moire::task::spawn(
        async move {
            caller
                .caller
                .call(RequestCall {
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

#[tokio::test]
async fn keepalive_timeout_returns_cancelled_when_pongs_are_missing() {
    let (client_link, server_link) = memory_link_pair(64);
    let client_conduit = DropPongConduit::new(BareConduit::new(client_link));
    let server_conduit = BareConduit::new(server_link);

    let server_task = moire::task::spawn(
        async move {
            let server_caller = acceptor_conduit(server_conduit, test_acceptor_handshake())
                .establish::<NoopClient>(BlockingHandler {
                    was_cancelled: Arc::new(AtomicBool::new(false)),
                    retry: RetryPolicy::VOLATILE,
                })
                .await
                .expect("server handshake failed");
            server_caller
        }
        .named("server_setup"),
    );

    let caller = initiator_conduit(client_conduit, test_initiator_handshake())
        .keepalive(SessionKeepaliveConfig {
            ping_interval: std::time::Duration::from_millis(20),
            pong_timeout: std::time::Duration::from_millis(50),
        })
        .establish::<NoopClient>()
        .await
        .expect("client handshake failed");

    let _server_caller_guard = server_task.await.expect("server setup failed");

    let call_task = moire::task::spawn(
        async move {
            caller
                .caller
                .call(RequestCall {
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

#[tokio::test]
async fn dropping_root_caller_shuts_down_session() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let (client_session_tx, client_session_rx) =
        tokio::sync::oneshot::channel::<moire::task::JoinHandle<()>>();
    let (server_session_tx, server_session_rx) =
        tokio::sync::oneshot::channel::<moire::task::JoinHandle<()>>();

    let server_task = moire::task::spawn(
        async move {
            let server_caller = acceptor_conduit(server_conduit, test_acceptor_handshake())
                .spawn_fn(move |fut| {
                    let handle = moire::task::spawn(fut.named("server_session"));
                    let _ = server_session_tx.send(handle);
                })
                .on_connection(EchoHandler)
                .establish::<NoopClient>()
                .await
                .expect("server handshake failed");
            server_caller
        }
        .named("server_setup"),
    );

    let caller = initiator_conduit(client_conduit, test_initiator_handshake())
        .spawn_fn(move |fut| {
            let handle = moire::task::spawn(fut.named("client_session"));
            let _ = client_session_tx.send(handle);
        })
        .establish::<NoopClient>()
        .await
        .expect("client handshake failed");

    let _server_caller_guard = server_task.await.expect("server setup failed");

    let client_session = client_session_rx.await.expect("client session handle sent");
    let server_session = server_session_rx.await.expect("server session handle sent");

    drop(caller);

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

/// An acceptor that spawns an EchoHandler driver on each accepted connection.

/// Regression test: schema recv tracker must be per-connection.
/// If it were per-session, the second call (on the virtual connection) would
/// fail because the response schemas overlap with the root connection's.
#[tokio::test]
async fn schema_tracker_is_per_connection_not_per_session() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let server_task = moire::task::spawn(
        async move {
            let server_caller = acceptor_conduit(server_conduit, test_acceptor_handshake())
                .on_connection(EchoAcceptor)
                .on_connection(EchoHandler)
                .establish::<NoopClient>()
                .await
                .expect("server handshake failed");
            server_caller
        }
        .named("server_setup"),
    );

    let root_caller = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<NoopClient>()
        .await
        .expect("client handshake failed");
    let session_handle = root_caller.session.clone().unwrap();

    let _server_caller_guard = server_task.await.expect("server setup failed");

    // Call on the root connection — this sends and receives schemas.
    let args_value: u32 = 100;
    let response = root_caller
        .caller
        .call(RequestCall {
            method_id: MethodId(1),
            args: Payload::outgoing(&args_value),
            schemas: Default::default(),
            metadata: Default::default(),
        })
        .await
        .expect("root call should succeed");
    let ret_bytes = match &response.ret {
        Payload::PostcardBytes(bytes) => *bytes,
        _ => panic!("expected incoming payload"),
    };
    let result: u32 = vox_postcard::from_slice(ret_bytes).expect("deserialize root response");
    assert_eq!(result, 100);

    // Open a virtual connection and call on it.
    // The same schema types (u32, Result, etc.) appear on both connections.
    // If the recv tracker were shared, recording the virtual connection's
    // schemas would hit a duplicate and panic.
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
    moire::task::spawn(async move { vconn_driver.run().await }.named("vconn_driver"));

    let args_value: u32 = 200;
    let response = vconn_caller
        .call(RequestCall {
            method_id: MethodId(1),
            args: Payload::outgoing(&args_value),
            schemas: Default::default(),
            metadata: Default::default(),
        })
        .await
        .expect("virtual connection call should succeed");
    let ret_bytes = match &response.ret {
        Payload::PostcardBytes(bytes) => *bytes,
        _ => panic!("expected incoming payload"),
    };
    let result: u32 = vox_postcard::from_slice(ret_bytes).expect("deserialize vconn response");
    assert_eq!(result, 200);
}

#[tokio::test]
async fn initiator_builder_customization_controls_allocated_connection_parity() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let server_task = moire::task::spawn(
        async move {
            let server_caller = acceptor_conduit(
                server_conduit,
                HandshakeResult {
                    role: SessionRole::Acceptor,
                    our_settings: ConnectionSettings {
                        parity: Parity::Odd,
                        max_concurrent_requests: 32,
                    },
                    peer_settings: ConnectionSettings {
                        parity: Parity::Even,
                        max_concurrent_requests: 64,
                    },
                    peer_supports_retry: false,
                    session_resume_key: None,
                    peer_resume_key: None,
                    our_schema: vec![],
                    peer_schema: vec![],
                    peer_metadata: vec![],
                },
            )
            .on_connection(EchoAcceptor)
            .establish::<NoopClient>()
            .await
            .expect("server handshake failed");
            server_caller
        }
        .named("server_setup"),
    );

    let _client_caller_guard = initiator_conduit(
        client_conduit,
        HandshakeResult {
            role: SessionRole::Initiator,
            our_settings: ConnectionSettings {
                parity: Parity::Even,
                max_concurrent_requests: 64,
            },
            peer_settings: ConnectionSettings {
                parity: Parity::Odd,
                max_concurrent_requests: 32,
            },
            peer_supports_retry: false,
            session_resume_key: None,
            peer_resume_key: None,
            our_schema: vec![],
            peer_schema: vec![],
            peer_metadata: vec![],
        },
    )
    .establish::<NoopClient>()
    .await
    .expect("client handshake failed");
    let session_handle = _client_caller_guard.session.clone().unwrap();

    let _server_caller_guard = server_task.await.expect("server setup failed");

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
    let (client_conduit, server_conduit) = message_conduit_pair();

    let initiator_task = moire::task::spawn(
        async move {
            let initiator_caller = initiator_conduit(
                client_conduit,
                HandshakeResult {
                    role: SessionRole::Initiator,
                    our_settings: ConnectionSettings {
                        parity: Parity::Even,
                        max_concurrent_requests: 64,
                    },
                    peer_settings: ConnectionSettings {
                        parity: Parity::Odd,
                        max_concurrent_requests: 32,
                    },
                    peer_supports_retry: false,
                    session_resume_key: None,
                    peer_resume_key: None,
                    our_schema: vec![],
                    peer_schema: vec![],
                    peer_metadata: vec![],
                },
            )
            .on_connection(EchoAcceptor)
            .establish::<NoopClient>()
            .await
            .expect("initiator handshake failed");
            initiator_caller
        }
        .named("initiator_setup"),
    );

    let _acceptor_caller_guard = acceptor_conduit(
        server_conduit,
        HandshakeResult {
            role: SessionRole::Acceptor,
            our_settings: ConnectionSettings {
                parity: Parity::Odd,
                max_concurrent_requests: 32,
            },
            peer_settings: ConnectionSettings {
                parity: Parity::Even,
                max_concurrent_requests: 64,
            },
            peer_supports_retry: false,
            session_resume_key: None,
            peer_resume_key: None,
            our_schema: vec![],
            peer_schema: vec![],
            peer_metadata: vec![],
        },
    )
    .establish::<NoopClient>()
    .await
    .expect("acceptor handshake failed");
    let acceptor_session_handle = _acceptor_caller_guard.session.clone().unwrap();

    let _initiator_caller_guard = initiator_task.await.expect("initiator setup failed");

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

// r[verify connection.close]
#[tokio::test]
async fn close_root_connection_is_rejected() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let server_task = moire::task::spawn(
        async move {
            let server_caller = acceptor_conduit(server_conduit, test_acceptor_handshake())
                .on_connection(EchoHandler)
                .establish::<NoopClient>()
                .await
                .expect("server handshake failed");
            server_caller
        }
        .named("server_setup"),
    );

    let _client_caller_guard = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<NoopClient>()
        .await
        .expect("client handshake failed");
    let session_handle = _client_caller_guard.session.clone().unwrap();

    let _server_caller_guard = server_task.await.expect("server setup failed");

    let result = session_handle
        .close_connection(vox_types::ConnectionId::ROOT, vec![])
        .await;
    assert!(
        matches!(result, Err(SessionError::Protocol(ref msg)) if msg == "cannot close root connection"),
        "expected root-close protocol error, got: {result:?}"
    );
}

#[tokio::test]
async fn echo_call_across_memory_link() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    // Server and client handshakes must run concurrently — both sides exchange
    // settings before either can proceed.
    let server_task = moire::task::spawn(
        async move {
            let server_caller = acceptor_conduit(server_conduit, test_acceptor_handshake())
                .on_connection(EchoHandler)
                .establish::<NoopClient>()
                .await
                .expect("server handshake failed");
            server_caller
        }
        .named("server_setup"),
    );

    // Set up client side (runs concurrently with server_task above).
    let caller = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<NoopClient>()
        .await
        .expect("client handshake failed");

    let _server_caller_guard = server_task.await.expect("server setup failed");

    // Make a call: serialize a u32 as the args payload.
    let args_value: u32 = 42;
    let response = caller
        .caller
        .call(RequestCall {
            method_id: MethodId(1),
            args: Payload::outgoing(&args_value),
            schemas: Default::default(),
            metadata: Default::default(),
        })
        .await
        .expect("call should succeed");

    // The echo handler sends back the same bytes. Deserialize the response.
    let ret_bytes = match &response.ret {
        Payload::PostcardBytes(bytes) => *bytes,
        _ => panic!("expected incoming payload in response"),
    };
    let result: u32 = vox_postcard::from_slice(ret_bytes).expect("deserialize response");
    assert_eq!(result, 42);
}

#[tokio::test]
async fn buffers_inbound_channel_items_until_rx_is_registered() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let server_task = moire::task::spawn(
        async move {
            let server_caller = acceptor_conduit(server_conduit, test_acceptor_handshake())
                .establish::<NoopClient>()
                .await
                .expect("server handshake failed");
            server_caller
        }
        .named("server_setup"),
    );

    let client_caller = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<NoopClient>()
        .await
        .expect("client handshake failed");
    let client_sender = client_caller.caller.driver().connection_sender().clone();

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
    let bytes = match item.item {
        Payload::PostcardBytes(bytes) => bytes,
        _ => panic!("expected incoming payload"),
    };
    let decoded: u32 = vox_postcard::from_slice(bytes).expect("deserialize buffered item");
    assert_eq!(decoded, 123);
}

#[tokio::test]
async fn grant_credit_unblocks_driver_created_tx_channel() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let server_task = moire::task::spawn(
        async move {
            let server_caller = acceptor_conduit(server_conduit, test_acceptor_handshake())
                .establish::<NoopClient>()
                .await
                .expect("server handshake failed");
            server_caller
        }
        .named("server_setup"),
    );

    let client_caller = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<NoopClient>()
        .await
        .expect("client handshake failed");
    let client_sender = client_caller.caller.driver().connection_sender().clone();

    let server_caller = server_task.await.expect("server setup failed");
    let (channel_id, sink) = server_caller.caller.driver().create_tx_channel();

    // Exhaust the default 16 credits.
    for _ in 0..16 {
        let value = 0_u32;
        sink.send_payload(Payload::outgoing(&value))
            .await
            .expect("send within initial credit");
    }

    let send_task = moire::task::spawn(async move {
        let value = 42_u32;
        sink.send_payload(Payload::outgoing(&value)).await
    });

    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    assert!(
        !send_task.is_finished(),
        "send should block when credit is exhausted"
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
            let server_caller = acceptor_conduit(server_conduit, test_acceptor_handshake())
                .establish::<NoopClient>()
                .await
                .expect("server handshake failed");
            server_caller
        }
        .named("server_setup"),
    );

    let client_caller = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<NoopClient>()
        .await
        .expect("client handshake failed");
    let client_sender = client_caller.caller.driver().connection_sender().clone();

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
            let server_caller = acceptor_conduit(server_conduit, test_acceptor_handshake())
                .on_connection(EchoHandler)
                .establish::<NoopClient>()
                .await
                .expect("server handshake failed");
            (
                server_caller.caller.driver().connection_sender().clone(),
                server_caller,
            )
        }
        .named("server_setup"),
    );

    let caller = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<NoopClient>()
        .await
        .expect("client handshake failed");

    let (server_sender, _server_caller_guard) = server_task.await.expect("server setup failed");

    server_sender
        .send(crate::session::ConnectionMessage::Request(RequestMessage {
            id: vox_types::RequestId(9999),
            body: RequestBody::Response(RequestResponse {
                ret: Payload::outgoing(&123_u32),
                schemas: Default::default(),
                metadata: Default::default(),
            }),
        }))
        .await
        .expect("send unsolicited response");

    let args_value: u32 = 42;
    let response = caller
        .caller
        .call(RequestCall {
            method_id: MethodId(1),
            args: Payload::outgoing(&args_value),
            schemas: Default::default(),
            metadata: Default::default(),
        })
        .await
        .expect("call should still succeed after unsolicited response");

    let ret_bytes = match &response.ret {
        Payload::PostcardBytes(bytes) => *bytes,
        _ => panic!("expected incoming payload"),
    };
    let result: u32 = vox_postcard::from_slice(ret_bytes).expect("deserialize response");
    assert_eq!(result, 42);
}

#[tokio::test]
async fn proxy_connections_forwards_calls_without_service_specific_proxy_code() {
    let (host_a_conduit, guest_a_conduit) = message_conduit_pair();
    let (host_b_conduit, guest_b_conduit) = message_conduit_pair();

    struct ProxyHostAcceptor {
        upstream_session: SessionHandle,
    }
    impl ConnectionAcceptor for ProxyHostAcceptor {
        fn accept(
            &self,
            _request: &ConnectionRequest,
            connection: PendingConnection,
        ) -> Result<(), Metadata<'static>> {
            let upstream_session = self.upstream_session.clone();
            let incoming = connection.into_handle();
            moire::task::spawn(
                async move {
                    let upstream = upstream_session
                        .open_connection(
                            ConnectionSettings {
                                parity: Parity::Odd,
                                max_concurrent_requests: 64,
                            },
                            vec![],
                        )
                        .await
                        .expect("host->guest-b open_connection");
                    let _ = proxy_connections(incoming, upstream).await;
                }
                .named("host_proxy_vconn"),
            );
            Ok(())
        }
    }

    let guest_b_task = moire::task::spawn(
        async move {
            let guard = acceptor_conduit(guest_b_conduit, test_acceptor_handshake())
                .on_connection(EchoAcceptor)
                .establish::<NoopClient>()
                .await
                .expect("guest-b establish");
            let _guard = guard;
            std::future::pending::<()>().await;
        }
        .named("guest_b_root"),
    );

    let _host_to_b_guard = initiator_conduit(host_b_conduit, test_initiator_handshake())
        .establish::<NoopClient>()
        .await
        .expect("host<->guest-b establish");
    let host_to_b_session = _host_to_b_guard.session.clone().unwrap();

    let host_for_a_task = moire::task::spawn(
        async move {
            let guard = acceptor_conduit(host_a_conduit, test_acceptor_handshake())
                .on_connection(ProxyHostAcceptor {
                    upstream_session: host_to_b_session,
                })
                .establish::<NoopClient>()
                .await
                .expect("host<->guest-a establish");
            let _guard = guard;
            std::future::pending::<()>().await;
        }
        .named("host_for_guest_a_root"),
    );

    let _guest_a_root_guard = initiator_conduit(guest_a_conduit, test_initiator_handshake())
        .establish::<NoopClient>()
        .await
        .expect("guest-a<->host establish");
    let guest_a_session = _guest_a_root_guard.session.clone().unwrap();

    let proxy_conn = guest_a_session
        .open_connection(
            ConnectionSettings {
                parity: Parity::Odd,
                max_concurrent_requests: 64,
            },
            vec![],
        )
        .await
        .expect("guest-a open proxy connection");
    let proxy_conn_id = proxy_conn.connection_id();

    let mut proxy_driver = Driver::new(proxy_conn, ());
    let proxy_caller = crate::Caller::new(proxy_driver.caller());
    let proxy_driver_task =
        moire::task::spawn(async move { proxy_driver.run().await }.named("guest_a_proxy_driver"));

    let args_value: u32 = 777;
    let response = proxy_caller
        .call(RequestCall {
            method_id: MethodId(1),
            args: Payload::outgoing(&args_value),
            schemas: Default::default(),
            metadata: Default::default(),
        })
        .await
        .expect("proxied call should succeed");
    let ret_bytes = match &response.ret {
        Payload::PostcardBytes(bytes) => *bytes,
        _ => panic!("expected incoming payload"),
    };
    let result: u32 = vox_postcard::from_slice(ret_bytes).expect("deserialize proxied response");
    assert_eq!(result, args_value);

    guest_a_session
        .close_connection(proxy_conn_id, vec![])
        .await
        .expect("close proxy connection");

    proxy_driver_task.abort();
    guest_b_task.abort();
    host_for_a_task.abort();
}
