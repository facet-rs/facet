+++
title = "Placement"
weight = 55
+++

Where a demand is evaluated. Placement is **cost-model plane**: it cannot change
a value. What it *can* change is who observed what, which is why the boundary is
typed rather than advisory.

Distinguish two boundaries that are easy to fuse and must not be:

- an **island edge** carries a value between two computations in **one**
  evaluator. Identity, memo entries and receipts live there
  (`machine.identity.*`, `machine.receipt.*`).
- a **place** carries a subgraph of demands to a **different** evaluator.

> r[machine.placement.value-irrelevant]
>
> [SETTLED, round 10] Placement never changes a value. The same demand evaluated
> on any admissible machine yields a bit-identical result. Consequences: the
> scheduler needs no consensus (duplicate work is a duplicate, not a conflict; a
> partition is not an outage; a stale advertisement is a rejected dispatch);
> speculation is always sound; kill-anytime is always sound; and a distributed
> build has a perfect correctness oracle — the local build.

> r[machine.placement.identity-crosses]
>
> [SETTLED, round 10] **A value may cross a `place` boundary only if its identity
> is known without evaluating it.** A pinned blob (checksum in the source), a
> capability identity, a literal, and an input pinned at the demand root all
> cross. A derived value does not: knowing what `let x = expensive();` *is* means
> computing it, so either it is computed before the boundary or the `place` is
> drawn wider to contain its demand.
>
> This is the restriction that makes placement analyzable. Before dispatch, the
> exact set of things that cross — and their weight — is known. No demand
> discovers in flight that it needs something the boundary never accounted for.

> r[machine.placement.no-in-program-steering]
>
> [SETTLED, round 10] A program cannot name where it runs. A program that could
> steer placement could make its value depend on the machine it ran on, and the
> same source would describe different artifacts in different places. This is
> `machine.scheduler.no-in-program-forcing` applied to location: nothing in a
> program observes the world.
>
> Ambient facts arrive as inputs, supplied at the demand root. `Target::host()`,
> `uname`, `cfg!(target_os)` evaluated inside a recipe are the same bug: they read
> the executor into the artifact. `vx build --target` is resolved by the CLI,
> which is outside the program, and the recipe receives a `Target` value with an
> identity (`machine.primitive.target-value`).
>
> **An ambient read is an observation. An input is a pin.**
>
> This constrains the *program*, not the operator. Placement policy — which
> machines are admissible, which are preferred — lives outside the language and
> may be as explicit as its owner likes, precisely because it cannot change a
> value.

> r[machine.placement.capability-requirements-are-derived]
>
> [DESIGN, amended round 12] Placement is constrained along **two independent axes**, and
> an earlier draft of this rule collapsed them.
>
> 1. **Execution-platform contract.** A tool is built for an ABI, an OS and an
>    architecture. A content-addressed `x86_64-linux` binary is *materialized* everywhere
>    and *executable* only on a node that can run `x86_64-linux`. **Materialization removes
>    locality, not platform compatibility.** Both materialized and ambient closures impose
>    this contract.
> 2. **Host-specific locality.** Only **ambient** closures (Xcode, MSVC, the platform's
>    system libraries) impose this: they exist solely where a daemon advertises their
>    fingerprint, and the node holding them is the only node that can run them.
>
> So a materialized closure constrains placement to nodes *of its platform*; an ambient
> closure constrains it to *one advertised host*. The earlier claim — "a materialized
> closure constrains placement not at all" — was false, and would have let the scheduler
> dispatch a Linux `rustc` to a Mac.
>
> The set of closures reachable in a placed subgraph is syntactic (union over branches,
> fixpoint over recursion), so both requirements are statically derivable — a conservative
> over-approximation, costing perf and never correctness. Toolchain identity is
> value-affecting and belongs in the semantic receipt; the machine that hosted it does not.
>
> Note the asymmetry with the *target*: the platform a tool RUNS on is cost-model
> (`no-in-program-steering`); the platform it BUILDS FOR is semantic. A cross-compiling
> `x86_64-linux` rustc emitting `aarch64-darwin` objects satisfies an `x86_64-linux`
> execution contract while producing a `aarch64-darwin` artifact.

> r[machine.placement.trees-cross-as-grants]
>
> [DESIGN] A tree crosses a `place` boundary as an identity plus a **mount
> grant**: authority to read a prefix, and the coordinates of its blobs. Nothing
> is copied. Blobs materialize per-file, on read, by content hash; every read and
> every miss is recorded (`machine.receipt.witness-reads`, absence-is-an-
> observation). A workspace of ten thousand files whose compiler opens two hundred
> moves two hundred.
>
> Corollary: the previous run's read-set is a **prefetch plan**, and changing a
> file nobody read invalidates nothing — the memo is indexed by location
> (`machine.memo.indexed-by-location`), which is content-free, and the entry it
> finds carries a read-set the change misses.

> r[machine.placement.kill-is-laziness]
>
> [DESIGN] Stopping a process early is not a scheduler feature. If a demanded
> projection of an exec's output is determined, the remainder of that process's
> output is *undemanded*, and stopping it is the laziness law arriving at a
> subprocess boundary. The demanded projection's value is bit-identical whether or
> not the process was stopped.
>
> This is why kill-anytime is sound, and it is why the kill point must be driven
> by the demanded projection rather than by a scheduler's judgement — otherwise a
> scheduling artifact would enter a value's identity.

> r[machine.placement.observation-inside-a-place]
>
> [SETTLED, round 10] An observation performed inside a `place` was performed by
> another evaluator, and by `machine.receipt.fetch-observation-pin` its pin
> becomes the receipt's authority. There is nothing to check it against.
> **Placement is trust-free exactly when everything inside it is
> content-addressed.** A placed subgraph containing an observation is therefore a
> different kind of object from one containing only pinned inputs, and must be
> visible as such.

## Open

- **Does the AST travel, or the lowered island?** Lowering artifacts are already
  epoch-scoped and as-if, so both are sound. The choice decides whether an
  executor must host `vixc` or only `weavy` — that is, whether an executor can be
  a static binary on a machine you do not administer.
- **Is capability disjointness a mandatory island cut?** Effects are already
  mandatory cuts (`vix/_index.md`), so an island contains at most one effect and
  therefore at most one capability requirement: capability disjointness may be a
  *theorem* rather than a rule. Confirm, and delete this bullet.
- **Does codata cross a `place` boundary?** Streams cross island edges (ruled,
  round 10). A stream produced by a process on one evaluator and consumed on
  another makes back-pressure and cancellation a distributed protocol. If the
  consumer is inside the place, nothing crosses but the final value.
