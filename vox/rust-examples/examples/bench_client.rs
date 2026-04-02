use std::time::Instant;

#[vox::service]
trait Testbed {
    async fn echo(&self, message: String) -> String;
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> eyre::Result<()> {
    let count: usize = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000);

    let addr = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "local:///tmp/bench.vox".to_string());

    tracing_subscriber::fmt::init();

    eprintln!("serving on {addr}, waiting for peer to connect...");

    vox::serve(
        &addr,
        vox::acceptor_fn(move |req, conn| {
            let _ = req.service();
            let client: TestbedClient = conn.handle_with_client(());
            tokio::spawn(async move {
                eprintln!("session established, running {count} echo calls...");
                let start = Instant::now();
                for i in 0..count {
                    let resp = client.echo(format!("hello {i}")).await.unwrap();
                    std::hint::black_box(resp);
                }
                let elapsed = start.elapsed();
                let per_call = elapsed / count as u32;
                eprintln!(
                    "{count} calls in {:.2}s — {per_call:?}/call — {:.0} calls/sec",
                    elapsed.as_secs_f64(),
                    count as f64 / elapsed.as_secs_f64()
                );
                std::process::exit(0);
            });
            Ok(())
        }),
    )
    .await?;

    Ok(())
}
