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
> [DESIGN] Admissible as a pin: `blake3`, `sha256`, `sha512`. **That set is the
> only list.** Anything else is rejected by one path with one message, whether it
> is weak (`md5`, `sha1`), real but unimplemented (`sha3`), or not a digest at
> all. Adding an algorithm is a project decision, because it widens what every
> machine must be able to compute; retiring one is deleting a line.
>
> A pin's whole value is that a stranger can check it, which a
> collision-attackable digest does not deliver — that is *why* `md5` and `sha1`
> are outside the set, not a second category inside the rules. An earlier draft
> named them as "recognized and refused" so the diagnostic could explain itself;
> ruled out (Amos, 2026-07-26) because a graveyard of known-bad names is a second
> list that must agree with the first forever, and retiring `sha256` would then
> mean *moving* it between two lists rather than shrinking one. The rejection
> message names what IS admissible, which is the actionable half and cannot rot.
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
> [DESIGN] An **acquirable** toolchain
> (`r[vixen.capability.rustc-is-acquirable]`) is pinned by the same mechanism.
> Upstream publishes a digest beside the distribution archive; that digest is
> the pin. No separate pin store, no discovery.
>
> **An ambient toolchain cannot be pinned at all** — that is what makes it
> ambient. This rule is scoped to the acquirable class and says nothing about
> the other one; `r[vixen.capability.host-cc-is-ambient]` owns that case and
> states its cost.
>
> Where the pin is *written* is open. An earlier draft said "where the root
> declares its toolchain", meaning a manifest field, and there is no manifest
> (`r[vixen.package.*]`).

What remains true of **network reads**, and is the whole point: a build never
reaches an unpinned *fetch*. There is no code path from a build to a network
read whose result is not already named by a digest a stranger can check.
`observe` — a read whose result nobody can predict — stays out of 0.1 and stays
a different primitive.

The stronger claim — that a build never reaches an unpinned *input* — is false
in 0.1 and the exception is named: the host linker and its libc are ambient,
undigested, and reached by every link step.

## Deliberately not

Not deferred — refused. Each names the system that does the thing and the
reason we will not.

- **Nix's fake-hash workflow.** Write `hash = lib.fakeHash;` — which is
  literally `sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=` — build,
  read the error, paste the digest it reports:

  ```
  error: hash mismatch in fixed-output derivation '/nix/store/…-fod-test.drv':
           specified: sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=
              got:    sha256-mOpuTyFvL7S2n/+bOkSELDhobKaF8/VdxIxdP7EQe+Q=
  ```

  It is superb ergonomics and effectively everyone uses it. Look at what
  happened, though: the derivation *ran* — the fetch completed — and only then
  was the digest compared. The value you paste is a report from a network read
  nobody checked, and trust-on-first-use is the entirety of the verification.
  A pin is a required argument (`machine.primitive.fetch-is-pinned`), so there
  is no `fetch` that can be run to find out what it would have got, and this
  workflow has no expressible form.

  **The cost is real and is paid on purpose.** A first pin for a source no
  lockfile covers means fetching the bytes yourself and hashing them, or taking
  the digest the publisher published beside the archive. That is a manual step
  per new source, and it is why `r[vixen.pins.come-from-the-ecosystem-lockfile]`
  matters so much: for the ecosystems that dominate, the digest is in a file the
  workspace already has, and the step disappears. We pay it because the
  alternative is a mode of the one primitive whose entire contract is that it
  does not have one.

- **`--impure`, and unlocked fetches.** Nix's checks here are exactly right.
  In its default pure evaluation mode, `builtins.getEnv "HOME"` returns `""`
  where impure evaluation returns the caller's actual home,
  `builtins.currentSystem` is `error: attribute 'currentSystem' missing`,
  `builtins.fetchGit { url = …; }` without a `rev` is `error: in pure
  evaluation mode, 'fetchGit' will not fetch unlocked input`, and
  `builtins.fetchTarball` without a `sha256` is refused by name. The problem is
  not any of the checks; it is the switch. A flag that turns reproducibility
  off makes every build's reproducibility a question about how it was invoked,
  answerable only by whoever invoked it — and the flag is one word long. vix
  has no such flag and will not get one. The pin is a parameter, so the
  property is a type error to violate rather than a policy to enforce, and
  there is no invocation that relaxes it.

- **An optional integrity field.** npm's `package-lock.json` records
  `"integrity": "sha512-…"` beside `"resolved"` for a registry tarball — and
  the field is optional. Install an ordinary git dependency and the v3 lockfile
  entry is `"resolved": "git+ssh://git@github.com/…#<sha>"` with **no
  `integrity` key at all**. So "this project has a lockfile" and "the bytes
  were checked against a published digest" are different questions, per entry,
  answerable only by auditing every line — and where npm does fill the field in
  for a tarball URL you handed it, it fills it in with a hash of whatever
  arrived, which is the previous entry wearing a lockfile. In vix the pin is an
  argument to `fetch`. There is no entry for it to be missing from, and the
  unpinned case does not typecheck.

- **Go's checksum database.** `go.sum` records a hash per module, and for code
  the build has not seen before the `go` command consults `sum.golang.org`,
  which "is built on a Transparent Log (or 'Merkle tree') of hashes backed by
  Trillian". The log is real engineering against a real attack — a registry
  serving different bytes to different people — and we still refuse it. The
  refusal is the **notary**, not the log. A pin's whole value is that a
  stranger holding the bytes can check it with a hash function and nothing
  else: no server reachable, no network, no root of trust, no clock, no party.
  A notary reintroduces every one of those, and it introduces somebody who has
  to keep operating. `r[vixen.pins.self-describing]` and
  `r[vixen.pins.algorithm-strength]` are the complete trust story — an
  algorithm you can compute, a digest you can compare — and adding an authority
  to that is subtraction.

- **Normalizing at fetch time.** Bazel's `http_archive` takes `sha256` (or an
  SRI `integrity`) *and* `strip_prefix`, `patches` (`patch_args` defaulting to
  `-p0`), `patch_cmds`, `remote_patches`, and `build_file`. The digest is
  checked on download; stripping the top-level directory, applying the patches,
  and injecting a `BUILD` file all happen afterwards. The consequence is that
  the value the pin names and the value the build reads are not the same value,
  and the pinned byte string is one nothing downstream ever sees. `fetch`
  returns exactly the bytes it verified. Unpacking, stripping a prefix, and
  patching are ordinary functions over that value, each producing a value with
  its own identity and its own receipt — so "what did this build actually read"
  is answerable without re-deriving somebody's transformation from a rule's
  attributes.

- **An attestation instead of a digest.** `cosign verify-attestation`,
  `slsa-verifier`, and `gh attestation verify` check a signed statement that a
  named builder produced an artifact from a named source at a named commit.
  That is a claim about **provenance**, and it answers a different question. A
  pin is not a claim at all: it is a digest of bytes, checkable by anyone
  holding them, with no key to trust, no certificate to still be valid, no
  transparency log to be reachable, nothing to expire and nothing to revoke.
  Verifying a keyless attestation needs all of those. Provenance is worth
  having and is not what a pin is for; we refuse to let it stand in.
  `r[vixen.pins.0-1-verifies-on-arrival]` is the whole of 0.1's check, and it
  is arithmetic.
