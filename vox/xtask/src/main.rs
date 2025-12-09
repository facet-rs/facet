//! xtask: Development tasks for rapace
//!
//! Run with: `cargo xtask <command>`

use std::io::{BufRead, BufReader};
use std::process::{Child, ExitCode, Stdio};

use clap::{Parser, Subcommand};
use xshell::{cmd, Shell};

#[derive(Parser)]
#[command(name = "xtask")]
#[command(about = "Development tasks for rapace")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run all tests (workspace + fuzz harnesses)
    Test,
    /// Run fuzz tests with bolero
    Fuzz {
        /// Target to fuzz (e.g., "desc_ring", "data_segment", "session", "shm_integration")
        /// If not specified, runs all fuzz harnesses in test mode (quick smoke test)
        target: Option<String>,
    },
    /// Build wasm client and browser server
    Wasm,
    /// Run browser WebSocket tests with Playwright
    BrowserTest {
        /// Run tests in headed mode (show browser)
        #[arg(long)]
        headed: bool,
    },
    /// Run the rapace dashboard (builds wasm client first)
    Dashboard,
    /// Run clippy on all code
    Clippy,
    /// Check formatting
    Fmt {
        /// Fix formatting issues instead of just checking
        #[arg(long)]
        fix: bool,
    },
    /// Benchmark HTTP tunnel overhead
    Bench {
        /// Duration per test (e.g., "10s", "30s")
        #[arg(long, default_value = "10s")]
        duration: String,
        /// Concurrency levels to test (comma-separated)
        #[arg(long, default_value = "1,8,64,256")]
        concurrency: String,
    },
}

fn main() -> ExitCode {
    if let Err(e) = run() {
        eprintln!("Error: {e}");
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let sh = Shell::new()?;

    // Find workspace root (where Cargo.toml with [workspace] lives)
    let workspace_root = std::env::var("CARGO_MANIFEST_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::env::current_dir().unwrap())
        .parent()
        .unwrap()
        .to_path_buf();
    sh.change_dir(&workspace_root);

    match cli.command {
        Commands::Test => {
            println!("=== Running workspace tests ===");

            // Try nextest first, fall back to cargo test
            if cmd!(sh, "cargo nextest --version").quiet().run().is_ok() {
                println!("Using cargo-nextest");
                cmd!(sh, "cargo nextest run --workspace").run()?;
            } else {
                println!("cargo-nextest not found, using cargo test");
                cmd!(sh, "cargo test --workspace").run()?;
            }

            println!("\n=== Running fuzz harnesses (test mode) ===");
            sh.change_dir(workspace_root.join("fuzz"));
            cmd!(sh, "cargo test").run()?;

            println!("\n=== All tests passed ===");
        }
        Commands::Fuzz { target } => {
            sh.change_dir(workspace_root.join("fuzz"));

            if let Some(t) = target {
                println!("=== Fuzzing target: {t} ===");
                println!("Press Ctrl+C to stop.\n");

                // Check if cargo-bolero is installed
                if cmd!(sh, "cargo bolero --version").quiet().run().is_err() {
                    eprintln!("cargo-bolero not found. Install with:");
                    eprintln!("  cargo install cargo-bolero");
                    return Err("cargo-bolero not installed".into());
                }

                cmd!(sh, "cargo bolero test {t}").run()?;
            } else {
                println!("=== Running all fuzz harnesses in test mode ===");
                println!("(For real fuzzing, specify a target: cargo xtask fuzz desc_ring)\n");
                println!("Available targets:");
                println!("  - desc_ring         (DescRing enqueue/dequeue)");
                println!("  - data_segment      (DataSegment alloc/free)");
                println!("  - slot_state_machine (SlotMeta state transitions)");
                println!("  - session           (Session credit/cancel)");
                println!("  - shm_integration   (Combined ring+slab flow)\n");

                cmd!(sh, "cargo test").run()?;
            }
        }
        Commands::Wasm => {
            println!("=== Building wasm client ===");
            cmd!(
                sh,
                "cargo build -p rapace-wasm-client --target wasm32-unknown-unknown --release"
            )
            .run()?;

            println!("\n=== Building browser WS server ===");
            cmd!(sh, "cargo build --package browser-ws-server").run()?;

            println!("\n=== Wasm builds complete ===");
            println!("\nTo test in browser:");
            println!("  1. cargo run --package browser-ws-server");
            println!("  2. cd examples/browser_ws && wasm-pack build --target web ../../crates/rapace-wasm-client");
            println!("  3. python3 -m http.server 8080");
            println!("  4. Open http://localhost:8080");
            println!("\nOr run: cargo xtask browser-test");
        }
        Commands::BrowserTest { headed } => {
            run_browser_test(&sh, &workspace_root, headed)?;
        }
        Commands::Dashboard => {
            run_dashboard(&sh, &workspace_root)?;
        }
        Commands::Clippy => {
            println!("=== Running clippy ===");
            cmd!(sh, "cargo clippy --workspace --all-features -- -D warnings").run()?;

            println!("\n=== Clippy on fuzz crate ===");
            sh.change_dir(workspace_root.join("fuzz"));
            cmd!(sh, "cargo clippy -- -D warnings").run()?;
        }
        Commands::Fmt { fix } => {
            if fix {
                println!("=== Fixing formatting ===");
                cmd!(sh, "cargo fmt --all").run()?;
            } else {
                println!("=== Checking formatting ===");
                cmd!(sh, "cargo fmt --all -- --check").run()?;
            }
        }
        Commands::Bench {
            duration,
            concurrency,
        } => {
            run_bench(&sh, &workspace_root, &duration, &concurrency)?;
        }
    }

    Ok(())
}

/// Run browser WebSocket tests with Playwright.
///
/// This function:
/// 1. Builds the wasm client with wasm-pack
/// 2. Starts the WebSocket server
/// 3. Starts a static file server
/// 4. Runs Playwright tests
/// 5. Cleans up all processes
fn run_browser_test(
    sh: &Shell,
    workspace_root: &std::path::Path,
    headed: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let browser_ws_dir = workspace_root.join("examples/browser_ws");
    let wasm_client_dir = workspace_root.join("crates/rapace-wasm-client");

    // Step 1: Build wasm client with wasm-pack
    println!("=== Building wasm client with wasm-pack ===");
    if cmd!(sh, "wasm-pack --version").quiet().run().is_err() {
        eprintln!("wasm-pack not found. Install with:");
        eprintln!("  cargo install wasm-pack");
        return Err("wasm-pack not installed".into());
    }

    sh.change_dir(&wasm_client_dir);
    cmd!(sh, "wasm-pack build --target web").run()?;

    // Step 2: Install npm deps if needed
    println!("\n=== Checking npm dependencies ===");
    sh.change_dir(&browser_ws_dir);
    if !browser_ws_dir.join("node_modules").exists() {
        println!("Installing npm dependencies...");
        cmd!(sh, "npm install").run()?;
    }

    // Install Playwright browsers if needed
    if cmd!(sh, "npx playwright --version").quiet().run().is_err() {
        println!("Installing Playwright browsers...");
        cmd!(sh, "npx playwright install chromium").run()?;
    }

    // Step 3: Build and start the WebSocket server
    println!("\n=== Starting WebSocket server ===");
    sh.change_dir(workspace_root);
    cmd!(sh, "cargo build --package browser-ws-server --release").run()?;

    let ws_server_path = workspace_root.join("target/release/browser-ws-server");
    let mut ws_server = std::process::Command::new(&ws_server_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;

    // Wait for server to start (spawns thread to drain remaining output)
    wait_for_server_ready(&mut ws_server, "WebSocket server listening")?;
    println!("WebSocket server started on ws://127.0.0.1:9000");

    // Step 4: Start static file server
    println!("\n=== Starting static file server ===");
    let mut http_server = std::process::Command::new("python3")
        .args(["-m", "http.server", "8080"])
        .current_dir(&browser_ws_dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    // Give it a moment to start
    std::thread::sleep(std::time::Duration::from_millis(500));
    println!("Static file server started on http://127.0.0.1:8080");

    // Step 5: Run Playwright tests
    println!("\n=== Running Playwright tests ===");
    sh.change_dir(&browser_ws_dir);

    let test_result = if headed {
        cmd!(sh, "npx playwright test --headed").run()
    } else {
        cmd!(sh, "npx playwright test").run()
    };

    // Step 6: Cleanup
    println!("\n=== Cleaning up ===");
    let _ = ws_server.kill();
    let _ = http_server.kill();

    // Check test result
    test_result?;

    println!("\n=== Browser tests passed ===");
    Ok(())
}

/// Wait for a server process to output a ready message, then spawn a thread to drain remaining output.
fn wait_for_server_ready(
    process: &mut Child,
    ready_marker: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let stdout = process.stdout.take().ok_or("no stdout")?;
    let reader = BufReader::new(stdout);
    let mut lines = reader.lines();

    // Wait for ready marker
    while let Some(line) = lines.next() {
        let line = line?;
        println!("  {}", line);
        if line.contains(ready_marker) {
            // Spawn thread to drain remaining output so process doesn't block
            std::thread::spawn(move || {
                for line in lines.map_while(Result::ok) {
                    println!("  [server] {}", line);
                }
            });
            return Ok(());
        }
    }

    Err("Server process exited before becoming ready".into())
}

/// oha JSON output format (partial - just what we need)
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct OhaResult {
    summary: OhaSummary,
    latency_percentiles: OhaLatencyPercentiles,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct OhaSummary {
    requests_per_sec: f64,
}

#[derive(serde::Deserialize)]
struct OhaLatencyPercentiles {
    p50: Option<f64>,
    p90: Option<f64>,
    p99: Option<f64>,
}

/// Benchmark result for a single run
#[allow(dead_code)]
struct BenchResult {
    name: String,
    endpoint: String,
    concurrency: u32,
    rps: f64,
    p50_ms: f64,
    p90_ms: f64,
    p99_ms: f64,
}

/// Run HTTP tunnel benchmarks using oha.
fn run_bench(
    sh: &Shell,
    workspace_root: &std::path::Path,
    duration: &str,
    concurrency_str: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    const HOST_PORT: u16 = 4000;
    const UNIX_SOCKET: &str = "/tmp/rapace-bench.sock";
    const SHM_FILE: &str = "/tmp/rapace-bench.shm";

    // Check for oha
    if cmd!(sh, "oha --version").quiet().run().is_err() {
        eprintln!("oha not found. Install with: cargo install oha (or brew install oha)");
        return Err("oha not installed".into());
    }

    // Parse concurrency levels
    let concurrency_levels: Vec<u32> = concurrency_str
        .split(',')
        .map(|s| s.trim().parse().expect("invalid concurrency"))
        .collect();

    // Get git info
    let git_commit = cmd!(sh, "git rev-parse --short HEAD")
        .quiet()
        .read()
        .unwrap_or_else(|_| "unknown".into());
    let git_branch = cmd!(sh, "git rev-parse --abbrev-ref HEAD")
        .quiet()
        .read()
        .unwrap_or_else(|_| "unknown".into());

    println!("============================================");
    println!("  HTTP Tunnel Benchmark");
    println!("============================================");
    println!();
    println!("Git commit: {}", git_commit);
    println!("Git branch: {}", git_branch);
    println!("Duration: {}", duration);
    println!("Concurrency: {:?}", concurrency_levels);
    println!();

    // Build release binaries
    println!("Building release binaries...");
    cmd!(sh, "cargo build --release -p rapace-http-tunnel").run()?;
    println!();

    let mut all_results: Vec<BenchResult> = Vec::new();

    // Helper to run oha and parse results
    let run_oha = |url: &str, c: u32, duration: &str| -> Result<OhaResult, Box<dyn std::error::Error>> {
        let c_str = c.to_string();
        let output = cmd!(sh, "oha {url} -z {duration} -c {c_str} --output-format json")
            .quiet()
            .read()?;
        let result: OhaResult = serde_json::from_str(&output)?;
        Ok(result)
    };

    // Cleanup helper
    let cleanup = || {
        let _ = cmd!(sh, "pkill -f http_baseline").quiet().run();
        let _ = cmd!(sh, "pkill -f http-tunnel-host").quiet().run();
        let _ = cmd!(sh, "pkill -f http-tunnel-plugin").quiet().run();
        let _ = std::fs::remove_file(UNIX_SOCKET);
        let _ = std::fs::remove_file(SHM_FILE);
        std::thread::sleep(std::time::Duration::from_millis(300));
    };

    // Wait for server to be ready
    let wait_for_server = |port: u16| -> bool {
        for _ in 0..50 {
            if std::net::TcpStream::connect(format!("127.0.0.1:{}", port)).is_ok() {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        false
    };

    let endpoints = vec![("small", "2 bytes"), ("large", "~256KB")];

    // ========== BASELINE ==========
    println!("=== Baseline (direct HTTP) ===");
    cleanup();

    let baseline_path = workspace_root.join("target/release/http_baseline");
    let mut baseline_proc = std::process::Command::new(&baseline_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    if !wait_for_server(HOST_PORT) {
        baseline_proc.kill()?;
        return Err("baseline server failed to start".into());
    }

    // Helper to convert seconds to ms and handle Option
    let to_ms = |secs: Option<f64>| secs.unwrap_or(0.0) * 1000.0;

    for (endpoint, desc) in &endpoints {
        println!("\n  /{} ({})", endpoint, desc);
        for &c in &concurrency_levels {
            let url = format!("http://127.0.0.1:{}/{}", HOST_PORT, endpoint);
            match run_oha(&url, c, duration) {
                Ok(result) => {
                    let p50_ms = to_ms(result.latency_percentiles.p50);
                    let p90_ms = to_ms(result.latency_percentiles.p90);
                    let p99_ms = to_ms(result.latency_percentiles.p99);
                    println!(
                        "    c={:3}: {:8.0} RPS, p50={:6.3}ms, p99={:6.3}ms",
                        c, result.summary.requests_per_sec, p50_ms, p99_ms
                    );
                    all_results.push(BenchResult {
                        name: "baseline".into(),
                        endpoint: endpoint.to_string(),
                        concurrency: c,
                        rps: result.summary.requests_per_sec,
                        p50_ms,
                        p90_ms,
                        p99_ms,
                    });
                }
                Err(e) => println!("    c={:3}: ERROR: {}", c, e),
            }
        }
    }

    let _ = baseline_proc.kill();
    let _ = baseline_proc.wait();

    // ========== TUNNEL OVER STREAM ==========
    println!("\n=== Tunnel over Stream (Unix Socket) ===");
    cleanup();

    let host_path = workspace_root.join("target/release/http-tunnel-host");
    let plugin_path = workspace_root.join("target/release/http-tunnel-plugin");

    // Start host (listens on socket)
    let mut host_proc = std::process::Command::new(&host_path)
        .args(["--transport=stream", &format!("--addr={}", UNIX_SOCKET)])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    std::thread::sleep(std::time::Duration::from_millis(300));

    // Start plugin (connects to socket)
    let mut plugin_proc = std::process::Command::new(&plugin_path)
        .args(["--transport=stream", &format!("--addr={}", UNIX_SOCKET)])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    if !wait_for_server(HOST_PORT) {
        let _ = host_proc.kill();
        let _ = plugin_proc.kill();
        return Err("tunnel-stream server failed to start".into());
    }

    for (endpoint, desc) in &endpoints {
        println!("\n  /{} ({})", endpoint, desc);
        for &c in &concurrency_levels {
            let url = format!("http://127.0.0.1:{}/{}", HOST_PORT, endpoint);
            match run_oha(&url, c, duration) {
                Ok(result) => {
                    let p50_ms = to_ms(result.latency_percentiles.p50);
                    let p90_ms = to_ms(result.latency_percentiles.p90);
                    let p99_ms = to_ms(result.latency_percentiles.p99);
                    println!(
                        "    c={:3}: {:8.0} RPS, p50={:6.3}ms, p99={:6.3}ms",
                        c, result.summary.requests_per_sec, p50_ms, p99_ms
                    );
                    all_results.push(BenchResult {
                        name: "stream".into(),
                        endpoint: endpoint.to_string(),
                        concurrency: c,
                        rps: result.summary.requests_per_sec,
                        p50_ms,
                        p90_ms,
                        p99_ms,
                    });
                }
                Err(e) => println!("    c={:3}: ERROR: {}", c, e),
            }
        }
    }

    let _ = host_proc.kill();
    let _ = plugin_proc.kill();
    let _ = host_proc.wait();
    let _ = plugin_proc.wait();

    // ========== TUNNEL OVER SHM ==========
    println!("\n=== Tunnel over SHM ===");
    cleanup();

    // Start host (creates SHM file)
    let mut host_proc = std::process::Command::new(&host_path)
        .args(["--transport=shm", &format!("--addr={}", SHM_FILE)])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    std::thread::sleep(std::time::Duration::from_millis(500));

    // Start plugin (opens SHM file)
    let mut plugin_proc = std::process::Command::new(&plugin_path)
        .args(["--transport=shm", &format!("--addr={}", SHM_FILE)])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    if !wait_for_server(HOST_PORT) {
        let _ = host_proc.kill();
        let _ = plugin_proc.kill();
        return Err("tunnel-shm server failed to start".into());
    }

    for (endpoint, desc) in &endpoints {
        println!("\n  /{} ({})", endpoint, desc);
        for &c in &concurrency_levels {
            let url = format!("http://127.0.0.1:{}/{}", HOST_PORT, endpoint);
            match run_oha(&url, c, duration) {
                Ok(result) => {
                    let p50_ms = to_ms(result.latency_percentiles.p50);
                    let p90_ms = to_ms(result.latency_percentiles.p90);
                    let p99_ms = to_ms(result.latency_percentiles.p99);
                    println!(
                        "    c={:3}: {:8.0} RPS, p50={:6.3}ms, p99={:6.3}ms",
                        c, result.summary.requests_per_sec, p50_ms, p99_ms
                    );
                    all_results.push(BenchResult {
                        name: "shm".into(),
                        endpoint: endpoint.to_string(),
                        concurrency: c,
                        rps: result.summary.requests_per_sec,
                        p50_ms,
                        p90_ms,
                        p99_ms,
                    });
                }
                Err(e) => println!("    c={:3}: ERROR: {}", c, e),
            }
        }
    }

    let _ = host_proc.kill();
    let _ = plugin_proc.kill();
    let _ = host_proc.wait();
    let _ = plugin_proc.wait();

    // ========== TUNNEL OVER SHM (LARGE CONFIG) ==========
    println!("\n=== Tunnel over SHM (large config: 256 slots × 16KB) ===");
    cleanup();

    // Start host with --shm-large flag
    let mut host_proc = std::process::Command::new(&host_path)
        .args(["--transport=shm", &format!("--addr={}", SHM_FILE), "--shm-large"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    std::thread::sleep(std::time::Duration::from_millis(500));

    // Start plugin (must also pass --shm-large since config is not stored in file)
    let mut plugin_proc = std::process::Command::new(&plugin_path)
        .args(["--transport=shm", &format!("--addr={}", SHM_FILE), "--shm-large"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    if !wait_for_server(HOST_PORT) {
        let _ = host_proc.kill();
        let _ = plugin_proc.kill();
        return Err("tunnel-shm-large server failed to start".into());
    }

    for (endpoint, desc) in &endpoints {
        println!("\n  /{} ({})", endpoint, desc);
        for &c in &concurrency_levels {
            let url = format!("http://127.0.0.1:{}/{}", HOST_PORT, endpoint);
            match run_oha(&url, c, duration) {
                Ok(result) => {
                    let p50_ms = to_ms(result.latency_percentiles.p50);
                    let p90_ms = to_ms(result.latency_percentiles.p90);
                    let p99_ms = to_ms(result.latency_percentiles.p99);
                    println!(
                        "    c={:3}: {:8.0} RPS, p50={:6.3}ms, p99={:6.3}ms",
                        c, result.summary.requests_per_sec, p50_ms, p99_ms
                    );
                    all_results.push(BenchResult {
                        name: "shm-large".into(),
                        endpoint: endpoint.to_string(),
                        concurrency: c,
                        rps: result.summary.requests_per_sec,
                        p50_ms,
                        p90_ms,
                        p99_ms,
                    });
                }
                Err(e) => println!("    c={:3}: ERROR: {}", c, e),
            }
        }
    }

    let _ = host_proc.kill();
    let _ = plugin_proc.kill();
    let _ = host_proc.wait();
    let _ = plugin_proc.wait();

    cleanup();

    // ========== SUMMARY ==========
    println!("\n============================================");
    println!("  Summary");
    println!("============================================");

    // Calculate overhead vs baseline
    println!("\nOverhead vs Baseline (negative = faster than baseline):\n");

    for (endpoint, _) in &endpoints {
        println!("/{} endpoint:", endpoint);
        println!("  {:>10} {:>8} {:>12} {:>12}", "Transport", "Conc", "RPS Δ%", "p99 Δ%");

        for &c in &concurrency_levels {
            let baseline = all_results
                .iter()
                .find(|r| r.name == "baseline" && r.endpoint == *endpoint && r.concurrency == c);

            if let Some(base) = baseline {
                for transport in &["stream", "shm", "shm-large"] {
                    if let Some(tunnel) = all_results
                        .iter()
                        .find(|r| r.name == *transport && r.endpoint == *endpoint && r.concurrency == c)
                    {
                        let rps_delta = ((tunnel.rps - base.rps) / base.rps) * 100.0;
                        let p99_delta = ((tunnel.p99_ms - base.p99_ms) / base.p99_ms) * 100.0;
                        println!(
                            "  {:>10} {:>8} {:>+11.1}% {:>+11.1}%",
                            transport, c, rps_delta, p99_delta
                        );
                    }
                }
            }
        }
        println!();
    }

    Ok(())
}

/// Run the rapace dashboard.
///
/// This function:
/// 1. Builds the wasm client with wasm-pack
/// 2. Copies wasm files to dashboard static directory
/// 3. Runs the dashboard server
fn run_dashboard(
    sh: &Shell,
    workspace_root: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let wasm_client_dir = workspace_root.join("crates/rapace-wasm-client");
    let dashboard_dir = workspace_root.join("examples/dashboard");

    // Step 1: Build wasm client with wasm-pack
    println!("=== Building wasm client with wasm-pack ===");
    if cmd!(sh, "wasm-pack --version").quiet().run().is_err() {
        eprintln!("wasm-pack not found. Install with:");
        eprintln!("  cargo install wasm-pack");
        return Err("wasm-pack not installed".into());
    }

    sh.change_dir(&wasm_client_dir);
    cmd!(sh, "wasm-pack build --target web").run()?;

    // Step 2: Copy wasm files to dashboard static directory
    println!("\n=== Copying wasm files to dashboard ===");
    let pkg_dir = wasm_client_dir.join("pkg");
    let static_dir = dashboard_dir.join("static/pkg");

    // Create static/pkg directory if it doesn't exist
    std::fs::create_dir_all(&static_dir)?;

    // Copy all files from pkg to static/pkg (skip directories and symlinks)
    for entry in std::fs::read_dir(&pkg_dir)? {
        let entry = entry?;
        let src = entry.path();
        let file_type = entry.file_type()?;

        // Skip directories and symlinks
        if file_type.is_dir() || file_type.is_symlink() {
            continue;
        }

        let dst = static_dir.join(entry.file_name());
        std::fs::copy(&src, &dst)?;
        println!("  Copied: {}", entry.file_name().to_string_lossy());
    }

    // Step 3: Run the dashboard
    sh.change_dir(workspace_root);

    // The dashboard prints its own banner
    cmd!(sh, "cargo run -p rapace-dashboard").run()?;

    Ok(())
}
