//! Tree-sitter-style parser generator and LR/GLR runtime scaffolding.
//!
//! This module is the final-shape parser lane. It is deliberately table- and
//! runtime-oriented: validated grammar facts become normalized productions,
//! lexical modes, LR actions, GLR metadata, tree plans, and traceable runtime
//! state. It is not a recursive recognizer and it never consumes generated
//! Tree-sitter implementation files.

use crate::{
    lexical::{LexicalFacts, TerminalKind},
    runtime_input::{ByteRange, PointRange},
    validated::{
        AliasId, FieldId, GrammarExprId, PrecedenceEntry as ValidatedPrecedenceEntry, RuleId,
        ValidatedGrammar, VisibleNodeKind,
    },
};

macro_rules! id_type {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(u32);

        impl $name {
            /// Build a dense id from an arena/table index.
            pub fn from_index(index: usize) -> Self {
                let value = u32::try_from(index).expect("parser id overflow");
                Self(value)
            }

            /// Dense numeric identity.
            pub const fn get(self) -> u32 {
                self.0
            }
        }
    };
}

id_type!(TerminalId, "Terminal symbol id in Snark parser tables.");
id_type!(
    NonterminalId,
    "Nonterminal symbol id in Snark parser tables."
);
id_type!(
    ExternalId,
    "External scanner terminal id in Snark parser tables."
);
id_type!(ParserSymbolId, "Unified parser symbol id.");
id_type!(ProductionId, "Flattened production id.");
id_type!(ProductionMetadataId, "Production metadata id.");
id_type!(ParseStateId, "LR parse state id.");
id_type!(LexModeId, "Lexical mode id derived from parser states.");
id_type!(ConflictId, "Declared or generated conflict id.");
id_type!(ItemSetId, "LR item-set id.");
id_type!(StackVersionId, "GLR stack-version id.");
id_type!(GraphStackNodeId, "GLR graph-stack node id.");
id_type!(TreeNodeId, "Runtime tree node id.");
id_type!(TraceEventId, "Structured parser trace event id.");
id_type!(InternalSymbolId, "Internal parser sentinel symbol id.");
id_type!(ReservedContextId, "Reserved-word context id.");
id_type!(ValidSymbolSetId, "External scanner valid-symbol-set id.");
id_type!(ScannerSnapshotId, "Serialized external scanner state id.");
id_type!(LookaheadTokenId, "Branch-local lookahead token id.");
id_type!(QueryPatternId, "Query pattern id.");
id_type!(QueryCaptureId, "Query capture id.");
id_type!(ProvenanceId, "Parser-generation provenance id.");
id_type!(FieldMapId, "Production field-map id.");
id_type!(AliasSequenceId, "Production alias-sequence id.");
id_type!(PublicNodeKindId, "Public node-kind id.");
id_type!(HighlightAssertionId, "Highlight assertion oracle id.");
id_type!(PrecedenceGroupId, "Static precedence group id.");

/// Generation phase represented by a parser-machine value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ParserGenerationStage {
    /// Symbol domains have been seeded from validated grammar and lexical facts.
    SymbolDomains,
    /// Grammar expressions have been normalized into productions.
    Productions,
    /// LR item sets and action/goto tables have been generated.
    Tables,
    /// Runtime tree, scanner, recovery, and query plans have been attached.
    RuntimePlans,
}

/// Parser-generator input after validated grammar facts enter the parser lane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParserGrammar {
    stage: ParserGenerationStage,
    start: NonterminalId,
    symbols: SymbolTables,
    productions: Vec<Production>,
    production_metadata: Vec<ProductionMetadata>,
    field_maps: Vec<FieldMap>,
    alias_sequences: Vec<AliasSequence>,
    lexical_modes: Vec<LexMode>,
    reserved_contexts: Vec<ReservedContext>,
    valid_symbol_sets: Vec<ValidSymbolSet>,
    extra_roots: Vec<ExtraRoot>,
    word: Option<NonterminalId>,
    supertypes: Vec<NonterminalId>,
    precedence_groups: Vec<PrecedenceGroup>,
    public_node_kinds: Vec<PublicNodeKind>,
    glr_plan: GlrPlan,
}

impl ParserGrammar {
    /// Seed parser symbol domains from validated grammar and lexical facts.
    ///
    /// This is not production lowering. The next parser-generator phase must
    /// flatten `ValidatedGrammar` expressions into [`Production`] rows before
    /// item sets or runtime execution are valid.
    pub fn seed_from_validated(grammar: &ValidatedGrammar, lexical: &LexicalFacts) -> Self {
        let nonterminals = grammar
            .rules()
            .map(|rule| NonterminalSymbol {
                id: NonterminalId::from_index(rule.id().get() as usize),
                rule: rule.id(),
                name: rule.name().as_str().to_owned(),
                visible: rule.visible() && !grammar.inline().contains(&rule.id()),
                inline: grammar.inline().contains(&rule.id()),
            })
            .collect::<Vec<_>>();
        let terminals = lexical
            .terminals()
            .iter()
            .enumerate()
            .map(|(index, terminal)| TerminalSymbol {
                id: TerminalId::from_index(index),
                kind: terminal.kind,
                spelling: terminal.spelling.clone(),
            })
            .collect::<Vec<_>>();
        let externals = lexical
            .external_tokens()
            .iter()
            .enumerate()
            .map(|(index, external)| ExternalSymbol {
                id: ExternalId::from_index(index),
                ordinal: external.ordinal().get(),
                name: external.name().map(str::to_owned),
            })
            .collect::<Vec<_>>();
        let reserved_contexts = grammar
            .reserved_sets()
            .iter()
            .enumerate()
            .map(|(index, reserved)| ReservedContext {
                id: ReservedContextId::from_index(index),
                name: reserved.name().to_owned(),
                entries: reserved.entries().to_vec(),
            })
            .collect::<Vec<_>>();
        let extra_roots = grammar
            .extras()
            .iter()
            .copied()
            .map(|expr| ExtraRoot { expr })
            .collect::<Vec<_>>();
        let word = grammar
            .word()
            .map(|rule| NonterminalId::from_index(rule.get() as usize));
        let supertypes = grammar
            .supertypes()
            .iter()
            .map(|rule| NonterminalId::from_index(rule.get() as usize))
            .collect::<Vec<_>>();
        let precedence_groups = grammar
            .precedence_groups()
            .iter()
            .enumerate()
            .map(|(index, entries)| PrecedenceGroup {
                id: PrecedenceGroupId::from_index(index),
                entries: entries
                    .iter()
                    .map(|entry| match entry {
                        ValidatedPrecedenceEntry::Name(name) => {
                            PrecedenceGroupEntry::Name(name.clone())
                        }
                        ValidatedPrecedenceEntry::Symbol(rule) => {
                            PrecedenceGroupEntry::Nonterminal(NonterminalId::from_index(
                                rule.get() as usize
                            ))
                        }
                    })
                    .collect(),
            })
            .collect::<Vec<_>>();
        let public_node_kinds = grammar
            .visible_node_kinds()
            .enumerate()
            .map(|(index, name)| {
                let source = match grammar
                    .visible_node_kind(name)
                    .expect("visible kind exists")
                {
                    VisibleNodeKind::Rule(rule) => {
                        PublicNodeKindSource::Rule(NonterminalId::from_index(rule.get() as usize))
                    }
                    VisibleNodeKind::Alias(alias) => PublicNodeKindSource::Alias(alias),
                };
                PublicNodeKind {
                    id: PublicNodeKindId::from_index(index),
                    name: name.to_owned(),
                    source,
                }
            })
            .collect::<Vec<_>>();
        let conflict_plans = grammar
            .conflicts()
            .iter()
            .enumerate()
            .map(|(index, symbols)| ConflictPlan {
                id: ConflictId::from_index(index),
                symbols: symbols
                    .iter()
                    .map(|rule| NonterminalId::from_index(rule.get() as usize))
                    .collect(),
            })
            .collect::<Vec<_>>();
        Self {
            stage: ParserGenerationStage::SymbolDomains,
            start: NonterminalId::from_index(grammar.start_rule().get() as usize),
            symbols: SymbolTables {
                terminals,
                nonterminals,
                externals,
                eof: EofSymbol,
                internal: vec![
                    InternalSymbol {
                        id: InternalSymbolId::from_index(0),
                        kind: InternalSymbolKind::Error,
                    },
                    InternalSymbol {
                        id: InternalSymbolId::from_index(1),
                        kind: InternalSymbolKind::Missing,
                    },
                    InternalSymbol {
                        id: InternalSymbolId::from_index(2),
                        kind: InternalSymbolKind::Recovery,
                    },
                ],
            },
            productions: Vec::new(),
            production_metadata: Vec::new(),
            field_maps: Vec::new(),
            alias_sequences: Vec::new(),
            lexical_modes: Vec::new(),
            reserved_contexts,
            valid_symbol_sets: Vec::new(),
            extra_roots,
            word,
            supertypes,
            precedence_groups,
            public_node_kinds,
            glr_plan: GlrPlan {
                conflicts: conflict_plans,
            },
        }
    }

    /// Current parser-generation phase.
    pub const fn stage(&self) -> ParserGenerationStage {
        self.stage
    }

    /// Start nonterminal.
    pub const fn start(&self) -> NonterminalId {
        self.start
    }

    /// Symbol tables.
    pub const fn symbols(&self) -> &SymbolTables {
        &self.symbols
    }

    /// Flattened productions.
    pub fn productions(&self) -> &[Production] {
        &self.productions
    }

    /// Production metadata rows keyed by [`ProductionMetadataId`].
    pub fn production_metadata(&self) -> &[ProductionMetadata] {
        &self.production_metadata
    }

    /// Field maps keyed by [`FieldMapId`].
    pub fn field_maps(&self) -> &[FieldMap] {
        &self.field_maps
    }

    /// Alias sequences keyed by [`AliasSequenceId`].
    pub fn alias_sequences(&self) -> &[AliasSequence] {
        &self.alias_sequences
    }

    /// Lexical modes attached to parser states.
    pub fn lexical_modes(&self) -> &[LexMode] {
        &self.lexical_modes
    }

    /// Reserved-word contexts referenced by lexical modes and productions.
    pub fn reserved_contexts(&self) -> &[ReservedContext] {
        &self.reserved_contexts
    }

    /// External scanner valid-symbol sets referenced by lexical modes.
    pub fn valid_symbol_sets(&self) -> &[ValidSymbolSet] {
        &self.valid_symbol_sets
    }

    /// Extra grammar roots.
    pub fn extra_roots(&self) -> &[ExtraRoot] {
        &self.extra_roots
    }

    /// Optional word token nonterminal.
    pub const fn word(&self) -> Option<NonterminalId> {
        self.word
    }

    /// Supertype nonterminals.
    pub fn supertypes(&self) -> &[NonterminalId] {
        &self.supertypes
    }

    /// Static precedence groups.
    pub fn precedence_groups(&self) -> &[PrecedenceGroup] {
        &self.precedence_groups
    }

    /// Public visible node kinds.
    pub fn public_node_kinds(&self) -> &[PublicNodeKind] {
        &self.public_node_kinds
    }

    /// GLR conflict/recovery plan facts.
    pub const fn glr_plan(&self) -> &GlrPlan {
        &self.glr_plan
    }
}

/// Parser symbol domains.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolTables {
    terminals: Vec<TerminalSymbol>,
    nonterminals: Vec<NonterminalSymbol>,
    externals: Vec<ExternalSymbol>,
    eof: EofSymbol,
    internal: Vec<InternalSymbol>,
}

impl SymbolTables {
    /// Terminal symbols.
    pub fn terminals(&self) -> &[TerminalSymbol] {
        &self.terminals
    }

    /// Nonterminal symbols.
    pub fn nonterminals(&self) -> &[NonterminalSymbol] {
        &self.nonterminals
    }

    /// External scanner symbols.
    pub fn externals(&self) -> &[ExternalSymbol] {
        &self.externals
    }

    /// EOF sentinel symbol.
    pub const fn eof(&self) -> EofSymbol {
        self.eof
    }

    /// Internal sentinel symbols such as error and missing.
    pub fn internal(&self) -> &[InternalSymbol] {
        &self.internal
    }
}

/// EOF sentinel symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EofSymbol;

/// Internal parser sentinel symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InternalSymbol {
    id: InternalSymbolId,
    kind: InternalSymbolKind,
}

impl InternalSymbol {
    /// Internal symbol id.
    pub const fn id(&self) -> InternalSymbolId {
        self.id
    }

    /// Internal symbol kind.
    pub const fn kind(&self) -> InternalSymbolKind {
        self.kind
    }
}

/// Internal parser sentinel kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum InternalSymbolKind {
    /// Error node/recovery sentinel.
    Error,
    /// Missing node sentinel.
    Missing,
    /// Generated recovery sentinel.
    Recovery,
}

/// Parser symbol in a normalized production.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ParserSymbol {
    /// Normal lexical terminal.
    Terminal(TerminalId),
    /// Grammar nonterminal.
    Nonterminal(NonterminalId),
    /// External scanner terminal.
    External(ExternalId),
    /// End of file.
    Eof,
    /// Internal sentinel.
    Internal(InternalSymbolId),
}

/// Lookahead key in an action table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum LookaheadSymbol {
    /// Normal lexical terminal.
    Terminal(TerminalId),
    /// External scanner terminal.
    External(ExternalId),
    /// End of file.
    Eof,
    /// Reserved-word-sensitive terminal in a context.
    ReservedWord {
        /// Terminal selected by lexing.
        terminal: TerminalId,
        /// Reserved-word context active for this table edge.
        context: ReservedContextId,
    },
    /// Generated error recovery lookahead.
    ErrorRecovery(InternalSymbolId),
}

/// Terminal symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalSymbol {
    id: TerminalId,
    kind: TerminalKind,
    spelling: String,
}

impl TerminalSymbol {
    /// Terminal id.
    pub const fn id(&self) -> TerminalId {
        self.id
    }

    /// Terminal kind.
    pub const fn kind(&self) -> TerminalKind {
        self.kind
    }

    /// Literal spelling or regex source.
    pub fn spelling(&self) -> &str {
        &self.spelling
    }
}

/// Nonterminal symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NonterminalSymbol {
    id: NonterminalId,
    rule: RuleId,
    name: String,
    visible: bool,
    inline: bool,
}

impl NonterminalSymbol {
    /// Nonterminal id.
    pub const fn id(&self) -> NonterminalId {
        self.id
    }

    /// Source validated rule id.
    pub const fn rule(&self) -> RuleId {
        self.rule
    }

    /// Source rule name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Whether this symbol emits a visible node by default.
    pub const fn visible(&self) -> bool {
        self.visible
    }

    /// Whether this rule is inlined before table generation.
    pub const fn inline(&self) -> bool {
        self.inline
    }
}

/// External scanner symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalSymbol {
    id: ExternalId,
    ordinal: u32,
    name: Option<String>,
}

impl ExternalSymbol {
    /// External parser symbol id.
    pub const fn id(&self) -> ExternalId {
        self.id
    }

    /// External scanner ordinal from `externals`.
    pub const fn ordinal(&self) -> u32 {
        self.ordinal
    }

    /// Optional external token name.
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
}

/// One flattened production used by LR table generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Production {
    id: ProductionId,
    lhs: NonterminalId,
    steps: Vec<ProductionStep>,
    dynamic_precedence: i32,
    metadata: ProductionMetadataId,
}

impl Production {
    /// Production id.
    pub const fn id(&self) -> ProductionId {
        self.id
    }

    /// Left-hand nonterminal.
    pub const fn lhs(&self) -> NonterminalId {
        self.lhs
    }

    /// Ordered production steps.
    pub fn steps(&self) -> &[ProductionStep] {
        &self.steps
    }

    /// Dynamic precedence accumulated by this reduction.
    pub const fn dynamic_precedence(&self) -> i32 {
        self.dynamic_precedence
    }

    /// Production metadata id.
    pub const fn metadata(&self) -> ProductionMetadataId {
        self.metadata
    }
}

/// One structural step in a flattened production.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductionStep {
    symbol: ParserSymbol,
    field: Option<FieldId>,
    alias: Option<AliasId>,
    structural_index: usize,
}

impl ProductionStep {
    /// Symbol consumed by this production step.
    pub const fn symbol(&self) -> ParserSymbol {
        self.symbol
    }

    /// Field applied at this structural child index.
    pub const fn field(&self) -> Option<FieldId> {
        self.field
    }

    /// Alias applied at this structural child index.
    pub const fn alias(&self) -> Option<AliasId> {
        self.alias
    }

    /// Structural child index used for fields and aliases.
    pub const fn structural_index(&self) -> usize {
        self.structural_index
    }
}

/// Metadata attached to one production.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductionMetadata {
    id: ProductionMetadataId,
    owner: RuleId,
    public_node: Option<PublicNodeKindId>,
    field_map: Option<FieldMapId>,
    alias_sequence: Option<AliasSequenceId>,
    origin: ProductionOrigin,
    static_precedence: Option<StaticPrecedence>,
    associativity: Associativity,
    dynamic_precedence: i32,
    reserved_context: Option<ReservedContextId>,
    provenance: Option<ProvenanceId>,
}

impl ProductionMetadata {
    /// Metadata id.
    pub const fn id(&self) -> ProductionMetadataId {
        self.id
    }

    /// Rule that owns this production before auxiliary expansion.
    pub const fn owner(&self) -> RuleId {
        self.owner
    }

    /// Public node emitted by this production, if any.
    pub const fn public_node(&self) -> Option<PublicNodeKindId> {
        self.public_node
    }

    /// Production-keyed field map.
    pub const fn field_map(&self) -> Option<FieldMapId> {
        self.field_map
    }

    /// Production-keyed alias sequence.
    pub const fn alias_sequence(&self) -> Option<AliasSequenceId> {
        self.alias_sequence
    }

    /// How this production was introduced.
    pub const fn origin(&self) -> ProductionOrigin {
        self.origin
    }

    /// Static precedence used for conflict resolution.
    pub const fn static_precedence(&self) -> Option<&StaticPrecedence> {
        self.static_precedence.as_ref()
    }

    /// Associativity used for equal-precedence conflicts.
    pub const fn associativity(&self) -> Associativity {
        self.associativity
    }

    /// Dynamic precedence applied to the reduced subtree.
    pub const fn dynamic_precedence(&self) -> i32 {
        self.dynamic_precedence
    }

    /// Reserved-word context active for this production.
    pub const fn reserved_context(&self) -> Option<ReservedContextId> {
        self.reserved_context
    }

    /// Provenance row for diagnostics and trace output.
    pub const fn provenance(&self) -> Option<ProvenanceId> {
        self.provenance
    }
}

/// How a production entered the normalized grammar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ProductionOrigin {
    /// Production came directly from a grammar rule.
    Rule,
    /// Production was introduced while expanding a repeat.
    Repeat,
    /// Production was introduced while inlining a rule.
    Inline,
    /// Production was introduced for the augmented start rule.
    AugmentedStart,
    /// Production was introduced for error recovery.
    Recovery,
}

/// Field map attached to one production.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldMap {
    id: FieldMapId,
    entries: Vec<FieldMapEntry>,
}

impl FieldMap {
    /// Field-map id.
    pub const fn id(&self) -> FieldMapId {
        self.id
    }

    /// Field entries keyed by structural child index.
    pub fn entries(&self) -> &[FieldMapEntry] {
        &self.entries
    }
}

/// One field attachment in a production field map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldMapEntry {
    structural_index: usize,
    field: FieldId,
}

impl FieldMapEntry {
    /// Structural child index.
    pub const fn structural_index(&self) -> usize {
        self.structural_index
    }

    /// Field attached at this index.
    pub const fn field(&self) -> FieldId {
        self.field
    }
}

/// Alias sequence attached to one production.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AliasSequence {
    id: AliasSequenceId,
    entries: Vec<AliasSequenceEntry>,
}

impl AliasSequence {
    /// Alias-sequence id.
    pub const fn id(&self) -> AliasSequenceId {
        self.id
    }

    /// Alias entries keyed by structural child index.
    pub fn entries(&self) -> &[AliasSequenceEntry] {
        &self.entries
    }
}

/// One alias attachment in a production alias sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AliasSequenceEntry {
    structural_index: usize,
    alias: AliasId,
}

impl AliasSequenceEntry {
    /// Structural child index.
    pub const fn structural_index(&self) -> usize {
        self.structural_index
    }

    /// Alias attached at this index.
    pub const fn alias(&self) -> AliasId {
        self.alias
    }
}

/// Public visible node kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicNodeKind {
    id: PublicNodeKindId,
    name: String,
    source: PublicNodeKindSource,
}

impl PublicNodeKind {
    /// Public node-kind id.
    pub const fn id(&self) -> PublicNodeKindId {
        self.id
    }

    /// Node kind name as observed in S-expressions and queries.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Source that introduced this public node kind.
    pub const fn source(&self) -> PublicNodeKindSource {
        self.source
    }
}

/// Source of a public visible node kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PublicNodeKindSource {
    /// Visible grammar rule.
    Rule(NonterminalId),
    /// Named alias.
    Alias(AliasId),
}

/// Extra grammar root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtraRoot {
    expr: GrammarExprId,
}

impl ExtraRoot {
    /// Extra expression root.
    pub const fn expr(&self) -> GrammarExprId {
        self.expr
    }
}

/// Static precedence group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrecedenceGroup {
    id: PrecedenceGroupId,
    entries: Vec<PrecedenceGroupEntry>,
}

impl PrecedenceGroup {
    /// Precedence group id.
    pub const fn id(&self) -> PrecedenceGroupId {
        self.id
    }

    /// Ordered precedence entries.
    pub fn entries(&self) -> &[PrecedenceGroupEntry] {
        &self.entries
    }
}

/// One static precedence group entry.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum PrecedenceGroupEntry {
    /// Named precedence entry.
    Name(String),
    /// Rule/nonterminal precedence entry.
    Nonterminal(NonterminalId),
}

/// Static precedence fact.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum StaticPrecedence {
    /// Integer precedence.
    Integer(i32),
    /// Named precedence.
    Named(String),
}

/// Production associativity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Associativity {
    /// No associativity override.
    None,
    /// Left associative.
    Left,
    /// Right associative.
    Right,
}

/// Reserved-word context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReservedContext {
    id: ReservedContextId,
    name: String,
    entries: Vec<GrammarExprId>,
}

impl ReservedContext {
    /// Reserved context id.
    pub const fn id(&self) -> ReservedContextId {
        self.id
    }

    /// Reserved context name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Reserved word entry expressions.
    pub fn entries(&self) -> &[GrammarExprId] {
        &self.entries
    }
}

/// External scanner valid-symbol set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidSymbolSet {
    id: ValidSymbolSetId,
    externals: Vec<ExternalId>,
}

impl ValidSymbolSet {
    /// Valid-symbol-set id.
    pub const fn id(&self) -> ValidSymbolSetId {
        self.id
    }

    /// External symbols enabled in this set.
    pub fn externals(&self) -> &[ExternalId] {
        &self.externals
    }
}

/// LR item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LrItem {
    production: ProductionId,
    dot: usize,
    lookahead: LookaheadSet,
}

impl LrItem {
    /// Production being recognized.
    pub const fn production(&self) -> ProductionId {
        self.production
    }

    /// Dot position.
    pub const fn dot(&self) -> usize {
        self.dot
    }

    /// Lookahead set.
    pub const fn lookahead(&self) -> &LookaheadSet {
        &self.lookahead
    }
}

/// Set of lookahead terminal/external/EOF symbols.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LookaheadSet {
    symbols: Vec<LookaheadSymbol>,
}

impl LookaheadSet {
    /// Lookahead symbols.
    pub fn symbols(&self) -> &[LookaheadSymbol] {
        &self.symbols
    }
}

/// One LR item set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemSet {
    id: ItemSetId,
    items: Vec<LrItem>,
}

impl ItemSet {
    /// Item-set id.
    pub const fn id(&self) -> ItemSetId {
        self.id
    }

    /// Items in this set.
    pub fn items(&self) -> &[LrItem] {
        &self.items
    }
}

/// Lexical mode selected by a parser state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexMode {
    id: LexModeId,
    terminals: Vec<TerminalId>,
    externals: Vec<ExternalId>,
    reserved_context: Option<ReservedContextId>,
    valid_symbols: Option<ValidSymbolSetId>,
    word: Option<TerminalId>,
}

impl LexMode {
    /// Lexical mode id.
    pub const fn id(&self) -> LexModeId {
        self.id
    }

    /// Normal terminal candidates.
    pub fn terminals(&self) -> &[TerminalId] {
        &self.terminals
    }

    /// External scanner candidates.
    pub fn externals(&self) -> &[ExternalId] {
        &self.externals
    }

    /// Reserved-word context active in this mode.
    pub const fn reserved_context(&self) -> Option<ReservedContextId> {
        self.reserved_context
    }

    /// External scanner valid-symbol set active in this mode.
    pub const fn valid_symbols(&self) -> Option<ValidSymbolSetId> {
        self.valid_symbols
    }

    /// Word token used for keyword/reserved-word rewrites.
    pub const fn word(&self) -> Option<TerminalId> {
        self.word
    }
}

/// Concrete branch-local lookahead token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LookaheadToken {
    id: LookaheadTokenId,
    symbol: LookaheadSymbol,
    bytes: ByteRange,
    points: PointRange,
    lexical_precedence: i32,
    tie_break: LexicalTieBreak,
    extra: bool,
    immediate: bool,
    keyword: KeywordStatus,
    scanner_snapshot: Option<ScannerSnapshotId>,
}

impl LookaheadToken {
    /// Lookahead token id.
    pub const fn id(&self) -> LookaheadTokenId {
        self.id
    }

    /// Symbol selected by lexing.
    pub const fn symbol(&self) -> LookaheadSymbol {
        self.symbol
    }

    /// Byte range consumed by this token.
    pub const fn bytes(&self) -> ByteRange {
        self.bytes
    }

    /// Point range consumed by this token.
    pub const fn points(&self) -> PointRange {
        self.points
    }

    /// Lexical precedence used for token selection.
    pub const fn lexical_precedence(&self) -> i32 {
        self.lexical_precedence
    }

    /// Lexical tie-break facts.
    pub const fn tie_break(&self) -> LexicalTieBreak {
        self.tie_break
    }

    /// Whether the token is an extra.
    pub const fn extra(&self) -> bool {
        self.extra
    }

    /// Whether the token was accepted by an immediate lexical rule.
    pub const fn immediate(&self) -> bool {
        self.immediate
    }

    /// Keyword/reserved-word rewrite status.
    pub const fn keyword(&self) -> KeywordStatus {
        self.keyword
    }

    /// External scanner snapshot after accepting this token.
    pub const fn scanner_snapshot(&self) -> Option<ScannerSnapshotId> {
        self.scanner_snapshot
    }
}

/// Stable facts used after lexical precedence ties.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LexicalTieBreak {
    byte_len: u32,
    source_order: u32,
}

impl LexicalTieBreak {
    /// Accepted token byte length.
    pub const fn byte_len(&self) -> u32 {
        self.byte_len
    }

    /// Source-order tie-breaker.
    pub const fn source_order(&self) -> u32 {
        self.source_order
    }
}

/// Keyword or reserved-word rewrite status for a lookahead token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum KeywordStatus {
    /// Token was not checked against the word/reserved system.
    Unchecked,
    /// Token remained the word token.
    Word,
    /// Token was rewritten to a keyword/reserved terminal.
    Rewritten,
    /// Token was rejected by the active reserved context.
    ReservedRejected,
}

/// Generated LR/GLR parse table.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParseTable {
    states: Vec<ParseState>,
}

impl ParseTable {
    /// Parse states.
    pub fn states(&self) -> &[ParseState] {
        &self.states
    }
}

/// One generated parse state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseState {
    id: ParseStateId,
    item_set: ItemSetId,
    entries: Vec<TableEntry>,
    gotos: Vec<GotoEntry>,
    lex_mode: LexModeId,
}

impl ParseState {
    /// Parse state id.
    pub const fn id(&self) -> ParseStateId {
        self.id
    }

    /// Item-set represented by this state.
    pub const fn item_set(&self) -> ItemSetId {
        self.item_set
    }

    /// Terminal action entries.
    pub fn entries(&self) -> &[TableEntry] {
        &self.entries
    }

    /// Nonterminal goto entries.
    pub fn gotos(&self) -> &[GotoEntry] {
        &self.gotos
    }

    /// Lexical mode selected by this state.
    pub const fn lex_mode(&self) -> LexModeId {
        self.lex_mode
    }
}

/// Actions for one lookahead symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableEntry {
    lookahead: LookaheadSymbol,
    actions: Vec<ParseAction>,
}

impl TableEntry {
    /// Lookahead symbol.
    pub const fn lookahead(&self) -> LookaheadSymbol {
        self.lookahead
    }

    /// Actions retained for this lookahead.
    pub fn actions(&self) -> &[ParseAction] {
        &self.actions
    }
}

/// Nonterminal goto entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GotoEntry {
    nonterminal: NonterminalId,
    state: ParseStateId,
}

impl GotoEntry {
    /// Reduced nonterminal.
    pub const fn nonterminal(&self) -> NonterminalId {
        self.nonterminal
    }

    /// State reached after reducing the nonterminal.
    pub const fn state(&self) -> ParseStateId {
        self.state
    }
}

/// Parser action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ParseAction {
    /// Accept the input.
    Accept,
    /// Shift the current lookahead.
    Shift {
        /// Target parse state.
        state: ParseStateId,
        /// Whether this shift belongs to generated repetition handling.
        repetition: bool,
    },
    /// Shift an extra token without changing structural state.
    ShiftExtra,
    /// Reduce a production.
    Reduce {
        /// Production to reduce.
        production: ProductionId,
        /// Metadata row attached to the production.
        metadata: ProductionMetadataId,
        /// Reduced nonterminal.
        symbol: NonterminalId,
        /// Structural child count.
        child_count: usize,
        /// Dynamic precedence attached to the reduced subtree.
        dynamic_precedence: i32,
    },
    /// Enter generated error recovery.
    Recover,
}

/// GLR runtime table facts that are not specific to one stack version.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GlrPlan {
    conflicts: Vec<ConflictPlan>,
}

impl GlrPlan {
    /// Declared/generated conflict plans.
    pub fn conflicts(&self) -> &[ConflictPlan] {
        &self.conflicts
    }
}

/// One GLR conflict plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictPlan {
    id: ConflictId,
    symbols: Vec<NonterminalId>,
}

impl ConflictPlan {
    /// Conflict id.
    pub const fn id(&self) -> ConflictId {
        self.id
    }

    /// Nonterminal symbols participating in this conflict.
    pub fn symbols(&self) -> &[NonterminalId] {
        &self.symbols
    }
}

/// Key used when deciding whether GLR stack versions can merge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StackMergeKey {
    state: ParseStateId,
    byte_position: u32,
    scanner_snapshot: Option<ScannerSnapshotId>,
}

impl StackMergeKey {
    /// Parse state at stack head.
    pub const fn state(&self) -> ParseStateId {
        self.state
    }

    /// Input byte position at stack head.
    pub const fn byte_position(&self) -> u32 {
        self.byte_position
    }

    /// External scanner snapshot compatible with this stack version.
    pub const fn scanner_snapshot(&self) -> Option<ScannerSnapshotId> {
        self.scanner_snapshot
    }
}

/// Branch-local ranking and liveness facts retained after merge checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BranchRanking {
    lookahead: Option<LookaheadTokenId>,
    error_cost: u32,
    dynamic_precedence: i32,
    active: bool,
}

impl BranchRanking {
    /// Branch-local lookahead token.
    pub const fn lookahead(&self) -> Option<LookaheadTokenId> {
        self.lookahead
    }

    /// Accumulated error cost.
    pub const fn error_cost(&self) -> u32 {
        self.error_cost
    }

    /// Accumulated dynamic precedence.
    pub const fn dynamic_precedence(&self) -> i32 {
        self.dynamic_precedence
    }

    /// Whether this branch remains active.
    pub const fn active(&self) -> bool {
        self.active
    }
}

/// Runtime tree operation emitted by parser actions.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum TreeEvent {
    /// A node was opened.
    OpenNode {
        /// Runtime tree node.
        node: TreeNodeId,
        /// Public or internal symbol.
        symbol: ParserSymbol,
        /// Whether this node is visible in public traversal.
        visible: bool,
        /// Whether this node is named.
        named: bool,
    },
    /// A token was shifted.
    Token {
        /// Token symbol.
        symbol: ParserSymbol,
        /// Lookahead token source.
        lookahead: LookaheadTokenId,
        /// Byte range.
        bytes: ByteRange,
        /// Point range.
        points: PointRange,
        /// Whether this token is an extra.
        extra: bool,
        /// Whether this token is named in public traversal.
        named: bool,
        /// Keyword/reserved-word rewrite status.
        keyword: KeywordStatus,
    },
    /// A production was reduced into a parent node.
    Reduce {
        /// Reduced production.
        production: ProductionId,
        /// Reduced production metadata.
        metadata: ProductionMetadataId,
        /// Runtime tree node.
        node: TreeNodeId,
        /// Byte range.
        bytes: ByteRange,
        /// Point range.
        points: PointRange,
    },
    /// A missing token was inserted by recovery.
    Missing {
        /// Missing symbol.
        symbol: ParserSymbol,
        /// Byte range.
        bytes: ByteRange,
        /// Point range.
        points: PointRange,
    },
    /// An error node was emitted.
    Error {
        /// Runtime tree node.
        node: TreeNodeId,
        /// Byte range.
        bytes: ByteRange,
        /// Point range.
        points: PointRange,
        /// Accumulated error cost at this node.
        error_cost: u32,
    },
    /// A node was finished.
    CloseNode {
        /// Runtime tree node.
        node: TreeNodeId,
        /// Byte range.
        bytes: ByteRange,
        /// Point range.
        points: PointRange,
    },
    /// A reusable subtree was accepted.
    ReuseNode {
        /// Runtime tree node.
        node: TreeNodeId,
        /// Byte range.
        bytes: ByteRange,
        /// Point range.
        points: PointRange,
        /// Scanner snapshot required for reuse.
        scanner_snapshot: Option<ScannerSnapshotId>,
    },
    /// A field was attached at a structural child index.
    Field {
        /// Parent runtime tree node.
        node: TreeNodeId,
        /// Field id.
        field: FieldId,
        /// Structural child index.
        structural_index: usize,
    },
    /// An alias was attached at a structural child index.
    Alias {
        /// Parent runtime tree node.
        node: TreeNodeId,
        /// Alias id.
        alias: AliasId,
        /// Structural child index.
        structural_index: usize,
    },
}

/// Structured parser trace event.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum TraceEvent {
    /// Parse started.
    ParseStart {
        /// Trace event id.
        id: TraceEventId,
        /// Start state.
        state: ParseStateId,
    },
    /// Parse finished.
    ParseFinish {
        /// Trace event id.
        id: TraceEventId,
        /// Outcome.
        outcome: ParseOutcome,
    },
    /// Parser entered a state on one branch.
    StateEnter {
        /// Trace event id.
        id: TraceEventId,
        /// Stack version.
        version: StackVersionId,
        /// State entered.
        state: ParseStateId,
    },
    /// A lexical mode produced a branch-local lookahead.
    Lex {
        /// Stack version.
        version: StackVersionId,
        /// Lexical mode.
        mode: LexModeId,
        /// Produced lookahead token.
        lookahead: LookaheadTokenId,
    },
    /// The external scanner was invoked.
    ExternalScanner {
        /// Stack version.
        version: StackVersionId,
        /// Valid symbols offered.
        valid_symbols: ValidSymbolSetId,
        /// Scanner snapshot before the call.
        before: Option<ScannerSnapshotId>,
        /// Scanner snapshot after the call.
        after: Option<ScannerSnapshotId>,
        /// Lookahead token accepted by the scanner.
        result: Option<LookaheadTokenId>,
    },
    /// A shift action executed.
    Shift {
        /// Stack version.
        version: StackVersionId,
        /// Lookahead token shifted.
        lookahead: LookaheadTokenId,
        /// Target parse state.
        state: ParseStateId,
    },
    /// A reduce action executed.
    Reduce {
        /// Stack version.
        version: StackVersionId,
        /// Reduced production.
        production: ProductionId,
        /// Production metadata.
        metadata: ProductionMetadataId,
    },
    /// A GLR branch split.
    GlrSplit {
        /// Source version.
        source: StackVersionId,
        /// Conflict that caused the split.
        conflict: ConflictId,
        /// Branches created by the split.
        branches: Vec<StackVersionId>,
    },
    /// GLR branches merged.
    GlrMerge {
        /// Surviving version.
        survivor: StackVersionId,
        /// Retired version.
        retired: StackVersionId,
        /// Merge key.
        key: StackMergeKey,
        /// Ranking retained for the surviving branch.
        ranking: BranchRanking,
    },
    /// A GLR branch was retired.
    GlrRetire {
        /// Retired version.
        version: StackVersionId,
        /// Reason for retirement.
        reason: BranchRetireReason,
    },
    /// Recovery emitted parser work.
    Recover {
        /// Stack version.
        version: StackVersionId,
        /// State being recovered.
        state: ParseStateId,
    },
    /// A tree event was emitted.
    Tree(TreeEvent),
    /// A query capture was emitted.
    QueryCapture {
        /// Query pattern.
        pattern: QueryPatternId,
        /// Query capture.
        capture: QueryCaptureId,
        /// Capture name.
        capture_name: String,
        /// Captured tree node.
        node: TreeNodeId,
        /// Captured byte range.
        bytes: ByteRange,
        /// Captured point range.
        points: PointRange,
        /// Predicate outcome for this capture.
        predicates: PredicateOutcome,
        /// Highlight assertion matched by this capture, if any.
        highlight_assertion: Option<HighlightAssertionId>,
    },
}

/// Parse outcome for trace/oracle events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ParseOutcome {
    /// Input was accepted without recovery cost.
    Accepted,
    /// Input was accepted with recovery.
    Recovered,
    /// Input could not be parsed.
    Failed,
}

/// Why a GLR branch was retired.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum BranchRetireReason {
    /// The branch was dominated by a lower-cost branch.
    Dominated,
    /// The branch reached an impossible action.
    NoAction,
    /// The branch exceeded configured runtime limits.
    Limit,
}

/// Query predicate outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PredicateOutcome {
    /// No predicates were attached.
    None,
    /// All predicates accepted the capture.
    Accepted,
    /// At least one predicate rejected the capture.
    Rejected,
    /// Predicate execution is not represented yet.
    Unknown,
}
