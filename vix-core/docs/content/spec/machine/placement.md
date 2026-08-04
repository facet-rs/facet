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
> [SETTLED] Placement never changes a value. The same demand evaluated on any
> admissible executor yields a bit-identical result. Consequences: the
> scheduler needs no consensus (duplicate work is a duplicate, not a conflict; a
> partition is not an outage; a stale advertisement is a rejected dispatch);
> speculation is always sound; and kill-anytime is always sound.
>
> **For placed pure work, the local build is a perfect correctness oracle** —
> run it here, compare, and any difference is a bug in placement. That oracle
> does **not** extend to a place containing an observation: an observation's
> result is not predictable from its inputs, so re-running it locally produces
> another observation rather than a check
> (`machine.placement.observation-inside-a-place`, which states there is nothing
> to check it against). The oracle is a property of purity, not of placement.

> r[machine.placement.identity-crosses]
>
> [SETTLED] **A value may cross a `place` boundary only if its identity
> is known without evaluating it.** A pinned blob (checksum in the source), a
> capability identity, a literal, and an input pinned at the demand root all
> cross. A derived value does not: knowing what `let x = expensive();` *is* means
> computing it, so either it is computed before the boundary or the `place` is
> drawn wider to contain its demand.
>
> This is the restriction that makes placement analyzable. Before dispatch, the
> exact **set** of things that may cross is known: no demand discovers in flight
> that it needs something the boundary never accounted for.
>
> Their **weight** is not known, and must not be claimed. A tree crosses as a
> grant, and blobs are fetched per-file on read
> (`machine.placement.trees-cross-as-grants`) — a workspace of ten thousand
> files whose compiler opens two hundred moves two hundred. What is settled
> before dispatch is *admissibility*, not *bytes*; discovering how much actually
> moves is the design, not a gap in it.

> r[machine.placement.no-in-program-steering]
>
> [SETTLED] A program cannot name where it runs. A program that could steer
> placement could make its value depend on the executor it ran on, and the same
> source would describe different artifacts in different places. This is
> `machine.scheduler.no-in-program-forcing` applied to location: **nothing in a
> program observes where it is**.
>
> That is narrower than "nothing observes the world", which an earlier draft
> said and which is false — `observe` is a primitive
> (`machine.primitive.fetch-is-pinned`), and
> `machine.placement.observation-inside-a-place` on this page is entirely about
> programs observing. What is forbidden is reading a *placement fact*, because
> placement is the one thing that must never reach the value.
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
> executors are admissible, which are preferred — lives outside the language and
> may be as explicit as its owner likes, precisely because it cannot change a
> value.

> r[machine.placement.capability-requirements-are-derived]
>
> [DESIGN] Placement is constrained along **two independent axes**, and
> an earlier draft of this rule collapsed them.
>
> 1. **Execution-platform contract.** A tool is built for an ABI, an OS and an
>    architecture. A content-addressed `x86_64-linux` binary can be *served* anywhere
>    and is *executable* only on a node that can run `x86_64-linux`. **Being acquirable
>    removes locality, not platform compatibility.** Both acquirable and ambient closures
>    impose this contract.
> 2. **Host-specific locality.** Only **ambient** closures (Xcode, MSVC, the platform's
>    system libraries) impose this: they exist solely where a daemon advertises their
>    fingerprint, so they are runnable exactly on **the set of nodes advertising that
>    fingerprint** — which may be empty, one, or many. Nothing about an ambient closure
>    makes it unique to a single node; what constrains it is the advertisement.
>
> So an acquirable closure admits nodes that **satisfy its execution contract**; an ambient
> closure admits the nodes that **advertise its fingerprint** — a set, possibly of more than
> one. The earlier claim, "a materialized closure constrains placement not at all", was
> false, and would have let the scheduler dispatch a Linux `rustc` to a Mac.
>
> "Satisfies the execution contract" is deliberately weaker than "is of that platform": a
> node may satisfy `x86_64-linux` through emulation, a compatibility layer, or a container.
> Admissibility is the node's claim to satisfy the contract, not an equality of labels.
>
> **Both requirements are known without analysis, because a capability is a bound
> argument rather than a discovered need.** A capability value is injected by the
> demand root and its capability-role arguments key the demand
> (`machine.primitive.capabilities-by-identity`), so by the time anything is placed,
> the closure and its contract are *data the demand is carrying*. The scheduler reads
> them; it does not infer them.
>
> An earlier draft derived them instead: "the set of closures reachable in a placed
> subgraph is syntactic (union over branches, fixpoint over recursion), so both
> requirements are statically derivable." That is a whole-program reachability
> analysis — the plan phase — and vix does not have one: plans do not exist until
> evaluation, and a statically-typed but interpreted language cannot enumerate what a
> subgraph will reach without running it. The conclusion survives because the premise
> was never needed; nothing has to be over-approximated when the value is already in
> hand.
>
> **Three things are being distinguished here, and an earlier draft collapsed two of them.**
>
> 1. **The target** — what the artifact is FOR. Semantic. It changes the value.
> 2. **The selected toolchain's host / execution ABI** — including Cargo's `HOST`. This is a
>    **pinned semantic property of the selected toolchain capability**,
>    and it enters exec identity via `machine.primitive.exec-probed-toolchain`) **and** a
>    scheduler **admissibility constraint**. It is not cost-model.
> 3. **The physical executor** — which executor, of the admissible set, actually ran it.
>    Cost-model, unobservable, absent from the semantic receipt
>    (`machine.placement.no-in-program-steering`).
>
> A cross-compiling `x86_64-linux` rustc emitting `aarch64-darwin` objects has an
> `x86_64-linux` execution contract (2) and an `aarch64-darwin` target (1); which admissible
> machine ran it (3) is nobody's business.

> r[machine.placement.results-cross-back]
>
> [DESIGN] `machine.placement.identity-crosses` governs **dispatch**: a value
> captured by, or imported into, a placed subgraph must have an identity known without
> evaluating that subgraph, because the boundary must know what it is shipping and what it
> weighs before it ships anything.
>
> It says nothing about **results**. A placed block's derived value — a `Tree`, a rendered
> diagnostic, a solved graph — is *computed remotely*, so its identity is not and cannot be
> known before the block is evaluated. It acquires an identity where it is computed, and
> that identity crosses back.
>
> The two directions are not symmetric and must not be conflated. A placed block
> may consume codata locally and return a finished diagnostic, or another
> evaluator may subscribe to codata/projections across the boundary. Both use
> the same remote demand/completion protocol; transport timing is unobservable.

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
> [SETTLED] An observation performed inside a `place` was performed by
> another evaluator, and by `machine.receipt.observation-pin` its pin
> becomes the receipt's authority. There is nothing to check it against.
> **Placement is trust-free exactly when everything inside it is
> content-addressed.** A placed subgraph containing an observation is therefore a
> different kind of object from one containing only pinned inputs, and must be
> visible as such.

> r[machine.placement.artifact-shipping]
>
> [SETTLED] Dispatch ships partitioned/lowered architecture-neutral artifacts,
> source maps, capture identities, primitive ABI requirements, and grants. The
> canonical closure AST remains semantic identity and audit authority, but the
> executor needs Weavy and registered primitives rather than vixc. It may JIT
> the shipped artifact locally.

> r[machine.placement.codata-crosses]
>
> [SETTLED] Codata and progressive projections may cross placement boundaries
> as remote demand edges. The protocol provides semantic keys or byte offsets,
> credit/backpressure, cancellation, completion, reconnection, and replay/spill.
> A transport frame is never a stream element. Capability disjointness needs no
> separate island rule: every effect is already a mandatory cut and carries its
> own derived admissibility requirements.
