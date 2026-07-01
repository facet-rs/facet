# `prec.dynamic` in snark: the bug, the fix, examples, and the perf follow-up

This directory backs the fix on branch `snark-conflict-collapse`. Everything here
was verified against **tree-sitter 0.26.9** (the DSL snark vendors), not invented.

## What was wrong

snark's Weavy accept path *honored declared conflicts* (it produced every accepted
tree) but **never applied `prec.dynamic`** — it returned `AmbiguousParse` where
tree-sitter picks the tree with the highest **total** dynamic precedence.

| grammar | input | tree-sitter (dynprec applied) | snark before fix |
|---|---|---|---|
| `dp.js` (reduce/reduce) | `a` | `(x)` — dyn 2 > 1 | `AmbiguousParse[x, y]` |
| `c1b.js` (shift/reduce, **no** `prec.left`) | `x \| f(y)` | `filter(x,f,args)` | `AmbiguousParse[filter-args, call]` |
| `c1.js` (shift/reduce, **with** `prec.left`) | `x \| f(y)` | `call(filter)` (assoc, static) | `call(filter)` — matched |
| `r1.js` (input-dependent) | `x x x x` | `pair pair` (maximal) | `AmbiguousParse[5 parses]` |

`c1` is the only pre-fix match, because `prec.left` resolves the shift/reduce
**statically by associativity** before dynprec ever engages ("unnecessary
conflict"). That is the trap that masked the whole thing — the filter-with-args
parse is achievable in plain tree-sitter/snark by *dropping* `prec.left` and
declaring the conflict (`c1b`), no grammar restructure required.

## The fix — DONE (`snark/src/lower/weavy.rs`, commit on this branch)

In the RuntimeWeavy accept path: among the lowest-error-cost accepted trees, keep
only those with the **maximum total dynamic precedence** (sum of each reduced
production's `dynamic_precedence`, via the existing `Production::dynamic_precedence()`
and the `TreeEvent::Reduce` production ids), then run the existing identical-tree
check. Genuinely-tied distinct trees still return `AmbiguousParse`.

- **No-op when no `prec.dynamic` is present** (max = 0 for all trees → filter is
  a pass-through), so unaffected grammars — including gingembre — are untouched.
- Verified: `dp` → `(x)`, `c1b` → filter-with-args, `r1` → `pair pair`, `c1`
  unchanged. 42 focused snark tests + the gingembre-snark-spike's 39 render-oracle
  tests all pass; no regressions.

Reproduce any row:
```
./ts.sh <grammar.js> '<input>'                          # tree-sitter reference
cargo run -p gingembre-snark-spike -- <grammar.js> '<input>'   # snark
```

## Classification (decidable / runtime / undecidable)

- **Collapsible (decidable, static):** input-independent winner — the competing
  continuations merge to a shared tail differing only by a *fixed production frame*,
  so the dynprec difference is a grammar constant. E.g. `c1b`: `+1`, always. These
  can be resolved at **build time** with no runtime split — the drop-in-perf
  optimization in `../static-conflict-collapse.md`. (The fix above resolves them
  *correctly* but pays the runtime split; the collapse makes them *free*.)
- **Runtime (decidable to classify):** input-dependent winner. E.g. `r1` — the
  number of maximal `pair`s depends on the input, so no constant frame. Must apply
  dynprec at runtime (which the fix now does).
- **Undecidable to classify:** deciding collapsibility in general reduces to CFL
  equivalence (do the two branches' tails recognize the same language and merge?),
  which is undecidable. The collapse analysis budgets the merge-search and falls
  back to the always-correct runtime split — so undecidability degrades to *perf*,
  never correctness.

## Remaining work

1. **Exact-tie tie-break:** after dynprec, genuinely-tied distinct trees still
   return `AmbiguousParse`; tree-sitter deterministically picks the first. For full
   drop-in parity add a deterministic tie-break. Kept conservative (error) here to
   surface real ambiguity.
2. **Reuse path:** a reused subtree replays its `Reduce` events into the accepted
   branch's `tree_events`, so `tree_dynamic_precedence` should sum them correctly —
   verified on fresh parse, not yet on incremental reparse.
3. **Static collapse** (`../static-conflict-collapse.md`): the build-time
   optimization that keeps snark a tree-sitter perf drop-in.

## Files
- `dp.js`, `c1.js`, `c1b.js`, `r1.js` — the example grammars.
- `ts.sh` — tree-sitter generate + parse harness (`./ts.sh grammar.js 'input'...`).
- `../static-conflict-collapse.md` — the collapse analysis (perf follow-up).
