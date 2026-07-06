# 90 — Substrate ledger: what NOT to build

rodin-core ran on plain Rust. Plain Rust gives you no memoization, no
incremental recomputation, no content-addressing, no dependency tracking — so
rodin-core built all of it by hand, and that hand-built machinery is a large
fraction of its ~10K lines. The vix machine provides every one of these
natively. Expressing the resolver as demand over the machine yields them for
free; re-implementing them in vix would rebuild the machine inside the machine.

This ledger exists so that when a rodin-core subsystem surfaces during
implementation, the reflex is "that's the substrate's job," not "port it."

## The mapping

- **Interner** (`Interner`, `PkgIx`, `SourceIx`, `FeatIx`) — small-integer keys
  so identities are cheap to compare and store. → The store content-addresses
  every value. Identity *is* the content hash; equal values are already the same
  handle. There is no interning step and no index types.

- **Read-sets** (`ReadSet`, `ReadSetField`, `ReadSetStabilityWitness`) — records
  of which fields a decision read, to know when a warm result is still valid. →
  The machine records projection read-sets automatically: a memoized function
  re-runs only when a field it actually read changed. You do not track reads; you
  just read, and the machine tracks.

- **Warm facts** (`WarmFactBundle`, `VerifiedWarmFacts`, `WarmLearnedNoGood`,
  `WarmFactVerifier`) — learned no-goods serialized, carried across runs,
  re-verified before reuse. → Memoization + warm reload. A learned fact is just a
  memoized value; it is reused when its content-addressed inputs recur, and it
  cannot be stale because identity is content. No serialize/verify step.

- **Proof graph** (`ProofGraph`, `ProofNode`, `ProofRule`, `*Witness`) — an
  explicit derivation DAG proving each learned fact, so warm reuse is sound. →
  The demand graph *is* the derivation. The trace of demands that produced a
  value is its proof; content-addressing is the verification. You do not build a
  proof object; the machine's evaluation is the proof.

- **Counterfactuals** (`CounterfactualQuery`, `ManifestEdit`, `SelectionDiff`,
  `ChangedSelection`) — "if this manifest edit, which selections change,"
  computed with warm-fact reuse. → Incremental invalidation. Edit an input,
  re-demand the result; only the blast radius recomputes, and the diff is the
  difference of two demanded results. No counterfactual engine.

## The rule this encodes

Any time the implementation is about to intern a value, assign an id, hand-roll
a canonical form, track what was read, serialize a fact for reuse, or prove a
derivation — stop. That is the machine's job. The resolver's job is only the
*resolution*: identity, constraints, narrowing, conflict learning, features,
targets. Everything else in rodin-core is scaffolding for a substrate we now
have.
