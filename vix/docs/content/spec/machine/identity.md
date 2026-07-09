+++
title = "Identity"
weight = 1
+++

Content identity: what enters hash bytes, what never does, and where hashing
lives. Sources: the hash-as-field proposal (committee-converged), the
canonical-zero-padding decree, and preserved invariants from the current
driver.

> r[machine.identity.single-module]
>
> [SETTLED] Canonical encoding and hashing live in exactly one module. Ad-hoc
> hasher construction and inline hand-rolled encodings anywhere else are
> banned. Layout constants live with the encoding module
> (`machine.identity.layout-constants` is subsumed here: an inline
> `assert!(entry == 32)` far from the layout definition is the violation).

> r[machine.identity.blake3]
>
> [SETTLED] All machine content hashes are blake3. This is load-bearing for
> persistence: a vix value IS a vx-store object by digest only if the hash
> families match. (Supersedes the stale SHA-256 recommendation in
> `vx-store-as-vix-memo.md`.)

> r[machine.identity.canonical-memory]
>
> [STRUCK — superseded by the content-hash ruling, changelog round 5 final
> addendum.] The one content-hash definition is the schema-specialized framed
> walked encoding (`machine.identity.framed-encoding`), computed once at
> intern and carried on the store entry (entry-carried identity), read as a
> load thereafter. Flat-memory hashing was rejected because the structural
> hash of a value must never depend on the ABI: layout exists to be changed
> for performance, and coupling identity to it was an implementation-plane
> leak into the semantic plane. Zero-padding remains hygiene (and a canary),
> not identity-load-bearing. This rule id is retained struck so stale
> references fail loudly rather than silently meaning the old thing.

> r[machine.identity.value-identity-pair]
>
> [SETTLED] Semantic value identity is the pair `(SchemaRef, ContentHash)`, not
> `ContentHash` alone. A bytes-only hash collides values with identical bytes
> and different schemas (`Bool(false)` and `Int(0)`, newtypes over one word,
> `None` singletons, layout-equal records with different field meaning). Every
> consumer — memo keys, receipts, dedup, persistence claims — uses the pair. A
> vx-store object may be addressed by digest, but vix semantic identity is the
> typed pair.

> r[machine.identity.framed-encoding]
>
> [SETTLED] The identity module exposes only FRAMED writer APIs: start(domain,
> schema, arity), field(index, schema), variant(tag), seq-len, map-pair,
> bytes-len. Every variable-length or role-bearing component is length-prefixed
> or role-tagged, so the hashed byte stream is prefix-free and unambiguous.
> Raw `hasher.update(user_bytes)` outside these APIs is banned. (Streaming-
> combine bans additive combination; framing closes the remaining
> ambiguous-concatenation and cross-domain-reuse surface.)

> r[machine.identity.zero-padding]
>
> [SETTLED] Padding in weavy-declared layouts is canonically zero — write
> paths zero-initialize, mutations preserve it, copies preserve it. Inactive
> enum payload bytes are zeroed; variant switch is atomic with payload
> zeroing. A release-profile canary verifies the invariant continuously (the
> padding law is enforced, not asserted). Facet-discovered values canonicalize
> at the bridge. Consequence: flat-byte hashing is valid unconditionally for
> canonical layouts; padding-range proofs (`is_padding_range`) demote to
> canary/verification machinery, not hash-path logic.

> r[machine.identity.le-encoding]
>
> [SETTLED] Identity hashing uses little-endian word encoding, uniformly,
> stated once as an invariant of the identity module — not left implicit at
> call sites. (There is no version tag on the hash format; any endianness
> drift silently invalidates every existing hash.)

> r[machine.identity.streaming-combine]
>
> [SETTLED] Aggregate identity is a pure function of a framed, ordered byte
> stream fed to one hasher. Commutative or additive combination of pre-hashed
> children is banned — additive combines over attacker-influenced content admit
> forged collisions (the class the committee referred to as "Wagner k-sum";
> the label is session vocabulary, the mechanism is the ban on summation).
> Corpus obligation: an N-step incrementally built aggregate hashes
> bit-identically to a bulk build.

> r[machine.identity.carried-hasher]
>
> [SETTLED] Ordered append-only aggregates (arrays, lists, handle lists) carry
> incremental hash state across mutation; recomputing their hash from scratch
> per intern crossing is banned (the measured O(N²): 86% of solver CPU in hash
> recompression because the aggregate hash had no memory between crossings).
> This rule is scoped to ordered aggregates: maps use sort-first-then-stream
> (`machine.identity.map-order-independence`) because insertion order is not
> semantic order, so a carried streaming hasher over insertion is unsound for
> them until the OPEN Merkle-map design lands.

> r[machine.identity.hash-at-construction]
>
> [DESIGN] Value identity is computed once — at construction for immutable
> values, at freeze for molten ones — and carried as a field (the term
> "write-once identity slot" is session vocabulary; droppable cache on molten
> mutation). Per-lookup recomputation is banned; the memo reads stored slots. A
> MOLTEN aggregate has no public final identity until freeze: its carried
> identity is valid only over final child identities, so a demand key over a
> molten aggregate forces freeze first. (The hash-as-field distinction: an
> interned value's `ContentHash` is write-once; a molten value's carried
> identity is validity-tracked and droppable.)

> r[machine.identity.handle-by-referent]
>
> [SETTLED] A handle's contribution to a container's identity is its
> referent's content hash — never the handle integer, which is process-local
> indirection. Handles are not hash-visible.

> r[machine.identity.tier-not-in-hash]
>
> [SETTLED] `HandleTier` (pending/realized scheduling state) never enters hash
> bytes. A `Pending<T>` and its eventual realized value share declared
> identity, computed as if resolved — this is what lets a waiter recognize the
> value it awaited without re-deriving identity. (Preserved from the current
> driver, comments at the three hash sites.)

> r[machine.identity.pending-identity]
>
> [DESIGN] A pending invocation is identified by its `DemandKey` (its
> `PromiseId`): `blake3` of the framed (closure identity, canonicalized args),
> so identical pending invocations share one promise and one waiter set before
> either resolves. This is NOT the realized value's `ContentHash` — under
> flat-memory hashing the pending bytes (closure/args/promise state) and the
> result bytes are different, so a pending value and its eventual realized
> value do NOT share a content hash. The "recognize the value I awaited"
> property is served by the memo (`DemandKey → result`), not by identity
> collision. `Pending<T>`/`Realized<T>` remain distinct schema wrappers.

> r[machine.identity.hashing-is-ambient]
>
> [SETTLED] Content-hashing is a free, always-available property of any
> DAG-shaped value the machine builds — never something a consumer
> re-implements. (Warm-facts' `proof_digest` is meant to be the demand
> machine's own content hash of the proof structure; this rule is the
> substrate obligation that makes proof-bearing facts affordable.)

> r[machine.identity.never-consults-order]
>
> [SETTLED, round 7 as amended in round 9] A value's identity is a function of
> its content alone; no program value may move it. `<=>` is the STRUCTURAL
> comparison — derived from a type's fields in declaration order, total, and not
> overridable — so it is itself a function of content and a canonical encoding
> may use it. What may never enter identity is an `Order<T>`: orders are ordinary
> program values passed to ranking operations, and an encoding keyed on one would
> make a value's identity depend on unrelated user code. (Round 7 first stated
> this as "no content hash may be defined in terms of `<=>`", on the premise that
> `<=>` was user-overridable. That premise was retracted; the law survives, the
> reason changed.)

> r[machine.identity.map-order-independence]
>
> [OPEN — never ratified; the "[SETTLED]" tag was asserted, not agreed (Amos,
> round 7).] Map identity is insertion-order-independent: that much holds.
> What is rejected is the characterization — **"a map is a set of pairs" is
> wrong**: a map's keys are unique and a set of pairs' are not, so the sentence
> licenses an encoding under which `Map<K,V>` and `Set<(K,V)>` with equal
> contents could produce equal bytes.
>
> ROUND-9 CORRECTION: this rule was first struck on the grounds that
> sort-first-then-hash keys identity on a user-overridable `<=>`. That ground is
> VOID — `<=>` is structural and not overridable
> (`machine.identity.never-consults-order`), so ordering map rows by key order is
> a function of content and is sound. What survives is Amos's objection to the
> characterization, and the fact that the rule was never ratified.
>
> ROUND-10 PROPOSED REPLACEMENT (needs Amos): a map is **not** a set of pairs. It
> is a keyed collection whose keys are UNIQUE, whose rows are kept in key order
> (structural order on `K`), and whose encoding is framed with its `SchemaId` —
> so `Map<K,V>` and `Set<(K,V)>` with equal contents cannot produce equal bytes.
> Insertion-order-independence follows from row order being a function of the
> keys. Nothing may cite this rule as settled until it is.

> r[machine.identity.merkle-tree]
>
> [DESIGN, round 10] A workspace is a value, so it has an identity, so it must be
> hashed. A `Tree` (`Map<Path, Blob>`) is therefore identified as a **Merkle map**:
> change one file, rehash one path. This is not an optimization — it is what makes
> a workspace a value at all, and it is the "OPEN Merkle-map design" that
> `machine.identity.carried-hasher` is scoped around. The daemon watching local
> disk maintains it incrementally.

> r[machine.identity.streams-cross-island-edges]
>
> [SETTLED, round 10] Codata may cross an island edge. The edge's semantic content
> is the VALUE the stream drains to; the incremental view is as-if. A stream
> therefore has recipe identity and no value identity: its elements are ordinary
> demands, memoized individually; the aggregate has no content hash until resolved
> and may not be a record field.
>
> This is what lets a consumer of a process's output be a separate demand — so
> changing an interpreter does not rerun the process — while still consuming
> progressively. Replay is the semantics; live consumption is the fast path.
>
> ASYMMETRY TO JUSTIFY: molten values may NOT cross an island edge (pending
> think-item, lean "never — merge islands"). Molten and codata are structurally
> the same problem: a thing with no stable public identity crossing a boundary
> where identity lives. If streams may cross and molten may not, the asymmetry
> must be principled. Currently it is not written down.

> r[machine.identity.schema-ref]
>
> [SETTLED] Schemas appear in every API as interned `SchemaRef` (taxon-derived
> identity, parameterized — `Concrete { id, args }` distinguishes generic
> instantiations). Schema strings exist only at ingest and debug boundaries.
> The current `DescriptorMap.by_ref` is the preserved shape; string-keyed
> lookup is legacy sugar that the rewrite does not reproduce.

> r[machine.identity.hasher-contract]
>
> [SETTLED] Every hasher documents its contract: input assumptions (pre-hashed
> or attacker-influenced), collision classes, keyed or unkeyed. Magic
> constants are named and cited. An "identity" hasher that transforms its
> input is misnamed or wrong; map keys derived from blake3 output are true
> identity (prefix), never re-mixed.

> r[machine.identity.taint-in-identity]
>
> [SETTLED] Structural taint (sealed values) is identity-affecting: a container
> holding a sealed value hashes differently from the same-shaped container
> holding untainted content. Taint identity is a CANONICAL LEAF-SET — a sorted,
> deduplicated set of leaf taint ids — and union flattens (associative,
> commutative, idempotent), so grouping cannot affect identity
> (`union(a, union(b,c))` = `union(union(a,b), c)`). Taint cannot be dropped by
> a copy path that looks only at raw bytes. (Preserved from
> `hash_with_taint`/`combine_taints`, with the union algebra now specified.)

> r[machine.identity.secret-plaintext-never-hashed]
>
> [SETTLED] Secret plaintext never enters a public identity. Sealed
> identities derive from ciphertext only; erring toward nondeterministic
> sealing (finer memo keys) is the safe direction. Rationale: the
> content-addressed store is public-adjacent; plaintext-in-hash is a
> dictionary-attack surface.
