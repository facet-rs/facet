+++
title = "Pins"
weight = 2
+++

`fetch` is pinned, always: a pin is a required argument, and an unpinned fetch
does not exist (`machine.primitive.fetch-is-pinned`). That is the reproducibility
line, and nothing below moves it.

A pin is a **digest of the bytes**, self-describing (`sha256:…`), passed as an
argument to `fetch`. It is not a file vixen maintains.

> r[vixen.pins.self-describing]
>
> [DESIGN] A pin is written `"<algorithm>:<digits>"` in one field named for its
> role (`hash`), never one field per algorithm. The algorithm travels in the
> value; the field name says what the value is *for*. A recipe may give several,
> as an array, and **every one of them must verify** — that is how a value carries
> both its vix name and the digest its registry published without the surface
> growing a field per registry.
>
> This is the general form of the rule the surface memo already reached for:
> algorithm-in-the-key (`sha256: "…"`) makes the *schema* ecosystem-specific, so
> adopting npm means editing the language. Algorithm-in-the-value makes it data.
>
> It is not stringly typing, and the discriminator that governs `Format`/`Mode`
> still holds. The algorithm set is CLOSED and runtime-implemented — you cannot
> verify a digest you cannot compute — so the text is *literal syntax for a closed
> enum plus a payload*, exactly like `p"…"` for a path. It parses to
> `(Algorithm, bytes)` at the boundary; an unknown algorithm is a typed error, at
> compile time when the pin is a literal.

> r[vixen.pins.canonical-digest-form]
>
> [DESIGN] Digits are **lowercase hex** canonically. Base64 spellings are accepted
> on input — SRI and npm write `sha512-<base64>`, Nix writes `sha256-<base64>` —
> and normalize to the canonical form. Accepting several spellings is what makes
> "paste the digest your ecosystem published" true.
>
> **A pin enters the demand key and the receipt as its parsed `(algorithm, bytes)`
> pair, never as its source text.** Two spellings of one digest are one pin, one
> key, one memo entry. Getting this wrong would let a whitespace or case
> difference in a lockfile fork the cache.

> r[vixen.pins.algorithm-strength]
>
> [DESIGN] Admissible as a pin: `blake3`, `sha256`, `sha512`. A pin's whole value
> is that a stranger can check it, which a collision-attackable digest does not
> deliver: `sha1` is accepted only as an additional recorded digest beside an
> admissible pin, never as the sole pin, and `md5` is rejected outright. Adding an
> algorithm is a project decision, because it widens what the machine must be able
> to compute.
>
> Not every ecosystem's integrity string is a digest of the bytes at all — Go's
> `h1:` is a hash over a file listing, not over an archive. Those are not pins and
> must not be spelled as if they were.

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

> r[vixen.pins.0-1-verifies-on-arrival]
>
> [DESIGN] For 0.1 a foreign-digest pin is simply **verified on arrival**: the
> bytes are fetched, checked against the pin, and interned under their blake3 like
> any other value. Nothing else is needed to be correct or reproducible, and a
> repeat demand never re-fetches — the fetch demand memoizes on its request
> (coordinate + pin) exactly as every other effect does.

> r[vixen.pins.digest-index-is-deferred]
>
> [DESIGN, post-0.1] The persisted `foreign digest -> blake3` side index is
> **deferred**. It is worth building eventually, and it is what buys the three
> things 0.1 does without: pre-resolution on a cold store (knowing the value's
> identity before the transfer), resolution of a foreign-pinned artifact from a
> peer or shared store by pin alone, and `machine.placement.identity-crosses` by
> construction rather than after first resolution. None of the three is reachable
> in 0.1 — there is no placement, no peer store, and one machine.
>
> Its absence costs a transfer, never correctness: without the index a cold store
> fetches bytes it could in principle have found locally under another name.

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
