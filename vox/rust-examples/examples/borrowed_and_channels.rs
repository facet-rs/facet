use std::convert::Infallible;

use eyre::{Result, WrapErr, eyre};
use vox::transport::tcp::StreamLink;
use vox::{Call, Rx, Tx, channel};

#[vox::service]
trait WordLab {
    // Borrowed arg.
    async fn is_short(&self, word: &str) -> bool;

    // Borrowed return.
    async fn classify(&self, word: String) -> &'vox str;

    // Borrowed arg + bidirectional channels.
    async fn transform(&self, prefix: &str, input: Rx<String>, output: Tx<String>) -> u32;
}

#[derive(Clone)]
struct WordLabService;

impl WordLab for WordLabService {
    async fn is_short(&self, word: &str) -> bool {
        word.len() <= 4
    }

    async fn classify<'vox>(&self, call: impl Call<'vox, &'vox str, Infallible>, word: String) {
        let label = if word.len() <= 4 { "short" } else { "long" };
        call.ok(label).await;
    }

    async fn transform(&self, prefix: &str, mut input: Rx<String>, output: Tx<String>) -> u32 {
        let mut count = 0;
        while let Ok(Some(item)) = input.recv().await {
            if output
                .send(format!("{prefix}:{}", item.as_str()))
                .await
                .is_err()
            {
                break;
            }
            count += 1;
        }
        let _ = output.close(Default::default()).await;
        count
    }
}

fn main() -> Result<()> {
    println!("[demo] borrowed_and_channels: starting runtime");
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .wrap_err("building Tokio runtime")?;
    rt.block_on(run_demo())
}

async fn run_demo() -> Result<()> {
    println!("[demo] binding TCP listener");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .wrap_err("binding TCP listener")?;
    let addr = listener.local_addr().wrap_err("reading listener addr")?;
    println!("[demo] listening on {addr}");

    let server_task = tokio::spawn(async move {
        println!("[server] waiting for client");
        let (socket, _) = listener.accept().await.expect("accept");
        println!("[server] client connected; establishing session");
        let (server_caller_guard, _) = vox::acceptor_on(StreamLink::tcp(socket))
            .establish::<WordLabClient>(WordLabDispatcher::new(WordLabService))
            .await
            .expect("server establish");
        let _server_caller_guard = server_caller_guard;
        std::future::pending::<()>().await;
    });

    println!("[client] connecting");
    let socket = tokio::net::TcpStream::connect(addr)
        .await
        .wrap_err("connecting client socket")?;
    let (client, _) = vox::initiator_on(StreamLink::tcp(socket), vox::TransportMode::Bare)
        .establish::<WordLabClient>(())
        .await
        .map_err(|e| eyre!("failed to establish initiator session: {e:?}"))?;
    println!("[client] session established");

    println!("[client] calling is_short");
    assert!(
        client
            .is_short("pear")
            .await
            .map_err(|e| eyre!("is_short(\"pear\") failed: {e:?}"))?
    );
    assert!(
        !client
            .is_short("watermelon")
            .await
            .map_err(|e| eyre!("is_short(\"watermelon\") failed: {e:?}"))?
    );
    println!("[client] is_short checks passed");

    println!("[client] calling classify");
    let short = client
        .classify("pear".to_string())
        .await
        .map_err(|e| eyre!("classify(\"pear\") failed: {e:?}"))?;
    let long = client
        .classify("watermelon".to_string())
        .await
        .map_err(|e| eyre!("classify(\"watermelon\") failed: {e:?}"))?;
    assert_eq!(*short, "short");
    assert_eq!(*long, "long");
    println!("[client] classify returned short={} long={}", *short, *long);

    let (input_tx, input_rx) = channel::<String>();
    let (output_tx, mut output_rx) = channel::<String>();
    println!("[client] created transform channels");

    let send_task = tokio::spawn(async move {
        for word in ["one", "two", "three"] {
            println!("[client/send] -> {word}");
            input_tx
                .send(word.to_string())
                .await
                .expect("send to input");
        }
        println!("[client/send] closing input");
        input_tx
            .close(Default::default())
            .await
            .expect("close input channel");
    });

    println!("[client] calling transform");
    let count = client
        .transform("item", input_rx, output_tx)
        .await
        .map_err(|e| eyre!("transform(...) failed: {e:?}"))?;
    assert_eq!(count, 3);
    println!("[client] transform returned count={count}");
    send_task.await.wrap_err("joining send_task")?;

    let mut got = Vec::new();
    while let Some(item) = output_rx
        .recv()
        .await
        .wrap_err("receiving from output_rx")?
    {
        println!("[client/recv] <- {}", item.as_str());
        got.push(item.to_string());
    }
    assert_eq!(got, vec!["item:one", "item:two", "item:three"]);
    println!("[client] output stream complete: {got:?}");

    // The demo is complete; stop background loops.
    server_task.abort();
    println!("[demo] borrowed_and_channels: complete");

    Ok(())
}
