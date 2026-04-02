//! Tests for different transport modes (bare, stable, CBOR handshake).

use vox::memory_link_pair;

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

#[tokio::test]
async fn call_through_cbor_handshake_reaches_handler() {
    let (client_link, server_link) = memory_link_pair(16);

    let server = tokio::spawn(async move {
        let s = vox::acceptor_on(server_link)
            .on_connection(EchoDispatcher::new(EchoService).establish::<vox::NoopClient>())
            .await
            .expect("server establish");
        s
    });

    let client = vox::initiator_on(client_link, vox::TransportMode::Bare)
        .establish::<EchoClient>()
        .await
        .expect("client establish");

    let _server_guard = server.await.expect("server task");

    let result = client.echo(42).await.expect("echo call");
    assert_eq!(result, 42);
}

#[tokio::test]
async fn call_through_stable_conduit_reaches_handler() {
    let (client_link, server_link) = memory_link_pair(16);

    let server = tokio::spawn(async move {
        let s = vox::acceptor_on(server_link)
            .on_connection(EchoDispatcher::new(EchoService).establish::<vox::NoopClient>())
            .await
            .expect("server establish");
        s
    });

    let client = vox::initiator_on(client_link, vox::TransportMode::Stable)
        .establish::<EchoClient>()
        .await
        .expect("client establish");

    let _server_guard = server.await.expect("server task");

    let result = client.echo(42).await.expect("echo call via stable");
    assert_eq!(result, 42);
}

#[tokio::test]
async fn multiple_calls_through_stable_conduit() {
    let (client_link, server_link) = memory_link_pair(16);

    let server = tokio::spawn(async move {
        let s = vox::acceptor_on(server_link)
            .on_connection(EchoDispatcher::new(EchoService).establish::<vox::NoopClient>())
            .await
            .expect("server establish");
        s
    });

    let client = vox::initiator_on(client_link, vox::TransportMode::Stable)
        .establish::<EchoClient>()
        .await
        .expect("client establish");

    let _server_guard = server.await.expect("server task");

    for i in 0..10 {
        let result = client.echo(i).await.expect("echo call");
        assert_eq!(result, i);
    }
}
