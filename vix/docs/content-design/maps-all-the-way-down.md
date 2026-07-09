+++
title = "Maps all the way down"
+++

Status: PROPOSAL (round 9). The collection kinds in this page are not four
different things. They are one thing, keyed four ways. Nothing here is
implemented; the pieces marked **RULED** were decided in conversation, the rest
is for Amos to tear.

## You already have one

The book says this about arrays, and has from the beginning:

> An array literal is a struct whose fields happen to be named `0`, `1`, `2`.
> This is not a metaphor — it's the semantics.

Read that sentence one more time. A struct whose fields are named by integers is
a map from integers to values.

```vix
let members = ["taxon", "weavy", "vix"];
```

`members` maps `0 → "taxon"`, `1 → "weavy"`, `2 → "vix"`. Demanding
`members[1]` demands the value at key `1` and nothing else. There is no
iteration order because there is no iteration: the array *is* its rows.

## You already have three more

A **Tree** — the thing `exec` produces — is a map from path to content. Not
approximately. This is `ExecTree`, in the code that runs today:

```rust
pub struct ExecTree {
    pub entries: BTreeMap<ExecPath, ExecText>,
    pub blobs:   BTreeMap<ExecPath, Vec<u8>>,
}
```

A **Set** is a map whose values carry no information:

```vix
let features = %["default", "std"];      // "default" → (), "std" → ()
```

And a **Map** is a map.

```vix
let versions = %{ "taxon" => v1, "weavy" => v2 };
```

So there is one structure, and the only question is what the keys are:

| you call it | keys are | written |
|---|---|---|
| array `[T]` | positions, dense from `0` | `[a, b, c]` |
| `Tree` | paths | produced by `exec` |
| `Set<T>` | the values themselves | `%[a, b]` |
| `Map<K, V>` | whatever you say | `%{k => v}` |

The `%` sigil means *the keys are explicit*. Bare brackets mean *the keys are
positions*. That's the whole story of the sigil, and it's why `%` cannot also be
modulo (**RULED**: it isn't; use `.rem()`).

## Rows are keyed values

A map's rows are pairs — a key and a value — and the rows are kept in key order.
Key order is **structural order** (**RULED**), which is derived from the value's
fields in declaration order and cannot be overridden. So a map's row order is a
function of its content, and no program can move it.

Write a row as `Keyed<K, V>`:

```vix
struct Keyed<K, V> { key: K, value: V }
```

The key is the *first field*, so a row's structural order compares the key
first. That one declaration is doing more work than it looks like, and we'll
come back to it.

## Where keys come from

Most things in vix are **streams** (**RULED**), not arrays. A stream is what you
get when you ask the world for something: the files in a directory, the lines a
process printed, the results of two hundred compiles running at once.

A stream is *not ordered*. Its elements arrive as they become available, and
arrival order is a scheduling artifact, not a property of any value. What a
stream does have is a **key for every element** — where that element came from.

```vix
[3, 2, 1].stream()          // Keyed(0, 3), Keyed(1, 2), Keyed(2, 1) — in any order
src.glob("*.c")             // Keyed(p"a.c", …), Keyed(p"b.c", …) — in any order
```

**Coming from Rust/JS**: this is `enumerate()`, except you never call it, and it
works for keys that aren't integers.

## `map` and `filter` keep the key

```vix
let objects = src.glob("*.c").map(compile);     // Keyed<Path, Tree>, still
let small   = objects.filter(|o| o.size < 4096) // Keyed<Path, Tree>, still
```

`map` transforms the value and leaves the key alone. `filter` drops rows and
renumbers nothing.

That second sentence is load-bearing. Here is what the book says about why
`filter` couldn't return an array:

> your `filter` returns a compacted *sequence* — survivor #2 sits at index 1,
> which silently forces an order on the computation

Compaction is what makes element *i* depend on how many earlier elements
survived. That dependency is why `filter` can't be parallel, and it's the entire
reason the design once needed an unordered collection type to escape into.

**Keep the key and nothing compacts.** Survivor #2 keeps key `2`. Every element
stays independent. The escape hatch isn't needed, and the multiset it escaped
into is gone (**RULED**).

## `collect` is where a value is born

A stream has no identity — it can't be a record field, can't be memoized, can't
be hashed, because its content isn't finished. `collect` drains it into a map:

```vix
let objects = src.glob("*.c").map(compile).collect();   // Map<Path, Tree>
```

The rows come out in key order, which is structural order on `Path`, which is
alphabetical. **That is where determinism is created**, and it is the only place
it can be.

`collect()` needs no argument telling it how to sort. The rows are `Keyed<Path,
Tree>`, the key is the first field, so structural order compares paths and never
reaches the trees. Sorting object files by their *contents* — which would
reshuffle your link line every time you edited a source file — is not something
you have to remember not to do. It's unreachable.

## `values` drops the keys

```vix
objects.values()      // [Tree] — the values, in key order
```

This is the only compaction in the language, it happens once, on a map that
already exists, and you wrote it. (In conversation this was called `unkey()`;
`values()` is the name the book already uses for exactly this.)

## The whole build

```vix
fn build(src: Tree) -> Tree {
    let objects = src.glob("*.c").map(|c| compile src c).collect();
    link objects.values()
}

fn compile(src: Tree, c: Path) -> Tree {
    exec cc!{ -c {src / c} -o {c.with_ext("o")} }
}

fn link(objs: [Tree]) -> Tree {
    exec cc!{ -o program {..objs} }
}
```

Two hundred `gcc` processes fan out. They finish in whatever order they finish.
`collect` sorts by source path. The link line is byte-identical on every machine
that ever builds this, and stays byte-identical when you edit `parser.c`,
because `parser.o` moved in content but not in key.

Compare what happens if `glob` returns an array. The array's positions come from
`readdir`. They flow into `objects`, and `objects`' field order *is* the link
command's argument order. Different filesystem, different link line, different
binary. Nobody wrote anything that looks wrong.

> **Positions have exactly two provenances: you wrote them, or you sorted them.**
> Never the filesystem's, never the scheduler's.

## Three things that come free

**Duplicate keys are a loud failure.** A map's keys are unique. If `a/foo.c` and
`b/foo.c` both compile to `foo.o`, the collect fails, and it fails with both
provenances in hand. Every build system in history ships this bug silently.

**`enumerate` is gone.** So is `Indexed<T>`. So is the trick where you wrap
elements in `(index, value)` to carry positions through a position-destroying
operation. The key was always there.

**Sorting by content is unreachable**, as above — not forbidden, *unreachable*,
which is cheaper and more honest than a rule.

## The wrinkle, stated plainly

After a `filter`, the keys are sparse:

```vix
let odd = [3, 2, 1].stream().filter(is_odd).collect();   // 0 → 3, 2 → 1
```

That is a `Map<Int, Int>` with keys `{0, 2}`. It is **not** an array, because an
array's keys are dense from zero. To get an array you call `.values()`, which
renumbers — and renumbering is exactly the compaction the design was avoiding.

So the compaction didn't disappear. It moved to a place where it's harmless: it
happens on a finished map, once, at a call you wrote, instead of implicitly
inside `filter` where it would have serialized the pipeline. The honest cost is
one extra word on the common path:

```vix
[3, 2, 1].stream().map(f).collect().values()      // and you get your array back
```

## Arrays are alive and rarer than you think

An array is one of two things:

1. **Authored.** You wrote the order, and the order is data. Library link order
   (`-lfoo -lbar` is not the same as `-lbar -lfoo`), include search paths,
   anything where position means priority.
2. **Collected.** You named a rule and the rule produced positions.

Almost nothing else should be an array. Environment variables are a map.
Directory listings are a stream. Command-line arguments are a typed command, not
a list of strings. The reflex to reach for `[T]` is inherited from languages
where it was the only container that existed.

## Coming from…

**Rust**: `Vec<T>`, `HashMap<K,V>`, `HashSet<T>`, and `BTreeMap<K,V>` are four
types because they're four *representations*. Here representation is the
implementation's business (the as-if law), and what's left is one type with four
key spaces. `.iter().enumerate()` is `.stream()`. `.collect()` really does mean
collect: it's where a lazy thing becomes a value.

**JS**: an `Array` whose keys are integers and an `Object` whose keys are strings
were always the same structure wearing different syntax, and you knew it, because
`arr[0]` and `obj["0"]` both work and you found it distasteful. It's the same
structure here too, but on purpose, and the keys are typed.

**Haskell**: `Data.Map k v`, and `[a]` is `Data.Map Int a` with dense keys.
Streams are not lazy lists — a lazy list has a *deterministic* order and a stream
does not. `collect` is not `sequence`; it is the point where nondeterministic
arrival becomes a deterministic value.

## Status

**RULED**: structural order and its inviolability; `Order<T>` values for
everything else; no modulo, `%` is a collection sigil; multisets die; streams are
the default and arrays are not the reflex; `glob` returns a stream; a stream is
not ordered.

**PROPOSED, and the point of this page**: that `collect()` returns a map, that
`[T]` *is* `Map<Int, T>` with dense keys, that `Tree` *is* a map (**corrected round 12**:
a *recursive* `Map<Name, TreeEntry>`, not a flat `Map<Path, Blob>` — the flat spelling
cannot hold a directory, a symlink, or an executable bit),
that `Set<T>` *is* `Map<T, ()>`, and that `values()` is the one compaction.

**OPEN**: whether `Keyed<K,V>` is a user-visible type or only the shape of a
map's rows. Whether a stream is `Stream<Keyed<K,V>>` or `Stream<K, V>`. Whether
`collect()` on a dense-`Int`-keyed stream may hand you `[T]` directly, which
would be convenient and would also be the one place the language pretends a
sparse map can't happen.
