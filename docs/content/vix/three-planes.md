+++
title = "The three planes of identity"
+++


*Status: design chapter (Amos + Fable, 2026-07-08 — the location-plane
conversation). Everything agreed in that conversation is stated here as
design; formal rules get extracted in a later pass, and two empirical
inputs are still in flight (the hash-consumption census and the prior-art
grounding). This chapter is written as a textbook, because the spec is a
textbook: it exists so that we — and anyone who walks in later — can
contend with this machine, its opportunities, and the rationale for why it
is so different from everything else they know.*

## The story this chapter must explain

You pass a tree — say, a source directory of three hundred files — to a
build step. The build runs, and the machine observes that it read sixty of
those files. Tomorrow the tree is different: someone touched a README the
build never looked at.

What should happen is a two-step dance:

1. **"Is the whole thing the same?"** Ask for the tree's stable content
   identity (its Merkle root). If it matches yesterday's, reuse the
   output with no further questions. One comparison.
2. **"Fine — is the part you *used* the same?"** The whole-tree identity
   differs, so consult what yesterday's run *recorded*: "of that tree, I
   read these sixty files, and here are their hashes." Check those sixty
   against today's tree — cheap, because Merkle trees make per-path
   lookups cheap. All sixty unchanged? Reuse the output. The README was
   never your problem.

Two opportunities for early cutoff: one coarse and O(1), one fine and
proportional to what was actually used. This is the concrete thing vix
does that Nix structurally cannot: Nix has step 1 only — whole-input hash
or nothing. Step 2 requires *observed read-sets*, and observation is the
machine's founding move.

But there is a question hiding inside step 2, and it stayed hidden until
we tried to formalize the dance: **when the input changed, how did we find
yesterday's entry at all?** The memo key contains the input's content
hash. The content changed. The key changed. By content alone, yesterday's
run is unfindable — the very entry whose read-set we need is filed under a
name we no longer know. Candidate nomination was hand-waved everywhere
this dance was described, and it is the load-bearing missing piece.

## The answer: a value has three identities, not two

The foundation chapter established two planes: **recipe identity** (what
would be computed — the operation plus its inputs' content identities) and
**value identity** (what came out — schema plus content hash). The dance
needs a third, and it is the one that stays stable while the other two
churn:

- **Location** — *where this value lives in the demand graph.* The path
  from the demand root down to this computation, built recursively from
  the location hashes of the path above — never from anyone's content.
  "The rustc invocation for this crate in this workspace's build," as a
  hash. Stable across input changes, across runs, across weeks.
- **Recipe** — *what would be computed here.* Operation identity combined
  with input value identities. Changes whenever any input's content
  changes.
- **Content** — *what came out.* `(SchemaRef, blake3)` of the result.
  Exists only after evaluation, at which point it is trivial: the value is
  materialized, hash its contents.

The memo store is **indexed by location** and **carries** recipe, read-set,
and result. The dance, re-told precisely: walk to the location (known
before evaluating anything — see below). Does an entry there have our
exact recipe? Exact hit; done. No? Do any entries there have a recorded
read-set that re-verifies against today's world? Projection hit; done.
Otherwise evaluate, and file what we observe — at this location, for the
next run to find.

## The law that makes the location plane safe: nomination, never validation

The location hash **nominates** candidates; it never **validates** reuse.
Validation is always content-grounded — recipe equality, or read-by-read
re-verification of a recorded read-set, verified never trusted. A wrong or
unstable location function therefore costs *performance only*: missed
candidates mean spurious rebuilds; bloated candidate sets mean wasted
verification. It can never serve a stale value.

This one law is what makes the whole plane buildable. It puts the location
function on the same footing as the island partition: an implementation-
plane choice, *chosen not commanded*, revisable as the cost model learns,
shippable as a v1 heuristic with a diagnostic. There is no epoch
discipline here, no committee gate on changing it, no need to get it right
the first time — because it cannot be wrong in the direction that matters.
Contrast the content plane, where changing the hash construction is a
sanctioned identity break with its own gates. The two planes have opposite
change economics, and that is by design.

## Why locations are computable at all: the demand-time property

A location would be useless if computing it required evaluating the thing
it names. It doesn't, and the reason is structural: **a demand's location
is known at demand time.** When evaluation reaches a call site, the path
taken from the root to here is already in hand — before the subtree below
is evaluated, which is exactly the moment the cache should be consulted.
Locations are *recorded* during the first run the same way read-sets are,
and recomputed incrementally top-down on later runs. Observed, not
declared, like everything else. Static analysis can only improve how
*early* a location is known (prefetching); it is not load-bearing.

## What goes in a path: names, not bodies

If locations incorporated the *content* of the functions along the path,
editing a function's body would relocate everything demanded beneath it —
every downstream nomination would miss, and the warm cache would go cold
precisely when early cutoff should shine. So paths are built from
**names**: the function's name, not its lowered bits.

Follow the consequence through the chapter's own stdlib example: edit a
standard-library function's body. Its recipe changes, so it recomputes.
Its output value comes back identical. Every downstream demand sits at an
*unchanged location* with an *unchanged recipe* — exact hits all the way
down. One node recomputed; the location plane is the mechanism that
*finds* the entries the two-plane story said would be reused.

The price: renames and structural refactors (an extra wrapper in the call
path) churn locations below them — a one-time cold rebuild, correctness
intact. We accept this. There are physical limits to what can be done
here, and machinery to outsmart them (compiler-proved path equivalences)
would be bug-prone and would likely cost more than it saves. The cheap
mitigation that *is* safe: when a full path misses, **nominate by suffix**
— any prior entry whose path ends the same way is a candidate, and
verification decides, as it always does. No proofs; the verifier does the
correctness work; the nominator is allowed to be merely good.

## Fan-out: key by the input's position

`xs.map(f)` issues one demand per element. Same function, same call site,
N times — what disambiguates the locations? Not the element's *content*
(that forces evaluation, and the point is to know locations first). Not
the *output's* position (derived aggregates observe in canonical value
order; output position is rank, which depends on everyone's content). The
answer is the **input's** position: an array is a struct whose fields are
named 0, 1, 2 — so element *i* already has a content-free location,
`location(xs) ⊕ field i`, and the per-element demand is located at
`(call site, that location)`.

Locations then *flow through derivations*: the element of `ys` that came
from `xs[i]` carries its origin, so a further `ys.map(g)` chains off it —
even across `sorted()` or canonical reordering, because an element's
location is where it *came from*, not where it now sits. Location is
provenance, literally. (This is the problem React makes users solve by
hand-writing `key=` — the framework can't see provenance. We can; the
dataflow is the program.)

## Not every demand has a useful location — and that's a classification, not a failure

A convergence loop feeds its own output back. Its iterations have no input
position; disambiguating them by depth is possible but smells wrong — and
the smell is informative. Ask what nomination-by-location would *buy* such
a demand: "same place, different content" is the situation the location
plane serves — stable structure, varying data. A trajectory demand is the
opposite: its content *is* its structure. The reuse it wants — "I have
seen this exact state before" — is content-level dedup, which the memo
already does with no location at all (and which is how solver warm facts
already key: by premise content, position-free).

So demands come in two classes, and the store indexes them differently:

- **Structure-anchored**: arguments carry stable locations (positional
  fields, names, provenance coordinates). Indexed on the location plane;
  eligible for the full nomination dance.
- **Trajectory-born**: arguments exist only as evolving content. Indexed
  by content only — exact recipe dedup, no projection-by-place, because
  "place" is not a meaningful notion for them.

Most of a build graph is structure-anchored. Most of a solver's interior
is trajectory-born — and is *also* mostly not demands at all (interior
iteration is molten, one island, one edge). The classification costs
nothing and spends effort where it pays.

## External inputs: provenance coordinates

Values that enter from outside the program need locations too, and theirs
come from provenance: a watched tree on a developer's machine is located
at (daemon installation UUID, local path); a registry crate at (registry,
package, version). Content-free, stable, and exactly the vocabulary the
receipts and journal already speak — the location plane is where "this
came from *that* laptop" was always going to live. (*"Provenance
coordinate" is a term coined in this conversation — the halves are earned
elsewhere (SLSA provenance; Maven coordinates), the compound is ours.*)

## The store this implies is not a dumb KV — sketch, pending grounding

A hundred thousand prior runs must be queryable by location, including
suffix matches, without scanning. The shapes (stated as a sketch; the
prior-art pass will confirm or correct):

- **Nomination index: a trie over reversed paths.** Locations are name
  sequences; index them leaf-first, and longest-suffix matching is a walk
  proportional to path length, not store size. Exact hits are the full
  walk; candidate sets are bounded by per-node retention policy — a cost
  knob, never a soundness knob.
- **Verification stamps: the red-green move.** Every entry carries the
  world-version at which its read-set last verified. The daemon owns the
  delta stream, so "did anything this entry read change since?" is an
  intersection of the entry's read-set with the changed-coordinates set —
  not a re-walk of the world. Nothing relevant changed since the stamp →
  accept in O(1). Merkle roots short-circuit hierarchically.
- **Filters for the intersection.** Read-sets and delta-sets as
  approximate-membership filters, false positives triggering a real
  verification (the safe direction). Candidate structure: ribbon filters
  or successors — to be grounded, not assumed.

Prior-art posture — grounded (the pass ran 2026-07-08; full report:
`~/vixenware/notes/machine-spec/location-plane-prior-art.md`):

- **Nomination-never-validation is restated doctrine, not invention** —
  `vx-store-as-vix-memo.md` already specified the five-step lookup order
  (local exact → persistent exact → local projection → persistent
  projection, each verified → spawn), and the machine spec ratified it as
  rules. What no page specified was how a projection candidate is *found*:
  `machine.persistence.trait-boundary` literally stubs "enumerate
  projection candidates" with no backing mechanism. The location plane
  fills a real hole in our own ratified spec.
- **Names-not-bodies has two shipped precedents inside this very system**,
  reached independently: exec mounts at role-stable paths ("content-derived
  paths would leak the world into the identity and kill tier 2") and
  capability *names* not indices in lowered programs (survives registry
  reordering across reload). Same doctrine, third layer.
- **Nominal Adapton**: the thesis transfers (names ≠ content, used only to
  find reuse candidates, correctness by construction); the allocation
  mechanism does not — Adapton's names are programmer-allocated and
  opt-in (explicit name-forking through loops), which "no programmer draws
  islands" rules out on principle. Our derivation is automatic and
  structural (call site + input position), and must be.
- **Salsa's stamps solve a different sub-problem than the trie** — this is
  the pass's sharpest scoping correction. Salsa's durability/revision
  shortcut makes re-verifying a dependency edge *you already have* O(1);
  it never needed nomination because its query keys are exact. So in this
  design: the trie *finds* the candidate; the stamp makes *confirming* it
  cheap. Complementary mechanisms, and the spec should scope them as such.
- **Skyframe's SkyKey** proves stable nominal keys work at production
  build-graph scale — but it conflates nomination and validation in one
  key (no separate content-verification layer) and has no fallback when
  the exact key misses: a renamed target is an unrecoverable cache miss.
  The suffix-match fallback is exactly what it lacks. Watchman's clockspec
  is the clean precedent for the daemon-owned monotonic delta stream.
- **Ribbon filters** (Dillinger & Walzer 2021; BuRR follow-up) are
  confirmed current and fit the false-positives-are-safe direction
  precisely. But probabilistic structures would be a *first* for this
  corpus — every existing "did what I read change" check is exact, and
  the corpus's one worked analog of this intersection problem chose an
  exact inverted index (see the warm-facts question below). Choosing
  probabilistic here is a scale-driven judgment call to make consciously,
  not a default.
- **Genuinely new territory, confirmed by absence in both corpora**: the
  content-free path identity for demand-graph nodes itself, suffix
  matching as fallback nomination (no build system or paper does it —
  nearest citation is GreenTrie's constraint-entailment caching, already
  adjudicated in the corpus for the solver domain), and the reversed-path
  trie as nomination index.

## What this chapter deliberately leaves open

1. **Path compression policy** — how much of a location to keep before
   collapsing to waypoints (demand root, effect boundaries, closure
   names), and how suffix-match weighting ranks candidates. Cost-model
   territory; safe to iterate.
2. **Trajectory-class edges** — the exact rule for when a demand is
   trajectory-born, and whether depth-disambiguation is ever worth
   offering as a nomination hint.
3. **The mechanism question underneath content hashing** (framed encoding
   vs canonical memory) — a separate adjudication, now better-posed: most
   values only ever need a location; only edge-crossing values need
   content identity at all. The consumption census will say whether the
   implementation agrees.
4. **Vocabulary ratification** — "location hash," "provenance coordinate,"
   and the plane names themselves await the explicit *bank it*.
5. **The warm-facts fork — RESOLVED by mechanism mapping, pending Amos's
   ratification** (full verdict:
   `~/vixenware/notes/machine-spec/warm-facts-fork-verdict.md`). All 18
   mechanisms of `warm-facts-spec.md` were mapped against the ratified
   adjudication requirements and the generic machine spec. The fork splits
   along soundness vs. acceleration: **every soundness mechanism is
   subsumed** — several by rules that cite warm-facts *by name*
   (`proof_digest` → `hashing-is-ambient`; replay-before-install →
   `reverify-on-load`, "same rule for exec tier-2, projection candidates,
   and warm solver facts, stated once"; absence witnesses →
   `misses-recorded`; quarantine → `value-vs-claim`, "not a bespoke
   path"). **One genuine residue: the exact inverted `premise → facts`
   index — as acceleration/nomination only**, which is precisely the
   candidate-nomination hole this chapter fills. So the old spec's index
   survives as this chapter's worked *precedent*, not as machinery.
   Nothing is contradicted. Scale pricing for the filter choice: solver
   scale is ~1–2 premises/fact and absence keys in the low hundreds —
   exact indexing is free there; this chapter's ~100k-run target is 2–3
   orders larger, so probabilistic-over-exact is a conscious scale
   departure whose crossover is a *measurement*, not a corpus answer.
   Recommendation on Amos's desk: retire the bespoke
   `Premise`/`WarmFactVerifier` subsystem (soundness), keep the index as
   the nomination precedent.
