+++
title = "machine: capability boundary"
+++

The machine-side boundary with capabilities — daemon-advertised toolchains
carrying command grammars. The capability packages themselves (discovery,
fingerprinting, grammars-as-data, the daemon) are `vixen.*` spec territory;
this page covers only what the MACHINE must honor about them.

r[machine.capability.two-classes]

[SETTLED] Two toolchain classes exist and the machine treats them
differently. MATERIALIZABLE toolchains (rustc-class) are ordinary
content-addressed inputs — a-priori identity, hash what you mount, not
deeply capabilities at all. AMBIENT toolchains (Xcode/MSVC-class — legally
or technically un-materializable) are capabilities proper: a-posteriori
identity, daemon-advertised, fingerprinted, continuously re-verified.

r[machine.capability.fingerprint-in-identity]

[SETTLED] The daemon's advertised fingerprint is the SINGLE source of truth
for an ambient capability's identity; it enters exec identity and the receipt
records it. A backend may probe per invocation only to VERIFY the advertised
fingerprint or to raise a poison event on mismatch — never to silently mint a
new competing identity. Advertisement is the trust event; the probe is
verification, not a second authority.

r[machine.capability.projectability-owned]

[DESIGN] Projectability — which of a value's shapes a capability knows how to
observe and record — is owned by the capability over `SchemaRef`, not by a
stringly ontology hardcoded in the machine (the old `is_projectable_schema`
name-match).

r[machine.capability.poison-honored]

[SETTLED] When the daemon poisons an in-flight build (the watched toolchain
mutated underfoot), the machine fails the affected executions loudly and
does not memoize their results. Advertise ⇒ watch ⇒ poison is what keeps
receipts honest over toolchains we can only observe; the machine's half of
the contract is refusing to launder a poisoned run into the memo.

r[machine.capability.no-argv-dialect]

[SETTLED] The machine knows no tool's argv dialect. Argument roles (input,
output, search-dir, env) come from the capability's command grammar as
typed captures; suffix sniffing, mount-prefix sniffing (`starts_with("/m/")`
deciding semantics), and per-tool match arms in machine code are banned.
Adding a tool is publishing a grammar.

r[machine.capability.undeclared-is-loud]

[SETTLED] Use of an undeclared capability is a loud failure (the trap-
executable mechanism, `machine.primitive.exec-hermetic-traps`). Declaration
is what makes ambient tooling legitimate; the receipt records the
declaration and the observed identity.
