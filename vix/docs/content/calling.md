+++
title = "Calling"
weight = 7
+++

```vix
let manifests = parse_workspace root where { members, target };
```

There are no parentheses in that call, and there is exactly one thing to the left
of `where`. Both facts are the rule.

## Application is juxtaposition

A value followed by a value calls the first with the second.

```vix
parse root            // call parse with root
double 21             // call double with 21
```

Parentheses are **grouping**. They are never call syntax. So `parse (root)` is
`parse root`, and `(a + b)` is what you expect.

**Coming from Rust/JS**: `f(x)` still works, because it is `f` applied to the
group `(x)`, which is `x`. But `f(a, b)` is `f` applied to the *tuple* `(a, b)`,
which is a different thing and almost never what you want.

## At most one positional argument

The one positional argument is the **subject** — the thing being acted on.
Everything else is named.

```vix
exec cc`-c input.c -o input.o` where { mounts, writable: [p"out"] }
rows.sorted where { order: by_key(|r| r.weight) }
range where { from: 0, to: n }
```

A call can have no subject at all: `range` acts on nothing, so it names both its
bounds — and that spelling kills the off-by-one squint that `range(0, n)` has
inflicted on every language that has one.

This is not a style rule. It is what makes juxtaposition safe: with two positional
arguments, `f a b` needs a grouping convention and a reader has to know it. With
one, the question cannot be asked.

**Coming from Swift or Smalltalk**: you have been here. `insert(_:at:)`,
`dict at: k put: v`. The labels are part of the call's meaning, not decoration.

### The rule bites, and it is supposed to

```vix
fn add_clause(state, parent_pkg, parent_version, tag, dep_pkg, req, req_text, kind)
```

Two of those parameters are adjacent `String`s. Swap them and it compiles, type-
checks, and is wrong. Named arguments fix that — and they are not the right fix.
**Make a record:**

```vix
struct Guard { parent: PackageVersion, dep: PkgId, req: VersionSet, kind: DepKind }

state.add_clause guard
```

Named arguments make a wide signature *safe*, which removes the pain that would
otherwise have made you write `Guard`. Watch for that.

## Named arguments are a record

`where` in a signature **declares** the named-argument type. `where { }` at a call
site **constructs** it.

```vix
fn exec(cmd: Command<A>) where {
    mounts:   [Mount] = [],
    writable: [Path]  = [],
} -> ExecOutcome<A>
```

`where` sits to the **left of `->`**, because it names *inputs*, and inputs belong
on the left of the arrow. (Rust's `where` is a different word that happens to be
spelled the same. It holds bounds. This one holds arguments.)

Inline `where { … }` declares an **anonymous** record type — this function's
one-off option set. Naming the type instead makes it a value you can build once
and reuse:

```vix
fn exec(cmd: Command) where ExecOpts -> ExecOutcome

let base = ExecOpts { mounts, writable };
exec cmd_a where { ..base };
exec cmd_b where { ..base, writable: [p"out2"] };
```

That is the whole of "partial application" in this language: pre-binding named
arguments is record construction, and pre-binding the subject is a closure. There
is no `partial` keyword because there is nothing left for it to do.

## Punning, and defaults

Bare names inside braces mean `name: name`:

```vix
let mounts = [src_mount];
exec cmd where { mounts }              // where { mounts: mounts }
Guard { parent, dep, req, kind }       // works in record literals too
```

A parameter with a default **must** be passed by name. So adding a defaulted
parameter never breaks an existing call site — which is why one function with
`target: Option<Target> = None` replaces the `do_thing` / `do_thing_target` pair
that every codebase grows.

## Attributes are records too

`name: value` is the spelling everywhere. Never `name = value`. So an attribute is
a record:

```vix
#[test { budget_wall: 5s, budget_rss: 1GB }]
fn molten_accumulator() -> Stream<Check> { … }
```

## Methods keep their argument

A method call takes a receiver and one argument, which is the same shape:

```vix
domains.with (pkg, narrowed)
xs.map(f)
```

The receiver is implicit in method syntax. A method may take one additional
positional argument; empty `()` is the zero-argument method form. Parentheses
around a nonempty method argument are still grouping: `xs.map(f)` is `xs.map (f)`.

## What you can't write, and why

```vix
abs -1        // parses as `abs - 1`. Subtraction.
abs (-1)      // what you meant
```

A juxtaposed argument must be an **atom**: an identifier, a literal, a string, a
path, or a bracketed group. `-1` is not one. The compiler says so and offers the
parenthesis.

The alternative — deciding by whitespace, so that `abs -1` and `abs - 1` differ —
is a bug you cannot see in a diff.

**Coming from Haskell or OCaml**: this is the `f (-1)` wound, healed by rejection
rather than by folklore.
