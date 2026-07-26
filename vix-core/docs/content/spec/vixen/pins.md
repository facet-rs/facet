+++
title = "Pins"
weight = 2
+++

`fetch` is pinned, always: a pin is a required argument, and an unpinned fetch
does not exist (`machine.primitive.fetch-is-pinned`). That is the reproducibility
line, and nothing below moves it.

A pin is a **digest of the bytes**, self-describing (`sha256:…`), passed as an
argument to `fetch`. It is not a file vixen maintains.

> r[vixen.pins.come-from-the-ecosystem-lockfile]
>
> [DESIGN] The pins for an ecosystem's artifacts are read out of that
> ecosystem's own lockfile. For Cargo, `Cargo.lock`'s `checksum` field is the
> SHA-256 of the published `.crate` archive — exactly the bytes `fetch` returns —
> so the pin is obtained by parsing a file the workspace already has, with
> `decode(Format::Toml)`, in vix. **Vixen adds no second lockfile.**
>
> An earlier draft of this page specified a `vixen.lock` sidecar minted by a
> `vx pin` command, on the reading that `machine.primitive.fetch-is-pinned`
> required a blake3 specifically. It does not — the rule was amended on
> 2026-07-26 to admit a foreign digest as a pin. The sidecar was also
> *configuration*, in a north star whose whole point is a **no-config** build,
> and it introduced a second lockfile that could disagree with the first.

> r[vixen.pins.foreign-digest-resolution]
>
> [DESIGN] blake3 stays the one identity space. A foreign-digest pin resolves
> through a `foreign digest -> blake3` side index in the store, minted the first
> time the machine sees the bytes and reused by identity thereafter. The cost is
> paid once per artifact per store, never per build: on a cold store the value's
> vix identity is not known until the transfer completes.

> r[vixen.pins.toolchain]
>
> [DESIGN] A materializable toolchain (`vixen.capability.rustc-is-materializable`)
> is pinned by the same mechanism. Upstream publishes a digest beside the
> distribution archive; that digest is the pin, written where the root declares
> its toolchain. No separate pin store, no discovery.

What remains true, and is the whole point: a build **never** reaches an unpinned
artifact. There is no code path from a build to a network read whose result is
not already named by a digest a stranger can check. `observe` — a read whose
result nobody can predict — stays out of 0.1 and stays a different primitive.
