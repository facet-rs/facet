use std::hint::black_box;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eyre::{Context as _, Result};
use htrace::Histogram;
use spec_proto::{
    Color, GnarlyAttr, GnarlyEntry, GnarlyKind, GnarlyPayload, Shape, TestbedClient,
    TestbedDispatcher,
};
use subject_rust::TestbedService;
use tokio::task::JoinSet;
use vox_ffi::declare_link_endpoint;

declare_link_endpoint!(mod ffi_bench_client { export = vox_ffi_bench_client_v1; });
declare_link_endpoint!(mod ffi_bench_server { export = vox_ffi_bench_server_v1; });

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Workload {
    Echo,
    Canvas,
    Gnarly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DriveMode {
    Closed,
    Open,
}

#[derive(Debug, Clone)]
struct Config {
    count: Option<usize>,
    warmup_secs: f64,
    measure_secs: f64,
    drive_mode: DriveMode,
    offered_rps: Option<f64>,
    addr: String,
    workload: Workload,
    payload_sizes: Vec<usize>,
    in_flights: Vec<usize>,
    json: bool,
}

#[derive(Debug, Clone)]
struct HistogramBin {
    value_us: u64,
    count: u64,
}

#[derive(Debug, Clone)]
struct BenchResult {
    workload: &'static str,
    transport: String,
    addr: String,
    mode: &'static str,
    count: Option<usize>,
    warmup_secs: f64,
    measure_secs: f64,
    offered_rps: Option<f64>,
    payload_size: usize,
    in_flight: usize,
    issued: usize,
    completed: usize,
    errors: usize,
    dropped: usize,
    elapsed_secs: f64,
    per_call_micros: f64,
    calls_per_sec: f64,
    p50_us: f64,
    p90_us: f64,
    p99_us: f64,
    p999_us: f64,
    max_us: f64,
    histogram: Vec<HistogramBin>,
}

struct TrialAccumulator {
    histogram: Histogram<u64>,
    issued: usize,
    completed: usize,
    errors: usize,
    dropped: usize,
}

impl TrialAccumulator {
    fn new() -> Result<Self> {
        Ok(Self {
            histogram: Histogram::new_with_bounds(1, 60_000_000, 3)
                .context("failed to allocate latency histogram")?,
            issued: 0,
            completed: 0,
            errors: 0,
            dropped: 0,
        })
    }

    fn record_issue(&mut self) {
        self.issued += 1;
    }

    fn record_ok(&mut self, elapsed: Duration) {
        let latency_us = elapsed.as_micros().clamp(1, 60_000_000u128) as u64;
        let _ = self.histogram.record(latency_us);
        self.completed += 1;
    }

    fn record_err(&mut self) {
        self.errors += 1;
    }

    fn record_drop(&mut self) {
        self.dropped += 1;
    }

    fn drain_histogram(&mut self) -> Result<Histogram<u64>> {
        let replacement = Histogram::new_with_bounds(1, 60_000_000, 3)
            .context("failed to allocate replacement latency histogram")?;
        Ok(std::mem::replace(&mut self.histogram, replacement))
    }
}

#[derive(Debug)]
struct TrialResult {
    issued: usize,
    completed: usize,
    errors: usize,
    dropped: usize,
    elapsed_secs: f64,
    per_call_micros: f64,
    calls_per_sec: f64,
    p50_us: f64,
    p90_us: f64,
    p99_us: f64,
    p999_us: f64,
    max_us: f64,
    histogram: Vec<HistogramBin>,
}

fn parse_workload(value: &str) -> Result<Workload> {
    match value {
        "echo" => Ok(Workload::Echo),
        "canvas" => Ok(Workload::Canvas),
        "gnarly" => Ok(Workload::Gnarly),
        _ => Err(eyre::eyre!(
            "invalid --workload value '{value}', expected echo, canvas, or gnarly"
        )),
    }
}

fn parse_drive_mode(value: &str) -> Result<DriveMode> {
    match value {
        "closed" => Ok(DriveMode::Closed),
        "open" => Ok(DriveMode::Open),
        _ => Err(eyre::eyre!(
            "invalid --drive-mode value '{value}', expected closed or open"
        )),
    }
}

fn parse_usize_csv(value: &str, flag: &str) -> Result<Vec<usize>> {
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

fn parse_f64_flag(value: &str, flag: &str) -> Result<f64> {
    value
        .parse::<f64>()
        .map_err(|e| eyre::eyre!("invalid {flag} value '{value}': {e}"))
}

fn parse_config() -> Result<Config> {
    let mut count: Option<usize> = None;
    let mut warmup_secs: f64 = 5.0;
    let mut measure_secs: f64 = 30.0;
    let mut drive_mode = DriveMode::Closed;
    let mut offered_rps: Option<f64> = None;
    let mut addr = "local:///tmp/bench.vox".to_string();
    let mut workload = Workload::Echo;
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
                count = Some(
                    v.parse::<usize>()
                        .map_err(|e| eyre::eyre!("invalid --count value '{v}': {e}"))?,
                );
            }
            "--warmup-secs" => {
                let v = args
                    .next()
                    .ok_or_else(|| eyre::eyre!("missing value for --warmup-secs"))?;
                warmup_secs = parse_f64_flag(&v, "--warmup-secs")?;
            }
            "--measure-secs" => {
                let v = args
                    .next()
                    .ok_or_else(|| eyre::eyre!("missing value for --measure-secs"))?;
                measure_secs = parse_f64_flag(&v, "--measure-secs")?;
            }
            "--drive-mode" => {
                let v = args
                    .next()
                    .ok_or_else(|| eyre::eyre!("missing value for --drive-mode"))?;
                drive_mode = parse_drive_mode(&v)?;
            }
            "--offered-rps" => {
                let v = args
                    .next()
                    .ok_or_else(|| eyre::eyre!("missing value for --offered-rps"))?;
                offered_rps = Some(parse_f64_flag(&v, "--offered-rps")?);
            }
            "--addr" => {
                addr = args
                    .next()
                    .ok_or_else(|| eyre::eyre!("missing value for --addr"))?;
            }
            "--workload" => {
                let v = args
                    .next()
                    .ok_or_else(|| eyre::eyre!("missing value for --workload"))?;
                workload = parse_workload(&v)?;
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
        count = Some(
            pos_count
                .parse::<usize>()
                .map_err(|e| eyre::eyre!("invalid positional count '{pos_count}': {e}"))?,
        );
    }
    if let Some(pos_addr) = positionals.get(1) {
        addr = pos_addr.clone();
    }

    payload_sizes.sort_unstable();
    payload_sizes.dedup();
    in_flights.sort_unstable();
    in_flights.dedup();

    if payload_sizes.contains(&0) {
        return Err(eyre::eyre!("payload sizes must be > 0"));
    }
    if in_flights.contains(&0) {
        return Err(eyre::eyre!("in-flight values must be > 0"));
    }
    if let Some(count) = count
        && count == 0
    {
        return Err(eyre::eyre!("count must be > 0"));
    }
    if warmup_secs < 0.0 {
        return Err(eyre::eyre!("warmup_secs must be >= 0"));
    }
    if measure_secs <= 0.0 {
        return Err(eyre::eyre!("measure_secs must be > 0"));
    }
    if let Some(offered_rps) = offered_rps
        && offered_rps <= 0.0
    {
        return Err(eyre::eyre!("offered_rps must be > 0"));
    }
    if drive_mode == DriveMode::Open && count.is_some() {
        return Err(eyre::eyre!(
            "open-loop mode does not support --count; use --warmup-secs/--measure-secs"
        ));
    }
    if drive_mode == DriveMode::Open && offered_rps.is_none() {
        return Err(eyre::eyre!("open-loop mode requires --offered-rps"));
    }

    Ok(Config {
        count,
        warmup_secs,
        measure_secs,
        drive_mode,
        offered_rps,
        addr,
        workload,
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

fn workload_name(workload: Workload) -> &'static str {
    match workload {
        Workload::Echo => "echo",
        Workload::Canvas => "canvas",
        Workload::Gnarly => "gnarly",
    }
}

fn make_canvas(shape_count: usize, seq: usize) -> (String, Vec<Shape>, Color) {
    let mut shapes = Vec::with_capacity(shape_count);
    for i in 0..shape_count {
        let n = seq + i;
        let shape = match i % 3 {
            0 => Shape::Rectangle {
                width: (n % 97 + 3) as f64,
                height: (n % 89 + 5) as f64,
            },
            1 => Shape::Circle {
                radius: (n % 53 + 1) as f64 / 2.0,
            },
            _ => Shape::Point,
        };
        shapes.push(shape);
    }
    let background = match seq % 3 {
        0 => Color::Red,
        1 => Color::Green,
        _ => Color::Blue,
    };
    (format!("bench-canvas-{seq:08x}"), shapes, background)
}

fn make_gnarly_payload(entry_count: usize, seq: usize) -> GnarlyPayload {
    let entries = (0..entry_count)
        .map(|i| {
            let attrs = vec![
                GnarlyAttr {
                    key: "owner".to_string(),
                    value: format!("user-{seq}-{i}"),
                },
                GnarlyAttr {
                    key: "class".to_string(),
                    value: format!("hot-path-{}", (seq + i) % 17),
                },
                GnarlyAttr {
                    key: "etag".to_string(),
                    value: format!("etag-{seq:08x}-{i:08x}"),
                },
            ];
            let chunks = (0..3)
                .map(|j| {
                    let len = 32 * (j + 1);
                    let byte = ((seq + i + j) & 0xff) as u8;
                    vec![byte; len]
                })
                .collect();
            let kind = match i % 3 {
                0 => GnarlyKind::File {
                    mime: "application/octet-stream".to_string(),
                    tags: vec![
                        "warm".to_string(),
                        "cacheable".to_string(),
                        format!("tag-{seq}-{i}"),
                    ],
                },
                1 => GnarlyKind::Directory {
                    child_count: i as u32 + 3,
                    children: vec![
                        format!("child-{seq}-{i}-0"),
                        format!("child-{seq}-{i}-1"),
                        format!("child-{seq}-{i}-2"),
                    ],
                },
                _ => GnarlyKind::Symlink {
                    target: format!("/target/{seq}/{i}/nested/item"),
                    hops: vec![1, 2, 3, i as u32],
                },
            };
            GnarlyEntry {
                id: seq as u64 * 1_000_000 + i as u64,
                parent: if i == 0 {
                    None
                } else {
                    Some(seq as u64 * 1_000_000 + i as u64 - 1)
                },
                name: format!("entry-{seq}-{i}"),
                path: format!("/mount/very/deep/path/with/component/{seq}/{i}/file.bin"),
                attrs,
                chunks,
                kind,
            }
        })
        .collect();

    GnarlyPayload {
        revision: seq as u64,
        mount: format!("/mnt/bench-fast-path-{seq:08x}"),
        entries,
        footer: Some(format!("benchmark footer {seq}")),
        digest: vec![(seq & 0xff) as u8; 64],
    }
}

fn transport_from_addr(addr: &str) -> String {
    addr.split("://").next().unwrap_or("unknown").to_string()
}

fn quantile_us(hist: &Histogram<u64>, q: f64) -> f64 {
    if hist.is_empty() {
        0.0
    } else {
        hist.value_at_quantile(q) as f64
    }
}

fn recorded_bins(hist: &Histogram<u64>) -> Vec<HistogramBin> {
    let mut out = Vec::new();
    for bucket in hist.iter_recorded() {
        out.push(HistogramBin {
            value_us: bucket.value_iterated_to(),
            count: bucket.count_since_last_iteration(),
        });
    }
    out
}

async fn run_one(
    client: Arc<TestbedClient>,
    workload: Workload,
    payload_size: usize,
    seq: usize,
) -> Result<()> {
    match workload {
        Workload::Echo => {
            let resp = client.echo(make_payload(payload_size, seq)).await?;
            black_box(resp);
        }
        Workload::Canvas => {
            let (name, shapes, background) = make_canvas(payload_size, seq);
            let resp = client.create_canvas(name, shapes, background).await?;
            black_box(resp);
        }
        Workload::Gnarly => {
            let resp = client
                .echo_gnarly(make_gnarly_payload(payload_size, seq))
                .await?;
            black_box(resp);
        }
    }
    Ok(())
}

async fn run_count_case(
    client: Arc<TestbedClient>,
    workload: Workload,
    count: usize,
    payload_size: usize,
    in_flight: usize,
) -> Result<TrialResult> {
    let start = Instant::now();
    let acc = Arc::new(Mutex::new(TrialAccumulator::new()?));

    if in_flight == 1 {
        for i in 0..count {
            let t0 = Instant::now();
            acc.lock().unwrap().record_issue();
            match run_one(Arc::clone(&client), workload, payload_size, i).await {
                Ok(()) => acc.lock().unwrap().record_ok(t0.elapsed()),
                Err(_) => acc.lock().unwrap().record_err(),
            }
        }
    } else {
        let mut launched = 0usize;
        let mut completed = 0usize;
        let mut joins: JoinSet<(Result<()>, Instant, Instant)> = JoinSet::new();

        while completed < count {
            while launched < count && joins.len() < in_flight {
                let c = Arc::clone(&client);
                let issue_at = Instant::now();
                acc.lock().unwrap().record_issue();
                joins.spawn(async move {
                    let result = run_one(c, workload, payload_size, launched).await;
                    let completed_at = Instant::now();
                    (result, issue_at, completed_at)
                });
                launched += 1;
            }

            if let Some(joined) = joins.join_next().await {
                let (result, issue_at, completed_at) = joined.context("bench task panicked")?;
                match result {
                    Ok(()) => acc
                        .lock()
                        .unwrap()
                        .record_ok(completed_at.duration_since(issue_at)),
                    Err(_) => acc.lock().unwrap().record_err(),
                }
                completed += 1;
            }
        }
    }

    let elapsed_secs = start.elapsed().as_secs_f64();
    let mut acc = acc.lock().unwrap();
    let histogram = acc.drain_histogram()?;
    let issued = acc.issued;
    let completed = acc.completed;
    let errors = acc.errors;
    let dropped = acc.dropped;
    let per_call_micros = if completed == 0 {
        0.0
    } else {
        histogram.mean()
    };
    let calls_per_sec = if elapsed_secs <= f64::EPSILON {
        0.0
    } else {
        completed as f64 / elapsed_secs
    };

    Ok(TrialResult {
        issued,
        completed,
        errors,
        dropped,
        elapsed_secs,
        per_call_micros,
        calls_per_sec,
        p50_us: quantile_us(&histogram, 0.50),
        p90_us: quantile_us(&histogram, 0.90),
        p99_us: quantile_us(&histogram, 0.99),
        p999_us: quantile_us(&histogram, 0.999),
        max_us: quantile_us(&histogram, 1.0),
        histogram: recorded_bins(&histogram),
    })
}

async fn run_timed_phase(
    client: Arc<TestbedClient>,
    workload: Workload,
    duration: Duration,
    payload_size: usize,
    in_flight: usize,
) -> Result<TrialResult> {
    let start = Instant::now();
    let deadline = start + duration;
    let seq = Arc::new(AtomicUsize::new(0));
    let acc = Arc::new(Mutex::new(TrialAccumulator::new()?));
    let mut joins = JoinSet::new();

    for _ in 0..in_flight {
        let client = Arc::clone(&client);
        let seq = Arc::clone(&seq);
        let acc = Arc::clone(&acc);
        joins.spawn(async move {
            loop {
                if Instant::now() >= deadline {
                    break;
                }
                let n = seq.fetch_add(1, Ordering::Relaxed);
                acc.lock().unwrap().record_issue();
                let t0 = Instant::now();
                match run_one(Arc::clone(&client), workload, payload_size, n).await {
                    Ok(()) => acc.lock().unwrap().record_ok(t0.elapsed()),
                    Err(_) => acc.lock().unwrap().record_err(),
                }
            }
            Ok::<(), eyre::Report>(())
        });
    }

    while let Some(joined) = joins.join_next().await {
        joined.context("timed bench worker panicked")??;
    }

    let elapsed_secs = start.elapsed().as_secs_f64();
    let mut acc = acc.lock().unwrap();
    let histogram = acc.drain_histogram()?;
    let issued = acc.issued;
    let completed = acc.completed;
    let errors = acc.errors;
    let dropped = acc.dropped;
    let per_call_micros = if completed == 0 {
        0.0
    } else {
        histogram.mean()
    };
    let calls_per_sec = if elapsed_secs <= f64::EPSILON {
        0.0
    } else {
        completed as f64 / elapsed_secs
    };

    Ok(TrialResult {
        issued,
        completed,
        errors,
        dropped,
        elapsed_secs,
        per_call_micros,
        calls_per_sec,
        p50_us: quantile_us(&histogram, 0.50),
        p90_us: quantile_us(&histogram, 0.90),
        p99_us: quantile_us(&histogram, 0.99),
        p999_us: quantile_us(&histogram, 0.999),
        max_us: quantile_us(&histogram, 1.0),
        histogram: recorded_bins(&histogram),
    })
}

async fn run_timed_open_phase(
    client: Arc<TestbedClient>,
    workload: Workload,
    duration: Duration,
    payload_size: usize,
    max_in_flight: usize,
    offered_rps: f64,
) -> Result<TrialResult> {
    let start = Instant::now();
    let deadline = start + duration;
    let mut next_arrival = start;
    let arrival_interval = Duration::from_secs_f64(1.0 / offered_rps);
    let seq = Arc::new(AtomicUsize::new(0));
    let acc = Arc::new(Mutex::new(TrialAccumulator::new()?));
    let mut joins: JoinSet<(Result<()>, Instant, Instant)> = JoinSet::new();

    loop {
        let arrivals_done = next_arrival >= deadline;
        if arrivals_done && joins.is_empty() {
            break;
        }

        if arrivals_done {
            if let Some(joined) = joins.join_next().await {
                let (result, scheduled_arrival, completed_at) =
                    joined.context("open-loop worker panicked")?;
                match result {
                    Ok(()) => acc
                        .lock()
                        .unwrap()
                        .record_ok(completed_at.duration_since(scheduled_arrival)),
                    Err(_) => acc.lock().unwrap().record_err(),
                }
            }
            continue;
        }

        let sleep = tokio::time::sleep_until(next_arrival.into());
        tokio::pin!(sleep);

        tokio::select! {
            joined = joins.join_next(), if !joins.is_empty() => {
                if let Some(joined) = joined {
                    let (result, scheduled_arrival, completed_at) = joined.context("open-loop worker panicked")?;
                    match result {
                        Ok(()) => acc.lock().unwrap().record_ok(completed_at.duration_since(scheduled_arrival)),
                        Err(_) => acc.lock().unwrap().record_err(),
                    }
                }
            }
            _ = &mut sleep => {
                if joins.len() < max_in_flight {
                    let c = Arc::clone(&client);
                    let n = seq.fetch_add(1, Ordering::Relaxed);
                    let scheduled_arrival = next_arrival;
                    acc.lock().unwrap().record_issue();
                    joins.spawn(async move {
                        let result = run_one(c, workload, payload_size, n).await;
                        let completed_at = Instant::now();
                        (result, scheduled_arrival, completed_at)
                    });
                } else {
                    acc.lock().unwrap().record_drop();
                }

                next_arrival += arrival_interval;
                let now = Instant::now();
                if next_arrival < now {
                    let lag = now.duration_since(next_arrival);
                    let skipped = (lag.as_secs_f64() / arrival_interval.as_secs_f64()).floor() as usize;
                    for _ in 0..skipped {
                        if next_arrival >= deadline {
                            break;
                        }
                        acc.lock().unwrap().record_drop();
                        next_arrival += arrival_interval;
                    }
                }
            }
        }
    }

    let elapsed_secs = duration.as_secs_f64();
    let mut acc = acc.lock().unwrap();
    let histogram = acc.drain_histogram()?;
    let issued = acc.issued;
    let completed = acc.completed;
    let errors = acc.errors;
    let dropped = acc.dropped;
    let per_call_micros = if completed == 0 {
        0.0
    } else {
        histogram.mean()
    };
    let calls_per_sec = if elapsed_secs <= f64::EPSILON {
        0.0
    } else {
        completed as f64 / elapsed_secs
    };

    Ok(TrialResult {
        issued,
        completed,
        errors,
        dropped,
        elapsed_secs,
        per_call_micros,
        calls_per_sec,
        p50_us: quantile_us(&histogram, 0.50),
        p90_us: quantile_us(&histogram, 0.90),
        p99_us: quantile_us(&histogram, 0.99),
        p999_us: quantile_us(&histogram, 0.999),
        max_us: quantile_us(&histogram, 1.0),
        histogram: recorded_bins(&histogram),
    })
}

async fn measure_case(
    client: Arc<TestbedClient>,
    cfg: &Config,
    payload_size: usize,
    in_flight: usize,
) -> Result<BenchResult> {
    let transport = transport_from_addr(&cfg.addr);
    let workload = workload_name(cfg.workload);

    let trial = if let Some(count) = cfg.count {
        run_count_case(
            Arc::clone(&client),
            cfg.workload,
            count,
            payload_size,
            in_flight,
        )
        .await?
    } else {
        if cfg.warmup_secs > 0.0 {
            let _ = if cfg.drive_mode == DriveMode::Open {
                run_timed_open_phase(
                    Arc::clone(&client),
                    cfg.workload,
                    Duration::from_secs_f64(cfg.warmup_secs),
                    payload_size,
                    in_flight,
                    cfg.offered_rps.expect("validated offered_rps"),
                )
                .await?
            } else {
                run_timed_phase(
                    Arc::clone(&client),
                    cfg.workload,
                    Duration::from_secs_f64(cfg.warmup_secs),
                    payload_size,
                    in_flight,
                )
                .await?
            };
        }
        if cfg.drive_mode == DriveMode::Open {
            run_timed_open_phase(
                Arc::clone(&client),
                cfg.workload,
                Duration::from_secs_f64(cfg.measure_secs),
                payload_size,
                in_flight,
                cfg.offered_rps.expect("validated offered_rps"),
            )
            .await?
        } else {
            run_timed_phase(
                Arc::clone(&client),
                cfg.workload,
                Duration::from_secs_f64(cfg.measure_secs),
                payload_size,
                in_flight,
            )
            .await?
        }
    };

    Ok(BenchResult {
        workload,
        transport,
        addr: cfg.addr.clone(),
        mode: if cfg.count.is_some() {
            "count"
        } else {
            "timed"
        },
        count: cfg.count,
        warmup_secs: if cfg.count.is_some() {
            0.0
        } else {
            cfg.warmup_secs
        },
        offered_rps: cfg.offered_rps,
        measure_secs: if cfg.count.is_some() {
            trial.elapsed_secs
        } else {
            cfg.measure_secs
        },
        payload_size,
        in_flight,
        issued: trial.issued,
        completed: trial.completed,
        errors: trial.errors,
        dropped: trial.dropped,
        elapsed_secs: trial.elapsed_secs,
        per_call_micros: trial.per_call_micros,
        calls_per_sec: trial.calls_per_sec,
        p50_us: trial.p50_us,
        p90_us: trial.p90_us,
        p99_us: trial.p99_us,
        p999_us: trial.p999_us,
        max_us: trial.max_us,
        histogram: trial.histogram,
    })
}

fn print_json(results: &[BenchResult]) {
    println!("[");
    for (i, r) in results.iter().enumerate() {
        let comma = if i + 1 == results.len() { "" } else { "," };
        println!(
            "  {{\"workload\":\"{}\",\"transport\":\"{}\",\"addr\":\"{}\",\"mode\":\"{}\",\"count\":{},\"warmup_secs\":{:.3},\"measure_secs\":{:.3},\"offered_rps\":{},\"payload_size\":{},\"in_flight\":{},\"issued\":{},\"completed\":{},\"errors\":{},\"dropped\":{},\"elapsed_secs\":{:.6},\"per_call_micros\":{:.3},\"calls_per_sec\":{:.3},\"p50_us\":{:.3},\"p90_us\":{:.3},\"p99_us\":{:.3},\"p999_us\":{:.3},\"max_us\":{:.3},\"histogram\":[{}]}}{}",
            r.workload,
            r.transport,
            r.addr,
            r.mode,
            match r.count {
                Some(count) => count.to_string(),
                None => "null".to_string(),
            },
            r.warmup_secs,
            r.measure_secs,
            match r.offered_rps {
                Some(offered_rps) => format!("{offered_rps:.3}"),
                None => "null".to_string(),
            },
            r.payload_size,
            r.in_flight,
            r.issued,
            r.completed,
            r.errors,
            r.dropped,
            r.elapsed_secs,
            r.per_call_micros,
            r.calls_per_sec,
            r.p50_us,
            r.p90_us,
            r.p99_us,
            r.p999_us,
            r.max_us,
            r.histogram
                .iter()
                .map(|bin| format!("{{\"value_us\":{},\"count\":{}}}", bin.value_us, bin.count))
                .collect::<Vec<_>>()
                .join(","),
            comma
        );
    }
    println!("]");
}

async fn run_ffi_bench(cfg: Config) -> Result<()> {
    // Server side: serve the Testbed service in a spawned task.
    let _server = tokio::spawn(async move {
        let link = ffi_bench_server::accept().await.expect("accept ffi link");
        let _conn = vox::acceptor_on(link)
            .on_connection(TestbedDispatcher::new(TestbedService))
            .establish::<vox::NoopClient>()
            .await
            .expect("ffi server handshake");
        std::future::pending::<()>().await
    });

    // Client side: establish initiator and get a TestbedClient caller.
    let link = ffi_bench_client::connect(ffi_bench_server::vtable()).context("connect ffi link")?;
    let client = Arc::new(
        vox::initiator_on(link, vox::TransportMode::Bare)
            .establish::<TestbedClient>()
            .await
            .context("ffi client establish")?,
    );

    eprintln!("ffi in-process session established, running benchmark matrix...");
    let mut results = Vec::<BenchResult>::new();
    for &payload_size in &cfg.payload_sizes {
        for &in_flight in &cfg.in_flights {
            let outcome = measure_case(Arc::clone(&client), &cfg, payload_size, in_flight).await;
            let result = match outcome {
                Ok(v) => v,
                Err(err) => {
                    eprintln!(
                        "workload={} transport={} size={} in_flight={} ERROR: {}",
                        workload_name(cfg.workload),
                        transport_from_addr(&cfg.addr),
                        payload_size,
                        in_flight,
                        err
                    );
                    continue;
                }
            };
            eprintln!(
                "workload={} transport={} size={} in_flight={} mode={} issued={} completed={} errors={} dropped={} elapsed={:.2}s mean={:.3}us p50={:.3}us p99={:.3}us p999={:.3}us calls_per_sec={:.0}",
                result.workload,
                result.transport,
                result.payload_size,
                result.in_flight,
                result.mode,
                result.issued,
                result.completed,
                result.errors,
                result.dropped,
                result.elapsed_secs,
                result.per_call_micros,
                result.p50_us,
                result.p99_us,
                result.p999_us,
                result.calls_per_sec,
            );
            results.push(result);
        }
    }
    if cfg.json {
        print_json(&results);
    }
    std::process::exit(0);
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let cfg = parse_config()?;

    tracing_subscriber::fmt::init();

    if cfg.addr == "ffi://" {
        return run_ffi_bench(cfg).await;
    }

    let serve_addr = cfg.addr.clone();
    eprintln!("serving on {}, waiting for peer to connect...", serve_addr);
    if let Some(count) = cfg.count {
        eprintln!(
            "plan: workload={}, drive_mode={:?}, count_mode(count={}), payload_sizes={:?}, in_flights={:?}",
            workload_name(cfg.workload),
            cfg.drive_mode,
            count,
            cfg.payload_sizes,
            cfg.in_flights
        );
    } else {
        eprintln!(
            "plan: workload={}, drive_mode={:?}, timed_mode(warmup_secs={:.2}, measure_secs={:.2}, offered_rps={:?}), payload_sizes={:?}, in_flights={:?}",
            workload_name(cfg.workload),
            cfg.drive_mode,
            cfg.warmup_secs,
            cfg.measure_secs,
            cfg.offered_rps,
            cfg.payload_sizes,
            cfg.in_flights
        );
    }

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
                            measure_case(Arc::clone(&client), &cfg, payload_size, in_flight).await;
                        let result = match outcome {
                            Ok(v) => v,
                            Err(err) => {
                                eprintln!(
                                    "workload={} transport={} size={} in_flight={} ERROR: {}",
                                    workload_name(cfg.workload),
                                    transport_from_addr(&cfg.addr),
                                    payload_size,
                                    in_flight,
                                    err
                                );
                                continue;
                            }
                        };

                        eprintln!(
                            "workload={} transport={} size={} in_flight={} mode={} issued={} completed={} errors={} dropped={} elapsed={:.2}s mean={:.3}us p50={:.3}us p99={:.3}us p999={:.3}us calls_per_sec={:.0}",
                            result.workload,
                            result.transport,
                            result.payload_size,
                            result.in_flight,
                            result.mode,
                            result.issued,
                            result.completed,
                            result.errors,
                            result.dropped,
                            result.elapsed_secs,
                            result.per_call_micros,
                            result.p50_us,
                            result.p99_us,
                            result.p999_us,
                            result.calls_per_sec,
                        );
                        results.push(result);
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
