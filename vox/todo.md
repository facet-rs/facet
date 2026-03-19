# TypeScript parity TODO

This document is the durable handoff / tracking checklist for bringing the TypeScript implementation to parity with the Rust implementation and therefore the current Roam specification.

## Goal

The goal is **not** to merely get a failing test green.

The goal is to make TypeScript match:

1. the **Rust implementation** as the source of truth
2. the **current spec**
3. the intended cross-language behavior

Tests are validation, not the target.

---

## Ground rules

- Treat Rust as canonical whenever semantics are unclear.
- Prefer removing stale TypeScript-era assumptions over preserving old behavior.
- Do not preserve legacy postcard-handshake logic just because old TS code/tests reference it.
- Track parity at the protocol level first, then update tests/helpers/docs.
- Avoid "compat shims" unless Rust/spec explicitly requires compatibility behavior.

---

## Current state summary

A partial start has already happened on this branch:

- Raw CBOR handshake helper code was started in `typescript/packages/roam-core/src/handshake.ts`.
- `xtask` was updated to generate/export `wireMessageSchemasCbor`.
- `SchemaTracker` was partially updated to accept current Rust `TypeSchemaId` encoding.
- `session.ts` was partially moved toward raw-handshake startup for transport-based APIs.

However, the TS runtime still contains major stale assumptions that conflict with the newer Rust/spec model, especially around:

- handshake modeling
- wire types
- schema exchange
- opaque payload framing
- stable conduit integration
- retry persistence / operation storage
- channel continuity semantics

At the time of writing, TypeScript also has known compile/diagnostic failures in at least:

- `typescript/packages/roam-wire/src/types.ts`
- `typescript/packages/roam-core/src/session.ts`
- `typescript/packages/roam-wire/src/types.generated.ts`

---

## Canonical Rust/spec semantics to mirror

## 1. Transport and handshake ordering

Canonical order:

1. transport prologue selects `bare` or `stable`
2. raw **CBOR session handshake**
3. build `MessagePlan` from exchanged protocol schemas
4. begin postcard `Message` traffic
5. if using stable conduit, its own handshake/framing layer must be integrated in the same order Rust uses

Important consequence:

- `Hello` and `HelloYourself` are **not** postcard `MessagePayload` variants anymore
- they are handshake types exchanged **before** postcard `Message` traffic

### References

- `rust/roam-types/src/handshake.rs`
- `rust/roam-core/src/handshake.rs`
- `rust/roam-core/src/lib.rs` (`MessagePlan::from_handshake`)
- `docs/content/spec/conn.md`

---

## 2. Handshake-level schema exchange

The CBOR handshake exchanges the peer's schema for the postcard protocol layer.

Specifically, Rust handshake messages carry:

- `connection_settings`
- retry support
- resume key
- `message_payload_schema`

Purpose:

- bootstrap decoding of postcard `MessagePayload` across schema evolution

Important rule:

- protocol schemas are exchanged **during handshake**
- transparent reconnect via `StableConduit` does **not** re-exchange protocol schemas
- session resumption via **new handshake** does re-exchange them

### References

- `rust/roam-types/src/handshake.rs`
- `rust/roam-core/src/handshake.rs`
- `docs/content/spec/conn.md`

---

## 3. Request/response schema exchange

Schemas are also used at the application payload level, not just at handshake.

Rust currently sends per-method schemas by inlining CBOR payloads into:

- `RequestCall.schemas`
- `RequestResponse.schemas`

There is no standalone `SchemaMessage` in the current Rust flow.

### Send side

Owned by:

- `rust/roam-core/src/session/mod.rs` (`SessionCore::send`)
- `rust/roam-types/src/schema.rs` (`SchemaSendTracker::prepare_send_for_method`)

Rules:

- sender-driven
- no round trips
- first use per method+direction on a connection
- once per type per connection
- include transitive schema dependencies
- empty CBOR payload means nothing new to send

### Receive side

Owned by:

- `rust/roam-core/src/session/mod.rs` (`Session::handle_message`)
- `rust/roam-types/src/schema.rs` (`SchemaRecvTracker::record_received`)

Rules:

- parse inlined schema CBOR from incoming request/response
- record schemas **before** routing/dispatch
- duplicate `TypeSchemaId` on same connection is a protocol error
- bindings are direction-specific (`Args` vs `Response`)
- all schema IDs are per-connection

### References

- `rust/roam-core/src/session/mod.rs`
- `rust/roam-types/src/schema.rs`

---

## 4. Opaque payload framing

Opaque payloads now have uniform framing.

Wire shape for opaque payload fields:

- 4-byte little-endian length prefix (`u32le`)
- followed by raw postcard payload bytes

This applies to:

- `RequestCall.args`
- `RequestResponse.ret`
- `ChannelItem.item`
- any other opaque adapter field

Important consequences:

- not varint length
- not CBOR bytes
- not special trailing bytes without framing
- passthrough still uses the same outer framing

### References

- `rust/roam-postcard/src/decode.rs` (`read_opaque_bytes`)
- `rust/roam-postcard/src/deserialize.rs`
- `rust/roam-types/src/message.rs` (`Payload`, `PayloadAdapter`)

---

## 5. Session resumption

Rust session resumption:

- preserves session-scoped state
- preserves operation ID scope
- does **not** preserve in-flight request attempts on the failed attachment
- does **not** preserve in-flight response deliveries on the failed attachment
- unresolved operations continue by creating a **new request attempt** for the same operation

On resume, Rust resets per-connection schema state:

- send-side schema state reset
- receive-side schema state reset
- protocol schemas re-exchanged through new handshake
- root settings must match

### References

- `rust/roam-core/src/session/mod.rs`
- `docs/content/spec/conn.md`
- `docs/content/spec/retry.md`
- `docs/content/spec/intro.md`

---

## 6. Stable conduit semantics

`StableConduit` is a lower layer than session resumption.

It provides:

- automatic reconnect over fresh links
- packet sequence numbering
- acknowledgements
- replay of unacked frames
- duplicate suppression
- continuity of conduit traffic across link interruption

It is **not** the same thing as session resumption.

Important distinction:

- stable conduit continuity alone does not settle ambiguous RPC outcomes
- retry/session resumption semantics still govern operation continuity

Docs explicitly describe stable conduit reconnect as preserving state/channels across reconnect.

### References

- `rust/roam-core/src/stable_conduit/mod.rs`
- `docs/content/spec/conn.md`
- `docs/content/guides/rust.md` (`stable_conduit_reconnect`)

---

## 7. Retry and operation continuity

Rust has a real operation persistence model.

Important spec/Rust expectations:

- retry is defined in terms of **operations**, not request attempts
- `persist` creates a durability obligation
- sealed outcomes for persist operations must be durably recorded
- operation ID scope persists across session resumption
- durable operation storage is distinct from transient in-memory tracking

TypeScript currently does not match this fully.

### References

- `docs/content/spec/retry.md`
- Rust operation-store wiring in `rust/roam-core`

---

## Major parity gaps in TypeScript

## A. `roam-wire` still models stale handshake/message shapes

### Problem

`typescript/packages/roam-wire/src/types.ts` still assumes:

- `Hello`
- `HelloYourself`
- `messageHello(...)`
- `messageHelloYourself(...)`

belong to postcard `Message`.

But Rust moved these to the raw CBOR handshake.

`types.generated.ts` also no longer exports the types that `types.ts` expects, causing drift and diagnostics.

### TODO

- [ ] Remove stale postcard-level `Hello` / `HelloYourself` assumptions from `roam-wire`
- [ ] Stop constructing `Message` values with payload tag `"Hello"` or `"HelloYourself"`
- [ ] Decide whether handshake types should live in a dedicated TS handshake module instead of `roam-wire` postcard message helpers
- [ ] Update `roam-wire/src/index.ts` exports accordingly
- [ ] Regenerate `types.generated.ts` only from actual postcard message shapes
- [ ] Make TS wire helpers reflect Rust's current `RequestCall` / `RequestResponse` shapes, including `schemas`

### Files

- `typescript/packages/roam-wire/src/types.ts`
- `typescript/packages/roam-wire/src/index.ts`
- `typescript/packages/roam-wire/src/types.generated.ts`

---

## B. `session.ts` still contains old message-handshake assumptions

### Problem

`typescript/packages/roam-core/src/session.ts` still contains stale postcard-handshake assumptions in several places, including:

- message switching on `"Hello"` / `"HelloYourself"`
- use of `messageHello(...)`
- use of `messageHelloYourself(...)`
- old resume path assumptions
- stale `SchemaMessage` handling

### Required direction

Match Rust:

- transport-based APIs do raw CBOR handshake before postcard `Conduit<Message>` traffic
- session establishment from a conduit should be thought of as **post-handshake**
- schema receive/send should be tied to request/response `schemas` fields
- no standalone `SchemaMessage`

### TODO

- [ ] Remove `Hello` / `HelloYourself` cases from postcard `Message` runtime handling
- [ ] Remove standalone `SchemaMessage` handling from session runtime
- [ ] Read/record inlined schema CBOR from `RequestCall.schemas` and `RequestResponse.schemas`
- [ ] Attach schemas when sending first request/response for method+direction on a connection
- [ ] Reset send/receive schema state correctly on session resumption
- [ ] Mirror Rust session establishment model: handshake result first, then session
- [ ] Rework or deprecate `Session.establishInitiator` / `Session.establishAcceptor` if they still imply old postcard handshake semantics
- [ ] Distinguish clearly between:
  - stable conduit reconnect
  - session resumption
- [ ] Remove old resume protocol based on postcard hello metadata if Rust no longer uses that path

### Files

- `typescript/packages/roam-core/src/session.ts`
- possibly `typescript/packages/roam-core/src/index.ts`

---

## C. `SchemaTracker` still thinks in terms of old standalone schema messages

### Problem

`typescript/packages/roam-core/src/schema_tracker.ts` and `cbor.ts` were written around the old `SchemaMessagePayload` framing model.

The tracker itself may still be useful, but the integration point is stale.

### TODO

- [ ] Keep the CBOR parsing logic, but integrate it with inlined request/response `schemas`
- [ ] Stop describing the receive path as standalone `SchemaMessage` if Rust no longer does that
- [ ] Ensure method bindings remain direction-specific
- [ ] Keep support for current `TypeSchemaId` encoding from Rust
- [ ] Validate duplicate-type handling semantics against Rust
- [ ] Ensure schema tracker lifecycle is per connection, not global

### Files

- `typescript/packages/roam-core/src/schema_tracker.ts`
- `typescript/packages/roam-core/src/cbor.ts`

---

## D. Opaque payload framing likely does not match Rust yet

### Problem

Every request/response payload and channel item is an opaque field. TS must match Rust's `u32le + bytes` framing exactly.

This needs verification across all TS postcard encode/decode paths.

### TODO

- [ ] Audit TS postcard implementation for opaque adapter framing
- [ ] Ensure opaque reads use 4-byte little-endian length prefix
- [ ] Ensure opaque writes emit 4-byte little-endian length prefix
- [ ] Ensure passthrough bytes still use same outer framing
- [ ] Verify request args / response ret / channel item all match Rust wire encoding
- [ ] Add/port golden-vector tests against Rust fixtures

### Likely files

- `typescript/packages/roam-postcard/...`
- `typescript/packages/roam-wire/...`
- any TS encode/decode helpers for payload-bearing messages

---

## E. Stable conduit integration is not yet parity-complete

### Problem

TS has a `StableConduit`, but current session/transport integration is not fully aligned with Rust/spec.

The current branch also explicitly throws for stable raw-handshake transport setup in some paths.

### Required semantics

Match Rust ordering and responsibilities:

1. transport prologue chooses `stable`
2. raw CBOR session handshake
3. build message plan from handshake schema exchange
4. stable conduit setup / replay / reconnect semantics
5. continue postcard message traffic with continuity guarantees

### TODO

- [ ] Audit `typescript/packages/roam-core/src/stable_conduit.ts` against Rust stable conduit behavior
- [ ] Verify packet framing details
- [ ] Verify replay/ack trimming semantics
- [ ] Verify duplicate suppression semantics
- [ ] Verify reconnect handshake (`ClientHello` / `ServerHello`) ordering relative to session handshake
- [ ] Ensure `StableConduit` carries post-handshake postcard `Message`s, not pre-handshake traffic
- [ ] Integrate message translation plan with stable conduit receive path if needed
- [ ] Remove current `"stable not implemented yet"` transport-path placeholders once parity is implemented
- [ ] Validate that stable reconnect preserves channel continuity as Rust/docs describe

### Files

- `typescript/packages/roam-core/src/stable_conduit.ts`
- `typescript/packages/roam-core/src/session.ts`
- `typescript/packages/roam-core/src/transport_prologue.ts`

---

## F. TypeScript does not have a Rust-style durable operation store

### Problem

TS currently has only an in-memory `OperationRegistry` in `driver.ts`.

That is not equivalent to Rust's operation store / durability story.

### Current TS behavior

TS can:

- admit operations
- attach duplicate requests
- replay sealed outcomes
- track released/indeterminate state

But only in-process and only in memory.

### Missing parity

- durable operation persistence
- restart-surviving operation records
- durable sealing for `persist`
- satisfying spec durability obligations for persist methods

### TODO

- [ ] Introduce a real operation-store abstraction in TS, mirroring Rust's concept
- [ ] Separate transient registry behavior from durable persistence behavior
- [ ] Ensure `persist` creates an actual durability obligation
- [ ] Ensure sealed persist outcomes are durably recorded before reporting sealed
- [ ] Ensure admitted persist operations are recoverable across process restart if required by spec
- [ ] Audit current `driver.ts` retry behavior against `docs/content/spec/retry.md`

### Files

- `typescript/packages/roam-core/src/driver.ts`
- `typescript/packages/roam-core/src/retry.ts`
- generated descriptors that expose retry policy

---

## G. Session resumption does not preserve channel continuity in TS

### Problem

Current TS `session.ts` explicitly closes all channels on session resume:

- `this.channelRegistry.closeAll()`

Then it retries requests with fresh request IDs / fresh channels if retry machinery is available.

That is not "seamless channel continuity."

### Important distinction

According to spec/Rust:

- session resumption does not preserve in-flight request attempts
- operation continuity may continue by creating a new request attempt
- stable conduit reconnect is the place where transparent channel continuity is expected

So the parity question here is layered:

1. TS session resumption semantics must match spec
2. TS stable conduit must provide its continuity story correctly
3. TS should not blur the two layers

### TODO

- [ ] Audit current channel-closing-on-resume behavior against Rust/spec
- [ ] Clarify what must happen for:
  - bare + session resumption
  - stable conduit reconnect
  - stable + session resumption
- [ ] Ensure TS does not incorrectly claim seamless channel continuity where only retry/new-attempt semantics apply
- [ ] Ensure TS stable conduit path preserves channels/state the way Rust/docs intend
- [ ] Update tests to distinguish stable reconnect continuity from session resumption retry semantics

### Files

- `typescript/packages/roam-core/src/session.ts`
- `typescript/packages/roam-core/src/stable_conduit.ts`
- channeling runtime pieces under `typescript/packages/roam-core/src/channeling/`

---

## H. Old `connection.ts` stack looks stale relative to current architecture

### Problem

`typescript/packages/roam-core/src/connection.ts` and related tests still model old Hello/HelloYourself postcard exchange.

That appears to be a pre-parity architecture and is now in conflict with current Rust/spec semantics.

### TODO

- [ ] Determine whether `connection.ts` is still part of the intended public runtime architecture
- [ ] If stale, deprecate/remove it
- [ ] If retained, rewrite it around current handshake/wire semantics
- [ ] Remove use of postcard-level hello exchange from tests/helpers
- [ ] Update all dependent tests

### Files

- `typescript/packages/roam-core/src/connection.ts`
- `typescript/packages/roam-core/src/connection.channeling.test.ts`
- `typescript/packages/roam-core/src/connection.keepalive.test.ts`

---

## I. `roam-wire` message helper payload shapes are out of date

### Problem

Current handwritten TS helpers in `types.ts` do not fully match Rust message structs.

For example, request/response helpers need to account for fields like:

- `schemas`

and should align exactly with Rust message layout.

### TODO

- [ ] Ensure `messageRequest` includes `schemas`
- [ ] Ensure `messageResponse` includes `schemas`
- [ ] Audit all handwritten message helper constructors against `rust/roam-types/src/message.rs`
- [ ] Verify discriminants and helper return types remain sound

### Files

- `typescript/packages/roam-wire/src/types.ts`

---

## J. Need protocol-schema generation to stay aligned with new handshake model

### Problem

We added `wireMessageSchemasCbor`, but TS generation and exports need to reflect the separation between:

- handshake schema exchange
- postcard message types
- any dedicated handshake TS types if needed

### TODO

- [ ] Keep `wireMessageSchemasCbor` generation
- [ ] Decide whether TS should also generate handshake type definitions/schemas from Rust `handshake.rs`
- [ ] Ensure codegen does not imply `Hello` / `HelloYourself` are postcard message payloads
- [ ] Verify generated docs/comments match current architecture

### Files

- `xtask/src/main.rs`
- `typescript/packages/roam-wire/src/schemas.generated.ts`
- potentially new generated handshake artifacts if added

---

## K. Tests need to be updated to validate the new reality, not preserve the old one

### Problem

A number of TS tests still assume postcard hello exchange or stale runtime layering.

Those tests are useful only if rewritten to validate the current protocol.

### TODO

- [ ] Update tests to use raw CBOR handshake where appropriate
- [ ] Remove tests that depend on stale postcard Hello/HelloYourself semantics
- [ ] Add/port golden-vector coverage for:
  - handshake messages
  - opaque payload framing
  - request/response schemas
  - stable conduit reconnect behavior
- [ ] Add parity tests against Rust subjects/harnesses where possible
- [ ] Ensure browser/inprocess tests use current transport/session semantics

### Known likely stale test areas

- `typescript/packages/roam-core/src/connection.channeling.test.ts`
- `typescript/packages/roam-core/src/connection.keepalive.test.ts`
- any tests around old `connection.ts` hello exchange

---

## Concrete implementation plan

## Phase 1 — Repair wire model and remove stale handshake assumptions
- [ ] Fix `roam-wire/src/types.ts` so it reflects current Rust postcard message shapes
- [ ] Remove postcard hello helpers/types from `Message`
- [ ] Add or relocate handshake-specific TS types/helpers as needed
- [ ] Regenerate wire artifacts
- [ ] Get `roam-wire` compiling again

## Phase 2 — Make session/runtime architecture match Rust
- [ ] Make transport-based APIs do raw CBOR handshake first
- [ ] Build/use handshake result to initialize session state
- [ ] Remove old postcard-level hello handling from `session.ts`
- [ ] Remove stale `SchemaMessage` path
- [ ] Integrate request/response schema exchange properly
- [ ] Get `roam-core` compiling again

## Phase 3 — Fix payload framing and schema exchange correctness
- [ ] Audit opaque framing end-to-end
- [ ] Add request/response schema send/receive logic that mirrors Rust
- [ ] Add/reset per-connection schema trackers correctly across reconnect/resume
- [ ] Verify against Rust vectors/fixtures

## Phase 4 — Stable conduit parity
- [ ] Audit TS stable conduit against Rust stable conduit
- [ ] Fix ordering with transport/session handshake
- [ ] Verify replay/ack/reconnect semantics
- [ ] Verify channel continuity guarantees

## Phase 5 — Retry persistence parity
- [ ] Introduce durable operation store abstraction
- [ ] Align `persist` semantics with spec durability obligations
- [ ] Audit retry/indeterminate/released/sealed behavior against Rust/spec

## Phase 6 — Clean up stale API/tests/docs
- [ ] Remove or rewrite `connection.ts`-based old architecture
- [ ] Update tests to current semantics
- [ ] Update TS docs/examples once runtime is correct

---

## Immediate next steps

These should happen first:

- [ ] Fix `typescript/packages/roam-wire/src/types.ts`
- [ ] Remove stale hello/schema-message assumptions from `typescript/packages/roam-core/src/session.ts`
- [ ] Move schema receive logic to inlined request/response `schemas`
- [ ] Verify opaque framing in TS postcard implementation
- [ ] Revisit `StableConduit` integration order after session handshake is corrected

---

## Notes from current investigation

### Confirmed facts

- CBOR handshake exists specifically to bootstrap protocol/message schema negotiation before postcard traffic.
- Schemas are used at two levels:
  - handshake-level protocol schema
  - request/response payload schema
- Rust sends payload schemas from `SessionCore::send(...)`.
- Rust receives payload schemas in `Session::handle_message(...)` before routing.
- Rust resets schema tracking on reconnect/resume because type IDs are per connection.
- Rust/session resumption and stable conduit reconnect are separate layers.
- TS currently has only in-memory retry operation tracking, not a durable operation store.
- TS currently closes channels on session resume instead of preserving seamless continuity there.

### Important caution

Do not "fix" TS by preserving old postcard hello semantics.

That would move it farther away from parity.

---

## Tracking checklist

### Wire model
- [ ] Postcard `Message` type parity
- [ ] Remove stale postcard hello types
- [x] Handshake-specific TS types/helpers
- [x] Request/response helper field parity (schemas field added to Call/Response)
- [ ] Generated artifact parity

### Handshake
- [x] Raw CBOR handshake send path
- [x] Raw CBOR handshake receive path
- [x] Handshake result model
- [ ] Message plan bootstrap from handshake schema
- [ ] Resume-key handling

### Schema exchange
- [x] Send in `RequestCall.schemas` (codegen-driven CBOR from Rust Facet shapes)
- [x] Send in `RequestResponse.schemas` (codegen-driven CBOR from Rust Facet shapes)
- [x] Receive from incoming request/response
- [ ] Duplicate type handling
- [ ] Per-connection reset on reconnect

### Payload framing
- [ ] Opaque `u32le` decode
- [ ] Opaque `u32le` encode
- [ ] Passthrough framing parity
- [ ] Args/ret/item verification

### Stable conduit
- [ ] Stable handshake ordering
- [ ] Seq/ack semantics
- [ ] Replay buffer semantics
- [ ] Duplicate suppression
- [ ] Channel continuity parity

### Retry / operation continuity
- [ ] In-memory registry audit
- [ ] Durable operation-store abstraction
- [ ] Persist durability obligations
- [ ] Resume behavior parity
- [x] Indeterminate/sealed/released parity (persist=true + failClosedOnDrop → INDETERMINATE)

### Tests
- [ ] Remove stale hello-exchange tests
- [ ] Add handshake tests
- [ ] Add schema exchange tests
- [ ] Add opaque framing tests
- [ ] Add stable reconnect continuity tests
- [ ] Add parity tests against Rust

---

## Final reminder

If there is ever a choice between:

- making an old TS helper/test happy
- matching Rust/spec

the right answer is:

- **match Rust/spec**