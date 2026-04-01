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

async fn run_server(listener: tokio::net::TcpListener) -> Result<()> {
    let (stream, _) = listener.accept().await?;
    let (_caller, _session) = vox::acceptor_on(StreamLink::tcp(stream))
        .establish::<vox::DriverCaller>(HelloDispatcher::new(HelloService))
        .await?;
    vox::closed(&_caller).await;
    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    // Start server on a fixed port so we can restart it
    let addr = "127.0.0.1:19876";
    let listener = tokio::net::TcpListener::bind(addr).await?;

    let server = tokio::spawn(async move {
        if let Err(e) = run_server(listener).await {
            eprintln!("[server] error: {e}");
        }
        eprintln!("[server] first instance exited");
    });

    // Connect client
    let client: HelloClient = vox::connect(addr).await?;

    // First call — should succeed
    let reply = client.say_hello("world".into()).await?;
    println!("[client] first call: {reply}");

    // Kill the server
    server.abort();
    let _ = server.await;
    println!("[client] server killed");

    // Give it a moment, then restart the server
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("[client] server restarted");

    let _server2 = tokio::spawn(async move {
        if let Err(e) = run_server(listener).await {
            eprintln!("[server2] error: {e}");
        }
    });

    // Second call — will this succeed?
    match client.say_hello("again".into()).await {
        Ok(reply) => println!("[client] second call succeeded: {reply}"),
        Err(e) => println!("[client] second call failed: {e}"),
    }

    Ok(())
}
