+++
title = "machine: errors"
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
> [DESIGN] A vix-level unwrap of `None` produces a `MachineError` carrying the
> unwrap's vix source span and the demand chain. The bare-string sentinel and
> span-less unwrap requests are banned.

> r[machine.error.structural-impossibility]
>
> [SETTLED] A structural impossibility — a state the types claim cannot happen
> (comparator index out of bounds, post-force pending) — is a typed error or a
> panic. It is never folded into a legitimate-miss or `Ok(false)` path.
> (Twin of `machine.obs.loud-fallbacks`.)
