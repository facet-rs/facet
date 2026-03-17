# Flexible Wire Protocol

Design notes for making the roam wire protocol itself schema-negotiated,
so protocol evolution doesn't require breaking changes.

## Handshake (CBOR)

Four message types, all CBOR-encoded structs (not part of the enum):

- **Hello** (initiator → acceptor): carries initiator's `MessagePayload` schema
- **HelloYourself** (acceptor → initiator): carries acceptor's schema (or **Sorry**)
- **LetsGo** (initiator → acceptor): confirms compatibility (or **Sorry**)
- **Sorry** (either direction): structured CBOR rejection with detailed
  schema diff — "here's what I need, here's what you have, here's the gap"

Three-way handshake. After `LetsGo`, both sides have translation plans
for each other's `MessagePayload`.

## Post-handshake (postcard)

Everything is one postcard-encoded enum (`MessagePayload`) whose schema
was exchanged during handshake. Both sides deserialize through translation
plans. Protocol evolves the same way user types evolve — add variants,
add fields, reorder — all handled by plans.

The sender MUST NOT send a variant the receiver doesn't have in their
schema. No silent drops, no unknown-variant runtime errors. The sender
knows the receiver's capabilities from the handshake.

If the peer is missing a variant you can live without (e.g. `Telemetry`),
don't send it. If they're missing something required (e.g. `RequestMessage`),
reject with `Sorry` during handshake.

## Schema tracking — two levels

- **Per-session** tracker for protocol types. Exchanged once in the
  handshake, immutable after `LetsGo`. Transparent reconnection
  (StableConduit) doesn't affect it. Session resumption = new handshake
  = re-exchange.

- **Per-connection** trackers for user/service types. Exchanged lazily
  per method. Scoped per connection because connections can terminate at
  different peers (proxy transparency).

## Trailing fields and opaque payloads

`RequestCall` has `#[facet(trailing)] pub args: Payload<'payload>`.
The trailing attribute means the payload consumes all remaining bytes —
no length prefix, no varint. The point is not saving the varint; it's
**avoiding computing the payload length** before writing it.

This raises the question: why do we have scatter plans at all? Scatter
plans exist to compute the encoded layout (segments + sizes) up front.
But trailing fields exist precisely to avoid needing the length. There's
a tension here that needs resolving.

## No more trailing

The conduit requires the total message size for buffer allocation, so
the payload size must be computed upfront anyway. This makes
`#[facet(trailing)]` useless — it was meant to avoid computing the
payload length, but the conduit already forces that.

Without trailing, opaque payloads are just length-prefixed byte buffers
borrowed from the input. Deserialization is deferred — the protocol
layer sees `&[u8]`, the per-connection user schema interprets it later.

## Two spec layers

1. **CBOR bootstrap**: the handshake (`Hello`, `HelloYourself`, `LetsGo`,
   `Sorry`). Fixed forever — can't schema-negotiate the schema
   negotiation. Defines how peers exchange their `MessagePayload` schemas.

2. **Well-known enum variant semantics**: the meaning of `MessagePayload`
   variants like `RequestMessage`, `ResponseMessage`, `SchemaMessage`,
   `Ping`, `Pong`, `ConnectionOpen`, etc. The *structure* of these is
   schema-negotiated (fields can evolve). The *semantics* — what it means
   to receive a `RequestMessage`, how to respond, the state machine — are
   specified in prose.
