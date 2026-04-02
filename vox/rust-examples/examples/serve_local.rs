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

    let dir = tempfile::tempdir()?;
    let sock = dir.path().join("hello.sock");
    let addr = format!("local://{}", sock.display());

    let server = {
        let addr = addr.clone();
        tokio::spawn(async move {
            vox::serve(&addr, HelloDispatcher::new(HelloService))
                .await
                .unwrap();
        })
    };

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let client: HelloClient = vox::connect(&addr).await?;
    let reply = client.say_hello("unix socket".into()).await?;
    println!("{reply}");

    drop(client);
    server.abort();
    Ok(())
}
