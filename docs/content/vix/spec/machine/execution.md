+++
title = "machine: execution"
+++

Execution authority: weavy owns running code; the machine consumes lowering
artifacts and never second-guesses the substrate.

r[machine.execution.weavy-owns-mode]

[SETTLED] Weavy owns the interp/JIT decision as the single authority. The
machine holds no Interp/Jit enum, no private cfg, no mode plumbing — it
hands weavy a program and receives execution.

r[machine.execution.jit-always]

[SETTLED, scope OPEN] The `jit` cargo feature is abolished: the JIT path
builds unconditionally and every `#[cfg(feature = "jit")]` gate dies (the
cfg-gated `Op` import that broke dependency-position builds is the incident
this rule prevents). OPEN sub-decision: whether phon's own engine `jit`
feature is included or exempt.

r[machine.execution.facts-precomputed]

[SETTLED] Properties of lowered code — effect stats, native-load bits,
declared comparators, tail-loop shapes — are computed at lowering and cached
on the artifact. Runtime opcode scans on hot paths are a missing analysis
phase. Weavy's IR analysis (`ProgramStats`/`EffectStats`) is the existing
mechanism; the machine reads artifact facts, never re-derives them.

r[machine.execution.no-pure-hostcalls]

[DESIGN] Pure computation — map, array, option, string, version, comparison,
boolean operations — is weavy vocabulary, lowered, never host FFI. The
machine's host surface contains zero pure-computation calls. (Census class A
= 32 current violations; the vocabulary itself is specified in `lang.*`,
this rule is the machine-side ban.) Classification is by actual effect, not
name: glob over an already-concrete tree is pure.

r[machine.execution.comparator-direct]

[DESIGN] Semantic-comparator invocation is a direct weavy call — vix-native,
enforced at lowering with a loud diagnostic if a comparator isn't — not a
full demand with per-pair allocation.

r[machine.execution.lowering-diagnostics]

[DESIGN] When a shape falls off a fast path — a syntactically tail-ish call
lowering through INVOKE, a native-eligible access going through a hostcall —
lowering emits a diagnostic naming why. Silent performance cliffs are
banned (the fixpoint that became 293 demands was legal, silent, and
catastrophic).
