+++
title = "Swift Guide"
description = "How to generate Swift bindings from Rust descriptors and wire them with VoxRuntime."
weight = 22
+++

Swift usage in Vox is descriptor-driven: define services in Rust, generate Swift code, then run it against `VoxRuntime`.

## 1) Add runtime dependency

In your Swift package:

```swift
// Package.swift
.dependencies([
  .package(path: "../vox-runtime")
]),
.targets([
  .executableTarget(
    name: "my-app",
    dependencies: [
      .product(name: "VoxRuntime", package: "vox-runtime")
    ]
  )
])
```

Generated Swift files import:

- `Foundation`
- `VoxRuntime`

## 1.5) Build the Rust SHM staticlib before testing

`VoxRuntime` links against `libvox_shm_ffi.a`, which is produced by the Rust
workspace.

From the Vox workspace root:

```bash
cargo build --release -p vox-shm-ffi
swift test --no-parallel -Xlinker -L$(pwd)/target/release
```

That root-level `swift test` command is the same validation path used in CI.

## 2) Generate Swift bindings from Rust

Use `vox-codegen` directly from your own Rust generator/build step:

```rust
// build.rs
fn main() {
    let svc = my_proto::greeter_service_descriptor();

    let code = vox_codegen::targets::swift::generate_service_with_bindings(
        svc,
        vox_codegen::targets::swift::SwiftBindings::ClientAndServer,
    );

    std::fs::write("../swift/Sources/MyApp/Greeter.swift", code).unwrap();
}
```

Generated Swift output includes:

- `GreeterCaller` protocol
- `GreeterClient`
- `GreeterHandler` protocol
- `GreeterChannelingDispatcher`
- method IDs and schema helpers

## 3) Wire a Swift client

`GreeterClient` takes a `VoxConnection`.

```swift
import Foundation
import VoxRuntime

struct NoopDispatcher: ServiceDispatcher {
    func preregister(methodId: UInt64, payload: [UInt8], channels: [UInt64], registry: ChannelRegistry) async {}
    func dispatch(
        methodId: UInt64,
        payload: [UInt8],
        channels: [UInt64],
        requestId: UInt64,
        registry: ChannelRegistry,
        taskTx: @escaping @Sendable (TaskMessage) -> Void
    ) async {}
}

let transport = try await connect(host: "127.0.0.1", port: 9000)
let (handle, driver) = try await establishInitiator(
    transport: transport,
    dispatcher: NoopDispatcher(),
    acceptConnections: false
)

Task {
    try await driver.run()
}

let client = GreeterClient(connection: handle)
let reply = try await client.hello(name: "world")
print(reply)
```

## 4) Wire a Swift server

`GreeterChannelingDispatcher` is generated per service; wrap it in a `ServiceDispatcher` adapter.

```swift
final class GreeterDispatcherAdapter: ServiceDispatcher {
    private let handler: GreeterHandler

    init(handler: GreeterHandler) {
        self.handler = handler
    }

    func preregister(methodId: UInt64, payload: [UInt8], channels: [UInt64], registry: ChannelRegistry) async {
        await GreeterChannelingDispatcher.preregisterChannels(
            methodId: methodId,
            channels: channels,
            registry: registry
        )
    }

    func dispatch(
        methodId: UInt64,
        payload: [UInt8],
        channels: [UInt64],
        requestId: UInt64,
        registry: ChannelRegistry,
        taskTx: @escaping @Sendable (TaskMessage) -> Void
    ) async {
        let dispatcher = GreeterChannelingDispatcher(
            handler: handler,
            registry: registry,
            taskSender: taskTx
        )
        await dispatcher.dispatch(
            methodId: methodId,
            requestId: requestId,
            channels: channels,
            payload: Data(payload)
        )
    }
}
```

Then establish as acceptor and run the driver:

```swift
let transport = try await connect(host: "127.0.0.1", port: 9000)
let (_, driver) = try await establishAcceptor(
    transport: transport,
    dispatcher: GreeterDispatcherAdapter(handler: MyGreeterService()),
    acceptConnections: true
)
try await driver.run()
```

## 5) Keep codegen and runtime versions aligned

Generated Swift code assumes the same protocol/runtime major version as your Rust descriptors and `VoxRuntime`.
