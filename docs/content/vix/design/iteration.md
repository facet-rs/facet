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
2. **Semantically element-wise.** `ys = xs.map(f)` denotes an array whose
   element `ys[i]` depends on `f(xs[i])` — and on nothing else. Projections
   preserve partial dependency: a consumer of `ys[3]` depends on `f(xs[3])`
   only. This is the semantic plane and it never changes.
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

### 4. Ordering & determinism (Amos, 2026-07-08 — doctrine, not optional)

The output of every combinator is deterministic regardless of execution
shape. Fan-out may *evaluate* elements in any order, on any number of
threads — that's the implementation plane. What is *observable* is fixed:

- **Ordered sources stay positional.** `ys = xs.map(f)` has `ys[i] =
  f(xs[i])` — output order is input order, structurally, no matter what
  order the fan-out completed in. Same for `filter`/`filter_map`/`flat_map`
  (input order, gaps closed).
- **Unordered aggregations observe in canonical value order.** Anything
  that collects results without an inherited positional order — collecting
  a set, map keys, glob results, tree merges — is observable in *increasing
  (lexicographical) order of the values themselves*. This works because
  every vix value is comparable, hashable, and serializable: a total order
  always exists, so "sorted ascending" is always well-defined and
  implementation-independent. The existing `array_collect` already does
  sort-then-alloc — this doctrine is why, now stated as surface semantics
  rather than an implementation habit.
- Consequence: the fused/fanned differential oracle (§3) is only possible
  *because* of this — bit-identical results require deterministic order.
- Known violation to fix on contact: the rodin mission logged "`[String]`
  returns unstable" — that instability is a bug under this doctrine, not
  an accepted quirk.

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
