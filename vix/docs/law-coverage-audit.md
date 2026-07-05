# Law Coverage Audit — the demand machine

Trigger: the real-process exec path serialized **all** runs — it awaited
completion at spawn — and the full suite (Interp+JIT machine tests in
`vix/src/machine/{lower,driver}.rs`, plus `vix/tests/*`) stayed green. The
demand engine's headline property ("nothing forces locally"; independent runs
overlap) was broken on an entire execution path and no test noticed.

This document reconstructs why, enumerates the stated laws, marks each
pinned/unpinned against the real test that would fail if the law broke, ranks
the gaps by blast radius, and proposes the smallest honest pin set.

The conclusion up front: **the suite pins laziness (what is *not* run) and result
correctness, both on a lane that executes synchronously by construction, and
never pins run *overlap* or event *order*.** The incident lives exactly in the
uncovered quadrant.

---

## 1. How it slipped — the exact blind-spot pattern

### 1.1 "Lane" is overloaded, and both meanings hide the incident

`lanes()` (`lower.rs:5765`) returns `[Interp, Jit]`. Everywhere PARITY.md and the
machine tests say "both lanes", they mean **the task-VM execution lane**
(tree-walking interpreter vs JIT) — *not* the exec backend. Both VM lanes drive
the **same synchronous fake exec backend** (`exec_cache.exec`, run inline inside
`ensure_run_started`, `driver.rs:3811-3822`). `assert_lane_traces_equal`
(`lower.rs:9547`) is a real differential trace oracle, but it only ever diffs
Interp vs JIT — two paths that share the identical synchronous execution
substrate. It cannot observe an exec-backend semantics fork.

The exec *backend* axis (the one the incident lives on) is a different thing:

- **fake** — `ExecCache`/`FakeCc`, in-process, **runs the command inline at
  `ensure_run_started`** and stores `run.completed = Some(...)` immediately
  (`driver.rs:3811`). Completion is at spawn *by construction*.
- **real** — `RealProcessBackend`, `#[cfg(feature = "real-process")]`, the only
  backend that can hold a genuinely in-flight process. Exercised by exactly three
  tests: `tests/real_process.rs` (1) and `tests/crate_real_process.rs` (2), all
  gated on `host_cc_available()`/`host_rustc_available()`.
- **vox** — does not exist in this tree. `grep -rni vox src/machine tests` is
  empty. The "three lanes agree" premise has only two implementations today, and
  they are never differentially compared to each other.

### 1.2 Overlap is unobservable on the lane every pin runs on

On the fake backend the execution is synchronous: `RunStarted` and `RunCompleted`
for a run bracket an inline `exec_cache.exec` call with nothing able to interleave
between them. A serialized real path produces the identical event *shape*
(`Started(A), Completed(A), Started(B), Completed(B)`) that a *correct* fake lane
produces. There is no witness in the trace that distinguishes "serialized" from
"pipelined" unless a test looks for two `RunStarted` before a `RunCompleted` — and
none does.

Worse, the driver's own merge site is written serially: `force_tree_handle`'s
`Merge` arm (`driver.rs:3486-3505`) loops over pending children doing
`demand → force → finish_run` **per child** — spawn *and* await each sibling
before touching the next. On the synchronous fake backend this is invisible
(there is nothing to overlap). The property only becomes real, and only becomes
breakable, on an async backend — which is precisely the untested one.

### 1.3 The multi-run assertions destroy the ordering evidence on purpose

Every assertion that touches more than one run reduces the trace to a **sorted
set** or a **count**, discarding order:

- `started_outputs`/`completed_outputs` (`lower.rs:9775`, `9787`) collect outputs
  and are compared against `output_set` (`lower.rs:9799`), which **sorts**. So
  `merge_demand_*` (`lower.rs:9813+`) and the lua contract assert *which* runs
  started/completed, never in *what order* or with *what overlap*.
- `run_requested_count` (`lower.rs:9761`) and `spawned_count` (`lower.rs:9709`)
  are pure counts.
- The lua contract (`lower.rs:7346`) filters/counts `RunRequested`/`RunStarted`
  (== 5/5) and collects `RunCompleted` into a set of 3 — order-free.
- `real_process.rs` demands `build_a`, then `build_b`, then `build_a`
  **sequentially, one run at a time**, and asserts only the *serving tier*
  (`Ran`/`Tier1Hit`/`Tier2Cutoff`) and output-byte equality. It never has two
  runs in flight, so there is nothing for it to serialize.

No test anywhere reads `timestamp_us` (present on every run event,
`driver.rs:3731/3776/3920`), and `grep -rniE 'overlap|interleav|concurren|in.?flight|parallel'`
over `src/`+`tests/` finds no run-scheduling assertion.

### 1.4 The named pattern

> **Laziness-and-result pinning on a synchronous oracle lane.** The pins assert
> (a) *absence* — a run/spawn that should not happen doesn't — and (b) *result
> correctness* — the returned value/bytes. Both are fully expressible on a lane
> that executes runs synchronously. The one headline property that is *not*
> expressible there — temporal overlap / non-blocking spawn / event order — is
> the property the incident violated, and it is asserted by nothing. The lane
> where it *is* observable (real) is exercised single-run, tier-only.

The incident was structurally invisible: broken behavior on the async lane
reproduces the exact event *shape* of correct behavior on the sync lane, and the
async lane is never differenced against the sync lane.

---

## 2. The laws × pins matrix

For each stated invariant: the test that would **fail if the law broke**, or
UNPINNED. A test that merely *exercises* the mechanism without asserting the law
does not count as a pin.

| # | Law | Pinned? | Pinning test (would fail if law broke) |
|---|-----|---------|----------------------------------------|
| L1 | **Demand-sunk lets** — an unused/unread `let` never spawns or runs | ✅ | `unused_command_binding_emits_no_exec_ops_or_runs` (`lower.rs:6973`, asserts `EXEC_HOST` op count 0), `unused_let_call_never_spawns` (`6999`), `let_binding_sinks_into_only_using_match_arm` (`7016`), `shared_let_binding_computes_once` (`7039`) |
| L2a | **Nothing-forces (narrowing)** — projecting/narrowing demand never forces unrelated producers | ✅ | `merge_demand_selected_tunnels_and_never_runs_left` (`9813`, `left.o` never `RunRequested`), `merge_demand_subtree_chain_refines_without_left` (`9920`), `untaken_arms_never_spawn` (`6128`), `undemanded_functions_never_trace` (`6072`) |
| **L2b** | **Nothing-forces (execution)** — forcing is deferred to bit-demand; **spawn never blocks; independent runs overlap** | ❌ **UNPINNED** | *No test.* Every multi-run assertion is a sorted set/count; overlap is unobservable on the only lane the pins run (fake, synchronous). **This is the incident.** |
| L3 | **Projection-narrowed memo repinning** — memo keyed by the read-set, not the whole input | ✅ | `record_projection_hit_ignores_untouched_field_and_misses_touched_field` (`6492`), `map_projection_hit_ignores_untouched_entry_and_misses_touched_entry` (`6589`), `is_patron_ruling_uses_only_is_patron_projection` (`6685`), `projection_read_sets_survive_warm_reload` (`6794`) |
| L4 | **Identity-never-by-forcing** — moving a value's identity never demands its bits | ✅ | `unwrapped_pending_identity_moves_without_demanding_bits` (`8369`, asserts `PENDING_COERCE_HOST` count 0 and no producer spawn), `render_result_is_schema_aware_and_never_forces_pending_values` (`9421`) |
| L5 | **Bits-strict / identities-lazy** — bits hash by word bytes; handles/pending hash by content identity, insertion-order-independent | ✅ | `lazy_map_pending_entries_hash_independent_of_insert_order` (`8307`), `ready_and_pending_map_entries_hash_differ_under_bitset_encoding` (`8336`), `maps_are_canonical_regardless_of_insertion_order` (`6421`), `store_values_are_totally_ordered_canonically_on_the_machine` (`9214`) |
| L6a | **In-flight join (function/INVOKE)** — concurrent callers of the same invocation share one spawn | ✅ | `shared_calls_spawn_once` (`6041`), `memo_boundaries_kill_the_exponential_tree` (`driver.rs:7570`), `fib_runs_linear_on_the_machine` (`6095`) |
| **L6b** | **In-flight join (exec run)** — two demands for the same run identity coalesce to one process, not two | ❌ **UNPINNED** | Only *sequential* cache reuse is pinned (`machine_commuting_flags_share_exec_identity` → second run `Tier1Hit`). No concurrent exec dedup pin; unobservable on the synchronous lane. |
| L7 | **Memo hit ⇒ zero side-effect events** — a warm hit emits exactly `[Demanded, MemoHit]`, no spawns/runs/observations | ✅ | `warm_demand_is_two_events` (`6057`), and the warm tail of `lua_vix_runs_on_machine_with_exec_depth_contract` (`7413`) and `merge_demand_selected_*` (`9852`) |
| L8 | **Run-role enforcement** — inputs outside the mount ceiling error; roles drive the plan | ⚠️ partial | `ceiling_is_the_mount_set` (`tests/exec.rs:201`, substrate-only). Machine-seam propagation is claimed by PARITY X07 but not asserted at the machine boundary. |
| L9 | **Wasm-lane exclusions** — real-process/native paths excluded on `wasm32` | ❌ **UNPINNED** | Enforced only by `#![cfg(... not(target_arch = "wasm32"))]` gates; no test asserts the exclusion. (Compile-time; low blast radius.) |
| L10 | **Warm/cold trace equivalence** — warm reload preserves value/memo identity; trivia is free; semantic edits repin exactly their blast radius | ✅ | `warm_reload_eval_identity_survives_trivia_and_semantic_edits` (`7939`), `warm_reload_trivia_costs_only_root_hit` (`7990`), `warm_reload_leaf_edit_misses_exact_blast_radius` (`8019`), `warm_reload_unused_edit_costs_zero_misses_and_hashes_only_itself` (`8054`), `warm_reload_type_decl_edit_misses_transitive_users` (`8092`) |
| **L11** | **Event-order guarantees** — the trace's *order* is a contract (requested→started→completed; siblings' relative order) | ❌ **UNPINNED** | Every multi-run assertion sorts (`output_set`) or counts. `assert_lane_traces_equal` compares full ordered traces but only across Interp/JIT, which are order-identical by construction. Nothing pins order across a semantics change. |
| L12 | **Cross-module hash stability** — same content hashes the same across module layout; edits repin transitively | ✅ | `cross_module_hashes_match_same_content_in_one_file` (`7585`), `warm_reload_cross_module_leaf_edit_misses_transitive_users_only` (`7628`), `recursive_scc_hashes_survive_definition_order_on_machine` (`8124`) |
| **L13** | **Lane parity (fake ≡ real)** — the same program yields the same semantics/trace (modulo timing) on every exec backend | ❌ **UNPINNED** | No test runs one program across fake **and** real backends and diffs. `assert_lane_traces_equal` is Interp-vs-JIT only. `real_process.rs`/`crate_real_process.rs` assert real-vs-*cargo-oracle* shapes, never real-vs-fake trace. **This is the meta-gap that let the incident through.** |
| L14 | **Warm root hit is exactly one** — second identical top-level demand appends exactly one hit | ✅ | lua warm tail (`7415`), `merge_demand_*` warm tails, `warm_reload_trivia_costs_only_root_hit` (`7990`) |
| L15 | **Exec cache tiers** — cold `Ran`, unchanged `Tier1Hit`, unread-edit `Tier2Cutoff`, read-edit reruns | ✅ | substrate `tests/exec.rs` (`cold_run_pins_reads_and_negative_lookups` etc.) + machine seam `machine_fn_memo_and_exec_tiers_compose` (`7723`), `machine_commuting_flags_share_exec_identity` (`7810`) |

**Tally:** 11 pinned, 1 partial, **5 unpinned** (L2b, L6b, L9, L11, L13).

---

## 3. Unpinned laws ranked by blast radius

Ranked by "would this ship a silent semantics fork like the incident did?"

1. **L13 — Lane parity (fake ≡ real).** *Highest.* This is the umbrella that
   would have caught the incident. With no fake-vs-real differential, *any*
   real-backend-only regression (serialization, missing observation, tier
   miscount, output divergence) ships green. The incident is one instance of a
   whole class this gap admits.

2. **L2b — Nothing-forces (execution / overlap).** *Highest.* The literal
   headline property that broke. Silent, catastrophic-for-performance, and
   correctness-adjacent (a blocking spawn can also change *which* runs get
   requested under demand). Fully invisible on the synchronous lane.

3. **L11 — Event-order guarantees.** *High.* The reason L2b/L13 are invisible:
   the trace *records* order and timing but every assertion throws them away.
   Pinning order is the mechanism by which L2b and L13 become checkable at all.

4. **L6b — Exec-run in-flight join.** *Medium.* Duplicate concurrent runs of the
   same identity would waste work and could double-emit observations; only
   observable with a real/async backend. Adjacent to L2b.

5. **L9 — Wasm-lane exclusions.** *Low.* Compile-time cfg gate; a break surfaces
   as a build failure, not a silent runtime fork.

The top three share one root: **the suite never differences an asynchronous
execution against a synchronous reference, and never asserts trace order/overlap.**

---

## 4. Proposed minimal pin set

House rules honored: no unnecessary tests; integration tests on the production
path; oracles preferred (differential vs reference; "trace contains / never
contains X"); event-**order** and event-**absence** over the machine trace are
the workhorse. Three pins close all five gaps; the first two are load-bearing.

### Pin A — Fake≡Real differential trace oracle (closes L13, and L2b/L6b/L11 on the real lane)

**Law:** L13 (and, on the real lane, L2b/L6b/L11).
**Assertion:** run one fixture that issues ≥2 independent `cc!` runs — reuse the
existing lua fixture, which already drives 3 real `cc` runs — on **both** the
fake `exec_cache` backend and `RealProcessBackend`. Normalize each trace (drop
`timestamp_us`, canonicalize `run_id` by first-appearance order, keep serving
tier as a tier *class* since fake/real read-set granularity differs per
`cargo-manifest-build.md`) and assert the two normalized traces are **equal**.
**Lane(s):** fake × real, gated on `host_cc_available()` exactly like
`real_process.rs`.
**Why smallest honest:** it is `assert_lane_traces_equal` generalized from the
VM axis to the backend axis, over a fixture and a backend that already exist. One
test turns the entire existing fake-lane corpus into a reference oracle for the
real lane — the maximum leverage per line. It fails the instant the real lane's
event stream diverges in shape from the fake lane's (which a serialized spawn
would *not* do by shape alone — hence Pin B).

### Pin B — Run-overlap witness on a deterministic async fake backend (closes L2b, L6b, L11 — deterministically, in CI)

**Law:** L2b (execution / non-blocking spawn), with L11 as its mechanism.
**Assertion, "the trace contains X":** introduce a **deterministic async fake
backend** — a `MachineExecBackend` whose `spawn` returns a pending handle that
does **not** execute until `flush`/`demand_path` (deferred, in-process, no host
processes, no wall-clock). Drive a two-independent-object target
(`ar! { *[cc!{...a}, cc!{...b}] }`). Assert the trace contains an **overlap
witness**: two `RunStarted` events with **no intervening `RunCompleted`** — i.e.
∃ positions i<j<k with `RunStarted, RunStarted, …, RunCompleted`.
**Lane(s):** a new async-fake backend on the default (Interp+JIT) VM lanes — so
it runs in CI with no `real-process` feature and no host toolchain.
**Why smallest honest:** the overlap law is *only* expressible on an async
backend; today the only async backend is `real-process` (feature-gated,
host-gated, flaky-timing). A deterministic async fake is the minimal object that
makes the law observable *and* keeps it in the default CI lane. It fails against
the incident directly: a spawn that awaits at completion emits
`Started(A),Completed(A),Started(B)` — no witness — a red test. As a bonus it
pins L6b (assert a repeated identity yields one `RunRequested`, not two) and
forces the `force_tree_handle` merge loop (`driver.rs:3486`) to start siblings
before awaiting — the actual site of the bug.

> Pin A and Pin B are complementary: A proves the real lane *agrees with* the
> reference lane in shape; B proves the *reference itself encodes overlap* so
> that "agreement" is meaningful. A alone can't catch a serialization that keeps
> event shape; B alone can't catch a real-backend-specific divergence. Together
> they close L2b + L13 with two tests.

### Pin C — Machine-seam mount-ceiling assertion (promotes L8 from partial to pinned)

**Law:** L8 (run-role enforcement) at the machine boundary, not just the
substrate.
**Assertion:** a `cc!` whose spliced input escapes the mount set errors with text
containing `outside the mounts`, demanded through `Machine`, on both VM lanes.
**Why smallest honest:** `ceiling_is_the_mount_set` proves the substrate; this
one line proves the machine *propagates* it, which PARITY X07 only claims. Small,
and closes the one ⚠️.

L9 (wasm exclusion) is intentionally **not** pinned: it is a compile-time gate
whose failure is a build error, not a silent fork — a runtime test would be the
kind of unnecessary test to avoid.

### Where the workhorse assertions live

- **Event-ABSENCE** (`trace never contains RunRequested{left.o}`) already exists
  and is correct — keep it (L2a).
- **Event-ORDER / overlap** (`trace contains two Started before a Completed`) is
  the missing workhorse — Pin B — and is the only new *assertion form* required.
- **Differential trace** (`normalize(fake) == normalize(real)`) is the missing
  oracle — Pin A — and reuses the existing `assert_lane_traces_equal` idea on a
  new axis.

---

## 5. Lane parity — finding #1

**There is no differential test running the same program across exec backends and
diffing traces.** `assert_lane_traces_equal` (`lower.rs:9547`) diffs Interp vs
JIT — two VM front-ends over the *same synchronous fake exec backend* — so it
cannot see a fake-vs-real divergence. The real backend is checked only against a
*cargo* unit-graph oracle (`crate_real_process.rs`) and against fixed serving
tiers (`real_process.rs`), never against the fake lane's own trace. The "vox"
lane the brief references does not exist in this tree.

That missing fake-vs-real differential (**L13 / Pin A**) is the single change that
would most directly have caught the incident, and it is finding #1.
