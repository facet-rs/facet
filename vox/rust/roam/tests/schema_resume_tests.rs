use std::time::Duration;

use roam_core::{
    BareConduit, SessionAcceptOutcome, SessionRegistry, TransportMode, acceptor_on,
    initiate_transport, initiator_on, memory_link_pair, testing::breakable_link_pair,
};
use roam_types::{ConnectionSettings, Parity};

#[roam::service]
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

/// Basic: establish via real CBOR handshake, call through macro-generated client.
#[tokio::test]
async fn call_through_real_handshake() {
    let (client_link, server_link) = memory_link_pair(64);

    let (server_result, client_result) = tokio::try_join!(
        tokio::time::timeout(
            Duration::from_secs(1),
            acceptor_on(server_link).establish::<EchoClient>(EchoDispatcher::new(EchoService)),
        ),
        tokio::time::timeout(
            Duration::from_secs(1),
            initiator_on(client_link, TransportMode::Bare).establish::<EchoClient>(()),
        ),
    )
    .expect("session establishment timed out");

    let (_server_client, _server_handle) = server_result.expect("server failed");
    let (client, _client_handle) = client_result.expect("client failed");

    assert_eq!(client.echo(42).await.unwrap(), 42);
    assert_eq!(client.echo(0).await.unwrap(), 0);
    assert_eq!(client.echo(u32::MAX).await.unwrap(), u32::MAX);
}

/// Same through stable conduit.
#[tokio::test]
async fn call_through_stable_conduit() {
    let (client_link, server_link) = memory_link_pair(64);

    let (server_result, client_result) = tokio::try_join!(
        tokio::time::timeout(
            Duration::from_secs(1),
            acceptor_on(server_link).establish::<EchoClient>(EchoDispatcher::new(EchoService)),
        ),
        tokio::time::timeout(
            Duration::from_secs(1),
            initiator_on(client_link, TransportMode::Stable).establish::<EchoClient>(()),
        ),
    )
    .expect("session establishment timed out");

    let (_server_client, _server_handle) = server_result.expect("server failed");
    let (client, _client_handle) = client_result.expect("client failed");

    assert_eq!(client.echo(42).await.unwrap(), 42);
    assert_eq!(client.echo(99).await.unwrap(), 99);
}

/// Multiple methods on the same service, each gets independent schemas.
#[roam::service]
trait MultiMethod {
    async fn add(&self, a: u32, b: u32) -> u32;
    async fn mul(&self, a: u32, b: u32) -> u32;
}

#[derive(Clone)]
struct MultiMethodService;

impl MultiMethod for MultiMethodService {
    async fn add(&self, a: u32, b: u32) -> u32 {
        a + b
    }
    async fn mul(&self, a: u32, b: u32) -> u32 {
        a * b
    }
}

#[tokio::test]
async fn multiple_methods_get_independent_schemas() {
    let (client_link, server_link) = memory_link_pair(64);

    let (server_result, client_result) = tokio::try_join!(
        tokio::time::timeout(
            Duration::from_secs(1),
            acceptor_on(server_link)
                .establish::<MultiMethodClient>(MultiMethodDispatcher::new(MultiMethodService)),
        ),
        tokio::time::timeout(
            Duration::from_secs(1),
            initiator_on(client_link, TransportMode::Bare).establish::<MultiMethodClient>(()),
        ),
    )
    .expect("session establishment timed out");

    let (_server_client, _server_handle) = server_result.expect("server failed");
    let (client, _client_handle) = client_result.expect("client failed");

    assert_eq!(client.add(3, 5).await.unwrap(), 8);
    assert_eq!(client.mul(3, 5).await.unwrap(), 15);
    assert_eq!(client.add(100, 200).await.unwrap(), 300);
    assert_eq!(client.mul(10, 10).await.unwrap(), 100);
}

/// After resume, calls should work — schemas re-sent on the new connection.
#[tokio::test]
async fn call_works_after_resume() {
    let registry = SessionRegistry::default();

    // First connection
    let (client_link1, client_break1, server_link1, server_break1) = breakable_link_pair(64);
    let (server_result, client_result) = tokio::try_join!(
        tokio::time::timeout(
            Duration::from_secs(1),
            acceptor_on(server_link1)
                .session_registry(registry.clone())
                .establish_or_resume::<EchoClient>(EchoDispatcher::new(EchoService)),
        ),
        tokio::time::timeout(
            Duration::from_secs(1),
            initiator_on(client_link1, TransportMode::Bare)
                .resumable()
                .establish::<EchoClient>(()),
        ),
    )
    .expect("session establishment timed out");

    let (_server_client, _server_handle) = match server_result.expect("server failed") {
        SessionAcceptOutcome::Established(c, h) => (c, h),
        _ => panic!("expected Established"),
    };
    let (client, client_session_handle) = client_result.expect("client failed");

    // Call succeeds on the initial connection
    assert_eq!(client.echo(1).await.unwrap(), 1);

    // Break the transport
    client_break1.close().await;
    server_break1.close().await;
    tokio::time::sleep(Duration::from_millis(25)).await;

    // Resume: new links, new handshake
    let (client_link2, _client_break2, server_link2, _server_break2) = breakable_link_pair(64);
    let (resume_result, server_accept_result) = tokio::join!(
        async {
            let mut resumed_link = initiate_transport(client_link2, TransportMode::Bare)
                .await
                .expect("transport");
            let handshake_result = roam_core::handshake_as_initiator(
                &resumed_link.tx,
                &mut resumed_link.rx,
                ConnectionSettings {
                    parity: Parity::Odd,
                    max_concurrent_requests: 64,
                },
                true,
                client_session_handle.resume_key(),
            )
            .await
            .expect("handshake");
            let message_plan =
                roam_core::MessagePlan::from_handshake(&handshake_result).expect("message plan");
            client_session_handle
                .resume(
                    BareConduit::with_message_plan(resumed_link, message_plan),
                    handshake_result,
                )
                .await
        },
        tokio::time::timeout(
            Duration::from_secs(1),
            acceptor_on(server_link2)
                .session_registry(registry.clone())
                .establish_or_resume::<EchoClient>(EchoDispatcher::new(EchoService)),
        ),
    );
    resume_result.expect("resume should succeed");
    let server_accept_result = server_accept_result.expect("server accept timed out");
    match server_accept_result.expect("server accept failed") {
        SessionAcceptOutcome::Resumed => {}
        _ => panic!("expected Resumed"),
    }

    // Call after resume — schemas must be re-sent on both sides
    assert_eq!(client.echo(99).await.unwrap(), 99);
}

/// Break before any calls, resume, then make the first call ever on the
/// resumed connection — schemas must be sent fresh.
#[tokio::test]
async fn first_call_after_resume_without_prior_calls() {
    let registry = SessionRegistry::default();

    // First connection — no calls
    let (client_link1, client_break1, server_link1, server_break1) = breakable_link_pair(64);
    let (server_result, client_result) = tokio::try_join!(
        tokio::time::timeout(
            Duration::from_secs(1),
            acceptor_on(server_link1)
                .session_registry(registry.clone())
                .establish_or_resume::<EchoClient>(EchoDispatcher::new(EchoService)),
        ),
        tokio::time::timeout(
            Duration::from_secs(1),
            initiator_on(client_link1, TransportMode::Bare)
                .resumable()
                .establish::<EchoClient>(()),
        ),
    )
    .expect("session establishment timed out");

    let (_server_client, _server_handle) = match server_result.expect("server failed") {
        SessionAcceptOutcome::Established(c, h) => (c, h),
        _ => panic!("expected Established"),
    };
    let (client, client_session_handle) = client_result.expect("client failed");

    // Break immediately — no calls on the first connection
    client_break1.close().await;
    server_break1.close().await;
    tokio::time::sleep(Duration::from_millis(25)).await;

    // Resume
    let (client_link2, _client_break2, server_link2, _server_break2) = breakable_link_pair(64);
    let (resume_result, server_accept_result) = tokio::join!(
        async {
            let mut resumed_link = initiate_transport(client_link2, TransportMode::Bare)
                .await
                .expect("transport");
            let handshake_result = roam_core::handshake_as_initiator(
                &resumed_link.tx,
                &mut resumed_link.rx,
                ConnectionSettings {
                    parity: Parity::Odd,
                    max_concurrent_requests: 64,
                },
                true,
                client_session_handle.resume_key(),
            )
            .await
            .expect("handshake");
            let message_plan =
                roam_core::MessagePlan::from_handshake(&handshake_result).expect("message plan");
            client_session_handle
                .resume(
                    BareConduit::with_message_plan(resumed_link, message_plan),
                    handshake_result,
                )
                .await
        },
        tokio::time::timeout(
            Duration::from_secs(1),
            acceptor_on(server_link2)
                .session_registry(registry.clone())
                .establish_or_resume::<EchoClient>(EchoDispatcher::new(EchoService)),
        ),
    );
    resume_result.expect("resume should succeed");
    let server_accept_result = server_accept_result.expect("server accept timed out");
    match server_accept_result.expect("server accept failed") {
        SessionAcceptOutcome::Resumed => {}
        _ => panic!("expected Resumed"),
    }

    // Very first call happens after resume
    assert_eq!(client.echo(77).await.unwrap(), 77);
}
