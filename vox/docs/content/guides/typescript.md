+++
title = "TypeScript Guide"
description = "How to generate TypeScript bindings from Rust descriptors and wire clients/servers with Roam runtime packages."
weight = 23
+++

TypeScript usage in Roam has two parts:

- runtime packages (`@bearcove/roam-core`, transports, wire/serialization)
- generated service bindings (client, handler interface, dispatcher, descriptor)

## 1) Runtime dependencies

For a Node server/client setup, start with:

```json
{
  "dependencies": {
    "@bearcove/roam-core": "7.0.0",
    "@bearcove/roam-tcp": "7.0.0",
    "@bearcove/roam-ws": "7.0.0",
    "@bearcove/roam-wire": "7.0.0",
    "@bearcove/roam-postcard": "7.0.0"
  }
}
```

Generated files in this repository import from `@bearcove/roam-core` and `@bearcove/roam-ws`, and may rely on channel/schema runtime pieces provided by the core/postcard/wire stack.

## 2) Generate TypeScript bindings from Rust

Use `roam-codegen` directly from your own generator (for example a Rust `build.rs`):

```rust
// build.rs
fn main() {
    let svc = my_proto::greeter_service_descriptor();
    let ts = roam_codegen::targets::typescript::generate_service(svc);
    std::fs::write("../typescript/generated/greeter.ts", ts).unwrap();
}
```

Typical output file contains:

- `GreeterClient`
- `GreeterHandler` interface
- `GreeterDispatcher`
- `greeter_descriptor`

## 3) Use the generated client

```ts
import { connectGreeter } from "@acme/roam-generated/greeter.ts";

const client = await connectGreeter("ws://127.0.0.1:9000");
const msg = await client.hello("world");
console.log(msg);
```

If your generated module does not expose a `connect*` helper for your transport, create a connection with runtime transport APIs and pass `connection.asCaller()` into `new GreeterClient(...)`.

## 4) Use the generated dispatcher on the server

```ts
import { Server } from "@bearcove/roam-tcp";
import { GreeterDispatcher, type GreeterHandler } from "@acme/roam-generated/greeter.ts";

class GreeterService implements GreeterHandler {
  hello(name: string): string {
    return `hello, ${name}`;
  }
}

const server = new Server();
const conn = await server.connect("127.0.0.1:9000", { acceptConnections: true });
await conn.runChanneling(new GreeterDispatcher(new GreeterService()));
```

## 5) Workspace/publishing layout

A common layout is:

- `typescript/generated/*.ts` for generated service bindings
- a small npm package (for example `@acme/roam-generated`) exporting those files
- app/service packages depending on both generated bindings and runtime packages

Keep generated package versions aligned with the Roam protocol/runtime major version.
