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
> identity (schema + content hash) where one exists, the vix source span where
> applicable, and the demand chain (the breadcrumb of demands that led here).
> An error that cannot name its subject is a bug in error construction.

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
> `UnwrapOnNone` payload and the unwrap source span. Reporting reconstructs the
> current demand chain. This is a language outcome, not a machine invariant
> error; bare strings and span-less unwrap requests are banned.

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
> indexing span. The machine's checked array-read vocabulary reports absence
> through a `present` witness so the lowering can raise it; a lowering that
> folds the miss into a zero element, a wrapped index, or an `Option` has
> erased the failure's address. Unlike
> `machine.error.structural-impossibility`, an out-of-bounds index is a
> legitimate program outcome, not an invariant break: a malformed array payload
> is the invariant break.

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

> r[machine.error.chain-not-in-identity]
>
> [SETTLED, round 11 — forced by `failure-is-a-value`] The **demand chain is not part
> of a failure's content identity.** A failure's identity is intrinsic:
> `(payload, subject identity, source span)`. The chain is *context* — it names who
> asked — and it differs per caller.
>
> Were the chain in the identity, the same failing computation demanded from two
> places would be two different values, and the memo would never hit. The chain is
> instead **reconstructed at the moment of observation**, by reading the live demand
> map (round-5 verdict: "error demand-chain = read of live demand map at failure
> time; no retention"). `machine.error.carries-context` is satisfied at the point of
> report, not at the point of construction.

> r[machine.error.failures-are-cached-and-cut-off]
>
> [DESIGN, round 11] Because a failure is a value and an outcome is memoized, a
> failing demand is an ordinary memo entry — with its read-set.
>
> Consequences, and the second one is a product property nobody else has:
>
> 1. A build that failed yesterday fails **instantly** today, with the identical
>    diagnostic, without rerunning the compiler.
> 2. **Early cutoff applies to failures.** A failed compile depended on exactly the
>    files its read-set names. Change anything outside that set — the README, an
>    unrelated crate — and the failure is still valid, proven, and reported without
>    recomputation. Change something inside it, and only then does the compiler run.
>
> A failure is not a special case of the memo. It is the memo, working.
