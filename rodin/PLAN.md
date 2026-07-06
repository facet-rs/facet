# rodin.vix — the plan (read this first)

Reimplement Rodin (the cargo-shaped version resolver) **entirely in vix**, as the
only production Rodin. This file is the entry point; read it, then `docs/`.

## Why the first plan failed (root cause)

Rodin was first implemented **directly in Rust** (`rodin-core`, `rodin-facts`).
That finished Rust artifact became the anchor: the plan was written as "port
rodin-core," which silently promoted its Rust representations to givens and
rewarded mirroring its internals to make a diff pass. The whole gap list became
Rust→vix *bridges* ("expose Version's fields," "expose VersionSet ops"), so an
agent handed "wire a host accessor" produced exactly that — an intrinsic that
re-parses a version's display-string bytes on every field access. The intrinsic
wasn't a slip; it was the plan's faithful output. Worse, `rodin-core` hand-built
an entire incremental / memo / proof / read-set / warm-reuse stack — precisely
the services the vix machine *is* — so a faithful port rebuilds the machine
inside the machine.

## The method (this plan)

1. **Distill `rodin-core` into `docs/`** — representation-neutral prose:
   behavior, invariants, and the solving strategy, never a struct/field/byte
   layout. Litmus for every sentence: if it names a type, a field, an interner,
   or canonical bytes, it's contraband; if it could be handed to someone
   implementing in an unnamed substrate, it's clean.
2. **Delete `rodin-core`** (in vixenware) once the distillation is reviewed. No
   Rust artifact left to port from; no internals left to mirror.
3. **Implement Rodin in vix**, derived from `docs/` + cargo, native from the
   start. The `vix/tests/rodin.rs` harness gets rebuilt around cargo fixtures.

## Doctrine (durable)

- **cargo is THE and ONLY oracle.** Ground truth = `cargo tree --target`,
  `cargo generate-lockfile` on small workspace fixtures. `rodin-core` is being
  turned into docs, then deleted; it is not a differential target.
- **No god `Value` enum.** Values are schema-laid-out bytes (records-at-offsets,
  enums-as-tag+variants). A field is an offset read, never a parse.
- **Content-addressing / memo / incremental are FREE.** Never hand-roll
  `canonical_bytes`, an interner, a read-set, a proof, or warm-fact reuse — the
  machine provides all of it. See `docs/90-substrate-ledger.md`.
- **Red-flag test for any step.** A step of the form "expose Rust type X to vix
  via a host op" is presumed wrong. Steps read "express X as a vix
  value/computation, check against cargo." A host op is admissible only when a
  *measured* hotspot demands it (the VersionSet interval-throughput question) —
  never because the Rust version had it.
- **Grow the vix language surface where the port needs it** — but as vix-native
  values/demand, not as Rust bridges.
- **Never revert by delete/checkout/reset; commit and push often; work in
  `~/oss/facet-cc`.** (rodin-core's deletion is a deliberate reviewed plan step,
  not a revert.)

## The spec (`docs/`, numbered = build order)

- `00-oracle.md` — cargo is the only oracle: invocations, comparison, fixtures.
- `10-identity.md` — identity = (source, name, compat-class); coexistence bucketing.
- `20-constraints.md` — dependency edges → clauses/guards/consequents as meaning; the consumption gate.
- `30-versions-sets.md` — versions as ordered values; interval-set meaning; cargo caret/tilde/wildcard; the prerelease gap.
- `40-search.md` — narrow → propagate-to-fixpoint → highest-first candidates → hypothesize/recurse/backtrack.
- `50-conflict-learning.md` — regions as boxes; point→widen→install; subsumption by containment; region unit-propagation.
- `60-features.md` — atoms + monotone unification; normal/build/dev scopes; the four enable forms; optional-dep implicit features.
- `70-targets-cfg.md` — cfg/target gating; the consumption gate; per-target projection.
- `90-substrate-ledger.md` — the do-NOT-build table: rodin-core subsystem → vix mechanism.

## Status

- **Distillation complete: all nine docs written** (00, 10, 20, 30, 40, 50, 60,
  70, 90). The model (`rodin-facts`), the core search
  (`solve`/`seed_problem`/`search`/`propagate`/`learn`/`install_learned_fact`/
  `widen`), the feature + cfg compilation, and the cargo-evidence harness are all
  read and distilled.
- Two invariants still flagged in-text for confirmation when implementing: the
  role of the *unclassed* identity form (10), and the exact inputs of the
  *consumption gate* (20/70). Both are answerable against cargo during the build.
- **Next: review the distillation, then delete `rodin-core` (in vixenware), then
  implement native from `docs/` + cargo (build order = doc numbering).** The
  deletion is a reviewed plan step, gated on your read of `docs/`.
- Superseded by this plan: the `VERSION_FIELD` intrinsic + string-blob `Version`,
  and the current gap-driven `rodin.vix`. Version becomes a vix value (parse a
  memoized demand); `rodin.vix` gets rewritten native. Not deleted to hide —
  replaced by the native implementation.
