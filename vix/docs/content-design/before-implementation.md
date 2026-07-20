+++
title = "Before implementing for real"
+++

Status: GATE LIST (round 11). What must be true before the implementation starts, in
the order it must become true. Grounded in what exists, not in what feels missing.

## Gate 0 — nothing has ever parsed this language

- The since-deleted `playgrounds/snark/src/bundled/vix/grammar.js` described the **old**
  surface. It knew nothing of juxtaposition, `where`-as-arguments, backtick commands,
  `struct { }`, punning, suffix literals, or postfix `?`.
- **Zero lines of code reference `vix/corpus-next/`.**
- `vix/tests/ratchet/` is 144 `.vix` files and three `.md` files. There is **no runner**.

So 5,700 lines of vix have been checked by prose and by three critics, and by no machine.

> Every part of vixen has an oracle except the scheduler. **The language has none,
> because nothing parses it.**

A grammar plus a parser turns 5,700 lines of prose-checked vix into machine-checked vix
overnight, and the **19 `.reject.vix` files are its conformance suite**. It is also the
only way to learn whether the grammar is unambiguous — the implementability critic flagged
that `where` and block atoms need their rejections written down, and nothing forces that
like a parser.

**This is the cheapest reality check available and it has not been taken.** It should be
the first thing built, before any engine.

## Gate 1 — rulings that are cheap now and expensive to retrofit

Ranked by retrofit cost.

1. ~~**What does `exec` return?**~~ **ANSWERED, round 12.**
   `{ tree: Tree, stdout: Stream<Int,String>, stderr: Stream<Int,String> }`, no exit status
   (`r[machine.primitive.exec-outcome]`, `r[machine.primitive.exit-status-is-not-a-value]`).
   `exec` and `place` are decoupled; the observer closure is the *lowering* of a placed
   block, not a surface construct. Progressive trees are partial dependency.
   **Owed:** the three `--stdout {p"cfg.stdout"}` corpus sites now have a home and should
   be re-ported; command grammars must declare which exit codes are answers.
2. **Typing discipline.** Bidirectional inference (expected types flowing into `%{}`,
   `[]`, `None`, decode calls, closure params) or annotations at ambiguous sites? The ports
   assume the former, inconsistently. Changes what is *writable*; cannot be retrofitted.
3. **Generics.** Monomorphization over schemas, with `eq`/`ord`/`hash`/`serialize` ambient
   for every `T`, no dictionaries — confirm or deny.
4. **Method lookup precedence.** One deterministic rule over: `namespace` inherent methods,
   import-scoped `extend`, builtins, ambient operators.
5. **Modules and imports.** Every rung is one file. Band 106 demands modules. `import` is
   unruled (statement vs function-style), visibility unspecified.
6. **Map identity.** `r[machine.identity.map-order-independence]` is OPEN, and
   `carried-hasher` cannot be settled without it. This is in the identity plane, so it must
   land **before the epoch freeze** (Gate 3).
7. **Does codata cross a `place` boundary?** Streams cross island edges (ruled). A stream
   crossing hosts makes back-pressure and cancellation a distributed protocol.
8. **Does the AST travel, or the lowered island?** Decides whether an executor hosts `vixc`
   or only `weavy` — that is, whether it can be a static binary on a machine you do not
   administer.
9. **Does `Rust::acquire` survive?** A materialized toolchain is a resolved package graph
   (round 11), so depending on one is depending on a package. `acquire` may name nothing.

## Gate 2 — the ratchet is not yet a specification

- **Its ordering invariant is broken.** `testing.md` says "each rung uses only surface
  introduced at or below it." Rung 001 now returns `Stream<Check>`. Generators, streams and
  the phase-partitioned `Check` are rung-**zero** infrastructure. Either restate the
  invariant or restructure the ladder.
- **~12 harness primitives are defined in no chapter**: `finished_before`, `killed`,
  `overlapped`, `memo_hits_at_least`, `store_interns_at_most`, `scheduler_requests_at_most`,
  `ran_processes`, `fetched`, `failure_span_in`, `failed_with`, `demanded_times`,
  `never_read`. The rungs use them. The book does not define them.
- **Twelve rungs and rung 070**: swept onto declared capabilities (round 11), but the
  capability *granularity* question is open (tool vs toolchain projection).
- **`ddc coverage` binding across the new mounts is UNVERIFIED.** `FOUNDATION.md` gates
  score bands on rule coverage. If the binding is broken, the gate does not exist. Verify by
  querying, not by eyeballing markdown.

## Gate 3 — the identity epoch

Implementing before the freeze means rehashing everything afterwards.

- Byte-discriminant kind tags (the "257th symbol" — a kind tag must not be drawn from the
  same alphabet as type names).
- The anonymous-`struct` kind, which needs a tag rather than an empty name.
- `legacy_marker_schema_id` derives a `SchemaId` from a type's **rendered name string**, not
  its structure. Identity by spelling.
- Map identity (Gate 1.6).

There are **no deployed users of the current identity bytes** (Amos), so the break is free —
but only until there are.

## Gate 4 — the shadow standard library

Nine types are used hundreds of times across the corpus and have no chapter:
`Map`, `Set`, `Tree`, `Path`, `String`, `Option`, `Result`, `Version`, `Doc`.
(`Option` and `Result` are now half-covered by `/vix/errors`.)

`.get()` ×153, `.unwrap()` ×135, `.insert()` ×77 — against pages that do not exist.

## Then: what to build, in order

1. **The grammar and the parser.** Reject corpus as its conformance suite. Parse the 5,700
   lines. Fix what it finds — in the corpus, and in the grammar.
2. **The reference evaluator — the oracle.** Slow, tree-walking, obviously correct.
   `open-proprietary-division.md` already lists it as an artifact of the open repo.
   `FOUNDATION.md` fears a tree-walker *climbing* the ladder; a **declared** tree-walker is
   the opposite thing — it is what makes the fast engine checkable. Differential-testing the
   real engine against it turns the ratchet from a behaviour test into a semantics test.
3. **The runner contract (rung 001).** Demand trace, standing counters
   (allocations-on-memo-hit = 0, hostcalls-per-pure-op = 0, store interns, effect spawns),
   a receipt for every demand, and `--chaos` from the first green.

`FOUNDATION.md` prices the architectural reckoning at rung 001, deliberately. **Rung 001 is
not small, and that is the contract working.** "Start implementing" means "build the runner
contract," not "write a tree-walker and see how far it climbs."

## The one thing that is already true

The corpus is the most valuable artifact in this repository, and it is design fiction until
something parses it. Five thousand five hundred lines of real programs — a resolver, a
manifest ingester, a build lane — written in a language with no parser.

Parse it first.
