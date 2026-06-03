# Handoff: PHON JIT Hybrid Context

## Completed
- Committed `3389818 Make phon JIT front door honest`.
- Added `phon::api::Codec<T>`, `api::encode`, and `api::decode` in `rust/phon/src/lib.rs`.
- Fixed stale `Lowered` vs `MemProgram` native-JIT call sites in `rust/phon/src/derive.rs`, `rust/phon/examples/seq_bench.rs`, and `rust/phon/examples/gnarly_bench.rs`.

## Active Work

### Origin
The user started with:

> Hey, you're gonna be in charge of making the Rust jit for PHON actually real and stop lying. Some agents is actually preparing a note for that, but you can already start looking around.

After the first front-door guardrail commit, the user challenged the whole-program fallback:

> Right now, you're doing something where if the JIT does not support an operation, it falls back to the interpreter. Why?

They then clarified the desired direction:

> Is there a hybrid path where it jits as much as possible, and for the things it cannot, it just falls back to helpers?

The answer is yes: the intended PHON JIT architecture is hybrid. Whole-program interpreter fallback in `3389818` is only a defensive guard around today's panic-shaped native compile API, not the desired final model.

### The Problem
`phon_jit::native::{NativeDecode, NativeEncode}::compile` currently takes a flat `&MemProgram` and panics for unsupported ops such as:

- `MemOp::Result`
- `MemOp::Dynamic`
- `MemOp::Opaque`
- `MemOp::CallBlock`
- encode-side compat-only ops (`SkipWire`, `Default`)

The new `phon::api::Codec<T>` avoids those panics by only entering native compile when a whole lowered program is known to be supported, otherwise running the typed interpreter. That makes the public API honest, but it is intentionally too coarse.

The real target is a hybrid JIT executor:

- JIT direct code for supported ops and subtrees.
- Lower ABI-unstable or type-erased ops to helper-call stencils.
- Eventually record/report what compiled direct, what compiled via helpers, and what fell back to an interpreter/helper subtree.
- Keep public runtime/product mode as only JIT enabled vs JIT not enabled; do not expose interpreter/native/helper as public modes.

### Current State
- Branch: `main`
- Commit: `3389818 Make phon JIT front door honest`
- Status after that commit was clean and `main...origin/main [ahead 9]`.
- Verification run after the commit:
  - `cargo check --all-features --all-targets --message-format=short`
  - `cargo nextest run --workspace --all-features --no-fail-fast`
  - Result: 153 tests passed.

The current front-door API lives in `rust/phon/src/lib.rs`:

- `api::Codec<T>::new()` derives, builds a `Registry`, lowers with `typed::lower_typed`, and conditionally compiles native encode/decode.
- The helper predicates `native_decode_supported`, `native_encode_supported`, `decode_program_supported`, and `encode_program_supported` are whole-program guards.
- Test-only methods `decode_uses_native_jit` and `encode_uses_native_jit` exist only to assert current behavior without creating a public backend-mode API.

### Technical Context
The existing native JIT already has the shape needed for hybrid compilation:

- `Option` already uses helper/thunk entries such as `is_some`, `get_value`, `init_some`, and `init_none`.
- `Map` already uses helper/thunk entries for length, init, iteration, insert, and iterator teardown.
- Compat decode already has native stencils for `Default` and `SkipWire`.

`Result` should follow that same helper-call pattern, not whole-program fallback. `phon_ir::ResultOp` and `ResultThunks` already exist. The native decode shape should be:

1. Read the wire arm index.
2. If it is `ok_wire_index`, decode the `ok` payload sub-chain into scratch, then call `init_ok`.
3. If it is `err_wire_index`, decode the `err` payload sub-chain into scratch, then call `init_err`.
4. Otherwise return the same bad-variant decode error as the interpreter.

The native encode shape should be:

1. Call `is_ok`.
2. Write `ok_wire_index` or `err_wire_index`.
3. Call `get_ok` or `get_err`.
4. Run the matching payload sub-chain at that returned pointer.

Do not probe or assume `Result` layout for now. The user explicitly said `Result` is hard because the ABI is not stable and probing it is not the task right now; use helpers/thunks.

For `Bytes`, `String`, borrowed leaves, `Vec`, `&str`, and `&[u8]`, do not frame helper calls as the ideal long-term path. The user corrected this: the JIT is supposed to probe concrete runtime layout facts by constructing values at runtime and inspecting layout, then use those facts for direct code. The important distinction:

- `Result` helper calls are the correctness-preserving design for now.
- Bytes/string/sequence helper calls are an implementation compromise until a probe-derived layout facts layer exists.

That probe layer should:

- Build concrete sentinel values for the exact reflected carrier type.
- Inspect initialized object bytes to discover pointer/len/cap or pointer/len positions.
- Validate the facts across enough cases to avoid ambiguous guesses.
- Record direct facts in a form lowering can use.
- Fall back to helpers only when probing cannot prove facts.

The user expects performance benchmarks to naturally surface helper tax later:

> all of that is gonna naturally surface when we start doing performance benchmarks again.

So do not prematurely overfit this before benchmark work resumes, but do not lose the architectural direction.

### Success Criteria
1. Native compile becomes fallible or report-producing instead of panic-producing for unsupported ops.
2. `Result` gets native encode/decode helper-call stencils using existing `ResultThunks`.
3. JIT enabled mode becomes hybrid internally: direct native code where possible, helper-call stencils where appropriate, and subtree fallback only where truly necessary.
4. The front door remains honest: no panic-shaped unsupported JIT path, and no claim that JIT is active when a program ran entirely through the interpreter.
5. Public API does not grow product modes like `Interpreter`, `Hybrid`, or `Strict`; those are implementation/reporting concepts.
6. Benchmark/reporting work can identify helper-tax hotspots without guessing.

### Files to Touch
- `rust/phon-jit/src/native.rs` - add Result encode/decode native support and replace panic-only unsupported-op handling with fallible/reporting compile.
- `rust/phon-jit/stencils/stencils.rs` - add Result decode/encode stencils, likely mirroring Option with two payload sub-chains.
- `rust/phon-jit/build.rs` / generated stencil extraction path - include any new stencil symbols.
- `rust/phon/src/lib.rs` - keep front-door guard/update once native compile reports support instead of needing whole-program prefiltering.
- `rust/phon/src/derive.rs` and `rust/phon-jit/src/native.rs` tests - add oracle tests comparing Result JIT against typed interpreter.

### Decisions Made
- The committed whole-program fallback is a temporary safety guard only.
- The desired architecture is hybrid JIT internally.
- `Result` should use helpers/thunks, not layout probing.
- Bytes/string/borrowed/Vec carriers should eventually use probe-derived direct layout facts where possible.
- Public runtime mode should remain JIT enabled vs JIT not enabled; JIT coverage details belong in diagnostics/reporting, not as product modes.

### What NOT to Do
- Do not keep treating whole-program fallback as the final JIT architecture.
- Do not probe or assume `Result` layout in this pass.
- Do not expose interpreter/native/helper/hybrid as public product modes.
- Do not bypass the typed lowering/plan path with a direct codec shortcut.
- Do not disable tests or suppress compiler/clippy warnings to get through JIT work.
- Do not revert `3389818`; build from it.

### Blockers/Gotchas
- `cargo fmt --all` wants to rewrap an unrelated old test hunk in `rust/phon-engine/src/typed.rs`. Avoid committing unrelated formatting churn unless doing a formatting commit intentionally.
- `phon_jit::native` currently compiles only `&MemProgram`, not a full `Lowered`, so recursion via `CallBlock` is not native-supported yet.
- Low-level native compile still panics for unsupported ops. The public `phon::api` avoids this, but direct users of `phon_jit::native` can still hit it.
- Existing all-feature tests are green; if they break, collect the full set with nextest `--no-fail-fast`.

## Bootstrap
```bash
cd /Users/amos/phon/rust
cargo check --all-features --all-targets --message-format=short
cargo nextest run --workspace --all-features --no-fail-fast
rg -n "Result\\(|ResultOp|ResultThunks|interpreter-only|panic!\\(\"phon-jit" phon-jit phon-ir phon-engine phon
```
