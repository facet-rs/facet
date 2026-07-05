# Exec Seam Audit: is `MachineExecBackend` carving at the right joint?

Status: architecture audit, no code changes. Measured from code at
`dae42d4de` on a fresh worktree off `main`.

Trigger: a stax profile caught the **real-process** exec path serializing all
runs — completion-await *at spawn* — while the **vox/fleet** path honors
producing handles + in-flight join. Two backends implementing the same trait
disagree on a concurrency law. This document asks whether the seam
(`MachineExecBackend` / `MachinePendingRun`) is the right one, or whether it
lets backends own semantics that belong to the driver, exactly once.

Convention below: **FINDING** = measured from code. **JUDGMENT** =
recommendation. **OPEN** = genuinely Amos's to rule.

---

## 0. The three lanes (not two)

The prompt frames "two backends." The code has **three execution lanes**, and
the framing hides the accident:

| Lane | Selector | Where completion happens | Cache lives in |
| --- | --- | --- | --- |
| **Fake VFS** (default) | `exec_backend == None` | `ensure_run_started` runs `exec_cache.exec` inline (blocking) | driver `self.exec_cache` |
| **RealProcess** | `Some(RealProcessBackend)` | inside `spawn`, blocking (`cache.exec`) | backend's **own** `ExecCache` |
| **Fleet/wire** | `Some(FleetBackend)` | progressive; `spawn` returns a live handle | executor-side `RunTable` |

The fake-VFS lane is **not** a backend. It is an inline `else` branch in
`ensure_run_started` (`driver.rs:3811`). The fleet doc
(`docs/design/fleet-on-the-machine.md:164`, `:211-213`) *says* "the default
backend remains the existing local `ExecCache` path" — but that promotion was
never done. The local lane stayed hardcoded, and `RealProcessBackend` /
`FleetBackend` were bolted on beside it as whole-lifecycle strategy objects.
That is the historical accident this audit is about.

**FINDING.** Completion-at-spawn is shared by *two* of the three lanes
(fake-VFS and real-process), not unique to real-process. Only fleet produces a
genuine pending handle. The serialization "bug" is the majority behavior; the
fleet handle is the minority that got the law right.

---

## 1. Responsibility matrix (measured)

Each cell is where the responsibility is *implemented*, per lane.

| Responsibility | Fake VFS (no backend) | RealProcess | Fleet / wire |
| --- | --- | --- | --- |
| **Staging** (materialize tree) | in-memory `MountedWorld` over `mounts` | `RealProcessTool::run`: tempdir + CAS hardlink/copy, `physical_path`/`map_arg` (`real_process.rs:161-300`) | executor CAS; orchestrator ships **hashes**, `ensure_mount` does put/pull (`lib.rs:754`) |
| **Spawn / launch** | `tool.run` in-process (fake) | `Command::new().output()` **blocking** (`real_process.rs:129`) | `client.exec` on chosen executor, `tokio::spawn`, returns immediately (`lib.rs:975`) |
| **Completion timing** | at `ensure_run_started` (eager, blocking) | at `spawn` (eager, blocking) | **progressive**; handle returns before completion |
| **In-flight join** | **none** (single-demand) | **none** | **yes** — `RunState::InFlight`, broadcast feed, `CacheSource::Joined` (`lib.rs:359,418`) |
| **Tier-1 cache** | driver `exec_cache.tier1` | backend's own `ExecCache` | executor `RunTable.states` |
| **Tier-2 verify / read-set** | driver `exec_cache` (`verify`) | backend's own `ExecCache` (`verify`) | executor `RunTable.candidates` + `verify` (`lib.rs:370`) |
| **read-set → driver** | kept (`Outcome.read_set`) | **dropped**: `flush` returns `(Tree, ExecEvent)` only (`real_process.rs:95`) | **dropped**: driver builds `Outcome { read_set: ReadSet::default() }` (`driver.rs:3890`) |
| **Event (`ExecEvent`) derivation** | driver reads `exec_cache.events.last()` | backend reads its cache's `events.last()` (`real_process.rs:57`) | executor `CacheSource` → mapped in `FleetRun::note` (`lib.rs:822`) |
| **`DriveEvent` (Requested/Started/Completed)** | driver | driver | driver |
| **Memo keying / identity** | driver `pending_exec_identity_hash` **and** `exec_cache.keys` | driver identity **and** backend `ExecCache.keys` (re-derived) | driver identity **and** executor `comp_identity` (re-derived) |
| **Plan normalization** (`normalized()`) | inside `ExecCache.exec` | inside backend `ExecCache.exec` + `RealProcessTool` re-normalizes | executor side |
| **Role / ceiling enforcement** | `MountedWorld` answers `None` outside mounts — ceiling is **real** | "deliberately trusts the host," no sandbox; roles bound *staged* set only (`real_process.rs:1-6`) | seatbelt confinement **"not wired yet"** (`lib.rs:18`) |
| **read-set completeness** | observed (fake tool probes via `ObservedWorld`) | **declared-only** (stages declared inputs; cannot observe arbitrary host reads) | observed executor-side (real VFS intended) |
| **Placement** (which host) | local | local | `FleetBackend::choose` — RoundRobin / Gravity (`lib.rs:732`) |
| **Byte movement / gravity** | n/a | n/a | `ensure_mount` put/pull, `transfers` ledger |
| **Error taxonomy** | `Result<_, String>` | `Result<_, String>` (stderr folded into string) | `Result<_, String>` ("run failed") |

The matrix has one loud shape: **everything in the top ~two-thirds is
re-implemented once per lane**, and the implementations already disagree
(completion timing, join, read-set retention). Everything in the bottom third
(staging mechanics, launch mechanics, placement, byte movement) is genuinely
different per lane and *should* be.

---

## 2. The joint: invariant vs substrate-varying

### 2a. Invariant machine semantics — belong in the driver, exactly once

These are properties of *what a run means*, not *how a host runs it*. The
codebase's own doctrine already says so: `exec.rs:1-32` states the language
owns "what a key IS, when a cached result may be reused, what an observation
means," and `verify()` "lives HERE so that false-positive-never cannot be
implemented wrong by a runtime." That is the correct seam. The following are
all on the language/driver side of it:

- **Concurrency law** — spawn yields a *pending value*; nothing forces until
  demanded; a path demand waits only for that path. (This is exactly the law
  real-process violates.)
- **Memo keying** — identity = normalized plan × capability × mounts. One
  definition; `ExecPlan::normalized()` is the whole correctness of "`cc -c x.c
  -O2` == `cc -O2 -c x.c`."
- **Two-tier cache + `verify`** — tier-1 coarse key, tier-2 read-set cutoff.
  `exec.rs` insists this must not be re-implementable by a runtime. It is
  currently re-implemented by *every* runtime.
- **In-flight join** — "identical live runs share one process" is a property
  of the demand model, not of a substrate.
- **read-set retention** — the tier-2 pin, the anti-Nix pillar. Must survive
  the seam or tier-2 cannot work driver-side.
- **Event taxonomy** — `ExecEvent::{Tier1Hit, Tier2Cutoff, Ran, Joined}` is a
  language-defined vocabulary; which one a run *is* is a driver decision, not a
  backend's to label.

### 2b. Substrate-varying — genuinely per-backend

- **How to materialize a tree** on this substrate (in-memory vs tempdir+CAS vs
  remote CAS put/pull).
- **How to launch** (in-process vs `Command` vs RPC-to-executor).
- **Where it lands** (placement).
- **Byte movement / gravity scheduling** (fleet-specific data path).

### 2c. Where the trait boundary disagrees with the split

`MachineExecBackend::spawn(request) -> Arc<dyn MachinePendingRun>` hands the
backend the **entire run**: staging, launch, *and* the whole invariant top of
§2a (cache tiers, join, read-set, event derivation, completion timing). The
trait draws the line *above* the invariants instead of *below* them.

**FINDING.** The trait boundary sits at "own the run," but the doctrine in
`exec.rs` draws the boundary at "move bytes + answer point queries + launch."
The trait contradicts the codebase's own stated seam. Because the trait is
coarse, each backend must re-derive the invariants — and re-derivation is
divergence waiting to happen. It already happened twice (completion timing;
join).

---

## 3. Verdict and target shape

### Verdict (JUDGMENT)

`MachineExecBackend` is carving at the **wrong joint**. It bundles three
orthogonal axes into one strategy object:

1. the **invariant run lifecycle** (cache / join / read-set / events /
   concurrency),
2. the **substrate leaf** (materialize + launch),
3. **placement** (which host).

By giving all three to the backend, the seam makes the invariant lifecycle a
*per-backend personality*. The stax-caught serialization is not a local defect
in `RealProcessBackend` — it is the seam working as designed: the backend was
handed completion-timing ownership, and it chose "block." The fix that restores
the semantics minimally (separate lane) is correct as triage, but the seam will
keep manufacturing this bug class until the joint moves.

This is a historical accident, not a considered design: the local `ExecCache`
lane was never promoted to a backend (it is still an inline `else`), and the
two real backends were added as whole-lifecycle objects that each duplicate the
driver's job. The fleet doc already names the smell (`lib.rs:16-18`: the
two-tier cache on the producing path is "deliberately not wired yet").

### Target shape (JUDGMENT)

**Move the joint below the invariants.** The driver owns the full run
lifecycle; the backend shrinks to a substrate primitive that only knows how to
*materialize + launch + report progress*.

```rust
// Driver-owned: ONE cache, ONE join table, ONE read-set retention,
// ONE event derivation, ONE concurrency law.

/// A substrate can do exactly two things: put a tree somewhere runnable,
/// and launch a command, streaming back path-ready + finished + read-set.
trait ExecSubstrate: Send + Sync {
    fn launch(&self, plan: &ExecPlan, capability: u64, mounts: &[Mount])
        -> Result<Arc<dyn RunningProcess>, String>;
}

trait RunningProcess: Send + Sync {
    /// Non-blocking: has this path been produced yet?
    fn poll_path(&self, path: &str) -> PathState;
    /// Block until the process finishes; return outputs AND the observed
    /// read-set. read-set is MANDATORY — no `ReadSet::default()` escape hatch.
    fn join(&self) -> Result<Outcome, String>;
}
```

The driver's `ExecCache::exec` calls `substrate.launch` **only on the RUN
branch** — i.e. after tier-1 and tier-2 both miss. Everything above `launch` is
the code that already exists in `exec.rs`; it stops being copied into backends.

Consequences that make the serialization bug **unwritable**:

- There is no per-backend `spawn` that can block, because the backend no longer
  decides *when* a run is a run vs a cache hit. The driver holds the pending
  handle and blocks only inside `RunningProcess::join`, only when a value is
  actually forced.
- In-flight join moves to the driver: a `HashMap<identity, Weak<dyn
  RunningProcess>>`. A second demand for a live identity attaches instead of
  calling `launch` again. **Every** substrate inherits join; a substrate can no
  longer *lack* it.
- read-set retention becomes a type obligation (`join -> Outcome`, not `(Tree,
  ExecEvent)`), so no substrate can drop the tier-2 pin.

### The deeper framing: backend as *placement datum*, not strategy object

The prompt asks whether "where processes land" is a Rodin/placement decision in
the end state. **It already is** — `FleetBackend::choose` is literally
placement (RoundRobin / Gravity). Real-process is "placement = here, fleet of
one." So the two-backends framing conflates two orthogonal axes: *substrate*
(how to launch) and *placement* (which host). The end state the ValueBundle /
fleet design implies:

- A **Run is an ordinary `Pending<T>` store value** (the fleet doc's whole
  thrust: `PendingInvocation` ships as a `StoreValue` with schema `Pending<T>`,
  `fleet-on-the-machine.md:86-93`).
- **Placement is a datum on that pending value**, chosen by a scheduler
  (Rodin-shaped), not a strategy object baked into the machine.
- **"Real process" = fleet-of-one, placement = local, substrate =
  native-process.** There is no separate "real-process backend"; there is a
  native substrate that a local placement selects.

Under this framing there is exactly **one** lifecycle, **one** cache, **one**
join, **one** read-set, and the only per-substrate code is the leaf
"materialize + launch on host H." The serialization fork is categorically
unwritable because there is no per-backend spawn to block inside.

**OPEN (Amos's to rule).**
- **O1.** End state = single lifecycle with placement-as-datum (fleet-of-one =
  local, real-process collapses into a native substrate)? Or keep a thin
  backend trait but demote it strictly to `launch + report`? (Both kill the
  bug; the first also kills the `if let Some(backend) else` fork and the
  duplicate `ExecCache`.)
- **O2.** Who owns tier-2 when the run is remote? Driver-side tier-2 needs the
  read-set shipped *back* to the orchestrator; executor-side tier-2 needs
  identity shipped *out*. Today fleet keeps tier-2 executor-side and the
  orchestrator is blind. The fleet doc flags this exact question
  (`lib.rs:16-18`: "what does an L1 entry point at before the output hash
  exists?"). Is the orchestrator cache authoritative, with executors holding
  only a **byte** cache (CAS), not a **decision** cache?
- **O3.** Is in-flight join a pure driver concern, or does it need executor
  cooperation? Driver-side dedup covers *same-orchestrator* identical demands.
  Fleet's join is executor-side because *two different orchestrators* can hit
  one executor for the same run. Join may need to exist at **both** levels —
  worth ruling explicitly rather than discovering later.
- **O4.** Must the host-trusting real-process substrate grow a VFS to become a
  first-class citizen, or is "read-set = declared set" an accepted lower tier?
  (See divergence #6 — this changes tier-2 precision per substrate.)

---

## 4. Migration: today → target

Each step is independently shippable and pinned by a test that turns red
without it. "Dies" names the code the step deletes.

1. **Make read-set retention non-optional across the seam.**
   Change `MachinePendingRun::flush` (and the fleet path) to return `Outcome`
   (tree + read_set), not `(Tree, ExecEvent)` with the driver fabricating
   `ReadSet::default()`.
   *Pins:* a test that a real-process run's tier-2 cutoff verifies through the
   driver's cache — impossible today because the read-set is dropped at
   `driver.rs:3890`.
   *Dies:* `ReadSet::default()` at `driver.rs:3890`; the `(Tree, ExecEvent)`
   return shape.

2. **Move tier-1/tier-2/candidate logic out of `RealProcessBackend` into the
   driver.** The backend stops owning an `ExecCache`; `self.exec_cache` becomes
   the single decision cache; the backend's launch runs only on a driver-side
   miss.
   *Pins:* the existing exec-cache event tests, now run through the
   real-process substrate — oracle: identical `Ran / Tier1Hit / Tier2Cutoff`
   sequence whether the substrate is fake or native.
   *Dies:* `RealProcessBackend.cache`; `RealProcessRun.{outcome,event}`
   pre-computed at spawn.

3. **Split the trait; move completion out of spawn.** `launch` stages + spawns
   a background thread and returns immediately; `poll_path`/`join` block.
   *Pins:* a concurrency test — demand two independent native runs, assert both
   processes are live simultaneously. This is the stax-caught bug as a red
   test.
   *Dies:* the synchronous `cache.exec` inside `RealProcessBackend::spawn`
   (`real_process.rs:55`).

4. **Lift in-flight join into the driver.**
   `HashMap<identity, Weak<dyn RunningProcess>>`; a second demand for a live
   identity attaches instead of launching again.
   *Pins:* "identical concurrent demands join one process," now green for the
   native substrate (today only fleet).
   *Dies:* nothing in fleet (it keeps executor-side join for cross-orchestrator
   sharing per O3); the native lane gains join it never had.

5. **Collapse the no-backend `else`.** The fake-VFS lane becomes an
   `InProcessSubstrate` wrapping `tool_for`. There is always a substrate;
   default = fleet-of-one, local.
   *Pins:* all machine tests unchanged (default substrate reproduces current
   behavior).
   *Dies:* the `if let Some(backend) … else` fork in `ensure_run_started`
   (`driver.rs:3795-3822`); the direct `exec_cache.exec` call; `tool_for`
   dispatch moves behind the substrate.

6. **Reframe placement as datum.** `choose` becomes a scheduler decision that
   tags the pending value with a placement; the substrate is selected by that
   tag.
   *Pins:* `lua_builds_across_two_machines` still asserts ≥1 executor→executor
   gravity pull and a warm root memo hit that never consults the fleet
   (`fleet-on-the-machine.md:184-193`).
   *Dies:* `FleetBackend` as a monolith — its lifecycle bits already left in
   steps 1–4; its placement + byte-movement bits remain as a placement engine.

Fixed point after step 6: one lifecycle, one cache, one join, one read-set;
substrates are leaves (`materialize + launch + report`); placement is a datum.

---

## 5. Latent divergences — same bug class, not yet caught

Every item below is a semantic currently owned per-backend that can fork the
way concurrency did. Ranked by how live the landmine is.

1. **In-flight join — ALREADY FORKED.** Fleet has it; fake-VFS and real-process
   do not (`exec.rs:590` even documents "wire executors only — the local cache
   is single-demand"). This is the *same* bug as the concurrency evidence, its
   second face: identical concurrent demands run twice natively, once on the
   fleet.

2. **read-set retention — ALREADY BROKEN at the driver.** For *every* backend
   run the driver builds `Outcome { read_set: ReadSet::default() }`
   (`driver.rs:3890`). The tier-2 pin is stranded in the backend (real-process)
   or executor (fleet); the driver's own tier-2 memo is blind to backend runs.
   Sharper landmine: `verify` over an empty read-set is **vacuously true**
   (`.all()` over `{}`); if a backend `Outcome` with an empty read-set ever
   entered `exec_cache.candidates`, it would tier-2 **false-positive** — the one
   thing `exec.rs` swears can never happen. It doesn't fire today only because
   backend outcomes bypass `candidates` entirely. It is one refactor away.

3. **Event ordering already lies.** `DriveEvent::RunStarted` is emitted in
   `ensure_run_started` *just before* the backend spawn (`driver.rs:3777`). For
   real-process the whole process then runs synchronously *inside* spawn, so for
   N runs, run 2's `RunStarted` cannot fire until run 1 fully finished — the
   trace shows serialized starts and its timestamps misrepresent when work
   happened. Fleet emits started/finished around genuine progress. The
   observability stream itself diverges per backend.

4. **Cache-tier consultation order** is re-derived three times (driver
   `ExecCache.exec`, real-process `ExecCache.exec`, wire `RunTable` roles at
   `lib.rs:345-395`). tier-1-before-tier-2, candidate indexing, re-pin-on-cutoff
   — each hand-rolled. A fourth substrate could consult in a different order or
   forget the re-pin, and nothing would catch it.

5. **Memo keying / normalization** is computed in three places
   (`pending_exec_identity_hash`, `ExecCache.keys`, wire `comp_identity`). If
   one path forgets `ExecPlan::normalized()`, `cc -c x.c -O2` and `cc -O2 -c
   x.c` fork the cache **on that substrate only** — a cache-correctness bug
   visible on one backend and invisible on another.

6. **read-set completeness (tier-2 precision) differs by substrate.**
   Real-process "trusts the host" and stages only *declared* inputs
   (`real_process.rs:3-6`), so its read-set is the declared set, not the
   observed set — it cannot record the negative lookups that `exec.rs` calls the
   anti-Nix pillar (`exec.rs:359-372`). Fleet with a real VFS observes actual
   reads. Same plan, different read-sets, different cutoff behavior. The tier-2
   *precision* is a per-substrate personality. (See O4.)

7. **Ceiling / sandbox enforcement diverges — security-relevant.** Fake-VFS
   enforces the mount ceiling exactly (reads outside answer `None`).
   Real-process has **no** sandbox by design. Fleet's seatbelt confinement is
   **"not wired yet"** (`lib.rs:18`). So the ceiling — which `exec.rs` treats as
   an invariant ("the sandbox ceiling is exactly the mount set,"
   `exec.rs:124`) — is real in the toy lane and absent in the two real ones.

8. **Error taxonomy is stringly-typed everywhere.** All three return
   `Result<_, String>` — real-process folds stderr into a string, fleet returns
   `"run failed"`. The driver cannot distinguish "tool exited nonzero"
   (deterministic, cacheable-as-failure) from "path vanished" from "executor
   unreachable" (transient, must-not-poison-cache). A future backend could map a
   transient transport error to the same `String` as a deterministic compile
   failure; caching/retry decisions cannot be made uniformly. Latent, but the
   taxonomy gap guarantees it eventually bites.

9. **Staging determinism has no cross-substrate oracle.** Real-process maps
   logical paths to absolute tempdir paths (`physical_path`, `map_arg`,
   embedded-path replacement, `real_process.rs:269-300`). If a build's output
   embeds absolute staged paths, the native substrate and a remote executor
   produce *different bytes* for the *same* cache identity — the memo says
   "same," the artifacts differ. Determinism of staging is legitimately a
   substrate concern, but "identical plans produce identical outputs across
   substrates" is an invariant no test pins.

Items 1–3 are not latent — they have already forked. Items 4–9 are loaded and
share the mechanism: a semantic that `exec.rs` declares invariant is
implemented on the backend side of the seam, so each backend gets to have its
own version.

---

## Summary

- **The seam is at the wrong joint.** `MachineExecBackend` hands backends the
  whole run — lifecycle *and* substrate *and* placement — so the invariant
  lifecycle becomes a per-backend personality. The concurrency fork is the seam
  working as designed, not a stray defect.
- **The doctrine already exists in the codebase.** `exec.rs:1-32` draws the
  correct line: language owns keys/verify/join/events; runtime moves bytes,
  launches, answers point queries. The trait draws the line above the
  invariants instead of below them.
- **Target:** driver owns one lifecycle (cache, join, read-set, events,
  concurrency); substrate shrinks to `materialize + launch + report`;
  placement becomes a datum (fleet-of-one = local). This makes the
  serialization fork *unwritable* — there is no per-backend spawn left to block
  in.
- **At least three divergences have already shipped** (join, read-set drop,
  event ordering) and six more are loaded (consultation order, keying,
  read-set precision, ceiling, error taxonomy, staging determinism). Each is
  the same bug class as the one stax caught.
