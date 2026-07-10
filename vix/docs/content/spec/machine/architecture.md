+++
title = "Compiler and runtime architecture"
weight = 1
+++

The runtime consumes a Vix-specific IR, partitions it into islands, lowers
islands to Weavy, and evaluates only externally demanded roots. This chapter is
the architectural spine joining the language and the lower-level rules.

## Vix IR

> r[machine.ir.vix-level]
>
> The typed AST lowers first to Vix IR (VIR), never directly to host calls or
> Weavy. VIR is architecture-neutral, fully typed, source-mapped, and explicit
> about value dependencies, projections, failure edges, codata edges, effects,
> and placement. It contains no Rust callback identities and no machine layout.

VIR has two related forms. Graph VIR is SSA/ANF-like demand wiring: nodes are
pure operations, projections, branches, calls, effects, codata producers or
consumers, and placed subgraphs. Partitioned VIR groups those nodes into islands
and makes every inter-island edge explicit. The partition is an artifact, not
semantic identity.

Every VIR node carries a stable node id within the compiled closure, its result
schema, source span, effect facts, and dependency operands. Closure capture is an
explicit typed record. Lowering cannot consult a global host-function number.

> r[machine.ir.inspectable]
>
> Graph VIR, partitioned VIR, Weavy IR, and the source-map chain between them
> have deterministic text and typed inspection forms. A legal shape that cannot
> lower carries a typed diagnostic naming the node, source span, attempted fast
> path, and reason; silent falloff is forbidden.

## Islands

> r[machine.island.partition]
>
> An island is a compiler-selected region of eager pure computation. Island
> interiors perform no memo lookup, identity publication, receipt recording, or
> scheduler request. Edges are the only semantic demand boundaries.

Mandatory cuts occur where demand is observably partial: effects, an unproven
branch or projection, codata, placement, or a value deliberately exposed for
independent memoization. Cost cuts MAY add or remove edges for sharing,
parallelism, code size, or recomputation economics. Because every graph node has
semantic recipe and value identity independent of the partition, repartitioning
may change reuse rate but never correctness or identity.

> r[machine.island.molten-private]
>
> Molten state is a private single-owner builder inside one island. It has no
> public identity and may not cross an island or placement boundary. It freezes
> once into an immutable value before publication.

Codata is not molten. A codata producer owns an append-only publication log;
each semantic element or byte range becomes immutable before it is exposed and
is replayable by key or offset. This immutable-publication property is why
codata may cross boundaries while molten state may not.

## Safepoints and preemption

> r[machine.safepoint.two-classes]
>
> Full edge safepoints and interior pollpoints are distinct. An edge safepoint
> may interact with demand, memo, receipts, placement, and scheduling. An
> interior pollpoint is a cheap compiler-inserted check at loop backedges and
> bounded long operations; it has no value identity and performs no semantic
> machinery unless armed.

Pollpoints serve cancellation, cooperative preemption, debugging, profiling,
counter flushing, and future memory management. Interpreter and JIT execute the
same placements. The inactive JIT form is a patchable no-op or predictable
branch; arming a pollpoint may yield the task but cannot publish a partial molten
value.

## Lowering and bytecode caches

> r[machine.lowering.cache]
>
> VIR, architecture-neutral Weavy programs, and optional native code are tiers
> of the single lowering-artifact cache. They are not memo entries and are not
> semantic values.

Graph VIR is keyed by `(closure identity, vixc IR epoch)`. Partitioned VIR adds
the partitioner/cost-model epoch. Weavy bytecode adds the Weavy vocabulary and
primitive ABI epochs. Native code additionally includes target architecture,
CPU feature contract, JIT epoch, and instrumentation mode. Any tier may be
evicted and rebuilt. The canonical closure AST remains the semantic and audit
authority; an auditor can re-lower it and compare artifacts.

Placed executors receive partitioned/lowered architecture-neutral artifacts,
source maps, primitive ABI requirements, capture identities, and grants. They
need Weavy and the registered primitives, not vixc. Native code is produced and
cached locally where allowed.

## Runtime state and basic scheduling

> r[machine.runtime.state-machines]
>
> Demand and task state are explicit passive data. A demand moves through
> `Absent -> Queued -> Running -> Ready | Failed`; running state owns one effect
> ticket at most and a waiter set. A task moves through
> `Runnable -> Running -> Parked -> Runnable | Completed | Discarded`.

One task walks a path through ready island edges inline. A fresh dependency
continues in the same task unless a deliberate parallel split is admitted. A
join or pending effect parks the task. `join` and `publish` are atomic scheduler
mutations. The initial implementation MAY have one deterministic worker, but it
must use these same state transitions, event vocabulary, admission interface,
and kill/replay contract; concurrency cannot require a second scheduler.

## Causal observability

> r[machine.inspect.source-map-chain]
>
> Every executing instruction is attributable through native PC or interpreter
> PC -> Weavy op -> VIR node -> island -> Vix source span -> demand chain.
> Anonymous JIT regions or un-attributable host time are runtime defects.

Traces are causal, not globally ordered. Each evaluator emits a monotonic local
sequence and links demand, task, effect, codata, and placement spans with typed
ids. Cross-host links carry parent ids. Transport arrival order never becomes a
semantic ordering claim.

> r[machine.inspect.live]
>
> A typed inspection service exposes versioned views of the demand map, task
> states, memo candidates and verdicts, receipts, store residency, capabilities,
> placement decisions, VIR/Weavy artifacts, counters, and recent events. Views
> are borrowed, paged, or copy-on-write; inspection never clones the store or
> forces an undemanded value.

Debugging stops tasks only at edge safepoints or armed pollpoints. Breakpoints
may target source spans, VIR nodes, demand transitions, primitive events, or
diagnostic codes. Inspecting a value uses the typed pull API and records no
program receipt. A distributed debug barrier is control-plane state and cannot
change a value.

Profiling exports native JIT mappings and logical Vix stacks to Stax. Standing
counters include demand transitions; exact/projection/semantic memo verdicts;
store dedup, freeze, eviction and rehydration; bytes hashed and transferred;
CAS local/peer/remote hits; primitive spawn/join/cancel/replay; runnable/parked
tasks; pollpoint arms and yields; VIR/Weavy/native cache verdicts; compilation
time and code size. Counters are exact and low-cardinality. Detailed events may
be retained or sampled by policy.

Diagnostics are typed values throughout parsing, checking, VIR lowering,
Weavy lowering, primitive execution, and runtime failure. The inspection service
may render them, but rendered strings are not the diagnostic protocol.
