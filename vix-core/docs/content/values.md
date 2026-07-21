+++
title = "Values"
weight = 5
+++

Everything in vix is a value. A string is a value. A parsed manifest is a
value. A compiled crate is a value. A test outcome is a value. A solved
dependency graph is a value. If something doesn't evaluate to a tangible
value, it does not exist in the language — there are no statements that
"do" things, no effect-objects, no null.

## Constructing values

```vix
struct Point { x: Int, y: Int }
enum Shape { Circle(Int), Rect { w: Int, h: Int } }

let p = Point { x: 3, y: 4 };
let s = Shape::Rect { w: 10, h: 20 };
let pair = (p, s);
let xs = [1, 2, 3];
```

Records, enums with payloads, tuples, arrays. Field access is `p.x`;
enums are consumed with `match` (see the pattern-matching chapter);
arrays are structs whose fields are named `0`, `1`, `2` (see
[Collections](/std/collections)).

## Nothing mutates

Every operation yields a fresh value. There is no assignment to a field,
no method that changes its receiver, no interior mutability, no escape
hatch. "Updating" a record means describing a new one:

```vix
let moved = Point { x: p.x + 1, ..p };
```

`moved` is a new value; `p` is untouched and every other user of `p` is
unaffected — nothing aliases, so nothing can be affected. Code that looks
like it accumulates (building up state across a fold, inserting into a
map) is describing a *sequence of values*, each fresh:

```vix
fn put_domain(state: State) where { pkg: PkgId, domain: Domain } -> State {
    State { domains: state.domains.with (pkg, domain), ..state }
}
```

`with` returns a new map; the record spread builds a new state around
it. Whether the implementation actually copies anything is its own
business — semantically, you were never sharing.

> **What doesn't happen: the fire-and-forget "update".** In most languages
> this line does something:
>
> ```js
> state.domains.set(pkg, domain);   // JS: state is now different
> ```
>
> ```vix
> state.domains.with (pkg, domain)   // vix: not a sentence
> ```
>
> In vix it isn't even grammatical — there are no expression statements,
> because an expression whose value goes nowhere *describes nothing*. The
> compiler rejects it, and that rejection is the language telling you the
> truth: `with` never changed anything; it denoted a new map you didn't
> name. If a result is discarded through an otherwise legal shape such as an
> underscore binding, `must_use` warns instead; warning promotion remains a
> user or harness policy.

Three languages, one situation — two names for one collection:

```rust
let mut a = vec![1, 2, 3];
let b = &a;
a.push(4);            // Rust: rejected — cannot mutate while borrowed
```

```js
const a = [1, 2, 3];
const b = a;
b.push(4);            // JS: accepted — and `a` changed. Spooky.
```

```vix
let a = [1, 2, 3];
let b = a;            // vix: b IS a, forever. Nothing to reject,
                      // nothing to be spooked by — neither can change.
let c = b + 4;        // [1, 2, 3, 4]; a and b are unchanged.
```

Rust protects you from aliased mutation with a borrow checker you must
negotiate with. JS just lets it happen. Vix removes the phenomenon.

**Coming from Rust**: ownership, borrowing, lifetimes, `&`, `&mut`,
`Clone`, `Rc` — none of it exists here, and not because everything is
secretly copied. There is nothing to coordinate: no aliasing means the
questions those features answer never come up. Write as if copies were
free; making that true is the implementation's job, not yours.

**Coming from JS/Python**: there is no reference semantics to be surprised
by. Passing a value somewhere cannot let that somewhere change it under
you; two names never point at one mutable thing.

**Coming from Haskell/Clojure**: this is the persistent-data-structure
world you know, taken as the only world — with no `IORef`/atom escape
valve anywhere in the language.

## Equality, ordering, and hashing are ambient

Every value supports `==`, `<=>` (three-way comparison), and content
hashing, by construction. You never derive them, never implement a trait,
never discover at the worst moment that some type deep in a structure
isn't comparable:

```vix
let same    = a == b;          // structural, any type
let ord     = a <=> b;         // Ordering: Less | Equal | Greater
let sorted  = things.sorted(); // no bounds to satisfy
```

This is not a convenience feature; it is load-bearing. Values are
content-addressed — a value's identity is derived from what it *is* — and
that is what makes builds cacheable, results shippable between machines,
and collections deterministically orderable. The [three planes of
identity](/three-planes) chapter tells the whole story.

**Coming from Rust**: no `#[derive(PartialEq, Eq, Hash, Ord)]` stanzas,
and no types that opt out. Functions and values containing them are
compared by identity of their definition, not excluded from comparison.

## Why by-value is the load-bearing wall

Because nothing is shared and nothing mutates, a value can be handed to
another thread, another process, or another machine without any
coordination — there is no lock to take, no ownership to transfer, no
race to imagine. The same property lets the implementation cache any
value, recompute instead of share when that's cheaper, and replay any
in-flight work from scratch without your program being able to tell.
Those freedoms — and their one governing law — are the subject of the
under-the-hood chapters; as a user you only need the guarantee they rest
on: **a value is only ever what it says it is.**
