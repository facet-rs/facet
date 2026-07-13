# 20 — Dependencies as constraints

Resolution is a constraint problem. Manifests and their dependency edges compile
into clauses; the search (doc 40) narrows package domains until every clause is
satisfied or a conflict is learned (doc 50). This doc fixes what a constraint
*means*.

## Atoms

An atom is an elementary proposition about the resolved graph:

- **in-graph(pkg)** — this package identity (doc 10) is present in the resolved
  graph at all.
- **version(pkg, set)** — pkg's selected version lies in this version set (doc
  30).
- **feature(pkg, feat)** — this feature of pkg is enabled.

Atoms are stated over package *identities*, so `serde@1` and `serde@2` carry
independent atoms.

## Clauses

A clause is `antecedents ⇒ consequent`: a conjunction of atoms implying one
atom. It is a compiled dependency edge — "if the parent is in-graph and the
edge's conditions hold, then the dependency's version lies in the required set /
the dependency is in-graph / a feature is enabled."

The consequent is optional. A clause with no consequent is a pure exclusion: its
antecedents cannot all hold at once — a stated impossibility.

## The consumption gate

A dependency edge is active only in some contexts: it is gated by the *kind* of
edge (normal / build / dev) and by an optional target predicate (cfg/target, doc
70), relative to the consuming parent. The gate decides whether a clause applies
in the current context before its antecedents are evaluated. (Confirm the
precise gate inputs while distilling the search — it is the per-context
consumption predicate the compiled edge carries.)

## What is NOT part of this

Clause ids, and the by-package / by-feature indexes rodin-core kept for "which
clauses mention pkg X." That lookup is a query over the clause set — a memoized
projection in vix, not a maintained index. Keep the clause *meaning*; drop the
store. See 90.
