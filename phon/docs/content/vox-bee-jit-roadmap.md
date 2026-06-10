+++
title = "Vox Bee JIT roadmap"
description = "Roadmap for prioritizing Phon JIT work from Bee's Vox surface"
+++

# Vox Bee JIT roadmap

This roadmap captures the source review of `~/bee` and turns it into a
work queue for the Vox/Phon integration. It is intentionally driven by the
surface Bee actually uses, not by broad spec completeness.

## Constraints

- Do not build Bee as part of this work unless Amos explicitly changes that
  instruction. Use source review and Phon-side fixtures/harnesses.
- Bee currently pins Vox 0.8.2 in `swift/bee-project.yml`. Treat Bee's current
  generated sources as the migration target surface, not as proof that the
  current local Vox/Phon stack is wired in.
- There are only two product modes to preserve: JIT enabled and JIT not enabled.
  The interpreter is the oracle and fallback implementation, but the shipping
  performance target is JIT coverage for the hot Bee shapes.
- Do not revive retry semantics while doing this work. Retry-shaped generated
  code in Bee belongs to the old pinned runtime surface.
- Nested channels should be rejected, not supported.

## Bee Vox surface

### Engine boundary

The production engine path is Swift app to Rust dylib over Vox FFI:

- `swift/bee/Services/BeeEngine.swift` loads `libbee_ffi.dylib`, opens the FFI
  endpoint, starts a Vox session, and exposes `BeeClient`.
- `swift/bee/Services/TranscriptionService.swift` uses `BeeClient` for model
  loading, session creation, streaming feed, finalization, language switching,
  stats, and debug transcription.
- `rust/bee-ffi/src/lib.rs` accepts the Vox FFI endpoint and serves
  `bee_services::Bee`.
- `rust/bee-services/src/lib.rs` is the service source for the generated Swift
  `GeneratedBeeService.swift`.

The hot method is:

```rust
feed(session_id: String, samples: Vec<f32>) -> Result<Option<FeedResult>, BeeError>
```

The engine surface uses these Phon/Vox shapes:

- Scalars: `bool`, `u32`, `i32`, `f32`, `f64`
- Strings: session IDs, model/cache paths, transcript text, language, errors
- Bulk vectors: `Vec<f32>` for audio samples
- Structs: `SessionConfig`, `FeedResult`, `EngineStats`, `RepoDownload`,
  `RepoFile`, `AlignedWord`, `Confidence`, `CorrectionEdit`, `EditResolution`
- Sequences: `Vec<RepoDownload>`, `Vec<RepoFile>`, `Vec<AlignedWord>`,
  `Vec<CorrectionEdit>`, `Vec<EditResolution>`
- Sum types: `Result<T, BeeError>`, `Option<FeedResult>`, `BeeError`

### Swift app to IME IPC

The app/IME path is Swift to Swift over Unix socket:

- `Ime` service: `setMarkedText`, `setPhase`, `commitText`,
  `advanceTranscript`, `stopDictating`
- `App` service: `imeHello`, `imeAttach`, `imeActivationRevoked`,
  `imeContextLost`, `imeKeyEvent`

This surface is narrow but hot for user-visible latency:

- `String`
- `UInt32`
- `Bool`
- `ImePhase` unit enum

### Trace viewer

`rust/hx-trace-viewer-service` exposes:

```rust
subscribe_trace(updates_out: Tx<StreamItem>) -> Result<(), String>
```

This is real Vox surface because it is channel-shaped and carries a large nested
event type, but it is not Bee dictation's hot path. Its current server is a
stub, so channel support should not outrank the engine and IME surfaces.

## Current Phon JIT gap

Rust typed lowering can represent the important Bee shapes, including strings,
bulk byte runs, sequences, options, enums/results, maps, dynamic values, and
opaque fields. The current Rust front door routes same-schema typed programs
through the native backend for those shapes on macOS/aarch64 when the `jit`
feature is enabled, and the Phon-side Bee fixture currently reports no Rust
native fallbacks for the mirrored Bee engine and IME method roots.

Rust compat decode ops such as `SkipWire` and `Default` remain decode-only
surface and should stay behind the same fallback audit; they are not on the
same-schema Bee hot path.

Swift typed execution has broad interpreter coverage for the Bee shapes. The
Swift native JIT now covers the Bee-relevant same-schema path for scalars,
options, bytes (`String`, `Data`, and bulk numeric arrays), enum/result payloads,
structured sequences, maps/sets, dynamic values, recursive blocks, and focused
compat decode ops. Its native fallback report identifies unsupported ops by
method phase and path; the remaining native proof surface is broad versioned
compat-corpus execution rather than the Bee dictation hot path.

The useful milestone is therefore: keep the Rust and Swift Bee fixtures
JIT-clean, then route generated Vox method arguments and responses through a
runtime-selected Phon typed engine instead of direct interpreter calls.

## Current implementation status

The Phon-side Bee audit is in place:

- Rust has a Bee surface fixture covering the engine and IME method roots. With
  the `jit` feature enabled on macOS/aarch64, the mirrored Bee roots report no
  native fallbacks.
- Swift native fallback reporting is method-scoped, and the Swift native JIT now
  compiles the IME roots and the core Bee `feed` hot shapes without native
  fallback.
- Swift and Rust Phon verification passes locally for the expanded fixture and
  native JIT tests.

The Phon-side Bee benchmarks are in place:

- Rust: `cargo run -p phon --release --features jit --example bee_surface_bench`
- Swift: `swift build -c release --product PhonJITBench`, then
  `.build/release/PhonJITBench`

Both benchmark entry points measure steady-state typed encode/decode after
lowering and native compilation, matching the cached generated-program path Vox
uses at runtime. They are performance probes, not coverage tests: the fallback
fixtures remain the source of truth for whether a method root is native-clean.

The remaining bridge is in Vox:

- Rust Vox already has a `vox-phon` typed program cache that compiles native
  encode/decode programs when the Phon JIT is available.
- Swift Vox generated code emits Phon descriptors and lowered typed programs,
  then routes args, responses, envelope encode/decode, and channel element
  codecs through VoxRuntime's selected typed engine.
- VoxRuntime owns the codec engine selection. Generated clients and dispatchers
  ask VoxRuntime to encode/decode typed values instead of importing `PhonJIT`
  themselves. The JIT-enabled Swift product is the small `VoxRuntimeJIT` opt-in
  wrapper, which configures VoxRuntime with `JITEngine`.

## Work plan

### 1. Add a Bee surface fixture in Phon

Create Phon-side schemas/descriptors for the Bee service shapes without building
Bee. The fixture should cover method argument and response roots for:

- `loadEngine`
- `createSession`
- `feed`
- `finishSession`
- `setLanguage`
- `transcribeSamples`
- `getStats`
- `setMarkedText`
- `setPhase`
- `commitText`
- `advanceTranscript`
- `imeKeyEvent`

The fixture should expose a small audit command/test helper that lowers every
root for Rust and Swift, then reports the exact `MemOp` shapes that are not
native-JIT covered.

### 2. Make fallback reporting shape-aware

Fallback reporting should be useful enough to drive work:

- report method name and direction: args encode, args decode, response encode,
  response decode
- report path inside the method shape, not just root
- report unsupported op kind and why
- distinguish hot path failures from deferrable surface

This lets us stop arguing from memory and ask the tool what the next blocking op
is.

### 3. Rust JIT priorities

1. Preserve the Bee fixture's empty native fallback report as Vox integration
   code starts using generated Phon typed programs.
2. Add method-root fallback reporting to the generated/runtime Vox path so Bee
   can fail loudly if a new hot method shape leaves the Rust native backend.
3. Compat decode ops for the Bee fixture.
   Implement or explicitly report `SkipWire` and `Default` only once the
   same-schema Bee hot path remains JIT-clean through generated Vox calls.
4. Lower priority for Bee dictation: `Map`, `Dynamic`, `Opaque`, `Borrow`, and
   recursive `CallBlock`, unless the fixture proves they are on the critical
   Bee path under generated Vox method roots.

### 4. Swift JIT priorities

1. `Bytes` for `String` and bulk arrays.
   This unlocks the IME text path and the `Vec<f32>` engine request shape.
2. Enum/result support.
   Start with unit enums (`ImePhase`), then payload enums (`BeeError`) and the
   generated `Result` shape.
3. Structured sequences.
   This unlocks returned feed metadata and correction vectors.
4. Keep option support wired through the same native path and verify it against
   `Option<FeedResult>`.
5. Lower priority for Bee dictation: map and dynamic value native stencils.

### 5. Vox integration priorities

1. Make generated Vox method args/responses route through Phon typed
   encode/decode, with the selected Phon engine supplied by the runtime.
2. Preserve the two runtime modes: JIT enabled and JIT not enabled.
3. Make schema registries and generated value descriptors produce Phon
   descriptors once, then reuse compiled programs per method root.
4. Keep old retry semantics out of the new surface.
5. Ensure subjects/sessions die on disconnect or inactivity; no accumulating
   orphan subject processes.
6. Reject nested channels.
7. Implement non-nested channel binding for the trace viewer after the Bee
   engine and IME method roots are covered.

## Acceptance milestones

### Milestone A: Bee hot shape audit

The Phon fixture can compile/audit the Bee service roots without reading or
building Bee during the run. The report identifies unsupported native-JIT ops
per method root.

Status: done on the Phon side.

### Milestone B: Rust feed path JIT-clean

Rust JIT has no fallback for:

- `feed` argument encode/decode: `String`, `Vec<f32>`
- `feed` response encode/decode:
  `Result<Option<FeedResult>, BeeError>`

Status: done in the Phon Rust fixture.

### Milestone C: Swift IME path JIT-clean

Swift JIT has no fallback for:

- `setMarkedText`
- `setPhase`
- `commitText`
- `advanceTranscript`
- `imeKeyEvent`

Status: done in the Phon Swift JIT tests.

### Milestone D: Bee engine core JIT-clean

Rust and Swift JITs have no fallback for:

- `loadEngine`
- `createSession`
- `feed`
- `finishSession`
- `setLanguage`
- `transcribeSamples`
- `getStats`

Correction methods can follow after the core engine surface unless the app
starts depending on them in the hot path.

Status: done for the mirrored core/feed hot roots; trace-viewer channel surface
is still explicitly separate.

### Milestone E: Bee hot shape benchmarks

Rust and Swift have runnable benchmarks that compare interpreter and JIT
steady-state encode/decode on:

- `feed` args: `String`, bulk audio samples
- `feed` response: `Result<Option<FeedResult>, BeeError>`
- IME/app latency roots: marked text, transcript advancement, key events

Status: done on the Phon side.

Status: partially done on the Phon Swift side. The core `feed` hot shapes are
native-clean; the remaining engine method roots should be moved from manual
fixture/audit coverage into generated Vox calls.

### Milestone E: Vox bridge uses Phon

Generated Vox service calls use Phon typed programs for args/responses and pass
Phon-side oracle tests with JIT enabled and JIT disabled.

Status: done for the Rust and Swift generated/runtime bridge. Swift codegen now
emits runtime `VoxTypedEncoder` globals, `SchemaTracker` compiles cached decode
functions through the selected engine, and regenerated Vox subject bindings pass
the Swift subject corpus with JIT enabled through `PhonEngineTestSupport`.

### Milestone F: Trace viewer channel surface

Non-nested `Tx<StreamItem>` is represented and bound through the Vox/Phon
bridge. The current trace viewer can be used as the motivating surface, but it
does not gate Bee dictation.

## Goal wording

Finish the Vox/Phon Bee-surface JIT integration roadmap: keep the Phon Bee
surface fixtures JIT-clean, make fallback reporting method-aware, implement the
Rust and Swift JIT ops needed for Bee's engine and IME paths, route generated
Vox args/responses through runtime-selected Phon typed programs, keep Bee itself
unbuilt unless explicitly requested, and leave trace-viewer channel support as
the follow-up after the Bee hot paths are JIT-clean.

Use this as the goal objective if we want the agent to keep going:

> Finish the Vox/Phon Bee-surface JIT integration roadmap in
> `docs/content/vox-bee-jit-roadmap.md`: add the Bee surface fixture, make
> fallback reporting method-aware, implement the Rust and Swift JIT ops needed
> for Bee's engine and IME paths, route generated Vox args/responses through
> Phon typed programs, and leave trace-viewer channel support as the follow-up
> after the Bee hot paths are JIT-clean.
