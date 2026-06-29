# Lexer And Scanner ABI

Snark must parse Tree-sitter packages from their grammar semantics: `grammar.json`,
scanner sources, query files, node metadata, and corpus fixtures. Generated
`src/parser.c` is not an input. Scanner source is package input because it is
the author-provided external-token recognizer, but parser table behavior must
come from Snark's own validation, lexical compilation, LR/GLR construction, and
oracle comparisons.

The corrected CSS path is therefore:

1. import package inputs with provenance;
2. validate `grammar.json` into Snark grammar facts;
3. compile terminal, extra, and lexical-mode facts from the validated grammar;
4. build Snark LR/GLR states and actions;
5. derive valid-token and valid-external masks per parser state;
6. run the normal lexer and external scanner ABI from those masks;
7. compare syntax trees and query captures against corpus/highlight oracles.

No step in this path is "parse CSS by hand". A tiny recognizer can only be a
debug probe or quarantined milestone, never the parser runtime.

## Tree-sitter Semantics To Preserve

Tree-sitter lexes on demand from the parser. At any input position, the lexer
does not try every token in the language; it tries only the terminals that are
valid for at least one active parse stack in the current parse state. That
context-sensitive valid-token set is the bridge between LR/GLR actions and
lexing.

Normal lexical conflicts are resolved in this order:

1. tokens valid in the current parser context only;
2. explicit lexical precedence from `token(prec(...))`;
3. longest match;
4. literal string tokens before regex tokens for equal precedence and length;
5. grammar/source order as the final tie-breaker.

Parse precedence is separate. `prec`, `prec.left`, and `prec.right` outside a
`token(...)` wrapper resolve parser conflicts while generating or executing
parse actions. `prec` inside `token(...)` is lexical precedence and must be
stored with the token candidate that participates in lexer competition.

`token(rule)` compiles a terminal expression from strings, regexes, choices,
sequences, and repetitions. It emits one terminal even if the expression is
complex. It must not admit non-terminal rule references into the token body.

`token.immediate(rule)` compiles the same kind of terminal expression but forbids
normal leading extras before the token. It does not mean "ignore extras
forever"; it means this token candidate starts at the current cursor position.
Once the token body is being matched, extras are not interleaved into either
`token` or `token.immediate` bodies.

`extras` are token/rule roots that may be consumed between normal tokens. They
are part of lexical mode construction, not a post-parse cleanup pass. Extras can
include regex/string terminals and rule references such as comments. Snark must
compile them into the same terminal machinery as other tokens and skip them
only at parser locations where the next candidate is not immediate. The reduced
CSS fixture has three extras: whitespace, `comment`, and `js_comment`.

External scanners are consulted before the normal lexer whenever at least one
external token is valid at the current parser position. If the scanner returns a
token, that token feeds the parse action just like a normal terminal. If it
returns false, Tree-sitter can fall back to the normal lexer for literal or regex
external entries; named external symbols that have no grammar token definition
are the scanner's responsibility.

During error recovery, Tree-sitter calls the external scanner with all external
tokens marked valid. Many grammars declare a sentinel external at the end of
`externals` to detect this mode and return false. The pinned CSS scanner does
exactly this with `__error_recovery`.

## Snark Boundaries

`snark/src/grammar.rs` owns raw compatibility DTOs. It should continue to mirror
`grammar.json` shapes without making runtime claims.

`snark/src/tree_sitter.rs` owns package import and provenance. It may import
`src/scanner.c`, `src/scanner.cc`, queries, and corpus fixtures. It must keep
generated parser and metadata files out of the import boundary, including
`src/parser.c` and `src/node-types.json`.

`snark/src/scanner.rs` owns imported scanner source and the raw external-token
table. This table preserves package order for diagnostics and provenance. It is
not the runtime ABI by itself.

`snark/src/validated.rs` owns resolved grammar facts. It already:

- preserves rule order and the first rule as the start rule;
- resolves symbols to `RuleId` or `ExternalTokenId`;
- validates external declarations into `ExternalTokenFact`;
- preserves external ordinals through `ExternalTokenOrdinal`;
- stores extras, conflicts, precedence groups, inline rules, supertypes, word,
  reserved sets, fields, aliases, and visible node kinds;
- rejects unsupported external declarations outside the current symbol/string/
  pattern slice.

`snark/src/lexical.rs` owns grammar-derived lexical and scanner ABI facts. It
already computes:

- external scanner tokens in grammar ordinal order;
- valid-symbol mask width from external token count;
- extra roots from `ValidatedGrammar::extras`;
- `token` versus `token.immediate` roots and leading-extra policy;
- literal and regex terminal facts;
- the host operations a scanner can observe.

That is the right boundary for the next runtime work. The missing pieces are not
new raw DTOs; they are hardened lexical automata, parser-state masks, scanner
execution state, and oracle checks.

## Lexical State Model

Snark should represent lexical state as a value derived from parser state, not
from source language names. A lexical state needs at least:

- the set of valid normal terminal ids for this parser state;
- the set of valid external token ordinals for this parser state;
- whether leading extras may be consumed before each terminal candidate;
- the active reserved-word context, when a `reserved(...)` wrapper applies;
- terminal tie-break metadata: lexical precedence, literal-vs-pattern kind,
  match length, and source/order index;
- an external scanner serialized-state id for each parse stack branch.

For LR parsing, a state usually has one lexical mode. For GLR parsing, multiple
parse stacks may ask for different valid-token sets at the same input byte.
Snark can either lex separately per stack or merge compatible requests into a
combined lexical mode, but the result must still be filtered through each
stack's action table before shifting/reducing.

The normal lexer should produce a candidate token as:

```text
lex(input, byte, lexical_state) -> Option<TokenCandidate>
```

The token candidate should include symbol id, byte range, point range, whether
it came from an extra, precedence metadata, and enough provenance to explain
tie-breaks. It should not directly mutate parser stacks. Parser action
selection consumes the token candidate afterward.

Extras should be consumed by an explicit loop before normal non-immediate
tokens. The loop must prevent zero-width progress. External scanner `advance`
with `skip = true` is similar in spirit but not the same mechanism: it skips
text from the external token range and is controlled by scanner code, not by
grammar `extras`.

## Regex And Token Compilation

Validated expressions should compile into two related arenas:

- grammar productions for parser construction;
- terminal NFAs/DFAs for lexical matching.

String and pattern expressions outside `token(...)` still become terminals.
`token(...)` creates one terminal from the wrapped terminal-only expression.
`token.immediate(...)` creates one terminal with leading extras forbidden.

The token compiler must walk terminal-only subgraphs:

- `StringToken`;
- `PatternToken`;
- `Choice`;
- `Seq`;
- `Repeat`;
- `Repeat1`;
- `Prec` when inside a token, storing lexical precedence;
- wrappers that do not change lexical language, such as `Alias` when the alias
  applies to a terminal.

It should reject or diagnose non-terminal rule references in token bodies unless
that referenced rule has been inlined into a terminal-only graph according to
Tree-sitter-compatible grammar lowering.

Regex compilation should use Tree-sitter-compatible syntax, not host-language
regex behavior by accident. The official Tree-sitter DSL describes regexes as
Rust-regex-like with a supported subset. Snark should compile them into its own
token automata and record unsupported constructs as structured diagnostics.

Tie-breaks belong in the compiled terminal metadata. They should not be
rediscovered ad hoc at scan time.

## External Scanner ABI

For a grammar with `N` external entries, Snark passes a `valid_symbols` mask of
width `N`. Bit/index `i` corresponds exactly to `grammar.externals[i]`, which is
the same order preserved by `ValidatedGrammar::externals()` and
`LexicalFacts::external_tokens()`.

The scanner host must expose Tree-sitter-like operations:

- `lookahead`: current Unicode scalar/codepoint, with EOF checked through `eof`;
- `advance(skip)`: consume the current lookahead and update cursor/points;
- `mark_end()`: remember the accepted token end position;
- `result_symbol`: external ordinal selected by scanner code;
- `eof()`: true end-of-input test, because NUL can be real input;
- `serialize()`: snapshot complete scanner state after a successful token;
- `deserialize(bytes)`: restore scanner state for a parse stack branch.

Tree-sitter also exposes column and included-range helpers. Snark's current
`ScannerHostAbi` slice does not list them. That is fine for the reduced CSS
fixture because its scanner only uses `lookahead`, `advance`, `mark_end`,
`result_symbol`, and `eof`, but the ABI should either add capability facts for
`get_column` and included-range starts or explicitly diagnose scanner sources
that require them before claiming general compatibility.

`advance(skip = false)` includes consumed text in the external token extent
unless a later `mark_end` keeps the accepted end earlier. `advance(skip = true)`
treats consumed text as skipped text outside the emitted token range and is
normally used before the token starts. Calling skip after `mark_end` is a range
hazard and should be traceable in a scanner harness.

If the scanner returns true without calling `mark_end`, the accepted token ends
at the current cursor. If it calls `mark_end`, later lookahead does not extend
the token unless `mark_end` is called again. The parser input position after an
accepted scanner token is the marked end, not necessarily the farthest peeked
cursor.

Serialized scanner state is part of the parse stack. GLR branching, incremental
reuse, and edit recovery require restoring scanner state per stack branch before
calling `scan`. A stateless scanner can serialize zero bytes, as the CSS fixture
does, but Snark must still carry the state identity in runtime data structures.

Scanner execution should be observable. At minimum, trace:

- input byte and point before scanner call;
- valid-symbol mask by external ordinal/name;
- scanner state id before and after;
- every accepted `result_symbol`;
- marked end versus peeked cursor;
- false returns, especially all-valid recovery calls;
- serialized byte length and overflow/diagnostic status.

These traces are the right way to debug scanner behavior. They are also a good
source for reduced oracle artifacts.

## Feeding LR/GLR Actions

Parser generation must assign terminal ids for:

- normal literal and regex terminals;
- complex `token(...)` and `token.immediate(...)` terminals;
- external tokens;
- extras, even when extras do not appear as named parse nodes;
- error/recovery sentinels as grammar-visible external entries when present.

For each parser state, Snark must derive:

- shiftable terminals and their target states;
- reducible productions and lookahead sets;
- valid external ordinals;
- valid normal lexical terminal ids;
- whether external scanning should be attempted before normal lexing;
- parse conflict metadata requiring GLR splits;
- dynamic precedence accumulation for conflict resolution.

Runtime loop sketch:

```text
for each active parse stack at input byte B:
    state = stack.top_state()
    lexical_state = lexical_state_for(state, stack.reserved_context)

    if lexical_state.valid_external_mask.any():
        scanner_result = call_external_scanner(
            stack.scanner_state,
            lexical_state.valid_external_mask,
        )
        if scanner_result.accepted:
            apply_actions(stack, scanner_result.token)
            continue

    token = lex_normal_token(input, B, lexical_state)
    apply_actions(stack, token)
```

`apply_actions` can shift, reduce, split, merge, or enter recovery depending on
the action table. The lexer/scanner layer must not decide parse structure. It
only supplies terminals compatible with the active parser states.

For GLR, an accepted scanner token may be valid for one stack and invalid for
another, because `valid_symbols` was stack-derived. Snark should keep scanner
calls branch-local until it has a proven merge rule that preserves scanner state
and action equivalence.

## CSS Fixture Implications

The pinned reduced CSS package is a useful ABI fixture because it includes:

- `token(prec(...))`, so lexical precedence cannot be ignored;
- multiple `token.immediate(...)` entries, so leading extras must be stateful;
- string and pattern terminals, so terminal tie-breaks matter;
- extras as both pattern and symbol references;
- three external entries in grammar order;
- scanner code that uses `valid_symbols`, skip advances, normal advances,
  `mark_end`, `result_symbol`, EOF, and an error-recovery sentinel;
- highlight queries that depend on real node and token names, not merely parse
  acceptance.

The scanner's enum order is:

1. `DESCENDANT_OP`;
2. `PSEUDO_CLASS_SELECTOR_COLON`;
3. `ERROR_RECOVERY`.

That order must match:

1. `_descendant_operator`;
2. `_pseudo_class_selector_colon`;
3. `__error_recovery`;

from `grammar.json`. Snark should validate and trace this ordinal mapping before
executing scanner code or a scanner translation.

## Current Hardening Needed

Before claiming parser runtime support, harden these areas:

- `validated.rs`: validate terminal-only constraints for `token(...)` and
  `token.immediate(...)`, including whether symbol references inside token
  bodies are legal after inlining.
- `validated.rs`: preserve source/order indexes for terminal expressions so
  lexical tie-breaks have a deterministic final key.
- `validated.rs`: distinguish lexical precedence from parse precedence in the
  typed facts rather than leaving both as generic `GrammarExpr::Prec`.
- `lexical.rs`: compile nested token expressions into terminal automata or
  terminal IR, not only top-level `TerminalFact` records.
- `lexical.rs`: attach lexical precedence, immediate policy, literal-vs-pattern
  kind, regex flags, and source/order key to each terminal candidate.
- `lexical.rs`: model lexical modes from parser states: valid normal terminals,
  valid external mask, extras policy, and reserved-word context.
- `lexical.rs`: prevent zero-width extra loops and zero-width token loops with
  structured diagnostics.
- `lexical.rs`: add scanner ABI capability coverage for `get_column` and
  included-range starts, or diagnose scanners that need them.
- scanner runtime: carry serialized scanner state per parse stack branch and
  define replay/restore behavior before GLR or incremental parsing.
- parser runtime: derive external valid-symbol masks from action tables and
  `ValidatedGrammar::externals()` ordinals, not from scanner source enums.
- oracle lane: parse corpus fixtures into structured expected trees, then
  compare Snark output and query captures against those fixtures.

## Next Implementation Steps

1. Add a terminal compiler under the `lexical` boundary that lowers validated
   string/pattern/token/immediate expressions into terminal IR with lexical
   precedence and tie-break metadata.
2. Add validation diagnostics for non-terminal references in token bodies and
   unsupported regex features.
3. Build parser-state lexical mode facts: normal terminal mask, external ordinal
   mask, extra roots, and immediate-token policy.
4. Implement a scanner host harness for the CSS fixture ABI operations, initially
   as an observable contract layer even before scanner source execution is
   generalized.
5. Parse the CSS corpus files into structured oracle cases and make the first
   runtime comparison target a narrow corpus slice that exercises an external
   scanner token and a `token.immediate` boundary.
6. Add query/highlight oracle comparison only after syntax-tree node and token
   ranges match corpus expectations for that slice.

## Sources

- Tree-sitter Grammar DSL:
  <https://tree-sitter.github.io/tree-sitter/creating-parsers/2-the-grammar-dsl.html>
- Tree-sitter Writing the Grammar, lexical analysis and extras:
  <https://tree-sitter.github.io/tree-sitter/creating-parsers/3-writing-the-grammar.html>
- Tree-sitter External Scanners:
  <https://tree-sitter.github.io/tree-sitter/creating-parsers/4-external-scanners.html>
- Snark current code boundaries:
  `snark/src/validated.rs`, `snark/src/lexical.rs`,
  `snark/src/scanner.rs`, `snark/src/tree_sitter.rs`
- Pinned CSS fixture inputs:
  `snark/tests/fixtures/packages/tree-sitter-css-reduced/src/grammar.json`,
  `snark/tests/fixtures/packages/tree-sitter-css-reduced/src/scanner.c`,
  `snark/tests/fixtures/packages/tree-sitter-css-reduced/queries/highlights.scm`
