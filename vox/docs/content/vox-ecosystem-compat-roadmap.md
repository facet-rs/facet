++
title = "Vox ecosystem compatibility roadmap"
description = "Frozen Vox 1.0 compatibility gate for the checked-in Phon-backed migration fixture corpus."
weight = 20
++

This is the frozen Vox 1.0 compatibility gate for the checked-in ecosystem
migration fixture corpus.

The goal is not to declare every future Vox consumer complete. The goal is to
keep this corpus green across the shipped Rust, Swift, and TypeScript Phon
engine paths, with plan-based typed programs as the only payload translation
path.

## Gate Boundary

The frozen corpus is the set of roots currently checked into
`spec/spec-proto/src/lib.rs`, implemented by the Rust, Swift, and TypeScript
subjects, and generated into the Rust, Swift, and TypeScript Vox bridge code.
It covers migration surfaces from:

- Bee
- Dodeca
- Dibs
- Styx
- Stax
- Helix
- Hotmeal
- Tracey

Newly discovered consumer roots are follow-up work unless this section is
amended. The gate is closed only when the frozen corpus is green in both
directions for the shipped language/transport combinations that support the
surface.

## Runtime Invariants

These are non-negotiable for this gate:

- There are exactly two runtime modes: JIT enabled and JIT not enabled.
- Same-schema identity still goes through the compatibility plan path.
- Generated Vox Rust, Swift, and TypeScript bridges use Phon typed programs
  for payload encode/decode.
- Rust and Swift hot roots either lower to native JIT cleanly or report a
  precise fallback reason.
- TypeScript generated DTOs stay on ordinary public JavaScript/TypeScript
  shapes. Public Rust `Result<T, E>` is represented as
  `{ ok: true, value: T } | { ok: false, error: E }`.
- Nested channels are rejected.
- External/fd values are transport-owned.
- Hosted subjects terminate on disconnect or inactivity.

Do not reintroduce retry/idempotency policy, stable conduit, zero-copy shared
memory, nested channel support, or transport-independent fd values as part of
this gate.

## Required Roots

The checked-in corpus currently includes these high-value groups:

- Dodeca template, HTML processing, code execution, data loading, markdown,
  image, search, asset-processing, small-cell service, devtools, editor, and
  channel-shaped roots.
- Dibs schema/list/get/create/update/delete/migration roots.
- Styx value, LSP, and host-callback roots.
- Stax flamegraph, live-update, Linux broker-control, and macOS record roots.
- Hotmeal live-reload and patch-result roots.
- Helix stream metrics, verify evidence, pulse subscription, and pulse bundle
  roots.
- Tracey status, rule, dashboard, validation, LSP, VFS, and update roots.
- Bee-shaped roots already represented by the checked-in fixture corpus.

The Dodeca small-cell service root is part of the frozen gate. It exercises
nested `Result`, maps, bytes, chars, enums, task progress DTOs, terminal/build
responses, link-check diagnostics, server status, and command DTOs.

## Source Of Truth

Edit source definitions and regenerate generated bridge files:

- Service and DTO source: `spec/spec-proto/src/lib.rs`
- Rust subject samples and handlers: `rust/subject-rust/src/lib.rs` and
  `rust/subject-rust/src/main.rs`
- Swift subject samples and handlers:
  `swift/subject/Sources/subject-swift/Subject.swift`
- TypeScript subject samples and handlers: `typescript/subject/subject.ts`
- Matrix source: `xtask/src/main.rs`

Do not manually edit generated files except as regeneration output:

- `spec/spec-tests/tests/spec_matrix.rs`
- `swift/subject/Sources/subject-swift/Testbed.swift`
- `typescript/generated/testbed.generated.ts`

Regenerate after changing the service surface:

```sh
cargo xtask generate-spec-matrix
cargo xtask codegen --typescript --swift
cargo fmt --all
```

## Focused Closure Tests

Before running a filtered nextest slice, list it first with the same filter.

The Dodeca small-cell root must pass this slice:

```sh
cargo nextest list -p spec-tests -E 'test(dodeca_small_cell_services_fixture)'
cargo nextest run -p spec-tests -E 'test(dodeca_small_cell_services_fixture)'
```

The broader frozen ecosystem corpus is tracked with this slice:

```sh
cargo nextest list -p spec-tests -E 'test(ecosystem_bridge) | test(dodeca_) | test(dibs_) | test(styx_) | test(stax_) | test(hotmeal_) | test(helix_) | test(tracey_)'
cargo nextest run -p spec-tests --no-fail-fast -E 'test(ecosystem_bridge) | test(dodeca_) | test(dibs_) | test(styx_) | test(stax_) | test(hotmeal_) | test(helix_) | test(tracey_)'
```

The language subjects and TypeScript Phon engine must also build/check:

```sh
cargo build -p subject-rust
swift build --package-path swift/subject -c release --product subject-swift
pnpm --filter @bearcove/vox-subject run check
pnpm --filter @bearcove/phon-engine run check
pnpm --filter @bearcove/phon-engine exec vitest run src/typed.test.ts
```

## Benchmark Entry Points

Closure requires correctness first. Once the focused and broad corpus tests are
green, record at least the benchmark entry points that exercise the hot paths:

```sh
cargo run --release -p rust-examples --example bench_client -- --addr ffi:// --workload gnarly --payload-sizes 16,1024 --in-flights 1,16 --count 1000 --json
cargo run --release -p rust-examples --example bench_runner -- --addr 127.0.0.1:64977 --payload-sizes 16,1024 --in-flights 1,16 --count 1000 --json -- --workload gnarly
```

`bench_client --addr ffi://` exercises the Rust in-process FFI path.
`bench_runner` starts an external subject over TCP; replace `64977` with a
free local TCP port. The local-socket Swift benchmark path is not part of this
gate until session establishment is fixed there.

Record median numbers in `notes/perf-baselines.md` when the numbers are meant
to become a baseline.

## Tracey Validation

Closure also requires Tracey to parse and validate every configured
implementation:

```sh
tracey query . status
tracey query . validate
tracey query . uncovered --spec-impl vox/swift
```

Swift uncovered rules outside the gate boundary, such as browser-only
transports or optional observability/debug surfaces, are not blockers unless
the gate boundary or required roots above are amended to include them.
