# Lane Model User Review, Retries, and Reliability

Status: obsolete brainstorm archive, not spec.

This note predates the narrowed direction in
`docs/design/operations-observability-and-progress.md`: Vox core currently
specifies request scopes and raw request-scoped channels only. Retry, resume,
operation identity, reliable streams, and durable delivery are out of scope for
the active Vox-core design round. Treat the retry/reliable-stream sections below
as historical exploration, not an implementation roadmap.

This note reviews the rootless connection and service-lane direction against
real Vox users found in this workspace. It also preserves earlier sketches for
retry, reliable-stream, and observability pieces.

The target model under review:

- `Link`: a dumb bidirectional frame transport.
- `Connection`: one authenticated, observable Vox protocol envelope over one
  link.
- `Lane`: one service namespace inside a connection.
- `Request`: one method call attempt on one lane.
- `Channel`: a request-scoped ordered stream attached to a request, unless an
  explicit durable/reliable stream primitive says otherwise.
- `Operation`: a logical unit of work above request attempts when retry,
  idempotency, resume, or tracing needs stable identity.

This is not the same as "one connection, one service". Multiple service lanes
on one connection are part of the core ergonomic story.

## Current Shape From Source Review

The current public API exposes the old model everywhere:

- a root connection with `ConnectionId::ROOT`;
- a `SessionHandle` used to open virtual connections later;
- `ConnectionOpen`, `ConnectionAccept`, `ConnectionReject`, and
  `ConnectionClose` messages;
- generated clients storing a `Caller` and optionally a `SessionHandle`;
- examples that establish a root `NoopClient` and then wait on
  `client.caller.closed()`;
- TypeScript examples using `established.rootConnection().caller()`;
- Swift examples and tests around `Session`, `SessionHandle`,
  `ConnectionHandle`, and `openConnection`.

At the wire/type level, `RequestId` and `ChannelId` are currently scoped to a
virtual connection. There is no first-class `OperationId`, no `LaneId` distinct
from old `ConnectionId`, no reliable stream ID, and no delivery commit point.

At the observer level, Vox already has local runtime events for RPC,
connection, channel, transport, driver errors, and queue state. That is useful
but not yet enough for the design we want: there is no stable hierarchy for
link establishment, connection auth, lane open/auth/readiness, operations,
attempts, raw channels, durable streams, and spans.

## User Inventory

This review sampled the Vox repository and the nearby `/Users/amos` checkouts
that actively use Vox. The purpose is to classify real pressure, not to count
every generated occurrence.

| User | Current usage | Proposed shape |
| --- | --- | --- |
| Vox Rust guide | `.establish::<WordLabClient>(dispatcher)` and root setup examples. | Teach `vox::serve(...).await?` for one service, `Server::new(...).service(...)` for many services, and `conn.open_lane::<WordLabClient>().await?` for clients. |
| Vox TypeScript guide | `established.rootConnection().caller()`. | `const conn = await connect(url); const api = await conn.openLane(ApiClient);` |
| Vox Swift runtime | `Session`, `SessionHandle.openConnection`, `ConnectionAcceptor`, `PendingConnection`, timeout-bearing `callRaw`. | `Connection` as the driven protocol object; `ConnectionHandle.openLane`; `LaneAcceptor` or service registry; request policies instead of response-only timeout as the default story. |
| Vox tests | Many tests use root `NoopClient`; virtual-connection tests cover open, reject, close, proxy, and drop behavior. | Most become lane-open tests. Proxy tests either become lane-frame proxy tests or link-to-link proxy tests, depending on which behavior remains core. Drop tests become explicit driver/handle/lane lifetime tests. |
| Macro snapshots | Generated clients store `Caller` and optional `SessionHandle`. | Generated clients store a typed lane caller and a connection/lane runtime reference only as needed; no root session field in the public shape. |
| Dodeca host/cell FFI | Host is initiator; cell is acceptor; host serves `HostService` and `DevtoolsService` back over reverse virtual connections selected by `vox-service`; loaded cells cache a root `NoopClient` to recover a `SessionHandle`. | One FFI link becomes one Vox connection. `HostService` and `DevtoolsService` are lanes. The cached root `NoopClient` becomes a cached `ConnectionHandle`. Reverse service usage is either an allowed reverse lane or a separate FFI/local link, chosen explicitly. |
| Dodeca browser devtools | Browser opens WebSocket Vox session to HTTP cell; HTTP cell accepts root Noop and proxies browser `DevtoolsService` virtual connection to a host virtual connection using `proxy_connections`. | Browser WebSocket opens a connection and a `DevtoolsService` lane. The HTTP cell either terminates and forwards at the application layer, proxies one lane to one upstream lane, or asks the host for a separate link and proxies links. The choice is visible in the code instead of hidden behind root virtual connections. |
| Dodeca BrowserService callback | Host accepts `DevtoolsService` and gets a `BrowserServiceClient` from `handle_with_client`, then tracks browser IDs. | The browser callback service is a reverse lane or explicit companion connection. The browser ID belongs in lane metadata/evidence, not in root session lifetime. |
| Stax daemon/server | Custom accept loops set channel capacity, observer, keepalive, route `"Noop"`, `"RunControl"`, `"Profiler"`, and `"TargetIngest"`, then establish `NoopClient` and wait for root close. | A server builder preserves capacity/observer/keepalive knobs on the connection, registers service lanes, and is driven as a future. `"Noop"` disappears. `TargetIngest` can get lane-specific channel credit/queue policy. |
| Stax CLI/frontends | Mostly simple `vox::connect` generated clients; generated TS uses root connection. | `vox::connect_lane::<RunControlClient>(...)` remains as a convenience, with `vox::connect(...).open_lane::<...>()` for multiple services on one connection. Generated TS opens lanes. |
| Bee/Bee-audio FFI | Backend accepts an FFI link, serves `Bee`/`BeeMl`, establishes `NoopClient`, waits for root close, sometimes calls session shutdown. | FFI acceptor drives a connection future serving one or more lanes. Shutdown is explicit on the driven connection, not hidden behind a noop generated client. |
| Dibs/Hotmeal/Styx/Helix | Mostly simple server/client setup plus generated clients. | These should get simpler: no root session, no `NoopClient`, same one-service convenience APIs. |
| Vixen/CCC | TLS/TCP and local socket users need peer identity. Swift VFS currently connects with `NoopClient`, opens a virtual `Vfs` connection with `prefix` metadata, then runs the session. FSKit accepts local peers with root `NoopClient`. | Transport evidence attaches to the connection. `Vfs` is a lane with lane metadata containing `prefix`; auth can use TLS, Unix peer credentials, XPC audit token, or app identity. The Swift code drives the connection and serves a lane explicitly. |

The pattern is consistent: the old root session mostly exists to hold the
transport open and make later virtual connections possible. Real services want
lanes. Real topology tricks want either another link or an explicitly named
proxy/rendezvous layer.

## API Sketches By Usage Class

Names are placeholders. The point is ownership, driving, and where service
selection lives.

### One-Service Server

Current tutorial shape:

```rust
vox::acceptor_on(link)
    .on_connection(HelloDispatcher::new(HelloService))
    .establish::<vox::NoopClient>()
    .await?
    .caller
    .closed()
    .await;
```

Proposed tutorial shape:

```rust
vox::serve(
    "local:///tmp/hello.sock",
    HelloDispatcher::new(HelloService),
)
.await?;
```

This future drives listening and accepted connections. Dropping a generated
client cannot stop the server. Dropping or cancelling the server future stops
the server because the future owns the work.

### Multi-Service Server

Stax-shaped server:

```rust
let server = vox::Server::bind("local:///tmp/stax.sock")?
    .channel_capacity(STAX_SERVER_CHANNEL_CAPACITY)
    .keepalive(vox::ConnectionKeepaliveConfig {
        ping_interval: Duration::from_secs(5),
        pong_timeout: Duration::from_secs(30),
    })
    .observer(stax_vox_observe::VoxObserverLogger::new("stax-server", "local"))
    .service(RunControlDispatcher::new(state.clone()))
    .service(ProfilerDispatcher::new(state.profiler()))
    .service(TargetIngestDispatcher::new(TargetIngestService::new(state)))
    .lane_config::<TargetIngestClient>(vox::LaneConfig {
        initial_channel_credit: STAX_SERVER_CHANNEL_CAPACITY,
        ..Default::default()
    });

server
    .with_graceful_shutdown(shutdown_signal())
    .await?;
```

The equivalent of today's service router becomes the lane registry. Routing
still exists, but the public concept is service-lane auth/readiness rather than
root connection dispatch.

### One-Service Client

The easy case should stay easy:

```rust
let client: CatalogClient = vox::connect_lane("local:///tmp/catalog.sock").await?;
let item = client.lookup("facet".to_owned()).await?;
```

This is a convenience over:

```rust
let conn = vox::connect("local:///tmp/catalog.sock").await?;
let catalog: CatalogClient = conn.open_lane().await?;
```

### Multi-Service Client

```rust
let conn = vox::connect("local:///tmp/app.sock").await?;

let catalog: CatalogClient = conn.open_lane().await?;
let metrics: MetricsClient = conn.open_lane().await?;
let admin: AdminClient = conn
    .open_lane_with(vox::LaneOpenOptions::new().metadata(admin_metadata))
    .await?;
```

Dropping the standalone `conn` handle should not secretly kill `catalog` while
`catalog` still exists. The generated lane client naturally holds the lane and
the connection runtime reference it needs. Graceful shutdown is explicit:

```rust
conn.close(vox::CloseReason::ClientDone).await?;
```

The server-side footgun is avoided by separating the driven connection/server
future from ordinary handles. Handles do not drive accepted peers.

### Single Accepted Link

For FFI, XPC, memory links, and manually accepted sockets:

```rust
let connection = vox::Connection::accept(link)
    .observer(observer)
    .service(BeeDispatcher::new(service))
    .establish()
    .await?;

connection.run().await?;
```

If the accepted side also needs to open a lane to the peer:

```rust
let established = vox::Connection::accept(link)
    .service(HostServiceDispatcher::new(host))
    .establish()
    .await?;

let handle = established.handle();
let browser: BrowserServiceClient = handle.open_lane().await?;

established.run().await?;
```

The API must make the driver visible. If `run()` is never awaited or spawned,
the connection does not run. That is a normal Rust async failure mode, and it
is easier to diagnose than dropping an apparently useless root `NoopClient`.

### Dodeca Host/Cell

```rust
struct LoadedCell {
    _lib: &'static Library,
    conn: vox::ConnectionHandle,
}

let established = vox::Connection::connect_on(link)
    .service(cell_host_proto::HostServiceDispatcher::new(host_service.clone()))
    .service(dodeca_protocol::DevtoolsServiceDispatcher::new(devtools_factory))
    .establish()
    .await?;

let conn = established.handle();
spawn_connection_driver(cell_name, established.run());
loaded.insert(cell_name, Arc::new(LoadedCell { _lib, conn }));
```

The cell runtime opens the host lane directly:

```rust
let host: HostServiceClient = self.conn.open_lane().await?;
```

If the host wants one browser session per devtools client, that should be lane
metadata or a lane-local service factory:

```rust
let devtools: DevtoolsServiceClient = host_conn
    .open_lane_with(
        LaneOpenOptions::service::<DevtoolsServiceClient>()
            .metadata(vox::metadata().u64("browser-id", browser_id).build()),
    )
    .await?;
```

If frame proxying is still the right implementation, it should say what is
being proxied:

```rust
vox::proxy_lanes(browser_lane, host_lane).await?;
```

or, if a separate child link is created:

```rust
vox::proxy_links(browser_link, host_link).await?;
```

The key distinction is important. `proxy_lanes` is an RPC/lane primitive and
must preserve lane-local request/channel/schema state. `proxy_links` is below
Vox and simply moves frames.

### Swift VFS

Current Swift has to say that the root connection is "just the session anchor".
The new tutorial should not need that sentence.

```swift
let connection = try await VoxRuntime.Connection.connect(
    UnixConnector(path: path),
    services: [
        VoxRuntime.service(Vfs.self, dispatcher: VfsDispatcher(handler: VfsBackend()))
    ])

let lane = try await connection.openLane(
    Vfs.self,
    metadata: Metadata.null
        .metaSetting("prefix", .string(vfsPrefix)))

try await connection.run()
```

The exact Swift names will differ, but the ownership should be visible:
`Connection` is the driven protocol object, `openLane` creates a service lane,
and `prefix` is lane metadata.

### TypeScript

Generated clients should hide the caller plumbing, but not by hiding a root
connection:

```ts
const conn = await connect(`ws://${location.host}/_vox`);
const devtools = await conn.openLane(DevtoolsServiceClient);

await devtools.subscribe(...);
```

For the one-service case:

```ts
const api = await connectLane(url, ApiClient);
```

## Lane Open Semantics

Opening a lane should be the point where service identity, schema
compatibility, authorization, readiness, and lane-local limits are negotiated.

Lane-open result states should be structured:

| State | Meaning |
| --- | --- |
| accepted | The lane is open and requests may run. |
| unknown service | The peer does not implement or does not reveal that service. |
| forbidden | The peer implements it, but this identity cannot open it. |
| not ready | The service exists but is temporarily unavailable. |
| draining | The service exists but is retiring and will not accept new lanes or requests. |
| schema incompatible | The service exists but the generated/client schema cannot interoperate. |
| policy rejected | The peer rejected lane options, metadata, limits, or auth requirements. |

Discovery should be filtered by authorization. An unauthenticated or
low-privilege peer should not learn every service just because they can open a
connection.

The current protocol can probably bridge this by renaming old virtual
connection messages conceptually:

| Current | Lane-model meaning |
| --- | --- |
| `ConnectionId` | `LaneId` in the RPC lane namespace. |
| `ConnectionOpen` | `LaneOpen`. |
| `ConnectionAccept` | `LaneAccept`. |
| `ConnectionReject` | `LaneReject`. |
| `ConnectionClose` | `LaneClose`. |
| ID `0` | Reserved connection/control lane, not an application service root. |

That bridge may reduce implementation churn, but the public API and docs must
stop treating ID 0 as a service-bearing root.

## Retry Model

Retries are operation-level behavior above request attempts. A link, conduit,
connection, or lane must not silently replay frames just because transport
delivery failed.

The model needs these IDs:

| ID | Scope | Purpose |
| --- | --- | --- |
| `connection_id` | one established Vox connection | Groups peer identity, auth, link, and establishment spans. |
| `lane_id` | one connection | Routes service-lane frames. |
| `operation_id` | caller-chosen logical operation | Stable across retry attempts, resume, tracing, and devtools. |
| `attempt_id` | one operation | Orders attempts and records retry history. |
| `request_id` | one lane attempt | Wire-level request/response matching. |
| `channel_id` | one lane | Raw request-scoped channel routing. |
| `stream_id` | one operation or durable stream service | Reliable stream resume and dedupe. |

The retry state machine should distinguish:

| Outcome | Meaning | Retry policy |
| --- | --- | --- |
| not enqueued | Request failed before entering the runtime send path. | Safe to retry with a new request attempt. |
| queued but not committed to transport | Local queue accepted it, but no frame send was attempted. | Usually safe; runtime can know this if it owns the queue boundary. |
| send attempted | The frame may or may not have reached the peer. | Indeterminate unless the operation is idempotent/resumable. |
| peer accepted request | The peer observed the request and may have started work. | Retry requires operation dedupe or method-specific replacement semantics. |
| response received | Attempt is terminal. | Do not retry. |
| protocol error | The peer or local runtime found a protocol violation. | Do not retry automatically; surface the bug. |
| connection lost | In-flight attempts are indeterminate. | Retry only through operation policy. |

Generated calls can expose a call builder without making the common case noisy:

```rust
let result = client
    .build_update_index(index)
    .operation(vox::OperationId::new())
    .idempotency_key(index.digest())
    .retry(vox::RetryPolicy::network_transient().max_attempts(3))
    .await?;
```

The default generated method remains:

```rust
client.update_index(index).await?;
```

and carries no automatic replay after an indeterminate send.

Server-side dedupe needs an operation registry keyed by at least:

```text
authenticated peer identity
service identity
method identity
idempotency key or operation ID
canonical request fingerprint when required
```

The registry states are:

| State | Duplicate attempt behavior |
| --- | --- |
| unknown | Start work if policy allows. |
| running | Wait, attach, or reject as `AlreadyRunning`, depending on method policy. |
| completed | Return the stored response or resume handle. |
| failed-terminal | Return the stored failure. |
| cancelled | Return cancellation, or allow replacement if method policy says so. |
| expired | Return `OperationExpired`; caller may start a new operation if safe. |
| conflict | Return `IdempotencyConflict` when the same key has incompatible request data. |

This registry can be in-memory for simple idempotent retry and durable for
operations that promise recovery across server restart. Vox can provide the
protocol and hooks; it cannot invent durable semantics for an arbitrary method.

## Reliable Channels

Raw Vox channels should remain request-scoped. They provide ordering and flow
control while the request scope and lane are alive. They are not message queues
and should not pretend to survive peer death.

The reliable version should be an explicit primitive, tentatively
`ReliableStream<T>`, not a hidden mode on every `Tx<T>`/`Rx<T>`.

Minimum properties:

- stable `stream_id`;
- stream epoch or generation for resume attempts;
- monotonically increasing item sequence numbers;
- sender retention until receiver commit;
- receiver dedupe by `(stream_id, sequence)`;
- explicit received acknowledgements for transport/window management;
- explicit committed acknowledgements for delivery semantics;
- retention expiry and resume expiry errors;
- terminal close with a final sequence number;
- operation identity tying the stream to the request or service that created
  it.

Received and committed are different. "Received" means the peer/runtime saw
the item and can grant flow-control credit. "Committed" means the receiver's
application policy says the item no longer needs replay. For a durable import,
that may mean the chunk was written to disk or transactionally recorded. For a
UI event stream, it may only mean the event was delivered to the app callback.

Possible API shape:

```rust
let stream = conn
    .reliable()
    .open_stream::<TraceRecord>(
        vox::ReliableStreamOptions::new()
            .retention(vox::Retention::for_items(10_000))
            .operation_id(operation_id),
    )
    .await?;

client.attach_trace_stream(stream.reader()).await?;

stream.send(record).await?;
stream.commit_through(seq).await?;
```

For service signatures, make the reliability visible in the type:

```rust
#[vox::service]
trait TargetIngest {
    async fn ingest(&self, records: vox::ReliableRx<TargetRecord>);
}
```

This is not the same as `Rx<TargetRecord>`. A `ReliableRx<T>` is a durable
resource with resume semantics. It may be represented on the wire by a request
scope at first, but it is explicitly detached from raw request-channel
lifetime by protocol.

Resume flow:

1. Caller creates or receives a `stream_id` and resume token.
2. Sender emits items with sequence numbers and retains uncommitted items.
3. Receiver emits `received_up_to` for runtime backpressure and
   `committed_up_to` for replay safety.
4. The connection or lane dies.
5. Caller opens a new connection/lane and sends `ResumeStream` with the same
   stream ID, operation ID, resume token, and last committed sequence.
6. Sender verifies peer identity and retention, then replays from
   `committed_up_to + 1`.
7. If retention expired, the sender returns `ResumeExpired`.

Delivery guarantees should be named honestly:

| Name | Meaning |
| --- | --- |
| at-most-once | No replay after uncertainty; dropped data is possible. |
| at-least-once | Replay after uncertainty; duplicates are possible. |
| effectively-once | At-least-once plus receiver dedupe/commit semantics. |

Exactly-once should not be promised generically. Vox can make the protocol
dedupe-friendly; the application still owns side-effect commit semantics.

The first implementation could be a system service running over a reserved
lane, rather than a new top-level wire family. That would let us prototype
retention, resume, and observer events before promoting it into the compact
core protocol.

## Timeouts And Progress

The default timeout should be an idle/progress timeout, not simply "no response
within N seconds".

A request or operation is active if protocol/runtime activity tied to that
scope occurs:

- request accepted;
- response sent or received;
- raw channel item sent, received, closed, reset, or credit-granted;
- reliable stream item, ack, commit, replay, or close;
- explicit progress event attached to the request or operation;
- cancellation or drain transition.

This should not count arbitrary logs or unrelated spans. Logs can be displayed
in devtools, but they should not keep a stuck operation alive.

Connection keepalive is connection progress, not request progress. A peer can
answer pings while one request is deadlocked.

The public API can expose:

```rust
client
    .build_import(path)
    .timeout(vox::TimeoutPolicy::idle(Duration::from_secs(30)))
    .await?;
```

or:

```rust
client
    .build_import(path)
    .deadline(Instant::now() + Duration::from_secs(300))
    .idle_timeout(Duration::from_secs(30))
    .await?;
```

Deadline and idle timeout are different policies and should appear separately
in metadata and observability.

## Observability Model

The object hierarchy should be explicit:

```text
transport mechanism
  link
    connection
      lane(service)
        operation
          attempt/request
            raw channel
            reliable stream attachment
            spans/events/progress
```

Transport establishment should be observable before any request exists:

- endpoint resolution;
- TCP, Unix socket, named pipe, stdio, in-process, FFI, XPC, WebSocket, or
  other link creation;
- TLS, mTLS, platform security, or code-signing verification;
- ALPN or equivalent protocol selection;
- WebSocket HTTP upgrade when present;
- Vox transport prologue;
- Vox connection handshake;
- schema cache negotiation;
- connection auth and peer evidence creation;
- lane open/auth/readiness.

Not every transport has every span. The event model should describe the real
stack instead of forcing everything into TCP/TLS.

The observability stream should use Vox's codec and schema machinery, but it
must not be scheduled like an ordinary application request on the lane it is
trying to explain. Tentative shape:

- reserved system/control lane per connection;
- independent bounded queue and flow-control budget;
- priority over ordinary app lanes for critical lifecycle/failure events;
- lossy or sampled mode for high-volume spans/logs;
- permissioned subscription filtered by connection identity and lane auth;
- local debug snapshots that inspect runtime state directly when in-process.

This gives us a devtools story:

```rust
let conn = vox::connect(addr)
    .observe(vox::ObservePolicy::request_server_spans())
    .await?;

let feed = conn.observability().subscribe().await?;
vox_devtools::serve_local(feed).await?;
```

It also gives command-line and local app tooling a way to explain cases like a
slow cache hit:

```text
resolve endpoint
tcp connect
tls handshake
vox prologue
connection handshake
lane open Ccc
request attempt request_download
server DB span
response
```

Observer IDs should be available for debugging, but metrics labels must stay
low cardinality: service, method, side, outcome, error kind, lane state, retry
kind, and transport kind. Request IDs, operation IDs, stream IDs, peer
addresses, and metadata values are trace fields, not default metric labels.

## Auth And Peer Evidence

Authorization decisions happen at multiple levels:

```text
transport evidence -> connection identity -> lane auth/readiness -> request auth
```

Examples of local evidence:

- mTLS certificate and verified identity;
- ALPN result;
- Unix socket UID/GID/PID where available;
- macOS XPC audit token and signing requirement;
- in-process/FFI component identity;
- test/memory synthetic identity.

This evidence is not ordinary remote-provided metadata. It is local
transport/assertion data. Public APIs can expose it through one context object,
but the trust boundary must remain visible.

Lane authorization should receive:

```rust
struct LaneAuthContext<'a> {
    peer: &'a PeerIdentity,
    evidence: &'a PeerEvidence,
    service: ServiceId,
    lane_metadata: &'a Metadata,
    connection_metadata: &'a Metadata,
}
```

Request authorization should additionally receive method identity and request
metadata. This scales from mTLS over WAN down to local Unix sockets and XPC
because the connection has one identity object, and transports can contribute
platform-specific evidence.

## Graceful Retire And Drain

The model needs protocol states for:

- stop accepting new links at a listener;
- stop opening new lanes on a connection;
- stop accepting new requests on a lane;
- let in-flight requests, raw channels, and reliable streams finish;
- cancel remaining work after a deadline;
- distinguish local intentional shutdown from peer death or protocol failure.

Tentative terms:

- retire connection: do not open more lanes on this connection;
- retire lane: do not send more requests on this lane;
- drain: let already accepted work finish;
- close: end the protocol object now.

Dropping a handle cannot do graceful async shutdown. It can at most release a
local reference and maybe trigger best-effort cleanup when the last reference
goes away. Graceful behavior needs an explicit async call or shutdown future.

## Performance Notes

The rootless lane model can be implemented cheaply if we reuse the current
virtual-connection machinery as the first bridge:

- old `ConnectionId` becomes the lane routing namespace;
- old connection settings become lane settings where appropriate;
- one connection handshake per link;
- one schema/auth/readiness negotiation per lane;
- request/channel frames remain lane-local;
- no reliable-stream cost unless the type or policy asks for it;
- no retry registry cost unless operation policy asks for it;
- observability high-volume data is sampled or bounded.

Schema exchange can avoid repeated full closures without unsafe guessing:

- exact schema digests;
- sorted compressed digest sets;
- cache snapshot IDs;
- resumption tickets tied to peer identity and protocol version.

Probabilistic summaries are only acceptable as hints. A false positive that
makes a peer believe a schema is known when it is not must be recoverable.

## Implementation Roadmap

1. Keep this design note tentative and confront it against the users above.
2. Rename the public concepts in docs and examples: session to connection,
   virtual connection to lane, root service to no public concept.
3. Add Rust facade types over current internals: `ConnectionHandle`,
   `LaneHandle`, `open_lane`, server builders, and explicit driver futures.
4. Update generated Rust client shape to stop exposing root `SessionHandle`.
5. Update TypeScript generation and docs from `rootConnection().caller()` to
   `openLane`.
6. Update Swift runtime naming and lifecycle around driven `Connection` plus
   `openLane`.
7. Convert current virtual-connection tests into lane tests, preserving reject,
   close, proxy, parity, schema, and drop/lifetime coverage.
8. Add lane-open auth/readiness error taxonomy.
9. Add operation metadata and observer IDs without automatic retry first.
10. Prototype retry for idempotent unary calls with a server-side operation
    registry hook.
11. Prototype `ReliableStream<T>` as an explicit system/service-layer
    primitive before changing raw channels.
12. Add establishment spans and reserved observability/control lane semantics.
13. Promote surviving pieces into `docs/content/spec/conn.md` and
    `docs/content/spec/rpc.md` with Tracey requirements.

## Things That Would Falsify This Direction

- A real user needs a single lane to mix several unrelated service namespaces
  with one request/channel ID namespace.
- Dodeca or Vixen cannot reasonably express its topology as lanes plus ordinary
  links, app-level forwarding, or optional link-producing transport machinery.
- Reverse lanes on one connection create auth/lifetime confusion worse than
  the current virtual-connection model.
- Reliable stream semantics cannot be made explicit in the type system without
  making common raw channels harder to use.
- Observability/control cannot avoid deadlocking behind app traffic without a
  separate transport, not just a reserved system lane.

None of those are proven by the current source review. The current evidence
mostly says the root service is a footgun, lanes are useful, and reliability
needs to be an explicit operation/stream layer rather than magic in channels.
