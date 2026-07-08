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

[SETTLED] Exec references an ambient capability by its advertised
fingerprint; the fingerprint enters exec identity and the receipt records
it. The machine never re-probes-and-trusts per invocation — advertisement
is the trust event, refined by the probed-toolchain rule
(`machine.primitive.exec-probed-toolchain`).

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
