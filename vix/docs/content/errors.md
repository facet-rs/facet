+++
title = "Failure"
weight = 22
+++

A failure **is a value**. It has a schema and a content hash like everything else,
so it can be stored, memoized, put in a record, and returned. What makes it a
failure is not what it is — it's what the machine does when you demand it.

```vix
fn require_key(m: Map<String, Version>) where { key: String } -> Version {
    m.get(key)
}
```

## `fail` says what; the machine says where

You supply a **payload** — any value, and it should be a typed one. The subject's
published identity and the failing operation's stable source site are attached for
you, because they are part of what the failure *is*. The current source span and
the **demand chain** — the breadcrumb of demands that led here — are reconstructed
when someone looks (see below).

You cannot forget any of it, because you never attach any of it.

> A failure that cannot name a published subject is a bug in the machine, not in
> your program. A task-private molten value has no published subject identity yet;
> the failure's payload carries the stable details the diagnostic needs.

This is why `fail "something went wrong"` is a weak thing to write and a fine thing
to have written: the string is a poor payload, but the failure still carries the
stable source site that raised it, and whoever reads it will see the current span
and the current demand that observed it.

**Coming from Rust**: `panic!` loses the chain and unwinds a stack you don't have.
`Err(String)` loses the address. Here, neither is possible — the address is not
yours to omit.

## A demand's answer is an outcome

```vix
Outcome<T> = Ok(T) | Failed(Failure)
```

That is what a memo entry holds. `fail e` has the type of nothing at all, so it
typechecks wherever you write it, and the demand it's inside answers `Failed(f)`.

**Propagation is a rule of the machine, not a property of the value.** Demanding
something whose outcome is `Failed(f)` makes your outcome `Failed(f)` too — you
wrote no symbol, and there is nothing to forget. That is why `?` is the operator
that *stops* propagation rather than the one that performs it.

## Failure poisons what demanded it, and nothing else

Nothing in vix evaluates until something demands it, so a failure spreads exactly
as far as demand did.

```vix
let objects = sources.map compile;   // 200 compiles; one fails
link objects.values()                // the link has no value
```

The other 199 objects still have values. Their receipts are valid, their memo
entries are live, and tomorrow they are cache hits. Only the demands that asked
for the failed one are poisoned.

So `vx build` reports one failure. `vx build --keep-going` reports all the
failures the graph contains, because each is an independent demand — the same
reason a test reports every check that fails rather than the first.

## Failures are cached, and cut off early

Because a failure is a value and an outcome is memoized, a failing demand is an
ordinary memo entry — **with its read-set.**

A build that failed yesterday fails *instantly* today, with the identical failure
value and a diagnostic rendered through today's source attribution, without
running the compiler again.

And early cutoff applies to failures. That failed compile depended on exactly the
files its read-set names. Edit your README, or an unrelated crate, and the failure
is still valid — proven by the read-set, reported without recomputation. Touch
something the compiler actually read, and only then does it run.

A failure is not a special case of the memo. It is the memo, working.

## The chain is not in the value

A failure carries its payload, its optional published subject identity, and its
stable source-site identity. Those are intrinsic: they are what the failure *is*,
and they go into its identity.

The current byte span and the **demand chain** do not. They name the current
source rendering and who asked, and they differ by compilation and caller. Were
either part of the identity, the same failure reached from two places or across a
formatting-only edit would be two different values and the memo would never hit —
the second caller would recompile a failure that was already known.

So the span and chain are reconstructed when you look, by resolving the stored
source site through the current source map and reading the live demand map. The
failure is content; the span and chain are context; you always see both, and only
one side is hashed.

## `?` is the only way to see a failure from inside

```vix
let parsed = (parse_manifest text)?;    // Result<Manifest, Failure>
```

A trailing `?` — no space before it — catches a failure and hands it to you **with
its address intact**:

```vix
match (parse_manifest text)? {
    Ok(m)   => use m,
    Err(f)  => yield diagnostic_for f,   // f has payload, subject, site; reports span, chain
}
```

It does **not** produce an `Option`. Turning a failure into `None` erases exactly
the thing the failure was carrying — which is the bug that motivated this entire
chapter. If you truly don't care why, say so, in the source:

```vix
let maybe = (parse_manifest text)?.ok();   // Option<Manifest>, deliberately
```

**Coming from Rust**: `?` is *propagation* there and *catching* here. It has to be:
propagation is the default — a failed demand poisons its dependents whether or not
anyone writes a symbol — so the operator's job is the opposite one.

## `Result` is for branching; `fail` is for not returning

They are not the same tool and they never substitute.

```vix
fn resolve(reqs: Reqs) -> Result<Solution, Unsat>   // the caller branches on both
fn require_key(m: Map<K,V>) where { key: K } -> V   // the caller branches on neither
```

If a caller will *branch* on the alternative — an unsatisfiable solve is a real
answer, with a derivation you want to print — that alternative is a value and it
belongs in a `Result`. If every caller would immediately give up, don't make them
say so: `fail`, and let the machine propagate it.

Both produce values. The difference is who is expected to look.

**`Option` is never an error channel.** `None` erases the failure's address by
construction: that is how a solve once came to fail with the string
`"unwrap on None"` and no location, no subject, and no demand chain.

## `unwrap` is a `fail`

```vix
o.unwrap()   // match o { Some(v) => v, None => fail UnwrapOnNone { … } }
```

It costs you a *diagnostic*, not a process: the span of the `unwrap` and the chain
of demands beneath it. That is why `.unwrap()` is honest in vix in a way it isn't
elsewhere — it names the place where you assumed, and says what you assumed about.

`m.get(k)` already has this shape: a present key produces `V`, and an absent key
fails with `MissingKey { key: k }` at the get site. There is nothing to unwrap.
Use `m.has(k)` when the question is membership, or `m.get(k)?` when the caller
intends to inspect the addressed failure.
