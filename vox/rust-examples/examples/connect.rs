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

    // Server
    let server = tokio::spawn(async {
        vox::serve("127.0.0.1:9000", HelloDispatcher::new(HelloService))
            .await
            .unwrap();
    });

    // Give server a moment to bind.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Client
    let client: HelloClient = vox::connect_lane("127.0.0.1:9000").await?;
    let reply = client.say_hello("world".into()).await?;
    println!("{reply}");

    client
        .connection
        .as_ref()
        .expect("generated client carries connection handle")
        .shutdown()?;
    server.abort();
    Ok(())
}
