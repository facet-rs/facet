+++
title = "Errors"
weight = 13
+++

The machine's failure model. The governing incident: a solve failing with
the string `"unwrap on None"` and no location, no subject, no demand chain.

> r[machine.error.typed]
>
> [SETTLED] Machine fallibility is one typed error enum (`MachineError`).
> `Result<_, String>` is forbidden everywhere in the machine.

> r[machine.error.carries-context]
>
> [SETTLED] Every `MachineError` carries: the operation, the subject's
> identity (schema + content hash) where one exists, the current vix source
> span where applicable, and the demand chain (the breadcrumb of demands that
> led here). `MachineError` is the immediate machine-error plane and may carry
> current context directly. A stored language `Failure` is a value on a
> separate plane: its identity does not include byte offsets, the live source
> map, or the live demand chain; those are resolved when the failure is
> reported.
>
> An error that cannot name its subject when a published subject exists is a
> bug in error construction.

> r[machine.error.option-not-channel]
>
> [SETTLED] `Option` is not an error channel. Fallible operations return
> `Result`; absence-as-failure erases the failure's address by construction.

> r[machine.error.from-propagation]
>
> [DESIGN] `MachineError` implements `From` for its component errors so `?`
> propagates without stringification (thiserror-style; snark-dsl is the
> in-house precedent).

> r[machine.error.option-unwrap-span]
>
> [SETTLED] A Vix-level unwrap of `None` produces a `Failure` with typed
> `UnwrapOnNone` payload and the unwrap source site
> (`machine.error.failure-source-site-identity`). Reporting resolves that site
> through the current lowering attribution to obtain the current source span
> and reconstructs the current demand chain. This is a language outcome, not a
> machine invariant error; bare strings and site-less unwrap requests are
> banned.

> r[machine.error.structural-impossibility]
>
> [SETTLED] A structural impossibility — a state the types claim cannot happen
> (comparator index out of bounds, post-force pending) — is a typed error or a
> panic. It is never folded into a legitimate-miss or `Ok(false)` path.
> (Twin of `machine.obs.loud-fallbacks`.)

> r[machine.error.index-out-of-bounds]
>
> [SETTLED] A dense-array read outside `0..len` is a typed `IndexOutOfBounds`
> demand failure carrying the demanded index, the array's length, and the
> indexing source site (`machine.error.failure-source-site-identity`). The
> machine's checked array-read vocabulary reports absence through a `present`
> witness so the lowering can raise it; a lowering that folds the miss into a
> zero element, a wrapped index, or an `Option` has erased the failure's
> address. Unlike `machine.error.structural-impossibility`, an out-of-bounds
> index is a legitimate program outcome, not an invariant break.
>
> A malformed array payload is a typed machine invariant error, never an
> `IndexOutOfBounds` language failure. If the array is a task-private molten
> value that has not crossed a publication boundary, there is no subject value
> identity to attach; the failure identity uses no subject slot, and the
> payload's `index` and `length` are the stable subject detail.

> r[machine.error.failure-is-a-value]
>
> [DESIGN, round 11] A failure **is a value**. It has a schema and a content hash
> like anything else (`machine.identity.value-identity-pair`), so it can be stored,
> memoized, put in a record, and returned. "The demand has no answer" is rhetoric;
> the demand's answer is a `Failure`.
>
> A demand's memo entry therefore stores an **outcome**, not a result:
> `Outcome<T> = Ok(T) | Failed(Failure)`. `fail e` has type `!` and typechecks
> anywhere.
>
> **Propagation is a rule of the machine, not a property of the value.** Demanding a
> value whose outcome is `Failed(f)` makes your outcome `Failed(f)` too, unless you
> catch it with `?`. Poison is per-demand: of two hundred sibling compiles, one
> failing poisons only what demanded it.
>
> The stored `Failure` is span-free and chain-free by construction. Its value
> bytes may contain a stable source-site identity
> (`machine.error.failure-source-site-identity`), never a raw byte range, live
> source map, or live caller chain.

> r[machine.error.failure-source-site-identity]
>
> [SETTLED] A `Failure` has an intrinsic source-site identity so two distinct
> failing operations with identical payloads remain distinct values. The
> canonical source-site identity is the producing island/closure `RecipeId`
> plus a stable closure-local node or trace-site identifier. The `RecipeId` is
> the semantic producer component; the closure-local site id identifies the
> raising operation within that producer.
>
> Source-site identity excludes byte offsets, rendered source spans, the live
> source map, the whole live demand key, and the live demand chain. Reporting
> resolves the stored site through the current `LoweringAttribution` /
> source-map chain to obtain the current span.
>
> A `Failure` value identity is framed in a dedicated failure-identity domain
> over `(typed payload, optional subject value identity, stable source-site
> identity)`. The subject identity is the value identity
> (`machine.identity.value-identity-pair`) of the subject where one has been
> published. Subject absence is explicit and means no published semantic subject
> exists, not that the subject is unknown after being available.

> r[machine.error.chain-not-in-identity]
>
> [SETTLED, round 11 — forced by `failure-is-a-value`] The **demand chain is not part
> of a failure's content identity.** A failure's identity is intrinsic:
> typed payload, optional subject value identity, and stable source-site
> identity (`machine.error.failure-source-site-identity`). The chain is
> *context* — it names who asked — and it differs per caller.
>
> Were the chain in the identity, the same failing computation demanded from two
> places would be two different values, and the memo would never hit. The chain is
> instead **reconstructed at the moment of observation**, by reading the live demand
> map (round-5 verdict: "error demand-chain = read of live demand map at failure
> time; no retention"). `machine.error.carries-context` is satisfied at the point of
> report, not at the point of construction.
>
> Raw byte spans follow the same rule: they are per-compilation observation
> context, not stable identity. A whitespace-only or span-only edit preserves the
> failure value identity and the cached `Failed(Failure)` outcome while the
> reported span is reconstructed from the current lowering/source-map state.

> r[machine.error.failures-are-cached-and-cut-off]
>
> [DESIGN, round 11] Because a failure is a value and an outcome is memoized, a
> failing demand is an ordinary memo entry — with its read-set.
>
> Consequences, and the second one is a product property nobody else has:
>
> 1. A build that failed yesterday fails **instantly** today, with the identical
>    failure value and a diagnostic re-rendered through the current attribution,
>    without rerunning the compiler.
> 2. **Early cutoff applies to failures.** A failed compile depended on exactly the
>    files its read-set names. Change anything outside that set — the README, an
>    unrelated crate — and the failure is still valid, proven, and reported without
>    recomputation. Change something inside it, and only then does the compiler run.
>
> Observing a cached failure reconstructs the current span and demand chain
> without re-executing the failed demand. A failure is not a special case of the
> memo. It is the memo, working.
