+++
title = "Collections"
weight = 1
+++


Vix has one collection structure and several names for it. A **map** takes keys to
values and keeps its rows in key order. An **array** is a map whose keys are
positions. A **set** is a map whose values carry no information. A **tree** is a map
from names to entries, and one kind of entry is another tree.

Everything else in this chapter follows from that sentence.

## Arrays are structs

```vix
let members = [p"crates/taxon/Cargo.toml",
               p"crates/weavy/Cargo.toml",
               p"crates/vix/Cargo.toml"];

let second = members[1];
```

An array literal is a struct whose fields happen to be named `0`, `1`, `2`. This
is not a metaphor — it's the semantics, and it's why an array is a map. Each
element is an independent value: demanding `members[1]` depends on that field and
nothing else, and two fields of the same array can be computed in parallel, or one
computed and the other never touched.

**Coming from Rust/JS**: no growth, no capacity, no `push`/`pop` mutation family.
An array is closer to a tuple whose elements share a type.

## The four names

```vix
let versions = %{ "taxon" => v1, "weavy" => v2 };   // Map<String, Version>
let features = %["default", "std"];                 // Set<String>
let members  = [a, b, c];                           // [T], keys 0,1,2
let out      = exec cc`…`;                          // ExecOutcome; out.tree is a Tree
```

The `%` sigil means *the keys are explicit*. Bare brackets mean *the keys are
positions*.

`Set<T>` uses the same canonical map representation with elements as keys and
unit payloads, but it is a distinct standard type rather than a source alias.
That gives it element-oriented methods without pretending `Map<T,()>.map` maps
keys. `[T]` is also distinct because its keys are `0..n-1` and **density is an
invariant**, not a shape. Array keys cost nothing to store: they are positions.

A **`Tree`** is a map too, but a recursive one, keyed by a single path *segment*:

```vix
Tree      = Map<Name, TreeEntry>
TreeEntry = File { content: Blob, executable: Bool }
          | Dir (Tree)
          | Symlink { target: String }
```

`tree / p"src/lib.rs"` is a projection through two maps. A directory is a value, so an
empty one exists; a symlink is a value, so it round-trips. A flat map from paths to bytes
would be a lie that costs you `mkdir -p`, every symlink, and the executable bit.

## Every value is ordered, and nothing can change that

```vix
let ranked = names.sorted();
```

Every vix value supports `<=>` (three-way comparison) by construction. `<=>`
subsumes the whole comparison family — `==`, `<`, `<=`, `>`, `>=` derive from it,
so a type never defines them separately.

Integers compare numerically, strings by scalar value, blobs bytewise, floats by
IEEE total order with a canonical NaN, structs field-wise in declaration order,
enums by variant position then payload, arrays and maps lexicographically. A
comparison of two values with equal identity answers `Equal` without looking at
them.

A value orders by its fields, in declaration order. This is the value's
**structural order**, it is total, and **nothing can replace it**: there is no way
to define `<=>` for a type.

> **If a type's structural order is wrong, the type is wrong.**

Reorder its fields, or declare a field whose own variant order carries the rule
you meant. A `Version` needs semver's rule — a prerelease sorts *below* its
release — and no comparison function is required to say so:

```vix
enum PreIdent { Numeric(Int), Alpha(String) }       // numeric ranks below alphanumeric
enum PreTag   { Prerelease([PreIdent]), Release }   // prerelease ranks below release

struct Version { major: Int, minor: Int, patch: Int, pre: PreTag, build: Option<String> }
```

Walk it: major, minor, patch, then `PreTag` by variant position, then the
identifiers lexicographically, then `build`. That *is* semver precedence, clause
for clause, and it is the declaration that says so.

**Coming from Rust**: no `#[derive(Ord)]`, no `Ord` bounds, and no `impl Ord`.
Intrinsic order is a property of the declaration.
**Coming from JS**: no default stringly comparison — values compare by their fields.

## When you want a different ranking, you pass an order

```vix
let by_weight = rows.sorted where { order: by_key(|r| r.weight) };
```

An `Order<T>` is a value. `by_key(f)` ranks by the structural order of `f(x)`,
breaking ties by the structural order of `x` — so it is a **total order by
construction**, and consistent with `==` for free. A comparison that answers
`Equal` for values that are not equal cannot be written.

Intrinsic order comes from the declaration. Extrinsic ranking comes from an
argument. There is nothing in between.

## Streams

A stream is what the world hands you: the files in a directory, the lines a process
printed, the results of two hundred compiles running at once. You never write one
down — you receive it, transform it, and `collect` it into a value.

Arrays outnumber streams in any real program, because most values are small and
authored. What matters is that **nothing from outside arrives as an array**, and
the four lines below are why.

**A stream is not ordered.** Its elements arrive as they become available, and
arrival order is a scheduling artifact, not a property of any value.

**A stream is not a value.** It has no content hash of its own, so it cannot be a map
key, and it cannot be sorted or compared. When the last section said every value is
ordered, it did not mean this. A stream's *elements* are ordinary values, memoized
individually; the aggregate isn't a value until you `collect` it.

It *can* be a record field, and this is how a process hands you its output before it
has finished. A stream-typed field's **semantic content is the value it drains to** —
so the record has an identity, computed when the stream is done, while a reader may
consume the stream long before that. The live view is an optimization; the drained
value is the meaning.

**Every element carries its key** — where it came from.

```vix
src.glob("*.c")                    // Stream<Path, Path>
[3, 2, 1].stream()                 // Stream<Int, Int>, keys 0, 1, 2
```

`Stream<T>` is a stream whose keys you do not write. A generator's element is keyed
by its **location** — where in the demand graph it was described — which is unique
even when one `yield` site fires a thousand times inside a recursion, content-free,
and known before anything runs. It is emphatically *not* the order elements arrive
in. That distinction is the whole point of a stream, and the
[testing chapter](/testing) leans on it.

**Coming from Rust/JS**: this is `enumerate()`, except you never call it, and it
works for keys that aren't integers.

### `map` and `filter` keep the key

```vix
let objects = src.glob("*.c").map(compile);           // Stream<Path, Tree>
let small   = objects.filter(|o| o.size < 4096);      // Stream<Path, Tree>
```

`map` transforms the value and leaves the key alone. `filter` drops rows and
**renumbers nothing** — survivor number two keeps key `2`. That is what lets every
element stay independent: nothing has to know how many earlier elements survived.

`flat_map` composes keys into a path, so one element becoming many keeps a
deterministic address for each.

### `collect` is where a value is born

```vix
let objects = src.glob("*.c").map(compile).collect();   // Map<Path, Tree>
```

The rows come out in key order. **That is where determinism is created**, and it
is the only place it can be.

`collect()` has exactly one return type. There is no polymorphic collect, no
inference from the binding, no turbofish. It fails if two rows share a key — and
since `map` and `filter` preserve keys and `flat_map` extends them, a duplicate
key is always attributable to a `rekey` you wrote.

Notice what `collect()` does *not* need: an argument telling it how to sort. A
row's structural order compares the key first and never reaches the value. So
**sorting object files by their contents — which would reshuffle your link line
every time you edited a source file — is not something you must remember not to
do. It is unreachable.**

### `values` drops the keys

```vix
objects.values()      // [Tree] — the values, in key order
```

This is the only compaction in the language. It happens once, on a map that
already exists, at a call you wrote.

### The whole build

```vix
fn build(src: Tree) -> Tree {
    let objects = src.glob("*.c").map(|c| compile src c).collect();
    link objects.values()
}
```

Two hundred processes fan out and finish in whatever order they finish. `collect`
orders by source path. The link line is identical on every machine that builds
this, and stays identical when you edit `parser.c`, because `parser.o` moved in
content but not in key.

Had `glob` returned an array, its positions would have come from `readdir`, and
they would have flowed straight into the link command's argument order.

> **Positions have exactly two provenances: you wrote them, or you sorted them.**
> Never the filesystem's, never the scheduler's.

An array is therefore either **authored** — you wrote the order and the order is
data, like library link order or include search paths — or **collected**. Almost
nothing else should be an array. Environment variables are a map. Directory
listings are a stream. Command-line arguments are a typed command.

---

## Array operations

### `array[i]`

Field access. Depends on element `i` alone.

### `.len() -> Int`

The number of elements. Free — it's the arity of the struct.

### `.map(f: fn(T) -> U) -> [U]`

Field-wise: the result's field `i` is `f(self[i])`. Each output element depends on
exactly one input element, so positions are preserved and all elements can be
computed in parallel.

```vix
let manifests = members.map(parse);    // manifests[1] = parse(members[1])
```

**Coming from Rust/Haskell/JS**: looks identical, and for pure functions it is.
There is no left-to-right execution promise, because there are no effects to
sequence, and no index argument — stream it if you want the keys.

### `.stream() -> Stream<Int, T>`

Give up random access and positions-as-data; get back-pressure and keys that
survive filtering.

### `.fold(init: R, f: fn(R, T) -> R) -> R`

Combines elements in field order. Deterministic; field order is real for arrays.

### `.any(p)`, `.all(p)`, `.contains(x)`

Order-free by nature. They commit as soon as the answer cannot change.

### `.sorted() -> [T]`, `.sorted where { order: Order<T> } -> [T]`

An array of the elements in structural order, or in the order you pass. An
`Order<T>` is total by construction, so you cannot hand `sorted` a comparison that
fails to define a result.

## Map operations

### `.get(k) -> Option<V>`, `.insert(k) where { value: V } -> Map<K,V>`, `.len()`

By value, like everything. `insert` denotes a new map. There is no `m[k]`: a map
lookup can fail, and the type says so.

### `.unwrap()` on an `Option<T>` or a `Result<T, E>`

Takes the value, or **fails the demand** — a typed failure carrying the unwrap's
source span and the chain of demands that led there, never a bare string. It is not
a panic and it does not unwind: the demand completes with a failed outcome, and
everything that asked for it learns why.

**Coming from Rust**: `.unwrap()` here costs you a *diagnostic*, not a process.

### `.keys() -> [K]`, `.values() -> [V]`

In key order. `values()` renumbers: it is the compaction.

### `.stream() -> Stream<K, V>`

The rows, keys attached.

## Stream operations

### `.map(f: V -> U) -> Stream<K, U>`

Key untouched.

### `.filter(p: V -> Bool) -> Stream<K, V>`

Nothing renumbers.

### `.flat_map(f: V -> Stream<J, U>) -> Stream<(K, J), U>`

Keys compose into a path.

### `.collect() -> Map<K, V>`

The only return type. Fails on duplicate keys.

### `.any(p)`, `.all(p)`, `.contains(x)`, `.count()`

`any` and `all` commit the moment an unarrived element could no longer change the
answer. `count` waits, because it must.

### `Set<T>.map(f: T -> U) -> Set<U>`

The set of images; equal images coalesce. `index_by` is the distinct operation
that retains each source element as a key and attaches an image as its value.

---

## What deliberately does not exist

- **`Multiset<T>`** — a collection that is unordered but keeps duplicates is an
  array that has forgotten why. If you want counts, you want `Map<T, Int>`.
- **`enumerate`, `Indexed<T>`** — the key was always there.
- **`pop`, `push`, `insert`, `remove` as mutations** — nothing mutates. The
  by-value forms exist where they earn it (`split_last`, `appended`, `take_min`,
  map/set `insert` returning a fresh collection).
- **"First ready" selection** — an operation whose result depends on completion
  order would make program output nondeterministic. The implementation may
  *process* in arrival order whenever that's invisible; it may never *show* you.
- **Iterator objects** — a stream is not a lazy list. A lazy list has a
  deterministic order; a stream does not.
- **A polymorphic `collect`** — the return type of a call is never inferred from
  the context it is assigned into.
- **An index-taking `map`** — stream it.
