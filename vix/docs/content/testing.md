+++
title = "Testing"
weight = 35
+++

The conformance ladder (`vix/tests/ratchet/`) is written against this page and
the normative language/runtime specifications.

Tests in vix are values, like everything else. A test describes checks; running
tests means demanding them; a failure is an ordinary value that says what went
wrong. There is no test framework in the usual sense — no runner lifecycle your
code hooks into, no setup or teardown ordering, no shared mutable fixtures
*because there is no shared mutable anything*.

There is also no test-specific syntax. A test is an ordinary function.

## Declaring a test

```vix
#[test]
fn point_fields_are_independent() -> Stream<Check> {
    let p = Point { x: 3, y: 4 };
    yield expect_eq(p.x, 3);
    yield expect_eq(Point { x: p.x, ..p }, p);
}
```

A test is a function returning `Stream<Check>` — a generator. `#[test]` marks it;
attributes are the same surface that carries decode annotations, so tests cost no
new grammar. Running a test means demanding its checks.

`vx test` demands every test in scope. `vx test point_fields` demands one. A test
nobody demands costs nothing, like everything else described and not asked for.

In-language test options are attribute fields:

```vix
#[test { budget_wall: 5s, budget_rss: 1GB }]
fn molten_accumulator() -> Stream<Check> { … }
```

## A test's parameters are what the harness supplies

A test that runs a process needs a capability, and a program may not go looking for
one — nothing in a program observes the world. So it **declares** what it needs, and
the harness, which stands outside the program, supplies it:

```vix
#[test]
fn exec_echo(sh: Sh) -> Stream<Check> {
    let out = exec sh`echo "hello ratchet"`;
    yield expect_eq(out.stdout.decode(Utf8).text().trim(), "hello ratchet");
}
```

This is the same act as `vx build --target` defaulting to the host: the demand root
supplies an input. An ambient read is an observation; an input is a pin.

It also means the harness may **forge** a capability — hand the test a fake `Sh`
whose outputs are fixtures — which is how you test an exec without a toolchain.

And using a tool you did not declare is not a special error. It is an **unbound
identifier**: `exec cc`…`` cannot resolve `cc` unless `cc: Cc` is a parameter. The
language has nothing to say about undeclared capabilities because it cannot express
one.

## Checks are values

```vix
expect(cond: Bool) -> Check
expect_eq(pair: (T, T)) -> Check         // any T: everything is comparable
expect_ne(pair: (T, T)) -> Check
expect_some(o: Option<T>) -> Check
expect_none(o: Option<T>) -> Check
expect_snapshot(v: T) where { name: String } -> Check
```

`expect_eq(a, b)` passes **one** argument — the tuple `(a, b)` — so at-most-one
holds without ceremony. The two sides of an equality are the same kind of thing, and
a tuple is what "the same kind of thing, in order" means.

A failing `expect_eq` renders both sides — structurally, for any type, because
every value is serializable; you never write a `Debug` impl to earn diagnostics.
A failure carries the check's source span.

`Check` is `must_use`. Constructing one and forgetting to yield it is a compile
error, not a test that silently passes.

**Coming from Rust/JS**: assertions don't throw or panic — a `Check` is a value
(pass, or failure-with-context). Every check in a test is evaluated; you get every
failure in one run, not the first one followed by silence.

## Testing what must not happen

Some of vix's most important promises are about absence: this arm was never taken,
that expensive value was never computed, this process ran once. Nothing *inside* a
program can observe evaluation — that's the point — so these are claims the harness
makes, holding the demand trace from outside.

They need no special syntax either, and the reason is the deepest fact in the
language:

```vix
#[test]
fn partial_dependency_skips_expensive() -> Stream<Check> {
    let p = Point { x: cheap(), y: expensive() };
    yield expect_eq(p.x + 1, 42);
    yield never_demanded (expensive());
    yield demanded (cheap());
}
```

`never_demanded(expensive())` is an ordinary function call. Passing an expression
*describes* a value; it does not compute one. So the check can hold `expensive()`
without ever putting it in a demanded position, and the harness compares that
description against what evaluation actually did.

```
demanded(expr)          — this value was demanded at least once
never_demanded(expr)    — it was not, transitively, ever
demanded_once(expr)     — exactly once (memoization checks)
demanded_times(f) where { times: n }  — f was demanded exactly n times, over any arguments
```

The first three are **value-level**: they pin *which* demand you mean.
`demanded_once (costly 1)` says more than "`costly` ran once." The last is
name-level, for when you mean any call at all — it takes a function value.

The harness also speaks about the run as a whole: `never_read path`,
`memo_hits_at_least n`, `ran_processes n`, `overlapped()`, `killed stage`, and
`finished_before consumer where { producer }`.

## Checks come in two kinds, and the order you yield them is not real

**Generators do not yield in yield order.**

```vix
#[test]
fn ordering_is_not_what_you_think() -> Stream<Check> {
    yield slow_check();      // may arrive second
    yield fast_check();      // may arrive first
}
```

A stream's order is *availability* order. Each check keeps its own identity — a
generator's elements are keyed by where they were described — but **nothing about
the source position of a `yield` determines when it arrives.** This is the single most surprising thing in the
language for a reader coming from any other generator, and it is load-bearing: it
is what lets the harness report failures the instant they are known, and it is why
a stream is not a lazy list.

It also means a claim about the *whole run* cannot be evaluated where you wrote
it. So `Check` is two things, and it says so:

```vix
enum Check {
    Value(ValueCheck),     // expect_eq, expect_some — demanded during the run
    Trace(TraceCheck),     // never_demanded, overlapped — a claim about the finished run
}
```

The harness drains the stream — which constructs every `Check` and demands nothing
— then demands the `Value` checks, and only then the `Trace` checks against the
completed trace. **You never order them; the variant does.** Yield them wherever
they read best.

And that is what makes `never_demanded(expensive())` work. Function arguments
are wires, so a trace-check constructor can identify the described invocation
without reading its result. These are harness intrinsics, not a general
reflection type:

```vix
fn never_demanded<T>(described: T) -> Check
fn demanded_times<A, R>(f: fn(A) -> R) where { times: Int } -> Check
```

Writing `expensive()` in argument position passes its wire; the trace intrinsic
records its recipe/location description without consuming the value.
`expect_eq(expensive(), 1)` constructs a value check whose payload consumes `T`,
so demanding that check demands both sides. There is no user-visible
`Demand<T>`/promise wrapper.

**Coming from Rust/Python/JS**: your generator resumes where it left off and
yields in program order. This one does not. If you write code that depends on
yield position, it is wrong in a way that will pass on your laptop.

## Compile-fail tests

A language's rejections are half its meaning. A test file whose name ends in
`.reject.vix` must *fail* to compile, and declares what the compiler must say:

```vix
//! reject: expression statement
//! at: 4

fn f(state: State) -> State {
    state.domains.insert(k, v);   // value goes nowhere — not a sentence
    state
}
```

Compile-fail cannot live in-language, so these keep their headers. The runner
compiles the file, expects failure, and matches the diagnostic. A reject file that
compiles is a failing test.

Fixture selection, rerun mutations, alternate source files, and expected harness
flags are likewise file-level harness metadata when they describe orchestration
rather than a Vix value. They remain leading `//!` directives until the ratchet
gains an adjacent typed Styx manifest; they are not language statements.

## The ratchet

The conformance suite (`vix/tests/ratchet/`) is a numbered ladder, ordered so that
each rung uses only surface introduced at or below it. `vx test --ratchet` reports
the highest rung *N* such that every rung ≤ *N* passes — the ratchet never counts
a green rung above a red one. Rung 100 is a working miniature of
[the solver chapter](/building-a-solver). When rung 100 is green, the language
in this book exists. Rungs 101 and up say it is good.
