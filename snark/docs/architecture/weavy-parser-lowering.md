# Snark Weavy Parser Lowering

This document fixes the target architecture for Snark's parser lowering. Snark
does not inspect generated Tree-sitter implementation files and does not treat
generated parser code as an oracle. The parser is Snark's own
Tree-sitter-compatible LR/GLR machine, built from validated `grammar.json`,
scanner, query, corpus, and runtime-input contracts, then carried by Weavy
programs.

Weavy is the lowering and execution carrier. Snark owns parser semantics.

## Boundary

Snark owns:

- Tree-sitter package import and provenance.
- Raw compatibility DTOs for `grammar.json`, scanner sources, queries, corpus
  fixtures, and highlight fixtures.
- Validation into stable Snark grammar facts.
- Parser generation from validated grammar facts.
- External scanner host ABI and scanner state replay.
- Parse stack, GLR graph stack, tree construction, error recovery, incremental
  edit handling, query execution, trace events, and oracle comparison.

Weavy owns:

- `Lowered<BlockId, Op>` and dense block tables.
- `Control` and `Step` execution with an explicit runner stack.
- Canonical `WeavyOp`: `Control`, `Memory`, `Init`, `Aggregate`, and
  domain-specific `Intrinsic` payloads.
- `IntrinsicOp` descriptors and effect contracts.
- Neutral typed-memory runtime helpers such as raw scratch and raw array buffers.
- Analysis helpers for op shape, effect shape, intrinsic counts, and block
  structure.

Weavy must not gain concepts such as LR states, Tree-sitter symbols, scanner
valid-symbol masks, parse stacks, reusable parse trees, query captures, or
S-expression output. Those are Snark concepts represented as Snark intrinsics
and Snark runtime state.

## Parser Generator Outputs

The parser generator consumes `validated::ValidatedGrammar` plus lexical facts
from `lexical::LexicalFacts`. Its output is a typed Snark parser machine:

- Symbol table: dense terminal, nonterminal, external, field, alias, and visible
  node ids with provenance back to validated grammar facts.
- Production table: left-hand nonterminal, right-hand symbol sequence, field
  bindings, alias sequence, dynamic precedence, static precedence, associativity,
  visibility, supertype, and inline behavior.
- Lexical table: literal terminals, regex terminals, token roots,
  immediate-token roots, extras, reserved-word contexts, lexical modes, and the
  word token.
- Scanner table: external token ordinal mapping, per-state valid-symbol masks,
  serialized scanner-state lanes, EOF behavior, mark-end semantics, and accepted
  external token result symbols.
- LR tables: parse states, shifts, reductions, accepts, recoveries, and lexical
  mode selection.
- GLR tables: conflict groups, split points, merge criteria, dynamic precedence
  accumulation, graph-stack node identities, and deterministic tie-breaking.
- Tree plan: visible node emission, anonymous token emission when required by
  the oracle, field edge emission, alias application, error node emission,
  missing-node emission, byte and point range propagation, and incremental reuse
  metadata.
- Query plan: compiled pattern graph for highlights, locals, injections, tags,
  predicates, capture names, capture quantifiers, and capture ordering.
- Trace schema: structured parser/scanner/tree/query events used for debugging
  and oracle comparison.

The generator output must be stored as Snark data. A Weavy program references
that data by ids; it does not encode parser semantics into Weavy itself.

## Snark Parser Intrinsics

`snark/src/lower/weavy.rs` already defines the beginning of the Snark dialect:
`SnarkWeavyLowered`, `SnarkWeavyOp`, `SnarkBlockId`, and `SnarkIntrinsic`.
The final dialect should expand that scaffold without moving semantics into
Weavy.

The executable intrinsic families should be:

- Input and lexing: select lexical mode, skip extras, lex literal, lex pattern,
  handle immediate tokens, recognize EOF, and preserve byte/point spans.
- External scanner: build valid-symbol masks from the current parser state, call
  the Snark scanner host ABI, expose lookahead/advance/mark-end/result-symbol
  operations to scanner code, serialize/deserialize scanner state, and emit
  scanner trace events.
- LR control: inspect lookahead, dispatch shift/reduce/accept/recover actions,
  enter generated state blocks, and execute table-driven action rows.
- GLR control: split stacks, enqueue deferred reductions, merge compatible
  graph-stack heads, rank alternatives by precedence/dynamic precedence, and
  retire losing branches with traceable reasons.
- Stack operations: push parser states, pop reduction handles, attach semantic
  values, preserve scanner state at stack heads, and expose graph-stack ids.
- Tree operations: open node, shift token, reduce node, attach field, apply
  alias, emit missing node, emit error node, finish node, and reuse old subtree.
- Query operations: run compiled query bytecode over produced trees, emit
  captures, evaluate predicates, and emit injection/tag/local/highlight events.
- Oracle operations: normalize tree events to corpus S-expressions and normalize
  query captures to highlight assertions.

Each intrinsic has a stable `IntrinsicDescriptor` in the
`snark.tree_sitter` dialect and a conservative `EffectContract`. Parser actions
that advance source input, mutate scanner state, mutate parser stack state, or
write tree/query sinks must remain ordered. External scanner calls are barriers
because they call language-owned scanner code.

## Blocks And Control Flow

The Weavy block table should mirror Snark-generated parser structure, not
grammar syntax recursion.

Required block families:

- Root parse entry: initialize runtime state, select the start state, and enter
  the state dispatcher.
- State blocks: one block per Snark-generated parser state or compacted action row.
  A state block selects lexical mode, gets lookahead, dispatches action, and
  loops through `ControlOp::CallBlock` or `ControlOp::Return`.
- Reduction blocks: one block per production or shared reduction shape. These
  pop stack handles, build tree events, run aliases/fields, push goto states,
  and return to the dispatcher.
- Recovery blocks: state-specific recovery actions that produce error/missing
  tree events and advance input only through explicit Snark intrinsics.
- Scanner blocks: wrappers around external scanner invocation and scanner-state
  persistence. These are Snark blocks carrying scanner ABI semantics, not Weavy
  primitives.
- Lexer blocks: generated lex-mode programs that merge literal and pattern
  terminals, execute composed token expressions, and emit the same candidates as
  the interpreter oracle. See `weavy-lexer-lowering.md`.
- GLR worklist blocks: split, merge, deferred reduction, branch retirement, and
  deterministic winner selection.
- Query blocks: query-pattern entry blocks and shared predicate/capture emit
  blocks.

Use `ControlOp::CallBlock` for Snark-generated parser jumps and reusable state
fragments. Use `Control::CallBlockThen` from the Snark stepper when the runtime
must resume with a Snark continuation after a child block returns. Do not encode
grammar recursion as Rust recursion or as recursive descent in a Snark stepper.

## Runtime State And Sinks

The Snark stepper is the owner of runtime parser state. Its state bundle should
be explicit and observable:

- Source input: bytes, byte offsets, UTF-8 point tracking, included ranges, and
  incremental edit metadata from `runtime_input`.
- Lookahead cache: selected terminal/external token, accepted byte range,
  lexical mode, and scanner state snapshot.
- LR stack: parser state, symbol, tree handle, field/alias metadata, byte/point
  ranges, and scanner state at the stack head.
- GLR graph stack: graph-stack nodes, packed parse alternatives, branch score,
  dynamic precedence, and merge keys.
- Tree sink: structured events for node/token/error/missing/reuse with fields,
  aliases, namedness, byte ranges, and point ranges.
- Scanner sink: valid-symbol mask, host ABI operations, accepted external
  symbol, mark-end byte, serialized state, and failure reason.
- Query sink: capture name, capture id, byte/point range, pattern id, predicate
  outcome, and fixture category.
- Trace sink: every parser/scanner/tree/query event in a facet-serializable
  top-level event enum.
- Oracle sink: corpus-normalized S-expression output and highlight/query assertion
  output derived from the structured sinks.

The sinks are not strings first. Rendered S-expressions and highlight assertion
text are views over structured events, so S-expression output can be compared with
`corpus::SexpNode` and `corpus::HighlightAssertion`.

## Trace And Oracle Events

Trace events should be designed before the first executable parser slice. The
minimum event enum should cover:

- `ParseStart`: language, start rule, input length, included ranges, and edit
  summary.
- `StateEnter`: parser or GLR branch id, state id, byte/point cursor, and stack
  depth.
- `Lex`: lexical mode, candidate terminal, accepted terminal, byte/point range,
  and extras consumed.
- `ExternalScannerCall`: state id, valid-symbol mask, scanner-state id,
  operation counts, accepted symbol, mark-end, EOF, serialized state, and error.
- `Shift`: branch id, terminal, target state, token range, and scanner snapshot.
- `Reduce`: production id, pop count, result symbol, fields, aliases, dynamic
  precedence, and goto state.
- `GlrSplit`: source branch, new branches, conflict id, and action set.
- `GlrMerge`: merged branches, merge key, retained branch, and discarded
  alternatives.
- `Recover`: state id, recovery strategy, emitted error/missing node, and input
  advance.
- `TreeEvent`: open, token, close, field, alias, missing, error, and reuse
  events with ranges.
- `QueryCapture`: query kind, pattern id, capture name, capture range, and
  predicate result.
- `ParseFinish`: accepted/recovered/failed status, final tree id, error count,
  branch count, and normalized oracle outputs.

Oracle comparison is against Tree-sitter's public test surface:

- Parse corpus cases compare normalized structured tree output to
  `corpus::SexpNode`/`SexpValue`, including fields, visible nodes, anonymous
  terminals required by expected output, missing nodes, and error nodes.
- Highlight fixtures compare structured query captures to
  `corpus::HighlightAssertion`, using byte columns and fixture order.
- Query outputs compare capture names, ranges, predicate behavior, injections,
  locals, tags, and highlight categories.
- Scanner behavior compares valid-symbol masks, accepted external token
  ordinals, mark-end spans, EOF behavior, and serialized state replay.
- Incremental parsing compares changed ranges, reused subtree events, and final
  tree equivalence.

## Keeping Weavy Neutral

When a parser feature needs more carrier support, first express it as a Snark
intrinsic with an effect contract. Only move a concept into Weavy when at least
two non-parser frontends need the same abstraction and it can be named without
Tree-sitter or parsing vocabulary.

Allowed Weavy-facing additions:

- More precise effect contracts for ordered resources, sink writes, and opaque
  barriers.
- Generic analysis of intrinsic descriptors and block graphs.
- Generic runner instrumentation such as step counts and frame depth.
- Neutral raw memory helpers with caller-supplied drop/adoption callbacks.
- Copy-and-patch host-call support for consumer-owned intrinsics.

Rejected Weavy-facing additions:

- Parser states, symbols, productions, fields, aliases, scanners, query
  captures, parse trees, S-expressions, recovery, or GLR terms.
- A Tree-sitter scanner ABI in Weavy.
- A parser-specific stack or tree builder in Weavy.
- Any oracle renderer or query-highlighting logic in Weavy.

The stable phrase for the architecture is: Weavy carries and schedules the
machine; Snark defines and executes parser meaning.

## Forced Implementation Order

1. Freeze the data boundary: keep `grammar`, `validated`, `lexical`,
   `scanner`, `query`, `corpus`, and `runtime_input` as the source of parser
   facts; add missing typed ids only where the generator needs them.
2. Define the trace/oracle event enum and sinks before executing parser actions.
   The first tests should compare structured events to fixture-derived expected
   values, not rendered ad hoc text.
3. Build parser-generator tables from `ValidatedGrammar` and `LexicalFacts`:
   productions, terminals, lexical modes, precedence/conflict facts, LR actions,
   GLR conflict metadata, and tree emission plans.
4. Lower generated tables into `SnarkWeavyLowered` blocks: root, state,
   lex-mode, reduction, recovery, scanner, GLR worklist, and query blocks. This
   is still data construction; no parser semantics move into Weavy.
5. Implement the Snark `Step` runtime for the existing `SnarkIntrinsic`
   families, with explicit parser stack, GLR graph stack, scanner state, tree
   sink, query sink, trace sink, and oracle sink.
6. Make the pinned CSS fixture lane pass against parse corpus S-expressions
   and highlight assertions. The first passing lane should include at least one
   external-scanner valid-symbol-mask trace even if the chosen parse case does
   not require a complex scanner branch.
7. Add recovery and GLR conflict coverage from fixture cases. Every ambiguity
   or recovery decision must produce trace events that explain the retained
   branch and emitted tree shape.
8. Add incremental parsing: old-tree reuse candidates, included ranges, edit
   coordinates, changed ranges, scanner-state replay, and final tree oracle
   equivalence.
9. Only after Weavy execution correctness is observable, add dense block
   resolution and optional copy-and-patch/JIT hooks for Snark intrinsics. JIT
   support must consume the same lowered program and emit the same trace/oracle
   events.

Anything that skips from raw grammar DTOs to a toy parser, recursive descent, or
generated implementation-file behavior is outside this plan.
