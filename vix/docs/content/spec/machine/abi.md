+++
title = "machine: substrate ABI"
+++

The driver↔weavy boundary. Boundary formats calcify into JIT stencils —
they are typed FIRST, because stencils turn ABI refactors into breaking
changes.

> r[machine.abi.typed-boundary]
>
> [SETTLED] The slot protocol is one typed boundary struct — Slots (readiness
> + awaited values), the host environment, value memories — not positional
> parallel slices. This lands before stencils bake the layout.

> r[machine.abi.readiness-bitset]
>
> [DESIGN] Slot readiness is bitset words (test = AND, schedule = find-first-
> set, progress = popcount), not byte-per-bool arrays. Positional protocols
> whose wrong index reads a valid neighbor are the named hazard.

> r[machine.abi.host-env-type]
>
> [SETTLED] The host environment is a type: `HostEnv` with a trait, methods on
> `&mut self`, a static dispatch table. Inline closure forests and
> `RefCell::new(&mut x)` — runtime borrow-checking to smuggle mutability into
> closures — are banned as patterns; the RefCell-of-&mut is a confession the
> design has the wrong shape.

> r[machine.abi.typed-frame-accessor]
>
> [DESIGN] Frame words are reached through one typed accessor. `from_le_bytes`
> on hand-computed frame offsets at call sites is hand-rolled ABI (64
> occurrences in the old dispatch region).
