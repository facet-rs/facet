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
> [DESIGN] A capability package is **data**, not host code with per-tool match
> arms. It carries the tool closure's identity and its four contracts: the
> command grammar (argv roles, validation, normalization, declarable products),
> the termination grammar (exits and signals to an `A` constructor or a typed
> failure), the output protocol (stdout/stderr framing), and the product
> protocol (when a declared product is immutable and ready). Adding a tool is
> publishing a package.
>
> **A package's content identity enters command identity**, so changing a
> grammar changes what a recipe means and invalidates honestly. That identity
> is a hash of the package's content, not a version anybody picks
> (`r[vixen.registry.no-declared-versions]`); an earlier draft of this rule
> said "versioned" three times and meant this.
>
> Note the word collision, which is not resolved here: a **capability package**
> (this page) and a **vix package** (`r[vixen.package.is-a-module]`,
> `r[vixen.registry.*]`) are different genres sharing a noun. Nothing on this
> page is about the thing a registry publishes.

> r[vixen.capability.package-declares-its-holes]
>
> [DESIGN] A package also declares the **hole delimiter** its command
> templates use (`lang.template.command-holes`). Not a fifth unrelated field:
> the package already owns the argv dialect, and how a hole is spelled inside
> that dialect is the same kind of fact as which flags carry which roles.
>
> It belongs here rather than in a global per-language table because the
> collision it avoids is per-tool. A delimiter must not occur in the tool's own
> argv, and **only the package knows that argv** — a shell package cannot use
> `{}` (brace expansion, `find -exec {}`), and a C compiler's argv is dense
> with braces that are not holes. One global choice would be wrong for someone
> by construction.
>
> The delimiter is part of the package's content and enters command identity
> with the rest of it, so changing it changes what a recipe means and
> invalidates honestly — the same promise every other contract here makes.
>
> **Open, and it is the case that motivated this rule:** a *flavor*
> (`r[lang.literal.flavor]`) cannot declare a delimiter — only a capability
> package can. So `@c```…``` `, guest text in a language full of braces, has no
> way to move its holes, and the collision this rule exists to avoid is
> unavoidable in exactly the place it was first noticed.

> r[vixen.capability.packages-ship-in-vixen-primitives]
>
> [DESIGN] For 0.1, packages are authored and registered in
> `vixen-primitives`, beside the primitives, and injected into a compilation the
> way host types and domain methods already are. `vix-core` gains no knowledge of
> any tool. Distribution of capability packages — third parties authoring and
> shipping them — is deferred.
>
> Being data does not mean not being compiled in: for 0.1 these packages ship
> as host-side declarations. `r[vixen.capability.package-is-data]` is a claim
> about their *shape* — one datum per tool, no per-tool match arms in the
> machine — not about where the bytes live.

> r[vixen.capability.rustc-is-acquirable]
>
> [DESIGN] 0.1's `Rustc` is an **acquirable** toolchain
> (`r[vixen.package.two-toolchain-classes]`): a pinned upstream distribution
> archive, fetched (`vixen.pins.*`) and **served at a VFS prefix**. Its identity
> is **the pin's** — known a priori, before a byte moves.
>
> Consequences, all of them deliberate:
>
> - **No probe is needed for identity.** `machine.primitive.exec-probed-toolchain`
>   answers "a declared token is not sufficient identity"; a pin answers it more
>   strongly and earlier, before anything is fetched. A probe may still run as a
>   *verification*, never to mint an identity.
> - **`exec` must be able to run a program out of the served tree** — which is
>   why the executable bit and symlinks are part of `machine.identity.tree-model`
>   and not a later refinement. A distribution archive that loses either is not
>   a toolchain.
> - **A just-built binary is a fourth case, not an instance of this one.** A
>   build script or proc macro is a tool closure with an identity, but that
>   identity comes from the build that produced it, not from a pin, so "known a
>   priori" does not apply. It is tagged the same way and acquired differently.
>
> This rule was `rustc-is-materializable`. Both the word and the verb were
> wrong: the toolchain is not "extracted to a `Tree` and mounted", and its
> identity is not that tree's identity computed after the fact. Deriving
> identity from the extracted bytes makes it a-posteriori in practice and
> reintroduces the extraction step as a load-bearing part of the model.

> r[vixen.capability.host-cc-is-ambient]
>
> [DESIGN] Linking reaches the host `cc` and its libc. That closure is
> **ambient** (`r[vixen.package.two-toolchain-classes]`): it does not go
> through the VFS, so its reads cannot be observed, so no read-set exists to
> record and the only sound assumption is that all of it is used.
>
> **0.1 reaches the ambient class and does not implement its regime.** The
> watch — filesystem change events over the whole image, poisoning everything
> on any change — is what soundness requires here and 0.1 has none of it. What
> 0.1 does instead is *declare* the closure and record its identity in the
> receipt, which is strictly weaker and is stated as a gap rather than a
> design: a 0.1 receipt is reproducible **given the same host linker**, and
> nothing detects when that stops being true.
>
> An earlier draft said "no daemon, no discovery, no advertisement, no poison
> is reachable in 0.1 — those exist for ambient toolchains, and 0.1 has none."
> The second clause is false. 0.1 has an ambient toolchain; it is this one.
> Making the linker acquirable is the post-0.1 work that would remove the gap,
> and is why the declaration is a package field rather than an assumption baked
> into the rustc grammar.
