//! Cross-language test matrix for roam RPC.
//!
//! Tests all client-server pairs across {Rust, Go, TypeScript, Swift}.
//! Runs tests in parallel for faster execution.

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Language {
    Rust,
    Go,
    TypeScript,
    Swift,
}

impl Language {
    fn name(&self) -> &'static str {
        match self {
            Language::Rust => "Rust",
            Language::Go => "Go",
            Language::TypeScript => "TypeScript",
            Language::Swift => "Swift",
        }
    }
}

struct Server {
    child: Child,
    #[allow(dead_code)]
    port: u16,
}

impl Server {
    fn spawn(lang: Language, port: u16) -> Result<Self, String> {
        let child = match lang {
            Language::Rust => Command::new("./target/release/tcp-echo-server")
                .env("TCP_PORT", port.to_string())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| format!("Failed to spawn Rust server: {}", e))?,

            Language::Go => Command::new("./go/server/go-server")
                .env("TCP_PORT", port.to_string())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| format!("Failed to spawn Go server: {}", e))?,

            Language::TypeScript => Command::new("sh")
                .args(["typescript/tests/tcp-server.sh"])
                .env("TCP_PORT", port.to_string())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| format!("Failed to spawn TypeScript server: {}", e))?,

            Language::Swift => Command::new("swift/server/.build/debug/swift-server")
                .env("TCP_PORT", port.to_string())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| format!("Failed to spawn Swift server: {}", e))?,
        };

        Ok(Self { child, port })
    }

    fn wait_ready(&mut self) -> Result<(), String> {
        // Read the port number from stdout to confirm server is ready
        if let Some(ref mut stdout) = self.child.stdout {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            reader
                .read_line(&mut line)
                .map_err(|e| format!("Failed to read server output: {}", e))?;
            // Server prints its port when ready
        }
        // Give it a moment to fully initialize
        thread::sleep(Duration::from_millis(100));
        Ok(())
    }

    fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.kill();
    }
}

fn run_client(lang: Language, addr: &str) -> Result<bool, String> {
    let output = match lang {
        Language::Rust => Command::new("./target/release/tcp-echo-client")
            .env("SERVER_ADDR", addr)
            .output()
            .map_err(|e| format!("Failed to run Rust client: {}", e))?,

        Language::Go => Command::new("./go/client/go-client")
            .env("SERVER_ADDR", addr)
            .output()
            .map_err(|e| format!("Failed to run Go client: {}", e))?,

        Language::TypeScript => Command::new("sh")
            .args(["typescript/tests/tcp-client.sh"])
            .env("SERVER_ADDR", addr)
            .output()
            .map_err(|e| format!("Failed to run TypeScript client: {}", e))?,

        Language::Swift => Command::new("swift/client/.build/debug/swift-client")
            .env("SERVER_ADDR", addr)
            .output()
            .map_err(|e| format!("Failed to run Swift client: {}", e))?,
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.contains("All tests passed"))
}

#[derive(Debug)]
struct TestResult {
    server: Language,
    client: Language,
    passed: bool,
    error: Option<String>,
    duration: Duration,
}

fn run_test(server_lang: Language, client_lang: Language, port: u16) -> TestResult {
    let start = Instant::now();

    // Spawn server
    let mut server = match Server::spawn(server_lang, port) {
        Ok(s) => s,
        Err(e) => {
            return TestResult {
                server: server_lang,
                client: client_lang,
                passed: false,
                error: Some(e),
                duration: start.elapsed(),
            };
        }
    };

    // Wait for server to be ready
    if let Err(e) = server.wait_ready() {
        return TestResult {
            server: server_lang,
            client: client_lang,
            passed: false,
            error: Some(e),
            duration: start.elapsed(),
        };
    }

    // Run client
    let addr = format!("127.0.0.1:{}", port);
    let passed = match run_client(client_lang, &addr) {
        Ok(p) => p,
        Err(e) => {
            return TestResult {
                server: server_lang,
                client: client_lang,
                passed: false,
                error: Some(e),
                duration: start.elapsed(),
            };
        }
    };

    TestResult {
        server: server_lang,
        client: client_lang,
        passed,
        error: None,
        duration: start.elapsed(),
    }
}

fn format_duration(d: Duration) -> String {
    let ms = d.as_millis();
    if ms < 1000 {
        format!("{}ms", ms)
    } else {
        format!("{:.2}s", d.as_secs_f64())
    }
}

fn main() {
    let total_start = Instant::now();

    println!("╔════════════════════════════════════════════════════════════╗");
    println!("║          Cross-Language Test Matrix (4×4 = 16 tests)       ║");
    println!("╚════════════════════════════════════════════════════════════╝");
    println!();

    let servers = [
        Language::Rust,
        Language::Go,
        Language::TypeScript,
        Language::Swift,
    ];
    let clients = [
        Language::Rust,
        Language::Go,
        Language::TypeScript,
        Language::Swift,
    ];

    // Create test pairs with unique ports
    let mut tests: Vec<(Language, Language, u16)> = Vec::new();
    let mut port = 9300u16;
    for &server in &servers {
        for &client in &clients {
            tests.push((server, client, port));
            port += 1;
        }
    }

    // Run tests in parallel using threads
    let (tx, rx) = mpsc::channel::<TestResult>();

    let handles: Vec<_> = tests
        .into_iter()
        .map(|(server, client, port)| {
            let tx = tx.clone();
            thread::spawn(move || {
                let result = run_test(server, client, port);
                tx.send(result).unwrap();
            })
        })
        .collect();

    drop(tx); // Close sender so receiver knows when all done

    // Collect results
    let mut results: Vec<TestResult> = rx.into_iter().collect();

    // Wait for all threads
    for handle in handles {
        let _ = handle.join();
    }

    // Sort results for consistent output
    results.sort_by(|a, b| {
        (a.server.name(), a.client.name()).cmp(&(b.server.name(), b.client.name()))
    });

    // Print results
    println!("Results:");
    println!("─────────────────────────────────────────────────────────────");

    let mut passed = 0;
    let mut failed = 0;

    for result in &results {
        let status = if result.passed {
            passed += 1;
            "\x1b[32m✓ PASS\x1b[0m"
        } else {
            failed += 1;
            "\x1b[31m✗ FAIL\x1b[0m"
        };

        println!(
            "  {:>10} → {:<10}  {}  ({})",
            result.client.name(),
            result.server.name(),
            status,
            format_duration(result.duration)
        );

        if let Some(ref err) = result.error {
            println!("    Error: {}", err);
        }
    }

    let total_duration = total_start.elapsed();

    println!();
    println!("═════════════════════════════════════════════════════════════");
    println!(
        "  Total: {} passed, {} failed in {}",
        passed,
        failed,
        format_duration(total_duration)
    );
    println!("═════════════════════════════════════════════════════════════");

    if failed > 0 {
        std::process::exit(1);
    }
}
