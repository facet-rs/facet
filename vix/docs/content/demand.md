+++
title = "Description, not action"
weight = 10
+++

A vix program does not run. It *describes* — and something outside the
program decides what, of everything described, is worth computing.

## Bindings describe wiring

```vix
let index    = fetch_index registry;
let solution = solve manifest where { index };
let report   = render_report solution;
```

Nothing above executes anything. Three values are described: `report`
depends on `solution`, which depends on `manifest` and `index`. The
program has built wiring — a graph of values — and that is *all* a vix
program can ever do. There is no statement form, no "and then", no main
loop. Order of bindings is scoping, not sequencing: reordering
independent bindings means nothing, because nothing was ever going to
happen in "order."

## Demand comes from outside

Evaluation belongs to whoever holds the graph. `vx build report` places
*demand* on `report`; the wiring then decides everything else — `solve`
runs because `report` needs it, `fetch_index` runs because `solve` needs
it. An IDE asking "what's the type here" is a demand. An audit walking a
receipt is a demand. Test selection is a demand. One evaluation model,
one cache, one observability story.

Inside the program there is **no way to force evaluation** — no `force`,
no `seq`, no strictness annotation, no `await`, and unlike every lazy
language before it, no escape hatch. Laziness in vix is not an
evaluation strategy the runtime prefers; it is the absence of evaluation
from the language's semantics altogether.

Two consequences worth internalizing early:

- **Described-but-undemanded costs nothing.** A binding nobody
  transitively needs is not dead code to feel guilty about — it is free.
  Describe the debug report, the expensive cross-check, the alternate
  target; whoever wants one demands it.
- **You cannot observe evaluation.** No print-style tracing of "when did
  this run" exists at the language level, because "when" isn't a language
  concept. (Observability exists — richly — but as a property of the
  system around the program, not as effects inside it.)

> **What doesn't happen: eager arguments.** Watch the same call in JS and
> in vix:
>
> ```js
> const report = pick(mode, cheapSummary(data), fullAudit(data));
> // JS: BOTH summaries are computed before pick even starts.
> ```
>
> ```vix
> let report = pick(mode, cheap_summary(data), full_audit(data));
> // vix: pick's result is wired to its arguments; whichever one the
> // result actually depends on is the only one ever computed.
> ```
>
> In strict languages, passing an expression to a function is a promise
> to compute it. In vix it's a wire. Helper functions stop being
> performance decisions — you cannot accidentally compute the expensive
> path by mentioning it.

A concrete free-description example — this is idiomatic, not wasteful:

```vix
pub fn solution(ws: Workspace) -> Solution { ... }
pub fn report(ws: Workspace) -> Report { render(solution(ws)) }
pub fn debug_dump(ws: Workspace) -> Text { render_full_graph(solution(ws)) }
```

`vx build report` computes `solution` and `report`. The `debug_dump`
description sits there costing nothing — until the day something demands
it, at which point `solution` is already memoized and the dump costs only
its own rendering. You don't comment out expensive diagnostics in vix;
you just don't demand them.

## Dependencies are exact, and partial

```vix
let p = Point { x: cheap(), y: expensive() };
let q = p.x + 1;
```

`q` depends on `p.x`, which depends on `cheap()` — and *not* on
`expensive()`. Demanding `q` never computes `expensive()`. Dependency is
tracked at the shape of what you actually touch: one field of a record,
one element of an array, one file inside a tree of thousands:

```vix
let tree   = untar(fetch(crate_url));
let config = tree / p"Cargo.toml";     // depends on ONE file of the tree
```

This is not an optimization you enable; it is what the dependency *is*.
The machinery of projections exists to say precisely which part of a
value another value needs.

## Selection defers too

```vix
let artifact = match profile {
    Dev     => build_debug(src),
    Release => build_release(src),
};
```

This describes a value that depends on `profile` and on *the selected
branch only*. Demanding `artifact` demands `profile`, picks the arm, and
demands that arm's value — the other build is never computed. `match` and
`if` are expressions describing selection, not control flow that happens.

## What "running a program" means

`vx build app` is not a command interpreted by a tool that consults your
program. It is a demand placed on a value your program denotes — and
*evaluating that demand is the build*. There is no separate build-graph
format, no configure-then-execute phase split, no rule DSL: the language
is the build graph. Which is why a cache hit here is a proof and not a
hope: the system knows exactly what every value depends on, because
dependency is the one thing a vix program talks about.

**Coming from Rust/JS/Python**: the mental model to drop is "statements
run top to bottom." There are no statements. A function body is a set of
descriptions and one resulting value; calling the function describes its
result.

**Coming from Haskell**: vix is lazier than you're used to, in a specific
sense — not lazy-by-default-with-escape-hatches (`seq`, bang patterns,
`deepseq`), but lazy as in *the program has no evaluation vocabulary at
all*. The strictness your intuition wants back is the implementation's
job, done well, invisibly (see [Islands](/three-planes) territory in
the under-the-hood chapters).

**Coming from build systems**: this is the part where vix stops being "a
language with a build tool." Bazel/Nix/Make describe graphs in a
configuration layer and then execute them with an engine; here the
graph *is* the program's meaning, dependencies are observed rather than
declared, and the engine's questions ("what changed? what can be
reused?") become questions about values.
