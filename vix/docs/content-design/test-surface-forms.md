+++
title = "test surface: the candidate forms"
+++

Status: working comparison (2026-07-09). One test, every candidate shape.
The test: bind a point, check the cheap field, assert the expensive one
was never computed.

## Form A — explicit threading with rebind sugar

```vix
#[test]
fn partial_dependency(t: Test) -> Test {
    let p = Point { x: cheap(), y: expensive() };
    t .= assert_eq(p.x + 1, 42);
    t .= never_demanded(expensive());
    t
}
```

Honest, aligned with the molten rebind fast path — and the receiver is
pure ceremony: `t` appears five times carrying zero information. Amos: "it
looks terrible. I don't want to repeat it."

## Form B — ambient receiver block

```vix
#[test]
fn partial_dependency(t: Test) -> Test {
    let p = Point { x: cheap(), y: expensive() };
    with t {
        .assert_eq(p.x + 1, 42);
        .never_demanded(expensive());
    }
}
```

Kills the repetition; introduces an ambient-receiver scope — a new
binding form whose leading-dot lines are secretly rebinds of an invisible
thing. Distance without honesty. Amos: "also going to be ugly."

## Form C — marked rebinding call

```vix
    t.!assert_eq(p.x + 1, 42);
    t.!never_demanded(expensive());
```

Form A with the rebind moved into the call sigil. Same ceremony count,
one sigil uglier. Recorded because it names the underlying idea: the
call-site should SAY it updates its receiver.

## Form D — the test is a generator; assertions are yielded

```vix
#[test]
fn partial_dependency() yields Check {
    let p = Point { x: cheap(), y: expensive() };
    yield assert_eq(p.x + 1, 42);
    yield never_demanded(expensive());
}
```

No receiver exists. Assertions are free functions constructing `Check`
values; the test body is a **generator** denoting a stream of checks —
the banked codata shape, `(head, rest-closure)`, written forward. The
`#[test]` attribute states the expected yield type; the harness demands
the stream **fully, check by check** — which is also, for free, live
per-assertion progress and precise failure attribution.

Why this is not test-specific syntax: generators are already owed.
Streams are codata `(head, rest)`; the observer closures shipped to
runners must CONSUME such streams (the ruled answer: "a closure that
gives you a tuple of the next item and the closure for the rest") — and
anything that consumes them wants a sane way to PRODUCE them. `yield` is
the write-side of the same feature: a generator block lowers to the
nested (head, rest) cells; `yield` is cell construction — description,
not action. The no-statements law refines to: blocks contain bindings;
generator blocks contain bindings and yields; both describe.

Trace assertions need no receiver because the wire rides the value:
`never_demanded(expensive())` receives its argument as wiring (arguments
are wires), wraps it in a `Check` that carries the wire un-demanded, and
the harness installs the watch when that check arrives.

## must_use, in each world

Amos's ask: a `Test`/`Check` value accidentally bound and dropped should
be a bug, Rust-`#[must_use]`-style. In forms A–C that's a needed lint on
the `Test` type (drop the threading on the floor = silent nothing). In
form D the failure mode shrinks to "constructed a Check and didn't yield
it" — same marker, smaller surface:

```vix
let c = assert_eq(a, b);   // #[must_use] type, binding never used → error
```

Adopt the marker regardless of form; it's cheap and general (Outcome-ish
types want it too).

## Recommendation

**Form D.** It spends zero test-specific syntax (generators amortize
across streams and observers, where they're mandatory anyway), deletes
the receiver entirely rather than abbreviating it, gives check-by-check
demand semantics that match how a harness wants to run, and its one new
word — `yield` — means in vix exactly what it denotes: the next cell of
a described stream. Forms A's rebind sugar (`.=`) remains wanted on its
own merits for ordinary accumulator code (the solver's `domains .=
insert(...)`), just not as the test surface.

Open spelling questions inside D: `yields Check` vs `-> Stream<Check>`
in the signature; whether `yield` may appear in non-generator blocks
(no); whether a failing check stops the stream (no — fully demanded, all
failures reported; a harness flag can early-exit).
