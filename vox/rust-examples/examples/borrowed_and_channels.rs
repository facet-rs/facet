use std::convert::Infallible;

use eyre::{Result, WrapErr, eyre};
use roam::{Call, Rx, Tx, channel};
use roam_stream::StreamLink;

#[roam::service]
trait WordLab {
    // Borrowed arg.
    async fn is_short(&self, word: &str) -> bool;

    // Borrowed return.
    async fn classify(&self, word: String) -> &'roam str;

    // Borrowed arg + bidirectional channels.
    async fn transform(&self, prefix: &str, input: Rx<String>, output: Tx<String>) -> u32;
}

#[derive(Clone)]
struct WordLabService;

impl WordLab for WordLabService {
    async fn is_short(&self, word: &str) -> bool {
        word.len() <= 4
    }

    async fn classify<'roam>(&self, call: impl Call<'roam, &'roam str, Infallible>, word: String) {
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
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .wrap_err("building Tokio runtime")?;
    rt.block_on(run_demo())
}

async fn run_demo() -> Result<()> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .wrap_err("binding TCP listener")?;
    let addr = listener.local_addr().wrap_err("reading listener addr")?;

    let server_task = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.expect("accept");
        let conduit = roam::BareConduit::<roam::MessageFamily, _>::new(StreamLink::tcp(socket));

        let (mut session, handle, _) = roam::acceptor(conduit)
            .establish()
            .await
            .expect("server establish");
        let mut driver = roam::Driver::new(
            handle,
            WordLabDispatcher::new(WordLabService),
            roam::Parity::Even,
        );

        tokio::spawn(async move {
            session.run().await;
        });
        driver.run().await;
    });

    let socket = tokio::net::TcpStream::connect(addr)
        .await
        .wrap_err("connecting client socket")?;
    let conduit = roam::BareConduit::<roam::MessageFamily, _>::new(StreamLink::tcp(socket));

    let (mut session, handle, _) = roam::initiator(conduit)
        .establish()
        .await
        .map_err(|e| eyre!("failed to establish initiator session: {e:?}"))?;
    let mut driver = roam::Driver::new(handle, (), roam::Parity::Odd);
    let caller = driver.caller();

    let client_session_task = tokio::spawn(async move {
        session.run().await;
    });
    let client_driver_task = tokio::spawn(async move {
        driver.run().await;
    });

    let client = WordLabClient::new(caller);

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

    let (input_tx, input_rx) = channel::<String>();
    let (output_tx, mut output_rx) = channel::<String>();

    let send_task = tokio::spawn(async move {
        for word in ["one", "two", "three"] {
            input_tx
                .send(word.to_string())
                .await
                .expect("send to input");
        }
        input_tx
            .close(Default::default())
            .await
            .expect("close input channel");
    });

    let count = client
        .transform("item", input_rx, output_tx)
        .await
        .map_err(|e| eyre!("transform(...) failed: {e:?}"))?;
    assert_eq!(count, 3);
    send_task.await.wrap_err("joining send_task")?;

    let mut got = Vec::new();
    while let Some(item) = output_rx
        .recv()
        .await
        .wrap_err("receiving from output_rx")?
    {
        got.push(item.to_string());
    }
    assert_eq!(got, vec!["item:one", "item:two", "item:three"]);

    // The demo is complete; stop background loops.
    client_driver_task.abort();
    client_session_task.abort();
    server_task.abort();

    Ok(())
}
