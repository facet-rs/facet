+++
title = "solver: conflict learning"
+++

Normative rules for rodin's region-based conflict learning. Distillation and
rationale: `rodin/docs/50-conflict-learning.md` (and `40-search.md` for the
surrounding search). The geometry is the point: learning is region-based,
not literal-based (not 2-watched-literal CDCL) — a researched decision.

r[solver.learning.no-good]

When propagation reaches a contradiction, the search MUST learn a no-good —
a region of assignment space known dead — not merely backtrack. The same
dead-end is never re-explored within a solve.

r[solver.learning.region]

A region is a box in assignment space: a version set per package plus a set
of feature polarities. Regions support `contains(assignment)`,
`contains(region)` (subsumption), `intersect` (empty ⇒ disjoint), and
`subtract` (a disjoint cover of `A − B`; correct algebra over compactness —
fragmentation is acceptable, unsoundness is not).

r[solver.learning.point]

A learned region starts as the point region: each package in the conflict's
support pinned to the exact version it holds. The point is precisely dead by
construction.

r[solver.learning.widen.sound]

Widening generalizes a point region to a larger region ONLY with evidence
that every assignment in it fails identically. Admissible evidence: declared
structure (manifest dependency edges constrain the range a priori) or the
observed read-set (any assignment agreeing on the fields the derivation read
fails the same way). Unsound widening is forbidden regardless of how much it
prunes.

r[solver.learning.frontier]

A new no-good is installed by two-way containment: dropped if an existing
active region contains it; deactivating any learned region it contains. The
learned set remains a frontier of maximal dead regions. Subsumption is region
containment, never hash-equality.

r[solver.learning.propagate]

Installed no-goods propagate on every subsequent node: a selection fully
inside a dead region is a conflict; all-but-one dimension pinned inside
narrows the last free domain by the region's complement (unit propagation
over regions).

r[solver.learning.reuse]

Across solves, a learned no-good is a memoized value keyed by content —
reused whenever its inputs recur. No hand-rolled warm-fact serialization or
verification: reuse soundness is the substrate's memo verification.
