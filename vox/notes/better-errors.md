# Better Vox Errors Roadmap

Vox currently flattens too many runtime failures into low-actionability enums such as `Unknown`, `Protocol`, `Transport`, `ConnectionClosed`, or `InvalidPayload(String)`. That makes failures hard to diagnose from API errors, observer events, snapshots, and logs.

This note is the roadmap for a dedicated error cleanup branch. The current branch should only make the minimal changes needed to stop swallowing connection driver receive failures and channel connection-close causes.

## Goals

- Preserve actionable cause chains across runtime boundaries.
- Keep public APIs stable where possible, but stop erasing useful details internally.
- Make observer events, debug snapshots, and returned errors tell the same story.
- Keep metrics labels low-cardinality while allowing logs/snapshots to carry high-cardinality diagnostic context.
- Bound payload samples and strings so diagnostics are useful without dumping unbounded user data.

## Non-Goals

- Do not turn every internal error into a metrics label.
- Do not expose transport-specific implementation details as required wire protocol.
- Do not block the current channel reliability fix on this larger redesign.

## Problems To Fix

- `ConnectionCloseReason` is only a coarse enum; `Protocol` and `Transport` hide the real source error.
- `DecodeErrorKind` loses type/message context, offset, payload length, and sample bytes.
- `EncodeErrorKind` loses the message family/type and serialization failure detail.
- `ProtocolErrorKind` says which broad bucket failed but not the bad ID, state, or invariant.
- `VoxError::ConnectionClosed`, connection shutdown, and `SendFailed` do not carry a close/send cause.
- `VoxError::InvalidPayload(String)` is ad hoc while observer decode errors use a separate lossy enum.
- Connection driver receive errors can currently become connection/channel teardown without enough structured context.
- Snapshots record close reasons but not rich terminal errors.

## Proposed Types

Add richer diagnostic structs and use them inside existing event/error enums.

```rust
pub struct VoxErrorContext {
    pub message: String,
    pub source: Option<String>,
}

pub struct PayloadSample {
    pub total_len: usize,
    pub offset: Option<usize>,
    pub bytes_before: Vec<u8>,
    pub bytes_at: Vec<u8>,
    pub truncated: bool,
}

pub struct DecodeErrorDetail {
    pub kind: DecodeErrorKind,
    pub message: String,
    pub target_type: Option<&'static str>,
    pub message_family: Option<&'static str>,
    pub lane_id: Option<LaneId>,
    pub payload_len: Option<usize>,
    pub offset: Option<usize>,
    pub sample: Option<PayloadSample>,
}

pub struct EncodeErrorDetail {
    pub kind: EncodeErrorKind,
    pub message: String,
    pub source_type: Option<&'static str>,
    pub message_family: Option<&'static str>,
    pub lane_id: Option<LaneId>,
}

pub struct ProtocolErrorDetail {
    pub kind: ProtocolErrorKind,
    pub message: String,
    pub lane_id: Option<LaneId>,
    pub request_id: Option<RequestId>,
    pub channel_id: Option<ChannelId>,
}

pub enum ConnectionCloseReason {
    Local,
    Remote,
    CallerDropped,
    ConnectionShutdown,
    Decode(DecodeErrorDetail),
    Encode(EncodeErrorDetail),
    Protocol(ProtocolErrorDetail),
    Transport(VoxErrorContext),
    Unknown(VoxErrorContext),
}
```

Exact naming can change, but the key point is that close reasons must carry the underlying actionable cause.

## Public API Direction

- Keep `TrySendError<T>` as `Full(T) | Closed(T)` because preserving `T` is the primary API requirement.
- Add optional cause accessors where practical, for example `TxError::Closed { reason }` or `VoxError::ConnectionClosed { reason }`.
- Avoid placing large diagnostic payloads in hot-path clone/copy events. Prefer `Arc<...>` for detailed structs if observer events must stay cheap.
- Consider splitting user-facing stable errors from internal diagnostic errors if backwards compatibility is important.

## Observer Events

- Replace `DriverEvent::DecodeError { kind }` with `DriverEvent::DecodeError { detail }`.
- Replace `DriverEvent::EncodeError { kind }` with `DriverEvent::EncodeError { detail }`.
- Replace `DriverEvent::ProtocolError { kind }` with `DriverEvent::ProtocolError { detail }`.
- Keep low-cardinality `kind` fields inside details for metrics adapters.
- Keep high-cardinality fields, IDs, and samples available for logs/snapshots, not default metric labels.

## Debug Snapshots

- Store `ConnectionCloseReason` with rich details in `ConnectionDebugSnapshot.close_reason`.
- Store channel terminal cause in `ChannelDebugSnapshot.close_reason` plus the owning connection close reason when the channel died due to connection teardown.
- Add a bounded `last_error` or `terminal_error` field if close reason becomes too semantically narrow.

## Decode Diagnostics

- Capture the type being decoded at conduit boundaries, at least `std::any::type_name::<F::Msg<'static>>()`.
- Capture payload length before decode.
- For postcard decode errors with an offset, capture a small sample around the offset, for example 16 bytes before and 16 bytes at/after.
- For `UnexpectedEof`, include expected context if available; otherwise include payload length and offset.
- Distinguish empty frame, truncated frame, schema mismatch, unknown variant, and trailing bytes.

## Transport Diagnostics

- Stream framing should distinguish clean EOF before a frame from EOF in the middle of a frame header/body.
- A zero-length frame should be explicit in diagnostics, not reported only as postcard `unexpected EOF at byte 0`.
- Include frame length and bytes read when frame decoding fails.

## Migration Plan

1. Add rich detail structs in `vox-types` without replacing every existing enum variant.
2. Thread rich close reasons through connection close state, driver shared state, channel teardown, and snapshots.
3. Change conduit decode errors to attach target type, payload len, offset, and bounded sample.
4. Change stream transport to report partial-frame context.
5. Update `VoxError` to carry optional runtime close/send causes.
6. Update tracing observer formatting to print details compactly.
7. Add tests that assert actionable details for malformed frames, decode failures, protocol errors, and channel teardown.
8. Revisit public semver impact and decide whether older variants need compatibility shims.

## Test Cases

- Malformed payload returns a decode error with type name, payload length, offset, and sample bytes.
- Empty stream frame reports empty frame or decode context, not generic connection closed.
- Partial stream header/body reports transport framing EOF with bytes read.
- Channel `Rx::recv` during connection teardown returns a connection-closed error with the same close reason seen in snapshots.
- RPC caller waiting for a response receives `VoxError::ConnectionClosed` with cause.
- Metrics adapter derives only low-cardinality labels from rich details.
