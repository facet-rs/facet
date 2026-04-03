use std::time::Instant;
use std::{hint::black_box, sync::Arc};

use tokio::task::JoinSet;

#[vox::service]
trait Testbed {
    async fn echo(&self, message: String) -> String;
}

#[derive(Debug, Clone)]
struct Config {
    count: usize,
    addr: String,
    payload_sizes: Vec<usize>,
    in_flights: Vec<usize>,
    json: bool,
}

#[derive(Debug, Clone)]
struct BenchResult {
    transport: String,
    addr: String,
    count: usize,
    payload_size: usize,
    in_flight: usize,
    elapsed_secs: f64,
    per_call_micros: f64,
    calls_per_sec: f64,
}

fn parse_usize_csv(value: &str, flag: &str) -> eyre::Result<Vec<usize>> {
    let mut out = Vec::new();
    for part in value.split(',') {
        let s = part.trim();
        if s.is_empty() {
            continue;
        }
        let parsed = s
            .parse::<usize>()
            .map_err(|e| eyre::eyre!("invalid {flag} value '{s}': {e}"))?;
        out.push(parsed);
    }
    if out.is_empty() {
        return Err(eyre::eyre!("no values provided for {flag}"));
    }
    Ok(out)
}

fn parse_config() -> eyre::Result<Config> {
    let mut count: usize = 10_000;
    let mut addr = "local:///tmp/bench.vox".to_string();
    let mut payload_sizes: Vec<usize> = vec![16];
    let mut in_flights: Vec<usize> = vec![1];
    let mut json = false;

    let mut positionals: Vec<String> = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--count" => {
                let v = args
                    .next()
                    .ok_or_else(|| eyre::eyre!("missing value for --count"))?;
                count = v
                    .parse::<usize>()
                    .map_err(|e| eyre::eyre!("invalid --count value '{v}': {e}"))?;
            }
            "--addr" => {
                addr = args
                    .next()
                    .ok_or_else(|| eyre::eyre!("missing value for --addr"))?;
            }
            "--payload-size" => {
                let v = args
                    .next()
                    .ok_or_else(|| eyre::eyre!("missing value for --payload-size"))?;
                payload_sizes.push(
                    v.parse::<usize>()
                        .map_err(|e| eyre::eyre!("invalid --payload-size value '{v}': {e}"))?,
                );
            }
            "--payload-sizes" => {
                let v = args
                    .next()
                    .ok_or_else(|| eyre::eyre!("missing value for --payload-sizes"))?;
                payload_sizes = parse_usize_csv(&v, "--payload-sizes")?;
            }
            "--in-flight" => {
                let v = args
                    .next()
                    .ok_or_else(|| eyre::eyre!("missing value for --in-flight"))?;
                in_flights.push(
                    v.parse::<usize>()
                        .map_err(|e| eyre::eyre!("invalid --in-flight value '{v}': {e}"))?,
                );
            }
            "--in-flights" => {
                let v = args
                    .next()
                    .ok_or_else(|| eyre::eyre!("missing value for --in-flights"))?;
                in_flights = parse_usize_csv(&v, "--in-flights")?;
            }
            "--json" => {
                json = true;
            }
            _ if arg.starts_with("--") => {
                return Err(eyre::eyre!("unknown flag: {arg}"));
            }
            _ => {
                positionals.push(arg);
            }
        }
    }

    if let Some(pos_count) = positionals.first() {
        count = pos_count
            .parse::<usize>()
            .map_err(|e| eyre::eyre!("invalid positional count '{pos_count}': {e}"))?;
    }
    if let Some(pos_addr) = positionals.get(1) {
        addr = pos_addr.clone();
    }

    payload_sizes.sort_unstable();
    payload_sizes.dedup();
    in_flights.sort_unstable();
    in_flights.dedup();

    if payload_sizes.iter().any(|&n| n == 0) {
        return Err(eyre::eyre!("payload sizes must be > 0"));
    }
    if in_flights.iter().any(|&n| n == 0) {
        return Err(eyre::eyre!("in-flight values must be > 0"));
    }
    if count == 0 {
        return Err(eyre::eyre!("count must be > 0"));
    }

    Ok(Config {
        count,
        addr,
        payload_sizes,
        in_flights,
        json,
    })
}

fn make_payload(size: usize, seq: usize) -> String {
    let mut bytes = vec![b'x'; size];
    if size >= 16 {
        let tag = format!("{seq:016x}");
        bytes[0..16].copy_from_slice(tag.as_bytes());
    }
    String::from_utf8(bytes).unwrap()
}

fn transport_from_addr(addr: &str) -> String {
    addr.split("://").next().unwrap_or("unknown").to_string()
}

async fn run_case(
    client: Arc<TestbedClient>,
    count: usize,
    payload_size: usize,
    in_flight: usize,
) -> eyre::Result<(std::time::Duration, f64)> {
    let start = Instant::now();
    if in_flight == 1 {
        for i in 0..count {
            let resp = client.echo(make_payload(payload_size, i)).await?;
            black_box(resp);
        }
    } else {
        let mut launched = 0usize;
        let mut completed = 0usize;
        let mut joins: JoinSet<eyre::Result<String>> = JoinSet::new();

        while completed < count {
            while launched < count && joins.len() < in_flight {
                let c = Arc::clone(&client);
                let msg = make_payload(payload_size, launched);
                joins.spawn(async move { Ok(c.echo(msg).await?) });
                launched += 1;
            }

            if let Some(joined) = joins.join_next().await {
                let resp = joined??;
                black_box(resp);
                completed += 1;
            }
        }
    }
    let elapsed = start.elapsed();
    let calls_per_sec = count as f64 / elapsed.as_secs_f64();
    Ok((elapsed, calls_per_sec))
}

fn print_json(results: &[BenchResult]) {
    println!("[");
    for (i, r) in results.iter().enumerate() {
        let comma = if i + 1 == results.len() { "" } else { "," };
        println!(
            "  {{\"transport\":\"{}\",\"addr\":\"{}\",\"count\":{},\"payload_size\":{},\"in_flight\":{},\"elapsed_secs\":{:.6},\"per_call_micros\":{:.3},\"calls_per_sec\":{:.3}}}{}",
            r.transport,
            r.addr,
            r.count,
            r.payload_size,
            r.in_flight,
            r.elapsed_secs,
            r.per_call_micros,
            r.calls_per_sec,
            comma
        );
    }
    println!("]");
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> eyre::Result<()> {
    let cfg = parse_config()?;

    tracing_subscriber::fmt::init();

    let serve_addr = cfg.addr.clone();
    eprintln!("serving on {}, waiting for peer to connect...", serve_addr);
    eprintln!(
        "plan: count={}, payload_sizes={:?}, in_flights={:?}",
        cfg.count, cfg.payload_sizes, cfg.in_flights
    );

    vox::serve(
        &serve_addr,
        vox::acceptor_fn(move |req, conn| {
            let _ = req.service();
            let client: Arc<TestbedClient> = Arc::new(conn.handle_with_client(()));
            let cfg = cfg.clone();
            tokio::spawn(async move {
                let mut results = Vec::<BenchResult>::new();
                eprintln!("session established, running benchmark matrix...");

                for &payload_size in &cfg.payload_sizes {
                    for &in_flight in &cfg.in_flights {
                        let outcome =
                            run_case(Arc::clone(&client), cfg.count, payload_size, in_flight).await;
                        let (elapsed, calls_per_sec) = match outcome {
                            Ok(v) => v,
                            Err(err) => {
                                eprintln!(
                                    "transport={} payload={}B in_flight={} count={} ERROR: {}",
                                    transport_from_addr(&cfg.addr),
                                    payload_size,
                                    in_flight,
                                    cfg.count,
                                    err
                                );
                                continue;
                            }
                        };
                        let per_call_micros = elapsed.as_secs_f64() * 1_000_000.0 / cfg.count as f64;
                        eprintln!(
                            "transport={} payload={}B in_flight={} count={} elapsed={:.2}s per_call={:.3}us calls_per_sec={:.0}",
                            transport_from_addr(&cfg.addr),
                            payload_size,
                            in_flight,
                            cfg.count,
                            elapsed.as_secs_f64(),
                            per_call_micros,
                            calls_per_sec
                        );
                        results.push(BenchResult {
                            transport: transport_from_addr(&cfg.addr),
                            addr: cfg.addr.clone(),
                            count: cfg.count,
                            payload_size,
                            in_flight,
                            elapsed_secs: elapsed.as_secs_f64(),
                            per_call_micros,
                            calls_per_sec,
                        });
                    }
                }

                if cfg.json {
                    print_json(&results);
                }

                std::process::exit(0);
            });
            Ok(())
        }),
    )
    .await?;

    Ok(())
}
