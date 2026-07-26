+++
title = "Capability packages"
weight = 1
+++

A capability value (`Rustc`, `Cc`) is a typed, identified executable closure
supplied by the demand root (`machine.primitive.capabilities-by-identity`,
`spec/language.md` — "there is no universal `Rustc::acquire`"). A **capability
package** is what makes such a value nameable: the tool's closure plus the four
contracts the machine requires of every command
(`machine.primitive.command-package`).

The machine knows no tool's argv dialect (`machine.capability.no-argv-dialect`),
so there is no legal shortcut: **rustc cannot run until a rustc package exists.**
This page says what one is and what 0.1's looks like.

> r[vixen.capability.package-is-data]
>
> [DESIGN] A capability package is a versioned **data** package, not host code
> with per-tool match arms. It carries the tool closure's identity and its four
> contracts: the command grammar (argv roles, validation, normalization,
> declarable products), the termination grammar (exits and signals to an `A`
> constructor or a typed failure), the output protocol (stdout/stderr framing),
> and the product protocol (when a declared product is immutable and ready).
> Adding a tool is publishing a package. A package's identity is versioned and
> enters command identity, so changing a grammar changes what the recipe means
> and invalidates honestly.

> r[vixen.capability.packages-ship-in-vixen-primitives]
>
> [DESIGN] For 0.1, packages are authored and registered in
> `vixen-primitives`, beside the primitives, and injected into a compilation the
> way host types and domain methods already are. `vix-core` gains no knowledge of
> any tool. A package registry, third-party packages, and package distribution
> are deferred; nothing about the four contracts assumes their absence.

> r[vixen.capability.rustc-is-materializable]
>
> [DESIGN] 0.1's `Rustc` is a MATERIALIZABLE toolchain
> (`machine.capability.two-classes`): a pinned upstream distribution archive,
> fetched (`vixen.pins.*`), extracted to a `Tree`, and mounted. Its identity is
> that `Tree`'s identity — an a-priori content identity, like any other input.
>
> Consequences, all of them deliberate:
>
> - **No daemon, no discovery, no advertisement, no poison** is reachable in
>   0.1. Those exist for AMBIENT toolchains (Xcode/MSVC-class), and 0.1 has none.
> - **No probe is needed for identity.** `machine.primitive.exec-probed-toolchain`
>   answers "a declared token is not sufficient identity"; a content-addressed
>   toolchain tree answers the same question more strongly, before the process
>   runs. A probe may still run as a *verification*, never to mint an identity.
> - **`exec` must be able to run a program out of a `Tree`** — which is why the
>   executable bit and symlinks are part of `machine.identity.tree-model` and not
>   a later refinement. A distribution archive that loses either is not a
>   toolchain.
> - The same rule covers a build script and a proc macro: a binary you just
>   compiled is a tool closure with an identity, and it is tagged the same way.
>   Nothing is special about a tool somebody else advertised.

> r[vixen.capability.host-cc-is-declared]
>
> [DESIGN] Linking reaches the host `cc` and its libc, which 0.1 does not
> materialize. That closure is declared, its identity recorded in the receipt,
> and its instability acknowledged in writing rather than hidden: a 0.1 receipt
> is reproducible *given the same declared host linker*. Making the linker
> materializable is post-0.1 work and is the reason the declaration is a package
> field rather than an assumption baked into the rustc grammar.
