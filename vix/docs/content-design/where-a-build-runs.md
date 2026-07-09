+++
title = "Where a build runs"
+++

Status: PROPOSAL (round 9, from conversation). Placement, policy, and the price
of a build. Nothing here is implemented. The `Target::host()` finding in §2 is a
live bug in `vix/corpus-next/`.

## 1. Three sources, and each may touch exactly one thing

| source | says | may change the value? |
|---|---|---|
| the recipe (`.vix`) | what the value *is*; which capabilities, implicitly, via its commands | **yes — it is the only thing that can** |
| the capability | what a tool is and how it is identified (`rustc -vV`, `cc` identity) | no |
| the policy (vixen) | hard constraints and soft preferences over nodes | no |

Policy is not the recipe. That boundary was already drawn: `vixen.*` is product
spec, outside the language tree. A policy is a committed, versioned, readable
artifact — styx, not vix.

## 2. `Target::host()` is a plane smear, and it is in the corpus eight times

```vix
crate.vix:921    target: Target::host(),
crate.vix:588    let rustc = Rustc::acquire(unit.target);
rodin.vix:516    let rustc = Rustc::acquire(Target::host());
```

Three different machines are wearing one word:

- **target** — what the artifact is *for*. **Semantic.** It changes the value.
- **host** — what the compiler binary *runs on*. Cost-model, if hermetic.
- **executor** — which physical node. Cost-model.

`Target::host()` sets the *target* from the *executor*. Same recipe, a Mac and a
Linux box, two artifacts, two content hashes: **content addressing dies.** It is
the same disease as `machine.identity.canonical-memory` (ABI into identity) and
`map-order-independence` (user code into identity), and it is the most expensive
instance, because it reaches the artifact.

Cross-compilation makes it plain. A `rustc` on linux emitting darwin objects must
produce the same value as a `rustc` on darwin emitting darwin objects. If it does
not, that is rustc's reproducibility bug — measure it, don't paper over it by
pinning the host.

`Rustc::acquire(unit.target)` compounds it by parameterizing capability
acquisition on the *target*, as though the target selected the binary.

**The recipe never mentions the host.** Target is an ordinary argument. The
executor is never named. Then "build for the local arch but run it remotely, to
save battery" is trivially expressible, and nothing in the program knows.

## 3. "No programmer control" was never "no operator control"

The islands doctrine shuts the door on a *program* steering placement, because a
program that steers placement can make its value depend on where it ran. It says
nothing about the operator.

Placement steering lives in policy, and policy may be as bossy as you like,
**because policy cannot change the value.**

- **Hard constraints** — feasibility. Capabilities (derived from the recipe's
  commands), data residency, tenancy, budget ceiling. A plan that violates one is
  refused, not slowed.
- **Soft preferences** — an objective function. Price class, latency, battery,
  locality.

Data residency is the interesting one: it does not change the value, and it has
legal force. **Placement is compliance-relevant while remaining
value-irrelevant.** That is the seam the sovereignty thesis sits on.

## 4. The receipt splits, and the cachet must not see placement

- **Semantic receipt** — read-set, input identities, toolchain identity.
  Placement-independent. **This is what the cachet seals.**
- **Operational record** — node, provider, region, duration, RSS, cost.
  Placement-dependent. This is the invoice, and the residency audit.

> **If placement leaks into the cachet, two identical builds attest
> differently.** Two runs of the same recipe under different policies must
> produce the same cachet and different invoices.

That property *is* the product, and it is a tripwire: for any field you are
tempted to seal, ask whether a different node would have changed it.

## 5. Nobody discovers a sixty-dollar build afterwards

The answer to "this ran for an hour on an expensive provider" is not a verb in
the program — that would put placement in the semantic plane and cost you
reproducibility. It is three things.

**Predict, don't observe.** The scheduler knows each island's capability set
before dispatch. `vx explain <demand>` prints the plan — islands, required
capabilities, admissible nodes, chosen node, why, predicted cost — *without
running it.*

**Veto, don't steer.** Budget ceilings, provider allowlists and residency are
hard constraints evaluated at planning time. `providers: [eu-only]`,
`budget: 5 EUR`. The planner refuses, or asks. You cannot discover the spend
afterwards because the plan was refused before dispatch.

**Trace everything.** The bargain islands made — observable, never steerable —
holds, but money raises observability from a nicety to a product requirement. The
trace must answer: which islands, what capabilities, which nodes were admissible,
which was chosen, why, what it cost.

## 6. Do not right-size the cores. The question is ill-posed.

Nobody can answer "how many cores should this job get." The speedup curve is
sublinear and workload-dependent, and the numerator — developer time — has no
price. There is no Pareto frontier to sit on.

So: **dial it up.** Give the job the 64-core worker, in small time slices, and
make the scheduler good enough that sub-second jobs are economical. Cloud billing
is per-core-second; a wide burst for two hundred milliseconds is cheap. The
objective is not per-job efficiency, it is **makespan under hard constraints**,
and in a demand graph that means: throw width at the critical path and let
everything else fill the gaps.

What *is* worth learning, from receipts:

- **Duration** — to order the critical path. Wrong estimate: suboptimal order.
- **Read-set** — to know where the bytes are. Wrong estimate: a cold read.

Neither can make an answer wrong. Both are cost-model plane.

## 7. Outputs are memories that fade

Build outputs are content-addressed, so eviction is always safe: a miss costs
latency, never staleness. That makes the memory metaphor exact. A blob is *warm*
on a node that recently produced or read it, *cool* in the store, *gone* when
evicted — and recomputable at every stage.

**Placement should maximize warm reuse**, and the previous run's receipt tells
you what this demand will read. So:

> **The read-set is a placement oracle.** Score a node by the warmth of the blobs
> the demand is predicted to read. If the prediction is wrong, you pay a cold
> read; you never get a wrong answer.

Which makes placement structurally identical to the location plane's **candidate
nomination** — content-free, known at demand time, freely revisable, wrong only
in the cost plane. Two nominations, different scales, one law:
**nomination-never-validation.**

## 8. Sticky by default

Because placement decisions must be cheaper than sub-second jobs, and because a
node that just ran your job holds your memories:

- **Stay on the node.** Migration is the exception and needs a reason: a
  capability the node lacks, or a large cold input that lives elsewhere.
- **Back-to-back on the same node** is the default for a run of islands, not a
  special case. It is the locality optimization and the tenancy boundary at once.
- **Tenancy is a hard constraint**, not a preference. A node that ran your build
  holds your source in its VFS. It should not also be running someone else's.

## Open

- Does the **AST** teleport (every runner hosts `vixc`) or the **lowered island**
  (every runner hosts `weavy`)? Lowering artifacts are already epoch-scoped and
  as-if, so both are sound. It decides whether a runner can be a static binary on
  a machine you do not administer — which is the sovereignty story.
- Capability **class** is syntactic (union over reachable command grammars, a
  static over-approximation — costs perf, never correctness). Capability
  **instance** (which rustc) is negotiated at acquisition. Is the instance ever
  value-affecting? It must be, or reproducibility is a lie — so toolchain
  identity is in the semantic receipt, and the *node* that hosted it is not.
- Is **capability disjointness** a mandatory island cut criterion, alongside
  effects and unprovable demand? If yes, an island can never discover mid-run
  that it needs a capability it lacks, and the distributed demand graph stays
  statically analysable. This is the restriction that makes placement tractable.
