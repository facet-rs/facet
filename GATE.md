# GATE.md — how work folds into `rodin` (the reviewer's checklist)

The method: agents implement on worktree branches; a GATE (a skeptical reviewer — a model
session or Amos) verifies and fast-forward-pushes into `origin/rodin` (updates the draft PR).
Nothing folds ungated. This file makes the gate transferable between sessions/models.
Written 2026-07-07 by Fable after ~16 gated folds; every rule below was earned that day.

## The checklist, in order

1. **Base-not-stale.** `git -C $W fetch origin rodin -q` then `git -C $W rebase origin/rodin`.
   On conflict: `rebase --abort` and send the branch back to its author agent — do not resolve
   machine-code conflicts by hand at the gate.
2. **Scope read.** `git -C $W show --stat HEAD` (or the branch range). Every touched file must be
   explainable by the mission. Surprise files get READ, not waved through. (A "stdlib" mission
   once legitimately changed the language surface — fine, but it must be *seen* and flagged.)
3. **Fmt-noise discipline.** Wandering `cargo fmt` collateral is common. Use `git show -w` to
   suppress whitespace and reveal real semantic changes. A 163-line driver.rs diff can be 100%
   reflow — or hide one real hunk. `-w` tells you which.
4. **Tripwire rule (load-bearing).** Any diff touching `vix/tests/demand_driven.rs`, molten/reuse
   machinery, weavy task execution, or lowering semantics gets its hunks READ before folding.
   The demand-driven invariant cannot be retrofitted; the tripwires are its only guard.
5. **Tests — `cargo nextest run`, NEVER `cargo test`.** Scope by surface:
   - vix touched: `-p vix`; add `--features real-process` when exec/rodin/build paths changed
     (that suite shells real rustc/cargo — it must run).
   - weavy touched: `-p weavy --all-features` (jit is feature-gated; without it half the tests
     don't exist).
   - facet-core touched: also `cargo build -p facet-core --no-default-features` (no_std must
     survive).
   - phon/taxon touched: `-p taxon -p phon-schema -p phon-ir -p phon-engine -p phon-jit -p phon`.
6. **Clippy**: `-p <touched crates> --all-features --all-targets -- -D warnings`. Pre-existing
   lint debt surfacing from deps: FIX it (separate commit), never suppress — Amos's standing rule.
7. **Push only if everything above passed** — and gate the push in the script itself
   (`cmd && cmd && git push`); do NOT let a push ride an unconditioned shell chain (this bit us).
8. **Fold = fast-forward only**: `git merge-base --is-ancestor origin/rodin HEAD && git push
   origin HEAD:rodin`. Never force-push. NEVER rewrite history (Amos policy, recorded).
9. **Durability rule (standing, Amos 2026-07-07):** every worktree branch gets pushed to origin
   when its agent reports — code AND learnings survive the laptop. Learnings additionally fold
   back to the PR (docs/reports) when publishable.
10. **Publication check on new files**: `rg -l 'vixenware|vx-store|vx-vfsd' <new files>`.
    Calibration: private crate NAMES are mild (most of vixenware becomes open source; only the
    cloud control plane is truly proprietary and lives elsewhere) — but don't publish Amos's
    curation decisions for him. `/vix/docs/` and `/RESURRECTION.md` are gitignored on purpose;
    sensitive docs archive to the private branch `vix-docs-archive` on code.vixen.rs.

## Shell gotchas (real incidents)
- The harness shell is zsh: unquoted `$VAR` does NOT word-split. Write explicit commands, no
  clever arg-string variables.
- Long gate runs get interrupted when agent notifications land — keep gate commands short and
  resumable; a rejected tool call is NOT retried verbatim without the user's word.
- Never pipe build/test output into tail/head/grep — run bare, read the persisted output file.

## Failure discipline
- Tests fail at the gate → the fold does not happen; the branch keeps its commits; the author
  agent gets the failure verbatim. NEVER revert, NEVER `git reset` work away.
- An agent that contorts to satisfy an acceptance criterion (e.g. vendoring a patched crypto
  crate to dodge a build-dep) is a GATE finding: reject the contortion, fix the criterion.
- Agents stopping at guardrails with a finding ("this dependency edge doesn't exist") is a
  SUCCESS — the finding goes to Amos, not around him.

## Standing reviewers
- The committee agents (codex + opus pair with full redesign context) review large diffs before
  fold — as of writing they are designated reviewers for the V3 hash-epoch branch
  (`vix-typed-schemas`). Spawn a fresh committee via the paseo-committee skill if they're gone.
