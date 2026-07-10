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
> zeroing. The verified discriminated constructor performs that whole-region
> initialization itself; a sequence of unrelated raw word writes is not an
> equivalent construction proof. A release-profile canary verifies the
> invariant continuously (the
> padding law is enforced, not asserted). Facet-discovered values canonicalize
> at the bridge. `is_padding_range` proofs demote to canary/verification machinery.
>
> ROUND-10 CORRECTION: this rule used to conclude "flat-byte hashing is valid
> unconditionally for canonical layouts." That is the conclusion of
> `machine.identity.canonical-memory`, which is **STRUCK** — and which
> `machine.identity.framed-encoding` [SETTLED] replaces outright. Zero padding is
> **hygiene**, not identity: it is a canary, and it makes flat-byte *comparison*
> meaningful for debugging. It licenses no hashing. A conclusion outlived its
> reason; this is the third instance found in this spec, and the pattern is now the
> thing to hunt.

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
> **This rule is scoped to ordered aggregates and says nothing about maps.**
>
> Maps canonicalize by structural key order
> (`machine.identity.map-order-independence`). A mutable map builder may carry
> keyed row state, but it MUST compute the final framed map hash from canonical
> rows at freeze; an insertion-order rolling digest is unsound.

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
> bytes. That is the whole rule.
>
> ROUND-10 CORRECTION: this rule used to continue "a `Pending<T>` and its eventual
> realized value share declared identity … this is what lets a waiter recognize the
> value it awaited." That clause **directly contradicted**
> `machine.identity.pending-identity` eight lines below, which says they do NOT
> share a content hash and that recognition is served by the memo. The round-5
> reconciliation identified this clause as the two planes smeared together and
> ordered it deleted; the sweep hit `pending-identity` and `store.dedup` and missed
> this one. A pending invocation is identified by its **recipe** (`DemandKey`); the
> realized value by its **content**. They are different planes and different bytes.

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
> [SETTLED] A map is a keyed collection with unique keys, not a set of pairs.
> Its canonical rows are sorted by the structural order of `K`; encoding frames
> the map schema, row count, and each key/value pair. Insertion order therefore
> cannot affect identity, while `Map<K,V>` and `Set<(K,V)>` remain distinct
> schemas and cannot share a canonical preimage.

> r[machine.identity.merkle-tree]
>
> [DESIGN, round 10] A workspace is a value, so it has an identity, so it must be
> hashed. A `Tree` (recursive, `machine.identity.tree-model`) is therefore identified as a
> **Merkle map** over its semantic encoding — not over the store's chunking
> (`machine.identity.tree-hash-is-not-node-hash`):
> change one file, rehash one path. This is not an optimization — it is what makes
> a workspace a value at all, and it is the Merkle-map design that
> `machine.identity.carried-hasher` is scoped around. The daemon watching local
> disk maintains it incrementally.

> r[machine.identity.streams-cross-island-edges]
>
> [SETTLED, round 10; corrected round 12] Codata may cross an island edge. The edge's
> semantic content is the VALUE the stream drains to; the incremental view is as-if. A
> stream therefore has recipe identity and no value identity of its own: its elements are
> ordinary demands, memoized individually; the aggregate has no content hash until resolved.
> A stream may not be a map key, and may not be sorted or compared.
>
> **A stream MAY be a record field.** A field is an edge, so the same rule applies: the
> field's semantic content is the value the stream drains to, and the record acquires its
> identity when the stream is done, while a reader consumes it long before.
> `machine.primitive.exec-outcome` depends on this — `ExecOutcome.stdout` is codata.
>
> This is what lets a consumer of a process's output be a separate demand — so
> changing an interpreter does not rerun the process — while still consuming
> progressively. Replay is the semantics; live consumption is the fast path.
>
> Codata and molten state are not symmetric. Molten state is a mutable,
> single-owner builder private to one island and has no replay protocol. Codata
> publishes immutable elements or byte ranges into an append-only logical log;
> published pieces are independently identified and replayable. Only the latter
> may cross (`machine.island.molten-private`).

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
> [SETTLED] Sealing is explicit value structure. `Sealed<T,Policy>` has a
> distinct schema and ciphertext-derived identity, so a container holding one
> necessarily hashes differently from an ordinary container. There is no
> parallel taint digest to combine or accidentally drop. Structural copy and
> projection preserve the wrapper through the same descriptor-driven paths as
> every other value (`machine.value.taint-provenance`).

> r[machine.identity.secret-plaintext-never-hashed]
>
> [SETTLED] Secret plaintext never enters a public identity. Sealed
> identities derive from ciphertext only; erring toward nondeterministic
> sealing (finer memo keys) is the safe direction. Rationale: the
> content-addressed store is public-adjacent; plaintext-in-hash is a
> dictionary-attack surface.

> r[machine.identity.tree-model]
>
> [DESIGN, round 12] **A `Tree` is not `Map<Path, Blob>`.** That spelling was an
> oversimplification: it cannot represent a directory (in particular an empty one), a
> symlink, or an executable bit, all of which the store models today.
>
> The semantic model mirrors `vx-services`'s `DirectoryNode`/`DirectoryEntryKind`, minus
> storage representation:
>
> ```
> Tree      = Map<Name, TreeEntry>          // Name is ONE path segment, not a path
> TreeEntry = File    { content: Blob, executable: Bool }
>           | Dir     (Tree)                 // recursive; an empty Dir is representable
>           | Symlink { target: String }
> ```
>
> The language value holds a **`Blob`**, not a `ContentHash` and not a size. A `ContentHash`
> is the Blob's *identity*; `size` is derived, and belongs to the store and the cost model.
> Naming them in the entry would leak storage fields back into the semantics while claiming
> to have removed them.
>
> `Tree` is therefore still a map — maps all the way down — but keyed by **segment**, and
> recursive. `tree / p"a/b"` is a projection through two maps.

> r[machine.identity.tree-hash-is-not-node-hash]
>
> [DESIGN, round 12] **Vix's `TreeHash` and Vixen's storage `NodeHash` are different
> identities over different preimages. They may not be conflated.**
>
> `DirectoryEntryKind` distinguishes `SmallFile { content_hash, size, executable }` from
> `LargeFile { content_hash, blob_node, size, executable }`, and `DirectoryNode` carries
> `total_size` documented as "for progress reporting only". The `SmallFile`/`LargeFile`
> split, `blob_node`, chunk boundaries, and `total_size` are **storage representation**.
>
> > **Rechunking must not change a Tree's semantic identity.** A file that crosses
> > `CDC_MAX_CHUNK` and becomes a `LargeFile` is the same file. A store that changes its
> > chunker must not invalidate a single memo entry.
>
> Therefore the semantic tree encoding hashes: entry name, entry kind tag, and per kind —
> `File`: (**the Blob's value identity**, executable); `Dir`: the child TreeHash;
> `Symlink`: target. The Blob's identity appears **once**, and carries its own size and
> bytes. The encoding hashes neither `blob_node`, nor the chunking discriminant, nor
> `total_size`, nor a separately-stated `size` — a size restated beside a content hash is a
> storage field wearing a semantic coat.
>
> This is the same disease that struck `machine.identity.canonical-memory` (ABI into
> identity) and `ExecTree`'s UTF-8 split (a representation predicate into the schema): an
> implementation convenience leaking into the semantic plane. `TreeHash` and
> `NodeHash` are intentionally domain-separated and MUST NOT share a preimage:
> `blake3("vix/tree/v1" || semantic_tree)` versus
> `blake3("vx/cas-node/v1" || versioned_phon_node)`. A side index maps between them.

> r[machine.identity.tree-canonicalization]
>
> [SETTLED] A `Name` is one nonempty valid-UTF-8 segment excluding `.`, `..`,
> separators, and NUL. Spelling is preserved without normalization. Entry order
> is Unicode scalar order; UTF-8 byte order is an equivalent storage comparator.
> Tree names are case-sensitive on every platform.
>
> `executable: Bool` is portable semantic intent and participates in identity on
> every platform. Windows preserves it as metadata. mtime, uid/gid, other mode
> bits, xattrs, resource forks, hardlink identity, device/FIFO/socket nodes, ACLs,
> and host case-folding are outside `Tree`.
>
> Ordinary symlink targets are relative valid UTF-8, preserved verbatim.
> Dangling links and `..` are representable; resolution may not escape the mount
> grant. Absolute links require an explicit non-relocatable artifact/import
> policy. Empty directories round-trip and participate in identity.
