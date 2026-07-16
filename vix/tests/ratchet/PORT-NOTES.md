# Ratchet v2 port notes

vix/tests/ratchet/079-cross-run-reuse.vix:1 - `//! rerun` has no attribute mapping in SURFACE.md, so the header remains. PROPOSAL: add `#[test { rerun: true }]`.
vix/tests/ratchet/082-flaky-detected.vix:1 - `//! rerun` has no attribute mapping in SURFACE.md, so the header remains. PROPOSAL: add `#[test { rerun: true }]`.
vix/tests/ratchet/105-reuse-not-recompute.vix:1 - `//! rerun` has no attribute mapping in SURFACE.md, so the header remains. PROPOSAL: add `#[test { rerun: true }]`.
vix/tests/ratchet/110-module-memo-boundary.vix:2 - `//! rerun` has no attribute mapping in SURFACE.md, so the header remains. PROPOSAL: add `#[test { rerun: true }]`.
vix/tests/ratchet/106-imports.vix:1 - `//! uses: lib/geometry.vix` intentionally remains a file-level module directive per the mission.
vix/tests/ratchet/107-visibility.reject.vix:3 - `//! uses: lib/geometry.vix` intentionally remains a file-level module directive per the mission.
vix/tests/ratchet/108-import-std.vix:1 - `//! uses: lib/geometry.vix` intentionally remains a file-level module directive per the mission.
vix/tests/ratchet/109-name-collision.reject.vix:3 - `//! uses: lib/geometry.vix` intentionally remains a file-level module directive per the mission.
vix/tests/ratchet/110-module-memo-boundary.vix:1 - `//! uses: lib/geometry.vix` intentionally remains a file-level module directive per the mission.
vix/tests/ratchet/079-cross-run-reuse.vix:10 - Former `expecting on rerun` is now an ordinary yielded trace check, losing explicit rerun-only source scoping in syntax. PROPOSAL: add `on_rerun { yield ... }` or a check wrapper with rerun phase metadata.
vix/tests/ratchet/080-early-cutoff.vix:13 - Former `expecting on rerun` is now an ordinary yielded trace check, losing explicit rerun-only source scoping in syntax. PROPOSAL: add rerun-phase scoping for trace checks.
vix/tests/ratchet/081-projection-reuse.vix:12 - Former `expecting on rerun` is now an ordinary yielded trace check, losing explicit rerun-only source scoping in syntax. PROPOSAL: add rerun-phase scoping for trace checks.
vix/tests/ratchet/099-warm-restart.vix:10 - Former `expecting on rerun` is now an ordinary yielded trace check, losing explicit rerun-only source scoping in syntax. PROPOSAL: add rerun-phase scoping for trace checks.
vix/tests/ratchet/101-body-edit-early-cutoff.vix:12 - Former `expecting on rerun` checks were rewritten as value-level trace checks on `helper(21)` and `render(helper(21))`; rerun-only scoping is implicit only in the harness. PROPOSAL: add rerun-phase scoping.
vix/tests/ratchet/102-body-edit-negative-control.vix:11 - Former `expecting on rerun` checks were rewritten as value-level trace checks on `helper(21)` and `render(helper(21))`; rerun-only scoping is implicit only in the harness. PROPOSAL: add rerun-phase scoping.
vix/tests/ratchet/103-rename-is-cold.vix:10 - Former `demanded render` is now `demanded(render(compute(4)))`, pinning the value-level demand but still lacking rerun-phase syntax. PROPOSAL: add rerun-phase scoping.
vix/tests/ratchet/104-wrapper-refactor-warm.vix:10 - Former `never_demanded leaf` is now `never_demanded(leaf(6))`, stronger than name-level but still lacking rerun-phase syntax. PROPOSAL: add rerun-phase scoping.
vix/tests/ratchet/105-reuse-not-recompute.vix:11 - Former rerun checks are yielded normally; `memo_hits_at_least(1)` has no rerun-only syntax. PROPOSAL: add rerun-phase scoping.
vix/tests/ratchet/110-module-memo-boundary.vix:11 - Former `never_demanded magnitude_sq` is now value-level, but the cross-run scope is not encoded in v2 source. PROPOSAL: add rerun-phase scoping.
vix/tests/ratchet/137-corrupted-store-caught.vix:10 - Former `demanded costly` is now `demanded(costly(1111))`, stronger than name-level but still lacking rerun-phase syntax. PROPOSAL: add rerun-phase scoping.
vix/tests/ratchet/128-progressive-tree.vix:11 - NOTE B resolved by binding `producer` and `consumer`; `consumer` is the demanded subfile text value, not an explicit stage-two process. PROPOSAL: define `finished_before` accepted operands for value-vs-process comparisons.
vix/tests/ratchet/132-division-by-zero.vix:10 - NOTE C resolved by binding failing expression `boom = risky(0)` and passing it to `failed_with`. PROPOSAL: document whether binding a failing expression is preferred over inline failure expressions.
vix/tests/ratchet/133-overflow-checked.vix:7 - NOTE C resolved by binding failing expression `boom = big + 1` and passing it to `failed_with`. PROPOSAL: document whether binding a failing expression is preferred over inline failure expressions.
vix/tests/ratchet/136-unwrap-none-span.vix:11 - NOTE C resolved by binding `boom = first_even([1, 3])` and passing it to both failure checks. PROPOSAL: document whether multiple failure checks may share one bound failing expression.
vix/tests/ratchet/136-unwrap-none-span.vix:12 - `failure_span_in(boom, first_even)` uses the failing value plus the span owner; SURFACE.md only gives one example. PROPOSAL: specify the exact signature.
vix/tests/ratchet/059-distinct-args-distinct-demands.vix:7 - NOTE A resolved as two value-level checks, `demanded_once(costly(1))` and `demanded_once(costly(2))`.
vix/tests/ratchet/092-learning-prunes.vix:10 - `demanded_times(conflict_analysis, 1)` remains name-level because the rung has no concrete value-level call site to pin. PROPOSAL: specify whether count 1 should prefer `demanded_once` when no value expression exists.
vix/tests/ratchet/140-memo-at-scale.vix:12 - `demanded_times(f, 100000)` intentionally remains name-level per NOTE A because 100k distinct wires are asserted.
vix/tests/ratchet/033-multiset-conversion.vix:5 - Old "positions die" multiset assertion became `xs.stream().collect()` and now asserts keys survive, matching v2 but changing the old rung's vocabulary. PROPOSAL: retitle the rung around stream keys.
vix/tests/ratchet/036-multiset-fold.vix:6 - Preserving canonical value-order fold requires `collect().values().sorted().fold(...)`, not the shorter table rewrite, because stream key order and value order differ. PROPOSAL: add a named helper for value-order folds if this ceremony is intended.
vix/tests/ratchet/037-filter-map-flat-map.vix:8 - `filter_map` on streams is used but not listed in SURFACE.md's stream method table. PROPOSAL: either ratify `Stream.filter_map` or spell it as `flat_map` to an Option-derived stream.
vix/tests/ratchet/038-find-split-min-max.vix:5 - RESOLVED: deterministic `find_min`/`find_max` selection and `split_min` decomposition are ratified; mutation-shaped `take_min` is retired.
vix/tests/ratchet/044-sets.vix:10 - RESOLVED: `Set<T>.values()` exposes elements in structural order; `Set.has`, one-item `+`, and whole-set `++` are ratified.
vix/tests/ratchet/087-propagate-narrows.vix:10 - RESOLVED: `Map.with (key, value)` is the explicit insert-or-replace spelling.
vix/tests/ratchet/088-propagate-conflicts.vix:8 - RESOLVED: `Map.with (key, value)` is the explicit insert-or-replace spelling.
vix/tests/ratchet/100-the-solver.vix:17 - Fixture index row lookup uses inferred `.row(pkg) where { version }` spelling. PROPOSAL: ratify fixture/index helper APIs in v2.
vix/tests/ratchet/100-the-solver.vix:19 - RESOLVED: selected rows are known-new and use `+`; existing domain rows are deliberately rebound with `with`.
vix/tests/ratchet/138-map-accumulator.vix:8 - RESOLVED: the unique-key accumulator uses one-row `+` and remains eligible for molten publish-once lowering.
vix/tests/ratchet/089-mini-solve-trivial.vix:5 - `mini_solve(fixture_index()) where { requirements: reqs }` uses an inferred helper argument name. PROPOSAL: ratify fixture solver helper signatures.
vix/tests/ratchet/096-features.vix:6 - `mini_solve_with_features` uses inferred named arguments `requirements` and `features`. PROPOSAL: ratify fixture solver helper signatures.
vix/tests/ratchet/097-features-off.vix:5 - `mini_solve_with_features` uses inferred named arguments `requirements` and `features`. PROPOSAL: ratify fixture solver helper signatures.
vix/tests/ratchet/139-deep-nesting-identity.vix:9 - `Chain::Link(i, acc)` remains a two-payload enum constructor; SURFACE.md shows one-payload variants but does not rule multi-payload spelling. PROPOSAL: require record payloads for multi-field variants or ratify tuple variants with more than one field.
vix/tests/ratchet/118-payload-mismatch.reject.vix:7 - Reject pattern `Shape::Circle(r, extra)` intentionally keeps a two-payload pattern to preserve the payload-mismatch assertion. PROPOSAL: update this rung if multi-payload enum patterns are removed.

## GAP (opened by the round-10 backtick ruling): tests cannot name a capability

Amos ruled that a command is a **backtick tagged template whose tag is a capability
VALUE**: `` exec rustc`-c {src} -o out` ``. A macro (`rustc!{ … }`) is structurally
wrong — it cannot refer to the capability you just bound.

Twelve rungs run processes with **no capability at all**:

```
067-exec-echo, 068-exec-failure-is-result, 069-exec-memoized,
073-exec-consumes-tree, 074-exec-env, 082-flaky-detected,
126-effects-overlap, 127-fanout-parallel, 128-progressive-tree,
129-no-inline-draining, 130-kill-when-satisfied,
070-undeclared-capability.reject
```

They all say `exec! { echo … }`. That form is dead, and there is **no legal
replacement**, because nothing in the surface says how a `#[test]` obtains a
capability: `Rustc::acquire(target)` needs a `target`, and a test has no target
parameter. Per SURFACE §10, we kept the old shape and logged it rather than invent
`Sh::acquire`. **These twelve rungs are the only files in the repo still on the
dead spelling.**

### The finding underneath, which is better news

**Rung 070 becomes redundant, and that is the design working.**

`070-undeclared-capability.reject.vix` exists to reject `exec! { cc -o hello hello.c }`
— using a tool without declaring its capability. Under the tagged form you *cannot
write* that: `exec cc`…`` requires `cc` to be a bound capability value, so an
undeclared capability is an **unbound identifier**, caught by name resolution.
The rejection stops being a bespoke check and falls out of scoping.

And the ratchet currently contradicts itself: rung 067 runs `echo` undeclared and is
expected GREEN, while rung 070 runs `cc` undeclared and must REJECT. Only one of
those can be right.

### PROPOSALS (Amos adjudicates)

1. Every command is tagged. A test that execs must acquire a capability first. Then
   067–069 acquire something (`Sh`? `Coreutils`? name unruled) and **070 is deleted**,
   because its rejection is now a name-resolution error covered by the general
   unbound-identifier rung.
2. Or: 070 keeps its `//! reject:` header but its expected diagnostic changes from
   "undeclared capability" to "unbound identifier `cc`". Cheaper; loses the specific
   diagnostic.
3. Where does a test get a `Target`? A `#[test]` has no parameters. Either the
   harness supplies one (an ambient read — forbidden), or `#[test { target: … }]`
   names it (an input — a pin). The second is consistent with
   `r[machine.placement.no-in-program-steering]`.

## RESOLVED (round 11, Amos): tests declare capabilities; rung 070 dissolves

Amos: *"declared arguments of functions sporting the test attribute have the harness
forge those capabilities for testing purposes."* That is not a hack — it is
`r[machine.placement.no-in-program-steering]`: nothing in a program observes the
world; ambient facts arrive as **inputs supplied at the demand root**, and the
harness *is* the demand root.

All twelve rungs are swept:

```vix
#[test]
fn exec_echo(echo: Echo) -> Stream<Check> {
    let out = exec echo`"hello ratchet"`;
    …
}
```

**Rung 070 no longer needs to exist as a special check.** `exec cc`…`` cannot resolve
`cc` unless `cc: Cc` is a parameter, so an undeclared capability is an **unbound
identifier**. The rung is retained with that diagnostic; deleting it is a one-line
change if you'd rather the general name-resolution rung cover it.

Rung 074 improved on the way through: `env GREETING = "hi"` was a *command*
pretending to be a declaration. It is now a named argument —
`exec sh`-c "echo $GREETING"` where { env: %{ "GREETING" => "hi" } }` — which is
what "environment variables are declared values, not ambient leaks" was always
trying to say.

## STILL OPEN (needs a ruling)

1. **Capability granularity.** Is a capability a *tool* (`Echo`, `Wc`, `Cc`) or a
   *toolchain* that yields tools (the zoo's `Map[Symbol, DiscoveredTool]` with
   `:cc`, `:ar`, `:ranlib`)? The rungs above assume per-tool. `crate.vix` tags a
   command with `build_script`, a **String** — which suggests a third answer: a
   command may be tagged by any value with an identity that resolves to an
   executable, capability (advertised) or artifact (produced).

2. **`expect_eq`'s shape.** `testing.md` now says
   `expect_eq(actual) where { expected }`; the 139 rungs still say
   `expect_eq(a, b)`. Under at-most-one-positional, `expect_eq (a, b)` is legal —
   it passes ONE argument, a tuple — but a tuple has no room for the roles, so the
   swap bug returns. The book and the ratchet disagree until this is ruled. **Not
   swept: 400+ call sites should not move on an unruled decision.**
