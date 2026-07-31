+++
title = "Primitives"
weight = 9
+++

Effect primitives: Rust-implemented host services exposed to vix through one
registered interface — exec, fetch, parsing of external formats, sealed-value
operations. Terminology decree: PRIMITIVES are in-machine host services;
CAPABILITIES are daemon-advertised toolchains (a `vixen.*` concern) that
primitives reference by identity.

> r[machine.primitive.trait]
>
> [SETTLED] Every Rust primitive is registered through a typed adapter over one
> object-safe runtime trait. Its descriptor contains a versioned `PrimitiveId`,
> request/response schemas, memo policy, capability/admissibility requirements,
> and protocol version. `begin(request_ref, EffectCtx) -> EffectTicket` is
> non-blocking; completion is delivered to the scheduler. A behavioral change
> changes `PrimitiveId` or protocol version and therefore re-keys demands.

> r[machine.primitive.registered]
>
> [SETTLED] Primitives are registered at machine construction. The machine has
> no fixed effect set: no per-primitive match arms, no per-primitive fields in
> scheduler data structures, no per-primitive receipt vocabulary variants, no
> per-primitive ontology strings. (The old machine hardcoded its set in FIVE
> homes; one registration replaces all five.)

> r[machine.primitive.requests-are-values]
>
> [DESIGN] Primitive requests and responses are ordinary typed vix values —
> interned, content-addressed, receipted. Lowering emits one generic effect
> request carrying (primitive identity, request value); adding a primitive
> touches zero machine code.

> r[machine.primitive.memo-policy]
>
> [SETTLED] Each primitive declares `Hermetic` (all inputs witnessed), `Pinned`
> (response identity is present in the request, as for fetch), `Observed`
> (identity becomes known through a receipted observation), or `Volatile`
> (never persistently memoized). `Hermetic` is
> a real obligation, not a label: it requires determinism PLUS interposition
> for every non-store input (files, env, time, randomness, network, process
> state) so that every input is a witnessed observation or pin. A backend that
> performs ambient OS/global reads it cannot witness (the current real-process
> backend outside declared roles) is NOT `Hermetic` — it is `Volatile` or
> produces non-persistent claims only. `EffectCtx` witness discipline
> (`machine.primitive.effectctx-witness-only`) is necessary but not sufficient
> for hermeticity; the confinement is. The machine applies policy uniformly
> through the memo (`machine.memo.effect-results`); a source that cannot be
> snapshotted (`machine.lifecycle.stable-snapshot`) forces `Volatile`.

> r[machine.primitive.effectctx-witness-only]
>
> [SETTLED] A primitive's only machine window is `EffectCtx`: witness-typed
> reads, typed result interning, progressive projection/codata publication,
> event emission, mount-grant minting, and completion. It exposes no raw store,
> memo, scheduler, path, network, or executor handle. A Rust-side primitive's
> read-set is exactly its witnessed reads; receipts require no opt-in call sites.

> r[machine.primitive.effect-set-v1]
>
> [DESIGN] The initial registered set is exec, fetch, observe, format decode,
> archive extraction, ELF/AST/OCI probes, attest, and the sealed operations
> (seal / reveal / identity rendering). Pure operations are
> not primitives (`machine.execution.no-pure-hostcalls`); `glob` over a
> concrete tree is the named example of a mis-classified pure op.

> r[machine.primitive.sealed-boundary]
>
> [DESIGN] The sealed family is a security boundary, deliberately
> host-mediated: declassify is capability-gated by recipient and closed by
> default; string coercion of sealed values renders `sealed:<identity>` and
> never plaintext. (Preserved behavior.)

> r[machine.primitive.exec-identity]
>
> [DESIGN] Exec identity has two independent axes, mirroring memo
> exact/projection: WHAT WOULD RUN (normalized plan + capability fingerprint —
> exact match required) and WHAT THE WORLD LOOKS LIKE (mounts/reads —
> approximable, re-verified against observations). Tier-2 reuse serves without
> matching mounts when the recorded read-set verifies — the anti-Nix event.
> (Preserved from `ExecCache`.)

> r[machine.primitive.exec-plan-normalized]
>
> [DESIGN] Exec plans are normalized before hashing: role-typed commutative
> flags sort; inputs, flag-owned pairs, and search order stay positional.
> "Same computation, different spelling" shares identity. Roles come from
> command grammars (`machine.capability.no-argv-dialect`), and normalization
> is the grammar's job — the equivalence is preserved, its implementation
> moves out of hand-rolled Rust.

> r[machine.primitive.command-package]
>
> [SETTLED] A versioned capability package owns four cooperating contracts:
> the command grammar (argv roles, validation, normalization, possible
> products), termination grammar (typed answer or failure), output protocol
> (stdout/stderr framing), and product protocol (when a declared product is
> immutable and ready). The invocation declares what may exist; the protocol
> declares readiness; Vix demand decides what is frozen/published; store policy
> decides residency. The machine never infers these from filenames or argv.

> r[machine.primitive.exec-probed-toolchain]
>
> [SETTLED] A declared capability token is NOT sufficient exec identity: the
> live toolchain's probe output (`rustc -vV`, `cc --version`) enters the
> effective identity, so two hosts with different compiler builds and the same
> declared token do not collide. Authority is single
> (`machine.capability.fingerprint-in-identity`): the DAEMON advertises the
> fingerprint as the source of truth; a backend probe VERIFIES the advertised
> fingerprint (or emits a poison event on mismatch) and never silently mints a
> competing identity. For a materializable toolchain the "probe" is just
> hashing the mounted content.

> r[machine.primitive.exec-hermetic-traps]
>
> [SETTLED] Undeclared reads fail loudly at two layers: path resolution
> outside declared mounts is a hard error that propagates (never an empty
> read), and undeclared ambient toolchains are ACTIVELY interposed — trap
> executables poisoning PATH — because passive omission lets the host leak
> in. A backend that does not interpose a VFS must document exactly which
> reads it can and cannot observe (the current real-process backend is
> explicitly host-trusting outside declared roles).

> r[machine.primitive.exec-two-tier-key]
>
> [DESIGN] The exec cache key is two-tier via the command grammar: tier 1 =
> normalized command + capability fingerprint + input NAMES (computable before
> reading any input byte); tier 2 = tier 1 + input content hashes, closed over
> the observed read-set. Lookup precedes input I/O by design.

> r[machine.primitive.capability-role]
>
> [DESIGN] A registered request declaration marks which arguments are
> capabilities (`ArgRoleDecl::Capability`). The rail derives the effect demand
> preimage generically from the declaration: closure = the normalized request
> recipe (absent a command grammar: primitive identity plus every
> non-capability argument identity, in declaration order, domain-separated),
> arguments = the capability arguments' identities in declaration order.
> A command grammar refines the *arguments before the request is built*
> (`machine.primitive.exec-plan-normalized`), never the derivation itself. This is the tier-1 exec identity
> (`machine.primitive.exec-two-tier-key`) generalized to every
> capability-carrying effect; hand-rolled per-effect keying in scheduler code
> is banned. The capability's identity enters the preimage; its value is
> redeemed only host-side by the effect's backend service.

> r[machine.primitive.effect-backend-service]
>
> [DESIGN] An authority-crossing backend is an explicit `PrimitiveServices`
> entry (the `OriginAdapter` shape), never scheduler code: exec's backend owns
> spawn, stream, and wait. The demand key never mentions the backend, so a
> confining backend (`machine.primitive.exec-hermetic-traps`) replaces a
> host-trusting one behind the same capability identity without re-keying any
> demand — enforcement upgrades the service and the witness quality, not the
> memo. A host-trusting backend's receipts say so
> (`ReadObservation::Unverifiable`), per `machine.primitive.memo-policy`.

> r[machine.primitive.progressive-response]
>
> [DESIGN] An in-flight registered effect may publish progressive projections
> of its response through `EffectCtx`: byte-stream extensions (addressed by
> byte offset — `machine.primitive.exec-outcome`) and immutable product
> readiness on the authority of the declared output protocol, with effect
> completion as the fallback authority and filesystem polling banned. Each
> served projection is its own demand with its own memo location (the
> generalization of today's exec-only `submit_exec_projection`). The settled
> tail of the response publishes once, at completion, through the demand-owned
> ticket; a replayed stream is indistinguishable from a live one because the
> witness records what was published.

> r[machine.primitive.origin-routing]
>
> [DESIGN] Origin adapters install as a declared set: each entry states the
> coordinate schemes it serves and the capability schema it admits, as data.
> Selection is a lookup over declarations — adapters do not sniff and refuse,
> and the machine holds no default backend. An unroutable coordinate or an
> unconfigured origin is a loud typed refusal naming what was asked and what
> is installed; a silent fallback to any backend (today: the fixture store) is
> the conjuring failure mode and is banned.

> r[machine.primitive.origin-verbs]
>
> [DESIGN] The origin seam speaks two verbs: a coordinate read (bytes by
> coordinate, with a structured failure taxonomy — a miss that may fall
> through, a refusal that routes elsewhere, and a corruption that stops are
> different answers), and a tree projection (entry kind, file bytes, directory
> listing for a lazily-backed tree, in `TreeEntry` vocabulary). No core trait
> or type names a particular backend; `CodataDrainCtx::fixture_directory` and
> `FixtureEntryKind` are the counterexamples this rule retires.

> r[machine.primitive.witness-reverification]
>
> [DESIGN] The seam that produced a witness re-verifies it: the rerun audit
> resolves a receipt's projections through the installed adapter set exactly
> as the original read did, and the scheduler compares observations without
> naming any backend (`Runtime::reverify_read_witness`'s direct fixture calls
> are the counterexample). Misses are witnessed — a failed origin candidate
> records a `ReadObservation::Missing` witness per tried coordinate, so a
> multi-origin fallthrough is in the receipt, not forgotten — and a foreign
> upstream digest verified on arrival is recorded beside the vix identity
> (`machine.primitive.fetch-integrity-vs-identity`'s "both digests" made
> real).

> r[machine.primitive.fetch-is-an-invocation]
>
> [DESIGN] Fetch is a memoized invocation with stable closure identity flowing
> through the same demand/memo path as everything else — not a bespoke
> journal-pinned side path.

> r[machine.primitive.fetch-is-pinned]
>
> [SETTLED, round 10; sharpened round 12] **`fetch` is pinned, always.** Its **vix
> `ContentHash` (blake3) is a REQUIRED argument**, so its value identity is known BEFORE
> evaluation; the URL is a *provenance coordinate* — a hint about where bytes live — not
> the identity.
> Demanding a fetch therefore resolves an identity (local store, peer, shared
> store, and only then the origin) rather than performing a network read; on a
> machine already holding the blob, nothing transfers. This is what makes a
> fetched value verifiable by a stranger, and it is the precondition for
> `machine.placement.identity-crosses`.
>
> A read whose result identity is unknown until it is performed is a DIFFERENT
> PRIMITIVE — an **observation** — and is not `fetch` with an argument omitted.
> One function may not be hermetic-or-discovering depending on the presence of a
> parameter. The generic primitive is named `observe`; capability packages
> expose typed observation constructors and policies rather than an untyped URL
> read. An observation result is pinned into its receipt at execution time and
> cannot enter a trust-free placed subgraph.
>
> Corollary: `machine.primitive.memo-policy`'s parenthetical "(memoizable by
> observation pin: fetch)" is stale. `fetch` is `Pinned` because its identity is
> GIVEN, not because its result is pinned after the fact.
>
> **AMENDED (Amos, 2026-07-26): a pin need not be a blake3.** `fetch` is still
> pinned, always — an unpinned fetch does not exist. What changes is what a pin
> may BE: a pin is a digest *of the bytes*, and a foreign digest the ecosystem
> already publishes (a `Cargo.lock` `checksum`, a channel manifest's `.sha256`)
> is admissible. blake3 remains the one identity space: a foreign-pinned blob is
> verified against its pin on arrival and interned under its blake3 like any other
> value, and the demand memoizes on (coordinate + pin) so a repeat never
> re-fetches. A persisted `foreign digest -> blake3` side index — an ordinary
> store index, the same shape `machine.identity.tree-hash-is-not-node-hash`
> already uses — would additionally make cold resolution identity-first; it is
> deferred (`vixen.pins.digest-index-is-deferred`) and its absence costs a
> transfer, not correctness.
>
> The distinction the paragraph above was reaching for survives, but it is not
> *which hash family*. An **observation** is a read whose RESULT is not determined
> until somebody looks, so the looker's authority is the only witness. A
> foreign-digest pin is nothing of the kind: the bytes are fully determined and
> **verifiable by a stranger** against a published digest. What a cold
> foreign-digest pin lacks is not trust — it is the store's lookup key.
>
> The price, stated exactly, is paid once per artifact per store, not per build:
> cold resolution cannot precede the transfer, and a never-yet-fetched
> foreign-pinned blob does not satisfy `machine.placement.identity-crosses` until
> it has been resolved locally. Against that: requiring a blake3 means requiring a
> second lockfile beside the ecosystem's own, which is configuration — and the
> 0.1 north star is a *no-config* build.

> r[machine.primitive.capabilities-by-identity]
>
> [SETTLED] Capabilities are referenced by identity, never by process-local
> handle. A root injects a capability value or a package/toolchain solve returns
> one; there is no ambient `Rustc::acquire`. The selected identity is captured
> before placement and every executor materializes that exact closure. If no
> admissible executor can satisfy its execution contract, the demand fails
> before an effect starts.

> r[machine.primitive.typed-deserialization]
>
> [DESIGN] Format parsing (doc-parse) targets vix structs directly via schema:
> one host call per document, typed store values out, zero generic-Doc
> projection walking on hot paths. Generic Doc access remains for
> dynamic/exploratory use only. (Stage two — grammar-driven generated weavy
> deserializers — is `lang.*`/weavy roadmap, referenced not specified here.)

> r[machine.primitive.target-value]
>
> [DESIGN] `Target` is a first-class vix value with schema and literal syntax;
> OS/arch derive from taxon schemas. `(os_index: u64, arch_index: u64)` and
> its kind are banned.

> r[machine.primitive.exec-outcome]
>
> [SETTLED] A capability template produces `Command<A>` and `exec` returns:
> `ExecOutcome<A> { answer: A, tree: Tree, stdout: ByteStream, stderr: ByteStream }`.
> There is no exit-status field.
>
> `stdout`/`stderr` are byte codata whose completed values are Blobs. OS writes
> and transport frames are not keys. Immutable published ranges are addressed
> by byte offset; text decoding and line framing are explicit projections.
>
> `tree` is an ordinary value whose PROJECTIONS resolve at different times. Demanding
> `out.tree / p"early.txt"` does not demand the whole tree. Progressive exec trees are
> therefore not a feature of `exec` — they are partial dependency arriving at a subprocess
> boundary, exactly as `machine.placement.kill-is-laziness` is the laziness law arriving
> there.

> r[machine.primitive.exit-status-is-not-a-value]
>
> [SETTLED] The command package's termination grammar maps process termination
> to either an `A` constructor or a typed `Failure`. Conventional commands use
> `A = ()` and map exit zero to unit. A grep-shaped package may map zero to
> `Match` and one to `NoMatch`. Unmapped exits and signals fail with raw
> termination information. The mapping is versioned command semantics and
> enters command identity. `$?` and undocumented magic integers do not exist.

> r[machine.primitive.fetch-returns-a-blob]
>
> [DESIGN, round 12] **`fetch` returns a `Blob`, never a `Tree`.** An archive is a file.
> Unpacking is a separate demand (`extract`), whose result is a `Tree` whose identity is
> the canonical tree encoding (`machine.identity.tree-model`).
>
> **An archive-byte digest is not the resulting tree's digest.** Two archives differing in
> compression, member order or timestamps may unpack to one tree: one `TreeHash`, two
> `ContentHash`es. Conflating them would make the tree's identity depend on how somebody
> chose to `tar`.

> r[machine.primitive.fetch-integrity-vs-identity]
>
> [DESIGN, round 12] A fetch names its value with **one** hash and may additionally carry an
> upstream digest as transfer provenance. The two are not alternatives.
>
> - **A pin is REQUIRED**, and it is a digest *of the bytes*. Given a blake3 pin, the fetch
>   resolves by identity immediately: local store, peer, shared store, and only then the
>   origin. ~~`blake3` is REQUIRED~~ — AMENDED 2026-07-26, see below.
> - **A foreign digest is an admissible pin**, and is also transfer provenance: an
>   integrity check on the bytes that actually arrive, and a record of what the CDN,
>   registry or lockfile published. Pins are written self-describing
>   (`"sha256:…"`, `"blake3:…"`), one role-named field, several permitted and all
>   verified — the algorithm is data, not schema (`vixen.pins.self-describing`).
>   **A foreign digest still never becomes the value's identity**
>   — a value is not named in a hash family chosen by whoever happened to host it. The
>   machine resolves it through a recorded `foreign digest -> blake3` side index, minting
>   the entry the first time it sees the bytes. Both digests are recorded in the receipt.
>
> ~~**There is no such thing as a SHA-only fetch.**~~ **AMENDED (Amos, 2026-07-26).** There
> is: what may not exist is an **unpinned** fetch. The original text conflated "the result
> is not determined until somebody looks" (a genuine observation — the looker's authority is
> the only witness) with "the result's *vix-space name* is not yet known locally" (a store
> lookup-key gap over bytes a stranger can verify). Only the first is an observation, and
> only the first may not be `fetch`. See `machine.primitive.fetch-is-pinned` for the full
> amendment and its stated price.
>
> Consequently a pin is read from **the ecosystem's own lockfile** — `Cargo.lock`'s
> `checksum` is the pin for a crate archive — and vix requires no second lockfile beside it.
> Minting a blake3 remains a lock/update-time act *when a recipe wants one*; it is no longer
> a precondition of fetching. `machine.placement.identity-crosses` is satisfied by
> construction for a blake3 pin, and after first resolution for a foreign one.

> r[machine.primitive.exec-is-placement-agnostic]
>
> [SETTLED, round 12] **`exec` and `place` are decoupled and neither mentions the other.**
>
> `exec` is an execution primitive. It returns an ordinary struct
> (`machine.primitive.exec-outcome`) whose `stdout`/`stderr` fields are codata. It has no
> observer parameter, no callback, no runner hook.
>
> `place` evaluates a subgraph of demands on another evaluator
> (`machine.placement.identity-crosses`). It does not inspect the subgraph.
>
> Stream processing normally runs next to the process by placing the surrounding
> block. It may also cross to another evaluator through the generic codata demand
> protocol (`machine.placement.codata-crosses`). `exec` has no observer callback
> in either case.
>
> Readiness is a product-protocol fact, not a filesystem guess. The package may
> accept a tool-controlled message or promise monotonic/close-final output. For
> a protocol-less tool, process exit is the safe readiness authority; a bare VFS
> close is not, because a process may reopen and mutate the file. Progressive
> Tree projection is the result of the protocol publishing an immutable product.
