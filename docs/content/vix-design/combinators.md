+++
title = "draft: the combinator surface"
+++


*For Amos to tear, on the website. Supersedes the flat v1 list in
`iteration.md` §1. Every operation shown with code; open questions marked
inline. Ruling inputs: by-value everywhere (nothing mutates, ever); names
carry semantics (no bare method whose ordering/selection meaning is
implicit); execution shape (fused vs fan-out) NEVER appears in a name —
that's the partition's business and stays invisible.*

## The two collection kinds

**Arrays are structs.** `[a, b, c]` is a struct with fields named 0, 1, 2 —
three independent values that happen to share a bracket. Positions are real,
projectable, and cost nothing: each field is separately demandable.

**Operations that destroy positions return a different kind of value.**
When survivors of a filter arrive, there is no honest answer to "which
field is survivor #2" — so the result is an **unordered collection**: an
immutable multiset *value*, born complete like every other value. It is not
a container that fills over time — nothing in vix is; a program only
*describes* it, and whenever it is observed, it is whole, in canonical
(increasing) value order. *Name TBD: `Multiset<T>` / `Bag<T>` / `{T}` —
"bag" may carry a mutable-container connotation worth avoiding.*

Arrays convert down for free (`.values()` — forgetting positions is sound);
the bridges back up are explicit and priced:

```vix
let ranked: [T]   = ms.sorted();          // rank materialized, global dependency, visible
let tagged        = xs.enumerate();       // [(Int, T)] — positions carried as data
```

## Where the earlier fear dissolves

"Positional order vs automatic concurrency" turns out to be about
*aggregation*, not mapping. Field-wise mapping of a struct has no ordering
problem at all:

```vix
let manifests = members.map(parse);   // [Path] -> [Manifest]
// manifests.1 = parse(members.1). Three independent output fields,
// each depending on exactly one input field. Nothing serializes;
// nothing needs canonical order; partial dependency survives.
```

Canonical order enters exactly where positions genuinely die:

```vix
let heavy = rows.values().filter(|r| r.weight > 100);
// Multiset<Row> — "which position" has no honest answer, so none is given
```

## The surface (v1 draft)

### On arrays `[T]` — positional, field-wise

| Operation | Type | Notes |
|---|---|---|
| `map(f)` | `[T] -> [U]` | field-wise: output field i = `f(input.i)` |
| `enumerate()` | `[T] -> [Indexed<T>]` | `Indexed<T> = (Int, T)`, plain std alias (ratified) |
| `fold(init, f)` | `[T] -> R` | field order (deterministic; it's a struct walk) |
| `len()`, `any(p)`, `all(p)`, `contains(x)` | | order-free |
| `split_last()` | `[T] -> (T, [T])` | by-value pop, renamed so the name says which element; fresh values both |
| `values()` | `[T] -> Multiset<T>` | forget positions |

### On the unordered collection `Multiset<T>` — canonical order

| Operation | Type | Notes |
|---|---|---|
| `map(f)` | `-> Multiset<U>` | element-wise |
| `filter(p)` | `-> Multiset<T>` | |
| `filter_map(f)`, `flat_map(f)` | `-> Multiset<U>` | |
| `fold(init, f)` | `-> R` | canonical element order — deterministic always |
| `find_min(p)` / `find_max(p)` | `-> Option<T>` | the deterministic "find": least/greatest satisfying `p` |
| `take_min()` / `take_max()` | `-> (T, Multiset<T>)` | by-value removal, name says which |
| `sorted()` / `sorted_by(cmp)` | `-> [T]` | the bridge to rank; needs `<=>` (which must just work) |
| `len()`, `any(p)`, `all(p)`, `contains(x)` | | order-free |

`filter` on an array is spelled through the honest conversion —
`xs.values().filter(p)` — and if provenance matters, carry it:

```vix
let kept = xs.enumerate().values().filter(|(i, x)| wanted(x));
let in_original_order: [Indexed<T>] = kept.sorted();
// canonical order of (Int, _) pairs IS index order — original positions restored
```

## Argued and excluded

- **`pop_first_ready` / any first-ready selection.** Program-visible
  arrival order is nondeterminism, full stop — it breaks the determinism
  doctrine and with it the replay/chaos-kill oracle. What the *idea* wants
  is already legal invisibly: for a provably commutative-associative fold,
  the lowering may process elements in arrival order under the as-if law,
  because no observer can tell. Arrival order as an *optimization*: yes,
  silently. As a *semantic*: never.
- **Bare `pop()`.** Dies twice: mutation-shaped, and silent about which
  element. `split_last` / `take_min` / `take_max` say what they do.
- **Fused-vs-fan-out in names** (`map_fanout`, `par_map`): execution shape
  is the partition's choice, invisible by law. A name that promises an
  execution shape is a knob, and the door stays shut.
- **`for` loops**: circle-back scheduled, deliberately not in this draft.

## Builtin vs std (line TBD — provisional column)

Combinators expressible in vix should be vix. Provisional split: `fold`
(needs the tail-loop/molten machinery) — builtin; `map` on arrays
(field-wise struct construction) — builtin; `sorted` (needs the sort
primitive) — builtin; `enumerate`, `any`, `all`, `contains`, `find_min`,
`filter_map` as std vix over `fold`/`filter` — std, the day std can express
them; until then they ride the same lowering as builtins with their std
future noted. The dividing line is explicitly undecided.

## Open questions in this draft

1. The unordered type's **name** (`Multiset` / `Bag` / bracket syntax).
2. **`flat_map` on arrays**: concatenation in field order is well-defined
   positionally — should an array flavor exist (`[T] -> [U]`), or is
   flattening inherently a multiset move?
3. **Same name, two receivers**: `map`/`fold` mean field-order on arrays,
   canonical-order on multisets — is type-directed meaning enough, or does
   names-carry-semantics demand distinct names (`fold_fields` vs
   `fold_canonical`)?
4. Which removal variants actually earn v1: `split_last`? `take_min`/`max`?
   (The corpus rewrite will show demand; the lean is: start with none and
   let a real site ask.)
