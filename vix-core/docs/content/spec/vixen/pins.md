+++
title = "Pins"
weight = 2
+++

`fetch` is pinned, always: its vix `ContentHash` (BLAKE3) is a required
argument, so a fetched value's identity is known before anything is evaluated
(`machine.primitive.fetch-is-pinned`). An upstream digest — what a CDN or a
`Cargo.lock` published — is transfer provenance and never becomes the value's
name (`machine.primitive.fetch-integrity-vs-identity`).

That leaves a question the 0.1 scope memo answered wrongly when it said
crate-archive hashes "come from the lock". **They cannot.** A `Cargo.lock`
carries SHA-256, and 0.1 deleted the one primitive that could mint a BLAKE3 from
an unpinned read. Minting is a *lock-time* act, and lock-time is out of 0.1's
scope — so it happens outside the language, before the build.

> r[vixen.pins.sidecar]
>
> [DESIGN] A workspace carries a **pin file**, `vixen.lock`, beside its
> `Cargo.lock`. One entry per fetched artifact: the provenance URL, the upstream
> digest as published (SHA-256, for transfer verification), and the minted
> BLAKE3 that names the bytes. A build reads pins; it never writes one.

> r[vixen.pins.minting-is-a-separate-act]
>
> [DESIGN] `vx pin` mints the file: it reads `Cargo.lock`, fetches each artifact
> once, verifies the published digest, computes the BLAKE3, and writes the entry.
> It is a CLI act outside the language — it observes the network, which no
> program may do. It is also where a materializable toolchain
> (`vixen.capability.rustc-is-materializable`) is pinned, by the same mechanism
> and into the same file.

> r[vixen.pins.missing-is-a-failure-not-a-fetch]
>
> [DESIGN] A build that reaches an unpinned artifact **fails**, naming the
> artifact and the command that would pin it. It does not fetch and it does not
> mint. This is what keeps "0.1 never observes a live coordinate" true in the
> implementation and not merely in the scope document: there is no code path
> from a build to an unpinned read, so the property cannot rot.

The shape is deliberately the one the retired `observe` primitive described:
an append-only claim history *is* a lockfile, `observe` writes an entry and
`fetch` reads a pin. 0.1 keeps the split and implements the writer in Rust,
outside the language, rather than reinstating the effect. When a live index is
back in scope, `vx pin` is what `observe` replaces — not `fetch`.
