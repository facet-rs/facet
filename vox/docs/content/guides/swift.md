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

## 1.5) Build and test

From the Vox workspace root:

```bash
swift test --no-parallel
```

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

`GreeterClient` takes a `VoxLane`. A `Connection` owns the driven Vox runtime
over one link; service clients are built on lanes opened from that connection.

```swift
import Foundation
import VoxRuntime

let connection = try await Connection.connect(TcpConnector(host: "127.0.0.1", port: 9000))
let driver = Task {
    try await connection.run()
}

let lane = try await connection.openLane(
    settings: ConnectionSettings(parity: .even, maxConcurrentRequests: 64, initialChannelCredit: 16),
    metadata: emptyMetadata().metaSetting("vox-service", .string("Greeter"))
)
let client = GreeterClient(connection: lane)
let reply = try await client.hello(name: "world")
print(reply)

connection.shutdown()
try await driver.value
```

## 4) Connection and lane policy

Handshake metadata carries early peer-authored claims. An identity resolver
verifies those claims locally and either returns the immutable connection
identity or throws `ConnectionDeclinedError`, which sends `Decline` during the
handshake:

Connector side:

```swift
let authMetadata = emptyMetadata()
    .metaSetting("-#authorization", .string("Bearer local-dev"))

let clientConnection = try await Connection.connect(
    TcpConnector(host: "127.0.0.1", port: 9000),
    metadata: authMetadata
)
```

Acceptor side:

```swift
let serverConnection = try await Connection.accept(
    TcpAcceptor(host: "127.0.0.1", port: 9000),
    identityResolver: { context in
        guard context.claims.metaStr("-#authorization") == "Bearer local-dev" else {
            throw ConnectionDeclinedError(reason: .unauthenticated)
        }
        return PeerIdentity.fromBasis(
            IdentityBasis(
                form: .applicationUser,
                provenance: .verifiedClaimBacked,
                redacted: "local-dev-user"
            )
        )
    }
)
```

The peer that sends metadata does not verify its own metadata. In this example,
the connector authors the early claim and the acceptor resolves the connector's
identity from the peer claims it received. A connector-side resolver can also
verify acceptor metadata or transport evidence, but it is still verifying the
peer.

Late credentials in lane or request metadata do not rewrite the connection
identity. Verify them in lane/request policy and record the result in a lane
grant:

```swift
struct GreeterLaneAcceptor: LaneAcceptor {
    let dispatcher: any ServiceDispatcher

    func accept(request: LaneRequest, lane: PendingLane) {
        guard request.peerIdentity.form == .applicationUser else {
            lane.reject(.withMessage(.forbidden, "connection is not authenticated"))
            return
        }

        guard request.service == "Greeter" else {
            lane.reject(.withMessage(.unknownService, "unknown service"))
            return
        }

        var grant = emptyMetadata()
        grant.metaSet("tenant", .string("lab"))
        grant.metaSet("grant-scope", .string("greeter:read"))
        grant.metaSet(
            "authenticated-peer",
            .string(request.peerIdentity.bases.first?.redacted ?? "unknown")
        )
        lane.handleWith(dispatcher, grant: LaneGrant(metadata: grant))
    }
}
```

Generated handlers that receive `RequestContext` can read the grant from
`context.authorization.laneGrant`.

## 5) Wire a Swift server

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

Then establish as acceptor and run the connection:

```swift
let connection = try await Connection.accept(
    TcpAcceptor(host: "127.0.0.1", port: 9000),
    onLane: GreeterLaneAcceptor(
        dispatcher: GreeterDispatcherAdapter(handler: MyGreeterService())
    )
)
try await connection.run()
```

## 6) Channel lifetime

Generated Swift clients and dispatchers use `Tx<T>`/`Rx<T>` for raw Vox
channels. Those channels are request-scoped sidebands: start the call that binds
the channel, then drive channel send/receive work concurrently with that call.
The method response terminates the request scope, so channel data that matters
must be sent and drained before, or as part of, the response. Durable or
resumable streams belong in explicit service-level protocols, not raw channels.

## 7) Keep codegen and runtime versions aligned

Generated Swift code assumes the same protocol/runtime major version as your Rust descriptors and `VoxRuntime`.
