+++
title = "TypeScript Guide"
description = "How to generate TypeScript bindings from Rust descriptors and wire clients/servers with Vox runtime packages."
weight = 23
+++

TypeScript usage in Vox has two parts:

- runtime packages (`@bearcove/vox-core`, transports, wire/serialization)
- generated service bindings (client, handler interface, dispatcher, descriptor)

## 1) Runtime dependencies

For a Node server/client setup, start with:

```json
{
  "dependencies": {
    "@bearcove/vox-core": "7.0.0",
    "@bearcove/vox-tcp": "7.0.0",
    "@bearcove/vox-ws": "7.0.0",
    "@bearcove/vox-wire": "7.0.0"
  }
}
```

Generated files import from `@bearcove/vox-core` and transport packages. The
wire package handles low-level phon encoding and is typically not used directly.

## 2) Generate TypeScript bindings from Rust

Use `vox-codegen` from a `build.rs` or code-generation script:

```rust
// build.rs
fn main() {
    let svc = my_proto::greeter_service_descriptor();
    let ts = vox_codegen::targets::typescript::generate_service(svc);
    std::fs::write("../typescript/generated/greeter.ts", ts).unwrap();
}
```

The output contains:

- `GreeterClient` — typed caller
- `GreeterHandler` interface — implement this on the server
- `GreeterDispatcher` — wires a `GreeterHandler` into a service lane
- `greeter_descriptor` — schema and method metadata used by the runtime

## 3) Use the generated client

```ts
import { connect } from "@bearcove/vox-core";
import { wsConnector } from "@bearcove/vox-ws";
import { GreeterClient } from "@acme/vox-generated/greeter.ts";

const conn = await connect(wsConnector("ws://127.0.0.1:9000"));
const client = await conn.openLane(GreeterClient);

const msg = await client.hello("world");
console.log(msg);
```

If the generated module exposes a `connectGreeter` helper (for WebSocket), you
can also use that lane-opening shorthand directly.

## 4) Use the generated dispatcher on the server

```ts
import net from "node:net";
import { accept } from "@bearcove/vox-core";
import { acceptTcp } from "@bearcove/vox-tcp";
import { Driver } from "@bearcove/vox-core";
import { GreeterDispatcher, type GreeterHandler } from "@acme/vox-generated/greeter.ts";

class GreeterService implements GreeterHandler {
  hello(name: string): string {
    return `hello, ${name}`;
  }
}

const server = net.createServer((socket) => {
  void (async () => {
    const conn = await accept(acceptTcp(socket), {
      onLane: async (_request, pending) => {
        const lane = await pending.accept();
        const driver = new Driver(
          lane,
          new GreeterDispatcher(new GreeterService()),
        );
        void driver.run().catch((error) => {
          console.error("Vox service lane failed", error);
        });
      },
    });
    await conn.closed();
  })();
});

server.listen(9000, "127.0.0.1");
```

`onLane` receives the peer's `LaneRequest` plus a `PendingLane`. Accept the
pending lane to attach a dispatcher, optionally with a lane grant; reject it to
return a structured lane-open policy error.

## 5) Connection loss and keepalive

Vox connections are bound to one link attachment. If that attachment breaks,
the connection ends and in-flight request attempts fail. Vox does not resume the
connection, replay requests, or automatically issue replacement calls.

Applications that want to recover after attachment loss should establish a new
connection and issue new calls explicitly.

### Keepalive for silent drops

By default, Vox relies on the transport to surface a closed connection. TCP and
browser WebSocket connections are usually reliable, but certain environments
(mobile networks, HTTP proxies, NAT gateways) can silently discard packets
without sending a FIN or RST — leaving the connection open on both sides while
data can no longer flow.

Enable keepalive to catch these silent drops:

```ts
const conn = await connect(wsConnector("ws://api.example.com"), {
  keepaliveIntervalMs: 15_000,   // send a Ping every 15 s
  keepaliveTimeoutMs:   5_000,   // give up if no Pong within 5 s
});
```

Vox sends a protocol-level `Ping` message every `keepaliveIntervalMs`. If the
peer does not reply with a `Pong` within `keepaliveTimeoutMs` (default: half
the interval), the connection is forcibly closed.

Choose values appropriate for your environment:

| Environment | `keepaliveIntervalMs` | `keepaliveTimeoutMs` |
|---|---|---|
| Desktop browser | 30 000 | 10 000 |
| Mobile browser | 15 000 | 5 000 |
| Server-to-server | 60 000 | 20 000 |

Setting `keepaliveIntervalMs` without setting `keepaliveTimeoutMs` uses half
the interval as the timeout.

### Full production example

```ts
import { connect } from "@bearcove/vox-core";
import { wsConnector } from "@bearcove/vox-ws";
import { ApiClient } from "@acme/generated/api.ts";

async function connectApi(): Promise<ApiClient> {
  const conn = await connect(
    wsConnector("wss://api.example.com"),
    {
      // Detect silent drops (important on mobile).
      keepaliveIntervalMs: 15_000,
      keepaliveTimeoutMs:   5_000,
    },
  );

  return conn.openLane(ApiClient);
}
```

## 6) Channels (streaming)

Vox supports bidirectional streaming via typed raw channels. Channels are
created as a `(Tx, Rx)` pair and one end is passed to a method call; the
request scope that introduced the channel owns the lifetime.

**Important:** always initiate the call *before* consuming the channel. The
channel is bound (becomes usable) when it is passed to a method, not when
`channel()` is called.

```ts
import { channel } from "@bearcove/vox-core";

// Server streams numbers to the client.
// Client gives the Tx end to the server and reads from Rx.
const [tx, rx] = channel<number>();
const callPromise = client.generate(100, tx); // bind tx — rx is now usable

const received: number[] = [];
const drain = (async () => {
  for await (const n of rx) received.push(n);
})();

await Promise.all([callPromise, drain]);
```

```ts
// Client sends numbers to the server (server has Rx, client keeps Tx).
const [tx, rx] = channel<number>();
const callPromise = client.sum(rx); // bind rx — tx is now usable

for (let i = 0; i < 100; i++) await tx.send(i);
tx.close();

const total = await callPromise;
```

For large streams (more than the initial flow-control credit of 16 items) use
`Promise.all` so the drain and the call run concurrently — this lets the client
grant credit back to the server:

```ts
const [tx, rx] = channel<number>();
const callPromise = client.generateLarge(1000, tx);

const items: number[] = [];
const drain = (async () => {
  for await (const n of rx) items.push(n);
})();

await Promise.all([callPromise, drain]);
```

The method response terminates the request scope. Raw channels that need to
carry important data must finish before or as part of that response; durable or
resumable streams should be modeled as explicit service-level protocols.

## 7) Error handling

All vox client calls return `Result`-shaped values for fallible methods:

```ts
const result = await client.divide(10n, 0n);
if (!result.ok) {
  // result.error is your typed user error (e.g. MathError)
  console.error("division error:", result.error.tag);
} else {
  console.log("quotient:", result.value);
}
```

For infrastructure errors (network, protocol, cancellation) vox throws
`RpcError`:

```ts
import { RpcError, RpcErrorCode } from "@bearcove/vox-core";

try {
  await client.echo("hello");
} catch (e) {
  if (e instanceof RpcError) {
    switch (e.code) {
      case RpcErrorCode.CANCELLED:      /* request was cancelled */ break;
      case RpcErrorCode.INDETERMINATE:  /* connection broke mid-call — may or may not have executed */ break;
      case RpcErrorCode.UNKNOWN_METHOD: /* server doesn't know this method */ break;
      default: /* other protocol error */ break;
    }
  }
}
```

`INDETERMINATE` means the connection dropped while the call was in flight and
the runtime could not confirm whether the server executed it. The runtime does
not replay the call automatically; callers must decide whether to issue a new
call.

## 8) Workspace/publishing layout

A common layout is:

- `typescript/generated/*.ts` — generated service bindings
- a small npm package (e.g. `@acme/vox-generated`) re-exporting those files
- app/service packages depending on both generated bindings and the runtime

Keep generated package versions aligned with the Vox runtime major version.
