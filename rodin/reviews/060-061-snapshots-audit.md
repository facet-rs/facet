# Adversarial audit — canonical snapshot rungs 060–061

- Base audited (HEAD, clean tree): `b455e87df1796fada5f566c48a2d8afc9e687f36`
- Branch: `audit/vix-snapshots-060-061`
- Scope: `expect_snapshot` as a `Check` — rendering engine, ratchet authority,
  and the production-harness contract. Read-only architecture audit; the only
  change committed by this audit is this artifact.
- Evidence: source/history trace plus an out-of-repo harness that links the
  `vix` crate at this SHA and calls `vix::ratchet::run_source` on adversarial
  fixtures (no repo files touched; scratch project deleted after use).

## Central question and disposition

**Is this a faithful `expect_snapshot`-as-`Check` whose external harness owns the
expected artifacts, or does it make ordinary snapshots vacuously pass?**

It makes ordinary snapshots **vacuously pass**. `expect_snapshot` is lowered to a
value-publishing island (`IslandPurpose::Value`), and `evaluate_snapshot_site`
copies the value island's verdict verbatim: `passed: evaluation.passed`
(`vix/src/ratchet.rs:782-794`). A value island always returns `passed: true`
whenever the value evaluates without a machine/language fault
(`vix/src/runtime/scheduler.rs:709-716`, `:775-779`). The rendered text is
attached to `CheckRun.snapshot` but is **never compared to any expected value**
anywhere in the demand path. `RatchetReport::passed()` only ANDs `check.passed`
over both lanes (`vix/src/ratchet.rs:492-500`), so the rendered content has no
bearing on the verdict.

The only comparison against an expected rendering lives in two hand-written Rust
`assert_eq!` goldens in `vix/tests/ratchet_runner.rs:5813-5816` and `:5836-5839`.
There is **no** harness that loads/compares/updates user `.snap` artifacts and
**no** general golden registry — confirmed by search across the tree; every other
`Snapshot`/`snapshot` symbol in `vix/src` is unrelated (world snapshot, lowerer
snapshot, store snapshot). So there is no external artifact owner for the Check to
delegate authority to.

`RatchetReport::agrees()` compares `plain.check_family() == chaos.check_family()`,
and that key includes the `snapshot` field (`vix/src/ratchet.rs:486-489`,
`CheckRun.snapshot` at `:296-298`). This catches *lane disagreement* on a
rendering, but gives **zero** protection against rendering *drift*: a code change
alters both lanes identically, so `agrees()` stays true. Only the out-of-band Rust
`assert_eq!` catches a regression, and only for the two hard-coded names.

**Empirical proof of vacuity** (out-of-repo harness at this SHA): a single test
that yields two snapshots under the *same* name with *different* renderings
returns `report.passed() == true`, `checks == 2`, snaps
`[("same","Dep {\n    name: \"a\",\n}"), ("same","Dep {\n    name: \"b\",\n}")]`.
Two arbitrary distinct renderings both pass — the rendered string content is
never gated.

### Answer to "can a changed rendering make `run_source`/`SuiteRun.passed` false today?"

**No.** A rendering change (or a wrong rendering) cannot flip any `CheckRun.passed`,
hence cannot flip `SuiteRun`/`RatchetReport::passed()`. Only the Rust
`ratchet_runner.rs` `assert_eq!` can fail, out of band from the ratchet mechanism
that certifies every other rung. This is the structural break: for all other rungs
the `.vix` check's own verdict encodes the property; for 060–061 the property lives
in a Rust string literal outside the ladder.

### Disposition: **REJECT** integrating 060–061 as snapshot *Check* rungs as-is.

The rendering engine (`render_frozen`) is sound and lane-agreeing and should be
kept. What must not land is the claim that these are `expect_snapshot` conformance
rungs: today they certify "the value evaluated" (via `run_source`) plus "one exact
Rust golden matched" (via a bespoke test), not "the snapshot check holds." Accept
only after **one** of:

1. **Define the harness snapshot-artifact contract** (smallest real fix): the
   ratchet loads an expected `name → rendered` registry; `evaluate_snapshot_site`
   sets `passed` from `expected == rendered` (record-new on first sight, with an
   `--update` seam), and a mismatch produces a `FailureValue` carrying
   `name`/expected/actual and the site provenance. Then a wrong rendering flips
   `report.passed()`, and a `.vix` rung certifies itself like every other rung.
2. Or **explicitly re-scope** testing.md's "The ratchet" section to say snapshot
   rungs assert rendering via an adjacent Rust/Styx golden, and make the `.vix`
   check a documented "renders without fault" probe — with the naming, scalar,
   escaping, and fault-attribution gaps below closed first.

Underspecification note: testing.md line 75 lists the signature but says nothing
about who owns expected artifacts, loading, updating, or mismatch reporting. That
is the missing explicit contract; option 1 is the smallest one that keeps
"tests are values / the check encodes the property."

## Ranked findings (file:line)

### P0 — snapshot verdict is decoupled from the rendering
- `vix/src/ratchet.rs:782-794` — `passed: evaluation.passed`; the render is
  attached, never compared. Combined with `vix/src/runtime/scheduler.rs:775-779`
  (`passed: true` for every realized value island) and
  `vix/src/ratchet.rs:492-500`, a snapshot check cannot go red on content.
- Sole authority is the Rust golden: `vix/tests/ratchet_runner.rs:5813-5816`,
  `:5836-5839`. No `.snap` loader / registry exists.
- **Seam:** feed a name→rendered comparison into `CheckRun.passed` (option 1
  above). Minimal red-proof certificate: perturb the expected golden and assert
  `report.passed()` becomes false — impossible today, which is the point.

### P0 — snapshotting any scalar aborts the whole suite run
The spec promises `expect_snapshot(v: T)` for *any* `T`, and fixture 060's comment
says "any value renders structurally." But `render_snapshot` requires a frozen
tree at the top (`vix/src/runtime/scheduler.rs:1790-1796`), and only
records/tuples/enums, ordered map/set, and (since `5282f5ded`) dense arrays attach
one. A top-level scalar falls into the `_` arm with `frozen = None`
(`vix/src/runtime/scheduler.rs:2121-2129`), because the outcome-envelope forcing
is gated on `Record|Tuple|Enum` only (`vix/src/lowering.rs:440-445`).
- **Empirically confirmed** (out-of-repo harness):
  - `expect_snapshot(3, "n")` → `Err(Runtime(SnapshotRender { "published snapshot value has no frozen structure" }))`.
  - `expect_snapshot(true, "b")` → same `SnapshotRender` fault.
  - `expect_snapshot("hi", "x")` → `Err(Task(InvalidResultShape { size: 8 }))` (faults even earlier).
  All three abort `run_source` with `Err` (a suite-level `RunError`), not a red
  check. There is no certificate for any scalar snapshot.
- **Seam:** either attach a frozen leaf for scalar/String value islands (extend
  `publishes_aggregate` and `realize_value` to freeze scalars, or let
  `render_snapshot` render a resident-bytes leaf without a frozen entry), and add
  Int (incl. negative)/Bool/String scalar certificates.

### P1 — surface signature diverges from the spec and its own doc comments
testing.md:75 documents `expect_snapshot(v: T) where { name: String }`, and the
doc comments at `vix/src/compiler.rs:2090` and `vix/src/runtime/model.rs:94`
both write the `where { name }` form. The implementation **rejects** named
arguments (`vix/src/compiler.rs:2102-2107`, "named arguments on a check
constructor") and requires a positional string literal
(`check_arity(call, 2)` + `args[1]` at `:2108-2118`).
- **Empirically confirmed:** `expect_snapshot(a) where { name: "x" }` →
  `Err(Diagnostics: UnsupportedExpression "named arguments on a check constructor")`.
  The documented signature is a compile error.
- **Seam:** reconcile — pick the positional form and fix testing.md:75 + both doc
  comments, or implement the `where`-clause binding in `lower_snapshot_check`.

### P1 — snapshot name namespace: bare global string, no qualification, no dedup
`lower_snapshot_check` clones the raw literal into `CheckRecipe::Snapshot { name }`
(`vix/src/compiler.rs:2116-2119`); it flows unchanged through
`PartitionedRecipe::Snapshot` (`vix/src/vir.rs:1220`, `:1556-1571`) into
`SnapshotCapture.name`. It is never qualified by test/module/site and never
checked for uniqueness.
- **Empirically confirmed:** two `expect_snapshot(_, "same")` in one test both
  pass and both emit under name `"same"` — a silent collision. Any external
  harness keying `.snap` files by name would map two distinct artifacts to one
  file. (Harmless in-tree today only because the ratchet keys by `ProvenanceKey`
  and never uses `name` for pass/fail.)
- **Seam:** qualify `name` by test (and, at the dynamic-key tail, by
  `ProvenanceKey.dynamic_keys`) and reject duplicate snapshot names at compile
  time — mirror the existing `RunError::DuplicateSiteKey` discipline
  (`vix/src/ratchet.rs:210-215`). Add a `.reject.vix` with two identical names.

### P2 — string/path leaf escaping *is* a `Debug` impl, contradicting the claim
`render_frozen` renders `String`/`Path` leaves with `write!(out, "{text:?}")`
(`vix/src/runtime/scheduler.rs:1871-1876`) — Rust's `str` `Debug`. The container
shape is type-directed, but leaf escaping is exactly the `Debug` impl the module
doc says it never is (`:1829-1830`, `vix/src/runtime/model.rs:96-99`).
- **Empirically confirmed:** a string field containing `"`, `\`, tab, newline
  renders as `\"`, `\\`, `\t`, `\n` — Rust `Debug` escaping. vix's *canonical*
  string escaping is thereby undefined by spec and silently inherits Rust's rules
  (control chars → `\u{7}`, etc.); no certificate exercises any escapable char
  (both goldens are plain ASCII).
- **Seam:** define a canonical escaper (or explicitly document "Rust-Debug
  canonical") and add a certificate over `"`, `\`, `\n`, `\t`, a control char, and
  a non-ASCII code point.

### P2 — render-fault attribution is a suite abort, not a red check
A shape/frozen mismatch in `render_snapshot` returns `Err` propagated by `?` at
`vix/src/ratchet.rs:780`, becoming a suite-level `RunError` that aborts *both*
lanes for *all* tests — unlike a value/language failure, which becomes a
`CheckRun { passed: false, failure, failure_context, provenance }`. The snapshot
site's own provenance is lost. This is the exact class of bug fixed by
`5282f5ded` (arrays previously had no frozen tree) and is still live for scalars
(P0 above). No certificate covers the fault path.
- **Seam:** decide whether a render mismatch is a machine invariant (keep abort,
  but add a certificate so it is intentional and observed) or a user-facing
  failure (attribute it to the site as a red `CheckRun`).

### P3 — native/interpreter parity is claimed but not certified in-tree
Both certificates call `run_source` (native lane) only; the WEAVY_JIT=0
interpreter agreement asserted in the commit messages is env-gated at CI time and
not proven by any committed certificate.
- **Seam:** add a certificate that runs both lanes and asserts byte-identical
  `SnapshotCapture.rendered`.

### P3 — freeze-on-publish is unconditional work on the array publication path
`realize_value`'s top-level `Array` arm now always calls `freeze_dense_array`
(`vix/src/runtime/scheduler.rs:2118`) for *every* published array, snapshotted or
not — a full extra structural walk of the array, mildly at odds with the "read
only off the publication path" wording (it is *computed* on the path, only *read*
off it). It is counter/identity-neutral (see withdrawn hypotheses), so severity is
low; consider gating the freeze on an actual snapshot consumer if array-heavy
publication cost matters.

## Adversarial checklist — results

- **Namespace/collisions:** unqualified global name, no dedup — P1 (confirmed).
- **record/tuple/enum/array/Map/Set/String escaping & formatting:** structure is
  type-directed and canonical (2-space indent, trailing commas, `{}`/`[]` empties,
  ordered-map/set follow collection order — 061 confirms canonical sort survives);
  **string escaping delegates to Rust `Debug`** — P2.
- **Semantic Type + FrozenValue identity vs ABI/Debug:** the *walk* follows VIR
  `Type` and `FrozenValue` (`render_frozen`, `vix/src/runtime/scheduler.rs:1831+`),
  resolving references through the store by identity — this part is faithful and is
  the branch's real strength. The **string leaf** is the sole `Debug` leak.
- **plain/chaos & native/interpreter equality:** plain/chaos agreement is enforced
  by `agrees()` (but see P0 — it cannot catch drift); native/interpreter parity is
  uncertified in-tree — P3.
- **publishing solely for rendering perturbing demand/identity/counters/sharing:**
  **withdrawn** — `attach_frozen` mutates only `entry.frozen`, computes no
  identity, bumps no counter (`vix/src/runtime/store.rs:126-132`);
  `freeze_dense_array` borrows `&Store` immutably and cannot intern
  (`vix/src/runtime/scheduler.rs:2407-2425`). The commit's neutrality claim holds;
  only unconditional CPU work remains (P3).
- **failure behavior/attribution when a value cannot resolve:** suite abort, not a
  red check — P2.
- **deep/nested & recursion:** `render_frozen` recurses per structural depth with
  no depth guard; a value island's own depth is already bounded by realization, so
  no independent snapshot-side stack risk was found. Nested records render
  correctly (`Outer { a: Inner { v: -5 }, ... }` confirmed).
- **Rust hard-coded goldens as ratchet certificates:** acceptable *only* as an
  out-of-band renderer oracle; **not** acceptable as the authority for a
  snapshot-`Check` rung, because the `.vix` check's verdict is independent of them
  (P0). How `vx test` would load/update/report user snapshots: it currently
  cannot — the contract does not exist.
- **can a changed rendering make `SuiteRun.passed` false today:** no (P0).

## Withdrawn hypotheses (refuted by current source)

- "Publishing for rendering perturbs counters/identity/sharing" — refuted:
  `attach_frozen` and `freeze_dense_array` are counter- and identity-neutral.
- "The renderer is a `Debug` impl of the whole value" — refuted for structure:
  container shape and field names come from VIR `Type`; identity/reference
  resolution is semantic. (The narrower string-leaf `Debug` leak stands as P2.)
- "plain/chaos rendering could diverge under chaos kills" — not observed;
  rendering reads frozen trees off equal identities, and `agrees()` would catch a
  divergence (its blind spot is drift, not lane disagreement).
