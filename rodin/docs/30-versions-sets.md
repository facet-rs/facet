# 30 — Versions and version sets

## Versions

A version is a **full semver value**: `major.minor.patch`, an optional
**prerelease** (a dot-separated sequence of identifiers), and optional **build**
metadata. All of it is modeled — prerelease is not deferred; cargo resolves
prereleases, so we do too. A version is a records-at-offsets vix value; parsing
(once, memoized) and comparison are vix. `.major` is a field read.

### Precedence (semver, exact)

Compare `major`, then `minor`, then `patch` numerically. If those are equal:

- a version **with** a prerelease has **lower** precedence than the same version
  without one (`1.2.3-alpha` < `1.2.3`);
- two prereleases compare identifier by identifier, left to right:
  - a purely numeric identifier compares numerically;
  - an alphanumeric identifier compares by ASCII order;
  - a numeric identifier is always **lower** than an alphanumeric one;
  - if every shared identifier is equal, the version with **more** identifiers is
    higher (`1.2.3-alpha` < `1.2.3-alpha.1`).

**Build metadata does not affect precedence.** Its role in equality is cargo
domain truth — pin `1.2.3+a` vs `1.2.3+b` against the oracle.

## Version sets

A version set is interval algebra — a normalized union of half-open
`[lower, upper)` intervals over the precedence order, canonical so the store
content-addresses it. Operations: `contains` / `union` / `intersect` /
`complement` / `is_subset_of` / `exact` / `empty` / `universe`.

The total order **includes prereleases**, so the model carries cargo's
**prerelease admission** rule — not a release-only approximation:

- cargo admits a prerelease `M.m.p-pre` into a requirement only when some
  comparator in that requirement pins the **same** `M.m.p` **and itself carries a
  prerelease**. `^1.2.3` does *not* admit `2.0.0-alpha` (even though
  `2.0.0-alpha < 2.0.0`); `>=1.2.3-alpha` *does* admit `1.2.3-beta`.
- so `exact(v)` is the true prerelease-aware singleton `{v}`, not the release-only
  shorthand `[v, next_patch(v))`; and the caret/tilde/comparator intervals carry
  admission at their prerelease-bearing bounds.

The exact admission behaviour is cargo domain truth: **model it fully and pin
every corner against `cargo generate-lockfile` / `cargo tree` fixtures** (doc 00).
That is validation against the oracle — not deferral of the capability.

## Requirements → sets (cargo's caret/tilde/wildcard truth)

A cargo `VersionReq` is a conjunction of comparators; each maps to an interval
(carrying prerelease admission per the rule above) and the requirement's set is
their intersection. Release-boundary rules, which must match cargo exactly:

- `=x` / wildcard — upper bound is the next unfixed component (`=1` ⇒ [1.0.0,
  2.0.0); `=1.2` ⇒ [1.2.0, 1.3.0); `=1.2.3` ⇒ the singleton).
- `>x` `>=x` `<x` `<=x` — half-lines with the same component rollover.
- `~x` (tilde) — [base, next-minor) if minor given, else [base, next-major).
- `^x` (caret) — upper bound at the next incompatible version: next major if
  major>0; else next minor if minor>0; else next patch — the compat-class
  boundary from doc 10.

## What is NOT part of this

The backing container and any interning of sets. The machine stores and
content-addresses the set value; normalization gives canonicality for free. See
90.
