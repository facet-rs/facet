+++
title = "Rust Guide"
description = "Use roam + a transport crate to define services, run drivers, and call methods with channels."
weight = 21
+++

The fastest way to learn the Rust API is the repository example:

- Source: [rust-examples/examples/borrowed_and_channels.rs](https://github.com/bearcove/roam/blob/main/rust-examples/examples/borrowed_and_channels.rs)
- Run it: `cargo run -p rust-examples --example borrowed_and_channels`

This example covers the three core method shapes you usually need:

- Borrowed argument: `&str`
- Borrowed return: `&'roam str`
- Channel args: `Rx<T>` and `Tx<T>`

## 1) Start with `roam` + transport

You usually only need `roam` and one transport crate.

```toml
[dependencies]
roam = "7.0.0"
roam-stream = "7.0.0"
tokio = { version = "1", features = ["rt", "net"] }
eyre = "0.6"
```

`roam` re-exports the runtime/session types used in app code (`Driver`, `initiator`, `acceptor`, `Parity`, `Call`, `Rx`, `Tx`, `channel`, etc.).

## 2) Define your service trait

```rust
#[roam::service]
trait WordLab {
    async fn is_short(&self, word: &str) -> bool;
    async fn classify(&self, word: String) -> &'roam str;
    async fn transform(&self, prefix: &str, input: roam::Rx<String>, output: roam::Tx<String>) -> u32;
}
```

The macro generates:

- `WordLabClient`
- `WordLabDispatcher`
- `word_lab_service_descriptor() -> &'static ServiceDescriptor`

For borrowed returns, the generated trait impl method takes a `Call` parameter:

```rust
async fn classify<'roam>(
    &self,
    call: impl roam::Call<'roam, &'roam str, std::convert::Infallible>,
    word: String,
) {
    call.ok("short").await;
}
```

## 3) Wire session + driver with TCP

```rust
use roam_stream::StreamLink;

let conduit = roam::BareConduit::<roam::MessageFamily, _>::new(StreamLink::tcp(socket));
let (mut session, handle, _) = roam::initiator(conduit).establish().await?;
let mut driver = roam::Driver::new(handle, (), roam::Parity::Odd);
let caller = driver.caller();
let client = WordLabClient::new(caller);
```

On the server side, use `roam::acceptor(...)`, then build a driver with `WordLabDispatcher::new(your_service)`.

Run both loops:

- `session.run()` handles frames
- `driver.run()` handles method dispatch and replies

## 4) Use channels in method calls

```rust
let (input_tx, input_rx) = roam::channel::<String>();
let (output_tx, mut output_rx) = roam::channel::<String>();

let count = client.transform("item", input_rx, output_tx).await?;

while let Some(item) = output_rx.recv().await? {
    println!("{item}");
}
```

This pattern is shown end-to-end in the example, including channel close behavior.

## 5) Generate TypeScript/Swift from Rust descriptors

If you want non-Rust bindings, use the generated service descriptor with `roam-codegen`.

```rust
let svc = my_proto::word_lab_service_descriptor();

let ts = roam_codegen::targets::typescript::generate_service(svc);
let swift = roam_codegen::targets::swift::generate_service_with_bindings(
    svc,
    roam_codegen::targets::swift::SwiftBindings::Client,
);
```

For migration details, see [Migrating from v6 to v7](/v6-to-v7/).
