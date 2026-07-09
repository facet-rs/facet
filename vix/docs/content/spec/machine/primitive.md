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
> [DESIGN] Every primitive implements one trait: identity (versioned
> `PrimitiveId` — primitive semantics enter demand keys, so a behavioral
> change re-keys), vocabulary (schemas of its request/response value types),
> memo policy, and non-blocking `begin(request, ctx) → Ticket` with completion
> delivered to the scheduler (`machine.scheduler.completion-resumes-direct`).

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
> [DESIGN] Each primitive declares `Hermetic` (fully memoizable), `Pinned`
> (memoizable by observation pin: fetch), or `Volatile` (never). `Hermetic` is
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
> [DESIGN] A primitive's window into the machine is `EffectCtx`: witness-typed
> store reads ONLY (a primitive physically cannot read unobserved), result
> interning, event emission, and mount minting. A Rust-side primitive's
> read-set is exactly its witnessed reads — receipts for primitives fall out
> of Law 18 with no separate declaration mechanism.

> r[machine.primitive.effect-set-v1]
>
> [DESIGN] The initial registered set is the census class-B eleven: exec,
> fetch, doc-parse, crate-archive, ELF-doc, AST-doc, OCI-doc, target-probe,
> and the sealed triple (seal / declassify / to-string). Pure operations are
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
> [SETTLED, round 10] **`fetch` is pinned, always.** Its checksum is a required
> argument, so its value identity is known BEFORE evaluation; the URL is a
> *provenance coordinate* — a hint about where bytes live — not the identity.
> Demanding a fetch therefore resolves an identity (local store, peer, shared
> store, and only then the origin) rather than performing a network read; on a
> machine already holding the blob, nothing transfers. This is what makes a
> fetched value verifiable by a stranger, and it is the precondition for
> `machine.placement.identity-crosses`.
>
> A read whose result identity is unknown until it is performed is a DIFFERENT
> PRIMITIVE — an **observation** — and is not `fetch` with an argument omitted.
> One function may not be hermetic-or-discovering depending on the presence of a
> parameter (Amos, round 10). The observation primitive's name and shape are
> OPEN; until it lands, checksumless retrieval has no surface.
>
> Corollary: `machine.primitive.memo-policy`'s parenthetical "(memoizable by
> observation pin: fetch)" is stale. `fetch` is `Pinned` because its identity is
> GIVEN, not because its result is pinned after the fact.

> r[machine.primitive.capabilities-by-identity]
>
> [SETTLED, round 10] Capabilities (daemon-advertised toolchains) are referenced
> by IDENTITY, never by handle. `Rustc::acquire(spec)` opens no binary — nothing
> in a vix program evaluates, so it cannot. It NAMES one. Acquisition therefore
> happens outside a `place`, and must: the recipe pins one toolchain identity and
> every executor materializes *that* one, or the same recipe yields different
> artifacts on different machines. A capability is structurally a pinned blob —
> an identity some machine may be able to materialize. If none can, the demand
> fails before anything has run.

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
> [DESIGN, round 12] `exec` returns a struct with three fields and **no exit status**:
> `{ tree: Tree, stdout: Stream<Int, String>, stderr: Stream<Int, String> }`.
>
> `stdout`/`stderr` are **codata fields**. A stream may be a record field; the field's
> semantic content is the value it drains to (`machine.identity.streams-cross-island-edges`
> — a field is an edge), so `ExecOutcome` acquires an identity when the process finishes
> while a consumer may read lines long before. Keys are LINE NUMBERS: a process writes its
> output in order, and only the timing varies, so consuming stdout is deterministic even
> though arrival is not.
>
> `tree` is an ordinary value whose PROJECTIONS resolve at different times. Demanding
> `out.tree / p"early.txt"` does not demand the whole tree. Progressive exec trees are
> therefore not a feature of `exec` — they are partial dependency arriving at a subprocess
> boundary, exactly as `machine.placement.kill-is-laziness` is the laziness law arriving
> there.

> r[machine.primitive.exit-status-is-not-a-value]
>
> [DESIGN, round 12] An exit code is a naked `Int` where a typed outcome belongs, so the
> language does not expose one. A nonzero exit is a **failure**
> (`machine.error.failure-is-a-value`): the machine attaches subject, span and demand
> chain; the payload carries the status and the collected stderr.
>
> Where a nonzero exit is a legitimate ANSWER — `grep` returning 1 for "no match" — the
> **command grammar declares it**. Grammars already type argv on the way in; they type the
> exit status on the way out: which codes are outcomes, which are failures. An unrecognised
> status fails. `$?` and its undocumented magic numbers do not exist.

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
> [DESIGN, round 12] A fetch carries up to two hashes, and they do different jobs.
>
> - **`blake3` is the vix ContentHash** — the value's name, in the one identity space
>   (`machine.identity.blake3`). Given it, the fetch resolves by identity: local store,
>   peer, shared store, and only then the origin.
> - **`sha256` (or any upstream digest) is an integrity and provenance check** on the bytes
>   that actually arrive over the wire. It is what the CDN, registry or lockfile published.
>   **It never becomes the value's identity.** A value must not be named in a hash family
>   chosen by whoever happened to host it.
>
> A recipe MAY bake the canonical `blake3` even when upstream publishes only a `sha256`.
> Both are then recorded in the receipt: the identity that named the value, and the
> upstream claim that was verified against the transfer.
>
> Corollary: a fetch pinned ONLY by an upstream digest does not have a vix identity until
> the bytes arrive, so **it cannot cross a `place` boundary**
> (`machine.placement.identity-crosses`). That is the operational difference between the
> two hashes, and it is why the `blake3` is worth baking.

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
> Stream processing happens remotely by **placing the surrounding block**, not by handing
> a closure to `exec`. A placed block that consumes `out.stdout` consumes it next to the
> process; only the resulting value crosses back.
>
> **The observer closure is NOT obsolete. It is the lowering.** `vix-language-design.md`
> §"What ships to executors" already described it as "the canonical AST of the closure …
> holding the process handle, able to return anything incl. streams" — which is precisely
> the lowering of a placed block over an exec's codata fields. What is retired is the
> observer as a *surface construct* and as a *special exec mechanism*. Any document
> presenting `exec cmd where { observer: … }` is stale.
>
> Readiness follows: a file appearing in an output tree is a filesystem fact; readiness is
> a **protocol fact** (rustc announces artifacts on stdout — how cargo pipelines rmeta). The
> placed block reading `out.stdout` is the readiness authority; a subfile projection
> resolving early is the consequence. A VFS close event remains the fallback authority for
> protocol-less tools. (`/vix-design/exec-observers` — findings intact, mechanism superseded.)
