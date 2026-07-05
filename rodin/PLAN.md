# rodin.vix — the plan (read this first)

Porting Rodin (the cargo-shaped version resolver) into the vix language. This
file is the single entry point; read it before touching anything.

## Mission

Reimplement Rodin **entirely in vix**, running on the vix demand machine (memo /
demand / incremental / content-addressing for free). This vix implementation is
the **only production Rodin**. Grow the vix language surface wherever the port
needs it — that IS the work, not a detour.

## Non-negotiable doctrine (each one a hard lesson from the session that made this)

- **cargo is THE oracle.** Ground truth for resolution = `cargo tree --target`,
  `cargo generate-lockfile`. Not rodin-core.
- **rodin-core (in vixenware/vixen: crates/rodin-core, rodin-facts, rodin-survey)
  is the Rust REFERENCE to port FROM and diff against — not production.** Do NOT
  optimize/benchmark/extend it. `vix-solve` (PubGrub) is dead. See the vixenware
  repo-root `CLAUDE.md`.
- **Faithful but functional, and COMPLETE.** rodin-core is imperative/`&mut`;
  vix is persistent `State` threaded through recursion (the recasting is already
  written: `docs/design/rodin-pure-kernel.md` §6, on branch
  `rodin-pure-kernel-design` / in vixenware). Port the WHOLE model. Do NOT build
  a 10% "version-only" toy and bolt features on later — that shape won't hold.
- **No god `Value` enum.** Values are schema-laid-out bytes (records-at-offsets,
  enums-as-tag+variants). Type is static.
- **Where a type is declared: `vix/docs/where-values-are-declared.md`.** Declare
  in Rust iff Rust code manipulates it (facet bridges to vix); else declare in
  vix. Content-addressing is FREE for any vix value — never hand-roll
  `canonical_bytes`. `Region`/`State`/`Domain`/`LearnedNoGood` are vix-declared.
  Only `Version`/`VersionSet` are Rust host primitives, and even their Rust
  justification is weak (canonical-bytes is free) — the sole open question is
  whether interval-algebra throughput needs them in Rust; that's a MEASUREMENT,
  default vix.
- **Don't manufacture side-structures.** No new crates, no new worktrees, no
  playground samples for real components. Work in `~/oss/facet-cc` (it is already
  the facet worktree). rodin.vix lives in the facet repo at `rodin/`.
- **Never leave work unpushed.** (This session lost nothing only because we
  caught 130 unpushed commits in time.) Commit and push often.

## Where everything is

- Branch: **`rodin`** off `origin/main` (facet, github.com/facet-rs/facet), in
  `~/oss/facet-cc`. Has the `Version`/`VersionSet`/flesh primitives.
- Resolver source: **`rodin/rodin.vix`**.
- Differential harness: **`vix/tests/rodin.rs`** — `Machine::load(source)` +
  `machine.demand_i64("main", args)`. Run: `cargo nextest run -p vix --test rodin`.
- Blueprint (the algorithm, functionally recast, with cited line numbers):
  `docs/design/rodin-pure-kernel.md` (vixenware `rodin-pure-kernel-design`).
- Rust reference: vixenware/vixen `crates/rodin-core/src/lib.rs`,
  `crates/rodin-facts/src/{interner,region,version_set}.rs`.
- The superseded oracle line is archived at `origin/snark-playground-rebased` —
  do NOT merge it (main is ~9k lines ahead; that branch is the interpret-only
  oracle main already replaced with the machine).

## Package identity (locked)

`PackageId { source: Source, name: String, compat: Option<CompatClass> }` — a
composite, NOT a bare name. `serde@1` + `serde@2` cohabitate (compat class);
crates.io / git / path are distinct provenances (source). This mirrors
rodin-facts `PkgId (source, name, compat)`. `CompatClass::from_version`: major!=0
→ Major(major); else minor!=0 → Minor(minor); else Patch(patch). (Modeled as a
3-variant enum in vix to avoid Option until it lands — see gaps.)

## The build loop

Write vix → `cargo nextest run -p vix --test rodin` → the machine reports the
next missing surface → add it to the machine (`vix/src/machine/{lower,driver}.rs`)
or model around it → repeat. Every resolver function is checked against **cargo**
(fixtures mirroring small workspaces) and **rodin-core** (same inputs, same
outputs).

## Surface gaps to add, IN ORDER (documented in rodin.vix header)

1. **`Version` accessors** `.major/.minor/.patch` — the current runtime blocker.
   `version::parse_bytes` (vix/src/machine/version.rs) already yields a
   `semver::Version`; wire a host accessor: `lower.rs` accepts field/method
   access on a `Version` value, `driver.rs` executes it → `Int`. Template for
   host-value ops: the `VersionSet` handling in driver.rs (~6609) / lower.rs
   (~557). `compat_of` goes green the moment this lands → un-`#[ignore]`
   `vix/tests/rodin.rs`.
2. **`Option` as a prelude enum + generic enum monomorphization.** Today Option
   has no user construction/matching (only `.unwrap()`), and generic enums don't
   lower ("expected Option, got Option<Int>, machine slice-2 subset"). Option is
   pervasive in the resolver (`Domain.selected`, `Gate.target`, `PackageId.compat`)
   — this is the biggest surface piece. Add generic enum lowering, put
   `enum Option<T> { None, Some(T) }` in a prelude.
3. **`Set` type** (has Map/Array). Currently modeled as `Map<K, Bool>`. Decide
   whether a first-class `Set` is worth it or `Map<K,Unit>` stands.
4. **`VersionSet` op exposure to vix** — `contains`/`intersect`/`union`/
   `complement`/`is_subset_of`/`from_req` must be callable from vix (the Rust
   methods exist in version_set.rs; confirm/expose the vix-facing dispatch).

## The algorithm, after the surface (blueprint §6, faithful)

`solve → seed_problem → search → try_candidates → propagate (fixpoint) →
fold_clauses → learn → install_learned_fact`. CSP-style domain narrowing over
per-package `VersionSet` intervals with **region** conflict learning (NOT
2-watched-literal CDCL). `State` = persistent record (`domains: Map<PackageId,
Domain>`, features, hypotheses, applied). Reference workload: 457 decisions, 69
conflicts. Combinators the kernel wants (from §6): a try-fold / early-abort-fold
(4 call sites) — explicit recursion works, sugar optional.

Build order after surface: version core (domains + propagate narrowing + backtrack,
matching cargo on a 2-crate fixture) → region learning → features → cfg/targets.
Each is a full slice of the real model, cargo-checked — not a throwaway subset.

## Current state

- `rodin/rodin.vix`: full type surface compiles through the machine; `compat_of`
  (CompatClass::from_version) lowers faithfully. Committed + pushed (`origin/rodin`).
- `vix/tests/rodin.rs`: `#[ignore]`'d, green the moment gap #1 lands.
- Immediate next step: **gap #1, Version accessors.**
