# Inline String Testing Plan

Status: _living document_. Update as features land or invariants change.

## Goals

- De-risk the new inline-string representation inside `Value` (`TypeTag::InlineString`) despite heavy unsafe usage.
- Provide deterministic, property-based, and fuzz coverage so every behavior change is caught.
- Ensure tooling (Miri, sanitizers, fuzzers) is wired into CI so regressions fail fast.

## Test Layers & Tasks

### 1. Deterministic / Unit Tests

- [x] Add table-driven tests for `VString::can_inline`, `len`, `is_inline`, and `as_bytes` covering lengths `0..=INLINE_LEN_MAX` plus an oversized case (see `src/string.rs`).
- [x] Add regression tests ensuring `Value::from(s)` returns an inline representation when `s.len() <= INLINE_LEN_MAX`, and flips to heap otherwise.
- [x] Add layout tests validating `Value::is_inline_string()` logic (`src/value.rs`) by peeking at `ptr_usize()` and tag bits.
- [x] Add tests for mutation APIs (`as_string_mut`, append/truncate) that transition between inline and heap while preserving UTF-8 and ownership rules.
- [x] Extend `VArray`/`VObject` tests to include inline strings in containers, asserting cloning/dropping leaves them valid.

### 2. Integration / Behavioral Tests

- [x] Round-trip serialization tests (format + deserialize) for inline strings to ensure no hidden heap allocations.
- [x] Add cross-crate conversions (e.g., `serde_json`, `facet-pretty`) verifying inline strings survive conversions without corruption.
- [x] Add 32-bit target check (`cargo nextest run -p facet-value --target i686-unknown-linux-gnu`) in CI/docs via `just test-i686`.

### 3. Property Testing (Bolero / Proptest)

- [x] Introduce Bolero-based property tests asserting `Value::from(s)` round-trips to `s` for randomly generated UTF-8 strings, tagging whether representation stays inline.
- [x] Add shrinking-aware property covering mutation sequences that may cross inline/heap boundary (append/remove/clear).
- [x] Build a model-vs-implementation property where a pure Rust enum mirrors `Value`; random ops (clone, drop, container insert) must match.

### 4. Fuzzing Enhancements

- [x] Extend `fuzz/fuzz_targets/fuzz_value.rs` to track inline-string density and assert `is_inline_string()` correctness after operations.
- [x] Add a focused `fuzz_inline_string` target that mutates raw bit patterns plus API calls (clone, drop, conversion).
- [x] Configure CI smoke runs (≤60s) for each fuzz target and document how to reproduce locally (see `just fuzz-smoke-*` + DEVELOP.md).

### 5. Tooling & Unsafe-Focused Checks

- [x] Add `cargo +nightly miri test -p facet-value --features alloc` workflow; document local invocation (`just miri`).
- [x] Add sanitizer jobs (`address`, optionally `memory`/`leak`) for `facet-value` (`just asan-facet-value`).
- [x] Record guidance for ad-hoc heap/memory profilers (Valgrind/heaptrack) for long fuzz sessions.

### 6. Differential / Snapshot Tests

- [x] Differential tests against `serde_json::Value` for inline strings (convert → JSON → convert back).
- [x] Snapshot `INLINE_LEN_MAX`, `INLINE_DATA_OFFSET`, etc., via `static_assertions` to prevent silent ABI changes.

### 7. Documentation & Maintainer Notes

- [x] Add run instructions (Miri, sanitizers, fuzz, Bolero) to `DEVELOP.md` or crate README.
- [x] Track completed tasks by checking boxes here when PRs land.

## Immediate Next Steps

1. Land deterministic unit tests that exercise inline encoding boundaries.
2. Scaffold Bolero test module (behind feature flag) to start property checks even before inline-string feature is finalized.
3. Prepare CI job definitions (even if initially optional) so tooling can be toggled on once inline support stabilizes.
