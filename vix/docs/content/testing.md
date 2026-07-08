+++
title = "Testing"
weight = 15
+++

*Status: provisional — this page documents the language as designed; the
test system is specified here before it is implemented, deliberately: the
conformance ladder (`vix/tests/ratchet/`) is written against this page.*

Tests in vix are values, like everything else. A test describes a check;
running tests means demanding those checks; a test failure is an ordinary
value that says what went wrong. There is no test framework in the usual
sense — no runner lifecycle your code hooks into, no setup/teardown
ordering to reason about, no shared mutable fixtures *because there is no
shared mutable anything*.

## Declaring a test

```vix
test point_fields_are_independent {
    let p = Point { x: 3, y: 4 };
    expect_eq(p.x, 3);
    expect_eq(Point { x: p.x, ..p }, p);
}
```

`test NAME { ... }` declares a demandable check named `NAME`. The body is
a sequence of bindings and *expectations*; each `expect_*` denotes a
`Check` value, and the test's value is all of them combined. (The test
block is the one place expression-lines are allowed — each must be a
`Check`, and they combine as "all of these." Everywhere else, an
expression whose value goes nowhere is not a sentence.)

`vx test` demands every test in scope. `vx test point_fields` demands
one. A test nobody demands costs nothing, like everything else described
and not asked for.

## Expectations

```vix
expect(cond: Bool) -> Check
expect_eq(a: T, b: T) -> Check          // any T: everything is comparable
expect_ne(a: T, b: T) -> Check
expect_some(o: Option<T>) -> Check
expect_none(o: Option<T>) -> Check
expect_snapshot(v: T, name: String) -> Check   // renders v, compares to the stored snapshot
```

A failing `expect_eq` renders both sides — structurally, for any type,
because every value is serializable; you never write a `Debug` impl to
earn diagnostics. A failure carries the expectation's source span.

**Coming from Rust/JS**: assertions don't throw or panic — a `Check` is a
value (pass, or failure-with-context). All expectations in a test are
evaluated; you get every failure in one run, not the first one followed
by silence.

## Testing what must NOT happen: the `expecting` clause

Some of vix's most important promises are about absence — this arm was
never taken, that expensive value was never computed, this exec ran only
once. Nothing *inside* a program can observe evaluation (that's the
point), so these assertions belong to the harness, which stands outside
the program and holds its demand trace:

```vix
test partial_dependency_skips_expensive {
    let p = Point { x: cheap(), y: expensive() };
    expect_eq(p.x + 1, 42);
} expecting {
    never_demanded expensive;
    demanded cheap;
}
```

The body is checked in-language; the `expecting` block is checked by the
runner against the trace of what evaluation actually did. Available
trace expectations:

```
demanded NAME            — the named function was demanded at least once
never_demanded NAME      — it was not, transitively, ever
demanded_once NAME       — exactly once (memoization checks)
```

This split is doctrinal, not incidental: in-language code describes
values; only the holder of the graph can speak about evaluation.

## Compile-fail tests

A language's rejections are half its meaning. A test file whose name ends
in `.reject.vix` must *fail* to compile, and declares what the compiler
must say:

```vix
//! reject: expression statement
//! at: 4

fn f(state: State) -> State {
    state.domains.insert(k, v);   // value goes nowhere — not a sentence
    state
}
```

The runner compiles the file, expects failure, and matches the diagnostic
against the header. A reject file that compiles is a failing test.

## The ratchet

The conformance suite (`vix/tests/ratchet/`) is a numbered ladder:
`001` through `100`, ordered so that each rung uses only surface
introduced at or below it. `vx test --ratchet` reports the highest rung
N such that every rung ≤ N passes — the ratchet never counts a green
rung above a red one. Rung 100 is a working miniature of
[the solver chapter](/vix/building-a-solver). When rung 100 is green,
the language in this book exists.
