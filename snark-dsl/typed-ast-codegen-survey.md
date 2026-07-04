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
2. Auto-box generated type-graph cycles. This removes fable's `_else_body`
   workaround and turns recursive grammar shape into a generator capability.
3. Preserve fields on anonymous token steps in the resolved tree. Both vix and
   fable route around this today by scanning anonymous children against
   grammar-derived token sets.
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

Tractable Part 2 shape: preserve anonymous-token field metadata in the resolved
builder, add a parser/resolved-tree regression test in `snark`, then remove the
token-set scan branch from the shared codegen only when vix and fable generated
ASTs stay semantically equivalent.

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

## 7. cstree-era no-downgrade audit targets

The fable migration removed the lossless cstree layer. The old README promised
that the CST preserved every byte, including whitespace, comments, and trivia,
for round-trip tooling. The new README no longer promises lossless round-trip.

Part 3 should verify and test:

- malformed fable input returns `Err(ParseError)` and never panics through
  generated lowering;
- parse diagnostics remain structured enough for callers, not only a formatted
  string;
- spans survive in generated AST structs and leaf values where the old typed
  layer exposed syntax nodes/tokens;
- lossless round-trip and recovery granularity are either restored or documented
  as intentionally absent from the snark path.

Minimal malformed-input tests:

```rust
assert!(fable::parse("root.age = ; root.ok = true").is_err());
assert!(fable::parse("if root.age { root.ok = true").is_err());
assert!(fable::parse("root.items[").is_err());
```

The old parser recovered and kept a CST with `errors()`. The new API returns a
single `ParseError { message }`, so preserving non-panic behavior is the floor,
but it is not full cstree parity.

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
