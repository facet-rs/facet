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
    /// Run clippy on all code
    Clippy,
    /// Check formatting
    Fmt {
        /// Fix formatting issues instead of just checking
        #[arg(long)]
        fix: bool,
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
fn wait_for_server_ready(process: &mut Child, ready_marker: &str) -> Result<(), Box<dyn std::error::Error>> {
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
                for line in lines {
                    if let Ok(line) = line {
                        println!("  [server] {}", line);
                    }
                }
            });
            return Ok(());
        }
    }

    Err("Server process exited before becoming ready".into())
}
