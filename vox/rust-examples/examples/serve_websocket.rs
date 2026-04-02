use eyre::Result;

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

    // Use port 0 via the lower-level WsListener for dynamic port allocation.
    let listener = vox::WsListener::bind("127.0.0.1:0").await?;
    let addr = format!("ws://{}", listener.local_addr()?);

    let server = tokio::spawn(async move {
        vox::serve_listener(listener, HelloDispatcher::new(HelloService))
            .await
            .unwrap();
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let client: HelloClient = vox::connect(&addr).await?;
    let reply = client.say_hello("websocket".into()).await?;
    println!("{reply}");

    drop(client);
    server.abort();
    Ok(())
}
