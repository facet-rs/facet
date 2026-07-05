# Reuse Analysis

This is the design spec for reuse analysis: the one new compiler feature the
Rodin-in-vix bet (`docs/design/rodin-in-vix.md`) needs to make Spike B (a pure
functional CDCL kernel) viable. It lets linear-mutation algorithms — trail
push/pop, watch-list moves — be written in pure functional style (`trail' =
trail.push(lit)`) and have the compiler lower uniquely-owned functional
updates into in-place mutation, so the "functional" program runs at
near-imperative speed when nothing aliases the value being updated.

Citations below are file:line against this checkout unless noted.

## Part 1 — Prior art, precisely

**Perceus** (Reinking, Xie, de Moura, Leijen, PLDI 2021, "Perceus: Garbage
Free Reference Counting with Reuse"): a precise reference-counting scheme
inserted by the compiler (explicit `dup`/`drop` ops at every use/scope-exit,
derived from a static last-use analysis) plus *drop-reuse fusion*: when the
compiler sees a `drop x` immediately followed by the construction of a
same-shape value, it fuses them into a *reuse token* — the dropped
allocation's address is threaded to the constructor, which reuses it in place
if the token is non-null (i.e., if `x`'s refcount hit zero, meaning this was
the last reference) and falls back to a fresh allocation otherwise. FBIP
("functional but in place") is the resulting programming style: write
persistent/functional updates, get in-place mutation when nothing else holds
a reference. The last-use analysis and reuse-token insertion are static
(compile time); the branch on "was this actually the last reference" is a
runtime refcount check.

**Lean 4's runtime**: RC-based, no static uniqueness types at all. Arrays and
strings expose functional update APIs; internally, `Array.set`/`push`/`pop`
check the object's refcount at the call site — refcount==1 mutates in place,
anything else copies first. This is the same reuse mechanism as Perceus but
applied ad hoc to specific builtin container ops rather than derived from a
general drop-reuse compiler pass over user code.

**Koka** implements the full Perceus pipeline (this is where the PLDI paper's
implementation lives) and adds "borrowed" parameter annotations as an
optimization to avoid unnecessary `dup`/`drop` traffic — not required for
correctness, purely to cut refcount-op overhead.

**Clean's uniqueness types**: the static alternative — a type system tracks
which values are guaranteed to have exactly one reference, so the compiler
knows at every call site, with no runtime check, whether an update can be
in-place. Correct and check-free at runtime, but the type system leaks into
every function signature that touches a unique value (annotation burden:
uniqueness must be threaded, and a function that's sometimes called on a
unique value and sometimes on a shared one either gets duplicated or forced
to the conservative case everywhere).

**Linear Haskell** (unverified level of detail — not independently checked
against the source paper here): a similar static-typing approach layered onto
an existing language, using linear arrow types to make a function's argument
usage exactly-once obligations part of the type. Same annotation-burden
tradeoff as Clean, mainstreamed into a widely-used language.

**Swift's copy-on-write (CoW)**: the mainstream cousin. Standard-library
collections carry a hidden uniqueness check (`isKnownUniquelyReferenced`)
before mutation; same dynamic-check shape as Lean, but hand-written into
specific stdlib types rather than derived by a general compiler pass, and
without Perceus's precise (cycle-free, immediate) refcounting discipline —
Swift's ARC has the usual reference-cycle caveats that don't apply to vix's
value model (vix values are trees/DAGs of handles with no user-visible
mutable cells to cycle through).

**What each gets wrong or right for us**: static uniqueness (Clean, Linear
Haskell) buys zero-runtime-cost dispatch at the price of surfacing linearity
in every signature — vix's whole design philosophy (per `checker-spec.md`
ruling 4, "no subtyping/coercions... contextual typing only") already leans
toward *not* growing the type system for compiler-internal bookkeeping the
user shouldn't have to think about. Dynamic (Lean/Koka/Swift) buys
zero-annotation-burden correctness at the price of a branch per candidate
update site — and vix's own standing position on this exact tradeoff is
already on record: `docs/design/rodin-in-vix.md:29-30` states "refcount==1 is
a RUNTIME fact our store already tracks — we need not prove uniqueness
statically." (That line asserts the store already tracks refcounts; Part 2
below shows it currently does not — see "Correcting a premise".) The design
adopted here (Part 3) is Perceus's *hybrid*: static last-use analysis to
decide *where* a runtime check is worth emitting, dynamic refcount check to
decide, at that site, whether to mutate or copy.

## Part 2 — Mapping onto vix's actual model

### Correcting a premise

The task brief for this doc assumed vix already has a "flesh vs spine"
distinction under those names, where flesh = frame-local uninterned values
and spine = store-interned values, and that this existing split is where
uniqueness tracking would naturally live. That is not what the codebase has
today. `grep -rn "flesh\|spine"` across the repo hits exactly
`docs/design/fleet-on-the-machine.md` and `vix-wire/src/lib.rs:1016`, and in
both places "spine" means the stable content-hash identity of a *shipped
closure invocation* (closure hash + canonical args) while "flesh" means the
lazily-fetched `CodeBundle` bytes for that closure
(`docs/design/fleet-on-the-machine.md:145-152`). That is a remote
code-shipping distinction, orthogonal to the value-representation question
this doc is about. There is no frame-local, pre-intern representation for
aggregate values anywhere in `vix/src/machine` today — this section spells
out why reuse analysis needs one, and proposes introducing it under the same
words by deliberate, flagged analogy (not because the machinery already
exists).

### The value model today

`ValueStore` (`vix/src/machine/driver.rs:687-690`) is pure content-addressed
dedupe with no ownership metadata:

```rust
pub struct ValueStore {
    entries: Vec<StoreEntry>,
    by_content: HashMap<(String, ContentHash), i64>,
}
```

`StoreEntry` (`driver.rs:665-670`): `{ schema, bytes: Vec<u8>, content_hash,
taint: Option<StructuralTaint> }`. A handle is a bare `i64` index into
`entries`. `ValueStore::alloc` (`driver.rs:866-890`) hashes the new value's
schema+bytes (folded with taint, see below), looks the hash up in
`by_content` (`driver.rs:877-879`), and returns the existing handle if
present — `alloc_raw`/`alloc_raw_tainted` (`driver.rs:892-...`) follow the
same shape for raw byte payloads. Every `StoreAlloc` event
(`driver.rs:233`) carries a `deduped: bool` specifically so tests can assert
dedup happened (`driver.rs:8258`, `:8298`).

**There is no refcount, uniqueness, or ownership field anywhere in this
path.** `grep -rn "refcount\|RefCount\|unique_owner" vix/src` returns nothing.
The store never frees an entry and has no notion of "how many live handles
point here" — it is structural dedup, not GC, and reuse analysis cannot be
bolted onto it as-is: **a deduped value is, by construction, shared.** If two
call sites separately construct a map with the same content, they get the
same handle — correctly, since content-addressing means "same content ⇒ same
identity" — but that handle now has (at least) two live references the
moment it's returned twice, even though neither call site did anything an
observer would call "sharing." Attempting to track a uniqueness bit *inside*
`StoreEntry` and mutate a deduped entry in place would corrupt every other
resolution of that same handle. **Reuse analysis must therefore never
operate on interned (store-resident) values at all.** It has to operate on
values *before* they are interned.

### Frame layout: scalars can be frame-only; aggregates cannot (yet)

Frames are untyped byte arenas, not tagged slot arrays. `weavy/src/task.rs`'s
module doc states this directly: a frame is a declared record addressed by
static byte offsets, and "the arena is raw bytes; ops imply types; nothing is
self-describing at runtime." `Fn { pub frame: Layout, pub code: Vec<Op> }`
(`task.rs:243`); `Layout` itself (`weavy/src/mem.rs:241`) is just `{ size,
align }`. `vix/src/machine/lower.rs:1996` and `:2308` build a frame `Layout`
per compiled function by walking a running byte-offset counter over its
slots. Reads/writes are raw words: `driver.rs:6286-6291`'s
`read_frame_word`/`write_frame_word` do `i64::from_le_bytes(...)` — a frame
slot holds either an immediate scalar or a store handle, and which one is
known only at compile time (no runtime tag), per vix's "typed instructions
over untagged operands" lowering discipline.

The consequence for reuse analysis: **scalars already live purely in frame
scratch** — a function computing only `i64`/`bool` results never touches the
store (`lower.rs:1985-1996`, a trivial body computing into a frame slot and
returning it via `Op::Ret`, no store interaction). But **every aggregate
(map/array/record) construction goes through `ValueStore::alloc*` the
instant it's built.** `map_insert` (`driver.rs:2236-2305`) clones the
*entire* existing pair list via `map_pairs`/`decode_map_pairs`
(`driver.rs:936-951`, a full decode+clone), appends the new pair
(`:2281-2288`), and calls `alloc_map` (`:2289-2294`, impl at `:922-934`) which
re-encodes and re-hashes the whole map and dedupes-or-inserts. `array_alloc`
et al. (`:2423-2452`, via `alloc_array_words` at `:1152-1167`) follow the same
shape: always build a fresh byte buffer, always hash, always dedupe-or-store.
**There is no in-place fast path for any `*_HOST` op today, and no
pre-intern aggregate representation to attach a fast path to.**

This is exactly the gap the "flesh" tier needs to fill, and exactly why it
doesn't exist yet: nobody needed it before, because every host op was already
paying a full copy on every call — introducing flesh *is* introducing the
optimization, not exposing a distinction that was implicit all along.
Proposed shape: a **flesh value** is a mutable, frame-local aggregate buffer
(same physical byte layout a store `StoreEntry` would use, so the eventual
`alloc_map`/`alloc_array_words` encode step is just "hash these bytes," not
"re-encode them") that is *not yet* in `ValueStore.entries` and carries no
content hash. A flesh value becomes a **spine value** (an interned,
content-addressed, immutable `StoreEntry`) the moment it crosses whichever
of these it's the first to hit:

- returned from the enclosing `demand()` invocation,
- passed as an argument to a nested `demand()` call,
- captured by a closure that escapes the current frame,
- explicitly traced (any `DriveEvent` that names it).

Before that point, it can be constructed once and mutated in place across an
arbitrary number of functional-update steps, as long as reuse analysis
(Part 3) can show it is never aliased in the interim.

### Why this boundary is exactly the demand boundary vix already has

vix's demand system already draws a granularity line that reuse analysis can
piggyback on, just not at the value level — at the *invocation* level.
`ValueStore::demand`... rather, the driver's `demand` entry point
(`driver.rs:1737-1739`) is `pub fn demand(&mut self, fn_ref: usize, args:
Vec<i64>) -> Result<i64, String>`: it computes a memo key from `(fn_ref,
args)`, emits `DriveEvent::Demanded { fn_hash }` (`driver.rs:215-216`,
described as "demand arrived for (fn hash, memo-key hash of args)"), and
looks up or spawns. Tests confirm the granularity is per-invocation, not
per-value: `warm_demand_spawns_nothing` and
`undemanded_functions_never_appear_in_the_trace` (`driver.rs:8032-8100`) show
an unreached function never spawns or hashes, and a warm re-demand of the
same `(fn, args)` is a zero-cost `MemoHit`. Everything between two `demand()`
crossings — ordinary host ops, frame arithmetic, `map_insert`, arbitrary
Rodin-kernel unit-propagation steps — runs synchronously inside one
invocation and is not itself a memo node. This *interior* region is precisely
where flesh values are allowed to live: they are never individually traced,
so mutating one in place is invisible to the demand graph as long as it
never escapes the interior region without being interned first. This matches
`rodin-in-vix.md:39-42`'s own framing: "solve interior is FLESH, not spine —
weavy-JIT'd function calls, not demand nodes... Demand granularity sits at
the fact level." Reuse analysis is the mechanism that makes that framing
literally true at the value level, not just at the invocation level.

### StructuralTaint as an implementation precedent — and where it diverges

The recent secrets work (`vix: secrets rung 1`) added exactly the kind of
side-channel metadata field reuse analysis needs a sibling of.
`StructuralTaint` (`driver.rs:672-677`) is a field on `StoreEntry` alongside
(not folded into) `content_hash`, computed at each `alloc*` call from the
taints of the new value's constituent fields — `taint_for_value_bytes`
(`driver.rs:6421-6459`) walks `Access::Record`/`Access::Enum`/`Access::Handle`
fields, recursing into each referenced entry's stored taint via
`taint_for_word`, and `combine_taints` (`:6379-6404`) merges multiple input
taints into one. This is precisely the "propagate a metadata bit through
structural composition, computed at each alloc site from the operands' bits"
pattern a uniqueness/refcount scheme needs.

Where it diverges, and why the divergence matters: taint is a **pure,
monotonic function of content** — the same bytes always produce the same
taint, so it's legitimate (and done: `hash_with_taint`, `driver.rs:6359-6367`)
to fold it into the *dedup identity* itself, so a tainted value dedupes
separately from an untainted one with identical raw bytes. Uniqueness is the
opposite: it is a **mutable fact about the current reference structure at
this moment in this invocation**, not a property of content. Two
structurally identical flesh buffers must remain eligible to dedupe into the
same spine entry once interned, regardless of how many live aliases either
one had while still flesh — so a uniqueness/refcount field must **never**
participate in content hashing or dedup identity, unlike taint. Concretely:
taint lives on `StoreEntry` (interned, hashed); a uniqueness refcount must
live on the flesh buffer's own (uninterned, unhashed) header, and must be
discarded, not carried forward, at the moment the buffer is interned into a
`StoreEntry`.

### The weavy JIT surface: precedent for a runtime-branching stencil

Copy-and-patch stencils already support exactly the two-continuation shape a
guarded in-place-update needs. `weavy/src/jit/mod.rs:15` documents
`extract_stencil_n` "for multi-successor stencils like conditional guards."
The concrete precedent is `weavy_task_jump_if_zero`
(`weavy/stencils/task_ops.rs:188-204`): it reads one frame word, branches on
a runtime condition, and jumps to one of two independently-patched
continuation symbols (`weavy_zero`, `weavy_nonzero`, declared
`task_ops.rs:54-56`), wired at build time via `extract_stencil_n(&bytes,
TASK_SYMBOLS, sym, &["weavy_zero", "weavy_nonzero"])`
(`weavy/build.rs:262`, symbol registration `build.rs:184`, op mapping
`build.rs:211`). A reuse-check stencil is the same shape with a different
predicate: read the flesh buffer's refcount word instead of a frame value,
branch to a `mutate_in_place` continuation vs a `copy_then_mutate`
continuation. No new JIT mechanism is needed — only a new stencil and two new
continuation bodies, using the multi-successor extraction path that's
already load-bearing for `JUMP_IF_ZERO`.

### Host ops that need a functional-update + in-place form

Current host ops (`driver.rs:112-152`) have no mutating forms at all —
`MAP_INSERT_HOST` is the closest existing op and, as shown above, is an
unconditional copy-append-rehash, not an in-place-if-unique op. The ops that
need a reuse-eligible form: `MAP_INSERT_HOST` (insert/overwrite a
key), plus new `ARRAY_SET_HOST`, `ARRAY_PUSH_HOST`, `ARRAY_POP_HOST` (none of
`ARRAY_SET`/`ARRAY_PUSH`/`ARRAY_POP` exist as named ops today — only
`ARRAY_ALLOC_HOST`, `ARRAY_MAP_PENDING_HOST`, `ARRAY_COLLECT_HOST`,
`ARRAY_FILTER_EXCLUDE_HOST`, `ARRAY_LEN_HOST`, `ARRAY_JOIN_HOST` exist,
`driver.rs:112-152`, dispatch table `:3341-3386`). Each reuse-eligible op
gets two bodies sharing one stencil: the existing copy body (verbatim
today's `alloc_map`/`alloc_array_words` path — the permanent fallback) and a
new in-place body that writes directly into the flesh buffer's existing
bytes and returns the same (still-flesh) handle, skipping re-encode/hash
entirely until the value is eventually interned.

## Part 3 — The spec

### Chosen design: hybrid (static shape-match, dynamic uniqueness check)

Static-only (Clean/Linear-Haskell-style uniqueness types) is rejected: it
would require vix to grow a linear/uniqueness type system that leaks into
every function signature touching an array/map, contradicting the language's
existing anti-annotation-burden stance (`checker-spec.md` rulings 1 and 4 —
no inferred-and-hidden return types, no coercions/subtyping machinery grown
for compiler convenience). Dynamic-only (Lean/Koka/Swift-style: check
refcount at every mutation call, no static analysis at all) is rejected as
the default because it would insert a runtime branch at *every* map/array
op, including ones with no realistic reuse opportunity (e.g., an op whose
result obviously escapes into three places) — pure overhead with no payoff
at those sites.

The adopted design is Perceus's hybrid, mapped onto vix's actual pipeline:

- **Static** (compile time, in `lower.rs`, over the typed AST before weavy
  `Op` emission): a last-use analysis per function body. For a functional
  update expression (`b = a.set(i, v)`, `b = a.push(v)`, `b = a.insert(k,
  v)`), check whether `a`'s binding has no other use reachable after this
  point in the same scope (standard backward liveness over the typed AST,
  no cross-function/global analysis, no user annotation). If so, this is a
  **drop-reuse candidate site**: emit the `Dup`/`Drop` bookkeeping ops
  Perceus requires around it (increment on share, decrement on last-use) and
  mark the update call for the reuse-checked lowering below. If `a` has a
  later use, or escapes the frame (returned, captured, passed to `demand()`),
  lower straight to the unconditional-copy host op — no runtime check
  emitted, zero overhead, identical to today's behavior.
- **Dynamic** (runtime, in the compiled stencil, only at candidate sites):
  the flesh buffer's header carries a small refcount word, incremented at
  each `Dup` and decremented at each `Drop` the static pass inserted. At a
  candidate site's mutation call, the reuse-check stencil (the
  `weavy_task_jump_if_zero`-shaped two-continuation branch described above)
  reads that word: `== 1` branches to the in-place-mutate body; `> 1`
  branches to the existing copy body. The copy path is exactly today's
  `alloc_map`/`alloc_array_words`, so the dynamic check can never produce a
  wrong answer — worst case (refcount check always sees sharing) is
  "unchanged today's behavior," never corruption.

### Where drop-reuse pairing happens

At `lower.rs`, specifically the pass that currently lowers a functional
update AST node straight to a `MAP_INSERT_HOST`/`ARRAY_*_HOST` call
(the `map_insert`/`array_alloc` lowering sites feeding `driver.rs:2236-2452`'s
host ops). This is a typed-AST-level pass, run once per function body, before
`Op` emission — the same place `lower.rs:1996`/`:2308` already computes frame
`Layout`s, since the pass needs the same scope/binding information. It is
*not* a weavy-IR-level pass: weavy `Op`s are already untyped, tagless bytes
by the time they exist (`task.rs` module doc), so last-use liveness has to be
computed while variable identity and scope are still typed-AST concepts.

### Runtime protocol

1. A flesh buffer header carries: a refcount word (native, not part of any
   content hash) and the raw payload bytes (identical layout to what
   `alloc_map`/`alloc_array_words` would eventually encode, so interning is a
   pure hash-and-register with no re-encode).
2. `Dup`/`Drop` ops (new weavy `Op` variants, lowered from the static pass)
   increment/decrement that word. These only ever apply to flesh buffers —
   store handles are never refcounted; the store's existing content-dedup is
   the only sharing mechanism for interned values, and interned values are
   immutable so there is nothing to protect them from.
3. At a reuse-checked update, the guard stencil reads the header word: `== 1`
   ⇒ in-place body (write new bytes over the existing buffer, keep the same
   flesh handle); `> 1` ⇒ copy body (allocate a fresh flesh buffer, copy,
   mutate the copy, return the new handle) — the copy body's output is
   itself a fresh flesh value with refcount 1, so a chain of updates after a
   forced copy reuse-mutates freely again.
4. Interning (flesh → spine, at any of the four boundary events in Part 2):
   hash the flesh buffer's current bytes, run the existing
   `ValueStore::alloc`/`alloc_map` dedupe lookup, discard the flesh header
   (refcount is not carried into `StoreEntry` — `StoreEntry` has no
   uniqueness field, by the divergence from `StructuralTaint` noted above).
5. Fallback ownership: the copy path is not new code — it is today's
   `alloc_map`/`alloc_array_words`/`map_insert` body, verbatim. Reuse
   analysis is purely additive: it introduces a faster path alongside the
   existing one and a static analysis that decides, per call site, whether
   the faster path is ever attempted. Nothing about today's semantics or
   fallback code needs to change for this feature to be correct on day one.

### Observable-semantics guarantee

Reuse must be **unobservable**: value semantics preserved exactly, byte for
byte, in both the returned value and the demand trace. This holds by
construction if two invariants are kept:

- flesh values are never referenced by more than one *observable* binding
  at the moment of an in-place write (exactly what the refcount check
  verifies before allowing the fast path — this is the whole mechanism, not
  an extra check on top of it),
- flesh buffer identity never appears in a `DriveEvent`. Only interning
  emits a trace-visible event (`StoreAlloc`, `Demanded`, `MemoHit`, etc.);
  whether a given interned value's bytes were produced by a fresh
  allocation or by mutating a flesh buffer in place is not something the
  trace format is capable of recording, so the two strategies are
  trace-indistinguishable by construction, not by discipline.

**Differential test harness**: a build/runtime toggle
(e.g. a JIT codegen flag analogous to `WEAVY_FORCE_COPY_PATH`) forces every
reuse-check stencil to always take the copy continuation, regardless of the
refcount word — i.e., mechanically reproduces pre-reuse-analysis behavior
without touching the static pass or the `Dup`/`Drop` instrumentation. Take
any vix program, run it once with the toggle off (reuse enabled) and once
with it on (reuse forced off), and assert:

- identical final result handles and identical `ValueStore` content (same
  set of interned entries, same content hashes),
- byte-identical `DriveEvent` trace sequences (same `Demanded`/`MemoHit`/
  `StoreAlloc`/`deduped` events, same order),

for the same input. Any divergence is a reuse-analysis bug (an aliasing case
the static/dynamic check missed), never a "different but valid" outcome —
this is a strict oracle, not a fuzz-and-eyeball check, precisely because
content-addressing makes "same trace" a decidable, exact comparison rather
than an approximate one.

### Acceptance benchmark: CDCL-shaped trail push/pop

A microbenchmark vix program implementing a persistent trail (append-only
stack with occasional pop-back-to-checkpoint, the actual CDCL trail access
pattern) in pure functional style:

```
trail' = trail.push(lit)
(lit, trail') = trail.pop()
```

run in a tight loop shaped like real CDCL behavior: monotonically growing
push sequences interrupted by pop-back-to-decision-level bursts (mirroring
`rodin-in-vix.md`'s own target — the "457-decision solve" slice named for
Spike B), never letting the trail variable's binding survive past the
following push/pop step so the static last-use pass classifies every step as
a reuse candidate.

Compare:

1. **Naive copy** (reuse analysis compiled out): every push/pop takes today's
   `alloc_array_words` path — full buffer copy every call. Cost per call
   scales with current trail length, so a loop of N operations against a
   trail that grows to size M is O(N·M) — this variant exists to quantify
   the problem, not as a viable target.
2. **Reuse-analysis enabled**: last-use analysis confirms the trail binding
   is never aliased across the loop body, so refcount stays 1 for the
   entire run and every push/pop takes the in-place stencil path. Cost per
   call is O(1) amortized, same as a native growable array.
3. **Reference**: a hand-written Rust `Vec<i64>` performing the equivalent
   push/pop loop, compiled natively, as the ceiling.

**Target**: (2) within a small constant factor (single-digit multiplier) of
(3)'s throughput; (1) should show the expected superlinear blowup as trail
size grows, making the gap between (1) and (2) the demonstration that reuse
analysis is doing real work, and the gap between (2) and (3) the number that
determines whether Spike B's pure kernel is viable per `rodin-in-vix.md`'s
own bar ("gap within the warm-solve envelope → kernel migrates too").

## Summary of open questions this doc does not resolve

- The exact set of weavy `Op` variants for `Dup`/`Drop` and the reuse-check
  stencil's frame-header layout are not designed here — this is a spec of
  the analysis and protocol, not the `Op` encoding.
- Whether `Dup`/`Drop` insertion should reuse any existing lowering pass
  infrastructure in `lower.rs` beyond the `Layout` computation cited above,
  or needs its own pass ordering, is an implementation question for whoever
  picks this up.
- Interaction with `StructuralTaint`-carrying values (does a tainted flesh
  buffer refuse in-place mutation, or is taint orthogonal to reuse
  eligibility?) is not addressed — taint propagation happens at intern time
  in the current code (`taint_for_value_bytes`, computed inside `alloc*`),
  which is also when flesh gets discarded per this design, so the two
  mechanisms don't obviously conflict, but this was not verified against
  the secrets-rung work in `driver.rs`'s taint code in depth.
