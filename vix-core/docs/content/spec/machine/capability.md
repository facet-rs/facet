+++
title = "Capabilities"
weight = 12
+++

The machine-side boundary with capabilities — toolchains carrying command
grammars, whether pinned and served or ambient and advertised. The capability
packages themselves (discovery,
fingerprinting, grammars-as-data, the daemon) are `vixen.*` spec territory,
specified at [/spec/vixen/capability-packages](/spec/vixen/capability-packages);
this page covers only what the MACHINE must honor about them.

> r[machine.capability.two-classes]
>
> [SETTLED] Two toolchain classes exist and the machine treats them
> differently. The axis is **whether reads can go through the VFS**, and
> everything else follows from it.
>
> **ACQUIRABLE** toolchains (rustc-class) are pinned fetches **served at a VFS
> prefix**. Identity is **a priori, from the pin** — known before a byte moves,
> so no read tracking is needed to establish it and no probe mints it. These
> are ordinary content-addressed inputs, not deeply capabilities at all.
>
> **AMBIENT** toolchains (Xcode/MSVC-class — legally or technically impossible
> to acquire) **cannot go through the VFS, therefore cannot be observed.** No
> read-set can be recorded, so the only sound assumption is that all of the
> image is used. These are capabilities proper: a-posteriori identity,
> daemon-advertised, fingerprinted, continuously re-verified, and poisoned
> wholesale on any change.
>
> The word for the first class was MATERIALIZABLE and its mechanism was "hash
> what you mount". Both are struck: identity comes from the pin rather than
> from hashing extracted bytes afterward, and nothing is untarred into a tree
> and mounted to obtain it (`r[vixen.package.two-toolchain-classes]`).

> r[machine.capability.fingerprint-in-identity]
>
> [SETTLED] The daemon's advertised fingerprint is the SINGLE source of truth
> for an ambient capability's identity; it enters exec identity and the receipt
> records it. A backend may probe per invocation only to VERIFY the advertised
> fingerprint or to raise a poison event on mismatch — never to silently mint a
> new competing identity. Advertisement is the trust event; the probe is
> verification, not a second authority.

> r[machine.capability.projectability-owned]
>
> [DESIGN] Projectability — which of a value's shapes a capability knows how to
> observe and record — is owned by the capability over `SchemaRef`, not by a
> stringly ontology hardcoded in the machine (the old `is_projectable_schema`
> name-match).

> r[machine.capability.poison-honored]
>
> [SETTLED] When the daemon poisons an in-flight build (the watched toolchain
> mutated underfoot), the machine fails the affected executions loudly and
> does not memoize their results. The machine's half of the contract is
> refusing to launder a poisoned run into the memo.
>
> Advertise ⇒ watch ⇒ poison exists **because** an ambient toolchain cannot be
> observed. Its reads do not go through the VFS, so there is no read-set to
> record and no way to ask afterward what was used — the only remaining move is
> to watch the whole image and invalidate everything on any change. Poison is
> what stands in for observation where observation is impossible; it is not a
> refinement of it.
>
> (An earlier draft said this kept receipts honest "over toolchains we can only
> observe", which inverts the mechanism: these are the toolchains we *cannot*
> observe, and that is the entire reason the machinery exists.)

> r[machine.capability.no-argv-dialect]
>
> [SETTLED] The machine knows no tool's argv dialect. Argument roles (input,
> output, search-dir, env) come from the capability's command grammar as
> typed captures; suffix sniffing, mount-prefix sniffing (`starts_with("/m/")`
> deciding semantics), and per-tool match arms in machine code are banned.
> Adding a tool is publishing a grammar.

> r[machine.capability.undeclared-is-loud]
>
> [SETTLED] Use of an undeclared capability is a loud failure (the trap-
> executable mechanism, `machine.primitive.exec-hermetic-traps`). Declaration
> is what makes ambient tooling legitimate; the receipt records the
> declaration and the observed identity.
