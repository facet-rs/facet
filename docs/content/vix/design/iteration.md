+++
title = "proposal: iteration & the combinator surface"
+++

# Iteration & the combinator surface

Status: PROPOSAL (Fable, 2026-07-08). For Amos to tear. Grounded in the
foundation chapter (`/vix/`), the language-gap censuses
(`~/vixenware/notes/machine-spec/lang-gaps-*.md`), and the lowering surface as
it exists at rodin `ae24fa103`.

## The wound, quantified

The dominant pattern in every non-trivial vix program is the manual recursive
list walker:

```vix
fn walk(xs: [Row], acc: Int) -> Int {
    match xs.len() == 0 {
        true => acc,
        false => walk_tuple(xs.pop(), acc),
    }
}
fn walk_tuple(t: (Row, [Row]), acc: Int) -> Int { ... }
```

Two lines of intent, eight lines of scaffolding, and a freestanding `_tuple`
helper because `pop()`'s tuple can't be destructured in a match arm. Census
counts: 45 walkers + 26 tuple helpers in `rodin/rodin.vix`, 52 walker+helper
pairs in `cargo_manifest.vix`, 102 markers across `crate.vix`/`index.vix`.
Estimated 800+ lines dissolved corpus-wide by this proposal.

## What exists today (read before proposing, per the method)

- `.map(f)` exists — but it is **always demand fan-out**: `array_map_pending`
  allocates an `Array` of `Pending` invocations, one demand per element
  (driver hostcall 11, lower.rs:4721). Correct, memoizable, parallel — and
  catastrophically wrong-shaped for `xs.map(|x| x + 1)`, where the memo entry
  costs more than the add. This is the drowning the chapter measured at four
  orders of magnitude.
- `.filter(...)` exists — as `array_filter_exclude`, a special-cased
  name-list exclusion hostcall (lower.rs:4720). Not a general predicate.
  `lua.vix`'s `.filter(|u| u != p"lua.c")` is aspirational surface riding a
  special case.
- `.collect()` exists (finalize pending/words array, tree-merge for Trees).
- `fold`, `find`, `find_map`, `any`, `all`, `flat_map` **do not exist**.
- **Tail loops landed**: self-tail-call lowers to `Op::Jump` with molten
  accumulators surviving across iterations (the 100k-iteration tripwires).
  The machine can already run iteration fast *inside* a demand region;
  interior iteration is not demand (banked doctrine).
- Closure parameter and match-arm **tuple destructuring do not exist** —
  the sole reason the `_tuple` helpers exist.

So the machinery below the surface is mostly built. What's missing is the
surface, the *fused* execution shape, and the decision rule between shapes.

## Design principles (from the chapter, not negotiable here)

1. **Description, not action.** A combinator application denotes a value.
   `xs.fold(0, |acc, x| acc + x)` is not a loop that runs; it is a value that
   depends on `xs`. There is no iteration "statement" anywhere in this
   proposal and there never will be.
2. **Semantically element-wise.** `ys = xs.map(f)` denotes a collection
   whose elements are exactly the `f`-images of `xs`'s elements — each
   output element depends on one input element and nothing else. (What
   *order* those elements are observable in is a separate question — §4,
   and the answer is not "input order.")
3. **Execution shape belongs to the partition.** Whether `map` runs as a
   fused island-interior loop or fans out as N demands is vixc's call under
   the as-if law — *chosen, not commanded*, invisible at the semantic plane,
   revisable as the cost model learns. The surface does not encode the
   choice. No `par_map`, no `map_eager`, no annotation. That door is the same
   door the chapter shut.

## The proposal

### 1. Combinators (the v1 set)

Methods on `[T]`, each denoting a value:

| Signature (sketch) | Dissolves |
|---|---|
| `fold<R>(init: R, f: fn(R, T) -> R) -> R` | the accumulator walkers |
| `map<U>(f: fn(T) -> U) -> [U]` | (exists; gains fused shape) |
| `filter(p: fn(T) -> Bool) -> [T]` | generalizes `filter_exclude` |
| `filter_map<U>(f: fn(T) -> Option<U>) -> [U]` | filter+transform walkers |
| `find_map<U>(f: fn(T) -> Option<U>) -> Option<U>` | early-exit search walkers |
| `any(p: fn(T) -> Bool) -> Bool`, `all(p) -> Bool` | boolean scan walkers |
| `flat_map<U>(f: fn(T) -> [U]) -> [U]` | nested append recursion |
| `last() -> Option<T>` | tail-chasing recursion |
| `sorted_by(cmp: fn(T, T) -> Ordering) -> [T]`, `sorted() -> [T]` where `T: <=>` | the missing sort primitive (rodin 40-search gap) |

Deliberately absent from v1: `zip`, `enumerate`, `take`/`skip`, `Map`/`Set`
iteration (they follow once `Set<T>` lands — see companion decisions), lazy
iterator chains as a user-visible type (fusion of `xs.filter(p).map(f)` is a
lowering concern, not a surface concern; the surface composes arrays).

### 2. Destructuring binders

Closure parameters and match arms accept tuple (and record) patterns:

```vix
let (head, tail) = xs.pop();
match xs.pop() { (head, tail) => ... }
rows.fold(state, |state, (pkg, version)| ...)
```

This alone kills all 78+ `_tuple` trampoline helpers even where explicit
recursion legitimately remains.

### 3. Two execution shapes, one surface

Every combinator lowers to one of two shapes; **the partition picks**:

- **Fused** (the new default for cheap pure closures): the whole combinator
  is one grain inside an island — a tail loop (`Op::Jump`, landed) with a
  molten accumulator (`fold`) or a molten array push loop (`map`/`filter`),
  publishing once at the end (`store.publish-once`, now an islands instance).
  Zero per-iteration identity work, zero memo entries, zero hostcalls in the
  loop body (per `execution.no-pure-hostcalls` this rides native weavy ops).
  Early-exit combinators (`find_map`/`any`/`all`) lower to a conditional
  jump out of the loop — same machinery, one extra edge target.
- **Fan-out** (today's `array_map_pending`, kept): N pending invocations,
  one demand per element, each independently memoized and parallelizable.

**v1 decision rule** (a heuristic, honestly labeled; the cost model replaces
it later): fuse when the closure body is pure and lowers inline — no effect
primitives, no invocation of a memo-boundary function. Fan out when the body
contains an effect (a **mandatory** semantic cut per the chapter — receipts
and memo policy live there, no choice exists) or calls into a function the
partition keeps as a demand boundary. Per `execution.lowering-diagnostics`,
choosing fan-out where fusion looks plausible emits a reasoned diagnostic —
the "reuse declined at site S" pattern, applied to iteration.

The as-if consequences fall out for free and should become tripwires:
- Fused vs fanned results are bit-identical (differential oracle: run the
  corpus both ways under a force-flag, `VIX_FORCE_MAP_FANOUT` — the same
  standing-guard pattern as `VIX_FORCE_MOLTEN_COPY`).
- Deopt may split a fused combinator into fan-out at runtime (island split);
  nothing invalidates (partition-as-filter).
- A consumer projecting `ys[3]` from a *fanned* map demands one element; from
  a *fused* map it may compute all of them — allowed, because laziness there
  is unobservable exactly when the closure is pure and cheap, which is the
  fusion precondition. Where per-element demand is observable (effectful f),
  fusion was already forbidden. The theorem does the work.

### 4. Ordering & determinism (Amos, 2026-07-08 — doctrine; supersedes the first draft's two-rule split)

**The ruling: positional order dies; automatic concurrency wins.** Amos,
from the original vix design rounds: in a build-graph language you must
give away either positional order or automatic concurrency — you cannot
have both. Vix cares more about automatic concurrency, and about removing
the footgun where everything gets serialized by accident, than about
positional order.

- **All aggregate observation is in canonical value order**: increasing
  (lexicographic) order *of the values themselves*. This is always
  well-defined because every vix value is comparable, hashable, and
  serializable — a total order exists for anything. Fan-out may evaluate
  in any order on any number of threads; observation order is canonical
  regardless. Deterministic output with zero order bookkeeping.
- **Positional order is opt-in, by carrying the position in the value.**
  Wrap elements as `(index, value)` tuples: canonical order over those
  tuples *is* index order (the index sorts first), so order-sensitive code
  gets exact positional semantics back — paid for explicitly, visible in
  the types. Candidate type alias: `Indexed<T> = (Int, T)`; `enumerate()`
  produces it and therefore moves INTO the v1 combinator set.
- The existing `array_collect` already does sort-then-alloc — this
  doctrine is why; it now graduates from implementation habit to surface
  semantics. The rodin-logged "`[String]` returns unstable" is a violation
  of it — a bug, not a quirk.
- The fused/fanned differential oracle (§3) is only possible because of
  this: bit-identical results require deterministic order.

**Consequences stated honestly, not hidden:**

- `ys[i] = f(xs[i])` is dead. `ys = xs.map(f)` observes the f-images in
  *their* canonical order; index `i` means "i-th smallest," which depends
  on the ranking of all elements — so positional projection of a bare map
  result no longer gives per-element partial dependency. Per-element
  dependency survives through `enumerate` (project the tuple whose index
  is `i`) or keyed access. Order-sensitive pipelines write
  `xs.enumerate().map(|(i, x)| (i, f(x)))` — and the cost they're paying
  (a serialization constraint) is now visible in the code instead of
  ambient.
- `fold` runs in canonical element order — deterministic always, but
  source-order only if you folded over `Indexed<T>`. A non-commutative
  fold over a bare collection is well-defined (canonical order), just not
  "insertion order," which no longer exists as a concept.
- `find_map` returns the first hit *in canonical order* — deterministic,
  parallelizable (compute all candidates, take the least), and free of
  the leftmost-bias that would have forced serial evaluation.
- `any`/`all` are order-free and unaffected.

**Construction is NOT affected (Amos, 2026-07-08 follow-up):** a literal
`[a, b, c]` is positional and stays positional — *an array is a struct
whose fields are named 0, 1, 2*. The literal denotes a value depending on
three values (each a field, each independently projectable, available at
different times — fine). It is not a stream and not a bag. So positional
projection of a *constructed* array is ordinary field access and keeps
per-field partial dependency; what dies is positional order of *derived*
aggregates (map/filter/collect output), per the ruling above.

**Open questions this raises** (for Amos, follow-up round):

1. **What, typed, does `xs.map(f)` return?** If `[T]` is a
   positional struct-with-numeric-fields and map output is canonically
   ordered, either (i) map returns a distinct collection type (a bag/
   multiset whose observation order is canonical — Amos: "kind of
   interesting… an even weirder thing" — explicitly undecided), or
   (ii) map returns `[U]` where field `i` holds the i-th value in
   canonical order (an array *re-indexed by rank*). Same observable
   behavior; different type-system story and different `pop`/field-access
   semantics downstream.
2. **What is `pop()` now?** On a positional array, "last field" is
   coherent but creates a dependency on field N-1 specifically (which is
   real and fine under array-as-struct — but serializes head/tail
   recursion, which the combinators exist to replace anyway). On a
   canonical aggregate it would mean "remove the greatest." Amos:
   "pop is complicated, I don't know what pop is supposed to do." With
   fold/find_map landed, pop's remaining legitimate uses may not exist —
   candidate for deprecation rather than redefinition.
3. Is `Indexed<T>` a blessed alias with constructor support, or just a
   convention?

### 5. `for` sugar — deferred, with a lean

A `for x in xs { ... }` expression form is *pure sugar* over `fold`/`map` and
adds nothing semantically. Lean: don't add it in v1. The combinators must be
ergonomic enough on their own; if after the corpus rewrite there's still a
class of site that reads worse as a fold than it would as a comprehension,
that's the evidence a `for` proposal needs. (Also: a statement-shaped `for`
invites action-thinking, which Vix 101 spends its whole first section
killing. If sugar comes, it should be expression/comprehension-shaped.)

### 6. What this does NOT touch

- `?`-shaped propagation for user enums (`Step::Conflict` threading) —
  parked by Amos, 2026-07-08. `fold` reduces the pain (the propagation
  pyramids often *are* folds) but the feature is separate.
- Non-tail recursion stays INVOKE/memoized (ratified). Combinators don't
  change what a function call is; they change how much code needs to be
  function calls.

### 7. Acceptance (oracle-first)

1. Corpus rewrite of the worst census sites: rodin walker count 45 → 0,
   `_tuple` helpers 26 → 0 (grep-checkable), differential fixtures vs real
   `cargo tree` stay green, sparse/oracle suites stay green.
2. The fused/fanned differential force-flag runs corpus-wide, green.
3. Perf tripwire: `xs.map(|x| x + 1)` over 100k elements does zero memo
   inserts and zero SHA/blake3 samples in the loop (flame-graph assertion,
   same discipline as the molten thesis check); the fan-out shape on the same
   input still produces N memo entries (proving the partition actually
   chooses).
4. Lowering diagnostics fire on a deliberately effectful closure (fan-out
   reason named).

## Open questions for Amos

1. The v1 combinator list above — anything missing that the corpus rewrite
   would immediately want? (`sorted`/`sorted_by` included on the strength of
   the rodin "no sort primitive" log.)
2. `for` sugar: agree to defer?
3. Method-style on `[T]` implies these are language-blessed (lowered
   specially), not std library functions — std can't express the fused shape
   today. Comfortable with combinators-as-language-surface for v1, migrating
   to "std functions the partition knows how to fuse" later?

---

## Appendix: companion decisions banked 2026-07-08 (Amos, verbatim intent)

Recorded here so follow-up proposals have an anchor; each gets its own design
pass:

- **Boolean surface** (`||`, `!`, `if`/`else`, match guards): obvious, add.
  No design needed beyond precedence; spec + implement.
- **Typed decode**: facet-style derive annotations on vix structs;
  facet-json/facet-toml deserialize directly onto them; a `facet_value`
  intermediate representation is an acceptable shortcut in the meantime.
  Kills the `doc_string` walking (52 uses in crate.vix alone).
- **In-language unit tests**: a must. (42 of cargo_manifest's 56 pub fns
  exist only as Rust-test probes — the harness gap is measured.)
- **Newtypes**: return "bigly," and must be markedly more ergonomic than
  Rust's. Design pass owed.
- **`Set<T>`**: first-class structure, obviously.
- **Const maps / string-match tables**: agreed.
- **Record defaults**: go further than Rust — per-field defaults.
- **Spaceship**: `<=>` should just work as you'd expect; `Version` in std
  must implement it (std currently advertises the contract and doesn't eat
  it — fix on contact).
- **`?`-shaped propagation for user enums**: PARKED — needs real thought,
  later.
- **Store-laundering mystery** (map insert-and-read-back to force realized
  representation): known, cause unknown — investigation fired (agent
  0d7e3923, report to
  `~/vixenware/notes/machine-spec/laundering-root-cause.md`).
