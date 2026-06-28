# Consolidating the languages into facet

Design note. Captures the decision to bring all three of our custom languages —
**fable**, **gingembre**, and **vix** (new) — into the facet workspace on a shared
language-infrastructure substrate, and the architecture that falls out of it.

## The repo/visibility constraint (why this shape, not another)

Three repos today:

- `oss/facet` — **public** (~110 crates, ~500k downloads, too-big-to-fail). Home of
  the reflection core and ecosystem.
- `oss/dodeca` — **public**. Incremental static site generator. Heavy cells
  (image/video/jxl/webp/sass/vite/markdown…).
- `vixenware/vixen` — **proprietary**. The `vx-*` crate family + the demand-driven
  runtime (strands via `corosensei`).

A git repo has a single visibility, so "one repo for everything" is impossible while
vixen is proprietary. That's a hard constraint, not a preference.

Each repo currently hosts its own language:

- facet → **fable** (tiny typed language over Facet-reflected Rust values, lowers to weavy)
- dodeca → **gingembre** (Jinja-like templating)
- vixen → the vixen language (demand-driven)

We want those three to **share tooling** (parsing, CST/AST, diagnostics, LSP, queries,
runtime) without sharing a syntax tree.

## Decision: absorb the langs into facet — do NOT make a third repo

Mapping the dependencies shows the language "island" is **not** free-floating — it's
bolted to facet's fastest-churning core:

- `fable` already = `cstree + facet-core + weavy`.
- `picante` (the incremental query engine the langs want) = `facet + facet-core +
  facet-hash + facet-reflect + facet-postcard`. **Unextractable** from facet.
- `weavy`, `margin` live in facet. The whole thesis (reflection-powered AST) depends
  on `facet-reflect` by construction.

If we pulled `{gingembre, fable, vix}` into a **third repo**, its upstream edge lands on
facet's pre-1.0 core (reflect/hash/picante at `rc.x`) — can't pin a stable facet, so we'd
iterate against facet HEAD. That manufactures a **three-hop release train**:

```
change facet-reflect → cut facet release → bump+release langs → bump dodeca/vixen
```

Three coordinated publishes for one logical change, across the seam that moves most.

**The seam wants to go the other way.** Put the langs *in* facet (next to fable/weavy/
picante, where they already couple). Then reflect↔picante↔weavy↔langs are all path deps —
change reflection, `cargo build`, zero publish. The only boundary left is the one we
already cross daily: facet→consumers via published/git versions.

```
facet workspace ─────────────────────────────────────────────┐
  facet-core / facet-reflect / facet-hash / facet-value        │
        ↑          ↑          ↑                                 │
     picante     weavy      margin      [facet-native CST]      │
        ↑          ↑                                            │
   fable   gingembre(moved)   vix(new)   + shared lang-infra    │
└─────────────────────────────────────────────────────────────┘
        ▲ publish / git                  ▲ git
        │                                │
   dodeca  ── consumes gingembre    vixen (PRIVATE) ── consumes vix
   (heavy cells stay here)          (strands / vx-* stay here)
```

The heavy stuff (dodeca's cells) does **not** move — facet stays light. The langs are
small: gingembre = `cstree+facet+ariadne`, fable = `cstree+facet-core+weavy`.

Consumers keep the same downstream shape on both sides of the open/closed line:
**dodeca consumes gingembre via publish; vixen consumes vix via git** — exactly like
they already consume facet/vox today. The proprietary boundary stays where git can
enforce it.

Cost accepted: `dodeca↔gingembre` goes from path-dep (instant) to a publish edge. That's
correct — gingembre's real co-evolution is with picante/cst/weavy/margin (all in facet),
not with dodeca's site logic.

## Naming: `vix`

The open, generalized demand-driven language extracted from vixen becomes **`vix`**,
files `.vix`. The proprietary product **vixen** = `vix` + the closed everything-else,
and consumes `vix` via git (same shape as dodeca→gingembre).

- `vx`/`vx-*` is the proprietary system's crate stem (in vixen). `vix`/`.vix` keeps the
  open language lexically distinct and pronounces itself ("the vix language").
- The nix/lix/tvix rhyme is category-accurate: vix *is* demand-driven, so landing in that
  phonetic neighborhood telegraphs the evaluation model. The only cost is a recoverable
  "oh, it's not Nix-compatible" beat — one sentence of docs.

## Front-end: shared CST/AST infra, generic over `K` (facet-native, not cstree)

The three langs **share machinery, never a tree**. The shared crate is generic over each
lang's `SyntaxKind`; each lang instantiates it → three distinct, non-interchangeable trees
(`Tree<VixKind>`, `Tree<GingembreKind>`, `Tree<FableKind>`). Kinds and grammars are
per-lang; plumbing is shared. This is the cstree design, made facet-native.

### Why facet-native CST instead of cstree

cstree gives four things; only two are load-bearing at our scale:

1. **Homogeneous untyped nodes** → generic algorithms. *Keep* — but reflection gives the
   same uniformity dynamically (walk any facet tree, each node carries a span).
2. **Interning + structural sharing** → memory/incremental reuse for million-line crate
   graphs. **We don't have those** — config/template/doc files are kilobytes; cross-file
   incrementality lives in picante, not intra-file green-node reuse. Drop.
3. **Red-green lazy navigation** → cheap parent/offset at huge scale. Same — eager
   parent+span is free at our sizes. Drop.
4. **Losslessness + offset cursor** → needed for LSP/formatting at any scale. *Keep* —
   but it's a convention (keep all trivia tokens, carry spans), not cstree-only.

So a facet-native lossless CST is strictly *more* leverage: the whole tree reflects →
`rediff` (structural diff → incremental reparse + "what changed"), `facet-pretty`/
`facet-json` (mechanical syntax dumps for debugging the parser), generic span-cursor and
visitor — all written **once**, parameterized over each lang's kind enum.

```rust
enum VixKind { /* … */ }                              // per-lang, facet-derived
struct Node<K> { kind: K, span: Span, children: Vec<NodeOrToken<K>> }  // facet-derived
```

If a lang ever parses multi-megabyte inputs, interning/sharing become *internal*
representation swaps (`Vec` → arena+dedup, `Arc` the children) under the same reflected
API — a known, bounded future patch. Starting on cstree and wanting reflection later is
the painful migration (its green nodes are exactly what facet can't see into). So: start
facet-native, defer the scale tricks.

### The carve

**Shared infra crate** (generic over `K: SyntaxKind`):
- node/token representation — `Node<K>`, `NodeOrToken<K>`, builder
- generic cursor: node-at-offset, ancestors/descendants, span queries
- reflected traversal/visitor (no hand-written `visit_*` per lang)
- `rediff` diff, `facet-pretty`/`facet-json` dumps
- `margin` diagnostics rendering
- LSP scaffolding: loop, forked-`lsp-types`+facet, position↔node mapping
- *(decision)* parser **driver**: Snark owns a grammar-derived LR/GLR parser
  generator/runtime that emits a flat event stream; a generic builder constructs
  the lossless tree, attaching trivia generically — so each lang owns grammar
  semantics, not tree construction details (rust-analyzer split)

**Per-lang crate** (`vix`, `gingembre`, `fable`, each maybe `+ -syntax`):
- its `SyntaxKind` enum (facet-derived) — the only thing parameterizing the infra
- its lexer/scanner contracts plus grammar facts consumed by the shared parser driver
- its typed-AST accessors
- its lowering to weavy

Open knob: how a lang declares kinds — `impl SyntaxKind for VixKind` (trait) vs a
`#[derive(SyntaxKind)]` macro in the infra crate that wires it up (incl. which kinds are
trivia).

### Settled stack

- parsing → grammar-derived Snark LR/GLR parser generation/runtime
- CST/AST → facet-native lossless tree (above)
- diagnostics → `margin`
- LSP types → forked `lsp-types` + facet (no serde)
- queries → `picante` (not salsa)

## Back-end: the weavy ⋈ picante seam

The shared layer has **two** substrates. Front end = syntax infra (above). Back end =
**how a lowered demand-driven program runs incrementally.** weavy says how you *evaluate*;
picante says how you make evaluation *memoized and incremental*. fable already lowers to
weavy; gingembre and vix want weavy + picante.

A demand-driven language is the natural fit because **pure + lazy = memoizable by
construction** (this is why Nix is demand-driven). vix's semantics is essentially
picante-over-weavy.

### Integration points

- **Reads become tracked queries.** weavy's data-access ops route through picante so
  dependencies register and invalidation works. The whole game.
- **Purity discipline.** Memoization needs determinism keyed by inputs; weavy programs
  under picante must be pure modulo declared (query) inputs.
- **Memoization granularity.** Which weavy ops are picante query boundaries vs plain
  execution — too fine = per-node overhead, too coarse = over-recompute.

### Resolving sync/async and JIT cost (the two sharp edges)

1. **weavy becomes async-aware; sync mode runs under a no-op ambient runtime/waker.**
   Eval is poll-based. Under a no-op waker a fully-cached program polls straight to
   `Ready` in one shot — zero async tax on the hot path. Only a cold picante query
   returns `Pending` and hands off to picante's real runtime. "Sync" = "async that never
   parks." This subsumes picante's asyncness without stackful strands.

2. **picante read/write ops get an IR version** so weavy's optimizer can see them —
   hoist redundant reads out of loops, CSE, batch dependency registration — instead of
   paying an opaque host-call per read (which would defeat the JIT entirely).

**The interlock:** the picante-read IR op is *both* the optimization surface *and* the
only suspension point. Because a suspend can happen only at a known IR op, the JIT emits a
two-path stencil (ready → continue inline; cold → suspend) **only at reads**, straight-line
everywhere else. Suspension points are static → no need to park an arbitrary native stack
→ strand-free, and the eval core stays **open** in weavy (not pulling proprietary
`corosensei` in).

**Cost accepted:** the JIT must make live state recoverable at read points — an IR-level
state-machine/CPS split around reads (segments between reads; suspend saves segment + live
locals). More compiler work in weavy, but open and in facet — the right call for an open
vix.

### Why async-aware, not stackful strands (corosensei)

This was an explicit decision against the strand model vixen uses today. Three things
async buys that stackful coroutines structurally can't:

1. **IO composition is free — and it's the point of a demand-driven lang.** Leaf queries
   fetch (HTTP, files, DB), and that ecosystem is async-to-the-bone (picante is already
   async). A leaf that `reqwest::get().await`s just propagates `Pending` up through eval.
   With strands you `block_on` per in-flight IO → one OS thread + one full native stack
   parked per outstanding request. Doesn't scale with fan-out.
2. **Cancellation under invalidation.** Input changes → in-flight computations go stale.
   Async cancels by dropping the future; picante invalidation and async cancellation
   compose for free. Cancelling a parked strand means manual unwind or a poison-resume.
3. **Unbounded depth.** Demand graphs nest arbitrarily; fixed-size strand stacks overflow
   (already hit in vixen). Owned/heap frames grow as needed — the overflow is the
   fixed-stack model meeting an unbounded-recursion workload, not a tuning bug.

**The honest counter (vixen's own rationale):** strands were chosen because *"each strand
is a real stack we can dump, step through, and off-CPU-profile — which async futures (a
shredded state machine, no persistent stack while parked) cannot give us."* Observability
is the thing we prize most, so this can't be waved away.

**Resolution — we own the state machine; rustc doesn't have to.** That objection is true
of *Rust `async fn`*: its futures are zero-cost *because* they're opaque (anonymous
compiler-generated state machine, nothing inspectable while parked). Since we own weavy's
eval, we take the other side of that trade *on purpose*: materialize the parked state as
**our own facet-reflectable structure** (value stack + IP + the query it's blocked on) —
which is *already* what the JIT's state-machine split requires. A parked computation is
then our data, dumpable/serializable via facet — arguably more inspectable than a raw
native stack you'd have to symbolicate. We keep strands' one advantage and gain the other
three.

**And the cost lands where it doesn't hurt.** rustc can't pick which awaits are hot, so it
makes them all opaque-zero-cost. We can:
- **Ready/hot path** (no suspension, no-op waker → polls straight to `Ready`): stays
  zero-cost and straight-line; no frame materialized; nothing given up.
- **Suspension path** (only at picante-read / IO leaves): materialize the explicit
  dumpable frame — but that's exactly where you were *already* stalling (cold query,
  network, disk). Observability there is essentially free.

So it isn't "zero-cost vs. observable" globally — it's **zero-cost on the straight line,
observable precisely at the parks**, and parks are sparse and already-cold.

Operationally: **"async-aware" means weavy owns its suspended-state representation and is
async only at the boundary** (awaits ecosystem futures at IO/query leaves). The trap to
avoid is writing the interpreter as `async fn` and inheriting opaque futures — that's the
version that would cost us exactly what strands gave us.

### Constraint: don't bake picante into weavy

weavy is the *general* lowered-program substrate — fable lowers to it with no picante at
all. So read/write must be **host-defined IR ops with a declared effect algebra**
(read = pure value + idempotent tracking → legal to CSE/hoist/dedup), and picante is the
*first client* of that mechanism, not a special case in weavy core. The optimizer reasons
from declared properties, not hardcoded picante knowledge — so gingembre's HTML-escaping,
vix's data access get the same optimizable treatment for free.

Back-end track in one line: **weavy = poll-based eval + extensible host-ops-with-effects;
a thin picante-glue defines its ops over that; the no-op waker is the standalone/
fully-cached fast path; suspension is JIT'd state-machine splits at read ops only.**

## Anti-drift protocol (governs execution)

These plans don't die from bad architecture — they die from a hundred small silent
substitutions, each individually reasonable, that together converge somewhere nobody chose.
Drift is the primary adversary. Engineer **detection**, not willpower.

The core lever: **every migration step has an oracle, and an oracle is a drift detector.**
"Same source → equivalent tree", "non-incremental render == incremental render", "dodeca
renders identically" — an agent physically cannot ship a degraded CST or a stubbed renderer
past a rediff or a render-equality gate. Drift becomes a red test, not an invisible
divergence. So make the oracles gates that nothing bypasses.

Rules:

- **This note is the contract.** Committed in-repo; it survives context summarization (when
  an agent's context compacts, the note doesn't — it's the anti-amnesia device). It changes
  *only* by deliberate edit reviewed with Amos. **Code and note must never silently
  disagree — divergence between them is the drift alarm.**
- **One step → one watchable Paseo agent → one commit → one reviewed diff.** Agents do not
  chain steps. The boundary between steps is the review checkpoint where drift is caught
  before it compounds. The step-N agent does not "while I'm here" into step N+1. (Watchable
  Paseo agents only — never the invisible built-in subagent that can drift or die unseen.)
- **No silent re-delegation: one tasked agent = one executor.** An agent given a scoped
  implementation task must *do it*, not spawn a further sub-agent and go idle. Re-delegation
  adds an unwatched layer and breaks the finish-notification chain (the grandchild notifies
  the idle middle agent, not the orchestrator), so work can silently stall or die unseen.
  Implementation prompts must say "do this yourself; spawn no further agents." (Observed in
  the dodeca repoint: a sonnet agent re-delegated to a codex worker and went idle; the chain
  had to be re-engaged by hand to surface the result.)
- **Each agent's prompt quotes its plan section + its oracle as the done-condition**, and
  forbids scope beyond the step. A pointer at the note, never a paraphrase — paraphrase is
  where drift is born.
- **Stuck → commit WIP, report the negative result, stop.** Never simplify-to-green, never
  disable/revert to "see if it works with less." That move destroys the codebase and
  converges far from intent. A negative result is reported and discussed, not hidden.
- **After every structural step, prove a real consumer on the production path** (dodeca
  builds + renders identically; vixen builds). Integration over unit — the best drift
  detector here.
- **The metric is fidelity-to-plan, not green tests.** Passing tests never stand in for
  faithful to the plan. Use **tracey**: lift these invariants into spec rules, annotate the
  code, let coverage be mechanically checked so faithfulness is a number that can go red.

Step-specific traps and their gates:

- **CST migration** — "approximately" reproducing cstree, losing trivia/span fidelity →
  gate: rediff old-vs-new tree over a corpus.
- **"Generic over K"** — premature abstraction for imagined languages → rule: never add a
  generic knob until a *second real consumer* demands it. Two instantiations or it is not
  generic, it is speculation.
- **weavy async** — sliding into `async fn` and inheriting opaque futures → gate: a "dump a
  parked computation's state" test. If you can't dump it, you drifted.
- **vix scaffolding** — scope explosion into "build the whole language" → first milestone is
  deliberately tiny (tokenizer + minimal grammar + typed AST) and gated.

Sequencing note: front-end and back-end tracks are *logically* parallel, but don't run both
novel tracks under live agents at once — review one hard thing at a time. The mechanical
move (step 1) can run while design continues; the two novel tracks (CST migration, the
weavy/picante seam) get anchored one at a time.

## Migration plan — progressive steps, each with an oracle

Never combine a move with a rewrite. Two converging tracks.

**Front-end track**

1. **Move gingembre into facet, unchanged.** Mechanical relocation (gingembre +
   gingembre-syntax, cstree and all). dodeca flips from path-dep to consuming it
   (git-pin/publish). *Oracle: still builds, tests still pass.* De-risks the boundary.
2. **Build the facet-native CST by migrating gingembre-syntax onto it.** Refactor with a
   built-in oracle: *same source → equivalent tree.* cstree leaves here.
3. **Generalize the proven CST into the shared infra crate.** One real consumer keeps the
   abstraction honest.
4. **Scaffold vix as the second consumer.** Validates "generic over `K`". Tokenizer +
   grammar facts + typed AST first.
5. **Fold fable in** as the third consumer.

**Back-end track** (parallel)

- **gingembre → weavy → + picante seam**, proven on gingembre first. *Oracle:
  non-incremental render == incremental render, same output.* Then generalize; vix
  inherits a runtime substrate that already works before vix exists.

Spine: **move → migrate-with-oracle → generalize → vix (second consumer) → fable (third)**,
front and back converging on vix.

**Step 1 is the first domino and it's purely mechanical.**
