+++
title = "Vox ecosystem compatibility roadmap"
description = "Roadmap for making the Vox/Phon integration cover the real Vox consumer surface"
+++

# Vox ecosystem compatibility roadmap

This roadmap turns the source sweep of current Vox consumers into the
compatibility target for the next Vox/Phon integration. The Bee roadmap remains
the first hot-path milestone. This document widens the target to the projects
that already depend on Vox or are expected to migrate onto the same surface.

The goal is not broad theoretical completeness. The goal is that the next Vox
can carry the payloads real Vox users already send, with the interpreter as the
correctness oracle, Rust and Swift JIT covering the hot paths that would
otherwise make the migration too slow, and TypeScript generated clients staying
idiomatic and correct before their source-specialized fast path is treated as a
performance gate.

## Architectural constraints

- There are only two product modes: JIT enabled and JIT not enabled. Strict
  fallback reporting is a diagnostic, not a third product mode.
- Encoding and decoding always go through a translation plan, including
  same-schema cases. Do not add direct codec shortcuts for identity shapes.
- The interpreter is the oracle. JIT implementations must produce identical
  bytes and values, differing only in speed.
- Generated Vox method arguments and responses must route through the
  runtime-selected Phon typed engine rather than bypassing Phon.
- TypeScript generated APIs expose ordinary JavaScript/TypeScript public
  shapes. The generic Phon `Value` representation is only for real dynamic
  fields, schema-less APIs, and oracle/fallback execution; it is not the public
  shape for ordinary generated DTOs.
- Retry semantics belong to the removed old Vox surface. Do not reintroduce
  retry-shaped generated code while doing this work.
- Nested channels are rejected. Non-nested `Tx<T>` and `Rx<T>` are supported as
  service parameters, with each stream item encoded as its own message.
- Subjects must die on disconnect or inactivity. No work in this roadmap may
  leave orphan subject processes accumulating.
- Consumer repositories are source inputs for fixtures and audits. Build those
  consumers only when Amos explicitly asks; prefer extracted Phon/Vox fixtures
  and oracle tests in this repository and the Vox repository.

## Priority tiers

The roadmap has three different kinds of gate. They should not be collapsed
into one vague "everything must JIT" requirement:

1. Correctness gates: the interpreter, compatibility planner, schema closure,
   generated Vox bridge, non-nested channel handling, external diagnostics, and
   subject teardown must be correct for the checked-in consumer fixture corpus.
   These are Vox 1.0 blockers.
2. Priority-1 performance gates: Rust native JIT and Swift native JIT must stay
   native-clean for the hot roots called out in this roadmap, especially Bee,
   the Swift-app surfaces, and the large/recursive ecosystem payload families
   that would otherwise make the migration too slow.
3. TypeScript performance tier: generated TypeScript clients must expose and
   consume ordinary JavaScript/TypeScript public DTO shapes. Direct public-shape
   source-specialized JIT is the useful TypeScript optimization path, but it is
   promoted only by browser/client benchmark evidence. A generic Phon `Value`
   pipeline is valid as the oracle, fallback implementation, and true dynamic
   API path; it is not a successful generated-client public shape for ordinary
   DTOs.

This means TypeScript JIT parity is not priority 1. The priority-1 JIT work is
Rust and Swift native execution for server, engine, and Swift-app hot paths.
TypeScript is release-critical for generated bridge correctness, public DTO
shape fidelity, websocket/browser coverage, and honest benchmarks. Its JIT work
should only chase direct public-shape source specialization for measured
client/browser bottlenecks; it should not recreate the Rust/Swift
descriptor-memory model in JavaScript.

Tracey coverage is a triage tool for this surface, not an independent demand
for 100% historical spec coverage. Rules tied to removed retry/idempotency,
stable-conduit, or zero-copy/shared-memory designs should be deleted or
rewritten before they become implementation work. In particular,
`rpc.response.one-per-request` is not a Vox 1.0 blocker as currently shaped; it
belongs with the old retry/idempotency response-finalization model unless it is
rephrased into a live call-outcome invariant.

Transport coverage should follow the transports each implementation actually
ships. Rust transport verification is a fair release audit for Rust memory,
stream, WebSocket, and in-process surfaces that remain part of Vox. Swift
transport verification is about the Swift stream/local transport and its
real cancellation/lifecycle behavior; browser-only or non-Swift transports such
as Swift WebSocket, memory, and in-process links are not 1.0 blockers.

Consumer repositories remain source inputs. Bee stays the protected hot-path
fixture and benchmark source, but normal Vox/Phon verification should keep
using checked-in fixtures while Bee is actively moving. Tracey is the bounded
generated-service smoke target for breadth: it exercises a realistic
dashboard/LSP-like service surface without making the live Tracey checkout
itself a prerequisite app build.

Roadmap accounting rule: a TypeScript proof that succeeds only by routing an
ordinary generated DTO through generic Phon `Value` counts as interpreter,
oracle, fallback, or dynamic-API coverage. It does not count as generated
TypeScript bridge completion, and it does not count as TypeScript JIT parity.

## Current implementation snapshot

This is the state this roadmap starts from. It should be updated as milestones
move, because the goal is to make future work start from repo truth rather than
from chat memory.

Already in place on the Phon side:

- Bee Rust fixtures for the mirrored engine and IME hot roots.
- Bee Rust benchmark entry point for cached typed encode/decode. The current
  `cargo run -p phon --features jit --example bee_surface_bench` run completes
  and reports JIT speedups for the checked-in Bee roots, including 1.41x/1.37x
  encode/decode for `feed(args)`, 13.09x/4.99x for `feed(response)`,
  4.95x/3.87x for `setMarkedText(args)`, 4.98x/3.88x for
  `advanceTranscript(args)`, and 4.95x/3.17x for `imeKeyEvent(args)`.
- Swift JIT smoke/fixture coverage for the Bee-relevant IME and feed shapes.
- Method/path-scoped fallback reporting in the typed front door.
- Initial Rust ecosystem fixture coverage for Dodeca-shaped maps, sets, tuple
  vectors, dynamic template values, data-loader dynamic results, and markdown
  parse/render results with a boxed source map, Dodeca image processor
  byte/scalar/result roots, Dodeca search indexer page/file/result roots, Dibs
  SQL value enums, generated Dibs Squel
  schema/list/get/create/update/delete/result DTOs, Dibs migration
  status/migrate/log DTOs, Styx recursive values plus
  LSP extension/host callback DTOs, Stax recursive flamegraph updates, Stax
  Linux fd-broker config/status/error DTOs with explicit external-fd
  diagnostics, Helix trace-server metadata, metric, attention, evidence,
  attendance, clip, provenance, transcript, piece-eval, and bundle payloads,
  Hotmeal live-reload payloads, and Tracey migration DTOs.
- Rust `HashSet<T>` descriptor/interpreter support through Facet set thunks.
- Rust `HashSet<T>` root lowering and duplicate-element rejection coverage.
- Rust native JIT support for set encode/decode when the element program is
  native-supported.
- Rust owned-pointer descriptor, interpreter, and native JIT support for
  `Box<T>`, `Rc<T>`, and `Arc<T>` when the pointee program is
  native-supported. The wire schema is the pointee `T`; the pointer handle is a
  local descriptor detail driven by borrow/construct thunks. The focused
  `cargo nextest run -p phon --all-features -E 'test(derived_box)'` run passes
  3/3, including native JIT encode/decode for both boxed fields and boxed
  roots. The public `phon-jit` threaded executor also carries owned pointers
  through the same thunk contract; the focused threaded-pointer run passes 2/2
  for boxed fields and boxed roots.
- Rust compact `read_from` support for chaining multiple schema-driven values
  through one message cursor, which is the shape Vox envelopes + payloads need.
- Rust schema-bundle validation through `Registry::try_new`: recomputed
  `SchemaId`s, complete closure checks, and zero-wire fixed-array caps.
- Swift schema-bundle validation through `Registry(validating:)` and
  `Registry.withValidating`: recomputed `SchemaId`s, complete closure checks,
  and zero-wire fixed-array caps.
- Swift native JIT support for same-schema string-keyed map encode/decode when
  key and value programs are native-supported, including duplicate-key
  rejection coverage.
- Swift native JIT support for same-schema scalar set encode/decode through the
  sequence stencil, including duplicate-element rejection coverage.
- Swift native JIT support for dynamic `Value` fields through host-call
  dynamic stencils that delegate to the canonical Phon self-describing value
  codec while staying native-clean in fallback reports.
- Swift native JIT support for recursive call blocks. The implementation
  compiles lowered recursive block programs, binds native block call sites, and
  gives recursive calls their own scratch frames so nested enum/option payloads
  do not clobber outer scratch state.
- Swift native JIT support for focused compat decode ops now covers enum
  remapping, writer-only enum errors, writer-only field skips, and reader-only
  field defaults. `swift test --filter PhonJITTests` from the package root
  covers the evolved-writer enum case, native-clean scalar field skip/default
  cases, and a nested list element drift fixture with writer-only dynamic
  metadata plus map value drift; the remaining proof work is broadening native
  execution across the full versioned compat corpus.
- Swift engine-level ecosystem fixture coverage now includes a Dodeca-shaped
  `Set<String>` root, Dodeca dynamic template calls with `Value` args and
  tuple-vector kwargs, Dodeca HTML processor inputs with optional maps, string
  sets, nested code metadata maps, responsive-image tuple vectors, Vite CSS
  maps, injections, and mount localization, Dodeca data-loader dynamic results,
  and Dodeca markdown parse/render results with dynamic frontmatter extras and
  source-map entries, Dodeca image processor byte/scalar/result roots, Dodeca
  search indexer page/file/result roots, a Dibs SQL row/list response with
  enum payloads and bytes, generated Dibs Squel
  schema/list/get/create/update/delete/result DTO roots, Dibs migration
  status/migrate/log DTO roots, a Stax-shaped
  recursive flamegraph update with `UInt64`, `UInt32?`, managed `[String]`, and
  recursive `[FlameNode]` payloads, Stax Linux broker-control
  config/status/error DTOs, Stax macOS `KdBufBatch` stream items, and Styx recursive value plus aggregate LSP
  extension/host-callback DTO roots. These fixtures run through the shared
  interpreter/JIT equivalence harness and assert the Dodeca, Dibs/generated,
  Dibs migration, recursive, Stax broker-control, and Stax macOS batch roots
  are native-clean in the Swift JIT.
- Rust ecosystem benchmark entry point for Dodeca, Dibs, Styx, Stax, Helix,
  Hotmeal, and Tracey migration payload families. The Styx benchmark family now
  includes both the recursive tree value and the aggregate LSP
  extension/host-callback DTO surface. The Stax benchmark family now includes
  recursive flamegraph updates, Linux fd-broker control DTOs, and macOS
  `KdBufBatch` record-stream fixtures. The Dibs benchmark family includes SQL
  row/list payloads, generated Squel schema/list/get/create/update/delete/result
  DTO roots, and migration status/migrate/log DTO roots. The current
  `cargo run -p phon --features jit --example ecosystem_surface_bench` run
  completes the full family and reports `native-clean` fallback status for every
  row. Representative selected-runtime speedups from that run include
  `dodeca(parse result boxed source map)` at 4,512 wire bytes with 9.72x encode
  and 3.62x decode, `dodeca(search indexer roots)` at 18,128 wire bytes with
  6.90x encode and 2.80x decode, `dibs(squel service roots)` at 1,128 wire
  bytes with 5.40x encode and 3.19x decode, `styx(lsp surface)` at 8,044 wire
  bytes with 4.98x encode and 3.13x decode, `stax(mac kdbuf batches)` at 8,387
  wire bytes with 5.11x encode and 3.67x decode,
  `helix(trace service aggregate)` at 6,400 wire bytes with 4.26x encode and
  3.31x decode, `hotmeal(live reload)` at 2,101 wire bytes with 4.86x encode
  and 2.75x decode, and `tracey(migration dto)` at 2,064 wire bytes with 5.35x
  encode and 2.78x decode. The Rust channel-item benchmark family now covers
  Dodeca byte and string items, Dibs migration logs, Helix pulse notifications,
  and Tracey data updates; the same current run reports those roots as
  native-clean at 4,100, 3,055, 94, 8, and 976 wire bytes respectively.
- Swift `PhonJITBench` now covers the Bee hot roots plus Dodeca `Set<String>`
  routes, Dodeca dynamic template calls, Dodeca HTML processor maps/sets/tuple
  vectors, Dodeca data-loader dynamic results, Dodeca markdown parse/render
  results with source maps, Dodeca image processor byte/scalar/result roots,
  and Dodeca search indexer page/file/result roots, a Dibs SQL row/list
  response fixture with enum payloads, nested row lists, strings, bytes, and an
  optional total, generated Dibs Squel service roots, Dibs migration
  status/migrate/log DTO roots, Styx recursive value and aggregate LSP
  extension/host-callback DTO roots, and Stax recursive flamegraph plus Linux
  broker-control DTO fixtures, and the broad Helix `TraceService` aggregate.
  It also benchmarks focused compat decode roots for writer-only scalar field
  skip and reader-only optional default, plus channel item roots for Dodeca byte
  tunnels, Dodeca LSP strings, Dibs migration logs, Helix pulse notifications,
  and Tracey data updates. The current `swift build --product PhonJITBench`
  plus direct `.build/debug/PhonJITBench` run from the package root completes
  and reports native-clean status for every row. Representative current results
  include native-clean Bee `feed(response)` at 1,789 wire bytes with 3.37x
  encode and 12.26x decode, compat field-skip/default decode at 1.10x and
  1.71x, Dodeca parse at 4,512 wire bytes with 1.72x encode and 10.63x decode,
  Dodeca search at 12,017 wire bytes with 0.97x encode and 7.57x decode, Dibs
  generated Squel roots at 1,128 wire bytes with 2.14x encode and 10.43x
  decode, Styx LSP aggregates at 6,688 wire bytes with 1.93x encode and 9.03x
  decode, Stax recursive flamegraphs at 924 wire bytes with 3.24x encode and
  10.90x decode, Helix trace-service aggregate roots at 6,400 wire bytes with
  5.32x encode and 8.47x decode, and native-clean channel item roots at 4,100,
  3,055, 104, 8, and 976 wire bytes.
- TypeScript engine-level ecosystem fixture coverage for Dodeca HTML
  maps/sets/tuple vectors, Dodeca dynamic template calls, Dodeca data-loader
  dynamic results, Dodeca markdown parse/render results, Dodeca image
  processor byte/scalar/result roots, Dodeca search indexer page/file/result
  roots, Dibs SQL value rows,
  generated Dibs Squel schema/list/get/create/update/delete/result DTO roots,
  Dibs migration status/migrate/log DTO roots,
  Styx recursive values plus LSP extension/host callback DTOs, Stax recursive
  flamegraph updates, Stax Linux fd-broker control DTOs, Stax macOS
  `KdBufBatch` record-stream fixtures, with explicit
  external-fd diagnostics, Helix trace snapshots, Hotmeal live reload, Tracey
  migration DTOs, and fixed-width target schemas for native-sized Rust integers.
  These fixtures run through typed encode/decode, interpreter mode,
  requested-JIT mode, recursive call-block source generation, and
  encoder JIT/fallback selection.
- TypeScript JIT now generates JavaScript functions for recursive `callBlock`
  decode plans and recursive encoder blocks instead of routing recursive roots
  through the interpreter fallback. The focused TypeScript benchmark includes a
  recursive rose-list call-block case; the current run measured 400,903 hz for
  JIT decode vs. 124,208 hz for the interpreter, a 3.23x decode speedup, and
  56,652 hz for JIT encode vs. 38,196 hz for the interpreter, a 1.48x encode
  speedup.
- TypeScript now has a direct public-shape typed JIT path for generated-client
  DTOs. `decodeTyped` with JIT enabled lowers the compatibility plan into
  generated JavaScript that constructs plain struct objects, codegen enum
  discriminated unions, arrays, sets, and schema maps directly; `encodeTyped`
  with JIT enabled consumes those same public shapes directly. The generic
  coarse `Value` engine remains the interpreter/oracle and the implementation
  for actual `Dynamic` fields and schema-less/dynamic APIs. The current
  TypeScript engine suite
  (`pnpm --filter @bearcove/phon-engine exec vitest run`) passes 103/103,
  Tracey validation is clean, and the
  current Helix `TraceService` aggregate benchmark measures direct-shape typed
  JIT at 47,028.70 hz for decode and 10,426.74 hz for encode, versus 7,997.22
  hz and 4,845.53 hz for the JIT-disabled typed fallback through `Value`. The
  focused Dodeca TypeScript benchmark now also measures the image processor and
  search indexer roots on larger benchmark payloads: image decode is
  300,588.56 hz direct-shape JIT vs. 133,576.12 hz fallback, image encode is
  103,405.99 hz vs. 70,289.24 hz, search decode is 106,268.31 hz vs.
  49,417.90 hz, and search encode is 37,859.36 hz vs. 24,907.05 hz.
- Current local fixture verification passes: Rust Bee surface with JIT
  (`cargo nextest run -p phon --features jit -E 'binary(bee_surface)'`, 2/2),
  Rust ecosystem surface with JIT
  (`cargo nextest run -p phon --all-features -E 'binary(ecosystem_surface)'`,
  22/22), Swift Bee feed JIT smoke
  (`swift test --filter swiftBeeFeedMethodRootsAreNativeClean`, 1/1), Swift
  ecosystem surface
  (`swift test --filter FixtureRoundTripsAcrossEngines`, 19/19),
  and TypeScript ecosystem surface
  (`pnpm --filter @bearcove/phon-engine exec vitest run src/ecosystem_surface.test.ts`,
  21/21). `pnpm check` from `~/phon` also passes, and Tracey
  validation reports zero errors across Rust, Swift, and TypeScript.
- The cross-language compat conformance corpus now includes 28 generated
  vectors, including explicit `channel_item_schema_compat` and
  `external_metadata_schema_compat` cases. The same committed
  `conformance/compat/vectors.json` replays through Rust, Swift, and TypeScript:
  `cargo nextest run -p phon-conformance -E 'test(corpus_is_golden_and_self_consistent)'`,
  `swift test --filter compatConformanceCorpus`, and
  `pnpm --filter @bearcove/phon-engine exec vitest run src/conformance.test.ts`
  all pass.
- Current Vox bridge verification passes: Rust runtime/codegen bridge
  (`cargo nextest run -p vox -p vox-core -p vox-phon -p vox-codegen --no-fail-fast`,
  192/192), Swift runtime
  (`swift test --package-path swift/vox-runtime`, 52/52), targeted TypeScript
  schema/channel/session tests
  (`pnpm --filter @bearcove/vox-core exec vitest run src/schema_tracker.test.ts src/driver.channel_schema.test.ts src/channeling/binding.test.ts src/channeling/registry.test.ts src/session.test.ts`,
  37/37), TypeScript browser WebSocket gate
  (`pnpm --dir typescript/tests/playwright test`, 2/2, covering generated
  browser clients against both TypeScript and Rust WebSocket servers), and the
  TypeScript workspace check (`pnpm check` from `~/vox`). A post-run
  process sweep found no lingering `subject-*`, echo-server, or browser Vite
  processes.
- Current Vox ecosystem bridge matrix verification passes:
  `cargo nextest run -p spec-tests -E 'test(ecosystem_bridge) | test(dodeca) | test(dibs) | test(styx) | test(stax) | test(helix) | test(hotmeal) | test(tracey)' --no-fail-fast -j 1`
  ran 424/424 across Rust TCP, Swift TCP, TypeScript TCP, and TypeScript
  WebSocket, in both harness-to-subject and subject-to-harness directions,
  including the generated Helix `TraceService` aggregate root plus Dodeca
  image processor and search indexer roots. This was reverified against the
  live `~/vox` checkout after the TypeScript direct-shape typed JIT cleanup and
  after increasing the Rust spec harness and Rust subject runtime stack budget
  for large recursive schema closure planning; the current run started 424
  selected tests across 4 binaries and finished with `424 passed, 511 skipped`.
  A post-run process sweep found no lingering `subject-*`, echo-server,
  `nextest`, Swift build, or Swift frontend processes.
- Focused generated Dodeca image/search bridge verification also passes:
  `cargo nextest run -p spec-tests -E 'test(echo_dodeca_image_processor_fixture) | test(echo_dodeca_search_indexer_fixture)' --no-fail-fast -j 1`
  ran 16/16 in `~/vox` across Rust TCP, Swift TCP, TypeScript TCP, and
  TypeScript WebSocket, in both harness-to-subject and subject-to-harness
  directions. The run uses generated clients/dispatchers and proves the
  Dodeca image processor byte/scalar/result root plus the search indexer
  page/file/result root through the Vox bridge. A post-run process sweep found
  no lingering `subject-*`, echo-server, Swift build, or Swift frontend
  processes.
- Focused generated Stax macOS record bridge verification also passes:
  `cargo nextest run -p spec-tests -E 'test(stax_macos_record)' --no-fail-fast -j 1 --status-level fail --final-status-level fail`
  ran 8/8 in `~/vox` across Rust TCP, Swift TCP, TypeScript TCP, and
  TypeScript WebSocket, in both harness-to-subject and subject-to-harness
  directions. This exercises the exact
  `stax_macos_record(config, Tx<StaxMacKdBufBatch>) -> Result<StaxMacRecordSummary, StaxMacRecordError>`
  root through generated Rust, Swift, and TypeScript clients/dispatchers,
  schema exchange, non-nested channel binding, typed channel item
  encode/decode, and terminal user-error result schema handling. The cold
  Swift subject release build completed with
  `swift build --package-path swift/subject -c release`; Vox's nextest config
  now gives Swift transport and Swift subject lifecycle tests a longer
  Swift-specific timeout so stale or absent release subject builds do not fail
  the matrix before the subject can compile. A post-run process sweep found no
  lingering `subject-*`, Swift build, Swift frontend, or nextest processes.
- Focused generated Swift bridge verification also passes:
  `cargo nextest run -p spec-tests -E 'test(lang_swift_transport_tcp::direction_harness_to_subject::rpc_echo_ecosystem_bridge) | test(lang_swift_transport_tcp::direction_subject_to_harness::subject_calls_echo_ecosystem_bridge)' --no-fail-fast -j 1`
  ran 2/2 in `~/vox`. This exercises both generated Swift dispatcher decode
  and generated Swift client encode for the ecosystem bridge payload through
  `readerDescriptor`/`readerBlocks`, `decodeVoxTyped`, and `encodeVoxTyped`.
- Current hosted-subject lifecycle verification passes:
  `cargo nextest run -p spec-tests -E 'binary(subject_lifecycle)' --no-fail-fast -j 1`
  ran 4/4 across Rust TCP, Swift TCP, TypeScript TCP, and TypeScript
  WebSocket. A post-run process sweep found no lingering `subject-*` or
  echo-server processes.
- Tracey Rust coverage is now complete for the current Phon spec: 60/60
  implemented and 60/60 verified. The spec no longer treats framing,
  transport-owned external attachment semantics, absolute-buffer zero-copy
  alignment, or thunk-only descriptor support as phon-core rules.
- Vox-side stale-surface cleanup has started: live Vox source/spec no longer
  uses retryability/non-retryability rule IDs, stable-conduit language, or SHM
  wording outside historical/generated artifacts. RPC errors are specified in
  terms of terminal outcomes, session interruptions, and indeterminate results;
  schema decode-plan failures are specified as terminal for the current remote
  peer schema.
- Vox metadata now matches the agreed contract in spec and implementation:
  metadata is a self-describing phon `Value` map with well-known key
  conventions, `#`/`-`/`-#` sigils are preserved in the key string, and Tracey
  links the Rust, Swift, and TypeScript implementations plus sigil tests.
- Phon external values are specified as transport-owned capabilities: compact
  messages carry the transport handle and optional in-band metadata descriptor
  value, while the resource/attachment remains owned by the transport. Current
  Rust and TypeScript Stax fd fixtures verify that external fd capabilities are
  reported as unsupported by ordinary payload encode/decode and compatibility
  planning instead of being treated as scalar bytes.
- Vox fd capability diagnostics are now explicit: Rust fd-capable local
  transports carry `vox::Fd`, non-fd transports reject descriptor-bearing
  frames, and generated Swift/TypeScript bindings reject `vox::Fd` service
  surfaces at codegen time instead of lowering them to `Data` or `unknown`.
- Tracey Swift coverage is now audited for the current Swift implementation:
  54/60 implemented and 57/60 verified, with zero implemented-but-untested
  rules. The remaining Swift holes are not annotation debt: Swift codegen is
  not in this package, `type-system.rust-subset` is Rust-only, borrowed
  descriptor decode is not implemented, named thunk binding is not Swift's
  closure-carrying descriptor model, and the typed IR is not total yet.
- Tracey TypeScript coverage has an audited schema/engine/codegen pass: 49/60
  implemented and 49/60 verified, with zero implemented-but-untested rules. The
  TypeScript schema and engine packages cover the schema model, schema parsing,
  schema-id recomputation/content hashes, closure validation, generic
  substitution, compact decode chaining, hostile-input guards, package
  boundaries, JIT opt-in selection, self-describing enum payloads, TypeScript
  type emission from schema, recursive call-block JIT source generation,
  direct public-shape typed JIT encode/decode, and the implemented compact
  interpreter paths.

Verified in the Vox checkout during the bridge audit:

- TypeScript packages, generated TypeScript, and the sibling Phon TypeScript
  packages pass `pnpm check` from `~/vox`.
- TypeScript `vox-core` passes its full Vitest suite with 73 tests; the focused
  session/schema/channel runtime slice passes 51 tests, `vox-wire` passes 20
  tests, `vox-inprocess` passes its focused transport suite with 1 test,
  `vox-tcp` passes its focused transport suite with 5 tests, and `vox-ws`
  passes its focused transport suite with 1 test.
- Vox Tracey validation is clean across Rust, Swift, and TypeScript. Current
  coverage is Rust 175/175 implemented and 162/175 verified, Swift 163/175
  implemented and 160/175 verified, and TypeScript 175/175 implemented and
  173/175 verified. That is not a global Vox Tracey completion claim: the
  remaining TypeScript unverified rules are the broad `rpc` and
  `rpc.one-service-per-connection` umbrella surfaces, while the remaining
  Swift holes are concentrated in uncovered non-Swift transports, debug
  observability rules, and five untested implemented umbrella or lifecycle
  rules; the non-Swift transport holes are not Vox 1.0 blockers. The remaining
  Rust unverified rules are now concentrated in shipped-transport audit items,
  observability emission, broad `rpc` / one-service umbrella surfaces, the
  legacy `rpc.response.one-per-request` rule that should be deleted or
  reworded rather than implemented as retry/idempotency residue, and two broad
  session message/peer rules.
- The roadmap-relevant Vox rules for subject teardown, channel shape, channel
  allocation/direction/lifecycle, channel payload indexes, connection-close
  channel errors, root and virtual connection behavior, virtual connection
  opening/acceptance, virtual connection close semantics, keepalive teardown,
  caller liveness teardown, and nested-channel rejection, plus the
  Rust-trait source-of-truth rule for
  generated service surfaces, are traced with implementation and verification
  references: `hosted.subject.lifecycle`, `service-macro.is-source-of-truth`,
  `rpc.channel`,
  `rpc.channel.allocation`, `rpc.channel.direction`, `rpc.channel.lifecycle`,
  `rpc.channel.payload-encoding`, `connection`, `connection.root`,
  `connection.virtual`, `connection.open`, `connection.open.rejection`,
  `connection.close`,
  `connection.close.semantics`,
  `rpc.virtual-connection.open`, `rpc.virtual-connection.accept`,
  `rpc.channel.connection-closure`, `rpc.caller.liveness.refcounted`,
  `rpc.caller.liveness.last-drop-closes-connection`,
  `rpc.caller.liveness.root-internal-close`,
  `rpc.caller.liveness.root-teardown-condition`, `session.keepalive`,
  `rpc.channel.direct-args`, and `rpc.channel.no-collections`.
- Vox `service-macro.is-source-of-truth` now has Tracey-backed proof across
  Rust, Swift, and TypeScript: the Rust `#[vox::service]` macro end-to-end
  test exercises generated client/dispatcher plumbing from a Rust trait, while
  Swift and TypeScript codegen tests prove generated method tables, schema
  closures, and method IDs are derived from the Rust `ServiceDescriptor`.
- Swift generated RPC caller, service, handler, and dispatcher surfaces now
  have Tracey-backed codegen coverage. The `vox-codegen` Swift target test
  `generated_swift_emits_rpc_caller_handler_and_dispatch_shapes` proves
  generated caller protocols expose async methods matching the source service,
  generated clients route through `VoxConnection.call` with method schema and
  registry data, generated handler protocols preserve method signatures, and
  generated dispatchers route by method ID, invoke the handler, map user
  fallible errors onto the wire `VoxError.user` arm, return call-level
  `UnknownMethod` errors for unrecognized method IDs, and reply through
  `TaskMessage`. Tracey now reports no remaining untested Swift `rpc.caller`,
  `rpc.handler`, `rpc.service.*`, `rpc.fallible.*`, or
  `rpc.unknown-method` rules.
- Swift generated channel discovery now has Tracey-backed codegen coverage.
  The `vox-codegen` Swift target test `generated_swift_emits_channel_schemas`
  proves generated callers discover direct `Tx<T>`/`Rx<T>` method arguments
  left-to-right and pass the resulting channel ID list on the call, while
  generated dispatchers resolve decoded channel wire indexes through
  `Request.channels` before creating server-side channel handles. Tracey now
  reports no remaining untested Swift `rpc.channel.discovery` rule.
- Swift runtime session setup now has Tracey-backed coverage in
  `ConnectionFailureTests`: `acceptorSessionExposesPeerHandshakeMetadata`
  establishes a fresh link with a user-provided root dispatcher, returns a
  root connection on connection ID 0, wires that connection to the driver
  handle, preserves the acceptor role on the session and driver, and exposes
  peer handshake metadata. Tracey now reports no remaining untested Swift
  `rpc.session-setup` rule.
- Swift core session/RPC envelope behavior now has Tracey-backed runtime
  coverage in `ConnectionFailureTests`: the handshake advertises the Phon
  Message schema closure, rejects an unsupported peer Message schema with
  `Sorry`, scopes the schema to the session, preserves connection IDs on
  messages, allocates request IDs, and observes one response per request.
  Tracey now reports no remaining untested Swift `session.handshake.*`,
  `session.message.*`, `rpc.request.*`, or `rpc.response.*` rules.
- Swift session settings, role/peer tracking, protocol-error teardown, and
  outbound request-limit behavior now have focused Tracey-backed runtime
  coverage in `ConnectionFailureTests`. The initiator handshake test proves the
  root `Hello` carries parity, `ConnectionSettings`, and the default
  `max_concurrent_requests`; the acceptor setup test proves a fresh session
  exposes role, root connection, dispatcher wiring, and peer metadata; the
  virtual-connection test observes the emitted `OpenConnection` settings and
  parity-based request IDs; the new
  `outboundMaxConcurrentRequestsWaitsForPeerLimit` test proves a peer limit of
  one blocks the second request until the first response releases capacity;
  `inboundMaxConcurrentRequestsViolationClosesConnection` proves a peer request
  above Swift's locally advertised inbound request limit sends
  `ProtocolError("rpc.flow-control.max-concurrent-requests.inbound")` and
  closes the driver;
  `queuedOutboundRequestFailsWhenLimitedSessionCloses` proves a queued request
  fails and is not replayed when that limited session closes; the timeout tests
  prove Swift sends `CancelRequest` and ignores late responses while the
  connection stays usable; and the unknown-response test proves a protocol
  error frame is sent while pending calls fail. Tracey now reports no remaining
  untested Swift `session`,
  `session.peer`, `session.role`, `session.parity`,
  `session.connection-settings*`, `session.protocol-error`,
  `rpc.flow-control`, `rpc.flow-control.max-concurrent-requests*`, or
  `rpc.observability.session-errors` rules.
- Swift cancel/channel independence now has Tracey-backed runtime coverage in
  `ConnectionFailureTests`. `inboundCancelDoesNotCloseRequestChannels`
  registers a request-created channel, receives a `CancelRequest` for that
  request, then proves a later channel item is still delivered without a
  protocol error. Tracey now reports no remaining untested Swift `rpc.cancel`
  or `rpc.cancel.channels` rules.
- Swift one-service-per-connection behavior now has Tracey-backed runtime
  coverage in `ConnectionFailureTests`.
  `inboundOpenConnectionAcceptsAndDispatchesOnVirtualConnection` keeps the
  root connection on its original dispatcher, accepts an inbound virtual
  connection with a separate dispatcher, and proves requests on that virtual
  connection are handled by the accepted service dispatcher. Tracey now reports
  no remaining untested Swift `rpc.one-service-per-connection` rule.
- Swift runtime pipelining now has Tracey-backed coverage in
  `ConnectionFailureTests`. `slowIncomingRequestDoesNotBlockLaterRequest`
  keeps the root connection alive, feeds two inbound requests through
  `Driver.run`, lets the first dispatch sleep, and proves the second response
  is sent before the first delayed response. Tracey now reports no remaining
  untested Swift `rpc.pipelining` rule.
- Rust runtime pipelining now has matching Tracey-backed coverage in
  `vox-core`. `slow_incoming_request_does_not_block_later_request` runs a real
  initiator/acceptor session over `MemoryLink`, sends two concurrent calls to a
  handler that delays method 1 but replies immediately to method 2, and proves
  method 2 completes before method 1. Tracey now reports no remaining untested
  Rust `rpc.pipelining` rule.
- Rust request-ID and protocol-error handling now has Tracey-backed runtime
  coverage in `vox-core`. The driver rejects wrong-parity request IDs and
  duplicate live request IDs, sends a connection-0 `ProtocolError` before
  teardown, and peers treat received `ProtocolError` as protocol closure. The
  focused Rust tests also prove root connection setup, virtual connection
  opening, virtual connection request parity, session role/symmetry, and
  max-concurrent request flow-control behavior. Tracey now reports no remaining
  untested Rust `rpc.request.id-allocation`, `rpc.flow-control`,
  `session.protocol-error`, `connection.*`, `session.connection-settings*`,
  `session.parity`, `session.role`, or `session.symmetry` rules.
- Rust call-scoped error behavior now has Tracey-backed runtime coverage in
  `vox-core`. `call_error_does_not_close_connection_or_cancel_other_requests`
  runs a real initiator/acceptor session over `MemoryLink`, returns a
  call-level `InvalidPayload` error for one request while another request
  remains in flight, proves the in-flight request still completes, and then
  proves the connection remains usable for a follow-up call. Tracey now reports
  no remaining untested Rust `rpc.error.scope` rule.
- TypeScript core session/RPC envelope behavior now has matching Tracey-backed
  runtime coverage in `src/session.test.ts`: the phon self-describing handshake
  exchanges the Message schema closure, rejects invalid peer Message schemas
  with `Sorry` before post-handshake traffic, preserves peer settings and
  metadata, keeps protocol schemas session-scoped, preserves connection IDs on
  messages, allocates request IDs from parity, and observes responses on the
  same request IDs. Tracey now reports no remaining untested TypeScript
  `session.*`, `rpc.request.*`, or `rpc.response.*` rules.
- TypeScript fallible method and `VoxError` behavior now has Tracey-backed
  coverage in both the runtime and generated-client shape. `vox-wire` tests
  prove `RpcError` discriminants distinguish user, unknown-method,
  invalid-payload, cancelled, and indeterminate outcomes; `vox-core`
  `src/session.test.ts` proves actual `Result<T, VoxError<E>>` response bytes
  map to `RpcError` without closing the connection; and the `vox-codegen`
  TypeScript target test proves generated fallible client methods return
  `{ ok: true, value }` for success, return `{ ok: false, error }` only for
  user errors, and rethrow non-user errors. Tracey now reports no remaining
  untested TypeScript `rpc.fallible.*` or `rpc.error.*` rules.
- TypeScript unknown-method handling now has driver-side Tracey-backed runtime
  coverage. `vox-core` `src/driver.channel_schema.test.ts` sends an unknown
  method ID to a dispatcher with a real service descriptor, proves dispatch is
  not invoked, decodes the response as `Err(VoxError::UnknownMethod)`, and
  fails the test if the connection is closed instead of returning a call-level
  response. Tracey now reports no remaining untested TypeScript
  `rpc.unknown-method` rule.
- TypeScript pipelining now has driver run-loop coverage. `vox-core`
  `src/driver.channel_schema.test.ts` feeds two incoming calls through
  `Driver.run`, suspends the first dispatch, lets the second dispatch reply,
  and proves the second response is sent before the first is released. Tracey
  now reports no remaining untested TypeScript `rpc.pipelining` rule.
- TypeScript cancellation now has session-level coverage for both the request
  cancel frame and independent channel lifecycle. `vox-core`
  `src/session.test.ts` sends a call with a channel ID, observes the inbound
  call, sends `Cancel`, observes the queued cancel and cleared live-request
  count, then sends channel data and receives it through the still-open channel
  before allowing a late response frame. Tracey now reports no remaining
  untested TypeScript `rpc.cancel.*` rules.
- TypeScript channel discovery now has caller and callee runtime coverage.
  `vox-core` `src/channeling/binding.test.ts` proves direct channel arguments
  are discovered left-to-right before wire indexes are assigned, even if the
  generated channel metadata is out of order. `src/driver.channel_schema.test.ts`
  proves the callee resolves decoded wire-index bytes through
  `Request.channels` to create the correct server-side `Rx` and `Tx` handles.
  Tracey now reports no remaining untested TypeScript `rpc.channel.discovery`
  rule.
- TypeScript flow-control now has Tracey-backed coverage for both levels of
  backpressure named by the umbrella rule. `vox-core` `src/session.test.ts`
  proves peer `max_concurrent_requests` blocks a second outbound call until
  capacity is released, while `src/channeling/registry.test.ts` proves
  per-channel credit exhaustion blocks outgoing data until credit is granted.
  Tracey now reports no remaining untested TypeScript `rpc.flow-control` rule.
- TypeScript link and transport behavior now has Tracey-backed coverage across
  the concrete transport packages. `vox-inprocess` proves owned byte buffers,
  message boundaries, empty payloads, in-order delivery, send-after-close
  errors, graceful close, and repeated EOF for the in-process/memory link.
  `vox-core` proves `singleLinkSource` yields one attachment and preserves the
  optional client hello. `vox-tcp` proves stream/local length-prefixed
  round-trips, oversized frame rejection before writing, complete-frame send
  publication, terminal receive errors, and cancel-safe partial-frame receive.
  `vox-ws` proves WebSocket link/source construction, send/recv forwarding, and
  EOF after close. Tracey now reports no remaining untested TypeScript `link.*`
  or `transport.*` rules.
- Rust stream and memory link behavior now has Tracey-backed coverage in
  `vox-stream` and `vox-core`. The focused run
  `cargo nextest run -p vox-core -p vox-stream -E 'test(memory_link_preserves_boundaries_order_and_close) | test(round_trip_single) | test(multiple_messages_in_order) | test(eof_on_peer_close) | test(recv_error_is_terminal)'`
  passes 5/5. It proves `MemoryLink` preserves message boundaries, empty
  payloads, order, and graceful close; `StreamLink` splits into independent
  halves over arbitrary Tokio read/write pairs, preserves message boundaries
  and order, yields owned receive payloads, preserves graceful close ordering,
  and treats a partial-frame receive error as terminal. Tracey now reports no
  remaining untested Rust `link.*`, `transport.memory`, or
  `transport.stream.kinds` rules.
- Swift stream link behavior now has Tracey-backed coverage in
  `TransportTests` against the actual `NIOFrameLink` and length-prefixed frame
  codec. `tcpStreamLinkPreservesBoundariesOrderEmptyPayloadsAndEof` proves TCP
  stream links preserve message boundaries, empty payloads, delivery order,
  owned receive buffers, and repeat EOF. `tcpStreamLinkSendsFramesClosesAndRejectsOversizedPayloads`
  proves outbound frame-size limits reject before publication, committed frames
  are delivered exactly once, and graceful close reaches the peer after
  committed frames. `tcpStreamLinkReceiveErrorIsTerminal` proves decode errors
  are terminal, and `unixStreamLinkConnectsToLocalSocketTransport` proves the
  local Unix-socket stream constructor. `singleLinkSourceYieldsOneFreshAttachment`
  covers the Swift `link.split` single-attachment source. The Swift
  `NIOFrameLink` now applies `setMaxFrameSize` to outbound sends as well as
  inbound decoding, and receive-side link errors finish the stream. Tracey now
  reports no remaining untested Swift `link.*`, `transport.stream`, or
  `transport.stream.*` rules. A focused Swift send/receive cancellation-safety
  proof remains a reasonable transport hardening item for the stream/local
  transport that actually ships; memory, in-process, WebSocket, and
  browser-only transports are not Swift implementations and are not 1.0
  blockers.
- TypeScript runtime observability now has Tracey-backed coverage for its
  local-only diagnostics surfaces. `vox-core` `src/observer.test.ts` proves
  `setVoxLogger` installs and clears a local runtime observer without a
  telemetry backend. `src/channeling/registry.test.ts` proves channel
  open/receive/send/credit/close events are emitted through the installed
  logger while existing debug snapshots keep channel context. `src/logging.test.ts`
  proves server request/failed-response diagnostics include service, method,
  outcome, duration, and error data. `src/session.test.ts` proves protocol
  violations are surfaced as `ProtocolError` frames and session teardown rather
  than ordinary EOF. Tracey now reports no remaining untested TypeScript
  `rpc.observability.*` rules.
- TypeScript generated RPC caller, service, handler, and session setup surfaces
  now have Tracey-backed codegen coverage. The `vox-codegen` TypeScript target
  test `generated_typescript_emits_rpc_caller_handler_and_session_shapes`
  proves generated callers expose async methods that route through
  `Caller.call` with descriptor, method-schema, and registry data;
  `connect<Service>` builds a generated client from the established root
  connection; generated handler interfaces preserve service method signatures;
  and generated dispatchers expose one service descriptor, route by method ID,
  invoke the handler, and reply through `VoxCall`. Tracey now reports no
  remaining untested TypeScript `rpc.caller`, `rpc.handler`,
  `rpc.service.*`, or `rpc.session-setup` rules.
- Swift inbound virtual connection acceptance now has focused runtime coverage
  in `swift test --package-path swift/vox-runtime --filter
  ConnectionFailureTests`: `inboundOpenConnectionAcceptsAndDispatchesOnVirtualConnection`
  injects a peer `ConnectionOpen`, observes `ConnectionAccept`, and verifies a
  request on the accepted non-root connection dispatches through the installed
  service dispatcher.
- TypeScript virtual connection opening and acceptance now have focused
  `vox-core` runtime coverage in `pnpm --filter @bearcove/vox-core exec vitest
  run src/session.test.ts`: the parity test opens virtual connections from both
  session roles and waits for accepts, and the inbound test injects
  `ConnectionOpen`, observes `ConnectionAccept`, then routes a call and response
  on the accepted non-root connection handle. The same focused suite also proves
  TypeScript `ConnectionReject` behavior when an inbound virtual connection is
  opened without a configured acceptor, and verifies root connection settings
  plus the public `closeConnection(0)` rejection path. Swift's focused
  connection suite now also verifies the exposed root connection ID and a
  successful root request/response path. Tracey reports no remaining untested
  `connection.*` rules for Swift or TypeScript.
- Swift and TypeScript virtual connection close semantics now have Tracey-backed
  coverage. Swift's `ConnectionFailureTests` injects `ConnectionClose`, verifies
  the pending virtual call fails, verifies an existing local handle cannot send
  another request on the closed ID, and verifies a peer message on that closed ID
  produces `call.lifecycle.unknown-connection-id`. TypeScript's
  `src/session.test.ts` verifies the last virtual caller sends
  `ConnectionClose`, verifies receiving `ConnectionClose` marks the virtual
  handle closed, and verifies a later peer request on the closed ID sends a
  `ProtocolError`. The focused Swift suite passes 26/26, the focused TypeScript
  session suite now passes 26/26, and Vox `pnpm check` remains green.
- TypeScript caller liveness is now fully Tracey-verified: the focused
  `vox-core` session suite proves virtual connections stay live while any
  caller handle for that connection remains undisposed, then send
  `ConnectionClose` only after the last caller handle is disposed. This closes
  TypeScript's `rpc.caller.liveness.*` untested set.
- Swift caller liveness is now Tracey-backed through `Connection` lifetime,
  which is the Swift runtime's public caller handle. Dropping the last outbound
  virtual `Connection` reference sends `ConnectionClose`; dropping the root
  `Connection` marks the session internally closed without sending a root close
  frame and waits for retained virtual connections before driver teardown. The
  focused Swift suite passes 26/26, the full Swift `vox-runtime` package test
  suite passes 51/51, and
  `swift build --package-path swift/subject` still builds the generated Swift
  subject package.
- Vox `session.keepalive` now has Tracey-backed protocol keepalive coverage
  for Ping/Pong handling and missing-Pong teardown in Rust, Swift, and
  TypeScript. Swift's focused keepalive path passes in
  `swift test --package-path swift/vox-runtime --filter keepalive`.
- TypeScript `vox-core` sends a connection-0 `ProtocolError` frame before
  tearing down on locally detected protocol violations, and treats received
  `ProtocolError` frames as peer-originated teardown without ping-ponging an
  error back.
- TypeScript `vox-core` implements session and connection parity for
  connection/request/channel ID allocation, max-concurrent request flow control
  in both outbound and inbound directions, local debug snapshots, channel debug
  context, detailed try-send outcomes for observers, low-cardinality metric
  label selection, and deterministic caller liveness via explicit caller
  disposal.
- TypeScript subject lifecycle now uses one shared inactivity guard for both
  the normal generated subject and the evolved schema-compat subject. The shared
  evolved-subject harness also shuts the session handle down on exit.
- Vox now specifies `hosted.subject.lifecycle`: compliance subjects must exit
  on peer disconnect/session shutdown, enforce an inactivity timeout, and be
  spawned by the harness with child ownership that prevents accumulation.
  `cargo nextest run -p spec-tests -E 'test(subject_exits_when_harness_disconnects)'`
  in `~/vox` passes Rust TCP, Swift TCP, TypeScript TCP, and TypeScript
  WebSocket subject teardown checks. Tracey sees implementation and verification
  references for this rule in Rust, Swift, and TypeScript.
- Vox `rpc.channel.connection-closure` now has close-all teardown evidence in
  Swift and TypeScript as well as Rust. TypeScript `ChannelRegistry.closeAll()`
  terminates live receivers and blocked senders in
  `pnpm --filter @bearcove/vox-core exec vitest run src/channeling/registry.test.ts`;
  Swift `ChannelRegistry.closeAllChannels()` has the same focused coverage in
  `swift test --package-path swift/vox-runtime --filter ChannelFlowControlTests`.
- TypeScript `vox-tcp` now provides a `LocalLink`, reconnecting
  `LocalLinkSource`, and `LocalLinkAcceptor` over Node local IPC addresses
  (Unix sockets on Unix-like platforms, named-pipe paths on Windows), using the
  same length-prefixed framing as TCP.
- TypeScript length-prefixed framing has coverage for partial-frame receive
  timeouts: transport-owned frame state survives the stopped receive and the
  next receive gets the completed frame.
- Swift `VoxRuntime` compiles and passes its runtime tests, including virtual
  connection open, Phon handshake/envelope schema exchange, metadata sigils,
  channel flow-control, and schema/channel binding coverage.
- Swift `subject-swift` compiles and passes its generated-service corpus tests,
  proving the generated Swift testbed bridge can route args, responses, and
  schema closures through the Phon typed runtime.
- The generated `echo_ecosystem_bridge` method root now passes the focused Vox
  spec matrix in both directions across Rust TCP, Swift TCP, TypeScript TCP, and
  TypeScript WebSocket. This proves the first Dodeca-shaped ecosystem payload
  root through generated clients/dispatchers, schema exchange, typed args, and
  typed responses.
- The generated `echo_dodeca_template_call` method root now passes the same
  focused 8-case Vox spec matrix. This adds the Dodeca dynamic-value root:
  `facet_value::Value` args, dynamic object/scalar payloads, and tuple-vector
  kwargs (`Vec<(String, Value)>`) through generated Rust, Swift, and TypeScript
  clients/dispatchers.
- The generated `dodeca_byte_tunnel` method root now passes the focused 8-case
  Vox spec matrix as well. This mirrors Dodeca `cell-http-proto::TcpTunnel`
  with direct non-nested `Rx<Vec<u8>>` and `Tx<Vec<u8>>` parameters, proving
  byte-channel item encode/decode and channel binding through generated Rust,
  Swift, TypeScript TCP, and TypeScript WebSocket subjects.
- The generated Dodeca-shaped `dodeca_html_process`,
  `dodeca_execute_code_samples`, `dodeca_load_data`,
  `dodeca_parse_and_render`, and `dodeca_devtools_lsp` method roots now
  broaden the focused Dodeca Vox spec matrix to 56 passing cases. This adds the
  `cell-html-proto::HtmlProcessor::process`-style DTO with optional maps,
  string sets, maps to nested code metadata, image variant maps, Vite CSS maps,
  injections, mount localization, and result enums; the
  `cell-code-execution-proto::CodeExecutor::execute_code_samples`-style DTO
  with code samples, dependency config, native-sized source lines, build
  metadata, and `Vec<(CodeSample, ExecutionResult)>`; the Dodeca data-loader
  root carrying parsed dynamic values; the markdown parse/render root with
  frontmatter, headings, req definitions, injections, and source maps; and the
  `dodeca_protocol::DevtoolsService::lsp`-style non-nested `Rx<String>` /
  `Tx<String>` channel path across generated Rust, Swift, TypeScript TCP, and
  TypeScript WebSocket subjects in both caller/callee directions.
- The generated Dibs-shaped `dibs_schema`, `dibs_list`, `dibs_get`,
  `dibs_create`, `dibs_update`, `dibs_delete`, `dibs_migration_status`, and
  `dibs_migrate` method roots now pass the focused 64-case Vox spec matrix.
  This covers Dibs/Squel schema metadata, SQL value enums, rows, filters, sort
  clauses, options, bytes, CRUD request/response roots, migration status rows,
  `Result<T, DibsError>`, and migration log streaming through
  `Tx<DibsMigrationLog>` across generated Rust, Swift, TypeScript TCP, and
  TypeScript WebSocket subjects in both caller/callee directions.
- The generated Styx-shaped recursive, extension, and host callback roots now
  pass the focused 120-case Vox spec matrix. This covers `echo_styx_value`;
  LSP extension roots for initialize, completions, hover, inlay hints,
  diagnostics, code actions, definition, and shutdown; and host callback roots
  for subtree/document/source/schema lookup plus offset/position conversion.
  The payload surface includes a recursive `StyxValue` tree with `Option<Tag>`,
  `Option<Payload>`, `StyxPayload::{Scalar,Sequence,Object}`, recursive
  `Vec<StyxValue>`, recursive entry key/value pairs, spans, doc comments,
  `Option<StyxValue>` in LSP params/results, and generated Rust, Swift,
  TypeScript TCP, and TypeScript WebSocket subjects in both caller/callee
  directions.
- The generated Stax-shaped `stax_flamegraph`,
  `echo_stax_flamegraph_update`, `stax_subscribe_flamegraph_updates`, and
  `echo_stax_linux_broker_control` method roots now pass focused Vox
  spec-matrix coverage. The flamegraph roots cover decoded request filters
  (`ViewParams`, `LiveFilter`, `TimeRange`, `SymbolRef`), recursive
  `FlamegraphUpdate` payloads with `FlameNode.children`, string tables,
  `Option<u32>` indices, scalar-heavy `u64` timing/counter fields, and a
  non-nested `Tx<StaxFlamegraphUpdate>` subscription root across generated
  Rust, Swift, TypeScript TCP, and TypeScript WebSocket subjects in both
  caller/callee directions. The Linux broker-control root adds ordinary DTO
  coverage for config/status/error shapes across the same generated bridge path
  without pretending file descriptors are cross-language payload data.
- A Rust-only Stax-shaped Linux fd-broker service now passes a focused Vox
  transport fixture over a real `#[vox::service]`: `PerfSessionConfig` in,
  `Result<PerfSessionFds, PerfSessionError>` out, `Vec<vox::Fd>` bundles over
  `FdStreamLink`, and explicit refusal over TCP. This proves the fd-capable
  local transport path without pretending `vox::Fd` is a cross-language DTO.
- The generated Hotmeal-shaped `echo_hotmeal_live_reload_event` and
  `echo_hotmeal_apply_patches_result` method roots now pass the focused
  16-case Vox spec matrix. This covers live-reload event variants
  (`Reload`, `Patches { route, patches_blob }`, `HeadChanged`) and the
  browser-fuzzer patch result with recursive `DomNode`, element attributes,
  patch trace entries, bytes, options, and generated Rust, Swift, TypeScript
  TCP, and TypeScript WebSocket subjects in both caller/callee directions.
- The generated Helix-shaped `echo_helix_stream_metrics`,
  `echo_helix_verify_evidence`, and `helix_subscribe_pulses` method roots now
  pass the focused 24-case Vox spec matrix. This covers Helix trace-server
  metric vectors (`Vec<u64>`, `Vec<f64>`), transparent pulse/audio/text ID
  wrappers, nested verify evidence rows with `Option` and enum status values,
  f32 evidence scores, and the non-nested `Tx<PulseAvailable>` subscription
  item path across generated Rust, Swift, TypeScript TCP, and TypeScript
  WebSocket subjects in both caller/callee directions.
- The generated Helix-shaped `helix_pulse_bundle` method root now passes the
  focused Helix Vox spec matrix, bringing that matrix to 32 passing cases. This
  covers the `PulseBundleFields` request mask plus a coherent large
  `PulseBundle` response with optional per-panel rollups: prompt layout, audio
  provenance, attention heatmap, encoder frontier, encoder provenance report,
  audio/mel clips, pulse rollup, timeline event enums, Chrome trace event maps,
  verify evidence, and scheduler evidence snapshots across generated Rust,
  Swift, TypeScript TCP, and TypeScript WebSocket subjects in both
  caller/callee directions.
- The generated Helix-shaped `helix_trace_service_surface` method root now
  passes the same bridge matrix in both directions across Rust TCP, Swift TCP,
  TypeScript TCP, and TypeScript WebSocket. This carries the broad trace-server
  aggregate: attention summary batches, attendance rows, audio self-attention,
  transcript tokens, decoder-evidence reports, piece-eval reference/snapshot
  DTOs, clips, provenance, Chrome trace events, scheduler evidence, and the
  bundle mask/response through generated clients and dispatchers.
- The generated Tracey-migration-shaped `tracey_status`, `tracey_rule`,
  `tracey_validate`, `tracey_uncovered`, `tracey_untested`, `tracey_stale`,
  `tracey_unmapped`, `tracey_config`, `tracey_vfs_open`,
  `tracey_vfs_change`, `tracey_vfs_close`, `tracey_reload`,
  `tracey_version`, `tracey_health`, `tracey_shutdown`,
  `tracey_lsp_surface`, `tracey_lsp_workspace_diagnostics`, and
  `tracey_subscribe_updates` method roots, plus the dashboard/query/config
  mutation roots `tracey_forward`, `tracey_reverse`, `tracey_file`,
  `tracey_spec_content`, `tracey_search`, `tracey_update_file_range`,
  `tracey_config_add_exclude`, and `tracey_config_add_include`, now pass the
  focused 64-case Vox spec matrix. This covers the current roam-to-Vox
  migration target shape:
  `RuleId`, `usize` counts and source locations as fixed-width wire integers,
  validation enum/options/vectors, uncovered/untested/stale/unmapped rule
  query DTOs, daemon config/health/reload/version/control roots, VFS overlay
  open/change/close notifications, LSP-style diagnostics, the full current LSP
  support family mirrored as one generated surface sweep (test-file
  classification, hover, definition, implementation, references, completions,
  document/workspace symbols, semantic tokens, code lens, inlay hints, prepare
  rename, rename, code actions, and document highlight), and the non-nested
  `Tx<DataUpdate>` subscription item path. The dashboard sweep adds forward
  and reverse coverage models, rendered file/spec content with highlighted
  search results, nullable query responses, `Result<(), TraceyUpdateError>`,
  and `Result<(), String>` config mutation errors across generated Rust,
  Swift, TypeScript TCP, and TypeScript WebSocket subjects in both
  caller/callee directions. `cargo nextest run -p spec-tests -E 'test(tracey_)'
  --no-fail-fast -j 1` in `~/vox` currently passes 64/64.
- TypeScript generated enum DTOs now use `$tag` as the discriminant only when a
  struct variant has a real payload field named `tag`, preserving that payload
  field instead of emitting an impossible duplicate `tag` property. Phon's
  TypeScript typed front door has matching `$tag` encode/decode coverage.
- TypeScript codegen now treats channel element DTOs as first-class generated
  types and includes direct channel element shapes in the local Phon registry,
  so TypeScript callees can encode structured channel items such as
  `DibsMigrationLog`.
- Rust `vox-codegen` and `vox-phon` pass targeted `cargo nextest` coverage for
  generated Swift/TypeScript channel rejection, Phon schema closure emission,
  schema compatibility snapshots, and Vox wire payload round-trips.
- Rust `vox-phon` now treats owned-pointer Phon programs as native-supported
  when their pointee program is native-supported. The focused
  `cargo nextest run -p vox-phon -E 'test(native_jit) | test(vox_wire_shapes_report_native)'`
  run passes 3/3, including typed and compatibility-decode native-status
  coverage for the real `spec-proto::DodecaParseResult` shape whose success arm
  contains `Box<DodecaSourceMap>`.
- Swift codegen now emits recursive descriptor schema refs from Phon's
  root-context derived descriptor instead of recomputing child shape ids in
  isolation. This is covered by a `vox-codegen` regression test for the Styx
  root and fixed the generated Swift typed lowering for recursive
  option/list/enum descriptors.
- Native-sized Rust integers are fixed-width wire types: `usize` maps to `u64`
  and `isize` maps to `i64` on every platform. The focused Phon fixture
  `native_sized_integers_are_fixed_width_on_the_wire` passes, and the
  same-schema JIT layout/round-trip test remains green. On current macOS
  aarch64 those derived fields lower as ordinary 8-byte scalars; the
  `MemOp::NativeInt` interpreter path remains the correctness path for
  narrower or otherwise mismatched memory widths with range checks on decode.
  `Set<T>` can use the native path when its element program is
  native-supported.

Known holes still remaining after the current Vox TypeScript direct-shape
closure:

- Swift now has Phon-side fixture parity for the current Swift-applicable
  ecosystem payload families: Bee feed roots, Dodeca set/template/HTML
  processor/data-loader/markdown parse/image processor/search indexer roots,
  Dibs SQL row/list response, generated Dibs Squel service roots, Styx
  recursive value/LSP aggregate, Stax recursive flamegraph and Linux
  broker-control DTO slices, the Hotmeal live-reload payload family, the broad
  Helix `TraceService` aggregate, and Tracey migration DTOs. Focused Swift compat tests now cover duplicate set/map
  rejection through both canonical decode and `planDecode`, and capability roots
  stay out of compatibility planning. Rust remains the owner for actual
  fd-capable transport diagnostics because Swift has generated-binding
  rejection for fd service surfaces rather than a platform fd transport surface.
- TypeScript now has engine-level fixture parity for the browser/websocket-facing
  and DTO-shaped payload families, including the broad Helix `TraceService`
  aggregate. Generated Vox TypeScript bridge parity is proven for Dodeca
  ecosystem/template/HTML/code-execution/data-loader/markdown
  parse/image processor/search indexer/byte-channel/LSP channel roots, while
  Phon TypeScript engine fixtures also cover the Dodeca markdown parse/render
  result wire DTO and image processor byte/scalar/result root plus the search
  indexer page/file/result root, the Dibs
  schema/list/get/create/update/delete/migration-status and migration-log
  roots, the Styx recursive value/LSP
  extension/host callback roots, and the Stax flamegraph plus Linux
  broker-control DTO roots, plus the Hotmeal live-reload/browser-fuzzer roots, Helix
  metric/verify/pulse/bundle/trace-service roots, and Tracey migration
  status/rule/validation/core-control/full-LSP/update roots. Remaining
  TypeScript breadth is generated Vox bridge parity, not the Phon engine
  fixture corpus.
- Generated Vox bridge coverage is proven for the testbed bridge path, the
  Dodeca ecosystem/template/HTML/code-execution/data-loader/markdown
  parse/image processor/search indexer/byte-channel/LSP channel roots,
  the Dibs schema/list/get/create/update/delete/migration-status and
  migration-log roots, the Styx recursive value/LSP extension/host callback
  roots, the Stax flamegraph request/update/subscription and Linux
  broker-control DTO roots, and the Hotmeal live-reload/browser-fuzzer roots,
  plus the Helix metrics/verify
  evidence/pulse subscription/PulseBundle/TraceService aggregate roots and
  Tracey migration
  status/rule/validation/core-control/full-LSP/update/dashboard/query/config
  mutation roots. Remaining generated-bridge breadth is now dominated by any
  Dodeca roots still outside the current data-loader/markdown/devtools/image
  processor/search indexer slices, and any newly identified channel item paths
  or externals, not by the current Tracey daemon protocol.
- Helix generated bridge coverage is still representative, not a complete
  mirror of every trace-viewer endpoint. The `PulseBundle` request mask and
  bundle slots plus the broad `TraceService` aggregate now have generated
  bridge coverage through a local mirror of the Helix wire shape. Rust, Swift,
  and TypeScript Phon fixtures cover the broader live `TraceService` return
  surface, and Rust, Swift, and TypeScript benchmarks now carry that aggregate:
  Rust and Swift as native-clean typed/JIT benchmarks, and TypeScript as
  direct public-shape typed JIT benchmarks. Standalone Helix endpoint roots
  outside the aggregate mirror are still open.
  Tracey migration generated bridge coverage now mirrors the
  current LSP, core/control, dashboard/query-model, and config mutation surface
  from the current roam protocol.
  Hotmeal payload roots are covered; the exact callback-style `subscribe` /
  `on_event` service shape can still be added if we want that separate smoke
  path.
- External values such as `vox::Fd` now have explicit Rust transport and
  non-Rust generated-binding diagnostics. Phon-side Stax fixtures prove the
  ordinary Linux broker DTOs and keep the fd bundle visible as unsupported
  `External("fd")` payload/capability planning; Vox-side tests prove
  descriptor-bearing frames are refused on non-fd transports and Swift/TypeScript
  codegen refuses fd-bearing service surfaces. Subject teardown has focused
  disconnect coverage across Rust TCP, Swift TCP, TypeScript TCP, and
  TypeScript WebSocket, plus clean post-run process sweeps after the current
  424-case ecosystem bridge matrix; longer repeated-run stress can still be
  added, but there is no current subject accumulation after the roadmap bridge
  gate.
- Benchmarks exist for Bee, the Rust ecosystem payload families including Dibs
  SQL rows, generated Squel service roots, Dodeca data-loader results, and
  Dodeca parse results with boxed source maps, image processor roots, and search
  indexer roots, and Swift ecosystem Dodeca set/template/HTML/data-loader/parse
  roots plus image processor and search indexer roots, Dibs SQL row/list,
  generated Dibs Squel service roots, Dibs migration service roots, Styx
  recursive/LSP, and Stax recursive plus Linux broker-control fixtures plus the
  broad Helix `TraceService` aggregate, including representative channel
  payload families.
  The TypeScript engine benchmark now includes recursive call-block source
  generation for decode/encode, direct public-shape typed JIT rows for the
  broad Helix `TraceService` aggregate, and direct public-shape typed JIT rows
  for the Dodeca image processor and search indexer roots.
- TypeScript no longer needs a Rust/Swift-style descriptor-memory IR to be
  useful for generated clients. Its typed fast path is direct public JavaScript
  shapes, with the generic `Value` engine kept as the oracle and for real
  dynamic/schema-less payloads. The remaining TypeScript work is generated Vox
  bridge breadth and codegen parity for any consumer roots not yet in the
  matrix, while recursive fixture roots already run through generated call-block
  functions in both decoder and encoder JIT paths with empty decoder fallback
  reports. Direct-shape TypeScript JIT stays measured and useful, but it only
  moves onto the Vox 1.0 blocking path if a browser or websocket consumer
  benchmark proves that TypeScript-side encode/decode is the migration
  bottleneck.
- Phon Swift still has no in-package codegen module by design, so the Phon-side
  `codegen.*` Tracey holes remain out-of-package rather than missing Swift
  implementation work. Vox generated Swift bridge coverage exists for the
  current matrix through generated descriptors, `readerDescriptor`/`readerBlocks`,
  `decodeVoxTyped`, and `encodeVoxTyped`, including the focused Swift TCP
  ecosystem bridge run. The remaining Swift holes are borrowed descriptor
  decode, named thunk binding, total typed-IR lowering, and any future generated
  Swift consumer root not yet added to the Vox bridge matrix.

## Killed or out-of-scope surface

The following concepts must not be reintroduced while completing this roadmap:

- `binette`. Phon replaced it.
- Stable conduit.
- Retry-shaped generated code and retry semantics.
- Shared-memory or zero-copy product paths.
- Nested channels.
- Direct same-schema codec shortcuts that bypass compatibility planning.
- Treating Dibs SQL values or Styx tree values as generic dynamic values. They
  are ordinary derived-Facet payloads unless the consumer source proves
  otherwise.

## Consumer surface inventory

### Bee

Bee is the first hot-path target and is tracked in
`docs/content/vox-bee-jit-roadmap.md`.

The important surface is:

- Swift app to Rust engine over Vox FFI.
- Swift app to Swift IME over Vox local IPC.
- Hot `feed(session_id: String, samples: Vec<f32>)` request.
- Responses shaped as structs, vectors, options, and result enums.
- Trace-viewer `Tx<StreamItem>` as a later non-nested channel target.

Bee proves the baseline:

- strings
- scalars
- byte-like/bulk vectors
- structs
- lists
- options
- result/enums
- method-root fallback reporting
- Rust and Swift JIT benchmarks for hot encode/decode shapes

### Dodeca

Dodeca is the main expansion target after Bee and the largest known consumer
surface in the ecosystem sweep. Its protocol crates under
`~/dodeca/cells/*-proto` and `~/dodeca/crates/dodeca-protocol` use a much
wider payload surface than Bee.

Required shapes from Dodeca:

- `Vec<u8>` blobs for images, fonts, static content, HTML diffs, and tunnel
  bytes.
- `HashMap<String, String>`.
- `HashMap<String, Vec<String>>`.
- `HashMap<String, CodeExecutionMetadata>` and other maps to nested structs.
- `HashSet<String>`.
- Tuple vectors such as `Vec<(String, u32)>`,
  `Vec<(CodeSample, ExecutionResult)>`, and `Vec<(String, Value)>`.
- `facet_value::Value` in markdown/data/gingembre/host protocols.
- Non-nested channels such as `Rx<Vec<u8>>`, `Tx<Vec<u8>>`,
  `Rx<String>`, and `Tx<String>`.

Dodeca is the reason maps, sets, tuple vectors, dynamic values, and channel
binding must move from "eventually" to the Vox 1.0 compatibility path.

Dodeca fixture work should be split into:

- HTML processing and asset metadata: maps, sets, tuple vectors, nested structs,
  and `Vec<u8>`.
- Template and host calls: `facet_value::Value`, dynamic objects/lists/scalars,
  and tuple-vector kwargs.
- Data-loader results: `facet_value::Value` dynamic objects/scalars in enum
  response payloads.
- Markdown parse/render results: dynamic frontmatter extras, headings,
  requirement definitions, source-map entries, and Rust `Box<DodecaSourceMap>`
  owned-pointer descriptors.
- Image processor roots: PNG/JPEG/GIF byte inputs, decoded/resized image byte
  buffers with `u32` dimensions and `u8` channels, thumbhash data URLs, and
  image-processing result enums.
- Search indexer roots: rendered page lists in, generated static search file
  byte payloads out, and search-index result enums.
- Devtools/live-reload/tunnel protocols: non-nested byte and string channels.
- Generated-service roots: the actual request/response roots Vox codegen would
  see, not only isolated field-level types.

The generated Vox bridge now has checked-in Dodeca roots for the ecosystem
payload, dynamic template call, byte tunnel, HTML processing, code execution,
data loading, markdown parse/render, image processing, search indexing, and
devtools LSP string-channel shapes. The focused Dodeca matrix covers Rust TCP,
Swift TCP, TypeScript TCP, and TypeScript WebSocket in both directions and
passes 72/72 with
`cargo nextest run -p spec-tests -E 'test(dodeca)' --no-fail-fast -j 1`,
including the generated image/search roots. The narrower generated
image/search bridge slice also passes 16/16 with
`cargo nextest run -p spec-tests -E 'test(echo_dodeca_image_processor_fixture) | test(echo_dodeca_search_indexer_fixture)' --no-fail-fast -j 1`.
Rust, Swift, and TypeScript Phon-side fixtures now cover the HTML processor
map/set/tuple-vector root, the dynamic template-call root, the data-loader
dynamic-result root, the markdown parse/render result shape, and the image
processor byte/scalar/result root from `cell-image-proto`, plus the search
indexer page/file/result root from `cell-search-proto`. Rust keeps the real
boxed source-map owner in the parse-result fixture; Swift and TypeScript cover
the generated wire DTO shape where the source map is the pointee object. The
Swift roots stay native-clean in the Swift JIT, and the Rust benchmark corpus
now includes the data-loader result, boxed parse result, image processor roots,
and search indexer roots as native-clean selected-runtime benchmarks. The Swift
benchmark corpus now includes the data-loader result, parse result, image
processor roots, and search indexer roots as native-clean typed/JIT benchmarks.
The TypeScript benchmark corpus now includes direct public-shape typed JIT rows
for the image processor and search indexer roots.
Remaining Dodeca work is broadening to any additional service roots that become
part of the migration gate.

### Dibs

Dibs uses Vox for schema, migration, and admin CRUD surfaces.

Its SQL value is sent over Vox, but it is not a generic dynamic value. It is a
normal derived-Facet enum with variants for null, bool, integers, floats,
strings, and bytes. Rows and filters contain that enum inside structs and
vectors.

Required shapes from Dibs:

- normal `#[derive(Facet)]` payload enums with scalar and byte payloads
- `Vec<Value>` inside filters
- rows as vectors of field structs
- `Result<T, DibsError>`
- migration log streaming through `Tx<MigrationLog>`

Dibs should be treated as a correctness and generated-service fixture, not as
the source of dynamic-value requirements.

Dibs fixture work should prove:

- SQL value enum round-trips through interpreter, Rust JIT, Swift JIT, and
  TypeScript engine/codegen.
- Rows and filters use ordinary struct/list/enum planning.
- Migration log `Tx<MigrationLog>` uses the same channel item codec path as
  ordinary method responses.
- Error/result shapes stay plan-based and do not get special-cased in codegen.

The generated Vox bridge now has checked-in Dibs roots for the Squel `schema`,
`list`, `get`, `create`, `update`, and `delete` shapes plus Dibs migration
status and migration-log channel shapes. The focused matrix covers Rust TCP,
Swift TCP, TypeScript TCP, and TypeScript WebSocket in both directions.
`cargo nextest run -p spec-tests -E 'test(rpc_dibs_) |
test(subject_calls_dibs_)' --no-fail-fast` in `~/vox` selected the 64 Dibs
generated-bridge cases and now passes 64/64 under the default nextest profile.
The Swift transport and lifecycle cases have targeted nextest slow-timeout
overrides so a stale or absent release subject build does not masquerade as a
protocol failure or leave a killed Swift compiler process behind.
Rust, Swift, and TypeScript Phon-side fixtures now cover the SQL value row/list
response shape, including byte payloads, and the generated Squel
schema/list/get/create/update/delete/result DTO roots. They also now cover the
Dibs migration service aggregate from `dibs-proto`: `MigrationStatusRequest`,
`Vec<MigrationInfo>` status responses, `MigrateRequest`, `MigrateResult`, and
the `MigrationLog` channel item shape. Rust and Swift benchmarks keep the
broader Squel and migration service roots native-clean; TypeScript engine
fixtures run them through interpreter mode, requested-JIT mode, and encoder
JIT/fallback selection. Remaining Dibs work is only any additional generated
root that becomes a migration gate.

### Styx

Styx uses Vox for LSP extension callbacks.

`styx_tree::Value` is sent over Vox, but it is also not a generic dynamic value.
It is a recursive derived-Facet tree:

- `Value { tag: Option<Tag>, payload: Option<Payload>, span: Option<Span> }`
- `Payload::Scalar`, `Payload::Sequence`, `Payload::Object`
- `Sequence { items: Vec<Value> }`
- `Entry { key: Value, value: Value, doc_comment: Option<String> }`

Required shapes from Styx:

- recursive structs/enums
- nested options
- vectors of recursive values
- ordinary LSP request/response structs
- `Option<Value>` in both directions

Styx is a recursion pressure test, not a dynamic-value pressure test.

Styx fixture work should prove:

- Recursive descriptors lower without losing field names or variant payload
  structure.
- Recursive decode uses bounded validation and does not turn malformed input
  into runaway allocation or recursion.
- Swift and TypeScript agree with Rust on schema identity for the recursive
  value tree.
- Representative LSP request/response roots use the same generated-service path
  as the recursive value fixture.

The generated Vox bridge now has checked-in Styx roots for the recursive
`echo_styx_value`, LSP extension request/response methods
(`styx_lsp_initialize`, `styx_lsp_completions`, `styx_lsp_hover`,
`styx_lsp_inlay_hints`, `styx_lsp_diagnostics`, `styx_lsp_code_actions`,
`styx_lsp_definition`, `styx_lsp_shutdown`), and LSP host callbacks
(`styx_host_get_subtree`, `styx_host_get_document`, `styx_host_get_source`,
`styx_host_get_schema`, `styx_host_offset_to_position`,
`styx_host_position_to_offset`). The focused 120-case matrix covers Rust TCP,
Swift TCP, TypeScript TCP, and TypeScript WebSocket in both directions, and
specifically caught the Swift descriptor-id bug for recursive
option/list/enum schemas plus TypeScript fixture drift in nested recursive
contexts. Rust, Swift, and TypeScript Phon-side fixture coverage now carry the
Styx recursive value and aggregate LSP surface through the typed engine/JIT
oracle path; the Rust ecosystem benchmark includes the aggregate LSP surface,
and the Swift benchmark keeps both the recursive value and aggregate LSP
surfaces native-clean. Remaining Styx work is only broader consumer roots if
new Styx/Vox surfaces enter the migration gate.

### Stax

Stax uses Vox for daemon and live profiling protocols.

Required shapes from Stax:

- recursive `FlameNode { children: Vec<FlameNode> }`
- string tables and scalar-heavy profiling snapshots
- many non-nested `Tx<...Update>` subscriptions
- macOS record streaming through `Tx<KdBufBatch>`
- Linux fd brokering through `vox::Fd` and `Vec<vox::Fd>`

`vox::Fd` is a transport-owned external capability. It should not be treated as
ordinary payload data. The Phon/Vox bridge must have a clear external-value
story for it, but the payload JIT should not try to serialize file descriptors
as bytes.

Stax fixture work should prove:

- Recursive flamegraph snapshots are native-clean where recursive JIT support
  exists, or produce a recursive-block fallback report.
- Update subscriptions bind as non-nested channels and clean up with the
  subject/session that owns them.
- `vox::Fd` and `Vec<vox::Fd>` are represented as external capabilities with
  explicit unsupported diagnostics when the transport cannot carry them.

The generated Vox bridge now has checked-in Stax flamegraph roots via
`stax_flamegraph`, `echo_stax_flamegraph_update`, and
`stax_subscribe_flamegraph_updates`, plus the ordinary broker-control DTO root
`echo_stax_linux_broker_control`. The focused flamegraph matrix covers Rust
TCP, Swift TCP, TypeScript TCP, and TypeScript WebSocket in both directions,
proving request filter decoding, recursive flamegraph updates, and a
non-nested `Tx<StaxFlamegraphUpdate>` subscription item path through generated
clients/dispatchers. The broker-control root adds an 8-case focused matrix over
the same language/transport set in both directions, proving
config/status/error DTOs through generated bridges while leaving the fd handoff
transport-owned. Rust, Swift, and TypeScript Phon-side fixture coverage now
carries the recursive flamegraph shape; Swift also benchmarks that shape and
the ordinary Linux broker-control DTO shape as native-clean JIT coverage. The
Phon-side fixture corpus now also carries the macOS `KdBufBatch` stream item:
Rust models the complete macOS record/config/result/status fixture and keeps it
native-clean through the ecosystem typed/JIT test, Swift carries the high-volume
`KdBufBatch` channel item through the descriptor/interpreter/JIT equivalence
harness, TypeScript carries the complete public-shape DTO fixture through the
typed engine, and the Rust ecosystem benchmark includes a larger macOS batch
family.
Rust, Swift, and TypeScript Phon-side fixture coverage now also carries the
ordinary Linux fd-broker config/status/error DTOs through the typed engine/JIT
oracle path, while Rust and TypeScript manual external schemas prove
`External("fd")` encode/decode/planning fails explicitly instead of treating
descriptors as scalar payload. Vox Rust now has a Stax-shaped fd-broker
transport fixture proving the actual
`Vec<vox::Fd>` handoff over `FdStreamLink` and refusal over TCP. Swift and
TypeScript generated bindings reject fd-bearing service surfaces at codegen
time. The generated Vox bridge now also carries the exact macOS
`record(config, Tx<KdBufBatch>) -> Result<RecordSummary, RecordError>` method
root through the focused Stax macOS record matrix. Remaining Stax work is only
any broader live-profile subscription roots that become migration-gated.

### Helix Trace Server

The Helix trace server family uses Vox for trace queries and subscriptions.

Required shapes:

- nested trace/query structs
- options and vectors
- large numeric/vector responses
- non-nested `Tx<PulseAvailable>` style subscriptions

This is lower priority than Dodeca for shape coverage, but useful as a
large-response and generated-service regression target.

Helix fixture work should focus on generated query/response breadth and large
numeric payloads. It should not outrank Dodeca container/dynamic/channel work,
but it should be part of the final ecosystem gate.

The generated Vox bridge now has checked-in Helix payload roots via
`echo_helix_stream_metrics`, `echo_helix_verify_evidence`,
`helix_subscribe_pulses`, `helix_pulse_bundle`, and
`helix_trace_service_surface`. The focused matrix covers Rust TCP, Swift TCP,
TypeScript TCP, and TypeScript WebSocket in both directions. This proves large
metric vectors, transparent ID wrappers, nested verify evidence rows, optional
seed/divergence fields, enum draft statuses, f32 evidence scores, the
non-nested `Tx<PulseAvailable>` subscription item path, and a coherent
`PulseBundle` response with field masks, optional rollups, timeline events,
Chrome trace maps, clips, heatmaps, provenance, and scheduler evidence through
generated clients/dispatchers. The generated `TraceService` aggregate spans the
live standalone query return families: `AttentionSummaryBatch`, attendance
rows, audio self-attention rows, transcript tokens, decoder-evidence reports,
piece-eval reference/snapshot DTOs, clips, provenance, Chrome trace events,
scheduler evidence, and the bundle mask/response. The focused Rust Helix
ecosystem run passes 2/2. TypeScript Phon-side fixtures now carry the same broad
aggregate through the table-driven ecosystem equivalence test with the JIT
fallback gate intact; the focused TypeScript ecosystem file passes 21/21 and
`pnpm check` is clean. Swift Phon-side fixtures now carry the same broad
aggregate through the cross-engine equivalence test with the native JIT fallback
gate intact; the focused Swift ecosystem fixture run passes 19/19. Generated
Vox bridge coverage is still representative, not a complete mirror of every
trace-viewer endpoint.

### Hotmeal

Hotmeal's live reload Vox surface is small.

Required shapes:

- basic service methods
- strings
- simple structs/enums
- websocket transport sanity coverage

Hotmeal is a smoke target for small browser-facing Vox use.

Hotmeal fixture work should exercise websocket transport, browser-facing
TypeScript codegen, and small service calls. It is the sanity check that the
ecosystem work did not optimize only the big Rust/Swift cases.

The Phon fixture corpus already models the callback payload surface as a
`HotmealSubscribeRequest` plus a delivered list of `HotmealLiveReloadEvent`
values. Rust derives the fixture from Facet, Swift carries it through the
descriptor/interpreter/JIT equivalence harness, and TypeScript carries it as a
public JavaScript-shape typed fixture.

The generated Vox bridge now has checked-in Hotmeal payload roots via
`echo_hotmeal_live_reload_event` and `echo_hotmeal_apply_patches_result`. The
focused matrix covers Rust TCP, Swift TCP, TypeScript TCP, and TypeScript
WebSocket in both directions. This proved live-reload event enums, byte blobs,
recursive browser DOM nodes, patch traces, and the TypeScript `$tag`
discriminator escape needed when an enum struct variant also has a real field
named `tag`. Swift Phon-side fixture coverage now includes the live-reload
event family and keeps the small enum/byte/list payload native-clean. The
remaining callback-shaped `subscribe` / `on_event` method shape is optional
generated Vox smoke coverage, not an open Phon typed-program or JIT gap.

### Tracey

In the current `~/tracey` checkout, `tracey-proto` uses `roam`, not Vox. It is
still a useful migration target because it has the shape of a large
dashboard/LSP service.

Expected migration shapes:

- many request/response structs
- strings, booleans, integers
- options and vectors
- `Result<(), Error>` style mutation methods
- one non-nested `Tx<DataUpdate>` subscription
- LSP-like vectors of diagnostics, locations, symbols, code actions, code
  lenses, inlay hints, and text edits

Tracey should be used as a future generated-service breadth target, not as proof
of current Vox coverage.

Tracey migration fixture work should model the target Vox service from the
current roam protocol shapes. The point is to cover dashboard/LSP breadth and a
`Tx<DataUpdate>` style subscription, not to claim the current Tracey checkout is
already a Vox consumer.

The generated Vox bridge now has checked-in Tracey migration roots via
`tracey_status`, `tracey_rule`, `tracey_validate`, `tracey_lsp_surface`,
`tracey_lsp_workspace_diagnostics`, `tracey_subscribe_updates`, and the
core/control roots for uncovered, untested, stale, unmapped, config, VFS
open/change/close, reload, version, health, and shutdown. It also now covers
the dashboard/query/config mutation roots for forward coverage data, reverse
coverage data, file content, rendered spec content, search results, inline file
range updates, and include/exclude config pattern mutations. The focused Tracey
matrix covers Rust TCP, Swift TCP, TypeScript TCP, and TypeScript WebSocket in
both directions and currently passes 64/64 in `~/vox`. This proves
representative status, rule info, validation, workspace diagnostics, current
LSP support, core/control DTOs, dashboard/query DTOs, config mutations, user
errors, nullable query responses, and update subscription payloads through
generated clients/dispatchers. Rust, Swift, and TypeScript Phon-side fixtures
now cover the representative migration aggregate; the Swift fixture keeps the
fixed-width status, uncovered, diagnostics, symbol, and update payloads
native-clean. Remaining Tracey work is only any newly discovered protocol root
from the current checkout; it is no longer the biggest compatibility surface.

## Shape compatibility matrix

Each shape below needs an explicit answer for:

- Phon schema support
- interpreter encode/decode correctness
- Rust JIT native coverage or fallback report
- Swift JIT native coverage or fallback report
- TypeScript engine/codegen coverage where relevant
- generated Vox Rust bridge
- generated Vox Swift bridge
- generated Vox TypeScript bridge where relevant
- fixture extracted from a real consumer
- benchmark when the shape is hot or large

### Baseline shapes

Covered first by Bee and core conformance:

- booleans
- signed and unsigned integers
- floats
- strings
- bytes and `Vec<u8>`
- bulk numeric vectors such as `Vec<f32>`
- structs
- unit enums
- payload enums
- options
- results
- lists

### Containers

Needed primarily by Dodeca:

- `HashMap<K, V>` / map schemas
- `HashSet<T>` / set schemas
- tuple values
- tuple vectors
- nested maps and sets inside options
- maps to nested structs
- maps to lists

### Dynamic values

Needed primarily by Dodeca:

- `facet_value::Value`
- dynamic scalars
- dynamic lists
- dynamic objects
- dynamic values inside result enums
- dynamic values inside tuple vectors such as kwargs

Dynamic is compatible only with dynamic. Do not treat a dynamic reader as a
magical reader for a concrete writer, or the reverse.

### Recursive shapes

Needed by Styx and Stax:

- recursive structs through `Vec<Self>`
- recursive enum/struct pairs
- optional recursive values
- recursion with spans or metadata fields

Recursive support must preserve cycle-free value traversal. Native JITs should
stay native-clean for the Styx/Stax recursive roots; intentionally deferred
recursive subtrees must still report a path-specific fallback rather than
collapsing the whole method into an unhelpful root fallback.

### Channels

Needed by Bee trace viewer, Dodeca, Dibs, Stax, Helix, and Tracey migration:

- non-nested `Tx<T>` service parameters
- non-nested `Rx<T>` service parameters
- channel element schema descriptors
- channel element encode/decode through the selected Phon engine
- lifecycle: close on disconnect, inactivity, or service teardown
- explicit rejection for nested channels

The channel itself is a capability. The stream items are normal messages and
must use the same compatibility planning and engine selection rules as ordinary
method arguments and responses. Rust, Swift, and TypeScript now carry focused
boundary tests proving channel roots are rejected by the core payload planner
while writer/reader channel item schemas still use ordinary compat decode.

### External values

Needed by Stax:

- `vox::Fd`
- `Vec<vox::Fd>`
- external metadata compatibility
- transport handoff through fd passing on platforms that support it
- explicit unsupported diagnostics on platforms/transports that do not

External values are not normal serialized payloads. Their compatibility story
belongs in the schema and transport bridge, not in byte-oriented JIT stencils.
Rust, Swift, and TypeScript now also prove external roots stay outside core
payload compat while external metadata schemas are planned and decoded as
ordinary payload structs.

### Compat decode operations

Needed for versioning across all consumers:

- field matching by name
- writer-only field skip
- reader-only default
- enum variant remapping by name
- compatible nested containers
- compatible recursive references
- compatible channel item schemas, decoded as per-item messages
- compatible external metadata schemas, decoded separately from the transport
  capability handle

Same-schema hot paths must stay JIT-clean while compat-only operations are added
and audited. Compat JIT work should be driven by generated versioned fixtures,
not by hand-waved assumptions about forwards/backwards compatibility.

## Work tracks

### 1. Spec and Tracey cleanup

The spec must describe the actual Vox/Phon compatibility contract:

1. Keep the plan-first compatibility rule as the central law.
2. Keep the two product modes: JIT enabled and JIT not enabled.
3. Keep strict fallback reporting as diagnostic-only.
4. Specify supported containers in terms of schema kinds, not Rust-only type
   names.
5. Specify dynamic compatibility as dynamic-to-dynamic only.
6. Specify channel compatibility and nested-channel rejection.
7. Specify external values as transport capabilities with metadata, not bytes.
8. Remove stale retry/stable-conduit/zero-copy language from any remaining
   specs or generated-code expectations.
9. Add Tracey annotations for every implemented rule in Rust, Swift, and
   TypeScript.
10. Add verification annotations for conformance, compatibility, JIT fallback,
    and generated-bridge tests.

The first Tracey cleanup pass has landed for Rust:

- Stale phon-core requirements for framing, transport-owned external attachment
  semantics, absolute-buffer zero-copy alignment, and thunk-only descriptor
  support were removed or reduced to non-normative transport/design prose.
- `decode.chained` is implemented by compact `read_from` and verified with a
  back-to-back message-cursor test.
- `validate.bundles` is implemented by `Registry::try_new` and verified against
  valid bundles, stale schema ids, incomplete closures, and unbounded
  zero-wire fixed arrays.
- Crate separation and binding-free engine/JIT rules are verified mechanically
  against the current Rust manifests.
- Rust Tracey now reports 60/60 implemented and 60/60 verified.

The first Swift Tracey audit has also landed:

- The Swift schema model, schema codec, value codec, identity computation,
  generic resolution, compact schema-driven decode, compact alignment, chained
  decode, bundle validation, hostile-input validation, package split, descriptor
  model, and implemented IR/JIT paths are annotated.
- Swift corpus tests now assert that the committed schema cases exercise array,
  tensor, channel, dynamic, external, generic refs, and every enum payload
  shape.
- Swift hostile tests now cover unknown tags, invalid UTF-8, invalid chars,
  length bombs, dimension bounds, nesting depth, and trailing bytes.
- Swift Tracey now reports 54/60 implemented and 57/60 verified.

The first TypeScript Tracey audit has landed for the schema and engine packages:

- The TypeScript schema model, schema parsing, generic substitution,
  schema-identity unknown-id errors, compact schema-driven decode, chained
  decode, hostile-input guards, and package split are annotated.
- TypeScript now recomputes content-derived `SchemaId`s with the same
  BLAKE3/SCC/backref algorithm as Rust and Swift, and validates received schema
  bundles for stale ids, incomplete closures, and unbounded zero-wire fixed
  arrays.
- TypeScript self-describing enum payload decode, TypeScript JIT opt-in
  selection, direct public-shape typed JIT encode/decode, and Rust-side
  TypeScript codegen/schema-source behavior are now included in the TypeScript
  Tracey implementation scope.
- TypeScript Tracey now reports 49/60 implemented and 49/60 verified.
- The remaining TypeScript holes are not untested implemented code. They are
  either intentionally non-applicable to the TypeScript value model or
  out-of-package surfaces: Rust-only subset support, Rust/Swift descriptor
  memory-model rules, memory/linear-op rules, and copy-and-patch stencil rules.

Tracey annotations must be honest:

- `r[impl ...]` goes on implementation code that actually enforces the rule.
- `r[verify ...]` goes on tests or conformance fixtures that would fail if the
  rule regressed.
- Generated code should not be annotated directly; annotate the generator.
- A rule that is intentionally unsupported should be in the spec as an explicit
  rejection or diagnostic, not as an uncovered wish.

### 2. Consumer fixture harvesting

Create Phon/Vox fixture definitions extracted from real consumer protocols:

1. Bee fixture: keep current hot roots native-clean.
2. Dodeca fixture: devtools, HTTP tunnel, HTML processor, image processing,
   search indexing, code execution, gingembre/host dynamic values,
   markdown/data dynamic values.
3. Dibs fixture: SQL value enum, rows, filters, migration status, migrate
   result, and migration logs.
4. Styx fixture: recursive `styx_tree::Value` and LSP request/response roots.
5. Stax fixture: recursive flamegraph update, update subscriptions, fd broker
   metadata.
6. Helix fixture: representative trace query and subscription payloads.
7. Hotmeal fixture: browser live reload smoke surface.
8. Tracey migration fixture: current roam proto shape represented as the target
   Vox service surface.

Fixtures should live close to the Phon/Vox integration tests and be generated
or mechanically derived where possible. They should not require building the
consumer repositories in normal verification.

Each fixture family should include:

- the consumer source path and commit/source date it was extracted from
- the service method root or value root it represents
- representative values large enough to exercise nested layout and allocation
- interpreter round-trip tests
- JIT fallback report assertions
- cross-language conformance vectors when the shape is part of public
  Rust/Swift/TypeScript surface
- generated Vox client/dispatcher coverage once the bridge is wired

Fixture values should be realistic, but not consumer builds. The normal Phon
verification loop must stay local to this repository and the Vox repository.

### 3. Interpreter and conformance correctness

Before demanding native JIT coverage for a shape, prove interpreter correctness:

1. Add conformance cases for maps, sets, tuples, dynamic values, channels,
   externals, and recursion where missing.
2. Add versioned compatibility vectors for map/set/tuple recursion and
   reader-only/writer-only changes.
3. Use the interpreter as the oracle for all JIT tests.
4. Keep hostile and malformed-input tests for recursive and dynamic values.
5. Ensure Swift, Rust, and TypeScript agree on schema identity and compact bytes
   for the same fixture values.

Interpreter acceptance for a shape means:

- schema identity is stable and cross-language
- compact encode/decode round-trips valid values
- malformed input is rejected before unsafe allocation or invalid memory writes
- compatibility plans are built before decode starts
- same-schema values still go through a translation plan
- the fixture can act as the oracle for JIT/native implementations

### 4. Rust JIT

Rust JIT priorities after the Bee baseline:

1. Preserve empty fallback reports for Bee hot method roots.
2. Make method-root fallback reporting available in generated Vox integration.
3. Add native coverage for maps and sets.
4. Add native coverage for tuples and tuple vectors.
5. Add native coverage or precise fallback reporting for dynamic values.
6. Keep recursive block support native-clean for Styx `Value` and Stax
   `FlameNode`, with precise fallback reports only for intentionally deferred
   recursive subtrees.
7. Keep native compat-only decode ops covered for versioned field/enum cases,
   and keep any future unsupported compat subtree path-specific in reports.
8. Treat channels and externals as bridge/capability work; JIT only covers the
   stream item payloads and external metadata payloads.

Rust JIT acceptance for a method root means:

- the cached typed program path is used
- interpreter and JIT produce byte-identical encodes where ordering is defined
- JIT decode produces the same value as interpreter decode
- fallback reports are empty for required native-clean roots
- fallback reports are path-specific for intentionally deferred subtrees
- unsupported `MemOp`s do not panic through the public JIT-enabled runtime path

### 5. Swift JIT

Swift JIT priorities after the Bee baseline:

1. Preserve native-clean Bee IME and engine hot roots.
2. Keep fallback reporting method-scoped and path-scoped.
3. Preserve native-clean coverage for string-keyed maps, managed set elements,
   Dodeca map shapes, and tuple-vector roots.
4. Keep dynamic `facet_value::Value` roots native-clean through the dynamic
   stencil path, and keep focused compat enum/skip/default decode ops
   native-clean while broader versioned corpus coverage is added.
5. Add or preserve native coverage for any additional tuple/vector roots that
   appear in the remaining consumer sweep.
6. Keep recursive block support native-clean for Styx `Value` and Stax
   `FlameNode`, with precise fallback reports only for intentionally deferred
   recursive subtrees.
7. Broaden the landed native compat enum/skip/default decode operations from
   focused tests to the versioned compatibility corpus.
8. Verify generated Vox Swift clients/dispatchers use the runtime-selected
   engine for method args, responses, envelopes, and channel elements.

Swift JIT acceptance mirrors Rust acceptance, with one additional rule: generated
Swift service code must depend on the Vox runtime's typed-engine abstraction,
not directly on the JIT package. The product surface remains JIT enabled or JIT
not enabled.

### 6. TypeScript engine and codegen

TypeScript is not a Rust/Swift descriptor-memory target and is not the first
native-performance gate. For Vox 1.0, the required part is correctness plus an
idiomatic generated-client boundary: browser and websocket consumers must see
ordinary JavaScript/TypeScript DTO shapes, not a generic Phon `Value` model.
The previous generic-`Value` shaped TypeScript engine remains useful as the
oracle, dynamic API, and fallback implementation, but it is not the generated
typed bridge and should not be mistaken for the performance path. A
source-specialized TypeScript JIT is only worth carrying when it emits direct
public-object encode/decode code for browser-hot generated DTOs. That work is
prioritized after Rust and Swift native JIT unless consumer benchmarks prove the
TypeScript client path is the bottleneck:

1. Keep interpreter/codegen correctness for every supported schema kind.
2. Keep generated JavaScript source-specialization behavior aligned with the
   same plan-first semantics when it is enabled.
3. Keep the generated-client fast path on direct public JavaScript-shape
   lowering: decoded structs become plain objects, generated enums become the
   codegen discriminated-union shape, sequences become arrays or sets as
   appropriate, schema maps stay `Map`, and `Dynamic` fields alone use the
   dynamic `Value` representation.
4. Keep the generic `Value` engine as the oracle, schema-less dynamic API, and
   implementation for actual `Dynamic` payload fields; do not use it as the
   substrate for ordinary generated Vox TypeScript DTOs.
5. Cover Dodeca browser/devtools payloads that are TypeScript-facing.
6. Cover channel element encode/decode for websocket transports.
7. Preserve generated decoder JIT support for maps, sets, tuple vectors,
   dynamic values, and recursive call-block shapes, and keep unsupported bridge
   surfaces explicit in generated diagnostics.

TypeScript acceptance means browser-facing generated clients can consume the
same fixture corpus over websocket transports, with exact unsupported errors
for surfaces that are bridge-only or platform-specific, while the generated
typed path constructs and consumes public JavaScript shapes directly. A generic
`Value` round trip is acceptable as the oracle, fallback implementation,
benchmark comparison, or true dynamic API path; it is not acceptable as the
public API shape for ordinary generated DTOs. The TypeScript JIT target that
matters is direct public-shape source specialization. It is useful and already
worth benchmarking, but it is not allowed to displace the release-critical order:
generated TypeScript bridge correctness and public-shape APIs first, Rust and
Swift native JIT for the hot paths, then TypeScript JIT polish when
browser-facing benchmark data says it is on the migration path.

### 7. Vox bridge

The Vox bridge is the release-critical integration layer:

1. Generated Rust, Swift, and TypeScript service calls use Phon typed programs
   for method args and responses.
2. The runtime owns engine selection.
3. The runtime exposes only JIT enabled and JIT not enabled as product modes.
4. Descriptor/schema registries compile once and cache per method root.
5. Generated clients and dispatchers produce useful method/path fallback
   reports in diagnostic mode.
6. Non-nested channels bind as capabilities, with item codecs routed through
   Phon.
7. Nested channels are rejected during schema/codegen, not halfway through a
   connection.
8. External values bind through transport-specific capability support.
9. Subjects, sessions, and channel tasks terminate on disconnect/inactivity.
   Vox now has Tracey-backed subject process coverage for Rust TCP, Swift TCP,
   TypeScript TCP, and TypeScript WebSocket disconnect teardown, plus
   Tracey-backed channel close-all coverage for Rust, Swift, and TypeScript,
   Tracey-backed session keepalive teardown coverage for Rust, Swift, and
   TypeScript, and Tracey-backed virtual connection close semantics for Rust,
   Swift, and TypeScript. Remaining work here is broader end-to-end session task
   teardown outside the keepalive path.

Bridge acceptance requires end-to-end generated service tests. Calling the Phon
`Codec<T>` API directly proves the payload engine, but it does not prove Vox
codegen, runtime engine selection, envelope handling, channel binding, or
subject lifecycle.

### 8. Benchmarks

Benchmarks should measure cached steady-state encode/decode, not descriptor
construction noise.

Required benchmark families:

1. Bee `feed` args and response, Rust and Swift.
2. Bee IME/app small-message latency, Swift.
3. Dodeca HTML process input/output with maps, sets, tuple vectors, and
   `Vec<u8>` side payloads where applicable.
4. Dodeca gingembre/host dynamic value calls.
5. Dibs list/create/update/migration payloads with SQL value enums, rows, and
   migration log channel items.
6. Styx recursive `Value` request/response payloads.
7. Stax flamegraph update with recursive `FlameNode`.
8. Channel element encode/decode for representative `Tx<T>` and `Rx<T>` items.

Every benchmark should have:

- interpreter baseline
- JIT enabled result where supported
- fallback report for the benchmarked method root
- enough input size variation to catch "fast only for tiny examples"

Benchmark output should be mechanically comparable between runs. It should
include the fixture name, method root, direction, byte size, value size notes,
engine mode, and fallback status.

## Execution order

This is the intended implementation order for a long-running goal. If new repo
truth contradicts an item, update this roadmap first and keep going from the
updated document.

### Phase 1: Lock the contract

1. Phon spec cleanup landed for stale framing/external/absolute-buffer
   zero-copy and overbroad thunk-only descriptor requirements. Vox-side specs,
   generated-code expectations, and live source have received the first
   matching audit: old retryability/non-retryability rule IDs were replaced by
   outcome/session-interruption and same-peer-terminal schema rules, no stable
   conduit references remain in live source/spec, and stale SHM wording was
   removed from active test configuration.
2. Make nested-channel rejection explicit in spec, codegen, and tests.
3. Metadata cleanup is in place: metadata is a self-describing phon `Value`
   map with well-known key conventions and `#`/`-`/`-#` sigils preserved in the
   key string, not a special sensitive/no-propagate type system.
4. External values are clarified as transport capabilities with optional
   metadata: Phon core owns the schema shape and in-band handle/metadata bytes,
   while attachment channels, lifecycle, flow control, and dereference
   semantics remain transport/RPC responsibilities. Rust, Swift, and TypeScript
   have focused boundary tests for capability-root rejection plus channel item
   and external metadata compat.
5. Rust Tracey validation is clean and fully covered for the current Phon spec;
   Swift and TypeScript Tracey validation is clean and their audited intentional
   holes are captured in the current implementation snapshot.

### Phase 2: Complete the fixture corpus

1. Keep Bee fixtures and benchmarks native-clean, without making live Bee app
   builds part of normal Vox/Phon verification while Bee is actively moving.
2. Treat Dodeca as the largest remaining consumer sweep. Broaden it beyond the
   current ecosystem, template-call, HTML processing, code-execution,
   data-loader, markdown parse/render, image-processing, search-indexing,
   byte-channel, and LSP string-channel generated roots into any additional
   Dodeca roots that become part of the migration gate.
3. Keep Tracey as the bounded generated-service proof target. The current
   status/rule/validation/core-control/full-LSP/update/dashboard/query/config
   mutation roots are covered; only add more Tracey roots when the live
   checkout exposes a newly relevant protocol method.
4. Keep the Rust and Swift Dibs SQL/generated Squel/migration service roots
   native-clean, and keep the TypeScript Dibs generated-service and migration
   fixtures passing through the typed engine/JIT selection path.
5. Keep the expanded Helix `TraceService` fixture in the Rust/TypeScript hot
   benchmark sets and keep the generated Vox method roots green beyond the
   current metrics/verify/subscription/PulseBundle/TraceService aggregate
   roots. Swift benchmark coverage is native-clean; keep the Rust, Swift, and
   TypeScript aggregate fixture parity green while broadening the remaining
   proof paths. Add any optional Hotmeal
   callback-shaped service smoke path if that shape becomes part of the
   migration gate.
6. Keep Styx Swift benchmark coverage native-clean for the recursive value/LSP
   aggregate surfaces.
7. Stax fd/external coverage now has ordinary DTO fixture coverage in Rust,
   Swift, and TypeScript, generated Vox bridge DTO and recursive subscription
   coverage, Phon Rust/TypeScript external diagnostics, a Vox Rust fd-capable
   transport fixture, non-fd transport refusal tests, and Swift/TypeScript
   generated-binding rejection for fd-bearing service surfaces. The macOS
   `record(config, Tx<KdBufBatch>) -> Result<RecordSummary, RecordError>` root
   is now covered by the generated Vox bridge matrix. Remaining work is any
   additional Stax roots that become part of the migration gate.
8. Keep Swift fixture parity green for every shape that Swift Vox must send or
   receive, and keep generated Swift coverage in the Vox bridge matrix as new
   consumer roots become migration-gated rather than adding more hand-written
   fixture descriptors.
9. Keep TypeScript Phon-side fixture parity green for browser/websocket-facing
   and DTO-shaped surfaces, and broaden generated Vox bridge parity when a
   consumer method becomes part of the migration gate.

### Phase 3: Make interpreters authoritative

1. Ensure Rust, Swift, and TypeScript interpreters pass the complete fixture
   corpus.
2. Keep malformed-input tests for set/map uniqueness, dynamic values, recursive
   values, channel roots, and external roots in the interpreter/oracle path; add
   new hostile vectors only when a new migration-gated fixture family exposes an
   untested failure mode.
3. Keep the generated 28-case compatibility corpus green across Rust, Swift,
   and TypeScript. It now includes field changes, enum variant changes, nested
   containers, recursive/dynamic values, channel item schemas, and external
   metadata schemas; add new versioned vectors only when a migration-gated
   fixture exposes a new compatibility shape.
4. Keep same-schema fixtures on the compatibility-plan path.

Phases 4 through 6 are release gates after interpreter correctness, but they
are not equally urgent performance gates. Rust native JIT and Swift native JIT
are priority 1 because they cover the server, engine, and Swift-app hot paths.
Generated TypeScript bridge correctness and public JavaScript-shape APIs remain
part of the compatibility gate. TypeScript source-specialized JIT is a measured
follow-up tier for browser-hot generated DTOs, not a parallel requirement to
Rust/Swift native JIT. It should be justified by the benchmarks already called
out in this roadmap rather than by importing the Rust/Swift descriptor-memory
model or routing ordinary generated DTOs through generic `Value`.

### Phase 4: Bring Rust JIT to ecosystem coverage

1. Preserve Bee native-clean status.
2. Preserve native coverage for maps, sets, tuples, tuple vectors, dynamic
   values, recursive blocks, and landed compat-only decode ops, while explicitly
   reporting any newly unsupported op.
3. Promote Dodeca hot roots from fallback-reported to native-clean where they
   affect real performance.
4. Ensure public JIT-enabled runtime paths never panic on unsupported shapes.

### Phase 5: Bring Swift JIT to ecosystem coverage

1. Preserve Bee native-clean status.
2. Preserve the landed Swift native-clean coverage for managed set elements,
   Dodeca maps, tuple vectors, dynamic values, recursion, and map shapes beyond
   the native string-keyed baseline.
3. Keep generated Swift Vox code routed through runtime-selected typed engines.
4. Keep Swift channel payload benchmarks native-clean for the representative
   Dodeca, Dibs, Helix, and Tracey item roots.

### Phase 6: Finish TypeScript bridge/public shapes, then measured JIT

1. Keep TypeScript interpreter/codegen passing the browser-facing fixture corpus.
2. Route generated Rust, Swift, and TypeScript Vox args/responses/envelopes
   through Phon typed programs.
3. Keep generated Vox TypeScript clients and dispatchers on direct
   JavaScript-shape lowering for their hot typed DTO path, with `Value` reserved
   for true `Dynamic` fields and dynamic APIs.
4. Keep the TypeScript direct public-shape typed JIT as an optimization target
   for browser-facing/generated Vox consumers, but do not let it outrank the
   Rust and Swift native JIT work unless TypeScript benchmark data shows that
   client-side encode/decode is blocking an actual migration.
5. Bind non-nested channels as capabilities and route stream items through
   Phon.
6. Ensure subjects and channel tasks die on disconnect and inactivity. Subject
   process teardown is covered in Vox by `hosted.subject.lifecycle`, and
   channel close-all teardown is covered by `rpc.channel.connection-closure`;
   session keepalive teardown is covered by `session.keepalive`, and Swift and
   TypeScript caller liveness teardown is covered by
   `rpc.caller.liveness.*`. Keep the remaining focus on broader end-to-end
   session task teardown outside keepalive.
7. Add external transport capability handling and diagnostics.

### Phase 7: Benchmark and gate Vox 1.0 compatibility

1. Add ecosystem benchmark families.
2. Record interpreter baseline, JIT enabled result, and fallback status.
3. Include TypeScript public-shape benchmarks for browser-facing generated DTOs:
   direct-shape JIT result plus the generic `Value` oracle/fallback for
   comparison.
4. Run Tracey coverage as a triage check: implement or annotate live
   roadmap-relevant holes, delete or reword dead legacy rules, and keep
   non-shipped language/transport surfaces out of the 1.0 blocker set.
5. Run the fixture corpus with JIT enabled and JIT not enabled.
6. Treat the roadmap as complete only when the Vox bridge, fixtures, live
   Tracey requirements, and benchmarks all agree.

## Acceptance milestones

### Milestone 0: Ecosystem matrix checked in

This roadmap exists and identifies the real consumer shapes that define the Vox
1.0 compatibility surface.

### Milestone 1: Fixture corpus

The repository has checked-in fixtures for Bee, Dodeca, Dibs, Styx, Stax,
Helix, Hotmeal, and Tracey migration shapes. The fixtures can be audited without
building those consumers.

### Milestone 2: Interpreter correctness

Rust, Swift, and TypeScript interpreters pass conformance and compatibility
tests for every shape in the fixture corpus, including maps, sets, tuples,
dynamic values, recursive values, channels, and externals where applicable.

### Milestone 3: Generated Vox bridge

Generated Vox Rust, Swift, and TypeScript clients/dispatchers route args,
responses, envelopes, and channel items through Phon typed programs. Both
runtime modes pass the fixture corpus. The TypeScript generated bridge exposes
and consumes the codegen JavaScript shapes directly; the generic `Value` model
appears only for actual `Dynamic` fields and dynamic/schema-less APIs. A
generated TypeScript bridge that is only correct through generic `Value` is an
oracle/fallback, not this milestone.

### Milestone 4: Bee remains native-clean

Bee hot roots remain native-JIT clean in Rust and Swift after the wider bridge
work lands.

### Milestone 5: Dodeca native coverage

Dodeca fixtures covering maps, sets, tuple vectors, bytes, dynamic values, and
non-nested channels either run native-clean on the prioritized hot roots or
produce precise fallback reports for intentionally deferred subtrees.

### Milestone 6: Recursive and external coverage

Styx recursive `Value` and Stax recursive `FlameNode` pass interpreter and JIT
oracle tests. Stax `vox::Fd` is represented as an external transport capability
with explicit platform/transport diagnostics.

### Milestone 7: Compat coverage

Versioned fixtures prove plan-first compatibility for field changes, enum
variant changes, nested containers, dynamic-to-dynamic values, recursive
references, channel element schemas, and external metadata.

### Milestone 8: Benchmarks

Rust benchmark entry points cover Bee plus the Dodeca, Dibs, Styx, Stax,
Helix, Hotmeal, Tracey migration, and channel payload families, including Dibs
Squel and migration service roots. Swift benchmarks cover Bee, Dodeca, Dibs,
Styx, Stax, Helix, and representative channel payloads, including Dodeca
image/search roots, Dodeca byte/string items, Dibs migration service roots and
migration logs, Helix pulse availability, and Tracey data updates. Results
separate interpreter baseline, JIT enabled path, and fallback-report status.
Browser-facing TypeScript benchmarks exist to decide whether direct-shape source
specialization needs to move up for a specific generated-client workload; they
distinguish direct public-shape JIT from the generic `Value` oracle/fallback
path for the broad Helix aggregate and Dodeca image/search roots.

### Milestone 9: Tracey coverage

Tracey validates, and no Vox 1.0 blocker rule remains uncovered or untested for
the live compatibility, execution, JIT, channel, external, generated-bridge, and
shipped-transport surface. Legacy retry/idempotency, stable-conduit,
zero-copy/shared-memory, and non-shipped language/transport rules are either
deleted, reworded into current semantics, or explicitly kept out of the 1.0
gate.

### Milestone 10: Vox 1.0 compatibility gate

The next Vox can serve the checked-in consumer fixture corpus with JIT enabled
and JIT not enabled, without retry/idempotency/stable-conduit/zero-copy
remnants, without subject leaks, and with precise diagnostics for any
intentionally unsupported surface. TypeScript generated clients use ordinary
public JavaScript/TypeScript shapes for ordinary DTOs, with `Value` reserved for
real dynamic payloads and
schema-less/dynamic APIs; TypeScript source-specialized JIT is accepted when it
is direct-shape and benchmark-justified, not because TypeScript is expected to
mirror the Rust/Swift descriptor-memory engines.

## Suggested goal wording

Use this as the objective for a long-running implementation goal:

> Finish the Vox ecosystem compatibility roadmap in
> `docs/content/vox-ecosystem-compat-roadmap.md`: turn the real Vox consumer
> surfaces from Bee, Dodeca, Dibs, Styx, Stax, Helix, Hotmeal, and Tracey
> migration into checked-in fixtures; make the interpreter and generated Vox
> bridges handle those fixtures through plan-based Phon typed programs; keep
> Rust JIT and Swift JIT native-clean for the hot paths; keep TypeScript
> engine/codegen on ordinary public shapes with source-specialized JIT only
> where benchmarks justify it; keep only the two runtime modes, reject nested
> channels, preserve subject teardown, add benchmarks for the hot families, and
> use Tracey annotations/tests to prove the roadmap's spec rules are implemented
> and verified.
