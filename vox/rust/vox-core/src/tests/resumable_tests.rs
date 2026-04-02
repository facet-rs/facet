use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use moire::task::FutureExt;
use vox_types::{
    ConnectionSettings, MethodId, Parity, Payload, RequestCall, RetryPolicy, VoxError,
};

use super::utils::*;
use crate::session::{
    SessionAcceptOutcome, SessionRegistry, acceptor_conduit, acceptor_on, initiator_conduit,
    initiator_on,
};
use crate::{Attachment, BareConduit, NoopClient, TransportMode, initiate_transport};

#[tokio::test]
async fn resumable_session_keeps_pending_call_alive_across_manual_resume() {
    let (client_link1, client_break1, server_link1, server_break1) = breakable_link_pair(64);
    let client_conduit1 = BareConduit::new(client_link1);
    let server_conduit1 = BareConduit::new(server_link1);

    let started = Arc::new(tokio::sync::Notify::new());
    let started_for_wait = Arc::clone(&started);
    let started_wait = started_for_wait.notified();
    let release = Arc::new(tokio::sync::Notify::new());

    let (server_established, client_established) = tokio::try_join!(
        tokio::time::timeout(
            Duration::from_secs(1),
            acceptor_conduit(server_conduit1, test_acceptor_handshake())
                .resumable()
                .on_connection(ResumableReplyingHandler {
                    started,
                    release: Arc::clone(&release),
                })
                .establish::<NoopClient>(),
        ),
        tokio::time::timeout(
            Duration::from_secs(1),
            initiator_conduit(client_conduit1, test_initiator_handshake())
                .resumable()
                .establish::<NoopClient>(),
        ),
    )
    .expect("initial session establishment timed out");
    let _server_caller = server_established.expect("server handshake failed");
    let server_session_handle = _server_caller.session.clone().unwrap();
    let caller = client_established.expect("client handshake failed");
    let client_session_handle = caller.session.clone().unwrap();

    let call_task = moire::task::spawn(
        async move {
            caller
                .caller
                .call(RequestCall {
                    method_id: MethodId(1),
                    args: Payload::outgoing(&55_u32),
                    schemas: Default::default(),
                    metadata: Default::default(),
                })
                .await
        }
        .named("resume_pending_call"),
    );

    tokio::time::timeout(Duration::from_secs(1), started_wait)
        .await
        .expect("timed out waiting for handler start");

    client_break1.close().await;
    server_break1.close().await;
    tokio::time::sleep(Duration::from_millis(25)).await;

    let (client_link2, client_break2, server_link2, server_break2) = breakable_link_pair(64);
    tokio::try_join!(
        client_session_handle.resume(BareConduit::new(client_link2), test_initiator_handshake()),
        server_session_handle.resume(BareConduit::new(server_link2), test_acceptor_handshake()),
    )
    .expect("session resume should succeed");

    release.notify_waiters();

    let response = call_task
        .await
        .expect("call task join")
        .expect("call should succeed after session resume");
    let response = response.get();
    let ret_bytes = match &response.ret {
        Payload::PostcardBytes(bytes) => *bytes,
        _ => panic!("expected incoming payload in response"),
    };
    let value: u32 = vox_postcard::from_slice(ret_bytes).expect("deserialize response");
    assert_eq!(value, 55);

    let _ = client_session_handle.shutdown();
    let _ = server_session_handle.shutdown();
    client_break2.close().await;
    server_break2.close().await;
}

#[tokio::test]
async fn resumable_acceptor_registry_keeps_pending_call_alive_across_auto_resume() {
    let registry = SessionRegistry::default();
    let (client_link1, client_break1, server_link1, server_break1) = breakable_link_pair(64);
    let started = Arc::new(tokio::sync::Notify::new());
    let started_for_wait = Arc::clone(&started);
    let started_wait = started_for_wait.notified();
    let release = Arc::new(tokio::sync::Notify::new());

    let (server_established, client_established) = tokio::try_join!(
        tokio::time::timeout(
            Duration::from_secs(1),
            acceptor_on(server_link1)
                .session_registry(registry.clone())
                .on_connection(ResumableReplyingHandler {
                    started,
                    release: Arc::clone(&release),
                })
                .establish_or_resume::<NoopClient>(),
        ),
        tokio::time::timeout(
            Duration::from_secs(1),
            initiator_on(client_link1, TransportMode::Bare)
                .resumable()
                .establish::<NoopClient>(),
        ),
    )
    .expect("initial session establishment timed out");
    let server_caller = match server_established.expect("server handshake failed") {
        SessionAcceptOutcome::Established(client) => client,
        SessionAcceptOutcome::Resumed => panic!("first accept should establish a new session"),
    };
    let caller = client_established.expect("client handshake failed");
    let client_session_handle = caller.session.clone().unwrap();

    let call_task = moire::task::spawn(
        async move {
            caller
                .caller
                .call(RequestCall {
                    method_id: MethodId(1),
                    args: Payload::outgoing(&66_u32),
                    schemas: Default::default(),
                    metadata: Default::default(),
                })
                .await
        }
        .named("registry_resume_pending_call"),
    );

    tokio::time::timeout(Duration::from_secs(1), started_wait)
        .await
        .expect("timed out waiting for handler start");

    client_break1.close().await;
    server_break1.close().await;
    tokio::time::sleep(Duration::from_millis(25)).await;

    let (client_link2, client_break2, server_link2, server_break2) = breakable_link_pair(64);
    let (resume_result, server_accept_result) = tokio::join!(
        async {
            let mut resumed_link = initiate_transport(client_link2, TransportMode::Bare)
                .await
                .expect("client transport prologue should succeed");
            let handshake_result = crate::handshake_as_initiator(
                &resumed_link.tx,
                &mut resumed_link.rx,
                ConnectionSettings {
                    parity: Parity::Odd,
                    max_concurrent_requests: 64,
                },
                true,
                client_session_handle.resume_key(),
                vec![],
            )
            .await
            .expect("client CBOR handshake should succeed");
            let message_plan = crate::MessagePlan::from_handshake(&handshake_result)
                .expect("message plan should build");
            client_session_handle
                .resume(
                    BareConduit::with_message_plan(resumed_link, message_plan),
                    handshake_result,
                )
                .await
        },
        acceptor_on(server_link2)
            .session_registry(registry.clone())
            .on_connection(ResumableReplyingHandler {
                started: Arc::new(tokio::sync::Notify::new()),
                release: Arc::clone(&release),
            })
            .establish_or_resume::<NoopClient>(),
    );
    resume_result.expect("client session resume should succeed");
    match server_accept_result.expect("server accept should succeed") {
        SessionAcceptOutcome::Resumed => {}
        SessionAcceptOutcome::Established(_) => {
            panic!("registry accept should have resumed the existing session")
        }
    }

    release.notify_waiters();

    let response = call_task
        .await
        .expect("call task join")
        .expect("call should succeed after registry-driven session resume");
    let response = response.get();
    let ret_bytes = match &response.ret {
        Payload::PostcardBytes(bytes) => *bytes,
        _ => panic!("expected incoming payload in response"),
    };
    let value: u32 = vox_postcard::from_slice(ret_bytes).expect("deserialize response");
    assert_eq!(value, 66);

    drop(server_caller);
    let _ = client_session_handle.shutdown();
    client_break2.close().await;
    server_break2.close().await;
}

#[tokio::test]
async fn resumable_source_initiator_keeps_pending_call_alive_across_auto_resume() {
    let registry = SessionRegistry::default();
    let (client_link1, client_break1, server_link1, server_break1) = breakable_link_pair(64);
    let (client_link2, client_break2, server_link2, server_break2) = breakable_link_pair(64);

    let source = TestLinkSource::new([
        Attachment::initiator(client_link1),
        Attachment::initiator(client_link2),
    ]);

    let started = Arc::new(tokio::sync::Notify::new());
    let started_for_wait = Arc::clone(&started);
    let started_wait = started_for_wait.notified();
    let release = Arc::new(tokio::sync::Notify::new());

    let (server_established, client_established) = tokio::try_join!(
        tokio::time::timeout(
            Duration::from_secs(1),
            acceptor_on(server_link1)
                .session_registry(registry.clone())
                .on_connection(ResumableReplyingHandler {
                    started,
                    release: Arc::clone(&release),
                })
                .establish_or_resume::<NoopClient>(),
        ),
        tokio::time::timeout(
            Duration::from_secs(1),
            crate::initiator(source, TransportMode::Bare).establish::<NoopClient>(),
        ),
    )
    .expect("initial session establishment timed out");

    let server_caller = match server_established.expect("server handshake failed") {
        SessionAcceptOutcome::Established(client) => client,
        SessionAcceptOutcome::Resumed => panic!("first accept should establish a new session"),
    };
    let caller = client_established.expect("client handshake failed");
    let client_session_handle = caller.session.clone().unwrap();

    let call_task = moire::task::spawn(
        async move {
            caller
                .caller
                .call(RequestCall {
                    method_id: MethodId(1),
                    args: Payload::outgoing(&77_u32),
                    schemas: Default::default(),
                    metadata: Default::default(),
                })
                .await
        }
        .named("source_auto_resume_pending_call"),
    );

    tokio::time::timeout(Duration::from_secs(1), started_wait)
        .await
        .expect("timed out waiting for handler start");

    client_break1.close().await;
    server_break1.close().await;

    let server_accept_result = tokio::time::timeout(
        Duration::from_secs(1),
        acceptor_on(server_link2)
            .session_registry(registry.clone())
            .on_connection(ResumableReplyingHandler {
                started: Arc::new(tokio::sync::Notify::new()),
                release: Arc::clone(&release),
            })
            .establish_or_resume::<NoopClient>(),
    )
    .await
    .expect("timed out waiting for source-driven resume");
    match server_accept_result.expect("server accept should succeed") {
        SessionAcceptOutcome::Resumed => {}
        SessionAcceptOutcome::Established(_) => {
            panic!("registry accept should have resumed the existing session")
        }
    }

    release.notify_waiters();

    let response = call_task
        .await
        .expect("call task join")
        .expect("call should succeed after source-driven auto-resume");
    let response = response.get();
    let ret_bytes = match &response.ret {
        Payload::PostcardBytes(bytes) => *bytes,
        _ => panic!("expected incoming payload in response"),
    };
    let value: u32 = vox_postcard::from_slice(ret_bytes).expect("deserialize response");
    assert_eq!(value, 77);

    drop(server_caller);
    let _ = client_session_handle.shutdown();
    client_break2.close().await;
    server_break2.close().await;
}

#[tokio::test]
async fn resumable_source_initiator_falls_back_to_fresh_session_when_resume_key_unknown() {
    let initial_registry = SessionRegistry::default();
    let restarted_registry = SessionRegistry::default();
    let (client_link1, client_break1, server_link1, server_break1) = breakable_link_pair(64);
    let (client_link2, client_break2, server_link2, server_break2) = breakable_link_pair(64);

    let source = TestLinkSource::new([
        Attachment::initiator(client_link1),
        Attachment::initiator(client_link2),
    ]);

    let started = Arc::new(tokio::sync::Notify::new());
    let started_for_wait = Arc::clone(&started);
    let started_wait = started_for_wait.notified();
    let release = Arc::new(tokio::sync::Notify::new());

    let (server_established, client_established) = tokio::try_join!(
        tokio::time::timeout(
            Duration::from_secs(1),
            acceptor_on(server_link1)
                .session_registry(initial_registry.clone())
                .on_connection(ResumableReplyingHandler {
                    started,
                    release: Arc::clone(&release),
                })
                .establish_or_resume::<NoopClient>(),
        ),
        tokio::time::timeout(
            Duration::from_secs(1),
            crate::initiator(source, TransportMode::Bare).establish::<NoopClient>(),
        ),
    )
    .expect("initial session establishment timed out");

    let initial_server_caller = match server_established.expect("server handshake failed") {
        SessionAcceptOutcome::Established(client) => client,
        SessionAcceptOutcome::Resumed => panic!("first accept should establish a new session"),
    };
    let caller = client_established.expect("client handshake failed");
    let client_session_handle = caller.session.clone().unwrap();

    let call_task = moire::task::spawn(
        async move {
            caller
                .caller
                .call(RequestCall {
                    method_id: MethodId(1),
                    args: Payload::outgoing(&88_u32),
                    schemas: Default::default(),
                    metadata: Default::default(),
                })
                .await
        }
        .named("source_auto_resume_unknown_key_then_fresh"),
    );

    tokio::time::timeout(Duration::from_secs(1), started_wait)
        .await
        .expect("timed out waiting for handler start");

    client_break1.close().await;
    server_break1.close().await;

    let restarted_started = Arc::new(tokio::sync::Notify::new());
    let restarted_started_waiter = Arc::clone(&restarted_started);
    let restarted_started_wait = restarted_started_waiter.notified();
    let restarted_accept = tokio::time::timeout(
        Duration::from_secs(1),
        acceptor_on(server_link2)
            .session_registry(restarted_registry.clone())
            .on_connection(ResumableReplyingHandler {
                started: restarted_started,
                release: Arc::clone(&release),
            })
            .establish_or_resume::<NoopClient>(),
    )
    .await
    .expect("timed out waiting for fallback reconnection");
    let restarted_server_caller = match restarted_accept.expect("server accept should succeed") {
        SessionAcceptOutcome::Established(client) => client,
        SessionAcceptOutcome::Resumed => panic!("fallback should establish a fresh session"),
    };

    tokio::time::timeout(Duration::from_secs(1), restarted_started_wait)
        .await
        .expect("timed out waiting for restarted handler start");
    release.notify_waiters();

    let response = call_task
        .await
        .expect("call task join")
        .expect("call should succeed after fallback reconnection");
    let response = response.get();
    let ret_bytes = match &response.ret {
        Payload::PostcardBytes(bytes) => *bytes,
        _ => panic!("expected incoming payload in response"),
    };
    let value: u32 = vox_postcard::from_slice(ret_bytes).expect("deserialize response");
    assert_eq!(value, 88);

    drop(initial_server_caller);
    drop(restarted_server_caller);
    let _ = client_session_handle.shutdown();
    client_break2.close().await;
    server_break2.close().await;
}

#[tokio::test]
async fn resumable_session_reruns_released_idem_call_after_manual_resume() {
    let (client_link1, client_break1, server_link1, server_break1) = breakable_link_pair(64);
    let client_conduit1 = BareConduit::new(client_link1);
    let server_conduit1 = BareConduit::new(server_link1);

    let first_started = Arc::new(tokio::sync::Notify::new());
    let first_started_waiter = Arc::clone(&first_started);
    let first_started_wait = first_started_waiter.notified();
    let drop_first = Arc::new(tokio::sync::Notify::new());
    let runs = Arc::new(AtomicUsize::new(0));

    let (server_established, client_established) = tokio::try_join!(
        tokio::time::timeout(
            Duration::from_secs(1),
            acceptor_conduit(server_conduit1, test_acceptor_handshake())
                .resumable()
                .on_connection(RetryAfterResumeHandler {
                    retry: RetryPolicy::IDEM,
                    runs: Arc::clone(&runs),
                    first_started,
                    drop_first: Arc::clone(&drop_first),
                })
                .establish::<NoopClient>(),
        ),
        tokio::time::timeout(
            Duration::from_secs(1),
            initiator_conduit(client_conduit1, test_initiator_handshake())
                .resumable()
                .establish::<NoopClient>(),
        ),
    )
    .expect("initial session establishment timed out");
    let _server_caller = server_established.expect("server handshake failed");
    let server_session_handle = _server_caller.session.clone().unwrap();
    let caller = client_established.expect("client handshake failed");
    let client_session_handle = caller.session.clone().unwrap();

    let call_task = moire::task::spawn(
        async move {
            caller
                .caller
                .call(RequestCall {
                    method_id: MethodId(1),
                    args: Payload::outgoing(&77_u32),
                    schemas: Default::default(),
                    metadata: Default::default(),
                })
                .await
        }
        .named("resume_retry_idem"),
    );

    tokio::time::timeout(Duration::from_secs(1), first_started_wait)
        .await
        .expect("timed out waiting for first handler start");

    client_break1.close().await;
    server_break1.close().await;
    drop_first.notify_waiters();
    tokio::time::sleep(Duration::from_millis(25)).await;

    let (client_link2, client_break2, server_link2, server_break2) = breakable_link_pair(64);
    tokio::try_join!(
        client_session_handle.resume(BareConduit::new(client_link2), test_initiator_handshake()),
        server_session_handle.resume(BareConduit::new(server_link2), test_acceptor_handshake()),
    )
    .expect("session resume should succeed");

    let response = call_task
        .await
        .expect("call task join")
        .expect("idem call should succeed after retry");
    let response = response.get();
    let ret_bytes = match &response.ret {
        Payload::PostcardBytes(bytes) => *bytes,
        _ => panic!("expected incoming payload in response"),
    };
    let value: u32 = vox_postcard::from_slice(ret_bytes).expect("deserialize response");
    assert_eq!(value, 77);
    assert_eq!(runs.load(Ordering::SeqCst), 2);

    let _ = client_session_handle.shutdown();
    let _ = server_session_handle.shutdown();
    client_break2.close().await;
    server_break2.close().await;
}

#[tokio::test]
async fn resumable_session_returns_indeterminate_for_released_non_idem_call_after_manual_resume() {
    let (client_link1, client_break1, server_link1, server_break1) = breakable_link_pair(64);
    let client_conduit1 = BareConduit::new(client_link1);
    let server_conduit1 = BareConduit::new(server_link1);

    let first_started = Arc::new(tokio::sync::Notify::new());
    let first_started_waiter = Arc::clone(&first_started);
    let first_started_wait = first_started_waiter.notified();
    let drop_first = Arc::new(tokio::sync::Notify::new());
    let runs = Arc::new(AtomicUsize::new(0));

    let (server_established, client_established) = tokio::try_join!(
        tokio::time::timeout(
            Duration::from_secs(1),
            acceptor_conduit(server_conduit1, test_acceptor_handshake())
                .resumable()
                .on_connection(RetryAfterResumeHandler {
                    retry: RetryPolicy::VOLATILE,
                    runs: Arc::clone(&runs),
                    first_started,
                    drop_first: Arc::clone(&drop_first),
                })
                .establish::<NoopClient>(),
        ),
        tokio::time::timeout(
            Duration::from_secs(1),
            initiator_conduit(client_conduit1, test_initiator_handshake())
                .resumable()
                .establish::<NoopClient>(),
        ),
    )
    .expect("initial session establishment timed out");
    let _server_caller = server_established.expect("server handshake failed");
    let server_session_handle = _server_caller.session.clone().unwrap();
    let caller = client_established.expect("client handshake failed");
    let client_session_handle = caller.session.clone().unwrap();

    let call_task = moire::task::spawn(
        async move {
            caller
                .caller
                .call(RequestCall {
                    method_id: MethodId(1),
                    args: Payload::outgoing(&88_u32),
                    schemas: Default::default(),
                    metadata: Default::default(),
                })
                .await
        }
        .named("resume_retry_non_idem"),
    );

    tokio::time::timeout(Duration::from_secs(1), first_started_wait)
        .await
        .expect("timed out waiting for first handler start");

    client_break1.close().await;
    server_break1.close().await;
    drop_first.notify_waiters();
    tokio::time::sleep(Duration::from_millis(25)).await;

    let (client_link2, client_break2, server_link2, server_break2) = breakable_link_pair(64);
    tokio::try_join!(
        client_session_handle.resume(BareConduit::new(client_link2), test_initiator_handshake()),
        server_session_handle.resume(BareConduit::new(server_link2), test_acceptor_handshake()),
    )
    .expect("session resume should succeed");

    let response = call_task
        .await
        .expect("call task join")
        .expect("runtime should return a response envelope");
    let response = response.get();
    let ret_bytes = match &response.ret {
        Payload::PostcardBytes(bytes) => *bytes,
        _ => panic!("expected incoming payload in response"),
    };
    let result: Result<u32, VoxError> =
        vox_postcard::from_slice(ret_bytes).expect("deserialize response");
    assert!(matches!(result, Err(VoxError::Indeterminate)));
    assert_eq!(runs.load(Ordering::SeqCst), 1);

    let _ = client_session_handle.shutdown();
    let _ = server_session_handle.shutdown();
    client_break2.close().await;
    server_break2.close().await;
}

#[tokio::test]
async fn recovery_timeout_gives_up_after_deadline() {
    let (client_link1, client_break1, server_link1, server_break1) = breakable_link_pair(64);

    // Source that only has one link — after break, next_link hangs forever.
    let source = TestLinkSource::new([Attachment::initiator(client_link1)]);

    let (server_established, client_established) = tokio::try_join!(
        tokio::time::timeout(
            Duration::from_secs(1),
            acceptor_on(server_link1)
                .on_connection(EchoHandler)
                .establish::<NoopClient>(),
        ),
        tokio::time::timeout(
            Duration::from_secs(1),
            crate::initiator(source, TransportMode::Bare)
                .resumable()
                .recovery_timeout(Duration::from_millis(500))
                .connect_timeout(Duration::from_millis(200))
                .establish::<NoopClient>(),
        ),
    )
    .expect("initial establishment timed out");

    let _server = server_established.expect("server establish");
    let client = client_established.expect("client establish");

    // Make a call to verify the session works.
    let args: u32 = 42;
    let response = client
        .caller
        .call(RequestCall {
            method_id: MethodId(1),
            args: Payload::outgoing(&args),
            schemas: Default::default(),
            metadata: Default::default(),
        })
        .await
        .expect("call should succeed");
    let response = response.get();
    let ret_bytes = match &response.ret {
        Payload::PostcardBytes(bytes) => *bytes,
        _ => panic!("expected postcard bytes"),
    };
    let result: u32 = vox_postcard::from_slice(ret_bytes).expect("deserialize");
    assert_eq!(result, 42);

    // Break the link — the server is gone forever.
    client_break1.close().await;
    server_break1.close().await;

    // The recovery should give up after ~500ms.
    let start = std::time::Instant::now();
    client.caller.closed().await;
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_secs(3),
        "recovery should have given up, but took {elapsed:?}"
    );
    assert!(
        elapsed >= Duration::from_millis(400),
        "recovery gave up too quickly: {elapsed:?}"
    );
}
