# 00 — The oracle: cargo, and only cargo

Correctness is defined by cargo, not by any reimplementation. Every slice of the
resolver is validated by making cargo resolve the same fixture and comparing.
There are two cargo oracles, because resolution has two observable outputs.

## Oracle 1 — version selection (the lockfile)

`cargo generate-lockfile --offline` in a fixture workspace, then read
`Cargo.lock`. The lock's package set is ground truth for *which version of each
package identity* the resolver must select. Offline against a pinned registry, so
the answer is deterministic.

Compare: the set of (name, version, source) the resolver selects must equal the
lock's, per package identity (doc 10 — `serde@1` and `serde@2` are separate rows).

## Oracle 2 — the target-projected graph (the tree)

`cargo tree -e normal,build --target <triple> --prefix none --offline`. Ground
truth for *the resolved dependency graph under a specific target and edge-kind
filter*: which edges are actually consumed once cfg/target gating (doc 70) and
edge kinds (normal + build, excluding dev) apply. The lockfile alone does not tell
you this — it is the superset of possibilities; the tree is the projection.

Compare: the resolver's graph, projected to the same target and edge kinds, must
equal the tree.

## Fixtures

Small workspaces isolating one behavior each — a 2-crate dependency, a compat-
class coexistence (`serde@1` + `serde@2`), a feature-unification case, a
cfg/target split. Each fixture is checked against both oracles as applicable.
Build the resolver slice-by-slice (docs 40+) and grow the fixture set alongside; a
slice is "done" when it matches cargo on its fixtures.

## What is NOT part of this

Absolute registry/workspace paths, report/divergence data structures, warm
counterfactual comparisons — harness specifics. The method is what matters: two
cargo oracles (lockfile for versions, `tree --target` for the projected graph),
offline against a pinned registry, compared per package identity.

## Provenance — the solver model is already de-risked against cargo

The now-deleted Rust reference resolver (`rodin-core`) was run differentially
against cargo at scale before deletion, and the result is what licenses "cargo is
the only oracle" as a *finished* validation rather than a hope. The record lives
in vixenware `docs/design/rodin-c-evidence.md` (Round 9, projection *lock-union
dev-included*, oracle *FreshLock* — cargo re-resolving from scratch, not a stale
historical lock):

- **853 / 892 = 95.63 %** exact locked-domain matches over a real dependency
  universe; 458 decisions, 69 conflicts.
- **105 divergences, every one classified `LegitimateTieBreakDifference`** — none
  a resolver bug. By kind:
  - **32 MissingInRodin** — a locked package class not reached under the
    extracted root/range/feature projection (lockfile-free graph membership).
  - **66 ExtraInRodin** — selected from the cached registry universe but absent
    from that particular `Cargo.lock` (lockfile-free graph membership).
  - **7 VersionMismatch** — the same compatibility class picked a different
    version than `Cargo.lock` (a lockfile-free tie-break, e.g. `clap 4.6.1` vs
    `4.5.60` — both valid).

This classification vocabulary is exactly the typed `Discrepancy` the live-Cargo
oracle harness emits (`vix/tests/rodin_fixtures.rs`): MissingInRodin ↔
`MissingSelection`, ExtraInRodin ↔ `ExtraSelection`, VersionMismatch ↔
`VersionMismatch`, each a structured value fit for minimization. The native Vix
kernel is re-validated the same way — against cargo, never against the deleted
resolver or any recorded selection.
