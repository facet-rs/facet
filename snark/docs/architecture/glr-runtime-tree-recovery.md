# GLR Runtime, Tree Construction, and Recovery

Snark must implement Tree-sitter grammar semantics as a real LR/GLR runtime.
The compatibility boundary is the grammar package input plus Tree-sitter's
observable outputs, not generated grammar implementation files. Generated
`src/parser.c` files from grammar packages stay out of scope; the useful source
model is Tree-sitter runtime behavior: parse-table actions, stack versions,
subtree construction, recovery, node visibility, and incremental reuse.

This note describes the runtime shape Snark needs before CSS parsing can be
called Tree-sitter-compatible. It is intentionally not a recursive parser plan
and not a green-tree-first shortcut. The parse tree exists because LR actions
construct it while the stack evolves.

## Source Model

The source-level behavior studied here comes from the Tree-sitter runtime in
the pinned `arborium-tree-sitter-2.18.0` crate:

- `src/parser.c`: parser loop, lexing, shift/reduce/accept/recover, stack
  condensation, node reuse, and missing-token recovery.
- `src/stack.c`: stack graph, versions, path popping, pause/halt/copy/merge.
- `src/subtree.c`: leaves, parent subtrees, ERROR/MISSING nodes, visibility
  summaries, S-expression printing, edits, and tree selection.
- `src/language.c`: parse-table lookup, lex modes, reserved words, symbol
  metadata, public symbols, next-state lookup, and field names.
- `src/get_changed_ranges.c`, `src/tree.c`, `src/node.c`, and
  `src/tree_cursor.c`: incremental tree comparison and public node/cursor
  contracts.

Snark should not copy these files mechanically. The point is to preserve their
semantic contracts in Rust types and Weavy-lowered programs.

## Snark Ownership

The current module split already points at the final shape:

- `grammar`: raw Tree-sitter JSON DTOs only.
- `validated`: durable grammar facts: rule ids, symbol references, fields,
  aliases, extras, externals, conflicts, precedence, supertypes, reserved sets,
  and visible node kinds.
- `lexical`: lexical automata and lex modes derived from validated facts,
  including keyword and reserved-word behavior.
- `scanner`: external scanner host ABI and scanner-state snapshots.
- `runtime_input`: byte/point edits and included ranges.
- `corpus`: corpus S-expression parsing and normalization oracle.
- future runtime modules should be final-shape and narrow:
  `runtime::table`, `runtime::stack`, `runtime::tree`,
  `runtime::recover`, `runtime::incremental`, and
  `runtime::sexp`.

`milestone::scannerless` stays quarantined as a smoke parser. No runtime type
should depend on it.

## Runtime Tables

Snark needs explicit table objects derived from `ValidatedGrammar`, not ad hoc
rule walking:

- `ParseStateId`: LR state id.
- `GrammarSymbolId`: internal symbol id for named rules, anonymous tokens,
  auxiliary rules, externals, aliases, and builtins.
- `ProductionId`: reduce production id used to recover fields and aliases for
  child positions.
- `ParseAction`: `Shift { state, extra, repetition }`,
  `Reduce { symbol, child_count, dynamic_precedence, production }`,
  `Accept`, and `Recover`.
- `TableEntry`: action slice plus reusability metadata for incremental token
  reuse.
- `LexMode`: main lexer state, external scanner state, and reserved-word set.
- `SymbolMetadata`: visible/named/supertype flags and public symbol mapping.
- `FieldMap` and `AliasSequence`: production-indexed child metadata.

The runtime must consume these tables only through typed operations:
`table_entry(state, lookahead)`, `next_state(state, symbol)`,
`lex_mode_for_state(state)`, `symbol_metadata(symbol)`,
`field_map(production)`, and `alias_sequence(production)`.

## Stack Versions

Tree-sitter's parser stack is a graph, not a single vector. Each active
version points at a stack head; heads share predecessor nodes when paths
converge. A stack node carries:

- current LR state;
- byte/point position;
- pushed subtree edge and pending flag;
- accumulated error cost;
- visible-node progress count;
- accumulated dynamic precedence.

Snark should model this as `runtime::stack::Stack` with `StackVersion` handles
and immutable-ish stack nodes behind shared ids. Required operations:

- `push(version, subtree, pending, state)`;
- `pop_count(version, structural_count) -> Vec<StackSlice>`;
- `pop_pending(version) -> Vec<StackSlice>`;
- `pop_error(version) -> Option<SubtreeArray>`;
- `copy_version(version) -> StackVersion`;
- `merge(left, right) -> bool`;
- `pause(version, lookahead)`, `resume(version)`, `halt(version)`;
- `record_summary(version, max_depth)`.

The important count is structural child count, not raw child count. Extra
subtrees do not advance structural child indexes and do not satisfy reduce
child counts.

## Split, Merge, and Condense

GLR happens when one `(state, lookahead)` has multiple actions. The runtime
does not choose early:

- SHIFT mutates the current stack version and ends that advance step.
- REDUCE pops one or more paths from the current stack version. Each distinct
  path can create a parent subtree and a new stack version.
- ACCEPT pushes EOF, pops the complete stack, builds candidate roots, and keeps
  the best root.
- RECOVER enters error handling for that version.

After versions advance, Snark must condense the stack:

- remove halted versions;
- compare versions by error status, error cost, progress count, and dynamic
  precedence;
- merge active versions only when state, byte position, error cost, and last
  external scanner state match;
- keep an upper bound on pursued versions;
- resume the best paused version for recovery if no better active version can
  proceed.

Merging is semantic, not cosmetic. A merged stack node can have multiple links,
and later `pop_count` must enumerate those paths so reductions can still select
between ambiguous child arrays.

## Shift Execution

A shift pushes the lookahead subtree and moves to the action's shift state.
For extra shifts, the next parse state remains the same. If the shifted
lookahead is a reused parent subtree with children, the runtime may need to
break it down until its first leaf parse state matches the current state.

Snark shift invariants:

- the pushed subtree's `extra` bit must match the shift action for leaves;
- external scanner state advances when the shifted subtree contains external
  tokens;
- extra shifts must not change the structural state;
- reused subtrees must be broken down before a mismatched parse state is
  allowed to corrupt `next_state`.

## Reduce Execution

A reduce with `child_count = N` pops N structural children from the stack
version. Because versions can have merged, the pop can return multiple paths.
For each path:

- trailing extra subtrees are removed from the reduce children;
- a parent subtree is constructed with the reduced symbol and production id;
- competing child arrays for the same version are compared by error cost,
  dynamic precedence, then stable subtree ordering;
- the parent parse state is the pre-reduce state unless ambiguity/error made
  the parent fragile;
- dynamic precedence from the reduce action is added to the parent;
- the parent is pushed using `next_state(previous_state, reduce_symbol)`;
- trailing extras are pushed back after the parent.

This means fields and aliases cannot be attached after the fact from a green
tree. The parent's production id is what lets later traversal know how to label
and alias its structural children.

## Node Construction

Snark needs a concrete `runtime::tree::Subtree` before any public syntax tree
view:

- symbol id;
- padding length and size length in bytes and points;
- lookahead bytes;
- parse state and first-leaf parse state;
- production id for parent nodes;
- child array;
- visible, named, extra, missing, keyword flags;
- fragile-left/right flags;
- has-changes flag;
- error cost;
- dynamic precedence;
- visible child/count summaries;
- external scanner state and scanner-state-change flag;
- depends-on-column flag for column-sensitive lexing.

Leaves are created by the lexer with padding, size, parse state, keyword flag,
and external scanner state. Parent nodes are created by reductions and then
summarized from children. Summary must compute total size, lookahead bytes,
visible/named counts, error cost, dynamic precedence, fragile flags, first leaf,
external-token presence, and column dependence.

The final `Tree` is the accepted root subtree plus language/table identity and
included ranges. Public nodes should be lightweight references into this tree,
not separate objects.

## Fields and Aliases

Fields and aliases are production metadata:

- `GrammarExpr::Field` in `validated` interns field ids.
- `GrammarExpr::Alias` interns aliases and records whether the alias is named.
- parser generation must lower each production into a `FieldMap` and
  `AliasSequence` keyed by structural child index.

Traversal must apply aliases and fields through invisible wrappers. Tree-sitter
walks up visible and invisible ancestors because fields can refer to a visible
descendant through hidden wrapper nodes. Snark's public cursor and S-expression
normalizer must do the same.

Anonymous aliases matter. A named alias can make an otherwise hidden child
visible as a named node. An anonymous alias can change node type and quoting
without making it a named corpus node.

## Named, Anonymous, Visible, Hidden

Tree-sitter separates these concepts:

- named visible symbols: regular public node types;
- visible anonymous symbols: string literal tokens like `";"`;
- hidden auxiliary symbols: internal runtime nodes that public traversal skips;
- supertypes: query/node-type metadata, not ordinary parse output.

The corpus S-expression is not a complete raw tree dump. By default it prints:

- visible named nodes;
- `MISSING` leaves even when anonymous;
- fields attached to visible descendants;
- aliases as public node types;
- ERROR nodes and unexpected characters.

Hidden nodes still exist internally and are required for parse states, fields,
aliases, recovery, changed ranges, and query behavior.

## Extra Nodes

Extras are skipped grammar content such as comments and whitespace-like tokens
declared in `extras`. Runtime implications:

- extra shifts do not advance the parse state;
- extra subtrees are not structural children for reduce counts or alias/field
  structural indexes;
- trailing extras are removed before reduce parent construction and then
  pushed back after the parent;
- error recovery can mark skipped lookahead as extra when the error-state
  action says it is extra;
- changed-ranges and cursors must account for extra text positions even when
  extras are hidden from named-child APIs.

Snark cannot implement extras as lexer-only trivia discarded before parsing.
They are tree nodes with positions and flags.

## ERROR and MISSING

Tree-sitter has two distinct recovery outputs:

- `ERROR`: a visible named node for skipped invalid structure, or an
  `UNEXPECTED` leaf for an unrecognized character.
- `MISSING`: a zero-width inserted leaf with padding and lookahead bytes.

Missing-token recovery is attempted by scanning possible token symbols from the
current state. If inserting a missing token would transition to a state that
has a reduce action for the current lookahead, the runtime forks a version with
that `MISSING` leaf and runs reductions.

`MISSING` leaves are treated as extra for parse-state purposes but are visible
in the corpus S-expression as `(MISSING name)` for named symbols and
`(MISSING "literal")` for anonymous symbols.

`ERROR` nodes carry error cost. Error cost is part of version comparison, tree
selection, changed-range comparison, and public `has_error` behavior. Snark
must preserve costed recovery instead of merely inserting a diagnostic node.

## Error Recovery

When no action can process the lookahead:

1. The stack version is paused with the lookahead.
2. Condensing either drops it because another version advanced, or resumes the
   best paused version.
3. Error handling first runs reductions that might now be possible.
4. It records a stack summary of prior states.
5. It tries missing-token insertion.
6. It pushes a discontinuity into `ERROR_STATE`.
7. It tries to recover to a prior summarized state where the current lookahead
   is valid, wrapping skipped subtrees into an `ERROR` node.
8. It also considers skipping the current lookahead by wrapping it in ERROR,
   unless that path is clearly worse or would misuse external scanner state.
9. At EOF, remaining error-state content is wrapped and accepted.

Recovery is constrained by costs:

- skipped trees, skipped chars, skipped lines, and recovery itself add cost;
- paused/error versions are penalized;
- better active or finished versions prune worse recovery paths;
- dynamic precedence breaks ties after error cost.

Snark's recovery module should expose cost constants and traceable decisions so
runtime traces can explain why a version was pruned, merged, or selected.

## Incremental Parse Contracts

Incremental parsing is not just reparsing a range. Tree-sitter mutates the old
tree with `InputEdit`, then tries to reuse old subtrees while parsing the new
input.

Snark's `runtime_input::InputEdit` already has the right byte/point shape. The
runtime tree must implement edit propagation:

- adjust subtree sizes and padding across the edit;
- set `has_changes` on edited ancestors/children;
- invalidate column-dependent children when a line/column shift matters;
- update included ranges with the same edit;
- preserve and compare external scanner state.

Node reuse is rejected when the candidate:

- starts before/after the current stack position;
- has changes;
- is an error;
- is missing;
- is fragile;
- intersects an included-range difference;
- has a mismatched previous external scanner state;
- has a first leaf that is not reusable in the current lex mode/table entry.

If a reused parent later proves invalid for the current lookahead, the runtime
breaks it down into its children and continues. This is essential: a reused
subtree is an optimization candidate, not proof that its parent shape is still
valid.

## Changed Ranges

`changed_ranges(old_tree, new_tree)` compares visible tree states, aliases,
parse states, sizes, error costs, `has_changes`, external-token presence, and
external scanner state. It descends when two visible nodes may differ
internally and emits ranges only where visible structure or included-range
membership differs.

Snark must therefore preserve internal metadata even when the corpus tree
matches:

- alias symbol at a structural child index;
- visible depth through hidden wrappers;
- parse state and `TS_TREE_STATE_NONE`-style unknown state;
- error cost;
- external scanner state;
- included range differences;
- padding positions around visible nodes.

The public `changed_ranges` contract belongs in `runtime::incremental`, but it
depends on `runtime::tree` and `runtime::cursor`.

## Corpus S-Expression Normalization

`corpus::SexpNode` is the oracle format, not the runtime tree. Snark should
normalize its final tree to corpus S-expressions with Tree-sitter's public node
view:

- start at the accepted root;
- walk structural children in order;
- skip hidden nodes unless they contain visible descendants;
- carry field names through hidden wrappers until a visible descendant receives
  them;
- ignore extras unless they are visible/named under the grammar or included by
  the relevant public view;
- apply alias sequences before deciding public type/name;
- quote anonymous visible tokens;
- print named visible symbols bare;
- print missing leaves as `MISSING` with named/anonymous formatting;
- print leaf ERROR with unexpected-character shape and parent ERROR nodes as
  normal visible error nodes;
- emit only the normalized string/value shape expected by `corpus::SexpNode`.

This gives Snark a mechanically testable pipeline:

`grammar.json -> ValidatedGrammar -> runtime tables -> GLR parse -> Tree ->
runtime::sexp::to_corpus_sexp -> corpus::SexpNode/to_sexp`.

## Actionable Milestones

The next implementation steps should keep final runtime boundaries:

1. Add `runtime::symbol` types mirroring validated ids plus builtin ERROR,
   ERROR_REPEAT, EOF, and public symbol mapping.
2. Add `runtime::table` with typed parse actions, lex modes, symbol metadata,
   production field maps, alias sequences, and `next_state`.
3. Lower a small grammar slice from `ValidatedGrammar` into table fixtures and
   assert table facts directly before parsing.
4. Add `runtime::tree::Subtree` with leaf/parent/error/missing constructors and
   summary recomputation.
5. Add `runtime::stack::Stack` with version graph operations and merge
   semantics.
6. Implement shift/reduce/accept without recovery first, but with real
   production ids, fields, aliases, extras, and subtree summaries.
7. Add `runtime::sexp` and compare accepted trees to structured corpus
   S-expressions.
8. Add recovery with costed `ERROR`/`MISSING` construction.
9. Add edit propagation, node reuse, and changed-ranges comparison.
10. Only then lower the same runtime operations to Weavy.

Each milestone should have an oracle that exercises the production path. A
table fact test is fine for table lowering; a parser milestone should compare
final corpus S-expressions and, for recovery/incremental work, structured tree
metadata and changed ranges.

## Must-Not-Violate Invariants

- Do not use generated grammar `src/parser.c` as input, oracle, or reference.
- Do not replace LR/GLR table execution with recursive rule walking.
- Do not discard hidden nodes, extras, fields, aliases, parse states, or
  production ids before public tree normalization.
- Do not treat the corpus S-expression as the internal tree model.
- Do not attach fields or aliases by visible child index; they are
  production/structural-child metadata and must pass through hidden wrappers.
- Do not count extra nodes as structural children for reduce counts,
  alias/field indexes, or parser state advancement.
- Do not merge stack versions unless parse state, byte position, error cost,
  active status, and external scanner state are compatible.
- Do not select ambiguous trees without considering error cost, dynamic
  precedence, and stable tree ordering.
- Do not model `MISSING` as a diagnostic only; it is a zero-width subtree that
  appears in the corpus oracle.
- Do not model `ERROR` as a flat span only; it participates in error cost,
  tree selection, public node APIs, and changed ranges.
- Do not reuse old subtrees that are changed, error, missing, fragile,
  included-range-different, or external-scanner-state-incompatible.
- Do not report changed ranges from byte diffs alone; compare the visible tree
  view plus parse/error/external metadata.
- Do not lower to Weavy from raw `grammar.json`; lower from validated Snark
  grammar and runtime-table facts.
