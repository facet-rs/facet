# Tree-sitter LR/GLR Parser Shape

This note fixes the parser architecture Snark must implement. Snark is not a
recursive recognizer, not a PEG, not a green-slice adapter, and not a generated
`src/parser.c` importer. The parser must be Snark's own Tree-sitter-compatible
table machine, derived from grammar semantics and checked against Tree-sitter's
public oracle surface.

Generated grammar package implementation files, especially `src/parser.c`, are
not inputs, not references, and not oracles. Upstream Tree-sitter docs,
generator sources, and runtime sources are valid architecture references because
they describe the algorithm and data model rather than a generated language
parser.

## Upstream Shape

Tree-sitter grammars are written to produce concrete syntax trees and to stay
close to LR(1). Tree-sitter uses a GLR parser when declared or unresolved
runtime ambiguity remains, but the ordinary fast path is an LR table machine:
current parse state plus lookahead symbol selects actions from a parse table.

The grammar DSL has two distinct precedence planes:

- Parse precedence and associativity resolve LR conflicts while building parse
  table actions.
- Lexical precedence participates in token choice before parse actions run.
- Dynamic precedence is accumulated at runtime for genuine GLR ambiguity and
  participates in final tree selection.

Tree-sitter lexing is context-aware. The parser state selects a lexical mode,
and that mode recognizes only terminals valid from that state, plus applicable
reserved-word and keyword behavior. External scanners are called from the lexing
path with a state-specific valid-symbol mask and must round-trip their serialized
state for ambiguity and incremental reparsing.

Tree-sitter corpus files are the API-level parse oracle: each case contains
source input and an expected S-expression tree. Query and highlight fixtures are
separate oracles over captures and byte/point ranges. Snark must compare against
those outputs, not against generated parser code.

## Algorithm Contract

The runtime loop is table driven:

1. Read the active stack head's parse state.
2. Get or reuse a lookahead token according to that state's lexical mode.
3. Look up the table entry for `(state, lookahead_symbol)`.
4. Execute every action in that entry in order.
5. Shift updates the current stack version and consumes the lookahead.
6. Reduce pops a production length, builds a parent node, computes goto from the
   exposed state and reduced nonterminal, and may create additional stack
   versions.
7. Accept selects a final root.
8. Recover enters the generated recovery path.
9. After each round, condense stack versions by score and merge compatibility.

`ParseAction` is the central runtime vocabulary: `Accept`, `Shift`,
`ShiftExtra`, `Recover`, and `Reduce { symbol, child_count,
dynamic_precedence, production_id }`. `ParseState` owns terminal entries,
nonterminal goto entries, reserved-word context, main lex state, external lex
state, and core identity. `LexTable` owns NFA/DFA-like lexical states and
accepted terminals. Snark should model these directly instead of hiding them
inside recursive Rust control flow.

## Grammar-Derived Tables

Snark must build these facts from `grammar.json` after validation:

- Symbols: terminals, external terminals, nonterminals, aliases, fields,
  supertypes, visible/hidden/public symbols, keyword/word token, EOF, error, and
  auxiliary repeat symbols.
- Productions: flattened and inlined rule productions, ordered steps, hidden
  inherited fields, alias sequences, production ids, static precedence,
  associativity, dynamic precedence, and reserved-word context.
- LR item sets: closure/goto item sets with lookahead sets and reserved-word
  lookaheads.
- Parse states: terminal action rows, nonterminal goto rows, `ShiftExtra` rows
  for extras, recovery rows, lex-state ids, external-lex-state ids, and
  minimization metadata.
- Conflict metadata: declared conflict groups, actual conflicts encountered
  while building tables, precedence/associativity resolutions, unresolved
  conflict diagnostics, and GLR action rows that intentionally keep multiple
  actions.
- Lexical tables: terminal regex/literal automata, token and immediate-token
  roots, lexical precedence, longest-match behavior, string-over-regex
  specificity, grammar order tie breaks, keyword extraction, reserved word sets,
  and merged lexical modes.
- External scanner tables: external token ordinal map, external valid-symbol
  masks per parse state, scanner symbol map, scanner state serialization lanes,
  and whether the scanner used column-sensitive host operations.
- Tree emission plan: visible node behavior, anonymous token behavior, hidden
  node elision, supertypes, fields, aliases, extras, missing nodes, error nodes,
  byte ranges, point ranges, lookahead bytes, and reusable-node metadata.

None of these may be recovered by reading generated implementation files. If a
fact is missing from Snark's current `grammar`, `validated`, or `lexical`
layers, add the typed semantic fact there and derive it from raw grammar input.

## GLR Stack Semantics

GLR in Tree-sitter is not "try another recursive branch". It is a graph-stack
runtime over the same LR table:

- A table entry can contain multiple actions for the same lookahead.
- Reductions can create additional stack versions.
- Shift usually advances the current version and ends action processing for that
  lookahead.
- Stack versions are periodically ranked, reordered, merged, or removed.
- Merge is only valid when stack heads are active, have the same parse state,
  same byte position, same accumulated error cost, and equivalent last external
  scanner state.
- A reduction over a graph stack may have multiple pop paths. The runtime builds
  candidate parent nodes for those paths and keeps the preferred children by the
  same tree-selection rules used for final roots.

Snark needs a graph stack model, not a `Vec<Frame>` pretending ambiguity does
not exist. Recommended types:

- `parser_table::ParseTable`, `ParseState`, `ParseTableEntry`, `ParseAction`,
  `GotoAction`, `ProductionInfo`, `LexModeId`.
- `runtime::StackVersionId`, `GraphStackNodeId`, `GraphStackLinkId`,
  `StackHead`, `StackMergeKey`, `BranchScore`.
- `runtime::ReductionCandidate`, `ReductionPath`, `ReducedNodePlan`.
- `runtime::TreeSelection { error_cost, dynamic_precedence, structural_order }`.

Snark's parser IR now owns the typed table ids and grammar facts; Weavy consumes
those facts as the execution carrier rather than mirroring a second native
runtime.

## Conflict Handling

Conflict handling has two phases.

At table-generation time, Snark must eagerly resolve conflicts where the grammar
semantics permit it:

- Higher static precedence wins.
- Equal precedence consults associativity.
- Left associativity prefers the earlier-ending reduce.
- Right associativity prefers the later-ending shift.
- Named precedence groups must be ordered according to `precedences`.
- Unresolved conflicts that are not declared in `conflicts` are generator
  diagnostics, not runtime experiments.

At runtime, declared conflicts keep multiple actions in the table. Snark must
execute those actions through GLR stack splitting/merging, accumulate dynamic
precedence on reduced nodes, and select among surviving trees by error cost,
dynamic precedence, then deterministic structural order.

Do not flatten conflicts into "first rule wins". Do not emulate conflict
resolution by grammar-order recursion. Do not treat dynamic precedence as a
static table-generation value.

## Lookahead And Lexing

Lookahead is a parser-state-dependent token, not a global lexer token stream.
Snark's lexer must be callable with a lexical mode selected by the active parse
state.

Required behavior:

- Internal lexing recognizes only terminals enabled by that state.
- External scanner runs before the internal lexer when the state has an external
  lex state.
- External scanner input is a valid-symbol mask indexed by grammar external
  ordinal, not by Snark's internal terminal ids.
- External scanner `deserialize` is called before scan with the stack head's
  last external token state, and `serialize` is captured on success.
- Empty external tokens are allowed only when they advance scanner state or are
  otherwise safe; they must not create infinite loops through extra tokens or
  recovery.
- `token.immediate` forbids leading extras.
- `extras` are parser actions, often `ShiftExtra`, not hidden global whitespace
  skipping.
- The `word` token and reserved-word context can rewrite a keyword result back
  to the word token when a keyword is not reserved in the current state.
- Lexical precedence, longest match, string specificity, and grammar order are
  part of token selection.

Recommended modules:

- Extend `lexical` from raw facts into executable lexical modes and token
  automata.
- Add `scanner_host` for the external scanner ABI and traceable scanner calls.
- Keep scanner source import in `scanner`; do not mix source loading with
  runtime scanner execution.

## Reductions And Tree Building

Reduction is where parse-table semantics become tree semantics. Snark must
preserve:

- Production id and child count.
- Reduced nonterminal symbol.
- Goto state computed from the state exposed after popping and the reduced
  symbol.
- Alias sequence and inherited field map from production info.
- Dynamic precedence accumulated onto the reduced subtree.
- Extra-token handling around reductions.
- Hidden, inline, anonymous, visible, named, supertype, error, and missing node
  behavior.
- Byte extent, point extent, padding, and lookahead bytes.
- Fragility/reuse metadata for incremental parsing.

Tree output should be structured events first. Rendered S-expressions are a
view over the tree sink and compare against `corpus::SexpNode`, not a string
assembled during parsing.

Recommended modules:

- `tree_plan`: grammar-derived node/field/alias/emission plan.
- `tree_sink`: structured `Open`, `Token`, `Close`, `Field`, `Alias`, `Error`,
  `Missing`, and `Reuse` events.
- `oracle`: normalization from structured tree/query events to corpus and
  highlight fixture assertions.

## Error Recovery

Error recovery is part of the parser algorithm, not a final fallback. The
runtime should:

- Pause stack versions that cannot act on the current lookahead.
- If all useful versions pause, perform reductions that might have been enabled
  before invalid input.
- Try missing-token insertion where shifting a missing terminal enables a
  reduction for the current lookahead.
- Push an error-state discontinuity and record a bounded stack summary.
- Recover to an earlier stack state where the current lookahead is valid by
  wrapping intervening subtrees in an `ERROR` node.
- Also consider skipping/wrapping the current lookahead as an `ERROR` node.
- Score alternatives with skipped-tree, skipped-character, skipped-line, and
  missing-token costs.
- Preserve external scanner state constraints; do not pursue a recovery branch
  that invalidly crosses scanner state changes.

Snark should model recovery costs and strategies explicitly:

- `recovery::ErrorCost`
- `recovery::StackSummary`
- `recovery::RecoveryStrategy::{MissingToken, RecoverToPreviousState,
  SkipLookahead, RecoverAtEof}`
- `TraceEvent::Recover` with chosen strategy, rejected strategy reasons, emitted
  tree events, and score deltas.

The oracle must include error and missing node shape where corpus fixtures
expect it.

## Reuse And Incremental Parsing

Incremental parsing is not required for the first parser table to exist, but the
table and tree data must leave room for it from the beginning.

Reusable concepts:

- Old tree cursor over reusable nodes.
- Included range differences.
- Node changes, error nodes, missing nodes, and fragile nodes as reuse blockers.
- First-leaf compatibility: current lexical mode, old leaf lexical mode,
  external scanner state, zero-width token safety, and table-entry reusability.
- Token cache keyed by byte position and last external scanner state.
- Reused subtree breakdown when a later lookahead proves reuse invalid.
- Final changed-range and final tree equivalence oracle.

Recommended modules:

- `incremental`: old-tree cursor, included-range diff, reuse candidates, and
  changed-range outputs.
- `runtime_input`: keep byte/point/range/edit coordinates here; do not bury them
  in parser tables.
- `trace`: reusable-node accept/reject events with mechanical reasons.

## Snark Module Plan

Existing modules should keep their boundaries:

- `grammar`: raw `grammar.json` compatibility DTOs.
- `validated`: resolved grammar semantics. Add missing semantic ids here, not in
  runtime code.
- `lexical`: lexical facts, then executable lexical automata and lexical modes.
- `scanner`: imported scanner sources and external token declarations.
- `query`: imported query sources and later compiled query facts.
- `corpus`: parsed parse/highlight fixtures and expected oracle values.
- `runtime_input`: source coordinates, included ranges, and edits.
- `lower::weavy`: lowering carrier ids and Snark intrinsics after the parser
  machine exists as Snark data.

New modules should be named for final parser concepts:

- `parser_table`: grammar-derived LR/GLR tables.
- `parser_gen`: construction of item sets, conflict handling, lexical modes, and
  parse tables from `ValidatedGrammar` plus `LexicalFacts`.
- `weavy_runtime`: Snark-owned Weavy execution state for stack versions,
  lookahead, recovery, tree sinks, scanner snapshots, and reuse.
- `tree_plan` and `tree_sink`: tree semantics and structured events.
- `scanner_host`: executable external scanner ABI.
- `recovery`: error scoring and strategy data.
- `incremental`: reusable old-tree logic.
- `oracle`: corpus/query/highlight comparison.
- `trace`: facet-serializable event enum for parser/scanner/tree/query
  observability.

If code scaffolding is added later, use these final names and final data shapes.
Do not add a "temporary parser" whose API cannot evolve into this table
machine.

## Required Oracles

Snark's correctness surface is observable Tree-sitter behavior:

- Parse corpus S-expressions from `test/corpus`.
- Query and highlight assertions from query/highlight fixtures.
- Scanner traces: valid-symbol masks, accepted external symbols, mark-end spans,
  EOF behavior, column access, and serialized state replay.
- Error recovery fixtures: `ERROR` and `MISSING` node placement plus final tree
  selection.
- Incremental fixtures: changed ranges, reused subtree decisions, and final
  tree equivalence.

Use structured values for comparisons. `rediff` is appropriate for comparing
Snark events to parsed corpus/query/highlight facts. Snapshots are acceptable
only after the structured event schema is stable.

## Anti-Patterns

These paths are rejected:

- Reading or translating generated grammar package `src/parser.c`.
- Extracting parse tables from generated C.
- Calling generated parser code and wrapping its tree as Snark output.
- Building a handwritten PEG, Pratt parser, recursive descent parser, or
  CSS-only recognizer.
- Adding side parser implementations outside the validated grammar to Weavy
  execution path.
- Treating grammar rule order as runtime branch order except where Tree-sitter
  lexical tie-breaking explicitly uses grammar order.
- Lexing a whole token stream before parsing.
- Ignoring external scanner serialization or valid-symbol masks.
- Collapsing GLR ambiguity to one branch before the table says it is resolved.
- Rendering S-expressions during parse actions instead of emitting structured
  tree events.
- Adding Weavy parser semantics because the Snark table/runtime layer is
  missing.

## References

Upstream docs:

- `/Users/amos/oss/tree-sitter/docs/src/creating-parsers/2-the-grammar-dsl.md`
- `/Users/amos/oss/tree-sitter/docs/src/creating-parsers/3-writing-the-grammar.md`
- `/Users/amos/oss/tree-sitter/docs/src/creating-parsers/4-external-scanners.md`
- `/Users/amos/oss/tree-sitter/docs/src/creating-parsers/5-writing-tests.md`
- https://tree-sitter.github.io/tree-sitter/creating-parsers/2-the-grammar-dsl.html
- https://tree-sitter.github.io/tree-sitter/creating-parsers/3-writing-the-grammar.html

Upstream source references used for architecture, not generated parser
artifacts:

- `/Users/amos/oss/tree-sitter/crates/generate/src/tables.rs`
- `/Users/amos/oss/tree-sitter/crates/generate/src/build_tables/build_parse_table.rs`
- `/Users/amos/oss/tree-sitter/crates/generate/src/build_tables/build_lex_table.rs`
- `/Users/amos/oss/tree-sitter/lib/src/parser.c`
- `/Users/amos/oss/tree-sitter/lib/src/stack.c`
- `/Users/amos/oss/tree-sitter/lib/src/language.c`

Snark-local companion docs:

- `snark/docs/methodology.md`
- `snark/docs/architecture/weavy-parser-lowering.md`
