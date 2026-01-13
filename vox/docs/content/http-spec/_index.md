+++
title = "roam HTTP Bridge Specification"
description = "JSON/HTTP gateway for roam services"
weight = 25
+++

# Introduction

This specification defines the **roam HTTP Bridge** — a gateway that exposes
roam services over HTTP with JSON encoding. The bridge translates between
HTTP/JSON and native roam (POSTCARD/binary), allowing web clients and
standard HTTP tooling to interact with roam services.

The bridge is a deployment pattern, not a core protocol binding. roam
services remain pure roam; the bridge handles HTTP-specific concerns.

# Architecture

```aasvg
┌─────────────┐      HTTP/JSON        ┌─────────────┐     roam/POSTCARD     ┌─────────────┐
│  Web Client │──────────────────────▶│ HTTP Bridge │──────────────────────▶│ roam Service│
│  (browser)  │◀───────────────────── │  (gateway)  │◀──────────────────────│  (backend)  │
└─────────────┘                       └─────────────┘                       └─────────────┘
```

Benefits:
- roam services stay fast (binary protocol)
- Web clients get a friendly JSON API
- HTTP infrastructure works normally (load balancers, proxies, CORS)
- Single point for HTTP-specific features (auth, rate limiting, etc.)

# URL Structure

> r[bridge.url.base]
>
> A bridge is mounted at a base URL. All bridge endpoints are relative
> to this base. The base URL is deployment-specific (e.g., `https://api.example.com/`
> or `https://example.com/api/roam/`).

> r[bridge.url.methods]
>
> RPC methods are exposed at `{base}/{service}/{method}` where `{service}`
> is the service name and `{method}` is the method name.
>
> ```
> POST /TemplateHost/load_template
> POST /Calculator/add
> ```

> r[bridge.url.websocket]
>
> The WebSocket endpoint is at `{base}/@ws`. The `@` prefix is reserved
> for bridge infrastructure endpoints and cannot collide with service
> names (service names cannot start with `@`).
>
> ```
> GET /@ws  (WebSocket upgrade)
> ```

> r[bridge.url.reserved]
>
> Paths starting with `@` are reserved for bridge infrastructure.
> Service names MUST NOT start with `@`.

# HTTP Endpoints

## Request Format

> r[bridge.request.method]
>
> RPC calls use HTTP POST.

> r[bridge.request.content-type]
>
> Requests MUST use `Content-Type: application/json`.

> r[bridge.request.body]
>
> The request body MUST be a JSON array containing the method's arguments
> in declaration order, matching roam's tuple encoding.
>
> ```json
> [3, 5]
> ```

> r[bridge.request.metadata]
>
> Request metadata is passed via HTTP headers. Custom application metadata
> uses a `Roam-` prefix: `Roam-{key}` maps to metadata key `{key}`.
>
> ```
> Roam-Request-Id: abc123
> Roam-Deadline: 1704067200
> ```

> r[bridge.request.metadata.wellknown]
>
> Well-known headers pass through without prefix transformation:
> - `traceparent` → metadata key `traceparent`
> - `tracestate` → metadata key `tracestate`
> - `authorization` → metadata key `authorization`
>
> This ensures compatibility with W3C Trace Context and standard auth.

> r[bridge.request.nonce]
>
> The `roam-nonce` metadata (see `r[core.nonce]`) is passed via the
> `Roam-Nonce` header. The value MUST be base64-encoded (standard alphabet,
> with padding). The bridge decodes it to 16 bytes for the roam request.
>
> ```
> Roam-Nonce: dGhpcyBpcyBhIG5vbmNl
> ```

## Response Format

> r[bridge.response.content-type]
>
> Responses MUST use `Content-Type: application/json`.

> r[bridge.response.success]
>
> On success, the response body is the JSON-encoded return value.
> HTTP status is 200 OK.
>
> ```json
> 8
> ```

> r[bridge.response.user-error]
>
> For application errors (`RoamError::User(E)`), the response body is
> a JSON object with `"error": "user"` and `"value"` containing the
> JSON-encoded error. HTTP status is 200 OK.
>
> ```json
> {
>   "error": "user",
>   "value": { "code": "NOT_FOUND", "message": "User does not exist" }
> }
> ```

> r[bridge.response.protocol-error]
>
> For protocol errors (`UnknownMethod`, `InvalidPayload`, `Cancelled`),
> the response is a JSON object with `"error"` set to the error type.
> HTTP status is 200 OK.
>
> ```json
> { "error": "unknown_method" }
> ```

> r[bridge.response.bridge-error]
>
> Bridge-level errors (cannot reach backend, timeout, etc.) use
> appropriate HTTP status codes (502, 504, etc.) with a JSON body
> describing the error.
>
> ```json
> {
>   "error": "bridge",
>   "message": "Backend service unavailable"
> }
> ```

## Response Metadata

> r[bridge.response.metadata]
>
> Response metadata from the roam service is returned via HTTP headers
> with a `Roam-` prefix, mirroring request metadata handling.

# JSON Encoding

> r[bridge.json.facet]
>
> Request and response payloads are encoded using facet-json. The JSON
> representation of each type is determined by its Facet implementation
> and any serialization attributes. The bridge does not define its own
> JSON mapping.

See [facet-json documentation](https://facet.rs) for encoding details.

## Channels over HTTP

> r[bridge.json.channels-forbidden]
>
> Methods with `Tx<T>` or `Rx<T>` parameters MUST NOT be called via
> HTTP POST. The bridge MUST reject such calls with HTTP 400.
>
> ```json
> {
>   "error": "bridge",
>   "message": "Channel methods require WebSocket"
> }
> ```

# Idempotency

The bridge leverages roam's nonce mechanism (see `r[core.nonce]`) for
idempotent delivery.

> r[bridge.nonce.passthrough]
>
> When a request includes `Roam-Nonce`, the bridge MUST include the
> decoded nonce in the roam request metadata as `roam-nonce`.

> r[bridge.nonce.backend]
>
> Nonce deduplication is performed by the backend roam service, not
> the bridge. The bridge is stateless with respect to nonces.

> r[bridge.nonce.retry-safe]
>
> Clients retrying a request due to HTTP-level failures (connection
> reset, 502, timeout) MUST use the same nonce. This ensures at-most-once
> delivery even across bridge restarts.

# WebSocket

For methods with channels, the bridge provides a WebSocket endpoint.

> r[bridge.ws.subprotocol]
>
> The WebSocket subprotocol MUST be `roam-bridge.v1`. Clients MUST include
> `Sec-WebSocket-Protocol: roam-bridge.v1` in the upgrade request.
>
> ```
> GET /@ws HTTP/1.1
> Upgrade: websocket
> Connection: Upgrade
> Sec-WebSocket-Protocol: roam-bridge.v1
> ```

> r[bridge.ws.text-frames]
>
> All WebSocket messages MUST be text frames containing JSON. Binary
> frames are not used.

## Message Format

> r[bridge.ws.message-format]
>
> Each WebSocket message is a JSON object with a `type` field. The
> `type` determines which other fields are present.

### Request

> r[bridge.ws.request]
>
> Initiates an RPC call. The `id` is client-assigned and used to
> correlate responses. Channel IDs are allocated by the client.
>
> ```json
> {
>   "type": "request",
>   "id": 1,
>   "service": "Streaming",
>   "method": "subscribe",
>   "args": ["events"],
>   "metadata": { "traceparent": "00-..." }
> }
> ```

### Response

> r[bridge.ws.response]
>
> Completes an RPC call. Contains either `result` (success) or
> `error` (failure).
>
> ```json
> {
>   "type": "response",
>   "id": 1,
>   "result": { "channel": 1 }
> }
> ```
>
> ```json
> {
>   "type": "response",
>   "id": 1,
>   "error": "unknown_method"
> }
> ```
>
> ```json
> {
>   "type": "response",
>   "id": 1,
>   "error": "user",
>   "value": { "code": "INVALID" }
> }
> ```

### Data

> r[bridge.ws.data]
>
> Sends a value on a channel. Direction depends on the channel type
> (`Tx` = client→server, `Rx` = server→client).
>
> ```json
> {
>   "type": "data",
>   "channel": 1,
>   "value": { "event": "user_joined", "user": "alice" }
> }
> ```

### Close

> r[bridge.ws.close]
>
> Signals end of a `Tx` channel (client→server). Sent by the client
> when done sending. `Rx` channels close implicitly with Response.
>
> ```json
> {
>   "type": "close",
>   "channel": 1
> }
> ```

### Reset

> r[bridge.ws.reset]
>
> Forcefully terminates a channel. Either peer may send this.
>
> ```json
> {
>   "type": "reset",
>   "channel": 1
> }
> ```

### Credit

> r[bridge.ws.credit]
>
> Grants flow control credit (in bytes) for a channel. The receiver
> sends this to allow the sender to continue.
>
> ```json
> {
>   "type": "credit",
>   "channel": 1,
>   "bytes": 65536
> }
> ```

### Cancel

> r[bridge.ws.cancel]
>
> Requests cancellation of an in-flight RPC.
>
> ```json
> {
>   "type": "cancel",
>   "id": 1
> }
> ```

### Goodbye

> r[bridge.ws.goodbye]
>
> Signals connection termination due to protocol error. After sending
> or receiving Goodbye, the WebSocket SHOULD be closed.
>
> ```json
> {
>   "type": "goodbye",
>   "reason": "channeling.unknown"
> }
> ```

## Channel Lifecycle Examples

These examples illustrate how channels work over WebSocket. They are
non-normative — the message types are defined above.

**Rx channel** — A call to `fn subscribe(topic: String, events: Rx<Event>)`:

```aasvg
.--------.                                                         .--------.
| Client |                                                         | Bridge |
'---+----'                                                         '---+----'
    |                                                                  |
    +---request {id:1, args:["news"], channel:1} --------------------->|
    |                                                                  |
    |<----------------------- data {channel:1, value:{...}} -----------+
    |<----------------------- data {channel:1, value:{...}} -----------+
    |<-------------------- credit {channel:1, bytes:8192} -------------+
    |<----------------------- data {channel:1, value:{...}} -----------+
    |<---------------------- response {id:1, result:null} -------------+
    |                                                                  |
```

**Tx channel** — A call to `fn upload(data: Tx<Chunk>) -> Summary`:

```aasvg
.--------.                                                         .--------.
| Client |                                                         | Bridge |
'---+----'                                                         '---+----'
    |                                                                  |
    +---request {id:2, channel:3} ------------------------------------>|
    |---data {channel:3, value:{...}} -------------------------------->|
    |---data {channel:3, value:{...}} -------------------------------->|
    |<-------------------- credit {channel:3, bytes:8192} -------------+
    |-- data {channel:3, value:{...}} -------------------------------->|
    |-- close {channel:3} -------------------------------------------->|
    |                                                                  |
    |<---------------------- response {id:2, result:{...}} ------------|
    |                                                                  |
```

# Implementation Notes (Non-normative)

## Backend Connection

The bridge maintains roam connections to backend services. Options:

- **Connection pool**: Multiple persistent connections for throughput
- **Single connection**: Simpler, relies on roam pipelining
- **Per-request**: Higher latency, simpler resource management

## Error Mapping

The bridge translates between roam errors and HTTP:

| roam | HTTP | Body |
|------|------|------|
| `Ok(value)` | 200 | JSON value |
| `User(E)` | 200 | `{"error":"user","value":...}` |
| `UnknownMethod` | 200 | `{"error":"unknown_method"}` |
| `InvalidPayload` | 200 | `{"error":"invalid_payload"}` |
| `Cancelled` | 200 | `{"error":"cancelled"}` |
| Connection failed | 502 | `{"error":"bridge",...}` |
| Timeout | 504 | `{"error":"bridge",...}` |

Note: roam errors return HTTP 200 because they are valid protocol responses.
HTTP error codes indicate bridge/transport failures.
