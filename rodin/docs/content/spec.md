+++
title = "Solver specification"
weight = 1
+++

This is the normative specification for Rodin. Rodin is a pure Vix function
from a resolved package index, roots, feature requests, target, and policy to a
selection or typed conflict. Runtime memoization, receipts, persistence, and
placement accelerate it without changing solver semantics.

## Oracle and determinism

> r[solver.oracle.cargo]
>
> For Cargo-domain inputs, the fixture harness MUST emit a real offline Cargo
> workspace, run Cargo's resolver, demand Rodin on the same modeled input, and
> compare per-target selected package/version/features. Fixtures are the domain
> oracle; hand-written expected maps are insufficient when Cargo can answer.

> r[solver.result.deterministic]
>
> One input value has one Rodin result. Map iteration, stream arrival, hash
> table order, executor count, placement, and learned-region insertion order
> cannot affect selection. All ties use structural order on domain values.

## Identities and modeled input

Package identity is `(source identity, canonical package name)`, not only a
name. A source identity includes registry/index identity, git repository plus
precise revision, or canonical path/workspace member identity. Versions from
different package identities never share a domain.

A version is full SemVer: major/minor/patch, prerelease identifiers, and build
metadata. Precedence follows SemVer exactly. Build metadata does not affect
precedence; equality and Cargo matching behavior are pinned by oracle fixtures.

> r[solver.version.prerelease]
>
> Prerelease admission matches Cargo: a prerelease is admitted only when a
> comparator in the requirement carries a prerelease for the same
> major/minor/patch release line. Interval endpoints alone are insufficient;
> admission metadata is part of the normalized `VersionSet` value.

`VersionSet` is a canonical normalized union of half-open precedence intervals
plus prerelease-admission constraints. It supports `contains`, `union`,
`intersect`, `complement`, `is_subset_of`, `exact`, `empty`, and `universe`.
Caret, tilde, wildcard, exact, and comparison requirements lower to this algebra
and are differentially checked against Cargo.

The package index is a content-addressed value. Rows expose package identity,
version, dependency clauses, features, links/native-library constraints,
target/cfg guards, yanked/publish metadata, and source coordinates. Index access
is lazy by package row and field; a solve does not fetch or decode unrelated
packages.

## Constraints and state

Each dependency edge is a clause with:

- parent package/version and dependency kind;
- target/cfg gate;
- optional feature gate;
- child package identity and version requirement;
- default-feature and requested-feature effects;
- provenance sufficient for a diagnostic and learned support.

Per package, search state contains `active`, normalized allowed `VersionSet`, an
optional selected version, enabled features, and support/reasons. Global state
contains decisions and the active learned-region frontier. Applied-clause and
index acceleration are implementation details, not semantic fields.

State is immutable at the Vix plane. A solve island may keep unique working
state molten and publish only stable states/results under the runtime as-if law.

## Feature semantics

Features unify per package identity across every incoming edge. Enabling a
feature activates its declared feature members; optional dependencies become
edges only when their enabling feature is active. `dep:foo`, `foo/bar`, and
weak `foo?/bar` follow Cargo's rules. Default features participate only when an
active edge requests them. Target-specific inactive edges do not contribute
features.

Mutually exclusive feature policy is an explicit input constraint and yields a
typed conflict when violated; it is not inferred from Cargo metadata.

## Target and cfg

The target is an explicit solve input. Cfg expressions are parsed into typed
boolean structure and evaluated against a target fact set supplied with that
input. Executor host facts never enter cfg evaluation. A cross-compile solve is
therefore independent of the physical machine performing it.

Build, normal, and development dependency kinds remain typed and are enabled by
the requested solve mode. Host/build-dependency target semantics are modeled
explicitly rather than collapsed into one `Target::host()` query.

## Propagation

Propagation repeats to a fixpoint:

1. apply learned regions, including unit-style narrowing when all but one
   dimension is pinned inside a dead region;
2. conflict on an active empty domain and force active singleton domains;
3. apply newly enabled dependency/feature clauses;
4. check explicit feature and links/native-library exclusions.

Every narrowing carries typed support. Equality of the complete state ends the
fixpoint; counters or mutation epochs are not semantic termination criteria.

An early-exit `try_fold` is an implementation idiom for propagation. Returning a
typed conflict stops the fold without inventing mutable control flow.

## Decision and backtracking order

> r[solver.search.package-order]
>
> After propagation, select the active undecided package with the fewest
> currently admissible candidate versions. Ties use structural `PkgId` order.
> Enumerate candidates by descending SemVer precedence, with structural version
> order as the final tie-break. This deterministic rule replaces activation or
> hash-map order.

For each candidate, add a decision, propagate, and recurse. The first solution
under the fixed ordering is the result. Exhausting candidates returns a conflict
to the parent. No mutable trail is semantically required because branches are
fresh persistent values with structural sharing.

> r[solver.search.restart]
>
> The initial solver performs chronological backtracking with learned regions
> and no heuristic restarts. A future restart policy is an optimization only:
> it must preserve the fixed decision/candidate order and therefore the selected
> result, and must pass the same oracle/determinism fixtures before becoming a
> default.

## Region-based conflict learning

> r[solver.learning.no-good]
>
> Every propagated contradiction learns a no-good region before backtracking.
> The same dead assignment is not re-explored within a solve.

A region is a box in assignment space: a `VersionSet` per involved package plus
feature polarities. It supports assignment/region containment, intersection,
and subtract as a disjoint cover. Fragmentation is acceptable; unsound
compactness is not.

The initial learned region is the exact decision/support point. It may widen
only with evidence that every assignment in the larger region fails identically:
declared clause structure or the demanded derivation's exposed read-set. The
runtime receipt is the reuse certificate; Rodin's proof value carries the
domain explanation.

The learned set is a frontier of maximal dead regions. A new region is dropped
if contained by an active region and removes active regions it contains.
Containment, not hash equality or insertion order, defines subsumption.

## Read-set widening and explanation

> r[solver.learning.widen.read-set]
>
> Rodin MAY demand the receipt/read-set of a propagation or conflict demand and
> widen a point no-good across versions that agree on every field actually read.
> The widening operation itself is a Vix function and MUST verify the proposed
> region against that read-set. If no certificate is demanded, Rodin falls back
> to declared-structure widening; it never guesses.

Human-facing “why selected?” and “why conflict?” answers are explicit Rodin proof
values built from clause support, decisions, propagation, and learned regions.
Generic runtime traces may explain scheduling or cache behavior but do not
replace domain proofs.

## Persistence and incrementality

Solver states, learned regions, proofs, and results are ordinary Vix values.
Exact and projection memo entries use the runtime's claim index and receipt
verification. Rodin has no private warm-fact cache, serializer, interner, or
read-tracking system.

Changing an index row or manifest field re-demands only computations whose
receipts observed it. Cross-process reuse is accepted only after the runtime's
claim/trust policy and receipt verification. Learned facts may be evicted and
recomputed; their identity does not expire.

## Fixture corpus

The standing corpus covers at least:

- caret/tilde/wildcard/exact/comparator boundaries, including zero-major;
- prerelease admission and build metadata;
- coexistence of incompatible compatibility classes;
- backtracking, conflict learning, widening, and deterministic tie breaks;
- registry, git, path, replacement/patch source identities;
- feature diamonds, defaults, optional dependencies, weak dependency features;
- target/cfg and host/build dependency distinctions;
- links/native-library conflicts, yanked versions, and policy constraints;
- lazy index access and warm-restart blast radius.

Each new Cargo-domain discrepancy becomes a minimized fixture before Rodin's
rule changes. Performance counters may tune implementation policy but cannot
rewrite a result to match an oracle after the fact.
