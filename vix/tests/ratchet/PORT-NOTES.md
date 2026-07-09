# Ratchet v2 port notes

vix/tests/ratchet/079-cross-run-reuse.vix:1 - `//! rerun` has no attribute mapping in SURFACE.md, so the header remains. PROPOSAL: add `#[test { rerun: true }]`.
vix/tests/ratchet/082-flaky-detected.vix:1 - `//! rerun` has no attribute mapping in SURFACE.md, so the header remains. PROPOSAL: add `#[test { rerun: true }]`.
vix/tests/ratchet/105-reuse-not-recompute.vix:1 - `//! rerun` has no attribute mapping in SURFACE.md, so the header remains. PROPOSAL: add `#[test { rerun: true }]`.
vix/tests/ratchet/110-module-memo-boundary.vix:2 - `//! rerun` has no attribute mapping in SURFACE.md, so the header remains. PROPOSAL: add `#[test { rerun: true }]`.
vix/tests/ratchet/082-flaky-detected.vix:2 - `//! expect-harness-flag: nondeterministic` is still a header because no in-language harness-flag surface is ratified. PROPOSAL: add `#[test { expect_harness_flag: "nondeterministic" }]` or keep this as a file directive.
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
vix/tests/ratchet/038-find-take-min-max.vix:5 - `find_min` on streams is used but not listed in SURFACE.md's stream method table. PROPOSAL: ratify deterministic selection operators or require `collect().values()` first.
vix/tests/ratchet/038-find-take-min-max.vix:7 - `take_min` on streams is used but not listed in SURFACE.md's stream method table. PROPOSAL: ratify deterministic selection operators or require `collect().values()` first.
vix/tests/ratchet/044-sets.vix:8 - Set element observation uses `s.keys().sorted()` because Round 9 addenda leave Set streaming/values shape open. PROPOSAL: define `Set<T>.values()` or `Set<T>.stream()` explicitly.
vix/tests/ratchet/087-propagate-narrows.vix:10 - Dynamic map update uses inferred `.insert(key) where { value }` spelling. PROPOSAL: ratify Map.insert's named parameter.
vix/tests/ratchet/088-propagate-conflicts.vix:8 - Dynamic map update uses inferred `.insert(key) where { value }` spelling. PROPOSAL: ratify Map.insert's named parameter.
vix/tests/ratchet/100-the-solver.vix:17 - Fixture index row lookup uses inferred `.row(pkg) where { version }` spelling. PROPOSAL: ratify fixture/index helper APIs in v2.
vix/tests/ratchet/100-the-solver.vix:19 - Dynamic selected-map update uses inferred `.insert(key) where { value }` spelling. PROPOSAL: ratify Map.insert's named parameter.
vix/tests/ratchet/100-the-solver.vix:31 - Dynamic domain-map update uses inferred `.insert(key) where { value }` spelling. PROPOSAL: ratify Map.insert's named parameter.
vix/tests/ratchet/138-map-accumulator.vix:8 - Dynamic accumulator update uses inferred `.insert(key) where { value }` spelling. PROPOSAL: ratify Map.insert's named parameter.
vix/tests/ratchet/089-mini-solve-trivial.vix:5 - `mini_solve(fixture_index()) where { requirements: reqs }` uses an inferred helper argument name. PROPOSAL: ratify fixture solver helper signatures.
vix/tests/ratchet/096-features.vix:6 - `mini_solve_with_features` uses inferred named arguments `requirements` and `features`. PROPOSAL: ratify fixture solver helper signatures.
vix/tests/ratchet/097-features-off.vix:5 - `mini_solve_with_features` uses inferred named arguments `requirements` and `features`. PROPOSAL: ratify fixture solver helper signatures.
vix/tests/ratchet/139-deep-nesting-identity.vix:9 - `Chain::Link(i, acc)` remains a two-payload enum constructor; SURFACE.md shows one-payload variants but does not rule multi-payload spelling. PROPOSAL: require record payloads for multi-field variants or ratify tuple variants with more than one field.
vix/tests/ratchet/118-payload-mismatch.reject.vix:7 - Reject pattern `Shape::Circle(r, extra)` intentionally keeps a two-payload pattern to preserve the payload-mismatch assertion. PROPOSAL: update this rung if multi-payload enum patterns are removed.
