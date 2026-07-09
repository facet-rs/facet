+++
title = "Building a solver"
weight = 90
+++

*Status: provisional — this chapter follows the design of rodin, the real
solver, whose specification lives at [/rodin](/rodin).*

You want to build a dependency solver. Given manifests and a package
index, pick one version per package so every requirement holds — an
NP-hard search problem with a folklore reputation: you'll need a
propagation engine, a backtracking scheme with undo machinery, conflict
learning if you want it fast, a persistence layer if you want it warm
across runs, and cache invalidation if you want the persistence to not
lie. In most stacks that's an engine project before it's a solver.

This chapter builds one in vix, and the point of the chapter is what you
*don't* build.

## The world as demanded values

A solver consumes an index: which versions of which packages exist, and
what each depends on. The index is enormous and remote — and that's fine,
because describing it costs nothing:

```vix
fn index_row(registry: Registry, pkg: PkgName, version: Version) -> Row {
    parse_row(fetch(registry.row_url(pkg, version)))
}
```

Nothing is fetched here. Rows are fetched when the search demands them —
and only the rows the search actually visits. A solve that touches 200
packages of a 150,000-package index performs 200 fetches, memoized, and
you wrote no cache: results of `fetch` and `parse_row` are values, and
values are cached by the machine under their identity. The second solve
of the day fetches nothing at all.

## Solver state is a value

```vix
struct State {
    domains:  Map<PkgId, VersionSet>,   // versions still possible, per package
    selected: Map<PkgId, Version>,      // decisions made so far
    learned:  Multiset<NoGood>,         // conflicts we've understood
}
```

Propagation shrinks domains until nothing changes or a contradiction
appears:

```vix
enum Step { Pass(State), Conflict(NoGood) }

fn propagate(state: State, row: Row) -> Step {
    row.deps.values().fold_ascending(Step::Pass(state), |step, dep| {
        match step {
            Step::Conflict(ng) => Step::Conflict(ng),
            Step::Pass(s) => {
                let narrowed = s.domains[dep.pkg].intersect(dep.req);
                if narrowed.is_empty() {
                    Step::Conflict(no_good_for(s, dep))
                } else {
                    Step::Pass(State {
                        domains: s.domains.insert(dep.pkg, narrowed),
                        ..s
                    })
                }
            },
        }
    })
}
```

Ordinary code: a fold, a map insert, a record spread. Each step denotes a
fresh `State`. Hold that thought.

## Backtracking is free

Here's where the folklore engine evaporates. Search tries a candidate,
descends, and — on conflict — must *undo* everything the failed branch
did. Imperative solvers maintain trails: logs of every mutation, replayed
backwards on backtrack, with unit-tested undo for every kind of change.
It's the most delicate code in any CDCL engine.

```vix
fn search(state: State) -> Outcome {
    match pick_undecided(state) {
        None => Outcome::Solved(state.selected),
        Some(pkg) => try_candidates(state, pkg, candidates(state, pkg)),
    }
}

fn try_candidates(state: State, pkg: PkgId, cands: [Version]) -> Outcome {
    match cands.split_last() {
        None => Outcome::Exhausted(no_good_for_exhaustion(state, pkg)),
        Some((v, rest)) => match attempt(state, pkg, v) {
            Outcome::Solved(sel) => Outcome::Solved(sel),
            Outcome::Exhausted(ng) =>
                try_candidates(learn(state, ng), pkg, rest),
            //              ^^^^^^^^^^^^^^^^^ backtracking is THIS:
            //              we still have `state`. The failed branch's
            //              hundred domain-narrowings belong to a value
            //              we simply stopped using.
        },
    }
}
```

There is no trail. There is no undo. `attempt` built new states from
`state`; when the branch fails, `state` — untouched, because nothing
touches anything — is right there to try the next candidate from. The
entire undo subsystem of a classical solver exists to *simulate*, with
mutation plus bookkeeping, what a by-value language just has.

> **"But surely copying all that state is slower than a trail."** At the
> semantic plane there is no copying — there are values. At the
> implementation plane, the machine mutates in place whenever it can
> prove you kept no other reference, shares structure when you did, and
> recomputes when that's cheaper — per the as-if law, invisibly. The
> classical trail is one point in that space; the machine is free to land
> on it. You just don't have to *write* it.

## Learning is just values

A `NoGood` — "these choices can never coexist, and here's why" — is a
record. Learning is `state.learned` growing through the fold above;
consulting learned facts is propagation reading them. No arena, no clause
database with reference counting, no watch lists to maintain under undo
(the states they'd be undone *to* still exist).

And warm restarts — reusing learned facts across solves, the feature that
turns nightly resolution from minutes to milliseconds — is the machine's
memoization applied to the derivations that produced the facts. A learned
fact rests on what it read (index rows, and crucially *absences* — "no
version of X in range Y exists", recorded as first-class observations).
Tomorrow, facts whose reads still hold are reusable; facts invalidated by
a new publish simply fail re-verification and are re-derived. You build
none of this. The persistence chapter of a solver README is, here, a
property of the substrate. The details are in
[the three planes of identity](/vix/three-planes).

## Incremental for free, exact by construction

Someone bumps one requirement in one manifest. What re-runs? Exactly the
demands whose inputs changed: the affected package's clauses, the
propagation that read them, the search regions that depended on those
domains. The unaffected 90% of yesterday's solve — including its fetches,
parses, and learned facts — are cache hits *proven* by recorded reads,
not guessed by timestamps. You did not design an incremental solver. You
wrote `solve`, and incrementality is what evaluation means here.

## What you didn't build

A fetch cache. A parse cache. An undo trail. A clause database. A
persistence format for learned facts. An invalidation scheme for that
persistence. An incremental-recompute engine. A parallelism strategy
(independent branches and independent fetches parallelize under the
machine's scheduling — by-value state has no locks to take).

What you did build: the actual solver — domains, propagation, selection,
learning. About fifteen hundred lines in the real one. It's called rodin,
it resolves real Cargo workspaces against the real crates.io index and
matches `cargo tree` on the fixtures that count, and its specification is
at [/rodin](/rodin). This chapter is its postcard.

> **"Surely it doesn't fit in thirty lines."** It doesn't, and the gap is
> instructive. The *kernel* — pick, propagate, learn, backtrack — really
> is about the size shown on this page, in the real source too. The other
> fourteen hundred lines are not solver machinery; they are Cargo's
> semantics: feature resolution (feature keys and enables, `dep:` syntax,
> weak optional dependencies, default-feature unification — the single
> biggest chunk), target gating (`cfg(...)` expressions parsed and
> evaluated against per-target compiler facts), sparse-index decoding
> (JSONL rows, yanked versions, renames), and semver set algebra. That
> dirt would be the same size in any language — what vix removed is the
> *engine*: the caches, the trail, the persistence, the invalidation. The
> domain remains yours to model, which is the correct division of labor.
