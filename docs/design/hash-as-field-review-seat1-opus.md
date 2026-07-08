# Committee review — HASH-AS-FIELD / memory-identity (seat 1, opus, independent)

Reviewing `docs/design/hash-as-field-proposal.md` @ `origin/hash-as-field-proposal`.
Grounded against `origin/rodin` live sites. Same invariants as the first epoch I
adjudicated (HandleTier out of identity; declared-type wrappers in identity; zero
Rust-side bypasses; demand-driven tripwires load-bearing).

## Verdict: **PROCEED to implementation charters** — with four binding rulings folded in

The spine is sound and the existential framing is grounded (identity recompute over
immutable data is real repeated work; write-once-store-it-as-a-field is the right
shape). §6 is honestly scoped. But the proposal leaves the *hardest* technical
decision — the fold algebra for mutable aggregates — as open "or" choices, and it
under-states one dependency that determines whether the biggest trunk (29.1% maps)
actually collapses. The rulings below are not optional refinements; they are the
conditions under which the design is sound. None is a reject-grade defect.

---

## The unifying finding (governs §1-2 exception, Q1, and Q2 at once)

Grounded: `update_array_element_hash` (driver.rs) folds the child's **`ContentHash`
(identity), not the raw word** — good. Yet `changed_words → carried_hash = None`
(driver.rs:~2135) still fires, because `canonical_word_hash_in_store(*word)` is taken
on the **pre-intern** child, and if that child **re-canonicalizes at intern** its hash
changes, so the carried midstate is stale and must be dropped.

Consequence, and it is the review's spine:
- **Array of inline scalars (the trail loop):** scalar canonical hash is stable across
  intern → `changed_words` stays false → carried hash survives → the measured 50×
  lever. This is the *only* case the tree proves.
- **Array/map/record of aggregates (what the 29.1% map trunk *is* — maps of
  package→version records in the solve):** children re-canonicalize at intern →
  `changed_words` true → carried hash dropped → O(n) recompute. **The win does not
  materialize for the biggest trunk.** The array proof does not generalize by
  assertion.

The `changed_words` exception is therefore **sound but a silent perf hole**: it voids
the map/record generalization exactly on nested data. The root cause is *intern
re-canonicalizes*, so a molten fold necessarily used a pre-canonical child hash. The
cure is to make **molten-canonical == intern-canonical** so a child's identity is
stable the instant it is folded:
- for **maps**, intern's re-canonicalization *is the sort* (`canonical_map_pairs` →
  `pairs.sort_by(key_hash)`); an **order-independent commutative combine (Q1) removes
  the sort**, so molten-fold == intern-fold and `changed_words` dissolves for maps;
- for **records/scalars**, the **zero-padding + flat-memory law (§3)** makes the molten
  bytes already canonical, so intern does not re-canonicalize and the fold is stable.

So Q1, Q2, and the §1-2 exception are **one problem**: incremental identity of a
*mutable* aggregate is only sound when the child identities folded are stable across
intern, which the second-epoch machinery (commutative combine + flat memory) is
precisely what provides. **This is the strongest argument *for* the second epoch and
the proposal undersells it** — it presents flat-memory as a perf lever when it is also
the *soundness enabler* for the carried-hasher generalization. Make this dependency
explicit in the charters; do not let gate 3 (carried maps/records) precede gate 1
(zero-padding) and the Q1 ruling.

---

## (1) §1-2 identity-slot semantics — SOUND, with the exception reclassified

Write-once/read-forever is correct; the slot representation split (molten
`Option<CarriedHasher>` → interned `[u8;32]`) is clean and matches the landed array
machinery. HandleTier stays out of the slot (invariant preserved — the slot holds the
*value's* digest, not the store tier).

**Ruling on the `changed_words` exception:** it is a *correct fallback* but must be
reclassified from "the one honest invalidation" to "a symptom to be engineered away
for aggregates." §2 must **commit** (not "or") to folding **stable, post-canonical
child identities**, which is only achievable once §3 (records) and Q1 (maps) land.
Add a fixture: an array/map **of aggregates** asserting `changed_words` never fires
post-flat-memory. Until that fixture is green, the map/record win is unproven.

## (2) Q1 — commutative map-hash combine — APPROVE the spec change, with hardenings

Ruling **for** the order-independent combine over sort-at-finalize. It is not merely a
perf choice; it is what dissolves `changed_words` for maps (above). The spec change to
`r[schema-identity.canonical-encoding]` is **worth it and load-bearing**. Mandatory
hardenings:

- **Additive (mod 2²⁵⁶), NOT XOR.** XOR is GF(2)-linear and carries multiplicity only
  mod 2; additive combine (MSet-Add-Hash shape) is non-linear over GF(2) (carries mix),
  carries true multiset multiplicity, and **detects a double-fold/missing-inverse bug**
  because folding the same digest twice is visibly `2×`. XOR's sole advantage
  (self-inverse) is matched by subtract. Both are birthday-safe (2¹²⁸) *only if* the
  addends are uniform — so:
- **Each addend is a full `blake3(domain ‖ key_id ‖ value_id)`**, not raw key/value
  bytes. Folding raw bytes additively would be linear-weak; folding per-pair blake3
  digests makes the addends uniform random and the combine birthday-safe. This is
  acceptable for *trusted machine-owned* content (not adversarial input), which is the
  correct threat model here.
- **The molten fold MUST implement the inverse (subtract old pair digest) on key
  overwrite and delete.** This is the map analogue of `changed_words` and is **missing
  from the proposal.** Insert-of-existing-key without subtracting the old pair
  double-counts → corrupt identity. Additive supports it (subtract old, add new); state
  it as a rule.
- **Seal with pair count + domain at finalize** (so `{}` and any cancelling structure
  cannot masquerade; count defeats the empty/degenerate cases).

## (3) §3 zero-padding obligations — INCOMPLETE; two additions before it is a law

The listed obligations (alloc zero-fill; frame `Init::Zero`; variant-switch slack;
debug canary; facet-bridge canonicalizes at copy-in) are right but miss two:

- **Narrowing-field-overwrite slack.** A molten write of a value narrower than its slot
  (stage-4 writes `to_le_bytes()[..field.layout.size]`) leaves the high bytes as-was.
  If the slot previously held a wider value, the stale high bytes are non-canonical →
  mis-identity. The write-side contract must include **re-zero on every narrowing field
  overwrite**, not only enum variant-switch. Same defect class, broader trigger.
- **The debug canary must run in CI corpus runs unconditionally, not only under
  `debug_assertions`.** This is load-bearing: the force-copy and cross-lane differentials
  (§5) are **relative** — they catch *divergence between two paths*, not a **shared**
  zero-init bug that is wrong identically on both. A shared padding bug passes every
  differential while producing globally wrong identity (false cache hits = wrong builds,
  the existential failure). The canary is the **only absolute** guard; the cargo
  differential is a slow backstop. Cheap zero-check over declared padding ranges at
  intern — run it always in CI.

## (4) Q2 — variant-switch × carried record hasher — RESOLVE as the same algebra as Q1

A linear blake3 midstate supports **append only** (arrays); it cannot do the **in-place
field/variant update** records need (you cannot subtract a field from a sequential
midstate). So the record carried-hasher must use the **same additive-with-inverse
combine over per-field child identities** as the map combine — unify Q1 and Q2 into one
mechanism ("commutative combine with inverse over child identities" for all *mutable*
aggregates; keep the linear append-midstate for arrays only). A variant switch then =
subtract the old variant's field contributions, zero slack, add the new variant's — a
single atomic mutation, no double-count, no race. The proposal's "fold the whole variant
region atomically" is the right instinct but only realizable under a separable
(additive) record algebra; a sequential midstate would force a full record recompute.

## (5) §5 migration gates — SUFFICIENT with one oracle strengthening

The gate sequence and the three differentials are well-chosen; the first-vs-second-epoch
**structural check** (equivalence classes preserved across the break) is exactly right
and rare-to-see. One gap and one sequencing note:

- **The structural check must include INJECTIVITY** (distinct-under-epoch-1 →
  distinct-under-epoch-2), not only equality preservation. A false cache hit is a
  padding/combine **collision** (two distinct values → one hash); only injectivity over
  the corpus catches it. Run it to corpus exhaustion; it is the absolute identity guard
  the relative differentials cannot be.
- **Gate 3 (carried maps/records) is blocked on the Q1/Q2 algebra + gate 1 (flat
  memory).** The sequence already orders 1→3, but make it a **dependency**, not just an
  order: gate 3's force-copy differential cannot even be written until the fold algebra
  exists. The absolute correctness anchor (rodin-vs-cargo build differential) survives
  the epoch and remains the backstop against a wrong-identity build.

## (6) §6 cost model — HONEST on the arc, OPTIMISTIC on coverage

Genuinely honest where it counts: it states outright that §1–§5 do **not** alone reach
50×, names the three residual trunks (per-INVOKE memo floor, interp dispatch, JIT
precondition), and frames identity-as-field as the *precondition that makes the JIT win
real* rather than the whole answer. The arc is credible and grounded in landed facts
(tail-loop, JIT compile-cache).

One correction: the **29.1% map trunk collapse is contingent, not banked.** Per the
unifying finding, it only materializes once Q1 (commutative combine) + §3 (flat memory)
dissolve `changed_words` for nested children. The 23.4% (alloc flat-bytes) and 13.6%
(observation field-read) are robust; the 29.1% is the one gated on unresolved design.
**Re-flame after gates 1–2 to confirm the map trunk actually collapses before banking
it** — and note the arithmetic: removing 66% of flame is ~2.9× at best (Amdahl), so the
JIT + memo-granularity legs are not optional garnish, they are required to clear 50×.
The proposal says this; the charters must hold the line and not let "66% removed" be
read as "50× reached."

---

## Summary of binding rulings for the charters

1. Fold **stable post-canonical child identities**, never pre-intern words; make the
   carried-hasher generalization's soundness an **explicit dependency** on §3 + Q1, and
   add an array/map-**of-aggregates** fixture proving `changed_words` never fires.
2. Q1: **additive (not XOR)** commutative combine of per-pair `blake3(domain‖key_id‖
   value_id)`, **inverse-subtract on overwrite/delete**, count+domain sealed. Approve the
   `r[schema-identity.canonical-encoding]` change.
3. §3: add **narrowing-field-overwrite re-zero**; run the **padding canary in CI always**
   (the differentials are relative; the canary is the only absolute guard).
4. Q2 = Q1: one **additive-with-inverse** algebra over child identities for all mutable
   aggregates; linear append-midstate for arrays only.
5. §5: structural check must assert **injectivity**; gate 3 is a **dependency** on gate 1
   + the fold algebra.
6. §6: mark the **29.1% map win contingent**; re-flame after gates 1–2; hold the line
   that identity-as-field is the precondition, not the finish.

Existential framing is real, spine is sound, gaps are specific and resolvable — proceed,
with these folded into the implementation charters and re-gated per the first-epoch
discipline.
