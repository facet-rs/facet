use eyre::Result;
use vox::transport::tcp::StreamLink;

#[vox::service]
trait Hello {
    async fn say_hello(&self, name: String) -> String;
}

#[derive(Clone)]
struct HelloService;

impl Hello for HelloService {
    async fn say_hello(&self, name: String) -> String {
        format!("Hello, {name}!")
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;

    // Server: accept one connection using the existing lower-level API
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await?;
        let (caller, _session) = vox::acceptor_on(StreamLink::tcp(stream))
            .establish::<HelloClient>(HelloDispatcher::new(HelloService))
            .await?;
        vox::closed(&caller).await;

        Ok::<(), eyre::Report>(())
    });

    // Client: the new one-liner
    let client: HelloClient = vox::connect(addr).await?;
    let reply = client.say_hello("world".into()).await?;
    assert_eq!(reply, "Hello, world!");
    println!("{reply}");

    drop(client);
    server.await??;
    Ok(())
}
