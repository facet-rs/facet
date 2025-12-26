use std::time::Duration;

use rapace::AnyTransport;
use rapace::Streaming;
use rapace_browser_tests_proto::{
    BrowserDemo, BrowserDemoServer, CountEvent, NumbersRequest, NumbersSummary, PhraseRequest,
    PhraseResponse,
};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::error;

struct BrowserDemoImpl;

impl BrowserDemo for BrowserDemoImpl {
    async fn summarize_numbers(&self, input: NumbersRequest) -> NumbersSummary {
        println!("summarize_numbers request: {:?}", input.values);
        let sum: i64 = input.values.iter().map(|&v| v as i64).sum();
        let mean = if input.values.is_empty() {
            0.0
        } else {
            sum as f64 / input.values.len() as f64
        };
        let min = input.values.iter().copied().min().unwrap_or(0);
        let max = input.values.iter().copied().max().unwrap_or(0);

        NumbersSummary {
            sum,
            mean,
            min,
            max,
        }
    }

    async fn transform_phrase(&self, request: PhraseRequest) -> PhraseResponse {
        println!("transform_phrase request: {}", request.phrase);
        let title = request
            .phrase
            .split_whitespace()
            .map(to_title_case)
            .collect::<Vec<_>>()
            .join(" ");

        let title = if request.shout {
            title.to_uppercase()
        } else {
            title
        };

        PhraseResponse {
            title,
            original_len: request.phrase.chars().count() as u32,
        }
    }

    async fn countdown(&self, start: u32) -> Streaming<CountEvent> {
        let (tx, rx) = mpsc::channel(8);
        tokio::spawn(async move {
            for value in (0..=start).rev() {
                if tx
                    .send(Ok(CountEvent {
                        value,
                        remaining: start - value,
                    }))
                    .await
                    .is_err()
                {
                    break;
                }

                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        });

        Box::pin(ReceiverStream::new(rx))
    }
}

fn to_title_case(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => {
            let rest: String = chars
                .as_str()
                .chars()
                .flat_map(|c| c.to_lowercase())
                .collect();
            format!("{}{}", first.to_uppercase().collect::<String>(), rest)
        }
        None => String::new(),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async_main())
}

async fn async_main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_target(false)
        .init();

    let port = std::env::var("RAPACE_BROWSER_WS_PORT")
        .ok()
        .and_then(|raw| raw.parse::<u16>().ok())
        .unwrap_or(4788);
    let addr = format!("127.0.0.1:{port}");

    let listener = TcpListener::bind(&addr).await?;

    println!(
        "rapace-browser-tests-server ready on ws://{} (methods: {})",
        addr,
        rapace_browser_tests_proto::BROWSER_DEMO_METHOD_ID_SUMMARIZE_NUMBERS
    );

    loop {
        let (stream, peer_addr) = listener.accept().await?;
        tokio::spawn(async move {
            match tokio_tungstenite::accept_async(stream).await {
                Ok(ws_stream) => {
                    let transport = AnyTransport::websocket(ws_stream);
                    let server = BrowserDemoServer::new(BrowserDemoImpl);

                    if let Err(err) = server.serve(transport).await {
                        error!(?err, "connection from {} closed with error", peer_addr);
                    }
                }
                Err(err) => {
                    error!(?err, "WebSocket handshake failed for {}", peer_addr);
                }
            }
        });
    }
}
