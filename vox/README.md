# vox

[![crates.io](https://img.shields.io/crates/v/vox.svg)](https://crates.io/crates/vox)
[![documentation](https://docs.rs/vox/badge.svg)](https://docs.rs/vox)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/vox.svg)](https://github.com/facet-rs/facet/blob/main/LICENSE-MIT)

vox (formerly **roam**) is a Rust-native RPC framework where Rust traits *are*
the schema. There is
no separate IDL, no code-generation pipeline to wire up: you define an async
trait, annotate it with `#[vox::service]`, and get a type-safe client and
dispatcher that serializes with [Facet](https://facet.rs). Implementations for
TypeScript and Swift are generated from those same Rust definitions.

```rust
#[vox::service]
pub trait Calculator {
    async fn add(&self, a: i32, b: i32) -> i32;
    async fn divide(&self, a: i32, b: i32) -> Result<i32, MathError>;

    /// Client sends numbers, server returns the running sum.
    async fn sum(&self, numbers: Rx<i32>) -> i64;

    /// Server streams results back to the client.
    async fn generate(&self, count: u32, output: Tx<i32>);
}
```

## Implementing a service

```rust
impl Calculator for MyCalculator {
    async fn add(&self, _cx: &Context, a: i32, b: i32) -> i32 {
        a + b
    }

    async fn divide(&self, _cx: &Context, a: i32, b: i32) -> Result<i32, MathError> {
        if b == 0 { Err(MathError::DivisionByZero) } else { Ok(a / b) }
    }

    async fn sum(&self, _cx: &Context, mut numbers: Rx<i32>) -> i64 {
        let mut total = 0i64;
        while let Some(n) = numbers.recv().await.ok().flatten() {
            total += n as i64;
        }
        total
    }
    // ...
}
```

## Using a client

```rust
let client = CalculatorClient::new(connection_handle);

let result = client.add(2, 3).await?;

match client.divide(10, 0).await? {
    Ok(r)  => println!("result: {r}"),
    Err(MathError::DivisionByZero) => println!("cannot divide by zero"),
}
```

## Features

- **Bidirectional RPC** — request/response with correlation ids
- **Channels** — `Tx<T>` / `Rx<T>` with credit-based flow control
- **Service lanes** — multiple independently routed services on one connection
- **Transport-agnostic** — TCP, WebSocket, Unix sockets, and shared-memory hub
- **Middleware** — intercept requests for auth, tracing, rate-limiting, etc.
- **OpenTelemetry** — built-in distributed tracing via `vox_telemetry`

## Language support

| Language   | Status                    |
|------------|---------------------------|
| Rust       | Reference implementation  |
| TypeScript | Generated client/server   |
| Swift      | Generated client/server   |

## Specification

See the [spec](https://github.com/facet-rs/facet/tree/main/vox/docs/content/spec) for the formal protocol definition.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](https://github.com/facet-rs/facet/blob/main/LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](https://github.com/facet-rs/facet/blob/main/LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
