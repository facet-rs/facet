# Weavy Lexer Lowering

Snark's lexer endpoint is not a loop over Rust regex calls. The current
`CompiledLexMode` path is a correctness-preserving interpreter and an oracle
for the lowered form, but the lowered executor should carry a lexer program
owned by Snark and executed by Weavy.

## Boundary

Snark owns the Tree-sitter lexical semantics:

- lexical modes selected by parser states;
- terminal validity from the current parse state;
- literal, pattern, `TOKEN`, `IMMEDIATE_TOKEN`, extras, reserved-word, and word
  token behavior;
- lexical precedence, implicit precedence, and literal-over-pattern ordering;
- byte and inspected-byte ranges for incremental reuse safety;
- declarative scanner replacements such as `until`, `nested`, `auto_close`, and
  later layout/ASI-style implicit insertion.

Weavy owns only the carrier:

- compact op blocks;
- typed scratch/cache storage;
- neutral control flow;
- later copy-and-patch/JIT emission for the same Snark lexer operations.

Regex syntax, Tree-sitter token semantics, parser lookahead, and scanner
valid-symbol masks do not become Weavy concepts.

## Lowered Shape

The generator should lower each lex mode into one lexer graph:

- string terminals compile into a shared literal trie or multi-string matcher;
- pattern terminals compile into a merged automaton for that lex mode;
- composed lexical expressions compile into graph nodes for `seq`, `choice`,
  `repeat`, `repeat1`, and token roots;
- `until` compiles into a first-marker scan over its marker set;
- `nested` compiles into a counting delimiter op;
- `auto_close` compiles into a declared-table-driven implicit-token op;
- external scanners remain explicit Snark host calls with scanner snapshots.

Executing a lex mode is then one pass through the graph at the current byte
position. The result is the same candidate set the interpreter would have found:
terminal id, accepted end byte, inspected end byte, lexical precedence, literal
flag, immediate flag, and scanner snapshot effect.

The parser state still filters valid terminals and externals. This preserves the
important Tree-sitter property that JavaScript-style ambiguities such as
regex-vs-divide are lexer-mode/lookahead questions, not global regex questions.

## Interpreter Oracle

The current Rust matcher stays useful as the differential oracle:

- for each generated lex mode and test byte position, run the interpreted
  matcher;
- run the lowered Weavy lexer graph;
- compare candidate terminal ids, end bytes, inspected end bytes, precedence
  fields, and selected winner;
- include extras, reserved words, direct pattern terminals, composed token
  roots, and declarative primitives in the corpus.

This lets the lowered lexer move incrementally without weakening correctness:
the first compiled graph can cover literals and direct pattern sets, then expand
to composed token expressions and declarative scanner primitives.

## Current Bridge

Direct pattern sets currently use `regex-automata` with caller-provided match
scratch, and the common path uses a hybrid DFA to report match end offsets
directly. That removes both the old per-token `regex::RegexSet::matches`
allocation and the follow-up leaf rematch for direct pattern terminals when the
hybrid DFA can be built. This is still a bridge, not the architecture endpoint:
the direction above remains merged lex-mode automata lowered into Snark's Weavy
program, with the Rust matcher kept as the oracle.

## JIT Path

The JIT should consume the same lowered lexer graph:

- literal tries become direct byte comparisons or table dispatch;
- pattern automata become compact transition tables;
- `until` becomes a multi-marker search loop;
- `nested` becomes a delimiter scan with a depth counter;
- `auto_close` becomes a table lookup against the Snark-maintained tag stack.

No second lexer semantics should appear in the JIT. Native code is a
specialization of the lowered Weavy lexer program, not a separate parser.
