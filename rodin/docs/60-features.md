# 60 — Features

Features are the intricate part of cargo resolution. A feature is a named,
per-package boolean; enabling it can pull in optional dependencies, enable
features on dependencies, and enable other features of the same package.
Resolution accumulates enabled features monotonically and **unifies** them: a
dependency ends up with the union of every feature any edge requested of it. This
doc fixes the feature semantics as clause meaning (doc 20), not as tables.

## Feature atoms and unification

A feature atom is `feature(pkg, name)` over a package *identity* (doc 10).
Enabling is monotone — features are only ever turned on during propagation, never
off — so if two edges each enable a different feature of the same dependency, the
dependency gets both. That monotone accumulation *is* cargo's feature unification;
there is nothing extra to implement for it.

Enabling a feature does **not** pull its package into the graph. Only an in-graph
edge (a dependency) does. A feature-enable on a package no edge has activated is
inert — harmlessly recorded, and it applies if some later edge activates the
package.

## Scopes: normal / build / dev

Features are resolved separately per dependency scope. A feature carries a scope
qualifier (normal, build, dev), so `default` for a normal dep and `default` for a
build dep are distinct atoms. This is cargo's resolver-v2 behavior: build-
dependency features do not unify with normal-dependency features. Compile each
feature relationship once per scope.

## How manifests become feature clauses

For a package version's manifest:

- **A non-optional dependency** compiles to: pull the dep in-graph, narrow it to
  the requirement, and enable the dep's requested features — its `default` unless
  the edge turned default-features off, plus any features the edge names. Each is
  guarded by the parent being in-graph and selected at this version.

- **An optional dependency** implicitly defines a feature of the same name that
  activates it (cargo's rule) — *unless* the manifest explicitly references the
  dependency as `dep:name` somewhere, which suppresses the implicit feature.

- **The `[features]` table**: each `feature = [enables…]` becomes, per scope, a
  clause per enable string, guarded by `feature(parent, feature) ∧
  selected(parent, v)`. The enable string has four forms:

  - `dep:name` — activate the (optional) dependency: pull it in-graph, narrow it,
    enable its requested features. No same-named feature is implied.
  - `name/feat` (**strong**) — activate the dependency *and* enable its `feat`.
  - `name?/feat` (**weak**) — enable the dependency's `feat` *only if the
    dependency is already in-graph*; do not pull it in. The weakness is an extra
    `in-graph(dep)` guard on the clause.
  - `feat` (plain) — enable another feature `feat` of the *same* package.

- **`default`** is just the feature named `default`; enabling it is how default
  features turn on, and an edge with default-features off omits the clause that
  would enable it.

## What is NOT part of this

Feature interning (`FeatIx`), the clause store and its by-feature index, scope
string formatting. Feature identity is content-addressed like everything else; the
"which clauses mention this feature" lookup is a memoized query. Keep the four
enable-form semantics, scope separation, monotone unification, and the "features
don't activate packages" rule. See 90.
