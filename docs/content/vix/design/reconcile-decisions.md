+++
title = "three decisions from the reconciliation pass"
+++

# Three decisions from the reconciliation pass

Fable, 2026-07-08. The 158-rule reconciliation (verdicts in
`~/vixenware/notes/machine-spec/reconcile-verdicts.md`) surfaced three
questions only Amos can close. Long form, with context — you have not read
the dossiers and should not have to. Each section: what the thing is, the
concrete situation, the options with their cases, my recommendation. Nothing
proceeds on these until you answer; everything else in the rewrite plan is
already unblocked.

---

## Decision 1 — two hashing rules both claim to be settled; which epoch is which?

### What the thing is

Every vix value has a `ContentHash` — the blake3 component of its identity
pair `(SchemaRef, ContentHash)`. The spec's identity page currently contains
**two different definitions of how that hash is computed, both tagged
`[SETTLED]`**, and they produce different bytes for the same value:

**Mechanism A — framed encoding** (`identity.framed-encoding`): to hash a
value, *walk* it structurally and feed a framed, prefix-free stream to the
hasher — domain tag, then arity, then field index 0, then the field's hash,
then field index 1... For `Point { x: 1, y: 2 }` the hasher sees something
like `struct-domain ‖ Point-schema ‖ arity=2 ‖ field0 ‖ Int-domain ‖ 1 ‖
field1 ‖ Int-domain ‖ 2`, with framing bytes making ambiguous concatenations
impossible. **This is what the epoch you just closed actually implements**
(`hash_value_into` walking the descriptor).

**Mechanism B — canonical memory** (`identity.canonical-memory`): don't walk
anything. The value's in-store bytes ARE the canonical representation —
weavy-declared layout, all padding forced to zero — and the hash is
`blake3(those raw bytes)`. For the same `Point`, the hasher sees its 16
actual memory bytes, no framing, no walk. The rule's text says, verbatim,
"there is no separate canonical encoding to decode from." This came from the
hash-as-field proposal you reviewed.

These are mutually exclusive as *the* live mechanism: a value's hash is
either the walked-encoding hash or the raw-memory hash. Different bytes in,
different hash out, and every memo key, receipt, and persistence claim keys
off it.

### Why this is on your desk

You already ruled on the *direction* — the identity amendment (2026-07-08):
"the epoch's encoding-hashes are NOT sacred. **If** canonical-zero-padding +
flat-memory hashing **proves viable**, identity **migrates** to
canonical-memory hashing as a SECOND sanctioned epoch — its own break, own
gates, committee-ratified."

That amendment says canonical-memory is *conditional* ("if it proves
viable") and *future* ("migrates... its own break"). But the rule in the
spec carries the same `[SETTLED]` tag as framed-encoding, so a reader —
human or agent — cannot tell which mechanism to implement today. An
implementing agent hitting this page would have a coin-flip's chance of
building the wrong hasher.

### The options

**Option 1 — retag canonical-memory to `[DESIGN, epoch-2 candidate]`** (my
recommendation). Framed-encoding stays `[SETTLED]` as the current epoch's
law. Canonical-memory's rule text gains one sentence: "this is the epoch-2
candidate per the identity amendment; it becomes law through its own
sanctioned break, own gates, committee ratification." Matches the
amendment's own conditional language exactly. Cost: none — no code changes,
no commitment weakened, just the tags now telling the truth.

**Option 2 — `[SETTLED, rollout OPEN]` on canonical-memory.** Use this if
you consider the *decision to migrate* already made and only the
timing/gating open — i.e., "we WILL do this, don't relitigate the
direction." Then framed-encoding gets marked "current epoch only,
scheduled for replacement." Stronger commitment than the amendment's "if it
proves viable" wording; pick this only if your actual position is firmer
than what you dictated in the amendment.

**Option 3 — leave both as-is.** Rejected on the merits: two `[SETTLED]`
tags on mutually exclusive mechanisms is exactly the kind of ambiguity that
sends an agent down the wrong path for a day. This isn't a real option,
listed only for completeness.

**The question in one line: is epoch-2 "conditional pending viability"
(Option 1) or "decided pending rollout" (Option 2)?**

---

## Decision 2 — where does the daemon/capability spec live, now that the language spec has exactly three namespaces?

### What the thing is

Spec rules carry namespaced ids (`r[vix.identity...]` etc.), and the
foundation chapter closes the language spec's namespace set at exactly
three: `vix.*` (semantics), `vixc.*` (the compiler), `solver.*` (rodin).

But `capability.md` — the page about toolchains, fingerprints, poison — has
a preamble sentence written *before* that decision: "The capability packages
themselves (discovery, fingerprinting, grammars-as-data, the daemon) are
**`vixen.*`** spec territory; this page covers only what the MACHINE must
honor about them." Nobody ever checked that sentence against the
three-namespace ruling. So there's a dangling reference to a fourth
namespace that may or may not be supposed to exist.

### What would actually go under `vixen.*`

Concretely, the specs for the *product/infrastructure layer* — the things
that live in the vixenware repo, not in facet-cc:

- the **daemon** (vx-vfsd): virtual prefix mounting, the
  `tracked_observations` contract, sandbox launch services;
- the **runner**: capability advertising (which toolchains a node has),
  work dispatch, the MachineExecBackend host;
- **toolchain discovery and fingerprinting**: how a daemon probes and
  advertises "Apple clang 21.0.0, identity a42725b2…";
- **command grammars as data packages**: the per-tool argv grammars
  (rustc's, cc's) that capability packages ship;
- eventually the registry, overlays, cachet sealing infrastructure.

The language spec's capability page only states the *evaluator's* side of
the seam — "the daemon's advertised fingerprint is the single identity
authority; a probe verifies or poisons, never mints." The question is where
the *other* side's spec lives.

### The options

**Option A — `vixen.*` is real, but it's the product spec and lives in the
vixenware repo** (my recommendation). The language spec tree keeps its
closed set of three; `capability.md`'s preamble is rewritten to say "the
capability packages themselves are product-spec territory (vixen), specified
in the vixenware repo; this page covers only what the evaluator must honor."
Case for: the daemon/runner/registry are a separate deployable product with
its own repo, its own privacy boundary (the cloud control plane), and its
own release cadence — its spec churning inside facet-cc's language spec
would couple things you've deliberately kept apart. The mark VIXEN is alive
(naming resolved: "vixen — certifiable builds"), so the namespace name is
not stale. Case against: two spec trees means the seam (this page ↔ the
daemon spec) can drift, and dodeca coverage can't see across repos today.

**Option B — fold it into `vixc.*` here.** Treat the daemon as part of "the
implementation" broadly. Case for: one tree, one coverage story, no
cross-repo seam. Case against: the daemon is *not* the compiler — vixc "is
a compiler" per the chapter's own words — and stuffing infrastructure specs
under it dilutes the namespace's meaning on day one. Also drags
vixenware-private material toward the public repo, against the grain of
your publication calibration.

**Option C — strike the sentence, name nothing.** Delete the `vixen.*`
pointer; the language spec just says "the packages are specified elsewhere."
Case for: minimal, defers the decision until the daemon spec actually
exists as a document. Case against: loses the only breadcrumb telling a
reader where "elsewhere" is; the outline had already deliberately carved
this territory out, so un-naming it discards a decision rather than
deferring one.

**The question in one line: does the daemon/runner/registry spec get the
`vixen.*` name in its own (vixenware) tree (A), live under `vixc.*` here
(B), or stay unnamed for now (C)?**

---

## Decision 3 — how do runtime "tasks" relate to compile-time "islands"? (the ⊆ question, in words)

### What the thing is

Two units of granularity now exist in the design and the spec never says
how they relate:

- An **island** is a *compile-time* thing: vixc partitions the program into
  islands (straight-line eager interiors — ordinary fast code, no
  bookkeeping) connected by **edges** (where identity, memo, receipts,
  suspension, and safepoints live). This is the foundation chapter's
  centerpiece.
- A **task** is a *runtime* thing: the scheduler's unit of actually-running
  work. The old rule `task-is-path` says a task executes along a path and
  stops at three kinds of boundary: **joining** on another demand's result,
  **waiting** on an effect, or a **parallel split**. It was written before
  the word "island" existed.

The reconciliation can't rewrite `task-is-path` into islands vocabulary
without knowing the intended relationship. The symbols I used last time,
spelled out: "task boundaries ⊆ island edges" meant "**every place a task
can stop is at an island edge, but not every island edge is a place where
some task stops**." Subset, not equality, in that direction.

### A concrete example

```vix
let ys = xs.map(expensive_fn);   // fan-out: one demand per element
ys[3] + 1                        // consumer projects one element
```

vixc's partition puts edges at the map fan-out and around each
`expensive_fn` invocation; the `+ 1` lives inside an island interior. Now
two runtime scenarios at the `ys[3]` edge:

- `expensive_fn(xs[3])` is **already memoized** → the running task reads
  the memo and flows straight through. There was an *edge* here (identity
  was checked, the memo was consulted) but **no task boundary** — nothing
  parked, no scheduler involvement beyond the lookup.
- It is **not memoized** → the task **parks** on that demand and a slot is
  handed over. Edge *and* task boundary.

The claim: the second kind of event can *only* happen at edges. A task can
never park, join, or split in the middle of an island interior — interiors
are straight-line code with no scheduler contact. That's the "⊆".

One honest wrinkle the compressed version hid: **kills** are a separate
category. Replay-is-semantics says any task can be killed anytime; kills
land at *safepoints*, which live at edges AND at loop back-edges *inside*
interiors (so a long fused loop can still be killed). So precisely:
park/join/split boundaries ⊆ edges; kill points ⊆ safepoints ⊇ edges. Two
different subsets, and the rewrite should state both.

### The options

**Option A — ratify the statement above** (my recommendation):
tasks are runtime traversals through the island graph; suspension-shaped
boundaries only at edges; kills at safepoints (edges + back-edges). Case
for: it's consistent with everything already banked — round 2's
"executions run as weavy tasks (pending = yield, completion = resume)"
means yields happen exactly where pendings surface, which is edges; the
chapter's own edge enumeration lists "suspension may occur there." It gives
the rewrite a concrete sentence to put in the rule. Case against: it
commits the spec to a coupling ("scheduler stops only where the partition
cut") that the chapter itself stopped short of stating — if you have a
future in mind where the scheduler can cut somewhere vixc didn't (a
preemption point mid-interior that isn't a safepoint?), this closes it.

**Option B — retire "task" as a spec noun entirely.** Harder line: the
spec speaks only of demands, islands, edges; "task" is a weavy
implementation detail with no more semantic standing than "thread."
`task-is-path` gets struck rather than re-derived; the scheduler page
describes demand states, not tasks. Case for: maximally consistent with
"there is no machine, only an implementation" — tasks are how *this*
implementation runs demands, and the as-if law says that's not spec
material. Case against: round 2 explicitly banked task vocabulary
("executions-as-weavy-tasks", "tickets-outlive-tasks", "chaos mode randomly
kills tasks"), and the chaos-kill oracle needs *something* to name the
thing it kills; striking the noun means re-wording four ratified rules.

**The question in one line: do tasks stop only at island edges (A — codify
the coupling), or does "task" leave the spec vocabulary altogether (B —
demands and islands only)?**
