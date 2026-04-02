//! Tests for vox::serve().

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
async fn serve_and_connect() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("local_addr");

    let server = tokio::spawn(async move {
        vox::serve_listener(listener, EchoDispatcher::new(EchoService))
            .await
            .expect("serve");
    });

    // Give server a moment to start accepting.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let client: EchoClient = vox::connect(format!("tcp://{addr}"))
        .await
        .expect("connect");
    let result = client.echo(42).await.expect("echo");
    assert_eq!(result, 42);

    server.abort();
}
