# LR/GLR Table Generation

Snark must build parser tables from grammar semantics, not from generated
Tree-sitter implementation files. The source boundary is:

1. raw `grammar.json` is decoded by `grammar`;
2. `validated` resolves names into Snark ids and preserves semantic wrappers;
3. the parser generator lowers those facts into Snark-owned lexer, LR action,
   goto, production, conflict, and metadata tables;
4. the GLR runtime consumes those tables.

Generated `src/parser.c` files are outside this design. They are neither inputs
nor table oracles. The oracle remains Tree-sitter's observable corpus trees,
query captures, scanner behavior, and incremental parse behavior.

## Generator Inputs

The generator should start from `ValidatedGrammar`, then build a generator IR
that separates syntax grammar from lexical grammar:

- syntax variables: visible, hidden, auxiliary, and start variables;
- lexical variables: literal tokens, pattern tokens, external tokens, extras,
  reserved words, and the optional `word` token;
- productions: a stable `ProductionId`, owning rule id, ordered production
  steps, field and alias metadata, static precedence, associativity, dynamic
  precedence, reserved-word context, and provenance;
- symbol ids split by domain: terminal, nonterminal, external, EOF, and any
  internal sentinels needed for extras or recovery.

`GrammarExpr::Choice` becomes multiple productions. `GrammarExpr::Seq` becomes
one ordered production. `GrammarExpr::Repeat` and `Repeat1` must become
auxiliary recursive variables, not ad hoc parser loops. `inline` copies the
target productions into use sites before item-set construction while preserving
provenance. `extras`, `reserved`, `externals`, `word`, `aliases`, and `fields`
remain first-class facts attached to the generated tables.

## Production IDs

Snark needs two related ids:

- `ProductionId`: stable within one generated grammar, assigned to every
  flattened syntax production, including generated auxiliary productions.
- `ProductionMetadataId`: optional deduplicated metadata record for field maps,
  alias sequences, visibility, and node construction data.

Reduce actions should carry `ProductionId`, reduced symbol, child count, dynamic
precedence, and `ProductionMetadataId`. This avoids overloading metadata
deduplication with parse semantics. Diagnostics should report the source rule
name and generated auxiliary name, but runtime should use ids.

## Items

An LR item is an in-progress production:

```text
ProductionId, dot_index, lookahead_set, following_reserved_word_set
```

For a production `A -> alpha beta`, `dot_index` is the number of already
matched steps. The item is complete when `dot_index == beta.len()`.

An item set is a parse state candidate. Its core is the same item set with
lookahead and reserved-word-follow information removed. The core is useful for
state identity, minimization, diagnostics, and optional state merging, but
Snark should keep full lookahead-bearing item sets as the initial correctness
model.

The augmented start item is:

```text
START -> . start_rule, lookahead = EOF
```

When the augmented item completes on EOF, the action is accept.

## Closure

Closure expands items whose next symbol is a nonterminal. For an item:

```text
A -> alpha . B beta, lookahead = L
```

closure adds an item for every production of `B`:

```text
B -> . gamma, lookahead = FIRST(beta)
```

If `beta` can be empty, the new item's lookahead also includes `L`. If a
grammar-preparation pass forbids nullable syntax productions except controlled
generated forms, keep that invariant explicit and test it. Do not let closure
quietly assume non-nullability as an optimization until the grammar IR proves
it.

Closure must also propagate the reserved-word context that applies when the
lookahead contains the `word` token. That context is part of lexical validity,
so it belongs in item-set entries and parse states, not in a later syntax-tree
construction pass.

Implementation shape:

- precompute `FIRST` sets for terminals, external tokens, and nonterminals;
- precompute nullable facts, even if most real syntax productions are
  non-nullable after preparation;
- precompute closure additions per nonterminal where possible;
- compute closure to a fixed point by merging lookahead sets for identical
  core items.

## Goto

`goto(I, X)` advances every item in `I` whose next symbol is `X`, then closes the
result:

```text
A -> alpha . X beta  =>  A -> alpha X . beta
```

If `X` is terminal or external, `goto` contributes a shift action in the source
state. If `X` is nonterminal, it contributes a goto entry. Each unique closed
item set becomes a parse state. The generator starts with the closed augmented
start set and performs breadth-first discovery until no new states appear.

## Parse States

A parse state should contain:

- `ParseStateId`;
- item-set core id;
- terminal entries: terminal or external symbol to one or more parse actions;
- nonterminal entries: nonterminal symbol to goto action;
- reserved-word set active for the state;
- lexical state id for internal tokens;
- external scanner valid-symbol set id;
- optional diagnostic provenance: a representative symbol path to the state.

The lexical state ids and external scanner valid-symbol sets are generator
output. Calling scanner code and choosing a token are runtime responsibilities.

## Action and Goto Tables

Terminal action entries must support multiple actions. A deterministic LR state
usually has one action per terminal. A declared GLR conflict intentionally keeps
more than one.

Actions:

- `Accept`;
- `Shift { state, is_repetition }`;
- `ShiftExtra`;
- `Recover`;
- `Reduce { symbol, child_count, dynamic_precedence, production_id,
  production_metadata_id }`.

Goto entries:

- `Goto(ParseStateId)`;
- `ShiftExtra` for nonterminal extras, if Snark keeps that Tree-sitter-shaped
  representation.

`ShiftExtra` and `Recover` are table facts, but the policies for skipping
extras, recovery costs, stack versions, subtree reuse, and incremental edits are
runtime policies.

## Conflicts

Conflicts are discovered when more than one action is present for the same
state and terminal lookahead.

### Shift/Reduce

A shift/reduce conflict means one item can shift the lookahead while another
complete item can reduce on that lookahead.

Resolution order:

1. compare static precedence of the shift candidate and reduce candidate;
2. if one side is strictly higher, keep only that side;
3. if precedence ties, apply associativity from the reduce production;
4. if still unresolved, keep the multi-action entry only when the involved
   rule set is declared in `conflicts`;
5. otherwise emit a structured generator diagnostic.

Left associativity chooses the interpretation that ends earlier, so it removes
the shift. Right associativity chooses the interpretation that ends later, so
it removes the reduce. No associativity leaves the conflict unresolved.

### Reduce/Reduce

A reduce/reduce conflict means two or more complete items can reduce on the
same lookahead.

Resolution order:

1. compare static precedence among reduce candidates;
2. keep only the highest-precedence reductions if there is a strict winner;
3. if multiple reductions remain tied, keep them only when the involved rule set
   is declared in `conflicts`;
4. otherwise emit a structured generator diagnostic.

Associativity does not resolve reduce/reduce conflicts by itself.

### Repetition Conflicts

Auxiliary repeat rules can produce intentional local ambiguity in how a repeated
sequence groups. The generator may mark the shift as `is_repetition` for the
runtime if every conflicting item belongs to the same auxiliary repeat symbol.
This is a generated-grammar fact, not a hand-written CSS recognizer shortcut.

## Static Precedence

Static precedence is a generator-time conflict resolver. Snark must support:

- integer precedence, where missing precedence is zero;
- named precedence, ordered by `precedences`;
- symbol precedence entries in precedence groups;
- validation that named precedences are declared;
- validation that precedence groups do not create contradictory ordering pairs.

Precedence wrappers should attach to the production step they govern. For a
completed reduction, the effective static precedence is the precedence on the
last consumed step, unless the grammar-preparation pass carries a more explicit
production-level value. For a shift candidate, compare against the precedence of
the item just before the shifted symbol, matching the LR conflict point.

Named precedence comparison is only meaningful when both sides appear in a
common ordering relation. If the ordering cannot compare them, they tie.

## Associativity

Associativity is only consulted after static precedence ties in a
shift/reduce conflict. Snark currently validates `PrecedenceAssoc::{None, Left,
Right}` in `validated`; the generator should lower that to:

- no associativity: no tie-break;
- left: prefer reduce over shift;
- right: prefer shift over reduce.

If a conflict has mixed left, right, and non-associative reductions, leave it
unresolved unless a declared conflict admits GLR.

## Dynamic Precedence

Dynamic precedence is not a generator-time conflict resolver. It is attached to
reduce actions so the GLR runtime can rank successful parses inside a declared
ambiguity. The runtime must accumulate dynamic precedence over reductions on
each stack version or subtree candidate, then prefer the highest total when
multiple parses survive for the same input span and goal.

The generator's job is to preserve `prec.dynamic` on the flattened production
and copy it into the reduce action. It must not use dynamic precedence to erase
table conflicts.

## Conflict Declarations

`conflicts` declares sets of rules whose ambiguity is intended. During conflict
handling, Snark should compute the actual involved visible parent symbols:

- use the owning nonterminal for ordinary items;
- map generated auxiliary repeat symbols back to their parent symbols for
  diagnostics and conflict-set matching;
- sort and deduplicate the symbol set before comparison.

If the actual conflict set matches a declared set, keep the table entry with
multiple actions. If a declared set is never used, emit a warning or structured
diagnostic. If an undeclared conflict remains, generation fails.

This is the point where GLR becomes enabled in the table: multiple actions are
retained. The runtime branching itself has not started yet.

## Lookahead Treatment

Snark should treat lookahead as a terminal/external/EOF token set, not as a
single token. Closure and conflict resolution need full sets. Parse actions are
emitted per lookahead symbol.

Lookahead must include:

- ordinary lexical tokens;
- external scanner tokens;
- EOF;
- any internal sentinel used for nonterminal extras or recovery;
- reserved-word context when the lookahead includes the `word` token.

The generator should prefer dense bitsets for token sets. The shape mirrors
Tree-sitter's efficient token-set usage without adopting generated C tables.

## Lexical Tables and External Tokens

Snark's LR table generation depends on lexing facts but should not blur them
with syntax reduction:

- internal string and regex tokens become lexer NFA/DFA states;
- lexical precedence inside `token(prec(...))` resolves competing tokens before
  syntax actions see a token;
- parse precedence outside `token(...)` resolves LR actions;
- extras contribute `ShiftExtra` entries or equivalent skip transitions;
- external tokens contribute valid-symbol sets per parse state.

The external scanner valid-symbol set is a generator output indexed by state.
At runtime, the scanner host receives that set and may return one of those
external symbols.

## Generator Output vs GLR Runtime

Generator output ends at immutable language tables:

- symbol table and symbol metadata;
- lexical automata;
- parse states;
- terminal action entries, including multi-action entries for declared
  conflicts;
- nonterminal goto entries;
- production table and production metadata table;
- field maps and alias sequences;
- external scanner valid-symbol sets;
- recovery and extra-token table facts;
- structured diagnostics for unresolved or unused conflicts.

The GLR runtime begins when parsing an input stream:

- lexing the next token using the current state's lexical and external scanner
  facts;
- applying one or more actions for a state/lookahead pair;
- forking stack versions on multi-action entries;
- reducing and constructing syntax subtrees;
- accumulating dynamic precedence;
- merging compatible stack versions;
- applying recovery and error costs;
- reusing subtrees during incremental parsing;
- selecting the surviving tree when ambiguity remains.

The generator should be deterministic and inspectable. Given the same
`ValidatedGrammar`, it should emit the same table ids, production ids, conflict
diagnostics, and provenance maps.

## First Snark Scaffolding

Add the table-generation side before attempting runtime parsing:

1. `snark/src/lr.rs`
   Public module boundary for grammar preparation and table generation.

2. `snark/src/lr/symbol.rs`
   Dense ids for terminal, nonterminal, external, EOF, and internal sentinels.

3. `snark/src/lr/production.rs`
   Flattened production arena, `ProductionId`, `ProductionStep`, precedence,
   associativity, dynamic precedence, field, alias, reserved-word context, and
   provenance.

4. `snark/src/lr/prepare.rs`
   Lower `ValidatedGrammar` into syntax and lexical generator IR. Implement
   choice flattening, repeat auxiliary generation, inline expansion, named
   precedence validation, and nullable/FIRST prerequisites here.

5. `snark/src/lr/item.rs`
   `LrItem`, `ItemSetEntry`, `ItemSet`, `ItemSetCore`, token bitsets, and
   display helpers for diagnostics.

6. `snark/src/lr/build.rs`
   Closure, goto, state discovery, action/goto insertion, static conflict
   resolution, declared-conflict preservation, and unresolved-conflict
   diagnostics.

7. `snark/src/lr/table.rs`
   Immutable output tables: `ParseTable`, `ParseState`, `ParseAction`,
   `GotoAction`, production metadata, lex-state ids, and external valid-symbol
   sets.

8. Tests
   Use tiny grammars as structural oracles for item sets and action/goto tables,
   then add Tree-sitter corpus fixtures as behavioral oracles once the runtime
   exists. Avoid tests that only prove a reduced recognizer accepts a CSS slice.

Reference points used for this note: Snark's current `grammar`, `validated`,
and `lower` boundaries; Tree-sitter's public grammar DSL documentation for
precedence, associativity, dynamic precedence, and conflicts; and
`tree-sitter-generate` 0.26.10 source for generator-side item sets, closure,
parse table entries, precedence comparison, and conflict handling.
