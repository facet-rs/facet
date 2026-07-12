+++
title = "vix"
+++

# What vix is

**The language is the build graph. The build graph is the language.**

Every build system contains a bad programming language. Make had one and
admitted it. Bazel has several and calls them configuration. Cargo pretends
not to have one until `build.rs`, at which point it has Rust but can't see
inside it. The industry's answer to "builds are programs" has been to write
the program in something too weak to be one, then bolt on caching,
sandboxing, and query engines to recover what a real language would have
given for free.

Vix inverts this. Vix is a real language — lazy, typed, content-addressed —
whose evaluation *is* the build. There is no separate graph format, no rule
DSL, no phase where configuration is "analyzed" into actions. A vix program
denotes values; some values are strings and versions, some are compiled
artifacts and test results. Demanding a value is what building is. The
dependency graph is not declared and not configured: it is *observed*,
exactly, as evaluation proceeds — which is why a cache hit in vix is a proof
and not a hope, and why the receipt of a build is its own decision trace
rather than paperwork bolted on afterward.

## Vix 101: description, not action

Everything is lazy. Everything is demand-driven. This is the born principle
from which the rest of the language falls out — and it is stricter than any
lazy language before it.

```vix
let p = Point { x: getx(), y: gety() };
```

This is not an instruction and it evaluates nothing. It *describes*: a value
`p`, whose `x` is `getx()`'s value and whose `y` is `gety()`'s value —
wiring, not action. `p.x` elsewhere in the program is likewise not an access
that "happens": it denotes a value that depends on `getx()`'s value and not
on `gety()`'s. There is no `force`, no `seq`, no strictness annotation — and,
unlike every lazy language before it, **no escape hatch**: nothing written in
vix can evaluate anything. A program's only power is to describe a graph of
values. Evaluation belongs to whoever holds the graph — the `vx` CLI, the
LSP, an audit — placing demand on roots; what gets computed is then decided
entirely by the wiring. Laziness in vix is not an evaluation *strategy*. It
is the absence of evaluation from the language's semantics altogether.

Everything is a value. There are no effect-objects, no monads, no statements
that "do" without denoting: if it doesn't evaluate to something tangible, it
does not exist. Values depend on other values — fully, or *partially* (a
dependency on `p.x` is a dependency on one field; the machinery of
projections exists to say so precisely). A value may be a literal, an
addition, a function application, a compiled crate, a solved dependency
graph, a test outcome. "Already computed" is not a different kind of value;
it is a *state* a value's computation can be in — residency — and it lives
in the implementation plane, never in the semantics.

This is why the language and the build system are the same thing.
`vx build app` is not a command interpreted by a tool that consults your
program; it is a demand placed on a value your program denotes. The LSP
asking "what's the type here" is a demand. Cargo-shaped resolution is a
demand on the solver's value. Test selection is a demand whose observed
read-set decides which tests could possibly be affected. One evaluation
model, one cache, one observability story.

## By value, as if

Everything in vix is by value. Semantically, every operation yields a fresh
value; nothing is shared, nothing is mutated, nothing aliases. This is not a
purity aesthetic — it is the load-bearing wall: values can be scheduled
across any number of threads and shipped between executors and machines
because there is nothing about them to coordinate.

The implementation honors one law — **the as-if law**: anything is permitted
that cannot be observed at the semantic plane. Value semantics make this law
unusually powerful, because without aliasing, vastly more transformations
are invisible and the compiler's proof obligations collapse. Instances, all
of them the same law:

- **Molten mutation.** A new value built from an old value whose last
  observer is gone may be built by mutating the old one in place. Vix code
  cannot know molten exists; it writes purely functional code.
- **Rematerialization.** A value consumed by eight islands may be computed
  eight times if that is cheaper than sharing. Sixteen threads may each
  carry their own copy of anything cheap, because nothing in the semantics
  can tell duplication from sharing.
- **Eager interiors.** Code runs eagerly exactly where no observer can
  distinguish eagerness from laziness (see Islands).
- **Suspension.** The semantic model of execution is replay — any in-flight
  work may be killed at any moment and restarted from its identity, losing
  time and never correctness. A suspended task is a discardable
  acceleration of replay, nothing more.

Streams deserve their six lines: an internal stream is a value of shape
(head, rest) where rest is a value like any other — the lazy unfold, as old
as SICP's cons-streams. No state in the semantics; a ring buffer under
uniqueness is molten's business. A genuinely stateful stream — stdin, a
socket — was never a value: it is an effect, edge-bound, journaled, each
read a pinned observation.

## Islands

Taken literally, "everything is lazy" is unimplementable at any speed worth
having, and "everything is memoized" is worse — you cannot memoize `a + b`,
and it would be senseless to: the memo entry costs more than the addition,
the identity hash costs more than the addition, the receipt costs more than
the addition. A naive demand-driven evaluator drowns in its own
bookkeeping. The drowning is measurable, and it is four orders of
magnitude.

The compiler's central job — the thing `vixc` *is* — is therefore a
partition. Between the AST and the lowered code sits the island graph:
**islands** of straight-line eager code inside, **edges** between. In the
semantic plane there are only values; in the compiler's plane, each value's
computation site is a **grain**, and vixc groups grains into islands. An
island's interior touches the machinery not at all — no memo, no identity,
no scheduler, no receipts; it is ordinary fast code, fused,
register-allocated, JIT-compiled. Every edge is where the language's
promises are kept: identity is computed there, memoization happens there,
receipts record there, suspension may occur there, and the safepoints live
there — one mechanism multiplexed for kill barriers, profiling counters,
and (someday) GC.

Eagerness inside an island is not a compromise of Vix 101; it is a theorem
about it — strictness analysis, as old as lazy languages, with one
difference that changes everything: **the analysis is not thrown away.**
GHC computes demand information and discards it into optimized code. Vixc
materializes it as a first-class artifact — inspectable, versioned,
load-bearing — because the partition decides the units of caching, the
granularity of incrementality, and the resolution of explanation.

Two forces cut the islands, and they are not symmetric. **Semantic cuts are
mandatory** — an edge must exist wherever the outside can see: at effects,
because receipts and memo policy live there; and wherever demand cannot be
proven, because there laziness itself is observable. **Everything else
belongs to the cost model**: sharing versus recomputation, island size
versus memo grain — chosen, not commanded, and revisable as the cost model
learns, because the as-if law guarantees no choice in this space is visible
to the program.

**No programmer draws islands.** Like codegen units, there may someday be a
knob for *how much*; there is never a knob for *which* or *how*. A
suboptimal partition is a compiler bug, not a user annotation opportunity.
The partition is observable (it is an artifact, with events and counters);
it is never steerable from inside the language. This door stays shut.

### The partition is a filter, not a key-maker

Memoization is keyed by **semantic value identity** — (operation,
identities of dependencies) — computable from the value graph alone,
partition-free. The partition decides *which* values get observed and
memoized: it is a filter over the key space, never the key-maker. The
consequences are load-bearing:

- **Deoptimization is fully as-if.** The JIT may split an island in twain
  or merge two at runtime: a new edge sits at a value that always had a
  well-defined identity (its entries were merely never recorded); a removed
  edge orphans cache entries harmlessly. Nothing invalidates.
- **Receipts survive repartition** — read-sets record values read, which
  are partition-independent.
- **Compiler upgrades cannot poison the cache.** A different partition
  reshapes which entries get used (a perf event, loudly reported), never
  their meaning. The true versioning surfaces are exactly two: the identity
  encoding itself (under the framed-encoding epoch discipline) and the
  lowering-artifact cache (always compiler-versioned, cheap to rebuild).
  Lowering determinism remains a goal for reproducibility, not a soundness
  requirement.

### Two identities, formally (the tier-1/tier-2 skeleton)

Every computation has two identities, and half the machine's historical
confusion came from smearing them:

- **Recipe identity** (tier 1): what would be computed — the operation's
  identity combined with its *inputs'* identities. Exec's plan+capability
  hash is this, for effects.
- **Value identity** (tier 2): what came out — the schema together with a
  `blake3` of the value's *framed, walked encoding*, plus, for effects, the
  observed read-set that vouches for it. Never a hash of raw memory: layout
  exists to be changed for speed, and identity must not move when it does.

The memo is the map from recipe identity to value identity. Downstream
demands key on the *value* identities of their inputs wherever possible —
which is where early cutoff falls out: change a standard-library function
and its recipe identity changes, so it recomputes; if the resulting value
is identical, everything downstream keys on that unchanged value identity
and stops. The recipe changed; the value didn't; the build notices in one
node. This two-plane structure is the same shape at every scale — pure
functions, exec (tier-1 plan / tier-2 read-set-verified world), and solver
warm facts — and the spec formalizes it once, generically.

## Lineage, and where it breaks from it

Vix stands on ancestors and says so. **Salsa** (and rustc's query system)
proved demand-driven memoized computation works for compilers — but Salsa's
granularity is manual: every query is a function the programmer chose to
make a query. Salsa is islands drawn by hand, forever, at function
boundaries. Vix computes the partition, moves it as the cost model learns,
and observes reads instead of trusting declarations. **Skyframe/Bazel**
proved restart-based demand graphs scale to monorepos — and also proved
what it costs when the build graph is configuration rather than a language:
over-approximated dependencies, "maybe" as an answer, and a restart tax
real enough that they retrofitted state. Vix keeps the replay foundation
and pays for suspension only as a discardable acceleration. **Adapton**
supplied the theory of demanded computation graphs; **GHC** the proof that
lazy semantics and fast code coexist when strictness analysis is taken
seriously; **Nix** the content-addressed conviction, minus the granularity
and minus the language.

The one-sentence version: *Salsa made queries; Bazel made graphs; vix makes
the compiler draw the graph through the queries, and signs the result.*

The ambition is not negotiable downward, because the premises interlock: a
language this lazy needs islands to be fast; islands need receipts to be
sound; receipts need content-addressing to be proofs; proofs need replay to
be auditable. Credibility comes from rigor — oracles, differential testing,
chaos kills, coverage-traced specification — and the design is accelerated
by eating itself: the compiler resolves its own dependencies, answers its
own editor's queries, and builds its own tests through the very machinery
it compiles. It can never be any less than that.

---

Specifications (rules traced to implementations via `r[impl ...]` /
`r[verify ...]`; see `ddc coverage`): the [spec section](/spec/).
Namespaces: `vix.*` — the semantics (this chapter is its preface);
`vixc.*` — the compiler (islands, lowering, JIT; it is a compiler);
`solver.*` — rodin. Implementation-charter rules migrate out of the
semantics during the vix/vixc rename sweep.
