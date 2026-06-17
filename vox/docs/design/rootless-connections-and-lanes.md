# Rootless Connections and Service Lanes

Status: tentative design note, not spec.

This note captures the current direction for simplifying Vox connection
lifecycle, removing the root-service footgun, and preserving the unusually
flexible proxy/topology cases that motivated virtual connections in the first
place.

If this direction survives review against real Vox users, the normative pieces
should move into `docs/content/spec/conn.md` and `docs/content/spec/rpc.md`
with Tracey requirements. Until then this file should not contain Tracey
requirement annotations.

Companion review:
`docs/design/lane-model-user-review-and-reliability.md` is an archived
brainstorm that checked this model against local Vox users. Its retry,
operation, and reliable-stream material is not part of the active Vox-core
design round.

## Problem

The current model has three concepts tangled together:

- a physical `Link`, which is already a dumb bidirectional message transport;
- a `Session`, which performs the Vox handshake and owns many connection IDs;
- `Connection`s, including a special service-bearing root connection with ID 0.

That root connection is the recurring footgun.

On the server side, accepting a link usually creates a root caller that is not
really an application client. Rust calls that value `NoopClient`. It is a
liveness token: drop it, and the root connection can close. That makes examples
and harnesses fragile. A server can accidentally stop listening to an accepted
peer by dropping something that looks like a useless no-op client.

On the client side, the same model leaks into generated clients and session
handles. The common HTTP/gRPC-shaped intuition is "connect, get a client, call
methods". Vox can support stranger topologies, but ordinary users should not
pay for them in the core API. The current API exposes the extra power through a
special root plus virtual connections, which makes the simple case look
stranger than it needs to and makes the advanced case depend on a fake root
service.

The important counterweight is not "Dodeca needs virtual connections exactly as
they exist today". It is weaker and cleaner: if some topology needs reverse
work, NAT traversal, proxying, or rendezvous, that topology should establish or
obtain another Vox link. Vox RPC itself should still see an ordinary link and
run an ordinary connection over it.

## Direction

Remove service-bearing root lanes.

A fresh Vox RPC instance over a `Link` should establish a connection: the
authenticated, observable protocol envelope shared by the two peers. The
connection can then contain service lanes. Each lane is one service namespace.

There should be no application-visible root lane, no root service, no root
caller, and no `NoopClient` liveness token.

Keep `Link` dumb.

A `Link` remains a bidirectional frame source/sink: send one payload, receive
one payload, preserve order, apply backpressure at send, close gracefully. A
`Link` should not know about connections, lanes, service schemas, operations,
request IDs, channel IDs, discovery, observability, or auth policy.

Keep the core one-link, one-connection, many possible lanes.

The common path should not include mux concepts at all:

```text
tcp/ws/ffi/xpc/etc link
    -> one Vox RPC connection
    -> authorized service lanes
```

Lanes are not the problem. The special root lane is.

The current "virtual connection" idea has two jobs mixed together:

- a service lane, where one lane is one service;
- a topology trick, where one physical carrier creates another link-like path.

The service-lane job belongs in Vox core. The topology job can live below or
beside Vox as a wrapper, rendezvous protocol, or optional mux transport that
hands Vox another `Link`.

That gives us a smaller stack:

```text
ordinary transport:

    link -> Vox connection -> lane(Catalog)
                           -> lane(Metrics)
                           -> lane(Admin)

advanced transport or rendezvous, outside core RPC:

    mechanism -> link A -> Vox connection -> lanes...
              -> link B -> Vox connection -> lanes...
              -> link C -> Vox connection -> lanes...
```

Tentative vocabulary:

| Old | Tentative | Meaning |
| --- | --- | --- |
| link | link | Dumb bidirectional frame transport. |
| session | connection | A Vox RPC instance over one link. It owns handshake state, peer identity, auth policy, observability, and lane lifecycle. |
| connection | lane | A service namespace inside a connection. One lane equals one service. |

The most important naming decision is not the exact word. It is that the
special root lane disappears.

## Directionality

Physical link direction and logical service direction may be decoupled by
creating another link.

If Alice dials Bob, that only says Alice created the physical link. It should
not imply that only Alice can initiate all logical work forever. It also should
not imply the first Vox connection needs to become bidirectional service soup.

Current thesis:

- a Vox connection authenticates the peers and holds shared connection state;
- a lane is opened for one service;
- requests on that lane target that lane's service;
- responses, request-scoped channels, cancellation, credit, and errors flow
  both ways as part of that lane's service interaction;
- if the accepted side wants to call services on the opener, it obtains
  another link or opens an allowed reverse lane, depending on the eventual
  connection policy.

The "another link" mechanism can be boring: dial a known address, connect over
FFI, ask an HTTP cell to establish a companion connection, use XPC rendezvous,
or run a NAT-punching protocol that returns a connected link. Vox does not need
to know which one happened.

Likewise, if a system needs the peer that physically established the connection
to be the peer serving the Vox service, that does not need to be a Vox RPC
primitive. A lower-level wrapper protocol can negotiate, authenticate, punch
holes, exchange handles, or otherwise set up the actual link. Once the wrapper
hands Vox a `Link`, Vox only cares which side opens the Vox connection and
which lanes are requested or authorized on that connection.

This avoids the confusing "one connection where both peers are arbitrary
clients and servers at once" model without forcing every user to carry
NAT/proxy/topology machinery.

Open question: callbacks. If a method hands out a channel, the stream already
has bidirectional protocol traffic. If a method wants the peer to invoke an
entire service, that should probably be represented as an explicit second link
or service capability, not as accidental bidirectionality on the same service
connection.

## Service Lanes

Multiple service lanes on one connection are a core feature.

Rust services are traits. Forcing users to define one giant trait that composes
all smaller service traits is awkward, brittle, and fights the language. Vox
should let an endpoint serve several service traits and let a peer open typed
lanes for the subset it is allowed to call.

Sketch:

```rust
let server = vox::Server::new(listener)
    .service(CatalogDispatcher::new(catalog))
    .service(MetricsDispatcher::new(metrics))
    .service(AdminDispatcher::new(admin))
    .authorize(authz);

server.await?;
```

```rust
let conn = vox::connect("local:///tmp/app.sock").await?;

let catalog: CatalogClient = conn.open_lane().await?;
let metrics: MetricsClient = conn.open_lane().await?;
```

The generated client types still stay small and trait-shaped. The shared
connection is what composes them, and each generated client is backed by a
lane for its service.

The protocol needs a real lane-open story. Opening a lane should identify the
target service and negotiate or validate the service's request/response/channel
schemas. After a lane is open, ordinary requests on that lane can identify only
methods within that lane's service namespace.

Open details:

- Does connection establishment advertise a filtered service catalog, or are
  lanes opened optimistically and rejected with structured denials?
- Are service IDs strings, schema-derived stable IDs, or both?
- Is a generated `FooClient` created only after lane open succeeds, or can it
  exist as a lazy handle whose first call opens the lane?
- Can a server add, remove, retire, or mark lane availability during a
  connection, or is lane availability fixed once advertised?

## Authorization and Readiness

Service lanes make authorization a first-class part of connection and lane
establishment.

The server may implement a service, but still not allow a given peer to call
it. The reason can depend on:

- transport evidence, such as mTLS identity, Unix peer credentials, XPC code
  identity, or in-process component identity;
- connection metadata, such as requested tenant, account, or capability token;
- lane/service identity;
- method identity;
- dynamic service readiness.

These states should be distinguishable:

| State | Meaning | Client-facing shape |
| --- | --- | --- |
| unknown lane service | The peer does not implement or does not reveal that service. | `UnknownService` or equivalent lane-open rejection. |
| forbidden lane | The peer implements the service, but this caller is not authorized to open its lane. | `Forbidden` with optional redaction. |
| not ready | The service lane exists but is temporarily unavailable or waiting for dependencies. | `ServiceUnavailable`/`NotReady`, optionally with retry or progress metadata. |
| ready | The lane may open and calls may proceed, subject to per-method auth. | Normal lane and request handling. |

Discovery must be filtered by authorization. An unauthenticated peer should not
learn every private service just because the connection supports lanes.

Readiness should not be faked as "service does not exist". If a service is
implemented but warming up, blocked on a dependency, draining, or administratively
paused, that is observability and policy data. It should be visible to callers
that are allowed to know it, and to local devtools.

This also gives devtools a clearer shape: connection health/auth belongs to the
connection, while per-service readiness and failures belong to lanes.

## Optional Mux Links

If Vox grows a mux primitive, it should be transport-shaped, not RPC-shaped,
and it should be optional. It produces links. It does not replace service
lanes.

Sketch:

```rust
trait MuxCarrier {
    type Link: vox::Link;
    type Evidence;

    async fn open_link(&self, metadata: Metadata) -> Result<AcceptedLink<Self::Link>, Error>;
    async fn accept_link(&self) -> Result<Option<AcceptedLink<Self::Link>>, Error>;
}

struct AcceptedLink<L> {
    link: L,
    evidence: PeerEvidence,
}
```

Names are placeholders. The shape matters:

- opening a child link is below Vox RPC;
- accepting a child link yields a fresh `Link`;
- any peer evidence travels beside the link, not inside arbitrary user
  metadata;
- Vox then performs normal prologue/handshake/schema negotiation on the child
  link.

`open_link` metadata is transport/mux metadata, not request metadata. It might
include a logical purpose or resumable setup hint, but the child link still has
to perform its own Vox connection handshake because a `Link` does not know Vox
connections or lanes.

This primitive belongs beside TCP, Unix sockets, WebSocket, FFI, XPC, and
memory links. It should not be a hidden mandatory layer under every Vox
connection.

## Dodeca Topology Case

The existing spec uses Dodeca to justify virtual connections:

```text
Host <-> HTTP Server Cell <-> Browser
```

Dodeca is useful because it mixes both concerns. Some of its current virtual
connections are service lanes: one service namespace inside an already
authenticated host/cell connection. Some of its topology pressure may instead
belong below Vox, as extra local/FFI links or rendezvous.

The browser opens a WebSocket Vox session to the HTTP server cell. The cell
already has a local/FFI Vox session to the host. Today the HTTP server cell
opens a virtual connection on the host session, then proxies the browser
connection to it without translating request IDs or channel IDs.

Dodeca should not force every Vox connection to pay for that topology.

Possible rootless shapes:

- the HTTP server cell asks the host to establish a separate FFI/local link for
  browser devtools, then proxies or hands off that ordinary link;
- the host exposes a local endpoint and the HTTP cell dials it when a browser
  connects;
- the browser connection terminates at the HTTP cell, which forwards at the
  application layer if that is good enough;
- an optional mux carrier is used only if Dodeca truly needs multiple links
  over one host/cell carrier.

The mux version would look like:

```text
Browser
  -> WebSocket link
  -> Vox connection
  -> lane(DevtoolsService)

HTTP Server Cell
  -> existing mux carrier to Host
  -> opens child link to Host
  -> Vox connection
  -> lane(DevtoolsService)

Proxy
  -> browser link/connection <-> host child link/connection
```

The key property is preserved: the HTTP server cell does not need to understand
or reimplement the Devtools RPC surface if it chooses the proxy shape. It
forwards frames between two links.

The stronger conclusion is not "Dodeca proves core mux is required". It is:
service composition should be modeled as lanes, while reverse/proxy topologies
may be modeled as separate links. A mux carrier is one possible way to obtain
those links, not the core Vox lane model.

## Handshake, Lanes, and Schema Cost

Every produced link should perform a full semantic Vox connection setup by
default:

- transport prologue;
- Vox handshake;
- protocol schema exchange for the connection envelope;
- connection settings and limits;
- auth/authorization checks.

That is the correct baseline because the produced link is just a link.

Opening a lane should be cheaper than creating a new link and connection. Lane
open should negotiate the service identity, service schema compatibility,
authorization, readiness, and lane-local limits. It should not repeat DNS/TCP,
TLS, transport prologue, or connection-auth work.

If this is too expensive for high-churn links, optimize with explicit
resumption rather than hidden parent/session state. For example:

- a parent mux carrier or rendezvous protocol can carry a resumption ticket or
  cache identity;
- peers can exchange exact schema digests instead of full schema closures when
  both sides prove they already have the closure;
- digest sets can be sorted and compressed;
- a cache snapshot ID can stand for "the set of schemas we both know";
- any probabilistic summary must be recoverable and cannot be the only source
  of truth.

Bloom filters are not a great first primitive for schema exchange. They have no
false negatives but can have false positives; a false positive would make a
peer think the other side has a schema that it does not have. That can be made
recoverable, but exact digests plus compression are easier to reason about.

## Serving Lifetime

Serving should be driven by an explicit future.

Common shape:

```rust
vox::serve(addr, MyDispatcher::new(service)).await?;
```

More explicit shape:

```rust
let listener = vox::local::bind(path).await?;
let server = vox::Server::new(listener, MyDispatcher::new(service));

server
    .with_graceful_shutdown(shutdown_signal())
    .await?;
```

Dropping an ordinary generated client should not secretly stop an accepted
server peer. Dropping a server future should cancel the server, because the
server future is what owns the work. That matches the Rust pattern used by
Hyper and similar runtimes: drive the serving future until you want it to stop.

This also makes the failure mode more legible. If someone creates a server
future and never awaits/spawns it, the server never runs. That is a simpler
lesson than "you dropped a no-op client that was actually the root liveness
anchor for an accepted session".

Open issue: `Drop` cannot perform graceful async shutdown. Graceful shutdown
must be requested explicitly with a future or handle. Drop can only be abrupt or
best-effort cleanup.

## Graceful Shutdown

Current Vox has close/error behavior, but not a rich drain story.

The rootless model needs protocol language for at least:

- stop accepting new physical links;
- stop opening/accepting new produced links in rendezvous/mux transports;
- stop opening new lanes on a Vox connection;
- stop accepting new requests on retiring lanes;
- let in-flight requests and request-scoped channels finish;
- fail/cancel the remaining work after a deadline;
- report shutdown reason distinctly from peer death.

Tentative terms:

- `retire` means "do not start more work here";
- `drain` means "finish already accepted work";
- `close` means "the transport/protocol is ending now".

For a mux carrier, retire means "do not open or accept new child links". It may
also propagate retire to existing child Vox connections, but those child
connections still need their own drain/close state.

For a Vox connection, retire means "do not open new lanes". For a lane, retire
means "do not send new requests on this service lane". Existing request scopes
and their channels may continue until they finish or are cancelled.

This area is intentionally not specified yet.

## Auth and Peer Evidence

Authentication and access control cannot be an afterthought because they decide
which connections, lanes, and requests may be opened.

The model should be:

```text
transport evidence -> connection peer identity -> lane auth/readiness -> request auth
```

Examples of transport evidence:

- TLS/mTLS peer certificate details;
- ALPN when TLS directly carries Vox frames (`vox/1`) or a wrapped transport
  (`h2`, `http/1.1`, WebSocket);
- Unix socket peer UID/GID/PID where available;
- XPC audit token and code-signing identity on macOS;
- in-process component identity for FFI/shared-library transports;
- synthetic identity for memory/test transports.

This evidence should not be stuffed into ordinary user metadata as if it came
from the remote application. User metadata is application-provided. Transport
evidence is asserted by the local transport. They need different trust levels,
even if the public API lets a server inspect both through one context object.

For mux child links, evidence should be inherited or derived from the parent
carrier unless the mux transport can provide more specific child evidence.

## Observability

This note does not replace
`docs/design/operations-observability-and-progress.md`; it changes the object
model that observability should attach to.

Important consequences:

- transport establishment spans attach to physical links, rendezvous
  mechanisms, or mux carriers;
- produced-link open/accept spans attach to the mechanism that produced the
  link;
- Vox handshake/schema spans attach to each connection;
- lane open/accept/reject/readiness spans attach to lanes;
- request progress attaches to request scopes within lanes, not to keepalive or
  arbitrary logs;
- request-scoped channel activity is visible under the request, lane, and
  connection that introduced the channel;
- observability/control traffic must not deadlock behind the application lane
  it is trying to explain.

The observability stream may use the same codec and schema machinery as Vox,
but it should not be just another ordinary user request on the endangered
service connection.

## Retry and Reliable Delivery

Retry, resume, reliable delivery, and operation identity are not part of the
active Vox-core design round. The core rule for this document is narrower:
links and connections do not replay requests, and raw channels are
request-scoped sidebands.

Transport/rendezvous/mux/link code may reconnect or re-establish links, but
that is not RPC retry. A raw send failure is not proof that the peer did not
observe the frame. Lane reopen is also not a delivery guarantee; it only
recreates a service namespace inside a live connection.

Raw Vox channels are ordered streams with flow control. They are not durable
queues. Reliable delivery across peer death needs a service-level protocol with
its own handle, authentication, retention, acknowledgement, and resume
semantics. Vixen's `Producing::force(PartKey) -> Part` shape is the current
example of important stream data being modeled above raw channels.

## Tutorial Sketches

Simple client:

```rust
#[vox::service]
trait Catalog {
    async fn lookup(&self, key: String) -> Option<Entry>;
}

let catalog: CatalogClient = vox::connect("local:///tmp/catalog.sock").await?;
let entry = catalog.lookup("facet".to_owned()).await?;
```

Simple server:

```rust
#[tokio::main]
async fn main() -> eyre::Result<()> {
    vox::serve(
        "local:///tmp/catalog.sock",
        CatalogDispatcher::new(CatalogService::new()),
    )
    .await?;

    Ok(())
}
```

Server with multiple services:

```rust
let server = vox::Server::new(listener)
    .service(CatalogDispatcher::new(catalog))
    .service(MetricsDispatcher::new(metrics))
    .service(AdminDispatcher::new(admin))
    .authorize(authz);

server.await?;
```

Client using multiple services on the same connection:

```rust
let conn = vox::connect("local:///tmp/app.sock").await?;

let catalog: CatalogClient = conn.open_lane().await?;
let metrics: MetricsClient = conn.open_lane().await?;

let entry = catalog.lookup("facet".to_owned()).await?;
let snapshot = metrics.snapshot().await?;
```

Server with explicit graceful shutdown:

```rust
let listener = vox::local::bind("/tmp/catalog.sock").await?;
let server = vox::Server::new(listener, CatalogDispatcher::new(service));

server
    .with_graceful_shutdown(async {
        tokio::signal::ctrl_c().await.ok();
    })
    .await?;
```

Optional mux carrier, one side opens multiple links:

```rust
let carrier = vox::mux::connect("local:///tmp/host.sock").await?;
let link = carrier.open_link().await?;
let conn = vox::connect_on(link).await?;

let catalog: CatalogClient = conn.open_lane().await?;
let metrics: MetricsClient = conn.open_lane().await?;
```

Optional mux carrier, accepted physical peer also opens a link back:

```rust
let carrier = vox::mux::accept(link).await?;

let serve = carrier.serve_links(|link| {
    let worker = worker.clone();
    async move {
        let conn = vox::accept_on(link)
            .service(WorkerDispatcher::new(worker))
            .await?;
        conn.closed().await
    }
});

let control_link = carrier.open_link().await?;
let control_conn = vox::connect_on(control_link).await?;
let control: ControlClient = control_conn.open_lane().await?;

tokio::try_join!(serve, async move {
    control.ready().await?;
    Ok(())
})?;
```

The last example is an advanced transport shape, not the default teaching path.
The important property is more general: accepting one link does not prevent the
acceptor from obtaining another link and opening a Vox connection in the other
logical direction.

## Local User Audit

This was a source/manifests scan under `/Users/amos`, excluding the obvious
cache output when practical. It is not exhaustive proof, but it is enough to
identify the compatibility pressure.

| Checkout | Evidence | Redesign pressure |
| --- | --- | --- |
| `/Users/amos/dodeca` | Uses `NoopClient`, `SessionHandle`, `ConnectionAcceptor`, `open_connection`, and `proxy_connections`. `cells/cell-http/src/devtools.rs` proxies browser Devtools connections through the host. `crates/dodeca/src/cell_loader.rs` stores root sessions for cell links and opens reverse virtual connections. | Important stress case. Some current virtual connections should become service lanes; topology pressure may simplify to separate FFI/local links or app-level forwarding. |
| `/Users/amos/stax` | Server accept loops use `.on_lane(...).establish::<vox::NoopClient>()`; clients are mostly simple `vox::connect`. The daemon has custom channel capacity, observer, and keepalive setup. | Root liveness should disappear; advanced server config still needs an explicit server builder/future. |
| `/Users/amos/bee` and `/Users/amos/bee-audio` | Rust FFI/server paths use `.on_lane(...).establish::<vox::NoopClient>()`; Swift app code stores `SessionHandle`; generated TypeScript clients call `established.rootConnection().caller()`. | Cross-language API should remove root handles and generated root access. Swift needs an explicit driven connection/server object. |
| `/Users/amos/dibs` | Example app and service code use `.on_lane(...).establish::<vox::NoopClient>()`; TypeScript generated client uses root connection. | Mostly ordinary service serving and generated-client migration. |
| `/Users/amos/hotmeal` | WASM/browser fuzz paths establish with `NoopClient`; WebSocket links are common. | Browser/WebSocket path should benefit from one link -> one connection with authorized service lanes. |
| `/Users/amos/styx` | LSP extension tests and server setup use `.on_lane(dispatcher)`. | Mostly simple server migration. |
| `/Users/amos/vixenware/ccc.vixen.rs` | Backend implements `ConnectionAcceptor` and serves Ccc; client manually establishes TLS/TCP before Vox. | Good auth/evidence case: mTLS/TLS evidence must reach lane authorization. |
| `/Users/amos/vixenware/vixen` | Many Rust paths use `NoopClient`, `.on_lane(...)`, and `vox::serve`; Swift app opens a virtual connection with a VFS dispatcher after a Noop root session; FSKit/local socket code needs local peer identity. | Strong cross-language and local-IPC stress case. The VFS virtual connection likely maps to a lane; local/XPC/FFI links still need peer evidence. |
| `/Users/amos/helix`, `/Users/amos/helix-fastenc`, `/Users/amos/helix-sched` | Older trace server code uses `vox::serve_listener`; generated web clients use root connection. | Mostly older simple server/client migration, but useful for compatibility shims and examples. |

Compatibility classes:

- Simple generated clients: should become easier. `rootConnection().caller()` and root `Caller` fields disappear from generated public API.
- Simple servers: should become clearer. They drive a server future; no `NoopClient` token.
- Configured servers: still need builder knobs for lane limits, channel capacity, keepalive, observers, auth/evidence, and graceful shutdown.
- Reverse-service users: need either an authorized reverse lane on the same connection or a way to obtain another link in the reverse logical direction. Another link can be a normal dial, FFI callback, XPC rendezvous, NAT-punching result, or optional mux child link.
- Proxy users: need link-to-link proxy helpers when they choose frame proxying, but application-level forwarding may be better for some products.
- Swift/TypeScript users: need the same conceptual model, without Rust-only drop semantics becoming protocol behavior.

## Migration Sketch

Likely order:

1. Introduce explicit server/connection futures while keeping current protocol.
   Make examples teach "drive the server future" and stop teaching root
   liveness.
2. Define the rootless connection/lane API and generated
   client/server shapes.
3. Add peer evidence types to accepted links/connections before auth APIs grow
   around untrusted metadata.
4. Remove public reliance on root `NoopClient` in Rust examples and generated
   TypeScript/Swift client shapes.
5. Rework Dodeca and Vixen sketches around separate ordinary links first:
   direct local/FFI/XPC links, explicit rendezvous, or application forwarding.
6. Only introduce a mux carrier abstraction if the real migrations still need
   multiple links over one carrier.
7. Split current virtual-connection machinery into service lanes and any
   separate topology/link-producing machinery that still proves necessary.
8. Promote surviving semantics into the spec with Tracey requirements.

## Open Questions

- Is lane availability advertised during connection establishment, discovered
  by service catalog, or denied lazily when opening a lane?
- Is service selection part of the lane-open envelope, well-known protocol
  metadata, or both?
- Does an accepted connection ever initiate requests on the same
  connection by opening reverse lanes, or do all callbacks use another link?
- Is mux needed at all in core, or should it live as a separate transport crate?
- Should NAT traversal be entirely external: a rendezvous/punching protocol that
  returns an ordinary `Link` to Vox?
- Should Dodeca use separate FFI/local links instead of preserving frame-level
  proxying through the HTTP cell?
- If mux exists, does a child link need a stable child-link ID visible to
  observability, or is it transport-private?
- How does graceful retire propagate from external carriers/rendezvous
  mechanisms to produced links and then to request scopes?
- How should request-scoped channel lifetime interact with lane retire and
  connection retire?
- Which evidence fields are portable enough for core Vox, and which belong in
  transport-specific extension structs?
- What exact API shape lets Dodeca choose between app-level forwarding,
  separate-link proxying, or optional mux without making every Vox user pay for
  it?
- Can the first migration bridge reuse existing virtual-connection machinery
  for lanes, or would that preserve too much of the root/session model we are
  trying to remove?
