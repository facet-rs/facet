# 50 — Conflict learning

When propagation (doc 40) hits a contradiction, the search does not merely
backtrack — it learns a **no-good**: a region of assignment space now known dead,
so the same dead-end is never re-explored (in this solve, and via the substrate,
future ones). This is what makes the search tractable. The learning is region-
based, not literal-based (not 2-watched-literal CDCL); the geometry is the point.

## Regions

A region is a box in assignment space: a version set per package (doc 30) plus a
set of feature polarities (feature → enabled/disabled). It names the assignments
that agree with all its bounds. The operations that matter:

- **contains(assignment)** — is a concrete assignment inside the box.
- **contains(region)** — subsumption: is one box ⊆ another (per-package subset,
  matching feature polarities). This is the containment lattice learning lives in.
- **intersect** — conjunction of boxes; empty (contradictory version set or
  opposing feature polarity) ⇒ disjoint.
- **subtract** — a disjoint cover of `A − B`. Deliberately *correct algebra, not a
  compact frontier* — it can fragment; correctness first.

## From conflict to a point region

A conflict carries its support: the premises (and the deciding hypothesis) that
jointly forced the contradiction. The learned region starts as the **point**: for
each package in the support, pin it to the exact version it currently holds. That
point region is precisely dead — that exact combination fails.

## Widening: the generalization move

Learning only the exact point is weak — it prunes one leaf. **Widening**
generalizes the point to the largest region still known dead *for the same
reason*, so one no-good prunes a whole swath. Widening must be **sound**: the
broader region is admissible only with evidence that every assignment in it fails
identically. Two evidence kinds:

- **declared structure** — the manifests in the support constrain the broader
  range by their declared dependency edges, so the whole range is dead a priori.
- **observed read-set** — the conflict's derivation only read certain fields; any
  assignment agreeing on those fields fails the same way, so the region widens to
  everything that agrees on the read-set.

The read-set justification is the machine's projection read-set (doc 90); the
manifest justification is manifest content. The *decision* — widen as far as the
evidence supports, keep the wider region if it covers strictly more — is
algorithm. The evidence bookkeeping (certificates, proof nodes, replay) is
substrate.

## Installing: a frontier of maximal dead regions

A new no-good is installed against the existing set by two-way containment:

- If an existing active region **contains** the new one, the new one is redundant
  — drop it.
- Any existing (locally-learned) region **contained by** the new one is now
  redundant — deactivate it.

The learned set stays a frontier of maximal dead regions — none subsumed by
another. Subsumption is region containment, never hash-equality: two textually
different regions where one contains the other are not independent facts.

## Reuse

Installed no-goods propagate on every subsequent node (doc 40): fully-entered ⇒
conflict; all-but-one-pinned ⇒ narrow the last domain by the complement. Across
solves, a learned no-good is a memoized value keyed by content — reused whenever
its inputs recur, never stale because identity is content (doc 90). rodin-core
serialized, carried, and re-verified "warm facts" by hand; that entire layer is
the substrate's memo + CAS.

## What is NOT part of this

Exclusion/derivation ids, the proof graph and replay, warm-fact bundles and
verification — all substrate (doc 90). Keep the geometry (regions, containment,
subtract), the point→widen→install strategy, and the soundness requirement on
widening.
