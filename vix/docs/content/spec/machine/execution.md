+++
title = "Execution"
weight = 6
+++

Execution authority: weavy owns running code; the machine consumes lowering
artifacts and never second-guesses the substrate.

> r[machine.execution.weavy-owns-mode]
>
> [SETTLED] Weavy owns the interp/JIT decision as the single authority. The
> machine holds no Interp/Jit enum, no private cfg, no mode plumbing — it
> hands Weavy a program and receives execution or typed fault facts. Vix and
> every other consumer do not select, implement, or bypass the execution lane
> or checked-execution mode.

> r[machine.execution.jit-single-feature]
>
> [SETTLED] There is exactly ONE jit feature in the ecosystem: weavy's. vix,
> phon, and every other weavy consumer carry no jit feature of their own — the
> per-crate `#[cfg(feature = "jit")]` gates that caused the dependency-position
> `Op` build break are abolished. Weavy's `jit` feature is the master switch
> (`jit_active = feature_on ∧ target_supports_copy_patch`): OFF means off for
> good, nothing downstream can turn JIT on against it; ON means on only where
> the platform supports executable memory. Mechanism:
>
> - Weavy's build script computes `jit_active` from `CARGO_FEATURE_JIT`,
>   `CARGO_CFG_TARGET_OS`, and `CARGO_CFG_TARGET_ARCH`. The predicate is the
>   copy-patch backend's explicit supported `(OS, architecture)` matrix;
>   W^X-locked targets (iOS/tvOS/watchOS/visionOS) and unsupported
>   architectures force it off even when the feature is on. It emits both a
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

> r[machine.execution.verified-admission]
>
> [SETTLED] A raw Weavy `Program` is inert architecture-neutral construction
> data. Every interpreter spawn, JIT compilation, and native execution entry
> point accepts only an opaque `VerifiedProgram` produced by one always-on Weavy
> verifier. There is no unchecked public constructor and no public consumer
> path that can run or compile raw program bytes.
>
> The lowering artifact carries the proof material the verifier needs. In
> particular, the architecture-neutral `Program` includes a compact frame
> contract/manifest: declared regions with offset, width, alignment, and
> machine kind/schema witness sufficient to verify every op's reads and writes,
> entry bindings, argument copies, indirect-call slots/contracts, and return
> regions. This is not full Vix type reflection and not runtime tagging in the
> fast lane; it is proof material cached with the lowering artifact. Frame
> layout byte bounds alone are insufficient.
>
> Declared frame regions do not overlap. Several mutually exclusive control-flow
> arms may write the same declared result region; that is one region with
> several verified writers, not an overlay. Verification checks each op's
> accesses against the declared region rather than imposing a single-writer
> rule. A future slot-coalescing allocator must introduce and prove an explicit
> alias contract before its programs are admissible.
>
> Compact discriminated values add selector-correlated shape facts to that
> manifest. The contract names the enum shape, its compact width and selector
> word, every valid variant, and each variant field's shape and shared-payload
> offset. A verified `EnumConstruct` zeroes the complete compact region, writes
> one statically valid selector, and copies exactly the fields declared for that
> variant. A verified `EnumProjectChecked` validates the live selector before
> copying the declared field; a mismatch is a typed `TaskFault`, never a raw
> union read. Static dominance analysis may later discharge that check, but the
> unchecked lowering pattern is not itself proof.
>
> A selector-correlated payload is not semantically compared as a raw frame
> word. Equality first proves equal valid selectors, dispatches to that variant,
> and compares only its declared fields using each field type's semantic
> equality. Raw `EqI64` and `NeI64` apply only to scalar leaves and selectors;
> handle-backed leaves compare through the checked referent-identity or
> byte-comparison operation (`machine.identity.handle-by-referent`). Calls,
> returns, arrays, and whole-value copies preserve the same structural shape
> identity recursively. Canonical inactive payload bytes serve representation
> and identity invariants only; they do not authorize handle-integer equality.
>
> Verification proves all static safety obligations before any lane executes:
> statically named function, call, and jump targets; function fallthrough;
> immediate and opcode shape; frame offset, width, and alignment using checked
> arithmetic; argument and return copies against the frame contract; declared
> inline regions; host and await requirements; and vocabulary/ABI support for
> the selected platform. For an indirect callee, verification proves the slot's
> machine kind and call contract, not the runtime function id or concrete
> target. The typed access/execution membrane checks both that the dynamic id is
> in range and that the selected function's declared call-contract identity
> equals the contract verified at the call site; either mismatch faults
> identically in every lane.
>
> Host and await table lengths supplied at drive time are checked against the
> verified program requirements before native code can enter. Weavy reports
> malformed programs as its own typed `ProgramError` and dynamic invariant
> violations as its own typed `TaskFault`, carrying function/op/contract facts.
> Vix wraps those values without stringification into `MachineError` and adds
> source attribution; Weavy does not depend on Vix types. The result is reported
> the same way for every lane, never as a panic, undefined behavior, silent
> truncation, or Vix `Failure`.
>
> Fast native stencils may omit only checks already discharged by
> `VerifiedProgram`. The safe interpreter remains the behavioral oracle. The
> checked/native differential gates compare results, step counts, traces, and
> typed faults; a shared helper agreeing with itself is insufficient evidence
> for shadow invariants.

> r[machine.execution.checked-access-membrane]
>
> [SETTLED] Dynamic value and aggregate access crosses one typed Weavy access
> membrane shared by interpreter and native lanes. It checks handle provenance
> and namespace, task generation and ownership, payload schema and element
> width, initialization, bounds, and allocation arithmetic. The membrane
> distinguishes malformed, invalid, uninitialized, and out-of-range statuses;
> it never collapses them to `Option`, a single `present` bit, a zero/default
> value, or a silently discarded write.
>
> Authoritative dense-array construction, store, and load operations transfer
> exactly one complete element. The op's element-width witness is the complete
> element width and must equal the well-formed payload header before any bytes
> are copied; projection into fields happens afterward through ordinary static
> frame projection. A partial-region width is not an element-width witness.
> Payload classification first validates the structural envelope — tag,
> tag-specific header, positive width, checked total size, and exact length —
> then compares schema and element width. Invalid structure is
> `MalformedPayload`; only a structurally valid array of another schema is
> `SchemaMismatch`, and only a structurally valid matching-schema array of
> another element width is `WidthMismatch`.
>
> `VerifiedProgram` proves static program shape; it does not prove dynamic
> aggregate contents or handle provenance. Fast native stencils therefore keep
> using the access membrane for dynamic aggregate, value, and indirect function
> checks even when their frame/op checks were statically discharged.
>
> Access statuses and task faults occupy two distinct planes. An op with a
> declared status slot writes the closed access status into the program's
> language-outcome plane; only a status whose lowering explicitly defines a
> language outcome may become a Vix `Failure`. A dynamic invariant violation on
> an op with no status slot terminates the task through the machine-fault plane
> as a typed `TaskFault`. In particular, residency for `CompareValueBytes` is a
> dynamic provenance fact: an unresident handle faults through the membrane and
> is never an `expect`, panic, zero value, or comparison result.
>
> Borrowed value-memory descriptors retain their borrow lifetime in Weavy's safe
> public API. Raw pointer/length descriptors exist only inside the private native
> ABI and are materialized for one drive while the borrow is live; safe code
> cannot construct a dangling value-memory table.
>
> A verified host call names its readable and writable frame regions and receives
> a region-scoped accessor that maintains initialization and kind shadows. It
> never receives an unrestricted `&mut [u8]` view of the whole frame. Programs
> using the frozen evaluator's whole-frame host ABI are not verified-admissible;
> that compatibility surface retires with the frozen evaluator rather than
> weakening the new execution contract.
>
> Weavy owns opt-in checked execution with independent shadow metadata: redzones,
> poison, generation tags, dynamic kind/schema shadows, and whatever additional
> lane-local witnesses are needed. This is an audit/instrumentation policy, not
> a fourth consumer-selected execution lane: the ordinary execution API has no
> `Interp`/`Jit`/`Checked` mode argument. A consumer may ask Weavy's separate
> diagnostic API for an audit fact, or Weavy may instrument the selected lane
> under its own policy; neither lets the consumer select the semantic executor.
> Effectful host operations are shadowed inline or audited against a recorded
> host transcript — an audit never repeats an external effect merely to compare
> engines.
>
> A checked-execution violation reports a typed `TaskFault` naming the function,
> PC, op, and violated contract. Checked execution cannot affect Vix value
> identity, memo entries, receipts, or program semantics; it observes the same
> program through stricter Weavy-owned instrumentation and cannot publish an
> alternative result.

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
> [SETTLED] Lowering injects full edge safepoints at demand boundaries and
> cheap interior pollpoints at loop backedges/long operations
> (`machine.safepoint.two-classes`). Pollpoints are patchable no-ops or
> predictable checks in copy-patch lanes and perform no identity, memo, receipt,
> or scheduler work until armed. Both classes are shared infrastructure for
> kill/migration barriers (`machine.scheduler.replay-is-semantics`), performance
> counters (`machine.obs.counters`), future GC, profiling, and debugging. This is possible
> because Weavy verifies the architecture-neutral programs it runs and owns the
> resulting execution lane — the capability rustc cannot offer arbitrary code —
> and it is a reason lowering is LOAD-BEARING SUBSTRATE for the monorepo's
> projects (vix, phon, snark, fable, the generated deserializers), to be
> engineered with that seriousness: safepoint placement is a specified,
> perf-gated lowering decision, not an
> implementation afterthought.

> r[machine.execution.lowering-diagnostics]
>
> [DESIGN] When a shape falls off a fast path — a syntactically tail-ish call
> lowering through INVOKE, a native-eligible access going through a hostcall —
> lowering emits a diagnostic naming why. Silent performance cliffs are
> banned (the fixpoint that became 293 demands was legal, silent, and
> catastrophic).
