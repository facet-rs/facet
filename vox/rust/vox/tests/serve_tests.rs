//! Tests for vox::serve().

use std::time::Duration;

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
async fn serve_single_service() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("local_addr");

    // Start server in background — we pass the listener's port to connect().
    let server = tokio::spawn(async move {
        // We can't use vox::serve() directly since it binds its own listener.
        // Instead, test the same pattern manually.
        loop {
            let (stream, _) = listener.accept().await.expect("accept");
            let link = vox::transport::tcp::StreamLink::tcp(stream);
            tokio::spawn(async move {
                let client = vox::acceptor_on(link)
                    .on_connection(EchoDispatcher::new(EchoService))
                    .establish::<vox::NoopClient>()
                    .await
                    .expect("server establish");
                client.caller.closed().await;
            });
        }
    });

    // Give server a moment to start accepting.
    tokio::time::sleep(Duration::from_millis(50)).await;

    let client: EchoClient = vox::connect(format!("tcp://{addr}"))
        .await
        .expect("connect");
    let result = client.echo(42).await.expect("echo");
    assert_eq!(result, 42);

    server.abort();
}
