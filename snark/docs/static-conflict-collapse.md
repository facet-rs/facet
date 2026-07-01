# Static collapse of declared conflicts — runtime `prec.dynamic` for free

## The problem

snark should honor a **declared** shift/reduce conflict + `prec.dynamic`, which
tree-sitter resolves statically (by associativity) and warns is "unnecessary".
That lets a grammar author say, declaratively, "when `x | f(args)` is ambiguous
between filter-with-args and `call(filter, args)`, prefer the filter" — a thing
tree-sitter cannot express except by restructuring the grammar into a precedence
tower.

But snark must stay a **performance drop-in** for tree-sitter. A naive
implementation keeps every declared shift/reduce as a runtime GLR split, which
is exactly the unbounded-split cost tree-sitter refuses to pay.

This note shows that for the *common, motivating* class of such conflicts, the
`prec.dynamic` winner is **provably input-independent** and can be collapsed to a
single static parse-table action **at build time** — no runtime split, tree-sitter
cost, correct parse. Only genuinely input-dependent conflicts pay a bounded,
opt-in runtime split.

## Dynamic-precedence semantics (empirically confirmed)

A parse tree's dynamic precedence is the **sum** of the dynamic precedences of its
productions; among competing full parses of the same span, the **highest total
wins**, ties broken deterministically (first-declared alternative). Confirmed on
tree-sitter reduce/reduce conflicts:

```
x=prec.dynamic(2), y=prec.dynamic(1), input "a"  ->  (x (ident))
x=prec.dynamic(1), y=prec.dynamic(2), input "a"  ->  (y (ident))
x=prec.dynamic(5), y=prec.dynamic(5), input "a"  ->  (x (ident))   # tie -> first
```

## The analysis: merge + fixed frame

A conflict is a state `S` in the LR automaton with ≥2 competing actions (e.g.
shift `t` vs reduce `R`). Follow each competing action forward through the
automaton:

- The branches necessarily consume the **same remaining input** — they are
  competing parses of the same span.
- If the two continuations **re-converge** to a common LR state after a bounded,
  input-independent number of steps (a **merge**), then:
  - everything past the merge is a single **shared sub-parse**, identical in both
    branches, and
  - the branches differ only in a finite **frame** of productions applied between
    `S` and the merge.

Then for *any* input reaching `S`:

```
P(branch₁) − P(branch₂) = Σ dynprec(frame₁) − Σ dynprec(frame₂) = a grammar CONSTANT
```

because the shared tail contributes equally to both and cancels. The sign of that
constant is the winner, for **all** inputs → collapse `S` to the winning action
statically.

If no bounded merge exists, or the frame varies with input, the difference is not
constant → keep `S` as a runtime GLR split (step-budgeted). **The runtime split is
always a correct fallback; the static collapse is a pure perf optimization on top.**

## Decision procedure (decidable, finite)

For each declared-conflict state `S`:

1. Enumerate the competing actions and their continuation states.
2. Bounded search on the (finite) LR automaton for a **merge**: a state both
   continuations reach having consumed the same terminal string, with stack
   suffixes reconciled. Give up after a budget.
3. **Merged with a fixed frame** → extract each branch's frame productions, sum
   their dynamic precedences, pick the max (tie → deterministic tie-break), bake
   that single action into the table cell for `S`. No runtime split.
4. **Otherwise** → emit a multi-action cell (real GLR split) for `S`, resolved at
   runtime by `prec.dynamic`, with a per-branch step budget.

## Correctness

The collapsed action yields the same tree the runtime split's winner would:

1. the shared tail is provably identical in both branches, so both build the same
   sub-parse there;
2. the frame difference is exactly the quantity `prec.dynamic` compares, computed
   to the same constant the runtime would sum;
3. the tie-break rule is identical.

So `collapse(S) ≡ runtime-winner(S)`, input-independently.

## Worked example: filter / call

```js
filter: prec.dynamic(+1, prec.left(seq($._expr, "|", $.ident, optional($.args))))
call:   prec.dynamic(-1, prec.left(seq($._expr, $.args)))
conflicts: [[$.filter, $.call]]
```

Conflict `S`: stack top `_expr | ident`, lookahead `(`.

- **Shift** `(` → continue the filter → production `filter → _expr | ident args`
  (dyn **+1**).
- **Reduce** `filter → _expr | ident` (dyn **+1**); GOTO the `call → _expr · args`
  state; shift → `call → _expr args` (dyn **−1**).
- **Merge**: after consuming `( … )`, both branches have reduced to "an `_expr`
  covering `x | f( … )`" in the same enclosing context → same state; the `args`
  sub-parse is byte-for-byte identical → shared tail.
- **Frame diff** = `dynprec{filter+args}` − `dynprec{filter, call}`
  = `(+1)` − `(+1 + (−1))` = **+1**, constant.
- → collapse `S` to **shift**. `x | f(y)` parses statically as
  `filter(x, f, args(y))`. Correct, and free.

(`optional(args)` is two productions of the same `filter` rule, so both carry
`+1`; they cancel, leaving exactly `−dynprec(call)`.)

## Boundary

Collapse fails → runtime split exactly when the branches do **not** merge with a
fixed frame: the two interpretations diverge into different, input-dependent
sub-structures whose dynamic-precedence totals depend on the input. Those are the
genuinely-runtime conflicts; they are rare and pay a bounded split. Everything
with a shared tail + fixed frame — the trailing-optional-suffix family, and "which
rule wraps a common tail" generally, i.e. the cases that actually bite — collapses
statically.

## Why this keeps snark a tree-sitter perf drop-in

- **No declared conflicts** → table identical to today, zero cost.
- **Declared conflict that collapses** → single static action, identical runtime
  cost to tree-sitter, but the *correct* tree tree-sitter couldn't produce.
- **Declared conflict that doesn't collapse** → bounded, budgeted runtime split —
  the only place snark spends more, and only because the author asked for a
  resolution tree-sitter can't express at all.

## Implementation notes (for the parser core)

Today a shift/reduce in a state is collapsed to one action by prec/assoc at table
build. The change:

1. When the shift rule and reduce rule are in a declared `conflicts` pair, do
   **not** collapse by associativity. Instead run the merge/frame analysis above.
2. If it resolves statically, emit the winning action (this is the common path).
3. If not, emit a multi-action GLR cell (the machinery already used for
   reduce/reduce splits) + attach a step budget.

Ship it incrementally: first the always-correct runtime split (honor declared
shift/reduce conflicts at all), then the static-collapse optimization on top so
the common cases cost nothing. A useful intermediate is a build-time *diagnostic*
that reports, per declared conflict, "statically collapsible to <action>" vs
"needs runtime split", so grammar authors see the cost of what they declared.
