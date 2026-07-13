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
> - **`blake3` — the vix `ContentHash` — is REQUIRED.** It is the value's name in the one
>   identity space (`machine.identity.blake3`). Given it, the fetch resolves by identity:
>   local store, peer, shared store, and only then the origin.
> - **`sha256` (or any upstream digest) is OPTIONAL transfer provenance**: an integrity
>   check on the bytes that actually arrive, and a record of what the CDN, registry or
>   lockfile published. **It never becomes the value's identity.** A value must not be named
>   in a hash family chosen by whoever happened to host it. Both are recorded in the receipt.
>
> **There is no such thing as a SHA-only fetch.** An operation whose result identity is
> unknown until the bytes arrive is not a `fetch` at all — it is an **observation**
> (`machine.primitive.fetch-is-pinned`), and `fetch` may not become hermetic-or-observational
> depending on whether an optional field is present. That was the exact defect Amos rejected.
>
> Computing the canonical blake3 for an upstream artifact is therefore a **lock/update-time**
> act, not a build-time one: it happens when a dependency is added or bumped, its result is
> written into the recipe or lockfile, and every subsequent build knows the final `Blob`
> identity before it evaluates anything. Every `fetch` consequently satisfies
> `machine.placement.identity-crosses` — by construction, not by care.

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
