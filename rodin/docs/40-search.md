# 40 — The search

Resolution is CSP backtracking search: narrow package domains by propagation to a
fixpoint, pick an undecided package, hypothesize a version, recurse; on conflict,
learn a no-good (doc 50) and backtrack. cargo is the oracle for the *result*; this
doc fixes the *strategy* rodin-core discovered — not its bookkeeping.

## What the search state must track

At each node, per active package domain:

- **active** — is this package in the resolved graph.
- **allowed** — the version set still permitted (doc 30), narrowed as constraints
  apply.
- **selected** — the chosen version, once decided or forced (optional).
- **support / reasons** — the provenance (which premises narrowed this domain),
  used only to explain conflicts. In vix this is the demand trace, not a field you
  maintain — see 90.

Across the node: which features are enabled (with provenance), the hypotheses
(decisions) taken to reach here, and which clauses have already been applied. This
state is threaded persistently: each narrowing produces a new state value with
structural sharing. rodin-core `clone()`d it per branch; in vix a branch is just a
new value over the same store — the clone is free.

## Seeding

From the problem's roots (and any user decrees): resolve each requirement to a
package identity, activate it (in-graph), narrow its domain by the requirement,
and enable its default + named features. Mutually-exclusive feature declarations
become pairwise exclusion constraints. This is the initial state the search
narrows from.

## Propagate to a fixpoint

Repeat until a full pass changes nothing:

1. **Apply learned no-goods** (doc 50): for each dead region, compare the current
   selection against it — fully inside ⇒ conflict; all-but-one dimension pinned
   inside ⇒ narrow the last free domain by the region's complement (unit
   propagation over regions).
2. **Force singletons**: an active domain whose allowed set has exactly one
   candidate is selected (a forced pick); an active domain with *no* candidate is
   a conflict.
3. **Apply clauses**: for each active, not-yet-applied clause whose gate is active
   and whose antecedents all hold, apply its consequent — activate a package,
   intersect a domain with a version set, or enable a feature. (Enabling a feature
   does not by itself pull a package into the graph; only an in-graph edge does.)
4. **Check feature exclusions**: two mutually-exclusive features both enabled ⇒
   conflict.

The fixpoint is the state being unchanged across a full pass. Every narrowing
carries the premise that caused it, so a conflict can name its support.

## Decide, recurse, backtrack

After propagation reaches a fixpoint with no conflict:

- If no package remains undecided → the state is a solution.
- Otherwise pick the next undecided package and enumerate its candidate versions
  **highest-first**. cargo prefers the highest admissible version, so the first
  solution found under highest-first ordering matches cargo.
- For each candidate: hypothesize it (record a decision, set selected) and recurse
  on the branch. A returned solution propagates up; a backtrack tries the next
  candidate. Exhausting candidates backtracks to the parent.
- An empty candidate set at decision time is a conflict (learn, backtrack).

A learned no-good may request a **restart** (discard the current search tree and
re-propagate from the seed with the enlarged learned set) when it is broad enough
to reshape early decisions. Restart vs. plain backtrack is a heuristic.

## What is NOT part of this

Decision/conflict counters, clone-per-branch, the applied-clause set as a
maintained structure, and the by-package clause / no-good indexes are bookkeeping
the machine subsumes (persistent state, memoized queries, projection). Keep the
*strategy*: propagate-to-fixpoint, highest-first candidates, hypothesize-recurse-
backtrack, learn-on-conflict. See 90.

## Open questions (decide against cargo, by measurement)

- **Package selection order.** The exact "next package" heuristic (most-
  constrained-first? activation order?) affects search size, not correctness.
  Pick what matches cargo's lockfiles on fixtures; treat speed as a measurement.
- **Restart policy.** When a learned no-good triggers a restart vs. a local
  backtrack. Correctness-neutral; tune against fixtures.
