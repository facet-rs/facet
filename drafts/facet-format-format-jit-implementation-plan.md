# facet-format: Format-Specific JIT (Implementation Plan)

This plan implements Tier-2 “format JIT” as specified in:
- `drafts/facet-format-format-jit-motivation.md`
- `drafts/facet-format-format-jit-reference-design.md`

The plan is written to be incremental, benchmark-driven, and safe: every step either preserves existing behavior or adds a strictly optional fast-path with a clear fallback.

## Guiding constraints

- Do **not** break existing `FormatParser` implementors: all new trait items must have defaults and/or be `cfg(feature = "jit")`.
- The Tier-2 fast path must be **opt-in** and only used when the parser can provide a complete slice.
- Correctness first: no UB; no leaking owned strings; no desynchronizing parser state.
- Respect repo requirements: use `cargo nextest run` for tests, don’t bypass hooks.

## Phase 0: Baseline + measurement harness (do this first)

1. Ensure we can reproduce the problem locally:
   - Run the existing booleans benchmark(s) comparing:
     - `facet-format-json` reflection path
     - `facet-format` Tier-1 shape JIT
     - `serde_json`
2. Keep a stable measurement method for inner-loop instruction counts:
   - `valgrind --tool=callgrind` for instruction deltas
   - optionally `perf`/`samply` later (but callgrind is enough to validate the structural win)

Deliverable:
- A short note in the PR description with “before” numbers for `Vec<bool>` on slice input.

## Phase 1: `facet-format` API + scaffolding for Tier 2

### 1.1 Add a separate opt-in trait: `FormatJitParser`

Files:
- `facet-format/src/jit/format_parser.rs` (new) or `facet-format/src/jit/format.rs` (if you keep Tier-2 types together)

Actions:
- Define `trait FormatJitParser<'de>: FormatParser<'de>` which exposes:
  - `type Format: JitFormat`
  - `fn jit_input(&self) -> Option<&'de [u8]>`
  - `fn jit_pos(&self) -> Option<usize>`
  - `fn jit_set_pos(&mut self, pos: usize)`
  - `fn jit_format(&self) -> Self::Format`
  - `fn jit_error(&self, input: &'de [u8], error_pos: usize, error_code: i32) -> Self::Error`

Design note:
- We do this as a separate trait (instead of “defaults on `FormatParser`”) because associated type defaults are unstable on stable Rust.
- Tier 2 is optional anyway; this makes the capability boundary explicit.

### 1.2 Define Tier-2 `JitFormat` trait and supporting types

Files (new):
- `facet-format/src/jit/format.rs` (or `emitter.rs`; pick one and stick to it)

Actions:
- Define:
  - `JitCursor`
  - `JitStringValue`
  - `JitFormat` trait
  - `NoFormatJit` stub implementation (all methods return “unsupported”)
- Keep the interface minimal but complete for JSON arrays/maps + scalars.

### 1.3 Add shared helpers required by Tier 2

Files:
- `facet-format/src/jit/helpers.rs`
- `facet-format/src/jit/compiler.rs` (helper registration)

Actions:
- Add `jit_drop_owned_string(ptr, len, cap)` helper and register it in the JIT builder.
- (Optional for MVP) Add small shared helpers for bounds checks / byte classification if you decide to call out to Rust instead of emitting IR for some pieces.

### 1.4 Implement the Tier-2 compiler skeleton

Files (new):
- `facet-format/src/jit/format_compiler.rs`

Actions:
- Implement:
  - a new compilation entry point `try_compile_format::<T, P>() -> Option<CompiledFormatDeserializer<T, P>>`
  - a new compiled function ABI (see reference design): `(input_ptr, len, pos, out, scratch) -> isize`
  - centralized error block writing `JitScratch`
  - a minimal shape traversal for **lists of scalars** first (Vec<bool> MVP)
- Reuse existing vec init/push helpers from `facet-format/src/jit/helpers.rs`.

Deliverable:
- Tier-2 compiler can produce a function that parses JSON array syntax *via emitter calls* (even if emitter is still Noop and thus returns unsupported).

### 1.5 Caching for Tier 2

Files:
- `facet-format/src/jit/cache.rs`

Actions:
- Extend the cache key to include tier:
  - easiest: maintain a second cache for Tier 2
  - or extend `CacheKey` from `(T, P)` to `(T, P, tier)` with a small enum/byte

### 1.6 Wire Tier-2 attempt into `jit::try_deserialize`

Files:
- `facet-format/src/jit/mod.rs`

Actions:
- Keep `try_deserialize::<T, P>(parser: &mut P)` as the Tier-1 entry point (shape JIT over events).
- Add a Tier-2 entry point:
  - `try_deserialize_format::<T, P>(parser: &mut P)` where `P: FormatJitParser<'de>`
- Optionally add a policy wrapper:
  - `try_deserialize_with_format_jit::<T, P>(parser: &mut P)` where `P: FormatJitParser<'de>`
  - which attempts Tier 2 first, then Tier 1

Guardrails:
- If `jit_input()` or `jit_pos()` returns `None`, do not attempt Tier 2.
- If the type isn’t Tier-2 compatible, do not attempt Tier 2.

## Phase 2: `facet-format-json` opts in (slice parser only)

### 2.1 Add a JSON format emitter

Files (new, behind a feature):
- `facet-format-json/src/jit/mod.rs`
- `facet-format-json/src/jit/format.rs` (implements `JitFormat` for JSON)
- `facet-format-json/src/jit/helpers.rs` (optional; helpers for string/number parsing)

Actions:
- Implement `JitFormat` for JSON with MVP operations:
  - `emit_skip_ws` (inline IR)
  - `emit_seq_begin`, `emit_seq_is_end`, `emit_seq_next` (inline IR)
  - `emit_parse_bool` (inline IR)
- All other methods can initially return “unsupported” to force fallback for non-MVP shapes.

Important:
- Start by supporting only the subset needed to make `Vec<bool>` fast.
- Add tests that the emitter’s generated control flow accepts valid JSON and rejects invalid separators.

### 2.2 Implement Tier-2 hooks on `JsonParser<'de>`

Files:
- `facet-format-json/src/parser.rs`

Actions:
- Under `cfg(feature = "jit")` (or under a crate feature like `format-jit`), implement `FormatJitParser<'de>` for `JsonParser<'de>`:
  - `type Format = JsonJitFormat`
  - `jit_input`: return `Some(self.input)`
  - `jit_pos`: return `Some(self.current_offset)` *only if safe*
  - `jit_set_pos`: set `self.current_offset = pos` and reset all internal scanning state
  - `jit_error`: create a `JsonError` with a span and an error kind mapped from the code

Safety/consistency rule:
- Tier 2 must be disabled if `JsonParser` has a cached `event_peek` (or any other buffered state that would make the “cursor position” ambiguous).
  - simplest: `jit_pos()` returns `None` when `self.event_peek.is_some()`.

Adapter reset detail:
- Do not create `SliceAdapter::new(&self.input[pos..])` unless you also track a base offset, because spans must remain absolute.
- Prefer adding an adapter method like `SliceAdapter::with_start(input, start_offset)` (or a `reset_to_offset` method) so that:
  - `input` remains the full original slice
  - `window_start/window_end` are set to `pos`
  - scanner pos is reset

### 2.3 Expose the module and feature wiring

Files:
- `facet-format-json/src/lib.rs`
- `facet-format-json/Cargo.toml`

Actions:
- Add a feature (e.g. `jit`) that:
  - enables `facet-format/jit`
  - pulls in any JSON JIT helper dependencies (if needed)
- Ensure default builds do not drag in Cranelift unless explicitly enabled.

## Phase 3: MVP milestone — `Vec<bool>` matches expectation

### 3.1 Make the `Vec<bool>` benchmark hit Tier 2

Actions:
- Verify that `facet-format-json` deserialization of `Vec<bool>` uses Tier 2:
  - callgrind instruction count should drop from ~7B → near the “direct parsing” range (sub-1B).
  - wall-clock should improve materially.

### 3.2 Add correctness tests for Tier 2

Where:
- `facet-format-json/tests/*` or `facet-format/tests/*` (prefer format-specific tests for JSON rules)

Tests to include:
- `Vec<bool>` parses correctly with whitespace, e.g. `[ true , false ]`
- Parser cursor updates correctly (parse a value, then parse another value from the remaining input)
- Error path produces a usable `JsonError` span for malformed arrays

### 3.3 Make sure Tier 1 and reflection are unaffected

Actions:
- Run the existing suite and ensure no behavior changed:
  - `cargo nextest run`
  - run the `facet-format-suite` cases for JSON

## Phase 4: Expand Tier-2 coverage (incremental, benchmark-driven)

Expand in this order (each step should come with at least one test + a benchmark check):

1. `Vec<i64>`, `Vec<u64>`, `Vec<f64>` (same array protocol, new scalar parsing)
2. `Vec<String>`:
   - start helper-based for escape handling
   - keep the “no escapes” fast path borrowed + clone
3. Simple structs:
   - `{ "field": <scalar>, ... }` with field matching
   - unknown field handling via `emit_skip_value` (helper-based is fine)
4. Nested lists/structs
5. `Option<T>` with null handling for JSON

## Phase 5: Correctness hardening and maintenance

### 5.1 Required-field validation (UB fix)

This should be treated as a must-fix before relying on Tier 2 broadly:
- Add a compiled bitset to track which required fields are set
- Validate before returning success

Apply the fix to Tier 1 too if possible, so both JIT tiers are correct.

### 5.2 Key handling without leaks

When parsing map keys:
- if the key is decoded into an owned buffer (escapes), and it is not moved into output,
  it must be dropped via `jit_drop_owned_string`.

Add targeted tests with escaped keys to ensure no leaks and correct matching.

### 5.3 Improve Tier-2 errors

Once performance is proven, invest in:
- mapping error codes to helpful `JsonErrorKind`
- storing `error_pos` precisely (byte offset)
- optionally capturing “expected token class” for richer messages

## Phase 6: Optional Tier-1 “fat trimming” (separate track)

This work is independent and can be done later:
- add `FormatParser::next_raw_event()` / `peek_raw_event()` as an optional fast path for Tier 1,
  avoiding `ParseEvent → RawEvent` conversion
- reduce parser layers for the reflection path (bytes → events more directly)

These optimizations make Tier 1 better, but they are not a substitute for Tier 2 on hot scalar loops.

## Deliverables checklist (MVP)

- `facet-format`:
  - `FormatJitParser` trait
  - `JitFormat` trait + `NoFormatJit`
  - Tier-2 compiler + cache + dispatch
  - `jit_drop_owned_string` helper
- `facet-format-json`:
  - JSON `JitFormat` implementation sufficient for `Vec<bool>`
  - `JsonParser` implements `FormatJitParser` and correctly resets internal state on `jit_set_pos`
  - tests verifying correctness and cursor consistency
- Bench validation:
  - callgrind instruction count shows large reduction for `Vec<bool>`

## Suggested PR slicing

To keep review manageable:
1. PR 1: `facet-format` scaffolding (`FormatJitParser` + `JitFormat` + compiler skeleton + cache + dispatch), but no JSON implementation (Tier 2 never used).
2. PR 2: `facet-format-json` opt-in + `Vec<bool>` MVP + tests + benchmark report.
3. PR 3+: expand coverage (numbers/strings/structs) with incremental benchmarks.
