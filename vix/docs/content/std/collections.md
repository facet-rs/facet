+++
title = "Collections"
weight = 1
+++


*Status: provisional — this page documents the language as designed; parts
are not implemented yet.*

Vix has two collection kinds. An **array** `[T]` is ordered: it remembers
where each element sits. A **multiset** `Multiset<T>` is unordered: it
remembers only what's in it. Every operation on either returns a new value
— nothing in vix mutates, ever. There is no `push` that changes an array;
there is only an array and, elsewhere, a bigger one.

## Arrays are structs

```vix
let members = [p"crates/taxon/Cargo.toml",
               p"crates/weavy/Cargo.toml",
               p"crates/vix/Cargo.toml"];

let second = members[1];
```

An array literal is a struct whose fields happen to be named `0`, `1`, `2`.
This is not a metaphor — it's the semantics. Each element is an independent
value: demanding `members[1]` depends on that field and nothing else, and
two fields of the same array can be computed in parallel, or one computed
and the other never touched. There is no iteration order because there is
no iteration: an array *is* its fields.

**Coming from Rust/JS**: there is no growth, no capacity, no `push`/`pop`
mutation family. An array is closer to a tuple whose elements share a type.

## Multisets are unordered values

```vix
let heavy: Multiset<Row> = rows.values().filter(|r| r.weight > 100);
```

A multiset is a complete, immutable value — it is *born whole*, like every
vix value; it is never a container that fills up. It has no positions:
there is no `heavy[0]`, because "which one is first" has no honest answer
for a collection whose elements were produced in no particular order.

When a multiset is *observed* in its entirety — serialized, rendered to a
string, converted to an array — its elements appear in **increasing value
order**. This is always well-defined because every vix value is ordered
(see `<=>` below); it makes every multiset operation deterministic without
constraining how anything is computed.

**Coming from anywhere**: this is the type your language calls a bag or
multiset, except immutable and with a guaranteed deterministic observation
order. It is what `map`/`filter` pipelines produce, because those
pipelines run with automatic parallelism and positions are the price.

## Every value is ordered

```vix
let sorted_names = names.sorted();      // no Ord bound, no comparator needed
```

Every vix value supports `<=>` (three-way comparison) by construction —
scalars, records, arrays, multisets, everything. `<=>` subsumes the whole
comparison family: `==`, `<`, `<=`, `>`, `>=` are derived from it, so a type
never defines them separately.

A value orders by its fields, in declaration order. This is the value's
**structural order**, it is total, and **nothing can replace it** — there is no
way to define `<=>` for a type. If a type's structural order is wrong, the type
is wrong: reorder its fields, or declare a field whose own variant order carries
the rule you meant.

When you want to rank values some *other* way, you pass an order:

```vix
let ranked = rows.sorted(order: by_key(|r| r.weight));
```

An `Order<T>` is a value. `by_key(f)` ranks by the structural order of `f(x)`,
breaking ties by the structural order of `x` — so it is a total order *by
construction*, and consistent with `==` for free. A comparison that answers
`Equal` for values that are not equal cannot be written.

**Coming from Rust**: no `#[derive(Ord)]`, no `Ord` bounds, and no `impl Ord` —
intrinsic order is a property of the declaration. `sort_by_key` becomes an
`Order<T>` you hand to the operation that ranks.
**Coming from JS**: unlike `Array.prototype.sort`, there is no default
stringly comparison — values compare by their fields.

## Closures have no side effects

Every function you pass to a combinator is pure vix — there are no side
effects for it to perform, so the question every other language must
answer ("in what order does my callback run? how many times?") has no
observable content here. The implementation may run your closure on
sixteen threads, once per element, twice, or not at all for elements
nobody demands. You cannot tell, and that's the point.

---

## Array operations

### `array[i]`

Field access. Depends on element `i` alone.

### `.len() -> Int`

The number of elements. Free — it's the arity of the struct.

### `.map(f: fn(T) -> U) -> [U]`

Field-wise application: the result's field `i` is `f(self[i])`. Each
output element depends on exactly one input element — positions are
preserved, partial dependency is preserved, and all elements can be
computed in parallel because none of them is related to any other.

```vix
let manifests = members.map(parse);    // manifests[1] = parse(members[1])
```

**Coming from Rust/Haskell/JS**: looks identical, and for pure functions
it is. But there is no left-to-right execution promise, because there are
no effects to sequence. JS readers: no index argument — use `enumerate`.

### `.enumerate() -> [Indexed<T>]`

Pairs each element with its position: field `i` becomes `(i, self[i])`.
`Indexed<T>` is a plain alias for `(Int, T)`. Use it to carry positions
into position-destroying operations and recover them later:

```vix
let kept = xs.enumerate().values().filter(|(i, x)| wanted(x));
let in_original_order = kept.sorted();
// value order of (Int, _) sorts by the index first — original order back
```

### `.fold(init: R, f: fn(R, T) -> R) -> R`

Combines elements in field order: `f(f(f(init, self[0]), self[1]), ...)`.
On an empty array, `init`. Deterministic; field order is real for arrays.

```vix
let total = rows.fold(0, |acc, r| acc + r.weight);
```

### `.any(p) -> Bool`, `.all(p) -> Bool`, `.contains(x) -> Bool`

What they say. `contains` uses value equality. "Short-circuiting" is not a
semantic notion here — the result is just the boolean; how little work
produces it is the implementation's business.

### `.split_last() -> Option<(T, [T])>`

The last element and the array of everything before it, as fresh values;
`None` on an empty array.

```vix
match xs.split_last() {
    Some((last, rest)) => ...,
    None => ...,
}
```

**Coming from Rust**: this is `pop`, except nothing mutates — you get both
the element and the remaining array back. There is no `pop` in vix; names
that sound like mutation don't exist because the operations don't either.

### `.values() -> Multiset<T>`

Forgets positions. Free and always sound — the elements were independent
values all along.

---

## Multiset operations

### `.len()`, `.any(p)`, `.all(p)`, `.contains(x)`

As on arrays. Order-free by nature.

### `.map(f: fn(T) -> U) -> Multiset<U>`

Element-wise. The result is unordered like the input.

### `.filter(p: fn(T) -> Bool) -> Multiset<T>`

Keeps the elements satisfying `p`.

**Coming from Rust/JS**: your `filter` returns a compacted *sequence* —
survivor #2 sits at index 1, which silently forces an order on the
computation. Here survivors form a multiset; if you need positions, you
carried them in with `enumerate` (see above), and the code says so.

### `.filter_map(f: fn(T) -> Option<U>) -> Multiset<U>`

Filter and transform in one move: keeps the `Some` payloads.

### `.flat_map(f: fn(T) -> Multiset<U>) -> Multiset<U>`

Applies `f` to each element and unions the results.

### `.fold_ascending(init: R, f: fn(R, T) -> R) -> R`

Combines elements in increasing value order — the name says the order,
because a multiset has no other order to offer. Always deterministic, on
any machine, at any parallelism. There is no bare `fold` on multisets:
an order-sensitive combine over an unordered collection must say which
order it means.

**Coming from JS/Haskell**: `reduce`/`foldl` promise insertion order; a
multiset *has* no insertion order. A commutative-associative `f` (sums,
unions, maxima) behaves exactly as you'd expect and may be computed in
any order internally. If you meant "in original array order," fold the
array — arrays keep bare `fold`, in field order, because there the order
IS the reader's intuition.

### `.find_min(p) -> Option<T>`, `.find_max(p) -> Option<T>`

The least (greatest) element satisfying `p`, or `None`.

**Coming from Rust**: `Iterator::find` means "first by position" — a
concept multisets don't have. The deterministic replacements name their
selection rule. `find_min(|_| true)` is the minimum.

### `.take_min() -> Option<(T, Multiset<T>)>`, `.take_max() -> Option<(T, Multiset<T>)>`

The least (greatest) element and the multiset without one occurrence of
it, as fresh values; `None` on empty. The by-value cousins of a priority
queue's pop.

### `.sorted() -> [T]`, `.sorted_by(cmp: fn(T, T) -> Ordering) -> [T]`

The bridge back to positions: an array of the elements in structural order, or
in the order you pass. An `Order<T>` is total by construction, so there is no
way to hand `sorted_by` a comparison that fails to define a result. This is
where you knowingly pay for rank: the array as a whole depends on every element.

```vix
let by_weight = rows.values().sorted_by(|a, b| a.weight <=> b.weight);
```

---

## What deliberately does not exist

- **`pop`, `push`, `insert`, `remove` as mutations** — nothing mutates.
  The by-value forms exist where they earn it (`split_last`, `take_min`).
- **"First ready" selection** — an operation whose result depends on
  completion order would make program output nondeterministic. The
  implementation is free to *process* in arrival order whenever that's
  invisible (commutative folds); it is never allowed to *show* you.
- **Iterator objects** — there is no lazy iterator type to hold wrong.
  Pipelines compose collections; fusing `filter(...).map(...)` into one
  loop is the compiler's job, not a type you manage.
- **An index-taking `map`** — use `enumerate`.
