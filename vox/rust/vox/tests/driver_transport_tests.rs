//! Tests for transport prologue plus bare conduit session setup.

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
// r[verify transport.prologue.first-payload]
// r[verify transport.prologue.post-accept]
async fn call_through_phon_handshake_reaches_handler() {
    let (client_link, server_link) = memory_link_pair(16);

    let server = tokio::spawn(async move {
        vox::acceptor_on(server_link)
            .on_connection(EchoDispatcher::new(EchoService))
            .establish::<vox::NoopClient>()
            .await
            .expect("server establish")
    });

    let client = vox::initiator_on(client_link)
        .establish::<EchoClient>()
        .await
        .expect("client establish");

    let _server_guard = server.await.expect("server task");

    let result = client.echo(42).await.expect("echo call");
    assert_eq!(result, 42);
}
