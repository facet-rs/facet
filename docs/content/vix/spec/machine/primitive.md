+++
title = "machine: primitives"
+++

Effect primitives: Rust-implemented host services exposed to vix through one
registered interface — exec, fetch, parsing of external formats, sealed-value
operations. Terminology decree: PRIMITIVES are in-machine host services;
CAPABILITIES are daemon-advertised toolchains (a `vixen.*` concern) that
primitives reference by identity.

r[machine.primitive.trait]

[DESIGN] Every primitive implements one trait: identity (versioned
`PrimitiveId` — primitive semantics enter demand keys, so a behavioral
change re-keys), vocabulary (schemas of its request/response value types),
memo policy, and non-blocking `begin(request, ctx) → Ticket` with completion
delivered to the scheduler (`machine.scheduler.completion-resumes-direct`).

r[machine.primitive.registered]

[SETTLED] Primitives are registered at machine construction. The machine has
no fixed effect set: no per-primitive match arms, no per-primitive fields in
scheduler data structures, no per-primitive receipt vocabulary variants, no
per-primitive ontology strings. (The old machine hardcoded its set in FIVE
homes; one registration replaces all five.)

r[machine.primitive.requests-are-values]

[DESIGN] Primitive requests and responses are ordinary typed vix values —
interned, content-addressed, receipted. Lowering emits one generic effect
request carrying (primitive identity, request value); adding a primitive
touches zero machine code.

r[machine.primitive.memo-policy]

[DESIGN] Each primitive declares `Hermetic` (fully memoizable: exec under
observation), `Pinned` (memoizable by observation pin: fetch), or `Volatile`
(never). The machine applies policy uniformly through the memo
(`machine.memo.effect-results`).

r[machine.primitive.effectctx-witness-only]

[DESIGN] A primitive's window into the machine is `EffectCtx`: witness-typed
store reads ONLY (a primitive physically cannot read unobserved), result
interning, event emission, and mount minting. A Rust-side primitive's
read-set is exactly its witnessed reads — receipts for primitives fall out
of Law 18 with no separate declaration mechanism.

r[machine.primitive.effect-set-v1]

[DESIGN] The initial registered set is the census class-B eleven: exec,
fetch, doc-parse, crate-archive, ELF-doc, AST-doc, OCI-doc, target-probe,
and the sealed triple (seal / declassify / to-string). Pure operations are
not primitives (`machine.execution.no-pure-hostcalls`); `glob` over a
concrete tree is the named example of a mis-classified pure op.

r[machine.primitive.sealed-boundary]

[DESIGN] The sealed family is a security boundary, deliberately
host-mediated: declassify is capability-gated by recipient and closed by
default; string coercion of sealed values renders `sealed:<identity>` and
never plaintext. (Preserved behavior.)

r[machine.primitive.exec-identity]

[DESIGN] Exec identity has two independent axes, mirroring memo
exact/projection: WHAT WOULD RUN (normalized plan + capability fingerprint —
exact match required) and WHAT THE WORLD LOOKS LIKE (mounts/reads —
approximable, re-verified against observations). Tier-2 reuse serves without
matching mounts when the recorded read-set verifies — the anti-Nix event.
(Preserved from `ExecCache`.)

r[machine.primitive.exec-plan-normalized]

[DESIGN] Exec plans are normalized before hashing: role-typed commutative
flags sort; inputs, flag-owned pairs, and search order stay positional.
"Same computation, different spelling" shares identity. Roles come from
command grammars (`machine.capability.no-argv-dialect`), and normalization
is the grammar's job — the equivalence is preserved, its implementation
moves out of hand-rolled Rust.

r[machine.primitive.exec-probed-toolchain]

[SETTLED] A declared capability token is NOT sufficient exec identity: the
backend probes the live toolchain (`rustc -vV`, `cc --version`) at
resolution time and folds probe output into the effective identity. Two
hosts with different compiler builds and the same declared token must not
collide. (Preserved from the real-process backend; this is the
runner-advertised-capability mechanism.)

r[machine.primitive.exec-hermetic-traps]

[SETTLED] Undeclared reads fail loudly at two layers: path resolution
outside declared mounts is a hard error that propagates (never an empty
read), and undeclared ambient toolchains are ACTIVELY interposed — trap
executables poisoning PATH — because passive omission lets the host leak
in. A backend that does not interpose a VFS must document exactly which
reads it can and cannot observe (the current real-process backend is
explicitly host-trusting outside declared roles).

r[machine.primitive.exec-two-tier-key]

[DESIGN] The exec cache key is two-tier via the command grammar: tier 1 =
normalized command + capability fingerprint + input NAMES (computable before
reading any input byte); tier 2 = tier 1 + input content hashes, closed over
the observed read-set. Lookup precedes input I/O by design.

r[machine.primitive.fetch-is-an-invocation]

[DESIGN] Fetch is a memoized invocation with stable closure identity flowing
through the same demand/memo path as everything else — not a bespoke
journal-pinned side path. Its observation-pin semantics
(`machine.receipt.fetch-observation-pin`) ride the ordinary receipt
machinery.

r[machine.primitive.typed-deserialization]

[DESIGN] Format parsing (doc-parse) targets vix structs directly via schema:
one host call per document, typed store values out, zero generic-Doc
projection walking on hot paths. Generic Doc access remains for
dynamic/exploratory use only. (Stage two — grammar-driven generated weavy
deserializers — is `lang.*`/weavy roadmap, referenced not specified here.)

r[machine.primitive.target-value]

[DESIGN] `Target` is a first-class vix value with schema and literal syntax;
OS/arch derive from taxon schemas. `(os_index: u64, arch_index: u64)` and
its kind are banned.
