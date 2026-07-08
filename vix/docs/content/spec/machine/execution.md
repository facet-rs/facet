+++
title = "machine: execution"
+++

Execution authority: weavy owns running code; the machine consumes lowering
artifacts and never second-guesses the substrate.

> r[machine.execution.weavy-owns-mode]
>
> [SETTLED] Weavy owns the interp/JIT decision as the single authority. The
> machine holds no Interp/Jit enum, no private cfg, no mode plumbing — it
> hands weavy a program and receives execution.

> r[machine.execution.jit-single-feature]
>
> [SETTLED] There is exactly ONE jit feature in the ecosystem: weavy's. vix,
> phon, and every other weavy consumer carry no jit feature of their own — the
> per-crate `#[cfg(feature = "jit")]` gates that caused the dependency-position
> `Op` build break are abolished. Weavy's `jit` feature is the master switch
> (`jit_active = feature_on ∧ platform_supports`): OFF means off for good,
> nothing downstream can turn JIT on against it; ON means on only where the
> platform supports executable memory. Mechanism:
>
> - Weavy's build script computes `jit_active` from `CARGO_FEATURE_JIT` and
>   `CARGO_CFG_TARGET_OS` (W^X-locked targets — iOS/tvOS/watchOS/visionOS —
>   force it off even when the feature is on) and emits both a
>   `weavy_jit_active` rustc-cfg (gating weavy's own runtime executor + stencil
>   extraction) and `cargo::metadata=jit=1` (via `links = "weavy"`), so every
>   direct dependent's build script reads `DEP_WEAVY_JIT` and gates its own
>   per-crate stencil extraction on the same single decision.
> - The JIT API surface is always compiled; only the copy-patch runtime
>   executor and the build-time stencil extraction are behind `jit_active`.
>   Consumers compile unconditionally and check
>   `NATIVE_COPY_PATCH_AVAILABLE` at runtime.
>
> This means an iOS build falls to the interpreter by construction — no W+X
> code compiled, no per-crate feature, no `default-features` dance at the app
> root — while a desktop/server build JITs. (Rationale:
> compiling the copy-patch machinery is build-time waste, not runtime W+X, so
> the feature is about waste and single-source-of-truth, not a hard W^X
> blocker.)

> r[machine.execution.facts-precomputed]
>
> [SETTLED] Properties of lowered code — effect stats, native-load bits,
> declared comparators, tail-loop shapes — are computed at lowering and cached
> on the artifact. Runtime opcode scans on hot paths are a missing analysis
> phase. Weavy's IR analysis (`ProgramStats`/`EffectStats`) is the existing
> mechanism; the machine reads artifact facts, never re-derives them.

> r[machine.execution.no-pure-hostcalls]
>
> [DESIGN] Pure computation — map, array, option, string, version, comparison,
> boolean operations — is weavy vocabulary, lowered, never host FFI. The
> machine's host surface contains zero pure-computation calls. (Census class A
> = 32 current violations; the vocabulary itself is specified in `lang.*`,
> this rule is the machine-side ban.) Classification is by actual effect, not
> name: glob over an already-concrete tree is pure.

> r[machine.execution.comparator-direct]
>
> [DESIGN] Semantic comparators ARE the memo's semantic tier: their invocation
> is a demand and can recurse through the memo (`machine.memo.three-tier-reuse`
> — this is the preserved, correct behavior). The performance rule is about the
> comparator BODY, not its dispatch: it must lower to native weavy ops with no
> per-pair allocation, enforced at lowering with a loud diagnostic if a
> comparator isn't vix-native. (The earlier "direct call, not a demand"
> phrasing wrongly denied the demand to state a perf property.)

> r[machine.execution.safepoints]
>
> [SETTLED] Lowering injects a general SAFEPOINT mechanism at demand
> boundaries and loop back-edges — patchable no-ops in the copy-patch lanes,
> patched into real checks only when a consumer is pending. Safepoints are
> shared infrastructure multiplexed across consumers: kill/migration barriers
> (`machine.scheduler.replay-is-semantics`), performance counters
> (`machine.obs.counters`), future GC points, and debugging. This is possible
> because weavy owns lowering for every program it runs — the capability rustc
> cannot offer arbitrary code — and it is a reason lowering is LOAD-BEARING
> SUBSTRATE for the monorepo's projects (vix, phon, snark, fable, the
> generated deserializers), to be engineered with that seriousness: safepoint
> placement is a specified, perf-gated lowering decision, not an
> implementation afterthought.

> r[machine.execution.lowering-diagnostics]
>
> [DESIGN] When a shape falls off a fast path — a syntactically tail-ish call
> lowering through INVOKE, a native-eligible access going through a hostcall —
> lowering emits a diagnostic naming why. Silent performance cliffs are
> banned (the fixpoint that became 293 demands was legal, silent, and
> catastrophic).
