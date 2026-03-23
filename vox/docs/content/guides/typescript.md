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
    "@bearcove/vox-wire": "7.0.0",
    "@bearcove/vox-postcard": "7.0.0"
  }
}
```

Generated files import from `@bearcove/vox-core` and transport packages. The
wire/postcard packages handle low-level encoding and are typically not used
directly.

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
- `GreeterDispatcher` — wires a `GreeterHandler` into the vox session
- `greeter_descriptor` — schema + retry metadata used by the runtime

## 3) Use the generated client

```ts
import { session } from "@bearcove/vox-core";
import { wsConnector } from "@bearcove/vox-ws";
import { GreeterClient } from "@acme/vox-generated/greeter.ts";

const established = await session.initiator(wsConnector("ws://127.0.0.1:9000"));
const client = new GreeterClient(established.rootConnection().caller());

const msg = await client.hello("world");
console.log(msg);
```

If the generated module exposes a `connectGreeter` helper (for WebSocket), you
can also use that shorthand directly.

## 4) Use the generated dispatcher on the server

```ts
import { session } from "@bearcove/vox-core";
import { tcpConnector } from "@bearcove/vox-tcp";
import { Driver } from "@bearcove/vox-core";
import { GreeterDispatcher, type GreeterHandler } from "@acme/vox-generated/greeter.ts";

class GreeterService implements GreeterHandler {
  hello(name: string): string {
    return `hello, ${name}`;
  }
}

const established = await session.initiator(tcpConnector("127.0.0.1:9000"));
const driver = new Driver(
  established.rootConnection(),
  new GreeterDispatcher(new GreeterService()),
);
await driver.run();
```

## 5) Session resumption and reconnection

For WebSocket clients in browsers or mobile apps, connections drop. Vox's
session resumption lets a client reconnect transparently — in-flight calls are
automatically retried on idempotent methods, and the session resumes exactly
where it left off.

### Enabling resumption

Pass `resumable: true` when creating the session. The server must also support
resumption (i.e., be using `establish_or_resume` with a `SessionRegistry`).

```ts
const established = await session.initiator(wsConnector("ws://api.example.com"), {
  resumable: true,
});
```

When enabled, vox exchanges a session resume key during the handshake. If the
connection drops, the session automatically reconnects using that key.

### Reconnect policy

By default the session retries forever with exponential backoff (500 ms base,
30 s cap). You can customize this:

```ts
const established = await session.initiator(wsConnector("ws://api.example.com"), {
  resumable: true,
  reconnect: {
    maxAttempts: Infinity,   // retry forever (default)
    baseDelay:   500,        // first retry after 500 ms
    maxDelay:    30_000,     // cap at 30 s between retries
  },
});
```

Set `maxAttempts` to a finite number if you want to give up after a fixed number
of failures. The delay between attempt `n` is `min(baseDelay × 2^(n-1), maxDelay)`.

### Observing connection state

Register callbacks to drive UI (spinners, banners, toasts):

```ts
const established = await session.initiator(wsConnector("ws://api.example.com"), {
  resumable: true,
  onConnectivityChange: (state) => {
    // 'connected' | 'disconnected' | 'reconnecting' | 'failed'
    setConnectionStatus(state);
  },
  onReconnecting: (failedAttempt, nextAttemptAt, retryNow) => {
    const secs = Math.ceil((nextAttemptAt.getTime() - Date.now()) / 1000);
    console.log(`Attempt ${failedAttempt} failed. Retrying in ${secs}s…`);
    // retryNow() skips the wait — wire it to a "Retry Now" button.
  },
  onReconnected: () => {
    toast.success("Reconnected");
  },
  onReconnectFailed: (error) => {
    toast.error(`Could not reconnect: ${error.message}`);
  },
});
```

The `onConnectivityChange` callback is the simplest hook for showing a
"Reconnecting…" banner in your UI. The more specific callbacks let you react to
individual attempts or final failure.

### Keepalive for silent drops

By default, Vox relies on the transport to surface a closed connection. TCP and
browser WebSocket connections are usually reliable, but certain environments
(mobile networks, HTTP proxies, NAT gateways) can silently discard packets
without sending a FIN or RST — leaving the connection open on both sides while
data can no longer flow.

Enable keepalive to catch these silent drops:

```ts
const established = await session.initiator(wsConnector("ws://api.example.com"), {
  resumable: true,
  keepaliveIntervalMs: 15_000,   // send a Ping every 15 s
  keepaliveTimeoutMs:   5_000,   // give up if no Pong within 5 s
});
```

Vox sends a protocol-level `Ping` message every `keepaliveIntervalMs`. If the
peer does not reply with a `Pong` within `keepaliveTimeoutMs` (default: half
the interval), the connection is forcibly closed and session recovery begins.

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
import { session } from "@bearcove/vox-core";
import { wsConnector } from "@bearcove/vox-ws";
import { ApiClient } from "@acme/generated/api.ts";

async function connect(): Promise<ApiClient> {
  const established = await session.initiator(
    wsConnector("wss://api.example.com"),
    {
      resumable: true,

      // Retry forever with exponential backoff, capped at 30 s.
      reconnect: {
        baseDelay: 500,
        maxDelay:  30_000,
      },

      // Detect silent drops (important on mobile).
      keepaliveIntervalMs: 15_000,
      keepaliveTimeoutMs:   5_000,

      // Drive a "connection" indicator in your UI.
      onConnectivityChange: (state) => {
        document.getElementById("status")!.textContent = {
          connected:    "●",
          disconnected: "○",
          reconnecting: "↻",
          failed:       "✕",
        }[state];
      },
      onReconnecting: (failedAttempt, nextAttemptAt, retryNow) => {
        const secs = Math.ceil((nextAttemptAt.getTime() - Date.now()) / 1000);
        // Replace with your own UI — e.g. a banner with a "Retry Now" button.
        // Call retryNow() from that button's onClick to skip the wait.
        console.log(`Attempt ${failedAttempt} failed. Retrying in ${secs}s.`);
        document.getElementById("retry-btn")?.addEventListener("click", retryNow, { once: true });
      },
    },
  );

  return new ApiClient(established.rootConnection().caller());
}
```

## 6) Channels (streaming)

Vox supports bidirectional streaming via typed channels. Channels are created
as a `(Tx, Rx)` pair and one end is passed to a method call; the session
manages the lifetime automatically.

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
      case RpcErrorCode.INDETERMINATE:  /* session broke mid-call — may or may not have executed */ break;
      case RpcErrorCode.UNKNOWN_METHOD: /* server doesn't know this method */ break;
      default: /* other protocol error */ break;
    }
  }
}
```

`INDETERMINATE` is particularly important for resumable sessions: it means the
connection dropped while the call was in flight and the session could not
confirm whether the server executed it. For **idempotent** methods the session
automatically retries on reconnect; for non-idempotent methods the caller must
decide how to handle the ambiguity.

## 8) Workspace/publishing layout

A common layout is:

- `typescript/generated/*.ts` — generated service bindings
- a small npm package (e.g. `@acme/vox-generated`) re-exporting those files
- app/service packages depending on both generated bindings and the runtime

Keep generated package versions aligned with the Vox runtime major version.