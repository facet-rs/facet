# Snark typed-AST codegen capability survey

Part 0 survey for `snark-playground-rebased` after fable's migration in
`6c7a7fd3e0b66113200902b396289f97e03cbf7c`.

The current typed-AST generator exists as a verbatim copy in `vix/build.rs` and
`fable/build.rs`. Their diff is only crate names, file names, grammar paths, and
diagnostic text. The older `gingembre-snark-spike/build.rs` generator is not the
same copy: it uses annotation selectors such as `named:N` and `token`, generates
only the expression AST shape, and does not emit the resolved-CST lowering or an
embedded grammar JSON.

## Ranking

1. Consolidate `vix` and `fable` into one shared generator in `snark-dsl`.
   This has the highest payoff because every capability below otherwise lands
   twice. The shared API should take a grammar path, an annotation file, and
   output stems for the generated AST and embedded grammar JSON.
   Status: landed in `a4f6e0ef4`; vix/fable generated AST and grammar JSON
   were byte-identical before/after consolidation.
2. Auto-box generated type-graph cycles. This removes fable's `_else_body`
   workaround and turns recursive grammar shape into a generator capability.
   Status: landed after consolidation; fable now spells `else if` directly as
   an `if_stmt` field on `else_clause`, and the generated field is
   `Option<Box<IfStmt>>`.
3. Preserve fields on anonymous token steps in the resolved tree. Both vix and
   fable route around this today by scanning anonymous children against
   grammar-derived token sets.
   Status: parked. The current runtime emits `TreeEvent::Field { child: None }`
   for token steps, so the resolved-CST builder has no token identity to attach.
   Fixing this cleanly needs an event-level child identity or a deliberate
   resolved-builder reconstruction pass keyed by parent production structure.
4. Make hidden-rule enum aliases explicit and deterministic. `_expr`,
   `_scrutinee`, and `_call_callee` can share the same Rust enum name today, and
   the emitter relies on the broad declaration being emitted first.
5. Tighten repeat/sepBy field collection tests around the current walker. The
   main `repeat(field(...))` bug is fixed, but repeated anonymous token fields,
   nested fields inside field content, and token/rule mixed fields still have
   sharp generator behavior.
6. Decide whether gingembre should use the same generator API now or stay as a
   separate spike. Folding it in is lower payoff until it needs the fielded
   lowering emitted for vix/fable.

## 1. Struct-cycle infinite types

Current fable workaround:

```js
else_clause: ($) => seq("else", field("body", $._else_body)),
_else_body: ($) => choice($.if_statement, $.block),
```

This forces a hidden enum between `ElseClause` and `IfStmt`, so generated Rust
gets a boxed enum variant on the recursive edge:

```rust
IfStmt -> Option<ElseClause> -> ElseBody::If(Box<IfStmt>)
```

A natural grammar should not need that hidden rule:

```js
if_statement: ($) =>
  seq(
    "if",
    field("condition", $._expr),
    field("then", $.block),
    optional(field("else_clause", $.else_clause)),
  ),

else_clause: ($) =>
  seq("else", field("body", choice($.if_statement, $.block))),
```

Today this shape can be made to work only if the field is expressed as a single
mixed-alternative field and annotated as an ad-hoc enum. A more direct spelling
with separate fields produces an infinite struct cycle:

```js
else_clause: ($) =>
  seq("else", choice(field("if_stmt", $.if_statement), field("block", $.block))),
```

That yields:

```rust
pub struct IfStmt {
    pub else_clause: Option<ElseClause>,
}

pub struct ElseClause {
    pub if_stmt: Option<IfStmt>,
    pub block: Option<Block>,
}
```

Capability target: build the generated Rust type graph and insert `Box<T>` on
cycle back-edges for struct fields, not only enum variant payloads. Then fable
can delete `_else_body`. I did not find an equivalent else-if contortion in
vix. Its `_scrutinee` hidden rule is a grammar-level syntactic restriction for
`match X { ... }`, not a type-cycle workaround.

## 2. `_call_callee`

Current fable workaround:

```js
call_expr: ($) =>
  prec.dynamic(
    1,
    prec(PREC.postfix, seq(field("callee", $._call_callee), field("args", $.arg_list))),
  ),

_call_callee: ($) => choice($.var_ref, $.field_expr, $.index_expr, $.paren_expr),
```

Minimal direct spelling:

```js
call_expr: ($) =>
  prec(PREC.postfix, seq(field("callee", $._expr), field("args", $.arg_list))),
```

This does not appear to be a Rust type-size problem: generated enum variants
already box struct payloads, so `Expr::Call(Box<CallExpr>)` with
`CallExpr { callee: Expr, ... }` is sized. It is also not inherently forbidden
by expression grammars; vix already has left-recursive postfix shapes for field
access and method calls.

The remaining question is parser behavior: the direct spelling may admit
recursive call-as-callee chains (`f()()`) and may change fable's accepted
surface compared to the old hand-written parser. For Part 2, treat this as a
parser/surface audit item, not a confirmed generator limitation. If the fable
oracle accepts the same 57 tests plus explicit call-chain checks with the direct
grammar, `_call_callee` should be deleted. If not, the restriction is a grammar
choice and should stay.

## 3. Anonymous token field loss

Both `vix/build.rs` and `fable/build.rs` document the same snark gap:
`field("op", ...)` and `field("vis", "pub")` on anonymous token steps do not
survive into `ResolvedCstNode` fields. The generated lowering therefore emits
calls to `crate::support::token_field`, which scans direct anonymous children
against the grammar-derived token set.

Minimal reproducer:

```js
source_file: ($) => field("expr", $.binary),
binary: ($) =>
  seq(field("lhs", $.ident), field("op", choice("+", "-")), field("rhs", $.ident)),
ident: () => /[a-z]+/,
```

Expected resolved tree property:

```text
binary
  lhs: ident
  op: "+"
  rhs: ident
```

Current property: `lhs` and `rhs` are fielded, but the anonymous operator token
is present only as an anonymous child, so codegen has to recover it by token-set
scan. This is a snark resolved-tree limitation first; once fixed, the generator
can lower token fields through the same `field_one`/`field_opt` route as named
children.

Precise blocker for Part 2: the runtime currently emits
`TreeEvent::Field { child: None, field, structural_index, .. }` for anonymous
token steps. `ResolvedCstBuilder` ignores that event because the field has no
child `TreeNodeId`; shifted tokens have no `TreeNodeId`, so there is no direct
identity to attach. A robust fix should either extend tree events with token
child identity or teach the resolved builder to reconstruct the fielded token
from the parent production/structural index after direct children have been
materialized. Until then, the shared codegen must keep the token-set scan path.

## 4. repeat()/sepBy() field collection

The core fixed case from `329f5798e` is covered by the copied generator:

```js
source_file: ($) => repeat(field("stmt", $._statement))
```

and by common `sepBy` forms:

```js
function sepBy(sep, rule) {
  return optional(seq(rule, repeat(seq(sep, rule)), optional(sep)));
}

arg_list: ($) => seq("(", sepBy(",", field("arg", $.arg)), ")")
```

The current walker correctly merges the first occurrence with the repeated tail
and produces `Vec<T>` for fable's `arg_list`, vix's `param_list`, tuple fields,
patterns, command parts, and similar forms.

Remaining edge cases worth tests in the shared generator:

```js
// Repeated anonymous token fields are discovered, but lowering panics as unsupported.
ops: ($) => repeat(field("op", choice("+", "-"))),

// Nested fields inside field content are intentionally out of scope in the
// current walker and will panic or be ignored depending on the wrapper.
pair: ($) => field("pair", seq(field("key", $.ident), ":", field("value", $.ident))),

// Token/rule mixed fields are rejected by resolve_shape.
part: ($) => field("part", choice("...", $.splice)),
```

The third form is not theoretical: vix avoids token/rule mixing in command
blocks by making `command_token` a named rule and then using
`choice($.splice, $.command_token)`.

## 5. Hidden enum aliases and subset enums

vix:

```js
_expr: ($) => choice(...),
_scrutinee: ($) => choice(/* subset of _expr */),
```

fable:

```js
_expr: ($) => choice(...),
_call_callee: ($) => choice(/* subset of _expr */),
```

Both annotation files give the subset rule the same enum name as the broad rule:

```js
_expr: { enum: "Expr" },
_scrutinee: { enum: "Expr" },
_call_callee: { enum: "Expr" },
```

The emitter dedupes by Rust enum name and says the first declaration wins.
`RawGrammarJson::rules` preserves Tree-sitter source order, so current grammars
put the broad `_expr` before subset rules and generate the desired enum.

Minimal failure reproducer:

```js
_small: ($) => choice($.ident),
_expr: ($) => choice($.ident, $.call),
call: ($) => seq(field("callee", $._small), field("args", $.args)),
```

with both `_small` and `_expr` annotated as `Expr`, if `_small` is emitted first
the generated `Expr` type misses `Call`. Capability target: group hidden rules
by `enum` name, emit a deterministic union or validated superset, and report a
clear diagnostic when same-name hidden rules are incompatible.

## 6. Per-language patches revealed by the generator copies

`vix/build.rs` and `fable/build.rs` are the same generator with only these
language-specific inputs:

- grammar path: `playgrounds/snark/src/bundled/{vix,fable}/grammar.js`
- annotation path: `{vix,fable}_ast.snark.js`
- output grammar JSON stem: `{vix,fable}_grammar.json`
- output AST stem: `{vix,fable}_ast.rs`
- generated header and panic text

That is strong evidence that the capability belongs in `snark-dsl`, not in the
language crates.

The language-specific support modules in `vix/src/lib.rs` and `fable/src/lib.rs`
are also duplicated in spirit (`Span`, `Spanned`, `field_one`, `field_opt`,
`token_field`, string/path/bool decode). They are outside Part 1 unless the
shared generator needs a stable support trait/module contract; moving them is a
separate runtime API decision.

## 7. cstree-era no-downgrade audit

The fable migration removed the lossless cstree layer. The old README promised
that the CST preserved every byte, including whitespace, comments, and trivia,
for round-trip tooling. The new README no longer promises lossless round-trip.

Part 3 verified and tested:

- malformed fable input returns `Err(ParseError)` at the public parse boundary
  and does not enter generated lowering;
- the diagnostic type is structured as `ParseError`, but its payload is still a
  single message string, either `no accepted parse` or formatted parser failure
  text;
- spans survive in the generated typed AST for the source file, statement
  structs, and decoded leaf values;
- lossless round-trip and recovery granularity are currently absent from the
  snark path.

Regression tests in `fable/src/lib.rs`:

```rust
malformed_inputs_return_parse_errors
generated_ast_preserves_statement_and_leaf_spans
```

The malformed-input test covers assignment-with-missing-RHS, unclosed `if`
block, and unclosed index expression. The span test parses `let answer = 42;`
and checks the root span, `LetStmt` span, identifier span, and integer literal
span against byte offsets in the original source.

No-downgrade findings:

- structured non-panic failure is preserved at the public fable parse API;
- generated typed AST spans are present at the node and leaf levels tested;
- full cstree parity is not present: callers no longer get a recovered lossless
  CST with trivia, round-trip bytes, and an `errors()` collection after a
  malformed parse;
- diagnostic granularity is below the cstree era because `ParseError` carries
  only one message string and no source range, expected-token set, or recovery
  tree.

## Parked questions

- Should the shared generator also own the `support` lowering helpers, or should
  language crates continue to provide a small support module with a fixed
  function contract?
- Should same-name hidden enum aliases be unioned automatically, or should the
  annotation DSL make subset/superset intent explicit?
- Is direct `$._expr` for fable call callees accepted by the parser without
  broadening the language beyond the cstree-era surface?
- Does gingembre need fielded resolved-CST lowering soon, or should its selector
  based spike remain separate until the shared vix/fable generator is stable?
