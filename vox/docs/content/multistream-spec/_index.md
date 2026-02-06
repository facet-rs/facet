+++
title = "Multi-stream Transport Specification"
description = "roam protocol bindings for QUIC, WebTransport, and other multi-stream transports"
+++

# Multi-stream Transport Specification

This specification defines how the roam protocol operates over multi-stream
transports like QUIC and WebTransport. These transports provide multiple
independent streams, which can eliminate head-of-line blocking.

## Status

This specification is **not yet implemented**. It will be implemented when
QUIC/WebTransport support is added to roam.

## Control Stream

> r[multistream.control]
>
> Implementations MUST designate a **control stream** for control messages
> (Hello, Goodbye, Request, Response, Cancel, Credit). The initiator opens
> this stream first; the acceptor's first received stream is the control
> stream. Control messages are length-prefixed [POSTCARD]-encoded Message
> values.

## Stream Mapping

> r[multistream.streams]
>
> Implementations MUST map each roam stream to a dedicated unidirectional
> transport stream. roam streams are unidirectional (see `r[core.stream]`
> in the main roam spec).

> r[multistream.stream-id-header]
>
> Each dedicated transport stream MUST begin with a 8-byte header containing
> the roam `stream_id` (little-endian u64). This allows the receiver to
> associate the transport stream with the stream ID from the Request/Response
> payload.

> r[multistream.stream-id-mapping]
>
> The data sender opens a transport stream, writes the stream ID header,
> then sends data. For `Push<T>` the caller opens it; for `Pull<T>` the
> callee opens it. The receiver reads the header to determine which roam
> stream this transport stream carries.

Note: Transport stream IDs (e.g., QUIC stream IDs) are transport-specific
and not visible to roam. The roam `stream_id` is allocated by the caller
according to the binding's scheme (e.g., `r[streaming.id.parity]` for
peer-to-peer).

## Stream Data

> r[multistream.stream-data]
>
> After the stream ID header, data is sent as length-prefixed [POSTCARD]-
> encoded values of the stream's element type `T`. No Message wrapper is
> needed — the stream identity was established by the header.

## Stream Lifecycle

> r[multistream.stream-close]
>
> Closing a roam stream is signaled by closing the transport stream
> (e.g., QUIC FIN). The Close message is not used on multi-stream transports.

> r[multistream.stream-reset]
>
> Resetting a roam stream is signaled by resetting the transport stream
> (e.g., QUIC RESET_STREAM). The Reset message is not used on multi-stream
> transports.

## Why Length Prefix on Control Streams?

QUIC streams are byte streams, not message streams. We need framing.
Length-prefix framing provides:
- Guaranteed message boundaries
- O(1) frame boundary parsing
- Consistent framing with roam byte-stream transports
[POSTCARD]: https://postcard.jamesmunns.com/
