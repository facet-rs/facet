+++
title = "Failure"
weight = 22
+++

*Status: provisional — this page documents the language as designed; parts are
not implemented yet.*

A failure is not a value. It is the **absence** of one: a demand that has no
answer, and can say why.

```vix
fn require_key(m: Map<String, Version>) where { key: String } -> Version {
    match m.get key {
        Some(v) => v,
        None    => fail MissingKey { key, available: m.keys() },
    }
}
```

## `fail` says what; the machine says where

You supply a **payload** — any value, and it should be a typed one. Everything
else is attached for you: the subject's identity, the source span, and the
**demand chain**, the breadcrumb of demands that led here, read from the live
demand map at the moment of failure.

You cannot forget to attach them, because you never attach them.

> A failure that cannot name its subject is a bug in the machine, not in your
> program.

This is why `fail "something went wrong"` is a weak thing to write and a fine
thing to have written: the string is a poor payload, but the failure still knows
which demand it belonged to, which source span raised it, and what asked for it.

**Coming from Rust**: `panic!` loses the chain and unwinds a stack you don't have.
`Err(String)` loses the address. Here, neither is possible — the address is not
yours to omit.

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

## `?` is the only way to see a failure from inside

```vix
let parsed = parse_manifest text ?;    //  Result<Manifest, Failure>
```

A trailing `?` — no space before it — catches a failure and hands it to you **with
its address intact**:

```vix
match parse_manifest text ? {
    Ok(m)   => use m,
    Err(f)  => yield diagnostic_for f,   // f has payload, subject, span, chain
}
```

It does **not** produce an `Option`. Turning a failure into `None` erases exactly
the thing the failure was carrying — which is the bug that motivated this entire
chapter. If you truly don't care why, say so, in the source:

```vix
let maybe = parse_manifest text ?.ok();   // Option<Manifest>, deliberately
```

**Coming from Rust**: `?` is *propagation* there and *catching* here. It has to be:
propagation is the default — a failed demand poisons its dependents whether or not
anyone writes a symbol — so the operator's job is the opposite one.

## `Result` is for outcomes; failure is for absence

They are not the same tool and they never substitute.

```vix
fn resolve(reqs: Reqs) -> Result<Solution, Unsat>   // both outcomes are answers
fn require_key(m: Map<K,V>) where { key: K } -> V   // the other case has no answer
```

If a caller will *branch* on the alternative — an unsatisfiable solve is a real
result, with a derivation you want to print — that alternative is a value, and it
belongs in a `Result`. If there is nothing sensible to return, the demand has no
answer, and `fail` says so.

**`Option` is never an error channel.** Absence-as-failure erases the failure's
address by construction: that is how a solve once came to fail with the string
`"unwrap on None"` and no location, no subject, and no demand chain.

## `unwrap` is a `fail`

```vix
o.unwrap()   // match o { Some(v) => v, None => fail UnwrapOnNone { … } }
```

It costs you a *diagnostic*, not a process: the span of the `unwrap` and the chain
of demands beneath it. That is why `.unwrap()` is honest in vix in a way it isn't
elsewhere — it names a demand that has no answer, at the place you assumed it would.

And `m.get(k).unwrap()` to force an error is a thing you should never write, because
`fail MissingKey { key: k }` says what you meant and carries what you'd want.
