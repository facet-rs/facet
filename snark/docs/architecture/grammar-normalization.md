# Grammar Normalization

Snark's parser path starts from Tree-sitter grammar semantics, not generated
implementation artifacts. `src/parser.c` is not an input, not an oracle, and
not an implementation reference. The compatibility source is the grammar DSL as
captured in `src/grammar.json`, plus observable Tree-sitter corpus/query
outputs.

This note defines the final shape Snark should build before LR/GLR table
generation:

```text
RawGrammarJson
  -> ValidatedGrammar
  -> ParserGrammar
  -> LR/GLR tables and Snark runtime/Weavy lowering
```

`ValidatedGrammar` is the checked semantic form of Tree-sitter input.
`ParserGrammar` is the table-generation form: lexical variables, syntax
variables, flat productions, precedence facts, conflict sets, aliases, fields,
reserved words, and provenance.

## Upstream Shape

Tree-sitter's generator performs a semantic normalization pipeline before parse
table construction. The important upstream shape is:

1. validate named precedence declarations and indirect single-symbol recursion;
2. intern rule names and external token names into typed symbols;
3. extract string, regex, `token(...)`, and `token.immediate(...)` rules into a
   lexical grammar;
4. expand `repeat(...)` into auxiliary syntax variables;
5. flatten choices and sequences into productions;
6. expand lexical rules into an NFA;
7. extract default aliases;
8. process inline variables into an inlined-production map.

Relevant upstream sources:

- Tree-sitter Grammar DSL:
  `https://tree-sitter.github.io/tree-sitter/creating-parsers/2-the-grammar-dsl.html`
- Tree-sitter writing guide:
  `https://tree-sitter.github.io/tree-sitter/creating-parsers/3-writing-the-grammar.html`
- Generator entry point:
  `https://github.com/tree-sitter/tree-sitter/blob/master/crates/generate/src/prepare_grammar.rs`
- Symbol/rule grammar structures:
  `https://github.com/tree-sitter/tree-sitter/blob/master/crates/generate/src/grammars.rs`
  and
  `https://github.com/tree-sitter/tree-sitter/blob/master/crates/generate/src/rules.rs`

Snark should track this shape directly, while keeping its own typed ids,
diagnostics, provenance maps, and Weavy lowering boundary.

## RawGrammarJson

`RawGrammarJson` remains a compatibility DTO. It should preserve the
Tree-sitter JSON surface without pretending to be parser input:

```rust
struct RawGrammarJson {
    name: String,
    rules: RuleTable,
    extras: Vec<RawRuleJson>,
    precedences: Vec<Vec<PrecedenceEntryJson>>,
    conflicts: Vec<Vec<String>>,
    externals: Vec<RawRuleJson>,
    inline: Vec<String>,
    supertypes: Vec<String>,
    word: Option<String>,
    reserved: ReservedSetTable,
}
```

Raw invariants:

- rule order is preserved; the first rule is the start rule;
- JSON shape is decoded with source identity and package-relative path;
- no grammar semantics are inferred from generated files;
- raw rule nodes may still contain `CHOICE`, `SEQ`, `REPEAT`, wrappers, strings,
  patterns, aliases, fields, and unresolved symbol names;
- raw diagnostics report source path and byte span where available.

`RawGrammarJson` must not expose an API that looks executable. It only answers
"what did the package declare?"

## ValidatedGrammar

`ValidatedGrammar` is the resolved semantic grammar. Current
`snark/src/validated.rs` is the right home for this layer, but it should grow
from "resolved expression arena" into "validated Tree-sitter input semantics."
It is still not final table input.

Suggested shape:

```rust
struct ValidatedGrammar {
    language: LanguageName,
    start: NonterminalId,
    rules: Vec<RuleDecl>,
    rules_by_name: OrderedMap<RuleName, NonterminalId>,
    externals: Vec<ExternalDecl>,
    externals_by_name: OrderedMap<String, ExternalTokenId>,
    exprs: Arena<GrammarExpr>,
    extras: Vec<GrammarExprId>,
    precedence_order: PrecedenceOrder,
    conflicts: Vec<ConflictSet>,
    inline: Vec<NonterminalId>,
    supertypes: Vec<NonterminalId>,
    word: Option<NonterminalId>,
    reserved_sets: Vec<ReservedSetDecl>,
    fields: InternTable<FieldName, FieldId>,
    aliases: InternTable<AliasDecl, AliasId>,
    visible_nodes: VisibleNodeTable,
    provenance: GrammarProvenance,
}

enum GrammarExpr {
    Blank,
    StringToken(String),
    PatternToken { value: String, flags: Option<String> },
    Symbol(SymbolRef),
    Choice(Vec<GrammarExprId>),
    Seq(Vec<GrammarExprId>),
    Repeat(GrammarExprId),
    Repeat1(GrammarExprId),
    Metadata {
        content: GrammarExprId,
        precedence: StaticPrecedence,
        associativity: Option<Associativity>,
        dynamic_precedence: i32,
        alias: Option<AliasId>,
        field: Option<FieldId>,
        token: TokenWrapper,
        reserved: Option<ReservedSetId>,
    },
}

enum SymbolRef {
    Nonterminal(NonterminalId),
    External(ExternalTokenId),
}
```

Validation invariants:

- start rule exists and is visible; a hidden start rule is rejected before
  normalization;
- every `SYMBOL` resolves to a declared rule or declared external token;
- when a rule name and external token name collide, rule references resolve to
  the nonterminal first, matching Tree-sitter symbol interning;
- contexts that require rules (`inline`, `supertypes`, `conflicts`, `word`
  before token extraction) reject unresolved names and external-only names;
- external declarations are validated as scanner/token declarations, not as
  self-recursive grammar expressions;
- fields and aliases are interned by semantic identity; aliases include
  namedness in the key;
- named aliases contribute visible node kinds; inline visible rules do not;
- supertypes are known rules and become hidden node categories later;
- named precedence values used by parse precedence wrappers are declared in
  `precedences`;
- precedence ordering declarations are acyclic and non-conflicting;
- indirect single-symbol recursion cycles are rejected before parser generation;
- reserved-word wrappers reference declared reserved sets;
- reserved-word entries remain terminal candidates and are checked again after
  token extraction;
- every declaration and wrapper records provenance: raw source id, package path,
  JSON pointer or byte span, and the normalization pass that produced it.

`ValidatedGrammar` should not lower repetitions, inline variables, extract
tokens, or flatten productions. Those are ParserGrammar normalization steps.

## ParserGrammar

`ParserGrammar` is the LR/GLR input. It contains no raw recursive grammar tree
that a parser could interpret directly. All choices, sequences, repeats, token
wrappers, aliases, fields, precedence wrappers, reserved wrappers, and inline
rules have been normalized into explicit table-generation facts.

Suggested shape:

```rust
struct ParserGrammar {
    language: LanguageName,
    start: NonterminalId,
    symbols: SymbolTable,
    syntax: Vec<SyntaxVariable>,
    lexical: LexicalGrammar,
    externals: Vec<ExternalToken>,
    productions: Vec<Production>,
    production_ids_by_lhs: Vec<Vec<ProductionId>>,
    extras: ExtraPolicy,
    conflicts: Vec<ConflictSet>,
    precedence_order: PrecedenceOrder,
    inlines: InlineMap,
    default_aliases: AliasMap,
    fields: Vec<FieldName>,
    aliases: Vec<AliasDecl>,
    supertypes: Vec<NonterminalId>,
    word_token: Option<TerminalId>,
    reserved_sets: Vec<TokenSet>,
    visible_nodes: VisibleNodeTable,
    provenance: ParserProvenance,
}

struct SymbolTable {
    terminals: Vec<TerminalSymbol>,
    nonterminals: Vec<NonterminalSymbol>,
    externals: Vec<ExternalSymbol>,
    eof: SymbolId,
    end_of_nonterminal_extra: SymbolId,
}

struct SyntaxVariable {
    id: NonterminalId,
    name: RuleName,
    visibility: VariableVisibility,
    productions: Vec<ProductionId>,
}

struct Production {
    id: ProductionId,
    lhs: NonterminalId,
    steps: Vec<ProductionStep>,
    dynamic_precedence: i32,
    origin: ProductionOrigin,
}

struct ProductionStep {
    symbol: SymbolId,
    precedence: StaticPrecedence,
    associativity: Option<Associativity>,
    alias: Option<AliasId>,
    field: Option<FieldId>,
    reserved_word_set: ReservedSetId,
    origin: StepOrigin,
}

struct LexicalGrammar {
    terminals: Vec<TerminalSymbol>,
    variables: Vec<LexicalVariable>,
    nfa: LexicalNfa,
    lexical_precedence: Vec<LexicalPrecedence>,
    immediate: TerminalSet,
    keyword_extraction: KeywordExtraction,
}

struct ExternalToken {
    id: ExternalTokenId,
    ordinal: ExternalTokenOrdinal,
    name: Option<String>,
    visibility: VariableVisibility,
    corresponding_internal_token: Option<TerminalId>,
}
```

ParserGrammar invariants:

- all ids are dense and domain typed; terminals, nonterminals, externals, EOF,
  and end-of-nonterminal-extra are distinct symbol domains;
- syntax productions contain only `ProductionStep` entries; there are no
  `Choice`, `Seq`, `Repeat`, `Token`, `Field`, `Alias`, `Prec`, or `Reserved`
  expression nodes left;
- every production id belongs to exactly one lhs nonterminal;
- duplicate productions for the same variable are removed after flattening;
- empty productions are allowed only where Tree-sitter allows them; a used
  non-start syntactic variable that matches the empty string is rejected;
- repetitions are represented by generated auxiliary variables and productions,
  never by runtime recursive-descent loops;
- inline variables are represented by an `InlineMap` from production step to
  replacement productions; recursive inline variables are rejected;
- every terminal has a lexical variable or external-token correspondence;
- every literal string and regex pattern used in syntax is extracted into the
  lexical grammar;
- a grammar rule whose entire body is a single unique token can be moved from
  syntax grammar to lexical grammar, except the start rule remains a syntax
  variable pointing at that terminal;
- external tokens preserve source order as scanner valid-symbol ordinals;
- an external token that corresponds to an internal token records that terminal
  mapping; an external-only token records no internal terminal;
- `word_token` is a terminal after token extraction, not a nonterminal;
- reserved-word sets are `TokenSet`s over terminals/externals; non-token
  reserved entries are rejected;
- extras are token sets plus separator lexical rules, not ad hoc parse
  productions;
- `token.immediate(...)` terminals are marked so leading extras are forbidden;
- normal tokens may consume separator/extras according to the lexical grammar;
- lexical precedence is separated from parse precedence;
- parse precedence and associativity live on production steps;
- dynamic precedence lives on productions and participates only in runtime GLR
  ambiguity selection;
- conflicts are sets of nonterminals after symbol replacement, sorted and
  deduplicated for stable comparison;
- supertypes are hidden nonterminals retained for node metadata/query semantics;
- alias metadata is step-local unless promoted into `default_aliases`;
- default aliases are computed only when a symbol always appears under an alias,
  and are not promoted through inline variables;
- field ids and alias ids remain stable and provenance-linked;
- every generated auxiliary variable, terminal, production, NFA state, inline
  expansion, and alias promotion records derivation provenance.

## Normalization Passes

### 1. Semantic Validation

Input: `RawGrammarJson`.

Output: `ValidatedGrammar`.

Work:

- allocate source-order nonterminal ids for rules;
- allocate source-order external ids and scanner ordinals;
- resolve all symbol names;
- validate start rule visibility;
- validate precedence names and precedence ordering consistency;
- validate conflicts, inline declarations, supertypes, word, and reserved-set
  names;
- reject indirect single-symbol recursion cycles;
- intern fields and aliases;
- preserve expression arena provenance.

### 2. Token Extraction

Input: `ValidatedGrammar`.

Output: `TokenExtractedGrammar`.

Work:

- extract every `StringToken`, `PatternToken`, `token(...)`, and
  `token.immediate(...)` into lexical variables;
- replace syntactic references to extracted token rules with terminal symbols;
- move single-use whole-token non-start variables into lexical variables when
  Tree-sitter would do so;
- preserve start rule as syntax even if its body is a token;
- split extras into terminal extra symbols and lexical separators;
- validate external declarations as external-only or corresponding-internal
  tokens;
- validate that the word token is a terminal after replacement;
- validate reserved-word entries as token symbols.

### 3. Repeat Expansion

Input: `TokenExtractedGrammar`.

Output: `RepeatExpandedGrammar`.

Work:

- replace `repeat(...)` with generated auxiliary variables;
- turn top-level hidden repetitions into auxiliary recursive variables where
  that matches Tree-sitter's binary-tree repeat shape;
- preserve metadata through generated rules;
- record generated-rule provenance from the original repeat expression.

`repeat1(...)` should either be represented as a required occurrence plus a
generated `repeat(...)` tail before this pass, or get its own equivalent
auxiliary expansion. It must not survive into `ParserGrammar`.

### 4. Production Flattening

Input: `RepeatExpandedGrammar`.

Output: `FlatSyntaxGrammar`.

Work:

- expand choices into alternative productions;
- expand sequences into production step vectors;
- push fields, aliases, reserved contexts, parse precedence, and associativity
  onto the affected steps;
- set production dynamic precedence from the strongest dynamic precedence
  wrapper on that alternative;
- reject used non-start empty syntactic variables;
- reject recursive inline declarations.

### 5. Lexical Expansion

Input: extracted lexical variables and separators.

Output: `LexicalGrammar`.

Work:

- compile string and regex terminal rules into a lexical NFA;
- apply separator/extras NFA before non-immediate tokens;
- forbid separator/extras before immediate tokens;
- preserve integer lexical precedence from `token(prec(N, ...))`;
- apply Tree-sitter's lexical selection ordering: context-aware valid-symbol
  set, lexical precedence, match length, string specificity, then grammar order;
- reject unsupported regex constructs and empty lexical rules.

Named precedence values are parse precedence only. Lexical precedence is integer
metadata on lexical variables.

### 6. Alias And Inline Finalization

Input: flat syntax grammar and lexical grammar.

Output: `ParserGrammar`.

Work:

- derive default aliases for symbols that always appear aliased;
- remove redundant step-local aliases when the default alias covers them;
- build the inlined-production map for declared inline variables;
- preserve inlined origin spans so parser events can still be explained;
- build stable `VisibleNodeTable` and node metadata inputs from named rules,
  named aliases, supertypes, and hidden/anonymous status.

## LR/GLR Preconditions

Before LR/GLR table generation starts, Snark should assert:

- no raw `RawRuleJson` or `GrammarExpr` is reachable from `ParserGrammar`;
- no generated parser implementation file has been read;
- all syntax variables have flat productions;
- all production steps reference symbols in `SymbolTable`;
- all terminal symbols are accepted by the lexical grammar or mapped to an
  external token;
- all external scanner ordinals match the original `externals` order;
- every parse state will be able to construct terminal/external valid-symbol
  masks from the same symbol ids used by scanner calls;
- all declared conflicts are available as nonterminal sets to the table builder;
- all precedence comparisons used by conflict resolution are defined;
- dynamic precedence is retained for GLR branch ranking;
- hidden, anonymous, named, auxiliary, inline, and supertype visibility are
  explicit, not inferred later from strings;
- parser recovery and incremental parse metadata can point back to productions
  and symbols with provenance.

## Current Snark Delta

Current `ValidatedGrammar` already resolves raw rules into ids, interns fields
and aliases, records extras, conflicts, precedence groups, inline declarations,
supertypes, externals, word, reserved sets, and visible node kinds.

The next architectural step is not a small parser. It is to split the remaining
normalization into explicit passes:

1. strengthen `ValidatedGrammar` with start visibility, named precedence,
   precedence-order, indirect-recursion, reserved-token, and provenance
   diagnostics;
2. introduce `ParserGrammar` as the only accepted input to parser-table
   construction;
3. implement token extraction and lexical symbol replacement;
4. implement repeat expansion and production flattening;
5. implement lexical NFA construction and external scanner valid-symbol facts;
6. implement default alias and inline-production maps;
7. build LR/GLR tables from `ParserGrammar`, not from raw grammar expressions.

This keeps the scannerless milestone quarantined as a smoke module. Production
Snark parsing must be grammar normalization plus table generation.
