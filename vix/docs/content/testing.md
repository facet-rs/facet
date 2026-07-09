+++
title = "Testing"
weight = 35
+++

*Status: provisional — this page documents the language as designed; the
test system is specified here before it is implemented, deliberately: the
conformance ladder (`vix/tests/ratchet/`) is written against this page.*

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

Runner directives are attribute fields, not magic comments:

```vix
#[test { budget_wall: 5s, budget_rss: 1GB }]
fn molten_accumulator() -> Stream<Check> { … }
```

## Checks are values

```vix
expect(cond: Bool) -> Check
expect_eq(a: T, b: T) -> Check          // any T: everything is comparable
expect_ne(a: T, b: T) -> Check
expect_some(o: Option<T>) -> Check
expect_none(o: Option<T>) -> Check
expect_snapshot(v: T, name: String) -> Check
```

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
    yield never_demanded(expensive());
    yield demanded(cheap());
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
demanded_times(f, n)    — the function f was demanded exactly n times, over any arguments
```

The first three are **value-level**: they pin *which* demand you mean.
`demanded_once(costly(1))` says more than "`costly` ran once." The last is
name-level, for when you mean any call at all — it takes a function value.

The harness also speaks about the run as a whole: `never_read(path)`,
`memo_hits_at_least(n)`, `ran_processes(n)`, `overlapped()`, `killed(stage)`.

## Checks come in two kinds, and the order you yield them is not real

**Generators do not yield in yield order.**

```vix
#[test]
fn ordering_is_not_what_you_think() -> Stream<Check> {
    yield slow_check();      // may arrive second
    yield fast_check();      // may arrive first
}
```

A stream's order is *availability* order. Nothing about the source position of a
`yield` survives into the stream. This is the single most surprising thing in the
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

And that is what makes `never_demanded(expensive())` typecheck. A trace-check
constructor does not take a `T`. It takes the *description* of a demand:

```vix
fn never_demanded<T>(d: Demand<T>) -> Check
fn demanded_times<A, R>(f: fn(A) -> R, n: Int) -> Check
```

`Demand<T>` is what an un-demanded expression already is. Writing `expensive()` in
argument position produces one; nothing else in the language needs to know, because
nothing else can force it. `expect_eq(expensive(), 1)` takes `T`, so demanding that
check forces both sides — which is exactly the difference between the two kinds.

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

## The ratchet

The conformance suite (`vix/tests/ratchet/`) is a numbered ladder, ordered so that
each rung uses only surface introduced at or below it. `vx test --ratchet` reports
the highest rung *N* such that every rung ≤ *N* passes — the ratchet never counts
a green rung above a red one. Rung 100 is a working miniature of
[the solver chapter](/vix/building-a-solver). When rung 100 is green, the language
in this book exists. Rungs 101 and up say it is good.
