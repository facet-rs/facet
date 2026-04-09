use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use eyre::{Context as _, Result};
use indicatif::{ProgressBar, ProgressStyle};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Workload {
    Echo,
    Canvas,
    Gnarly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Transport {
    Local,
    Ffi,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ServerImpl {
    Swift,
    Rust,
}

#[derive(Debug, Clone)]
struct Config {
    quick: bool,
    short: bool,
    workload: Workload,
    payload_sizes: Vec<usize>,
    in_flights: Vec<usize>,
    blocks: usize,
    warmup_secs: f64,
    measure_secs: f64,
    calibration_warmup_secs: f64,
    calibration_measure_secs: f64,
    calibration_target_drop_min: f64,
    calibration_target_drop_max: f64,
    calibration_deadline_secs: f64,
    load_factors: Vec<f64>,
    transports: Vec<Transport>,
    server_impls: Vec<ServerImpl>,
    out: PathBuf,
    logs_dir: PathBuf,
    serve_report: bool,
    bind: String,
    sweep_rps: Option<Vec<usize>>,
    samply: bool,
    server_samply: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HistogramBin {
    value_us: u64,
    count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BenchResult {
    workload: String,
    transport: String,
    addr: String,
    mode: String,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TrialRow {
    #[serde(flatten)]
    bench: BenchResult,
    server_impl: String,
    block: usize,
    order_in_block: usize,
    baseline_rps: Option<f64>,
    load_factor: Option<f64>,
    peak_rss_kib: Option<u64>,
    label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Calibration {
    server_impl: String,
    payload_size: usize,
    in_flight: usize,
    baseline_rps: Option<f64>,
    transport_trials: BTreeMap<String, TrialRow>,
    offered_rps_values: Vec<usize>,
    load_factors: Vec<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Aggregate {
    calibrations: Vec<Calibration>,
    rows: Vec<TrialRow>,
}

#[derive(Debug, Clone)]
struct Probe {
    offered_rps: usize,
    row: TrialRow,
}

fn repo_root() -> Result<PathBuf> {
    Ok(Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| eyre::eyre!("rust-examples crate must live under the workspace root"))?
        .to_path_buf())
}

fn workspace_root() -> Result<PathBuf> {
    repo_root()
}

fn subject_cmd_for(server_impl: ServerImpl, root: &Path) -> PathBuf {
    match server_impl {
        ServerImpl::Swift => root.join("swift").join("subject").join("subject-swift.sh"),
        ServerImpl::Rust => root.join("target").join("release").join("subject-rust"),
    }
}

fn bench_runner_cmd(root: &Path) -> PathBuf {
    let bin_name = if cfg!(windows) {
        "bench_runner.exe"
    } else {
        "bench_runner"
    };
    root.join("target")
        .join("release")
        .join("examples")
        .join(bin_name)
}

fn bench_client_cmd(root: &Path) -> PathBuf {
    let bin_name = if cfg!(windows) {
        "bench_client.exe"
    } else {
        "bench_client"
    };
    root.join("target")
        .join("release")
        .join("examples")
        .join(bin_name)
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

fn parse_transport(value: &str) -> Result<Transport> {
    match value {
        "local" => Ok(Transport::Local),
        "ffi" => Ok(Transport::Ffi),
        _ => Err(eyre::eyre!(
            "invalid --transports value '{value}', expected local or ffi"
        )),
    }
}

fn parse_server_impl(value: &str) -> Result<ServerImpl> {
    match value {
        "swift" => Ok(ServerImpl::Swift),
        "rust" => Ok(ServerImpl::Rust),
        _ => Err(eyre::eyre!(
            "invalid --server-impls value '{value}', expected swift or rust"
        )),
    }
}

fn parse_csv_usize(value: &str, flag: &str) -> Result<Vec<usize>> {
    let mut out = Vec::new();
    for part in value.split(',') {
        let s = part.trim();
        if s.is_empty() {
            continue;
        }
        let parsed = s
            .parse::<usize>()
            .map_err(|e| eyre::eyre!("invalid {flag} value '{s}': {e}"))?;
        if parsed == 0 {
            return Err(eyre::eyre!("{flag} values must be > 0"));
        }
        out.push(parsed);
    }
    if out.is_empty() {
        return Err(eyre::eyre!("no values provided for {flag}"));
    }
    Ok(out)
}

fn parse_csv_f64(value: &str, flag: &str) -> Result<Vec<f64>> {
    let mut out = Vec::new();
    for part in value.split(',') {
        let s = part.trim();
        if s.is_empty() {
            continue;
        }
        let parsed = s
            .parse::<f64>()
            .map_err(|e| eyre::eyre!("invalid {flag} value '{s}': {e}"))?;
        if !(parsed.is_finite() && parsed > 0.0) {
            return Err(eyre::eyre!("{flag} values must be finite and > 0"));
        }
        out.push(parsed);
    }
    if out.is_empty() {
        return Err(eyre::eyre!("no values provided for {flag}"));
    }
    Ok(out)
}

fn parse_args() -> Result<Config> {
    let mut quick = false;
    let mut short = false;
    let mut workload = Workload::Gnarly;
    let mut payload_sizes = vec![256];
    let mut in_flights = vec![1];
    let mut blocks = 1usize;
    let mut warmup_secs = 0.25f64;
    let mut measure_secs = 1.0f64;
    let mut calibration_warmup_secs = 0.1f64;
    let mut calibration_measure_secs = 0.2f64;
    let mut calibration_target_drop_min = 0.01f64;
    let mut calibration_target_drop_max = 0.05f64;
    let mut calibration_deadline_secs = 10.0f64;
    let mut load_factors = vec![
        0.1, 0.2, 0.35, 0.5, 0.65, 0.8, 1.0, 1.2, 1.5, 1.75, 2.0, 2.5, 3.0, 4.0,
    ];
    let mut transports = vec![Transport::Local, Transport::Ffi];
    let mut server_impls = vec![ServerImpl::Swift, ServerImpl::Rust];
    let mut out = PathBuf::from("/tmp/shootout.json");
    let mut logs_dir = PathBuf::from("/tmp/shootout-logs");
    let mut serve_report = false;
    let mut bind = "127.0.0.1:8000".to_string();
    let mut sweep_rps: Option<Vec<usize>> = None;
    let mut samply = false;
    let mut server_samply = false;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--quick" => {
                quick = true;
            }
            "--short" => {
                short = true;
            }
            "--workload" => {
                workload = parse_workload(
                    &args
                        .next()
                        .ok_or_else(|| eyre::eyre!("missing value for --workload"))?,
                )?
            }
            "--payload-sizes" => {
                payload_sizes = parse_csv_usize(
                    &args
                        .next()
                        .ok_or_else(|| eyre::eyre!("missing value for --payload-sizes"))?,
                    "--payload-sizes",
                )?
            }
            "--in-flights" => {
                in_flights = parse_csv_usize(
                    &args
                        .next()
                        .ok_or_else(|| eyre::eyre!("missing value for --in-flights"))?,
                    "--in-flights",
                )?
            }
            "--blocks" => {
                blocks = args
                    .next()
                    .ok_or_else(|| eyre::eyre!("missing value for --blocks"))?
                    .parse::<usize>()
                    .map_err(|e| eyre::eyre!("invalid --blocks value: {e}"))?
            }
            "--warmup-secs" => {
                warmup_secs = args
                    .next()
                    .ok_or_else(|| eyre::eyre!("missing value for --warmup-secs"))?
                    .parse::<f64>()
                    .map_err(|e| eyre::eyre!("invalid --warmup-secs value: {e}"))?
            }
            "--measure-secs" => {
                measure_secs = args
                    .next()
                    .ok_or_else(|| eyre::eyre!("missing value for --measure-secs"))?
                    .parse::<f64>()
                    .map_err(|e| eyre::eyre!("invalid --measure-secs value: {e}"))?
            }
            "--calibration-warmup-secs" => {
                calibration_warmup_secs = args
                    .next()
                    .ok_or_else(|| eyre::eyre!("missing value for --calibration-warmup-secs"))?
                    .parse::<f64>()
                    .map_err(|e| eyre::eyre!("invalid --calibration-warmup-secs value: {e}"))?
            }
            "--calibration-measure-secs" => {
                calibration_measure_secs = args
                    .next()
                    .ok_or_else(|| eyre::eyre!("missing value for --calibration-measure-secs"))?
                    .parse::<f64>()
                    .map_err(|e| eyre::eyre!("invalid --calibration-measure-secs value: {e}"))?
            }
            "--calibration-target-drop-min" => {
                calibration_target_drop_min = args
                    .next()
                    .ok_or_else(|| eyre::eyre!("missing value for --calibration-target-drop-min"))?
                    .parse::<f64>()
                    .map_err(|e| eyre::eyre!("invalid --calibration-target-drop-min value: {e}"))?
            }
            "--calibration-target-drop-max" => {
                calibration_target_drop_max = args
                    .next()
                    .ok_or_else(|| eyre::eyre!("missing value for --calibration-target-drop-max"))?
                    .parse::<f64>()
                    .map_err(|e| eyre::eyre!("invalid --calibration-target-drop-max value: {e}"))?
            }
            "--calibration-deadline-secs" => {
                calibration_deadline_secs = args
                    .next()
                    .ok_or_else(|| eyre::eyre!("missing value for --calibration-deadline-secs"))?
                    .parse::<f64>()
                    .map_err(|e| eyre::eyre!("invalid --calibration-deadline-secs value: {e}"))?
            }
            "--load-factors" => {
                load_factors = parse_csv_f64(
                    &args
                        .next()
                        .ok_or_else(|| eyre::eyre!("missing value for --load-factors"))?,
                    "--load-factors",
                )?
            }
            "--transports" => {
                transports = args
                    .next()
                    .ok_or_else(|| eyre::eyre!("missing value for --transports"))?
                    .split(',')
                    .map(|s| parse_transport(s.trim()))
                    .collect::<Result<Vec<_>>>()?
            }
            "--server-impls" => {
                server_impls = args
                    .next()
                    .ok_or_else(|| eyre::eyre!("missing value for --server-impls"))?
                    .split(',')
                    .map(|s| parse_server_impl(s.trim()))
                    .collect::<Result<Vec<_>>>()?
            }
            "--out" => {
                out = PathBuf::from(
                    args.next()
                        .ok_or_else(|| eyre::eyre!("missing value for --out"))?,
                )
            }
            "--logs-dir" => {
                logs_dir = PathBuf::from(
                    args.next()
                        .ok_or_else(|| eyre::eyre!("missing value for --logs-dir"))?,
                )
            }
            "--serve-report" => serve_report = true,
            "--samply" => samply = true,
            "--server-samply" => server_samply = true,
            "--bind" => {
                bind = args
                    .next()
                    .ok_or_else(|| eyre::eyre!("missing value for --bind"))?
            }
            "--sweep" => {
                sweep_rps = Some((1..=8).map(|i| i * 25).collect());
            }
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            _ => return Err(eyre::eyre!("unknown arg: {arg}")),
        }
    }

    if blocks == 0 {
        return Err(eyre::eyre!("--blocks must be > 0"));
    }
    if warmup_secs < 0.0 || measure_secs <= 0.0 {
        return Err(eyre::eyre!(
            "warmup-secs must be >= 0 and measure-secs must be > 0"
        ));
    }
    if calibration_warmup_secs < 0.0 || calibration_measure_secs <= 0.0 {
        return Err(eyre::eyre!(
            "calibration warmup must be >= 0 and calibration measure must be > 0"
        ));
    }
    if calibration_target_drop_min < 0.0
        || calibration_target_drop_max < calibration_target_drop_min
    {
        return Err(eyre::eyre!("invalid calibration drop band"));
    }
    if calibration_deadline_secs <= 0.0 {
        return Err(eyre::eyre!("--calibration-deadline-secs must be > 0"));
    }
    if samply && server_samply {
        return Err(eyre::eyre!(
            "--samply and --server-samply are mutually exclusive"
        ));
    }

    if quick {
        workload = Workload::Gnarly;
        payload_sizes = vec![16];
        in_flights = vec![1];
        blocks = 1;
        warmup_secs = 0.1;
        measure_secs = 0.2;
        calibration_warmup_secs = 0.05;
        calibration_measure_secs = 0.1;
        calibration_target_drop_min = 0.01;
        calibration_target_drop_max = 0.05;
        calibration_deadline_secs = 2.0;
        load_factors = vec![1.0];
        transports = vec![Transport::Local];
        server_impls = vec![ServerImpl::Swift];
    }

    if short {
        workload = Workload::Gnarly;
        payload_sizes = vec![256];
        in_flights = vec![1];
        blocks = 1;
        warmup_secs = 0.1;
        measure_secs = 0.2;
        calibration_warmup_secs = 0.05;
        calibration_measure_secs = 0.1;
        calibration_target_drop_min = 0.01;
        calibration_target_drop_max = 0.05;
        calibration_deadline_secs = 5.0;
        load_factors = vec![0.75, 1.0, 1.5, 2.0];
        transports = vec![Transport::Local];
        server_impls = vec![ServerImpl::Swift];
    }

    Ok(Config {
        quick,
        short,
        workload,
        payload_sizes,
        in_flights,
        blocks,
        warmup_secs,
        measure_secs,
        calibration_warmup_secs,
        calibration_measure_secs,
        calibration_target_drop_min,
        calibration_target_drop_max,
        calibration_deadline_secs,
        load_factors,
        transports,
        server_impls,
        out,
        logs_dir,
        serve_report,
        bind,
        sweep_rps,
        samply,
        server_samply,
    })
}

fn print_usage() {
    eprintln!(
        "usage: cargo run -p rust-examples --example shootout -- [options]\n\
single-binary workflow:\n\
  1. run the benchmark and write JSON\n\
  2. optionally serve the report from the same binary with --serve-report\n\
options:\n\
  --quick              run a small smoke-test preset\n\
  --short              run a slightly broader preset\n\
  --workload <echo|canvas|gnarly>\n\
  --payload-sizes <csv>\n\
  --in-flights <csv>\n\
  --blocks <n>\n\
  --warmup-secs <n>\n\
  --measure-secs <n>\n\
  --calibration-warmup-secs <n>\n\
  --calibration-measure-secs <n>\n\
  --calibration-target-drop-min <n>\n\
  --calibration-target-drop-max <n>\n\
  --calibration-deadline-secs <n>\n\
  --load-factors <csv>\n\
  --transports <local,ffi>\n\
  --server-impls <swift,rust>\n\
  --out <path>\n\
  --logs-dir <dir>\n\
  --serve-report       serve the static HTML/JS report after writing JSON\n\
  --bind <addr>        HTTP bind address for --serve-report\n\
  --samply             profile the bench client under samply (local transport only)\n\
  --server-samply      profile the subject under samply (local transport only)"
    );
}

fn quick_summary(cfg: &Config) -> &'static str {
    if cfg.quick {
        "quick preset"
    } else if cfg.short {
        "short preset"
    } else {
        "custom matrix"
    }
}

fn addr_for_transport(transport: Transport) -> &'static str {
    match transport {
        Transport::Local => "local:///tmp/bench.vox",
        Transport::Ffi => "ffi://",
    }
}

fn workload_name(workload: Workload) -> &'static str {
    match workload {
        Workload::Echo => "echo",
        Workload::Canvas => "canvas",
        Workload::Gnarly => "gnarly",
    }
}

fn transport_name(transport: Transport) -> &'static str {
    match transport {
        Transport::Local => "local",
        Transport::Ffi => "ffi",
    }
}

fn server_impl_name(server_impl: ServerImpl) -> &'static str {
    match server_impl {
        ServerImpl::Swift => "swift",
        ServerImpl::Rust => "rust",
    }
}

fn make_trial_label(
    prefix: &str,
    server_impl: ServerImpl,
    transport: Transport,
    payload_size: usize,
    in_flight: usize,
    offered_rps: usize,
) -> String {
    format!(
        "{prefix}-srv={}-transport={}-payload={}-in_flight={}-rps={}",
        server_impl_name(server_impl),
        transport_name(transport),
        payload_size,
        in_flight,
        offered_rps
    )
}

fn make_calibration_label(
    server_impl: ServerImpl,
    transport: Transport,
    payload_size: usize,
    in_flight: usize,
) -> String {
    format!(
        "cal-srv={}-transport={}-payload={}-in_flight={}",
        server_impl_name(server_impl),
        transport_name(transport),
        payload_size,
        in_flight
    )
}

fn make_calibration_probe_label(
    server_impl: ServerImpl,
    transport: Transport,
    payload_size: usize,
    in_flight: usize,
    offered_rps: usize,
    suffix: &str,
) -> String {
    format!(
        "calprobe-srv={}-transport={}-payload={}-in_flight={}-rps={}-phase={}",
        server_impl_name(server_impl),
        transport_name(transport),
        payload_size,
        in_flight,
        offered_rps,
        suffix
    )
}

fn build_release_binaries(root: &Path) -> Result<()> {
    eprintln!("building release benchmark binaries...");
    let status = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
        .current_dir(root)
        .args([
            "build",
            "--quiet",
            "-p",
            "rust-examples",
            "--example",
            "bench_runner",
            "--example",
            "bench_client",
            "-p",
            "subject-rust",
            "--bin",
            "subject-rust",
            "--release",
        ])
        .status()
        .context("failed to invoke cargo build")?;
    if !status.success() {
        return Err(eyre::eyre!("cargo build failed with {status}"));
    }
    Ok(())
}

fn parse_peak_rss_kib(stderr: &str) -> Option<u64> {
    stderr
        .lines()
        .find_map(|line| line.strip_prefix("subject peak_rss_kib="))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|s| s.parse::<u64>().ok())
}

fn copy_trial_logs(logs_dir: &Path, from_label: &str, to_label: &str) -> Result<()> {
    if from_label == to_label {
        return Ok(());
    }
    for ext in ["stdout.json", "stderr.log"] {
        let from = logs_dir.join(format!("{from_label}.{ext}"));
        let to = logs_dir.join(format!("{to_label}.{ext}"));
        if from.exists() {
            fs::copy(&from, &to).with_context(|| {
                format!("failed to copy {} to {}", from.display(), to.display())
            })?;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run_ffi_trial_once(
    root: &Path,
    label: &str,
    server_impl: ServerImpl,
    payload_size: usize,
    in_flight: usize,
    offered_rps: usize,
    warmup_secs: f64,
    measure_secs: f64,
    cfg: &Config,
) -> Result<TrialRow> {
    let addr = addr_for_transport(Transport::Ffi);
    let args = vec![
        "--addr".to_string(),
        addr.to_string(),
        "--workload".to_string(),
        workload_name(cfg.workload).to_string(),
        "--payload-sizes".to_string(),
        payload_size.to_string(),
        "--in-flights".to_string(),
        in_flight.to_string(),
        "--drive-mode".to_string(),
        "open".to_string(),
        "--offered-rps".to_string(),
        offered_rps.to_string(),
        "--warmup-secs".to_string(),
        warmup_secs.to_string(),
        "--measure-secs".to_string(),
        measure_secs.to_string(),
        "--json".to_string(),
    ];

    let output = Command::new(bench_client_cmd(root))
        .current_dir(root)
        .args(&args)
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .output()
        .with_context(|| format!("failed to run ffi trial {label}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    fs::write(cfg.logs_dir.join(format!("{label}.stdout.json")), &stdout)
        .with_context(|| format!("failed to write stdout log for {label}"))?;
    fs::write(cfg.logs_dir.join(format!("{label}.stderr.log")), &stderr)
        .with_context(|| format!("failed to write stderr log for {label}"))?;

    if !output.status.success() {
        return Err(eyre::eyre!(
            "ffi trial {label} failed with status {}\n{stderr}",
            output.status
        ));
    }

    let mut rows: Vec<BenchResult> = serde_json::from_str(&stdout)
        .with_context(|| format!("failed to parse ffi trial JSON for {label}"))?;
    if rows.len() != 1 {
        return Err(eyre::eyre!(
            "ffi trial {label} produced {} rows, expected 1",
            rows.len()
        ));
    }
    let bench = rows.remove(0);
    Ok(TrialRow {
        bench,
        server_impl: server_impl_name(server_impl).to_string(),
        block: 0,
        order_in_block: 0,
        baseline_rps: None,
        load_factor: None,
        peak_rss_kib: None,
        label: label.to_string(),
    })
}

#[allow(clippy::too_many_arguments)]
fn run_trial_once(
    root: &Path,
    label: &str,
    server_impl: ServerImpl,
    transport: Transport,
    payload_size: usize,
    in_flight: usize,
    offered_rps: usize,
    warmup_secs: f64,
    measure_secs: f64,
    cfg: &Config,
) -> Result<TrialRow> {
    if transport == Transport::Ffi {
        return run_ffi_trial_once(
            root,
            label,
            server_impl,
            payload_size,
            in_flight,
            offered_rps,
            warmup_secs,
            measure_secs,
            cfg,
        );
    }

    let addr = addr_for_transport(transport);
    let mut args = vec![
        "--subject-cmd".to_string(),
        subject_cmd_for(server_impl, root).display().to_string(),
        "--subject-mode".to_string(),
        "server".to_string(),
        "--addr".to_string(),
        addr.to_string(),
    ];
    args.extend([
        "--".to_string(),
        "--addr".to_string(),
        addr.to_string(),
        "--workload".to_string(),
        workload_name(cfg.workload).to_string(),
        "--payload-sizes".to_string(),
        payload_size.to_string(),
        "--in-flights".to_string(),
        in_flight.to_string(),
        "--drive-mode".to_string(),
        "open".to_string(),
        "--offered-rps".to_string(),
        offered_rps.to_string(),
        "--warmup-secs".to_string(),
        warmup_secs.to_string(),
        "--measure-secs".to_string(),
        measure_secs.to_string(),
        "--json".to_string(),
    ]);

    let output = Command::new(bench_runner_cmd(root))
        .current_dir(root)
        .args(&args)
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .output()
        .with_context(|| format!("failed to run trial {label}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    fs::write(cfg.logs_dir.join(format!("{label}.stdout.json")), &stdout)
        .with_context(|| format!("failed to write stdout log for {label}"))?;
    fs::write(cfg.logs_dir.join(format!("{label}.stderr.log")), &stderr)
        .with_context(|| format!("failed to write stderr log for {label}"))?;

    if !output.status.success() {
        return Err(eyre::eyre!(
            "trial {label} failed with status {}\n{stderr}",
            output.status
        ));
    }

    let mut rows: Vec<BenchResult> = serde_json::from_str(&stdout)
        .with_context(|| format!("failed to parse trial JSON for {label}"))?;
    if rows.len() != 1 {
        return Err(eyre::eyre!(
            "trial {label} produced {} rows, expected 1",
            rows.len()
        ));
    }
    let bench = rows.remove(0);
    Ok(TrialRow {
        bench,
        server_impl: server_impl_name(server_impl).to_string(),
        block: 0,
        order_in_block: 0,
        baseline_rps: None,
        load_factor: None,
        peak_rss_kib: parse_peak_rss_kib(&stderr),
        label: label.to_string(),
    })
}

#[allow(clippy::too_many_arguments)]
fn run_trial(
    root: &Path,
    label: &str,
    server_impl: ServerImpl,
    transport: Transport,
    payload_size: usize,
    in_flight: usize,
    offered_rps: usize,
    warmup_secs: f64,
    measure_secs: f64,
    cfg: &Config,
) -> Result<TrialRow> {
    const MAX_RETRIES: usize = 2;
    let mut last_err = None;
    for attempt in 0..=MAX_RETRIES {
        if attempt > 0 {
            eprintln!(
                "retrying trial {label} (attempt {}/{})",
                attempt + 1,
                MAX_RETRIES + 1
            );
            thread::sleep(Duration::from_millis(500));
        }
        match run_trial_once(
            root,
            label,
            server_impl,
            transport,
            payload_size,
            in_flight,
            offered_rps,
            warmup_secs,
            measure_secs,
            cfg,
        ) {
            Ok(row) => return Ok(row),
            Err(err) => {
                eprintln!("trial {label} attempt {} failed: {}", attempt + 1, err);
                last_err = Some(err);
            }
        }
    }
    Err(last_err.unwrap())
}

fn drop_rate(row: &TrialRow) -> f64 {
    let issued = row.bench.issued as f64;
    let dropped = row.bench.dropped as f64;
    let denom = issued + dropped;
    if denom <= 0.0 { 1.0 } else { dropped / denom }
}

fn distance_to_drop_band(row: &TrialRow, min_drop: f64, max_drop: f64) -> f64 {
    if row.bench.errors != 0 {
        return f64::INFINITY;
    }
    let drop = drop_rate(row);
    if drop < min_drop {
        min_drop - drop
    } else if drop > max_drop {
        drop - max_drop
    } else {
        0.0
    }
}

fn choose_best_calibration_probe(probes: &[Probe], min_drop: f64, max_drop: f64) -> Option<&Probe> {
    let center = (min_drop + max_drop) / 2.0;
    probes.iter().min_by(|a, b| {
        let da = distance_to_drop_band(&a.row, min_drop, max_drop);
        let db = distance_to_drop_band(&b.row, min_drop, max_drop);
        da.partial_cmp(&db)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                let ea = a.row.bench.errors;
                let eb = b.row.bench.errors;
                ea.cmp(&eb)
            })
            .then_with(|| {
                let ca = (drop_rate(&a.row) - center).abs();
                let cb = (drop_rate(&b.row) - center).abs();
                ca.partial_cmp(&cb).unwrap_or(std::cmp::Ordering::Equal)
            })
    })
}

#[allow(clippy::too_many_arguments)]
fn run_calibration_probe(
    root: &Path,
    cfg: &Config,
    server_impl: ServerImpl,
    transport: Transport,
    payload_size: usize,
    in_flight: usize,
    offered_rps: usize,
    suffix: &str,
) -> Result<TrialRow> {
    let label = make_calibration_probe_label(
        server_impl,
        transport,
        payload_size,
        in_flight,
        offered_rps,
        suffix,
    );
    run_trial(
        root,
        &label,
        server_impl,
        transport,
        payload_size,
        in_flight,
        offered_rps,
        cfg.calibration_warmup_secs,
        cfg.calibration_measure_secs,
        cfg,
    )
}

fn calibrate_transport_open_loop(
    root: &Path,
    cfg: &Config,
    server_impl: ServerImpl,
    transport: Transport,
    payload_size: usize,
    in_flight: usize,
    pb: &ProgressBar,
) -> Result<(usize, TrialRow, BTreeMap<String, TrialRow>)> {
    let min_drop = cfg.calibration_target_drop_min;
    let max_drop = cfg.calibration_target_drop_max;
    let deadline = std::time::Instant::now()
        + std::time::Duration::from_secs_f64(cfg.calibration_deadline_secs);
    let mut offered = 10_000usize;
    let mut probes = Vec::<Probe>::new();
    let mut transport_trials = BTreeMap::<String, TrialRow>::new();
    let alias_label = make_calibration_label(server_impl, transport, payload_size, in_flight);
    let mut low: Option<Probe> = None;
    let mut high: Option<Probe> = None;

    loop {
        if std::time::Instant::now() >= deadline {
            break;
        }
        pb.set_message(format!(
            "calibrating srv={} transport={} payload={} in_flight={} rps={}",
            server_impl_name(server_impl),
            transport_name(transport),
            payload_size,
            in_flight,
            offered
        ));
        let row = run_calibration_probe(
            root,
            cfg,
            server_impl,
            transport,
            payload_size,
            in_flight,
            offered,
            &format!("probe{}", probes.len()),
        )?;
        let probe = Probe {
            offered_rps: offered,
            row,
        };
        let d = drop_rate(&probe.row);
        probes.push(probe.clone());

        if probe.row.bench.errors != 0 || d > max_drop {
            if high
                .as_ref()
                .map(|p| offered < p.offered_rps)
                .unwrap_or(true)
            {
                high = Some(probe);
            }
        } else {
            if low
                .as_ref()
                .map(|p| offered > p.offered_rps)
                .unwrap_or(true)
            {
                low = Some(probe);
            }
        }

        // Converged?
        if let (Some(lo), Some(hi)) = (&low, &high)
            && hi.offered_rps <= lo.offered_rps + 1
        {
            break;
        }

        offered = match (&low, &high) {
            (Some(lo), Some(hi)) => (lo.offered_rps + hi.offered_rps) / 2,
            (Some(lo), None) => lo.offered_rps.saturating_mul(2).min(1_000_000),
            (None, Some(hi)) => hi.offered_rps / 2,
            (None, None) => unreachable!("just ran a probe"),
        };

        if offered == 0 {
            break;
        }
    }

    let best = choose_best_calibration_probe(&probes, min_drop, max_drop)
        .ok_or_else(|| eyre::eyre!("no calibration probes produced a candidate"))?;
    let mut best_row = best.row.clone();
    copy_trial_logs(&cfg.logs_dir, &best_row.label, &alias_label)?;
    best_row.label = alias_label.clone();
    transport_trials.insert(transport_name(transport).to_string(), best_row.clone());
    Ok((best.offered_rps, best_row, transport_trials))
}

fn unique_sorted_usizes(values: Vec<usize>) -> Vec<usize> {
    let mut values = values;
    values.sort_unstable();
    values.dedup();
    values
}

fn planned_trials(cfg: &Config) -> usize {
    let combinations = cfg.server_impls.len()
        * cfg.payload_sizes.len()
        * cfg.in_flights.len()
        * cfg.transports.len();
    if let Some(sweep) = &cfg.sweep_rps {
        return cfg.blocks * combinations * sweep.len();
    }
    cfg.blocks * combinations * cfg.load_factors.len()
}

fn content_type(path: &str) -> &'static str {
    match Path::new(path).extension().and_then(|s| s.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "application/javascript; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn normalize_path(path: &str) -> Option<&str> {
    let path = path.split('?').next().unwrap_or(path);
    match path {
        "/" | "" => Some("/bench_open_loop_report.html"),
        "/index.html" => Some("/bench_open_loop_report.html"),
        p if p.starts_with('/') && !p.contains("..") => Some(p),
        _ => None,
    }
}

fn response(status: &str, content_type: &str, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    write!(
        &mut out,
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .unwrap();
    out.extend_from_slice(body);
    out
}

fn header(status: &str, content_type: &str) -> Vec<u8> {
    response(status, content_type, &[])
}

fn handle_connection(mut stream: TcpStream, report_root: &Path, data_path: &Path) -> Result<()> {
    let mut buf = [0u8; 8192];
    let n = stream.read(&mut buf).context("failed to read request")?;
    if n == 0 {
        return Ok(());
    }
    let request = String::from_utf8_lossy(&buf[..n]);
    let mut lines = request.lines();
    let request_line = lines.next().unwrap_or_default();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let path = parts.next().unwrap_or("/");

    if method != "GET" && method != "HEAD" {
        let body = b"method not allowed\n";
        stream.write_all(&response(
            "405 Method Not Allowed",
            "text/plain; charset=utf-8",
            body,
        ))?;
        return Ok(());
    }

    if path == "/data.json" {
        let body = fs::read(data_path)
            .with_context(|| format!("failed to read data file {}", data_path.display()))?;
        let resp = if method == "HEAD" {
            header("200 OK", content_type(path))
        } else {
            response("200 OK", content_type(path), &body)
        };
        stream.write_all(&resp)?;
        return Ok(());
    }

    let normalized = normalize_path(path).unwrap_or("/bench_open_loop_report.html");
    let full = report_root.join(normalized.trim_start_matches('/'));
    if full.is_file() {
        let body = fs::read(&full).with_context(|| format!("failed to read {}", full.display()))?;
        let resp = if method == "HEAD" {
            header("200 OK", content_type(normalized))
        } else {
            response("200 OK", content_type(normalized), &body)
        };
        stream.write_all(&resp)?;
        return Ok(());
    }

    let body = b"not found\n";
    stream.write_all(&response(
        "404 Not Found",
        "text/plain; charset=utf-8",
        body,
    ))?;
    Ok(())
}

fn serve_report(bind: &str, data_path: &Path, report_root: &Path) -> Result<()> {
    let listener =
        TcpListener::bind(bind).with_context(|| format!("failed to bind HTTP server on {bind}"))?;
    eprintln!(
        "serving open-loop report at http://{bind} (data: {})",
        data_path.display()
    );

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let report_root = report_root.to_path_buf();
                let data_path = data_path.to_path_buf();
                thread::spawn(move || {
                    let _ = handle_connection(stream, &report_root, &data_path);
                });
            }
            Err(err) => eprintln!("accept error: {err}"),
        }
    }

    Ok(())
}

#[derive(Clone)]
struct SimpleRng(u64);

impl SimpleRng {
    fn new() -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        Self(seed | 1)
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

fn shuffle<T>(items: &mut [T], rng: &mut SimpleRng) {
    for i in (1..items.len()).rev() {
        let j = (rng.next_u64() as usize) % (i + 1);
        items.swap(i, j);
    }
}

fn run_samply_session(root: &Path, cfg: &Config) -> Result<()> {
    let server_impl = *cfg
        .server_impls
        .first()
        .ok_or_else(|| eyre::eyre!("--samply requires at least one --server-impls"))?;
    let transport = cfg
        .transports
        .iter()
        .find(|&&t| t != Transport::Ffi || server_impl == ServerImpl::Rust)
        .copied()
        .ok_or_else(|| eyre::eyre!("no valid transport for samply session (ffi requires rust)"))?;
    if transport == Transport::Ffi {
        return Err(eyre::eyre!(
            "--samply does not support ffi transport (subject runs in-process)"
        ));
    }
    let payload_size = cfg.payload_sizes[0];
    let in_flight = cfg.in_flights[0];
    let addr = addr_for_transport(transport);

    eprintln!(
        "samply session: target={} srv={} transport={} payload={} in_flight={} warmup={:.1}s measure={:.1}s",
        if cfg.server_samply {
            "server"
        } else {
            "client"
        },
        server_impl_name(server_impl),
        transport_name(transport),
        payload_size,
        in_flight,
        cfg.warmup_secs,
        cfg.measure_secs,
    );

    let mut args = vec![
        "--subject-cmd".to_string(),
        subject_cmd_for(server_impl, root).display().to_string(),
        "--subject-mode".to_string(),
        "server".to_string(),
        "--addr".to_string(),
        addr.to_string(),
        "--".to_string(),
        "--addr".to_string(),
        addr.to_string(),
        "--workload".to_string(),
        workload_name(cfg.workload).to_string(),
        "--payload-sizes".to_string(),
        payload_size.to_string(),
        "--in-flights".to_string(),
        in_flight.to_string(),
        "--warmup-secs".to_string(),
        cfg.warmup_secs.to_string(),
        "--measure-secs".to_string(),
        cfg.measure_secs.to_string(),
    ];
    if cfg.server_samply {
        args.insert(6, "--server-samply".to_string());
    } else {
        args.insert(6, "--samply".to_string());
    }

    let status = Command::new(bench_runner_cmd(root))
        .current_dir(root)
        .args(&args)
        .status()
        .context("failed to run bench_runner for samply session")?;

    if !status.success() {
        return Err(eyre::eyre!("samply session failed with {status}"));
    }
    Ok(())
}

fn main() -> Result<()> {
    let cfg = parse_args()?;
    let root = workspace_root()?;
    fs::create_dir_all(&cfg.logs_dir)
        .with_context(|| format!("failed to create logs directory {}", cfg.logs_dir.display()))?;
    let total = planned_trials(&cfg) as u64;
    let pb = ProgressBar::new(total);
    pb.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} {pos}/{len} [{elapsed_precise}<{eta_precise}] {bar:40.cyan/blue} {msg}",
        )?
        .progress_chars("=>-"),
    );
    pb.set_message(format!(
        "building release binaries ({})",
        quick_summary(&cfg)
    ));
    build_release_binaries(&root)?;

    if cfg.samply || cfg.server_samply {
        pb.finish_and_clear();
        return run_samply_session(&root, &cfg);
    }

    let mut calibrations = Vec::<Calibration>::new();
    let mut rng = SimpleRng::new();

    for &server_impl in &cfg.server_impls {
        for &payload_size in &cfg.payload_sizes {
            for &in_flight in &cfg.in_flights {
                if let Some(sweep) = &cfg.sweep_rps {
                    calibrations.push(Calibration {
                        server_impl: server_impl_name(server_impl).to_string(),
                        payload_size,
                        in_flight,
                        baseline_rps: None,
                        transport_trials: BTreeMap::new(),
                        offered_rps_values: sweep.clone(),
                        load_factors: vec![1.0; sweep.len()],
                    });
                } else {
                    let mut transport_trials = BTreeMap::<String, TrialRow>::new();
                    for &transport in &cfg.transports {
                        if transport == Transport::Ffi && server_impl != ServerImpl::Rust {
                            continue;
                        }
                        pb.set_message(format!(
                            "calibrating srv={} transport={} payload={} in_flight={}",
                            server_impl_name(server_impl),
                            transport_name(transport),
                            payload_size,
                            in_flight
                        ));
                        let (_best, row, trials) = calibrate_transport_open_loop(
                            &root,
                            &cfg,
                            server_impl,
                            transport,
                            payload_size,
                            in_flight,
                            &pb,
                        )?;
                        transport_trials.extend(trials);
                        if let Some(trial) = transport_trials.get_mut(transport_name(transport)) {
                            trial.server_impl = server_impl_name(server_impl).to_string();
                            trial.baseline_rps =
                                Some(row.bench.offered_rps.unwrap_or(row.bench.calls_per_sec));
                        }
                    }

                    let transport_rows: Vec<&TrialRow> = transport_trials.values().collect();
                    let baseline_rps = transport_rows
                        .iter()
                        .map(|row| row.bench.offered_rps.unwrap_or(row.bench.calls_per_sec))
                        .fold(f64::INFINITY, f64::min);
                    let baseline_rps = if baseline_rps.is_finite() {
                        baseline_rps
                    } else {
                        0.0
                    };
                    let offered_rps_values = unique_sorted_usizes(
                        cfg.load_factors
                            .iter()
                            .map(|factor| {
                                std::cmp::max(1, (baseline_rps * factor).round() as usize)
                            })
                            .collect(),
                    );
                    calibrations.push(Calibration {
                        server_impl: server_impl_name(server_impl).to_string(),
                        payload_size,
                        in_flight,
                        baseline_rps: Some(baseline_rps),
                        transport_trials,
                        offered_rps_values,
                        load_factors: cfg.load_factors.clone(),
                    });
                }
            }
        }
    }

    let mut rows = Vec::<TrialRow>::new();
    for block_idx in 0..cfg.blocks {
        let block = block_idx + 1;
        let mut conditions = Vec::<(ServerImpl, Transport, usize, usize, usize, f64, f64)>::new();
        for cal in &calibrations {
            let server_impl = parse_server_impl(&cal.server_impl)?;
            for (i, offered_rps) in cal.offered_rps_values.iter().copied().enumerate() {
                for &transport in &cfg.transports {
                    if transport == Transport::Ffi && server_impl != ServerImpl::Rust {
                        continue;
                    }
                    if cfg.sweep_rps.is_none()
                        && !cal.transport_trials.contains_key(transport_name(transport))
                    {
                        continue;
                    }
                    conditions.push((
                        server_impl,
                        transport,
                        cal.payload_size,
                        cal.in_flight,
                        offered_rps,
                        cal.load_factors[i],
                        cal.baseline_rps.unwrap_or(0.0),
                    ));
                }
            }
        }

        shuffle(&mut conditions, &mut rng);
        for (
            order_idx,
            (
                server_impl,
                transport,
                payload_size,
                in_flight,
                offered_rps,
                load_factor,
                baseline_rps,
            ),
        ) in conditions.into_iter().enumerate()
        {
            pb.set_message(format!(
                "block={} trial={} srv={} transport={} payload={} in_flight={} rps={}",
                block,
                order_idx + 1,
                server_impl_name(server_impl),
                transport_name(transport),
                payload_size,
                in_flight,
                offered_rps
            ));
            let label = make_trial_label(
                &format!("b{}-o{}", block, order_idx + 1),
                server_impl,
                transport,
                payload_size,
                in_flight,
                offered_rps,
            );
            let mut row = run_trial(
                &root,
                &label,
                server_impl,
                transport,
                payload_size,
                in_flight,
                offered_rps,
                cfg.warmup_secs,
                cfg.measure_secs,
                &cfg,
            )?;
            row.block = block_idx + 1;
            row.order_in_block = order_idx + 1;
            row.baseline_rps = Some(baseline_rps);
            row.load_factor = Some(load_factor);
            rows.push(row);
            pb.inc(1);
        }
    }

    let aggregate = Aggregate { calibrations, rows };
    fs::write(&cfg.out, serde_json::to_string_pretty(&aggregate)? + "\n")
        .with_context(|| format!("failed to write {}", cfg.out.display()))?;
    pb.finish_with_message(format!(
        "wrote {} rows to {}",
        aggregate.rows.len(),
        cfg.out.display()
    ));
    eprintln!(
        "wrote {} open-loop trial rows to {}",
        aggregate.rows.len(),
        cfg.out.display()
    );
    if cfg.serve_report {
        let report_root = workspace_root()?.join("rust-examples");
        serve_report(&cfg.bind, &cfg.out, &report_root)?;
    }
    Ok(())
}
