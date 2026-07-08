# HASH-AS-FIELD / memory-identity review, seat 2

Verdict: **revise, then proceed**.

The core direction is right: identity belongs with the value, finished store
entries already carry it, and the existing carried array hasher proves the
array-shaped slice. I would not authorize implementation from the proposal as
written because the map-hash recommendation weakens canonical content identity,
and the "never invalidated" language needs to become a precise validity rule for
carried state.

Grounding note: the proposal references `RESURRECTION.md`,
`capabilities-ambient-vs-materialized.md`, and
`vix/docs/content/redesign/2-hashing-flesh.md` as required branch-local
grounding. On `origin/hash-as-field-proposal`, only the proposal itself is
branch-local; those paths either exist only in the current working tree lineage
or were absent under the named path. The review below uses the live proposal
branch for code anchors and the current local `RESURRECTION.md` /
`2-hashing-flesh.md` context where available, but the proposal should make its
required grounding reproducible before it becomes the implementation charter.

## Findings

### P1: Reject the proposed commutative map combine as canonical identity

The proposal's Q1 recommends a per-pair hash followed by XOR/add-style
commutative combination for maps (`docs/design/hash-as-field-proposal.md:329`).
That should not ship as the canonical value hash.

Current map identity is sort-based: `alloc_map` canonicalizes through
`canonical_map_pairs` before hashing (`vix/src/machine/driver.rs:1466`), and
`canonical_map_pairs` canonicalizes key/value words, computes key/value hashes,
sorts by canonical key order, and dedupes (`driver.rs:10376`). Tests already
pin construction-order independence (`vix/src/machine/lower.rs:9502`,
`vix/src/machine/lower.rs:11697`).

Simple commutative folds do not preserve the same security contract:

- XOR is linear and duplicate-canceling. Even with length sealing, it is the
  wrong shape for a canonical content hash.
- Modular addition over 256-bit words is also algebraic. Hashing each pair
  before addition helps domain separation, but the accumulator is still a
  public abelian-group sum, not a collision-resistant encoding of a multiset
  in the same sense as `blake3(canonical_bytes)`.
- "Trusted machine-owned data" is not enough here. Vix values include registry
  metadata, manifests, archives, and eventually build outputs. Content identity
  is a correctness boundary, not an optimization-only checksum.

The safe choices are:

1. Keep sort-at-finalize for the second epoch and treat map carry as deferred.
   This preserves the canonical encoding and still lets arrays, whole-value
   identity slots, and projection field loads pay off.
2. If map performance must move in this epoch, use an ordered incremental
   structure: a B-tree or Merkle trie keyed by canonical map key order
   (`key_schema`, canonical key comparison / key hash with collision tie-break)
   and update O(log n) nodes on insertion. Final identity is the root hash of
   that canonical tree, not an O(1) commutative accumulator.
3. If the committee still wants a multiset hash, it needs a separate written
   cryptographic spec with security assumptions, collision target, duplicate
   semantics, canonical pair encoding, and adversarial tests. It should not be
   smuggled in as "XOR/add of pair digests."

This is a blocking revision because the proposal explicitly says the choice
changes the canonical encoding spec (`docs/design/hash-as-field-proposal.md:341`).
The spec change does not pay for itself as written.

### P1: "Invalidated never" is false for molten carried state

Finished interned identity is immutable. Molten carried identity is not a
finished identity slot; it is a validity-tracked cache of a future identity.

The proposal acknowledges the existing exception:
`intern_molten_word` clears the array carried hash when child interning changes
the word (`vix/src/machine/driver.rs:2120`, `driver.rs:2135`). That exception is
not incidental. It is the rule: a carried state is valid only if every folded
child identity is the final post-intern identity that will appear in the frozen
bytes.

Please rewrite the lifecycle rules as:

- Interned `ContentHash` is write-once and never invalidated.
- Molten carried state is valid/invalid, not write-once.
- Any mutation that cannot fold final canonical child identities must either
  force the child to a store identity before folding, or mark the carried state
  dirty and recompute at intern.
- The rule also applies to taint. Current array carry finalizes the base array
  hash and then applies `hash_with_taint` after collecting child taints
  (`driver.rs:1846`, `driver.rs:1852`; taint wrapper at `driver.rs:9683`).
  A generalized slot must state whether it carries base identity only or
  base-plus-taint, and must not silently drop taint invalidation.

This still supports the proposal. It just removes the misleading "never
invalidate" shorthand.

### P1: Zero-padding obligations need concrete writer coverage before flat memory identity

The zero-padding law is the right direction, but the proposal's obligation list
is not yet complete enough to justify "raw memory bytes are identity."

Fresh store allocation is zeroed today for the main construction paths:
`STORE_ALLOC` starts with `vec![0u8; descriptor.layout.size]` before writing
the tag and fields (`vix/src/machine/driver.rs:3200`), and `alloc_doc_variant`
does the same (`driver.rs:7707`). That covers fresh construction.

The known risk is in-place mutation and variant switching. Current retagging
writes only the direct tag (`write_variant_tag`, `driver.rs:11410`) and the
record-update path can retag cloned bytes (`driver.rs:5772`) before writing
selected fields. If a future in-place enum update switches from a larger
payload to a smaller one, stale payload bytes will remain unless the switch
operation zeros the entire inactive region before the new identity is folded.

The proposal should make the variant-switch operation a single primitive:

1. determine old active variant and new active variant;
2. zero every byte in the enum payload region not owned by the new active
   variant, including slack left by the old payload;
3. write the tag;
4. write/fold the new payload region exactly once.

The debug padding canary is necessary but not sufficient. It catches declared
padding, but stale bytes in an inactive union payload are not merely padding
unless `Descriptor`/`Access::Enum` exposes them as such for the selected
variant. The canary must cover inactive variant bytes as part of enum
canonicality, not only `RecordByteOwnership::Padding`.

### P2: The second-epoch gates should be split so array wins are not blocked by map research

The proposal's gate sequence puts "carried-hasher generalization to
maps/records" before flipping identity input (`docs/design/hash-as-field-proposal.md:266`).
That couples the proven array path to the unsettled map design.

I recommend this sequencing instead:

1. Define `StoredIdentity` / `CarriedIdentity` semantics, including taint and
   invalidation rules.
2. Land zero-fill and padding/inactive-payload canaries behind the old
   encoding hash, so the canary can fail before identity changes.
3. Flip whole-value identity slots and projection `Whole` reads to field reads
   where the current store already has `content_hash`.
4. Move arrays to the second-epoch carried identity path and keep the existing
   `changed_words` fallback.
5. Preserve sort-at-finalize for maps, or gate the map-specific structure as a
   separate sub-epoch with its own proof.
6. Only then delete descriptor-walk arms that are truly subsumed by the
   zero-padding/flat-memory proof.

That keeps the sanctioned hash break coherent without making map multiset
hashing a prerequisite for the array/trail win.

### P2: Cost model needs one more subtraction

The cost model is directionally credible because it says identity-as-field
alone does not hit 50x (`docs/design/hash-as-field-proposal.md:321`). But the
expected-win table overclaims two cells:

- `ProjectionPath::Whole` is already structurally a field-read wrapper:
  `projection_observation_hash` calls `canonical_word_hash_in_store`
  (`driver.rs:9989`), and that function returns `entry.content_hash` directly
  for store handles with matching schema (`driver.rs:9824`). The remaining
  cost there is call/dispatch/schema matching, not a full rehash.
- Map re-canonicalization is measured as a large trunk, but the proposal's
  replacement is the unsafe commutative combine above. Until the map design is
  revised, the "66%" should be reported as "addressable trunks" rather than
  "removed by this proposal."

The honest three-step path should be:

1. Arrays and whole-value identity slots remove the proven repeated-identity
   work on the solve trail and demand boundary.
2. JIT removes interpreter dispatch once identity reads are no longer host-call
   shaped.
3. Solver granularity keeps iteration molten; map identity gets optimized only
   after a map-heavy profile justifies a canonical ordered/Merkle structure.

## Non-blocking notes

- HandleTier remains correctly outside identity in the current code. It is used
  in `ValueStore::by_content` keys (`driver.rs:972`, `driver.rs:1380`) and
  allocation tier selection, not in the hash input. The declared wrapper rule is
  still honored at map option sites (`driver.rs:10618`).
- The proposal should stop citing `r[schema-identity.canonical-encoding]` for
  value map identity unless the intent is to extend taxon from schema identity
  into value canonicalization. The current taxon file encodes `Kind::Map` as
  key ref then value ref (`phon/rust/taxon/src/identity.rs:301`); it does not
  define runtime map-value canonicalization.
- If a blake3 stencil is still required for JIT, the proposal should name the
  exact ABI of the stencil state. `blake3::Hasher` is a Rust type, not a stable
  portable memory contract for persisted or cross-version midstate. This is fine
  for process-local molten state, but the proposal should say so.

## Required revisions before proceed

1. Replace the Q1 recommendation with either sort-at-finalize or an ordered
   Merkle map design. Do not use XOR/add commutative accumulation as canonical
   content identity.
2. Rewrite identity-slot semantics so finished store identity is write-once,
   while molten carried identity is validity-tracked and may be dropped and
   recomputed.
3. Specify taint handling in the identity slot.
4. Expand zero-padding obligations to inactive enum payload bytes and in-place
   variant switching, with a concrete force-copy fixture that shrinks variants.
5. Split the migration gates so array/whole-value wins can land without making
   map hashing research part of the critical path.

After those revisions, I would proceed with the second epoch.
