# facet-format: Format-Specific JIT (Motivation)

## Context

`facet-format` exists to centralize *type semantics* (Facet shapes, flatten, solver integration, rename rules, etc.) while letting each wire format focus on *surface syntax* (JSON punctuation, XML mapping, etc.). Today, `facet-format` already achieves this by having each format implement `FormatParser<'de>` which produces a `ParseEvent<'de>` stream consumed by the shared deserializer (`FormatDeserializer`).

Separately, there is an existing Cranelift JIT effort in `facet-format` (`facet-format/src/jit/*`). This “shape JIT” compiles deserializers from `Shape` and still consumes `ParseEvent` via a `FormatParser` vtable.

This document motivates a second tier: **format-specific JIT** (aka “format JIT”), where a format crate (starting with `facet-format-json`) can provide Cranelift-building facilities so the compiled deserializer parses bytes directly, bypassing the event abstraction.

## Problem: Shape JIT is dominated by parser abstraction overhead

On a hot microbenchmark (1024 booleans in a JSON array), we observed:

- `facet-format` (shape JIT over `ParseEvent`) spends ~**7.2B** instructions.
- `serde_json` spends ~**1.17B** instructions.
- `facet-json` (direct byte parsing JIT) spends ~**0.46B** instructions.

The key finding: **the generated machine code is not the bottleneck**. In the shape-JIT case, callgrind shows the JIT code itself is a small fraction of the total; the bulk is spent in:

```
bytes
  ↓
SliceAdapter::next_token
  ↓
Scanner::next_token
  ↓
JsonParser::produce_event
  ↓
ParseEvent → RawEvent conversion
  ↓
JIT code (writes fields / pushes vec items)
```

Each boolean element forces multiple layers of function calls, enum matches, span bookkeeping, and “token → event → raw event” representation conversions before the JIT even sees the value.

This architecture is intentional and valuable for the *general* path:
- Streaming support (NeedMore / windowed scanning)
- Zero-copy borrowing where possible
- A single format-agnostic deserializer across formats

But for the “full slice available upfront” case, this layering fundamentally prevents serde-competitive performance for simple, tight loops like `Vec<bool>`.

## Why we can’t “just optimize the layers” enough

We can trim some fat (e.g. eliminate `ParseEvent → RawEvent` conversion, reduce tokenization overhead), but the shape JIT remains structurally coupled to:

1. “Pull an event” (`next_event`) per syntactic unit
2. Convert it into a generic representation
3. Branch on tags in compiled code

For a format like JSON, the fastest implementations fuse these concerns:
- whitespace skip
- delimiter handling (`[`, `]`, `,`, `{`, `}`, `:`)
- scalar parsing
- container loop control

Serde does this; `facet-json`’s `jitson_*` does this.

With the current event pipeline, `facet-format` is paying a “virtualized parser tax” per element that dominates runtime on simple values.

## Non-goal: abandoning `facet-format` for per-format JIT compilers

A straightforward way to get performance is “Option A”: every format has its own JIT compiler (like `facet-json`).

That is specifically not what we want:
- It duplicates shape traversal logic and semantics across formats.
- It makes `facet-format` irrelevant for the performance path.
- It’s harder to keep flatten/solver/rename behavior consistent.

Instead, the objective is:

> Keep `facet-format` as the owner of “what this Rust shape means”, but let format crates supply “how this shape is parsed from bytes” in the JIT path.

## The desired end-state: Two-tier JIT

We want two tiers of JIT deserialization:

### Tier 1: Shape JIT (existing)
- Compiles from `Shape`
- Consumes the generic `ParseEvent` stream via `FormatParser`
- Works with streaming parsers and all formats
- Performance improves over pure reflection, but still bounded by abstraction overhead

### Tier 2: Format JIT (new)
- Still compiles from the same `Shape` traversal logic (in `facet-format`)
- The format crate provides a *format-specific emitter* used during compilation
- The generated code parses from `(input_ptr, len, pos)` directly
- Requires “whole slice available upfront” (initially)
- Aims for serde-competitive performance on hot paths (arrays of scalars, structs with scalar fields)

Tier 2 is an *optional acceleration path*, not a replacement. If the parser cannot provide a complete slice, we fall back to Tier 1 / reflection.

## Implementation reality check: associated type defaults are unstable on stable Rust

Rust stable (including Rust 1.91.1 used by this repo) does **not** support *associated type defaults* in traits. That means we cannot express “every `FormatParser` has `type FormatJit = NoFormatJit` by default” in a way that compiles on stable.

Practically, this pushes Tier-2 capability to a **separate opt-in trait** implemented only by parsers that support format JIT. This aligns with the goal anyway: Tier 2 is optional and should be explicitly enabled by a format/parser pair.

## Design constraints and guiding principles

### 1) Opt-in and capability-based
- Format JIT should not change behavior unless explicitly supported by a parser type.
- A parser can decide at runtime to disable format JIT (e.g., if it has buffered `peek_event` state).

### 2) “Surface syntax” stays in the format crate
For JSON arrays, the format-specific logic is not “parse bool”; it is the loop protocol:
- Expect `[` / `]`
- Skip whitespace
- Recognize `,` separators
- Decide end vs element

The same applies to maps/objects (`{}` and `:` in JSON, indentation in YAML, `=` in TOML, etc.).

Therefore the integration point must be **container protocols and cursor operations**, not merely scalar parsers.

### 3) `facet-format` stays responsible for semantics
`facet-format` must continue to own:
- traversal of `Shape` (struct fields, enum layouts, lists/maps/options)
- memory writes and field offsets
- vec init/push and nested allocation strategy
- caching of compiled functions
- correctness of flatten/solver rules (within supported subset)

Format crates should not need to re-implement “what a `Vec<T>` is” or “how Option is represented in memory”.

### 4) Initial focus: complete slices, not streaming
Per explicit scope choice: it is acceptable if format JIT only works when the complete input slice is available.

Streaming JIT (growable buffer / `AsyncRead`) can be layered later on top of the same interface, but is out of scope for the initial implementation.

## What success looks like

### Performance targets (initial)
- `Vec<bool>` / `Vec<i64>` / `Vec<f64>` should approach `facet-json` JIT instruction counts (within ~2×) on slice inputs.
- For structs with scalar fields, the format JIT path should remove the “token → event” overhead and become dominated by actual parsing and memory writes.

### Product targets
- No behavior change for existing non-JIT paths.
- Easy opt-in for `facet-format-json` slice parser.
- Clear fallbacks when unsupported (Tier 2 → Tier 1 → reflection).
- A design that can be extended to YAML/TOML later (even if implementation starts with JSON).

## Why this is worth doing (summary)

Without format JIT, `facet-format` risks being:
- excellent for correctness and shared semantics,
- but permanently uncompetitive with serde on the highest-volume hot paths.

With format JIT, we can:
- preserve the value proposition of `facet-format` (shared semantics across formats),
- while allowing format crates to provide the last-mile byte-level parsing needed for speed.
