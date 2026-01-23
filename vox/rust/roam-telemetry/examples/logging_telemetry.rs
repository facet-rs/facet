//! Example showing TelemetryMiddleware with LoggingExporter.
//!
//! Run with: cargo run -p roam-telemetry --example logging_telemetry

use std::io;
use std::time::Duration;

use roam_stream::{Connector, HandshakeConfig, NoDispatcher, accept, connect};
use roam_telemetry::{LoggingExporter, TelemetryMiddleware};
use tokio::net::{TcpListener, TcpStream};
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

// Custom struct argument
#[derive(Clone, facet::Facet)]
struct Person {
    name: String,
    age: u32,
    #[facet(sensitive)]
    email: Option<String>,
}

// Define a simple service
#[roam::service]
trait Calculator {
    async fn add(&self, a: i32, b: i32) -> i32;
    async fn multiply(&self, x: i32, y: i32) -> i32;
    async fn greet(&self, name: String, age: u32) -> String;
    async fn register(&self, person: Person) -> String;
}

// Implement the service
#[derive(Clone)]
struct CalculatorService;

impl Calculator for CalculatorService {
    async fn add(&self, _cx: &roam::Context, a: i32, b: i32) -> i32 {
        a + b
    }

    async fn multiply(&self, _cx: &roam::Context, x: i32, y: i32) -> i32 {
        x * y
    }

    async fn greet(&self, _cx: &roam::Context, name: String, age: u32) -> String {
        format!("Hello {}, you are {} years old!", name, age)
    }

    async fn register(&self, _cx: &roam::Context, person: Person) -> String {
        format!("Registered: {}", person.name)
    }
}

// TCP connector for the client
struct TcpConnector {
    addr: String,
}

impl Connector for TcpConnector {
    type Transport = TcpStream;

    async fn connect(&self) -> io::Result<TcpStream> {
        TcpStream::connect(&self.addr).await
    }
}

#[tokio::main]
async fn main() {
    // Set up tracing to see the telemetry output
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::new("roam_telemetry=info"))
        .init();

    println!("=== roam-telemetry LoggingExporter Example ===\n");

    // Start server
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    println!("Server listening on {}", addr);

    // Spawn server task
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            // Create a logging exporter (prints spans to console)
            let exporter = LoggingExporter::new("calculator-service");

            // Create the telemetry middleware
            let telemetry = TelemetryMiddleware::new(exporter);

            // Create dispatcher with telemetry middleware
            let dispatcher =
                CalculatorDispatcher::new(CalculatorService).with_middleware(telemetry);

            tokio::spawn(async move {
                if let Ok((handle, _incoming, driver)) =
                    accept(stream, HandshakeConfig::default(), dispatcher).await
                {
                    let _ = driver.run().await;
                    drop(handle);
                }
            });
        }
    });

    // Give server time to start
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Connect client
    let connector = TcpConnector {
        addr: addr.to_string(),
    };
    let client = connect(connector, HandshakeConfig::default(), NoDispatcher);
    let calculator = CalculatorClient::new(client);

    println!("\nMaking RPC calls...\n");

    // Make some calls - each will produce a telemetry span
    let result = calculator.add(5, 3).await.unwrap();
    println!("add(5, 3) = {}", result);

    let result = calculator.multiply(7, 6).await.unwrap();
    println!("multiply(7, 6) = {}", result);

    let result = calculator.greet("Alice".to_string(), 30).await.unwrap();
    println!("greet(\"Alice\", 30) = {}", result);

    // Call with a custom struct containing a sensitive field
    let person = Person {
        name: "Bob".to_string(),
        age: 25,
        email: Some("bob@secret.com".to_string()),
    };
    let result = calculator.register(person).await.unwrap();
    println!("register(Person {{ ... }}) = {}", result);

    println!("\n=== Done ===");
    println!("\nThe INFO lines above show the telemetry spans with:");
    println!("  - trace_id and span_id");
    println!("  - duration in milliseconds");
    println!("  - per-argument attributes (rpc.args.a, rpc.args.name, etc.)");
}
