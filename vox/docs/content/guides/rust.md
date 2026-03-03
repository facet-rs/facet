+++
title = "Rust Guide"
description = "How to define services, wire runtime/session/transport, and export descriptors for codegen."
weight = 21
+++

Rust is the source of truth in Roam: service traits define the schema, and the macro generates descriptors and Rust client/server glue.

## 1) Define a proto crate

Create a `*-proto` crate that only contains service traits and shared types.

```toml
# my-proto/Cargo.toml
[dependencies]
roam = "7.0.0"
facet = "0.44"
```

```rust
// my-proto/src/lib.rs
use roam::service;

#[service]
pub trait Greeter {
    async fn hello(&self, name: String) -> String;
}
```

The macro generates:

- Rust client (`GreeterClient`)
- Rust dispatcher (`GreeterDispatcher`)
- Descriptor function (`greeter_service_descriptor() -> &'static ServiceDescriptor`)

## 2) Wire a server runtime

```toml
# my-server/Cargo.toml
[dependencies]
my-proto = { path = "../my-proto" }
roam-core = "7.0.0"
roam-stream = "7.0.0"
roam-types = "7.0.0"
tokio = { version = "1", features = ["rt-multi-thread", "macros", "net"] }
```

```rust
use my_proto::{Greeter, GreeterDispatcher};
use roam_core::{BareConduit, Driver, initiator};
use roam_stream::StreamLink;
use roam_types::{MessageFamily, Parity};

#[derive(Clone)]
struct GreeterService;

impl Greeter for GreeterService {
    async fn hello(&self, name: String) -> String {
        format!("hello, {name}")
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let stream = tokio::net::TcpStream::connect("127.0.0.1:9000").await?;

    let conduit = BareConduit::<MessageFamily, _>::new(StreamLink::tcp(stream));
    let (mut session, handle, _) = initiator(conduit).establish().await?;

    let dispatcher = GreeterDispatcher::new(GreeterService);
    let mut driver = Driver::new(handle, dispatcher, Parity::Odd);

    tokio::spawn(async move { session.run().await });
    driver.run().await;
    Ok(())
}
```

## 3) Build Rust clients

Generated Rust clients are created from `driver.caller()`.

```rust
use my_proto::GreeterClient;

let caller = driver.caller();
let client = GreeterClient::new(caller);
let reply = client.hello("world".to_string()).await?;
```

## 4) Generate non-Rust bindings (optional)

You do not need any Roam-internal task runner for this. Call `roam-codegen` directly from your own `build.rs` or generator binary.

```toml
# my-bindings/Cargo.toml
[build-dependencies]
my-proto = { path = "../my-proto" }
roam-codegen = "7.0.0"
```

```rust
// my-bindings/build.rs
fn main() {
    let svc = my_proto::greeter_service_descriptor();

    let ts = roam_codegen::targets::typescript::generate_service(svc);
    std::fs::write("../typescript/generated/greeter.ts", ts).unwrap();

    let swift = roam_codegen::targets::swift::generate_service_with_bindings(
        svc,
        roam_codegen::targets::swift::SwiftBindings::Client,
    );
    std::fs::write("../swift/generated/GreeterClient.swift", swift).unwrap();
}
```

For full API migration notes from v6, see [Migrating from v6 to v7](/v6-to-v7/).
