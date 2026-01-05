# Gaps: `docs/content/` spec (Jan 5, 2026) vs Rust implementation

Scope:
- Treat `docs/content/**` as read-only (canonical spec).
- Rust implementation today is primarily in:
  - `rust-legacy/rapace-protocol/src/lib.rs`
  - `rust-legacy/rapace-core/src/session.rs`
  - `rust-legacy/rapace-core/src/descriptor.rs`
  - `rust-legacy/rapace-core/src/transport/**`
  - `rust-legacy/rapace-macros/src/lib.rs`
  - `rust-legacy/rapace-core/src/transport/shm/**`

## Executive summary

The Rust implementation is still built around an older **frame + channel** protocol (control channel 0 + `OpenChannel`/`CancelChannel`/`GrantCredits`, per-channel message IDs, descriptor-based framing, etc.). The new spec in `docs/content/spec/_index.md` defines a different **message** protocol (single `Message` enum with `Request{request_id,..}`, `Data{stream_id,..}`, explicit `Credit`, and `Goodbye{reason}`), and different transport framing rules (COBS for byte streams, “one POSTCARD message per WebSocket message” for message transports).

This is not a small “missing feature” list; it’s a **protocol model mismatch**.

## Major mismatches (highest impact)

### 1) Wire model: `Frame`/`MsgDescHot` vs `Message`

Spec:
- `docs/content/spec/_index.md#L929` defines `enum Message { Hello, Goodbye, Request/Response/Cancel, Data/Close/Reset/Credit }`.

Rust impl:
- Uses `Frame { desc: MsgDescHot, payload: Payload }` (`rust-legacy/rapace-core/src/frame.rs#L84`) and “control verbs” on channel 0 (`rust-legacy/rapace-core/src/control.rs#L16`).
- `MsgDescHot` identity is `(msg_id, channel_id, method_id)` (`rust-legacy/rapace-core/src/descriptor.rs#L31`), not `(request_id|stream_id, msg_type, ...)` as in the new spec.
- Inline payload size is 16 bytes (`rust-legacy/rapace-core/src/descriptor.rs#L10`), but SHM spec requires 32 bytes inline for descriptors (`docs/content/shm-spec/_index.md#L222`).

### 2) Hello handshake shape + negotiation

Spec:
- Hello is versioned: `enum Hello { V1 { max_payload_size, initial_stream_credit } }` (`docs/content/spec/_index.md#L975`).
- Both peers send Hello “immediately”, before any other message (`docs/content/spec/_index.md#L954`).
- Effective limits are the min of advertised values (`docs/content/spec/_index.md#L988`).

Rust impl:
- Hello is a struct with protocol version, role, feature bits, limits, and method registry (`rust-legacy/rapace-protocol/src/lib.rs#L288`).
- Session handshake is ordered “initiator sends first; acceptor receives first” (`rust-legacy/rapace-core/src/session.rs#L1034`) rather than “both send immediately”.
- There is no negotiation of `initial_stream_credit` or of min/max payload sizing as described in `docs/content/spec/_index.md` (Rust sends `Limits::default()` and negotiates feature bits instead; `rust-legacy/rapace-core/src/session.rs#L895`).

### 3) Call correlation: `request_id` vs `channel_id`/`msg_id`

Spec:
- Call identity is `request_id` (u64) and must be unique/monotonic within a connection (`r[core.call.request-id]` in `docs/content/spec/_index.md`, see `docs/content/spec/_index.md#L82`).
- Multiple calls can be in flight concurrently over one connection (`docs/content/spec/_index.md#L602`).

Rust impl:
- Requests are sent on a per-call `channel_id` and are correlated by the channel waiter, not by a `request_id` (`rust-legacy/rapace-core/src/session.rs#L1246`).
- `msg_id` is per-frame and monotonic (`rust-legacy/rapace-core/src/session.rs#L418`), but it is not used as a request-scoped “call id” in the new spec sense.

### 4) Streams + flow control: `stream_id` + `Credit` vs STREAM channels + `GrantCredits`

Spec:
- Stream identity is `stream_id` (u64). Data uses `Data{stream_id,payload}`. Flow control uses explicit `Credit{stream_id,bytes}` (`docs/content/spec/_index.md#L940`).

Rust impl:
- Streaming is modeled as separate channel kinds (`ChannelKind::Stream`) and a control-plane `GrantCredits { channel_id, bytes }` (`rust-legacy/rapace-protocol/src/lib.rs#L448`), not `Credit{stream_id,..}`.

### 5) Connection errors: `Goodbye{reason}` (rule IDs) vs `GoAway`/local errors

Spec:
- Protocol violations are connection errors; peer must send `Goodbye{reason}` and close; `reason` must include the violated rule ID (`docs/content/spec/_index.md#L995`).

Rust impl:
- Has `GoAway` control message (`rust-legacy/rapace-protocol/src/lib.rs#L496`), but no `Goodbye` message type matching the new spec.
- Many protocol checks return local `RpcError::Status` strings like `"[verify handshake.ordering]: ..."` rather than emitting a spec-compliant `Goodbye` (`rust-legacy/rapace-core/src/session.rs#L946`).

### 6) Transport framing: COBS vs raw descriptor+payload

Spec:
- TCP/byte-stream framing uses COBS with 0x00 delimiters (`docs/content/spec/_index.md#L1092`).
- WebSocket/message transports carry exactly one POSTCARD-encoded `Message` per transport message (`docs/content/spec/_index.md#L1034`).

Rust impl:
- WebSocket transport sends `[desc bytes][payload bytes]` with a fixed 64-byte descriptor prefix (`rust-legacy/rapace-core/src/transport/websocket.rs#L239`).
- No COBS framing implementation exists in the legacy Rust code (`rg -n "\\bcobs\\b|COBS" rust-legacy` returns no matches).

## Rust-specific method identity gaps

Spec (Rust implementation spec):
- Method ID is u64, computed using BLAKE3 over normalized service/method + signature hash (`docs/content/rust-spec/_index.md#L18`).

Rust impl:
- Method IDs are u32 FNV-1a hashes (protocol + macros): `rust-legacy/rapace-protocol/src/lib.rs#L619`, `rust-legacy/rapace-macros/src/lib.rs#L16`.

## SHM transport gaps (new shm-spec vs current hub SHM)

Spec:
- Segment header is 128 bytes and magic must be exactly `RAPAHUB\x01` (`docs/content/shm-spec/_index.md#L115`, `docs/content/shm-spec/_index.md#L133`).
- Descriptor format is `MsgDesc { msg_type:u8, flags:u8, id:u32, method_id:u64, ..., inline_payload:[u8;32] }` (`docs/content/shm-spec/_index.md#L222`).
- Peer IDs are u8, max guests ≤ 255 (`docs/content/shm-spec/_index.md#L59`).

Rust impl:
- Hub magic is `RAPAHUB\\0` (`rust-legacy/rapace-core/src/transport/shm/hub_layout.rs#L16`) and header size is 256 bytes (`rust-legacy/rapace-core/src/transport/shm/hub_layout.rs#L65`).
- Peer IDs/max peers are u16 (`rust-legacy/rapace-core/src/transport/shm/hub_layout.rs#L21`).
- Descriptor (`MsgDescHot`) is channel-based and has 16-byte inline payload (`rust-legacy/rapace-core/src/descriptor.rs#L31`).

## Notes / questions to resolve early

1) `docs/content/implementors/transports.md` is explicitly non-normative and still describes the **frame** model; it appears out of sync with `docs/content/spec/_index.md`’s **Message** model. Before implementation work, we should confirm whether:
   - the intention is to rewrite Rust to match `docs/content/spec/_index.md`, or
   - `docs/content/spec/_index.md` is a newer direction and the “frame model” docs are still being migrated.

2) If the new spec is canonical, the Rust implementation likely needs:
   - a new `rapace-protocol` message enum that matches `docs/content/spec/_index.md`,
   - a new transport codec (COBS framing for byte streams; WebSocket “one message per frame”),
   - rework of call/stream identity and flow control (request_id/stream_id/Credit),
   - SHM layout + descriptor rewrite to match `docs/content/shm-spec/_index.md`,
   - method-id rewrite to match `docs/content/rust-spec/_index.md`.
