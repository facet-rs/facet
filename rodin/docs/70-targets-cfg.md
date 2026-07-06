# 70 — Targets and cfg gating

A dependency edge is not unconditional. It carries a **kind** (normal / build /
dev) and an optional **target predicate** (a `cfg(...)` expression or an explicit
target triple). Resolution happens in a **context** — a target triple plus the
set of edge kinds being consumed — and an edge's clause (doc 20) is active only
when the context consumes it. This is the consumption gate referenced in doc 20.

## The context

A resolve context fixes:

- the **target** — the platform being resolved for (triple: arch, os, env, plus
  the cfg atoms derived from it: `target_arch`, `target_os`, `target_family`,
  `unix`/`windows`, …).
- the **consumed edge kinds** — normal always; build for build-dependencies; dev
  only for the workspace roots (transitive dev-deps are not consumed). Oracle 2
  (doc 00) uses `-e normal,build`, so the default projection excludes dev.

## The gate

An edge's clause is active in a context iff:

1. the edge's **kind** is consumed by the context, and
2. the edge's **target predicate**, if any, matches the context's target.

`cfg(...)` matching is cargo domain truth: a cfg expression (`cfg(unix)`,
`cfg(target_arch = "wasm32")`, `cfg(not(all(...)))`, boolean combinations) is
evaluated against the target's cfg atoms. An edge with no predicate is
unconditional (subject only to kind). A corner case worth a fixture, from the
reference data: the *same* dependency can carry *different* cfg predicates across
versions of the parent (reqwest's `hyper` edge cfg tightened between 0.13.0 and
0.13.3) — so gating interacts with version selection, not just projection.

## Where gating applies

Gating is checked during propagation (doc 40, "apply clauses"): a gated clause
whose gate the current context does not consume is inert — its antecedents are
never evaluated and its consequent never fires. The same compiled clause set
resolves differently under different contexts, which is why Oracle 2 projects per
target.

## What is NOT part of this

The gate struct and the target-cfg representation. The cfg *evaluation semantics*
(matching a cfg expression against a target) are domain truth and stay; how the
predicate is stored does not. See 90.
