+++
title = "machine: identity"
+++

Content identity: what enters hash bytes, what never does, and where hashing
lives. Sources: the hash-as-field proposal (committee-converged), the
canonical-zero-padding decree, and preserved invariants from the current
driver.

r[machine.identity.single-module]

[SETTLED] Canonical encoding and hashing live in exactly one module. Ad-hoc
hasher construction and inline hand-rolled encodings anywhere else are
banned. Layout constants live with the encoding module
(`machine.identity.layout-constants` is subsumed here: an inline
`assert!(entry == 32)` far from the layout definition is the violation).

r[machine.identity.blake3]

[SETTLED] All machine content hashes are blake3. This is load-bearing for
persistence: a vix value IS a vx-store object by digest only if the hash
families match. (Supersedes the stale SHA-256 recommendation in
`vx-store-as-vix-memo.md`.)

r[machine.identity.canonical-memory]

[SETTLED] Store bytes are canonical zero-padded memory and value identity is
`blake3(memory)`. There is no separate canonical encoding to decode from:
reads are typed views over the same bytes that were hashed. Freeze = hash +
intern, no re-encode.

r[machine.identity.zero-padding]

[SETTLED] Padding in weavy-declared layouts is canonically zero — write
paths zero-initialize, mutations preserve it, copies preserve it. Inactive
enum payload bytes are zeroed; variant switch is atomic with payload
zeroing. A release-profile canary verifies the invariant continuously (the
padding law is enforced, not asserted). Facet-discovered values canonicalize
at the bridge. Consequence: flat-byte hashing is valid unconditionally for
canonical layouts; padding-range proofs (`is_padding_range`) demote to
canary/verification machinery, not hash-path logic.

r[machine.identity.le-encoding]

[SETTLED] Identity hashing uses little-endian word encoding, uniformly,
stated once as an invariant of the identity module — not left implicit at
call sites. (There is no version tag on the hash format; any endianness
drift silently invalidates every existing hash.)

r[machine.identity.streaming-combine]

[SETTLED] Aggregate identity is a pure function of an ordered byte stream
fed to one hasher. Commutative or additive combination of pre-hashed
children is banned (the Wagner k-sum attack class: additive combines over
attacker-influenced content admit forged collisions). Corpus obligation: an
N-step incrementally built aggregate hashes bit-identically to a bulk build.

r[machine.identity.carried-hasher]

[SETTLED] Growing aggregates carry incremental hash state (a live hasher)
across mutation; recomputing an aggregate's hash from scratch per intern
crossing is banned. (The measured O(N²): 86% of solver CPU in hash
recompression because the aggregate hash had no memory between crossings.)

r[machine.identity.hash-at-construction]

[DESIGN] Value identity is computed once — at construction for immutable
values, at freeze for molten ones — and carried as a field (write-once
identity slot; droppable cache on molten mutation). Per-lookup identity
recomputation is banned; the memo reads stored identity slots.

r[machine.identity.handle-by-referent]

[SETTLED] A handle's contribution to a container's identity is its
referent's content hash — never the handle integer, which is process-local
indirection. Handles are not hash-visible.

r[machine.identity.tier-not-in-hash]

[SETTLED] `HandleTier` (pending/realized scheduling state) never enters hash
bytes. A `Pending<T>` and its eventual realized value share declared
identity, computed as if resolved — this is what lets a waiter recognize the
value it awaited without re-deriving identity. (Preserved from the current
driver, comments at the three hash sites.)

r[machine.identity.pending-identity]

[DESIGN] A not-yet-invoked closure has content identity before it runs:
`PendingInvocation { closure_hash, canonicalized args }` hashes at
allocation, so identical pending invocations dedup to one handle before
either resolves. `Pending<T>`/`Realized<T>` wrappers are part of schema
identity.

r[machine.identity.map-order-independence]

[SETTLED] Map identity is insertion-order-independent (a map is a set of
pairs). The required baseline is sort-first-then-hash over the canonical
pair order; incremental/Merkle map identity is an OPEN future optimization
that must be earned by profile, not assumed.

r[machine.identity.schema-ref]

[SETTLED] Schemas appear in every API as interned `SchemaRef` (taxon-derived
identity, parameterized — `Concrete { id, args }` distinguishes generic
instantiations). Schema strings exist only at ingest and debug boundaries.
The current `DescriptorMap.by_ref` is the preserved shape; string-keyed
lookup is legacy sugar that the rewrite does not reproduce.

r[machine.identity.hasher-contract]

[SETTLED] Every hasher documents its contract: input assumptions (pre-hashed
or attacker-influenced), collision classes, keyed or unkeyed. Magic
constants are named and cited. An "identity" hasher that transforms its
input is misnamed or wrong; map keys derived from blake3 output are true
identity (prefix), never re-mixed.

r[machine.identity.taint-in-identity]

[SETTLED] Structural taint (sealed values) is identity-affecting: a
container holding a sealed value hashes differently from the same-shaped
container holding untainted content, and child taints union into composite
identity. Taint cannot be dropped by a copy path that looks only at raw
bytes. (Preserved from `hash_with_taint`/`combine_taints`.)

r[machine.identity.secret-plaintext-never-hashed]

[SETTLED] Secret plaintext never enters a public identity. Sealed
identities derive from ciphertext only; erring toward nondeterministic
sealing (finer memo keys) is the safe direction. Rationale: the
content-addressed store is public-adjacent; plaintext-in-hash is a
dictionary-attack surface.
