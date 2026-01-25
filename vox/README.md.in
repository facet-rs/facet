# roam

A Rust-native RPC protocol that's going places.

And remember: roam wasn't built in a day.

## What is roam?

roam is a **Rust-native** RPC protocol. Rust is the lowest common denominator — there's no independent schema language. Rust traits *are* the schema:

```rust
#[roam::service]
pub trait Calculator {
    /// Infallible method — just returns a value
    async fn add(&self, a: i32, b: i32) -> i32;

    /// Fallible method — returns Result<T, E>
    async fn divide(&self, a: i32, b: i32) -> Result<i32, MathError>;

    /// Streaming: client sends numbers, server returns sum
    async fn sum(&self, numbers: Rx<i32>) -> i64;

    /// Streaming: server sends numbers to client
    async fn generate(&self, count: u32, output: Tx<i32>);

    /// Bidirectional streaming
    async fn transform(&self, input: Rx<String>, output: Tx<String>);
}
```

Implementations for other languages (TypeScript, Swift) are **generated from Rust definitions** using Rust tooling.

## Implementing a Service

```rust
impl Calculator for MyCalculator {
    async fn add(&self, _cx: &Context, a: i32, b: i32) -> i32 {
        a + b
    }

    async fn divide(&self, _cx: &Context, a: i32, b: i32) -> Result<i32, MathError> {
        if b == 0 {
            Err(MathError::DivisionByZero)
        } else {
            Ok(a / b)
        }
    }

    async fn sum(&self, _cx: &Context, mut numbers: Rx<i32>) -> i64 {
        let mut total: i64 = 0;
        while let Some(n) = numbers.recv().await.ok().flatten() {
            total += n as i64;
        }
        total
    }
    // ...
}
```

## Using a Client

```rust
let client = CalculatorClient::new(connection_handle);

// Simple call
let result = client.add(2, 3).await?;

// Fallible call
match client.divide(10, 0).await? {
    Ok(result) => println!("Result: {result}"),
    Err(MathError::DivisionByZero) => println!("Cannot divide by zero"),
}

// With metadata
let result = client.add(2, 3)
    .with_metadata(vec![("request-id".into(), "abc123".into())])
    .await?;
```

## Middleware

Middleware intercepts requests before and after the handler:

```rust
struct AuthMiddleware { /* ... */ }

impl Middleware for AuthMiddleware {
    fn pre<'a>(
        &'a self,
        ctx: &'a mut Context,
        _args: SendPeek<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<(), Rejection>> + Send + 'a>> {
        Box::pin(async move {
            let token = ctx.metadata().get("auth-token");
            match validate_token(token) {
                Ok(user) => {
                    ctx.extensions.insert(user);
                    Ok(())
                }
                Err(_) => Err(Rejection::unauthenticated("invalid token")),
            }
        })
    }
}

// Add to dispatcher
let dispatcher = CalculatorDispatcher::new(handler)
    .with_middleware(AuthMiddleware::new());
```

## Observability

Built-in OpenTelemetry integration with distributed tracing:

```rust
use roam_telemetry::{TelemetryMiddleware, OtlpExporter, TracingCaller};

// Server side: export spans to Tempo/Jaeger
let exporter = OtlpExporter::new("http://tempo:4318/v1/traces", "my-service");
let dispatcher = CalculatorDispatcher::new(handler)
    .with_middleware(TelemetryMiddleware::new(exporter.clone()));

// Client side: automatic trace propagation
let caller = TracingCaller::new(connection_handle, exporter);
let client = DownstreamClient::new(caller);
// Calls automatically inject traceparent headers
```

## Features

- **Bidirectional RPC** — Request/response with correlation
- **Streaming channels** — `Tx<T>`/`Rx<T>` with credit-based flow control
- **Virtual connections** — Multiple independent contexts on a single link
- **Transport-agnostic** — TCP, WebSocket, shared memory, with QUIC/WebTransport planned
- **Type-safe** — [Facet](https://facet.rs)-based serialization

## Language Support

| Language | Status |
|----------|--------|
| Rust | Reference implementation |
| TypeScript | Generated client/server |
| Swift | Generated client/server |

## Transport Bindings

| Transport | Framing | Status |
|-----------|---------|--------|
| TCP / Unix sockets | COBS | ✓ |
| WebSocket | Binary frames | ✓ |
| Shared Memory Hub | Lock-free rings | ✓ |
| HTTP Bridge | WebSocket upgrade | ✓ |
| QUIC / WebTransport | Native streams | Planned |

## Project Structure

```
rust/           # Rust implementation (roam, roam-wire, roam-session, etc.)
typescript/     # TypeScript packages (roam-core, roam-tcp, roam-ws)
swift/          # Swift packages
spec/           # Compliance test suite
docs/           # Specifications
```

## Quick Start

```bash
# Run Rust compliance tests
just rust

# Run TypeScript compliance tests  
just ts

# Run all language tests
just all
```

## Specification

Read the [spec](docs/content/spec/_index.md) for the formal protocol definition.

## License

MIT OR Apache-2.0
