+++
title = "Implementation strategy"
weight = 45
+++

# Build Vix by climbing the ratchet

This chapter is an implementation charter, not a second specification. The
[language specification](/vix/spec/language), the
[runtime specification](/vix/spec/machine/), the [Rodin specification](/rodin),
and the checked-in ratchet and corpus define the system. This chapter says how
to turn those authorities into one implementation without allowing a
transitional evaluator, an expedient test, or several parallel branches to
quietly define another Vix.

The governing strategy is simple:

> Keep one authoritative implementation lane on the new VIR/runtime path and
> climb the ratchet in order. Let executable pressure discover the next missing
> mechanism. Settle a semantic ambiguity at its normative source when it first
> becomes load-bearing. Never obtain a green rung by weakening the rung, the
> foundation contract, or the architecture.

At the adoption of this charter, rung 001 establishes the production spine:
source is parsed into the generated AST, checked and lowered to VIR, partitioned
into islands, lowered to Weavy, and executed by the new runtime. Rung 002 is the
first red target. The ratchet runner, rather than this sentence, is the source
of truth for the current score.

## What success means

There are four coupled outcomes:

1. The ratchet advances consecutively through the production compiler and
   runtime. A green rung above a red rung does not increase the score.
2. The implementation preserves the foundation contract from rung 001:
   typed diagnostics, source maps, causal traces, counters, receipts, chaos-run
   agreement, and the required specification-coverage gates.
3. The old evaluator is frozen, displaced from every production call site, and
   ultimately deleted. It is neither an oracle nor a library of code to port.
4. Documentation converges on the small authoritative surface named in the Vix
   README: README, book, language specification, runtime specification, Rodin
   specification, rungs, and corpus. Historical notes do not survive as shadow
   authorities.

Rung 100 means the language and solver described by the book exist. Rungs
101–140 certify the quality properties that keep that implementation usable:
code-edit early cutoff, modules, diagnostics, differential guards,
parallelism, edge semantics, trust, and scale. Passing rung 100 is not a reason
to weaken or abandon the remaining ladder.

## One authoritative implementation lane

The central implementation has one writer at a time. That writer owns the
coupled spine:

- typed AST to graph VIR;
- graph VIR to partitioned VIR and explicit island edges;
- VIR to architecture-neutral Weavy programs;
- runtime identity, memo nomination and validation, store interaction,
  scheduling, and receipts;
- the ratchet runner and production-facing diagnostics and observability.

These components are one architectural knot while their interfaces are still
being discovered. Separate worktrees prevent textual conflicts, but they do not
prevent two implementations from assigning different meanings to an island,
an edge, a recipe, or a receipt. Until a boundary is explicit in the runtime
specification and exercised by the ratchet, implementation ownership stays
serial.

The main writer should use the strongest useful reasoning mode for this lane.
Parallel agents are explicit Paseo agents with bounded assignments; invisible
built-in delegation is not part of the workflow.

## The rung loop

For each next red rung:

1. Run it through the production ratchet path and capture the exact failure.
2. Locate the first missing or incorrect layer: parsing, name resolution,
   checking, graph VIR, partitioning, Weavy lowering, runtime execution,
   primitive boundary, persistence, or harness interpretation.
3. Implement the smallest *general mechanism* that gives the rung its specified
   meaning. Do not add a rung-number dispatch, fixture-specific result, AST
   evaluator hidden inside the runner, Rust-side bypass, or special case that
   evades VIR.
4. Keep the work observable. A lowerer falloff must produce a typed diagnostic;
   a scheduling or identity decision must be visible in the trace and counters;
   an effect must produce the receipt evidence required by the runtime spec.
5. Run the focused rung, then the consecutive prefix with `cargo nextest run
   --release` by default. Treat the first release artifact build separately
   from warm test execution; debug-mode timings are not performance evidence.
   Use the debug profile only when a debugger or debug-only instrumentation
   requires it. At band boundaries, run the foundation's chaos, performance,
   and Dodeca coverage gates.
6. Run the relevant production-path integration tests and Clippy with warnings
   denied. Public runtime or Weavy vocabulary changes also require their
   consumers to compile; a crate-local green result is not sufficient evidence.
7. Commit each coherent substrate increment. Preserve a negative attempt when
   it reveals a real language, lowering, or architecture blocker; do not revert
   it into invisibility.
8. Continue immediately with the next red rung. Passing one rung or one band is
   a checkpoint, not completion of this charter.

If a rung contradicts the book or specification, stop changing implementation
behavior and report the contradiction with exact anchors. A rung may be changed
only as an explicit design decision. If the rung instead exposes an unanswered
question, resolve the question at its normative source and then resume the same
rung.

## The first band

The first implementation sequence is rungs 002–010:

- integer literals and arithmetic;
- bindings and strings;
- functions and application;
- tuples and positional projection;
- records and field projection;
- enums, payloads, and matching;
- record spread as a fresh value;
- ambient structural equality;
- ambient total comparison and `Ordering`.

This band is deliberately pure. It forces the value model, control/data wiring,
aggregate representation, source mapping, and intrinsic vocabulary to become
real before effects, placement, or persistent storage add more dimensions. A
mechanism introduced for an early rung should be designed as the foundation
that later demand, memo, and island semantics will use, not as a disposable
mini-interpreter.

After rung 010, keep climbing in the checked-in order. Review boundaries at the
existing ratchet bands, not at invented milestones. In particular, the later
bands progressively force:

- control flow, destructuring, collections, streams, closures, recursion, and
  scale;
- demand wiring, partial dependencies, laziness, memo behavior, snapshots, and
  typed decoding;
- exec, trees, fetch, receipts, persistent reuse, early cutoff, and flakiness
  detection;
- Rodin's value model, propagation, search, learning, features, oracle
  agreement, and warm restart;
- code-edit incrementality, modules, diagnostics, differential modes,
  observable parallelism, typed edge failures, store distrust, and scale.

## Specification work follows executable pressure

Broad speculative specification work is no longer the main lane. The current
language and runtime architecture are sufficiently definite to execute the
early ladder. From here, specification work has three triggers:

- the next rung depends on a semantic choice the specifications do not make;
- implementation reveals that two existing rules cannot both hold;
- a production-path proof requires an ABI, lifecycle, identity, or
  observability contract that is presently only implied.

When triggered, make the ruling in the canonical language, runtime, or Rodin
specification, update explanatory book prose where necessary, add or repair
traceability, and resume the blocked rung. Do not append a resolution below
contradictory prose: correct the original normative location.

Known later decision areas include typed results for command grammars with
multiple accepted exit statuses; canonical Tree semantics; the relationship
between `TreeHash` and store `NodeHash`; and collision semantics for
`Tree::union`. They must be settled before their first dependent rung, but they
do not block arithmetic, aggregates, or the other early pure-language rungs.

Questions about islands, codata, safepoints, the Rust primitive API, basic
scheduling, debugging, profiling, counters, traces, inspection, diagnostics,
and VIR/Weavy/native artifact caches should now be sharpened against their
implementation and acceptance evidence. The architecture chapter establishes
their common shape; the ratchet determines when each shape must become
operational.

## The old evaluator is frozen

`engine.rs` and the other frozen evaluator machinery receive no new semantics,
features, or architecture. The new implementation must not call into them, copy
their control structure, or derive expected results from them at assertion
time.

During the transition they may remain buildable and may provide historical
comparison data where explicitly useful. Such a comparison is evidence, not an
oracle. Absolute ratchet and corpus contracts are the authority.

Retirement is dependency-driven rather than ceremonial:

1. Inventory every production caller, test caller, and compatibility shim.
2. Move production entry points to the new compiler/runtime path as their
   required surface becomes green.
3. Replace parity-derived tests with literal ratchet or corpus contracts.
4. Remove dead slices once no live caller depends on them.
5. Delete the remaining frozen evaluator when default execution, testing, and
   code-intelligence paths no longer reach it and the relevant ratchet prefix is
   green.

Deletion is not a porting project. If the new runtime needs a capability that
only the old evaluator has, derive the correct mechanism from the current
specification and acceptance surface.

## Documentation convergence

The documentation collapse has two stages.

First, perform a read-only census. Every design note, analysis, critic report,
and gap file is mapped to one of:

- material already present in an authoritative surface;
- a still-valid decision that must be integrated at its original normative
  location;
- an open question that must be resolved before a named rung;
- historical argument or superseded design that should be deleted.

Second, one isolated worktree owns the mechanical consolidation. It integrates
surviving material into README, book, language spec, runtime spec, Rodin spec,
rungs, or corpus; repairs links and Dodeca coverage; and deletes the retired
notes. It does not make semantic decisions independently of the main lane.

The consolidation should be folded at a reviewed checkpoint so it does not
race with normative edits made while passing a rung.

## Deliberate parallelism

Parallel work is useful when it produces independent evidence or consumes an
already-stable interface. It is harmful when it creates another semantic
authority.

Work that may proceed beside the main writer immediately:

- read-only adversarial review of a landed rung band;
- the documentation census;
- an old-evaluator caller and deletion census;
- exact source/spec traceability audits;
- focused performance or observability analysis that does not rewrite the
  implementation under measurement.

Read-only agents may inspect the main checkout. Any agent that edits receives a
dedicated Paseo worktree, a disjoint file/ownership boundary, explicit
acceptance criteria, and a requirement to commit its result. Changes are
reviewed and folded serially into the authoritative branch.

Implementation can split only after a fence is both specified and exercised.
Likely later lanes include:

- store and persistence adapters behind a settled runtime store contract;
- placement and executor integration behind settled capability and primitive
  APIs;
- command-grammar result interpretation behind settled exit and readiness
  semantics;
- code-intelligence consumers of stable compiler queries and source maps;
- debugging and inspection interfaces over stable typed trace events.

Even then, graph VIR, island partitioning, runtime identity, and the scheduler
retain clear owners. A gatekeeper reviews each proposed fold for semantic and
traceability compatibility before it reaches the main branch.

## Observability is part of correctness

Behavior-only green is insufficient. From rung 001 onward, the implementation
must make its decisions mechanically inspectable:

- typed diagnostics with source spans and lowering context;
- causal trace links from demand through island and Weavy program to source;
- counters for demand transitions, memo verdicts, hashing, store traffic,
  scheduling, pollpoints, compilation tiers, and effects;
- receipts and read-sets for effectful and persistent reuse;
- inspectable VIR, partitioned VIR, and Weavy artifacts;
- chaos-run evidence that killing and replay do not change semantic results.

Use tracing and typed dump formats rather than `println!`, `eprintln!`, or
hand-written JSON. Performance claims require profiler or counter evidence;
the island model exists to make the cost of demand bookkeeping explicit rather
than invisible.

## Working state and completion

The authoritative branch should remain recoverable through frequent coherent
commits. Do not rewrite or discard negative evidence. Do not weaken warnings,
tests, or coverage to keep the branch green. Unrelated concurrent changes are
preserved and worked around through ownership boundaries and worktrees.

This charter is complete only when all of the following hold:

- the current ratchet through rung 146 passes consecutively through the
  production path and satisfies `FOUNDATION.md`;
- the real-program corpus uses that same compiler/runtime path;
- the old evaluator and parity scaffolding have no live callers and are
  deleted;
- the documentation has collapsed to the authoritative surfaces, with no live
  contradictory note presented as current design;
- the relevant Dodeca traceability, integration, consumer-compile, Clippy, and
  chaos gates pass.

Until then, the default action after a green checkpoint is to inspect and climb
the next red rung.

## Persistent agent goal

The following goal is deliberately larger than one rung or one band. Its
completion condition prevents an unattended implementation session from
treating an intermediate green checkpoint as the end of the work.

```text
Implement Vix according to vix/docs/content/implementation-strategy.md. Starting
at the first red rung after the current consecutive green prefix, climb the
ratchet strictly in order through the production source -> generated AST -> VIR
-> islands -> Weavy -> new runtime path.

Never edit a rung or FOUNDATION.md merely to make it pass. Never call into,
copy, extend, or use vix/src/engine.rs or another frozen evaluator as the new
implementation. Never add rung-number, fixture-specific, AST-evaluator, or
Rust-side bypasses. For each red rung, capture the exact production-path
failure, implement the smallest general mechanism that gives it its specified
meaning, preserve typed diagnostics, source maps, traces, counters, receipts,
and chaos/replay semantics from rung 001, run the focused and consecutive-prefix
gates, commit the coherent increment, and immediately continue to the next red
rung.

When executable pressure exposes an ambiguity or contradiction, resolve it at
the original normative location in the language, runtime, or Rodin spec, repair
the book and Dodeca traceability as needed, and resume the same rung. Do not
append a shadow resolution over contradictory prose. If changing a rung is
actually required, stop that implementation path and report the exact conflict
for an explicit design decision.

Keep the old evaluator frozen and retire its production callers and
parity-derived tests as new-path coverage makes them obsolete. Progress the
read-only documentation and old-engine censuses when they do not conflict with
the main rung lane; consolidate documentation only into README, book, language
spec, runtime spec, Rodin spec, rungs, or corpus. Use one authoritative main
writer. Use Paseo agents only for bounded read-only review/census work or for
stable, disjoint edit lanes in dedicated worktrees, with serial review and
folding.

Do not mark this goal complete because one rung or band passes, because the
remaining work is difficult, or because the token budget is low. After every
green checkpoint, inspect and climb the next red rung. If a genuine blocker
requires user design authority, preserve and report exact evidence and continue
any independent in-scope census, observability, documentation-convergence, or
retirement work that does not bypass the blocker.

The goal is complete only when rungs 001-146 pass consecutively through the
production path under FOUNDATION.md; the real-program corpus uses that same
path; the frozen evaluator and parity scaffolding have no live callers and are
deleted; the documentation is collapsed to the authoritative surfaces without
live contradictions; and the relevant Dodeca coverage, production integration,
consumer compile, Clippy-with-warnings-denied, performance, and chaos gates
pass.
```
