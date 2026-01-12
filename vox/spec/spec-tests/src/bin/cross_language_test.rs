//! Cross-language test matrix for roam RPC.
//!
//! Tests all client-server pairs across {Rust, TypeScript, Swift}.
//! Supports both TCP and WebSocket transports.
//! Runs tests in parallel for faster execution.

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Language {
    Rust,
    TypeScript,
    Swift,
}

impl Language {
    fn name(&self) -> &'static str {
        match self {
            Language::Rust => "Rust",
            Language::TypeScript => "TypeScript",
            Language::Swift => "Swift",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Transport {
    Tcp,
    WebSocket,
}

impl Transport {
    fn name(&self) -> &'static str {
        match self {
            Transport::Tcp => "TCP",
            Transport::WebSocket => "WS",
        }
    }
}

struct Server {
    child: Child,
    #[allow(dead_code)]
    port: u16,
}

impl Server {
    fn spawn(lang: Language, transport: Transport, port: u16) -> Result<Self, String> {
        let child = match (lang, transport) {
            (Language::Rust, Transport::Tcp) => Command::new("./target/release/tcp-echo-server")
                .env("TCP_PORT", port.to_string())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| format!("Failed to spawn Rust TCP server: {}", e))?,

            (Language::Rust, Transport::WebSocket) => {
                Command::new("./target/release/ws-echo-server")
                    .env("WS_PORT", port.to_string())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .spawn()
                    .map_err(|e| format!("Failed to spawn Rust WS server: {}", e))?
            }

            (Language::TypeScript, Transport::Tcp) => {
                return Err("TypeScript TCP server not implemented".to_string());
            }

            (Language::TypeScript, Transport::WebSocket) => {
                return Err("TypeScript WebSocket server not implemented".to_string());
            }

            (Language::Swift, Transport::Tcp) => Command::new("sh")
                .arg("swift/subject/subject-swift.sh")
                .env("TCP_PORT", port.to_string())
                .env("SUBJECT_MODE", "server")
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| format!("Failed to spawn Swift server: {}", e))?,

            (Language::Swift, Transport::WebSocket) => {
                return Err("Swift WebSocket server not implemented".to_string());
            }
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

fn run_client(lang: Language, transport: Transport, addr: &str) -> Result<bool, String> {
    let output = match (lang, transport) {
        (Language::Rust, Transport::Tcp) => Command::new("./target/release/tcp-echo-client")
            .env("SERVER_ADDR", addr)
            .output()
            .map_err(|e| format!("Failed to run Rust TCP client: {}", e))?,

        (Language::Rust, Transport::WebSocket) => {
            return Err("Rust WebSocket client not implemented".to_string());
        }

        (Language::TypeScript, Transport::Tcp) => {
            return Err("TypeScript TCP client not implemented".to_string());
        }

        (Language::TypeScript, Transport::WebSocket) => {
            return Err("TypeScript WebSocket client not implemented".to_string());
        }

        (Language::Swift, Transport::Tcp) => {
            return Err("Swift TCP client not implemented".to_string());
        }

        (Language::Swift, Transport::WebSocket) => {
            return Err("Swift WebSocket client not implemented".to_string());
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.contains("All tests passed"))
}

#[derive(Debug)]
struct TestResult {
    server: Language,
    client: Language,
    transport: Transport,
    passed: bool,
    error: Option<String>,
    duration: Duration,
}

fn run_test(
    server_lang: Language,
    client_lang: Language,
    transport: Transport,
    port: u16,
) -> TestResult {
    let start = Instant::now();

    // Spawn server
    let mut server = match Server::spawn(server_lang, transport, port) {
        Ok(s) => s,
        Err(e) => {
            return TestResult {
                server: server_lang,
                client: client_lang,
                transport,
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
            transport,
            passed: false,
            error: Some(e),
            duration: start.elapsed(),
        };
    }

    // Build address based on transport
    let addr = match transport {
        Transport::Tcp => format!("127.0.0.1:{}", port),
        Transport::WebSocket => format!("ws://127.0.0.1:{}", port),
    };

    // Run client
    let passed = match run_client(client_lang, transport, &addr) {
        Ok(p) => p,
        Err(e) => {
            return TestResult {
                server: server_lang,
                client: client_lang,
                transport,
                passed: false,
                error: Some(e),
                duration: start.elapsed(),
            };
        }
    };

    TestResult {
        server: server_lang,
        client: client_lang,
        transport,
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

    // TCP tests: 3×3 = 9 (Rust, TypeScript, Swift)
    let tcp_servers = [Language::Rust, Language::TypeScript, Language::Swift];
    let tcp_clients = [Language::Rust, Language::TypeScript, Language::Swift];

    // WebSocket tests: Rust server × TypeScript client = 1
    // (expandable as more implementations are added)
    let ws_servers = [Language::Rust];
    let ws_clients = [Language::TypeScript];

    let tcp_count = tcp_servers.len() * tcp_clients.len();
    let ws_count = ws_servers.len() * ws_clients.len();
    let total_count = tcp_count + ws_count;

    println!("╔════════════════════════════════════════════════════════════╗");
    println!(
        "║     Cross-Language Test Matrix ({} TCP + {} WS = {} tests)      ║",
        tcp_count, ws_count, total_count
    );
    println!("╚════════════════════════════════════════════════════════════╝");
    println!();

    // Create test pairs with unique ports
    let mut tests: Vec<(Language, Language, Transport, u16)> = Vec::new();
    let mut port = 9300u16;

    // TCP tests
    for &server in &tcp_servers {
        for &client in &tcp_clients {
            tests.push((server, client, Transport::Tcp, port));
            port += 1;
        }
    }

    // WebSocket tests
    for &server in &ws_servers {
        for &client in &ws_clients {
            tests.push((server, client, Transport::WebSocket, port));
            port += 1;
        }
    }

    // Run tests in parallel using threads
    let (tx, rx) = mpsc::channel::<TestResult>();

    let handles: Vec<_> = tests
        .into_iter()
        .map(|(server, client, transport, port)| {
            let tx = tx.clone();
            thread::spawn(move || {
                let result = run_test(server, client, transport, port);
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

    // Sort results: TCP first, then WS; within each, by server then client
    results.sort_by(|a, b| {
        (a.transport.name(), a.server.name(), a.client.name()).cmp(&(
            b.transport.name(),
            b.server.name(),
            b.client.name(),
        ))
    });

    // Print TCP results
    println!("TCP Results:");
    println!("─────────────────────────────────────────────────────────────");

    let mut passed = 0;
    let mut failed = 0;

    for result in results.iter().filter(|r| r.transport == Transport::Tcp) {
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

    // Print WebSocket results
    println!();
    println!("WebSocket Results:");
    println!("─────────────────────────────────────────────────────────────");

    for result in results
        .iter()
        .filter(|r| r.transport == Transport::WebSocket)
    {
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
