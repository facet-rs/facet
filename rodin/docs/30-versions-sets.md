# 30 — Versions and version sets

## Versions

A version is a semver point — major.minor.patch plus prerelease and build tags —
ordered by semver precedence. Resolution needs three things from a version: its
ordering (to compare and to bound intervals), its components (major/minor/patch,
for the compat-class rule in doc 10), and equality. All three are properties of
the *value*; none requires parsing at use. A version is parsed once (a memoized
demand) and thereafter read structurally. Do not store a version as its display
string and re-parse it per access — see 90.

## Version sets are interval algebra

A version set is a set of versions represented as a normalized union of
half-open intervals `[lower, upper)`, each bound being either a version or
unbounded. Normalized means: intervals sorted by lower bound, with touching or
overlapping intervals merged — so the representation is canonical, equal sets are
structurally equal, and the store content-addresses them for free (no
hand-rolled canonical form).

The operations resolution uses, as meaning:

- **contains(v)** — is a version in the set.
- **union / intersect** — set algebra (interval intersect is max-lower /
  min-upper).
- **complement (relative to a universe)** — the versions in the universe not in
  the set; expresses "everything the requirement excludes."
- **is_subset_of** — `self ∩ other == self`; the containment test conflict
  learning leans on (doc 50).
- **exact(v)** — the singleton `[v, next_patch(v))`.
- **empty / universe** — lattice bottom and top.

## Requirements → sets (cargo's caret/tilde/wildcard truth)

A cargo `VersionReq` is a conjunction of comparators; each comparator maps to an
interval and the requirement's set is their intersection. These rules are cargo
domain truth and must match exactly:

- `=x` / wildcard — upper bound is the next unfixed component (`=1` ⇒ [1.0.0,
  2.0.0); `=1.2` ⇒ [1.2.0, 1.3.0); `=1.2.3` ⇒ [1.2.3, 1.2.4)).
- `>x` `>=x` `<x` `<=x` — half-lines, same component-rollover on the open side.
- `~x` (tilde) — [base, next-minor) if minor given, else [base, next-major).
- `^x` (caret) — upper bound at the next *incompatible* version: next major if
  major>0; else next minor if minor>0; else next patch. This is exactly the
  compat-class boundary from doc 10, expressed as an interval.

## The prerelease boundary (known modeling gap)

This interval model is exact for **release-only** version universes. cargo's
prerelease admission policy is *not* a finite union of plain half-open intervals
over semver's total order: an admitted release range implies infinitely many
excluded prerelease points above it. rodin-core deferred this ("v2," via a
per-interval admission flag or an ordering trick) and so do we — but it is an
explicit open problem, not a silent drop. Adding prerelease changes the *set
representation*, so decide it before the set type ossifies.

## What is NOT part of this

The `Vec<Interval>` backing and any interning of sets. The machine stores and
content-addresses the set value; normalization gives canonicality for free. No
canonical-bytes step. See 90.
