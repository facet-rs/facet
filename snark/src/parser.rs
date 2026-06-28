//! Tree-sitter-style parser generator and LR/GLR runtime scaffolding.
//!
//! This module is the final-shape parser lane. It is deliberately table- and
//! runtime-oriented: validated grammar facts become normalized productions,
//! lexical modes, LR actions, GLR metadata, tree plans, and traceable runtime
//! state. It is not a recursive recognizer and it never consumes generated
//! Tree-sitter implementation files.

use std::{collections::HashMap, error::Error, fmt};

use crate::{
    lexical::{LexicalFacts, TerminalKind},
    runtime_input::{ByteRange, PointRange},
    validated::{
        AliasId, FieldId, GrammarExpr, GrammarExprId, PrecedenceAssoc,
        PrecedenceEntry as ValidatedPrecedenceEntry, ReservedSetId, RuleId, StaticPrecedenceValue,
        SymbolRef, ValidatedGrammar, VisibleNodeKind,
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
id_type!(LexicalRuleId, "Parser-owned lexical rule id.");
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
    provenances: Vec<Provenance>,
    fields: Vec<FieldDecl>,
    aliases: Vec<AliasDecl>,
    lexical_rules: Vec<LexicalRule>,
    inline_rules: Vec<InlineRule>,
    lexical_modes: Vec<LexMode>,
    reserved_contexts: Vec<ReservedContext>,
    valid_symbol_sets: Vec<ValidSymbolSet>,
    extra_roots: Vec<ExtraRoot>,
    word: Option<TerminalId>,
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
        Self::seed(grammar, lexical)
    }

    /// Normalize validated grammar facts into flattened productions.
    ///
    /// This is the parser-generator input stage before LR item sets and parse
    /// tables. It lowers grammar expressions into flat production rows and
    /// generated auxiliary nonterminals; it does not execute the grammar.
    pub fn normalize_from_validated(
        grammar: &ValidatedGrammar,
        lexical: &LexicalFacts,
    ) -> Result<Self, ParserNormalizeError> {
        let mut parser = Self::seed(grammar, lexical);
        ProductionNormalizer::new(grammar, &mut parser).normalize()?;
        parser.add_public_anonymous_terminals_from_productions();
        parser.stage = ParserGenerationStage::Productions;
        Ok(parser)
    }

    fn seed(grammar: &ValidatedGrammar, lexical: &LexicalFacts) -> Self {
        let nonterminals = grammar
            .rules()
            .map(|rule| NonterminalSymbol {
                id: NonterminalId::from_index(rule.id().get() as usize),
                source_rule: Some(rule.id()),
                name: rule.name().as_str().to_owned(),
                visible: rule.visible() && !grammar.inline().contains(&rule.id()),
                inline: grammar.inline().contains(&rule.id()),
                origin: NonterminalOrigin::Rule,
            })
            .collect::<Vec<_>>();
        let mut lexical_rules = Vec::new();
        let terminals = seed_terminal_symbols(grammar, lexical, &mut lexical_rules);
        let terminal_by_expr = terminals
            .iter()
            .map(|terminal| (terminal.source_expr(), terminal.id()))
            .collect::<HashMap<_, _>>();
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
        let reserved_contexts = seed_reserved_contexts(grammar, &terminal_by_expr);
        let extra_roots = grammar
            .extras()
            .iter()
            .copied()
            .filter_map(|expr| extra_root_symbol(grammar, &terminal_by_expr, expr))
            .map(|symbol| ExtraRoot { symbol })
            .collect::<Vec<_>>();
        let word = resolve_word_terminal(grammar, &terminal_by_expr);
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
        let fields = grammar
            .fields()
            .map(|(id, field)| FieldDecl {
                id,
                name: field.as_str().to_owned(),
            })
            .collect::<Vec<_>>();
        let aliases = grammar
            .aliases()
            .map(|alias| AliasDecl {
                id: alias.id(),
                value: alias.value().to_owned(),
                named: alias.named(),
            })
            .collect::<Vec<_>>();
        let inline_rules = grammar
            .inline()
            .iter()
            .copied()
            .map(|rule| InlineRule {
                rule,
                nonterminal: NonterminalId::from_index(rule.get() as usize),
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
            provenances: Vec::new(),
            fields,
            aliases,
            lexical_rules,
            inline_rules,
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

    /// Parser-generation provenance rows keyed by [`ProvenanceId`].
    pub fn provenances(&self) -> &[Provenance] {
        &self.provenances
    }

    /// Field declarations keyed by [`FieldId`].
    pub fn fields(&self) -> &[FieldDecl] {
        &self.fields
    }

    /// Alias declarations keyed by [`AliasId`].
    pub fn aliases(&self) -> &[AliasDecl] {
        &self.aliases
    }

    /// Parser-owned lexical rules keyed by [`LexicalRuleId`].
    pub fn lexical_rules(&self) -> &[LexicalRule] {
        &self.lexical_rules
    }

    /// Inline declarations that still need expansion before LR item generation.
    pub fn inline_rules(&self) -> &[InlineRule] {
        &self.inline_rules
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
    pub const fn word(&self) -> Option<TerminalId> {
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

    fn add_public_anonymous_terminals_from_productions(&mut self) {
        let mut additions = Vec::new();
        for production in &self.productions {
            for step in &production.steps {
                if let ParserSymbol::Terminal(terminal_id) = step.symbol {
                    let terminal = &self.symbols.terminals[terminal_id.get() as usize];
                    for name in &terminal.public_names {
                        if !self.public_node_kinds.iter().any(|kind| kind.name == *name)
                            && !additions.iter().any(|(existing, _)| existing == name)
                        {
                            additions.push((name.clone(), terminal_id));
                        }
                    }
                }
            }
        }
        for (name, terminal_id) in additions {
            self.public_node_kinds.push(PublicNodeKind {
                id: PublicNodeKindId::from_index(self.public_node_kinds.len()),
                name,
                source: PublicNodeKindSource::AnonymousTerminal(terminal_id),
            });
        }
    }
}

fn seed_terminal_symbols(
    grammar: &ValidatedGrammar,
    lexical: &LexicalFacts,
    lexical_rules: &mut Vec<LexicalRule>,
) -> Vec<TerminalSymbol> {
    let mut terminals = lexical
        .terminals()
        .iter()
        .enumerate()
        .map(|(index, terminal)| {
            let id = TerminalId::from_index(index);
            let kind = match terminal.kind {
                TerminalKind::String => ParserTerminalKind::String,
                TerminalKind::Pattern => ParserTerminalKind::Pattern,
            };
            let public_names = match terminal.kind {
                TerminalKind::String => vec![terminal.spelling.clone()],
                TerminalKind::Pattern => Vec::new(),
            };
            let lexical_rule = push_lexical_rule(
                lexical_rules,
                id,
                LexicalRuleSource::Terminal {
                    expr: terminal.expr,
                    kind,
                    spelling: terminal.spelling.clone(),
                },
            );
            TerminalSymbol {
                id,
                kind,
                spelling: terminal.spelling.clone(),
                source_expr: terminal.expr,
                lexical_rule,
                lexical_root: None,
                public_names,
            }
        })
        .collect::<Vec<_>>();
    for root in lexical.lexical_roots() {
        let kind = match root.kind {
            crate::lexical::LexicalRootKind::Token => ParserTerminalKind::Token,
            crate::lexical::LexicalRootKind::ImmediateToken => ParserTerminalKind::ImmediateToken,
        };
        let id = TerminalId::from_index(terminals.len());
        let public_names = collect_public_literal_names(grammar, root.content);
        let lexical_rule = push_lexical_rule(
            lexical_rules,
            id,
            LexicalRuleSource::TokenRoot {
                root: root.id,
                content: root.content,
                kind,
                public_names: public_names.clone(),
            },
        );
        terminals.push(TerminalSymbol {
            id,
            kind,
            spelling: lexical_root_spelling(grammar, root.id),
            source_expr: root.id,
            lexical_rule,
            lexical_root: Some(root.id),
            public_names,
        });
    }
    terminals
}

fn push_lexical_rule(
    lexical_rules: &mut Vec<LexicalRule>,
    terminal: TerminalId,
    source: LexicalRuleSource,
) -> LexicalRuleId {
    let id = LexicalRuleId::from_index(lexical_rules.len());
    lexical_rules.push(LexicalRule {
        id,
        terminal,
        source,
    });
    id
}

fn lexical_root_spelling(grammar: &ValidatedGrammar, expr: GrammarExprId) -> String {
    match grammar.expr(expr) {
        GrammarExpr::Token(content) => format!("token#{}:{}", expr.get(), content.get()),
        GrammarExpr::ImmediateToken(content) => {
            format!("token.immediate#{}:{}", expr.get(), content.get())
        }
        _ => format!("token#{}", expr.get()),
    }
}

fn collect_public_literal_names(grammar: &ValidatedGrammar, expr: GrammarExprId) -> Vec<String> {
    let mut names = Vec::new();
    collect_public_literal_names_into(grammar, expr, &mut names);
    names.sort();
    names.dedup();
    names
}

fn collect_public_literal_names_into(
    grammar: &ValidatedGrammar,
    expr: GrammarExprId,
    names: &mut Vec<String>,
) {
    match grammar.expr(expr) {
        GrammarExpr::StringToken(value) => names.push(value.clone()),
        GrammarExpr::Choice(members) | GrammarExpr::Seq(members) => {
            for member in members {
                collect_public_literal_names_into(grammar, *member, names);
            }
        }
        GrammarExpr::Repeat(content)
        | GrammarExpr::Repeat1(content)
        | GrammarExpr::Field { content, .. }
        | GrammarExpr::Token(content)
        | GrammarExpr::ImmediateToken(content)
        | GrammarExpr::Prec { content, .. }
        | GrammarExpr::PrecDynamic { content, .. }
        | GrammarExpr::Alias { content, .. }
        | GrammarExpr::Reserved { content, .. } => {
            collect_public_literal_names_into(grammar, *content, names);
        }
        GrammarExpr::Blank | GrammarExpr::PatternToken { .. } | GrammarExpr::Symbol(_) => {}
    }
}

fn seed_reserved_contexts(
    grammar: &ValidatedGrammar,
    terminal_by_expr: &HashMap<GrammarExprId, TerminalId>,
) -> Vec<ReservedContext> {
    grammar
        .reserved_sets()
        .iter()
        .enumerate()
        .map(|(index, reserved)| ReservedContext {
            id: ReservedContextId::from_index(index),
            name: reserved.name().to_owned(),
            entries: reserved
                .entries()
                .iter()
                .filter_map(|expr| terminal_by_expr.get(expr).copied())
                .collect(),
        })
        .collect()
}

fn extra_root_symbol(
    grammar: &ValidatedGrammar,
    terminal_by_expr: &HashMap<GrammarExprId, TerminalId>,
    expr: GrammarExprId,
) -> Option<ParserSymbol> {
    if let Some(terminal) = terminal_by_expr.get(&expr).copied() {
        return Some(ParserSymbol::Terminal(terminal));
    }
    match grammar.expr(expr) {
        GrammarExpr::Symbol(SymbolRef::Rule(rule)) => Some(ParserSymbol::Nonterminal(
            NonterminalId::from_index(rule.get() as usize),
        )),
        GrammarExpr::Symbol(SymbolRef::External(external)) => Some(ParserSymbol::External(
            ExternalId::from_index(external.get() as usize),
        )),
        _ => None,
    }
}

fn resolve_word_terminal(
    grammar: &ValidatedGrammar,
    terminal_by_expr: &HashMap<GrammarExprId, TerminalId>,
) -> Option<TerminalId> {
    let word = grammar.word()?;
    let expr = grammar.rule(word).expr();
    terminal_by_expr.get(&expr).copied()
}

/// Error produced while normalizing grammar expressions into productions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParserNormalizeError {
    kind: ParserNormalizeErrorKind,
}

impl ParserNormalizeError {
    fn new(kind: ParserNormalizeErrorKind) -> Self {
        Self { kind }
    }

    /// Error kind.
    pub const fn kind(&self) -> &ParserNormalizeErrorKind {
        &self.kind
    }
}

impl fmt::Display for ParserNormalizeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            ParserNormalizeErrorKind::MissingTerminalExpression { expr } => {
                write!(
                    f,
                    "grammar expression {} has no parser terminal",
                    expr.get()
                )
            }
            ParserNormalizeErrorKind::MissingReservedContext { context } => {
                write!(
                    f,
                    "reserved context {} has no parser context row",
                    context.get()
                )
            }
            ParserNormalizeErrorKind::NullableRepeatContent { expr, content } => {
                write!(
                    f,
                    "repeat expression {} has nullable content {}",
                    expr.get(),
                    content.get()
                )
            }
        }
    }
}

impl Error for ParserNormalizeError {}

/// Parser production-normalization error kind.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ParserNormalizeErrorKind {
    /// A grammar expression expected to be a terminal had no terminal symbol.
    MissingTerminalExpression {
        /// Missing expression id.
        expr: GrammarExprId,
    },
    /// A reserved wrapper referenced a missing reserved context row.
    MissingReservedContext {
        /// Missing context id.
        context: ReservedSetId,
    },
    /// Repetition content normalized to an empty production.
    NullableRepeatContent {
        /// Repeat expression id.
        expr: GrammarExprId,
        /// Repeated content expression id.
        content: GrammarExprId,
    },
}

struct ProductionNormalizer<'a> {
    grammar: &'a ValidatedGrammar,
    parser: &'a mut ParserGrammar,
    terminal_by_expr: HashMap<GrammarExprId, TerminalId>,
    public_node_by_rule: HashMap<RuleId, PublicNodeKindId>,
}

impl<'a> ProductionNormalizer<'a> {
    fn new(grammar: &'a ValidatedGrammar, parser: &'a mut ParserGrammar) -> Self {
        let terminal_by_expr = parser
            .symbols
            .terminals
            .iter()
            .map(|terminal| (terminal.source_expr, terminal.id))
            .collect::<HashMap<_, _>>();
        let public_node_by_rule = parser
            .public_node_kinds
            .iter()
            .filter_map(|kind| match kind.source {
                PublicNodeKindSource::Rule(nonterminal) => parser
                    .symbols
                    .nonterminals
                    .get(nonterminal.get() as usize)
                    .and_then(|symbol| symbol.source_rule.map(|rule| (rule, kind.id))),
                _ => None,
            })
            .collect::<HashMap<_, _>>();
        Self {
            grammar,
            parser,
            terminal_by_expr,
            public_node_by_rule,
        }
    }

    fn normalize(&mut self) -> Result<(), ParserNormalizeError> {
        for rule in self.grammar.rules() {
            let lhs = NonterminalId::from_index(rule.id().get() as usize);
            let sequences = self.lower_expr(rule.id(), rule.expr())?;
            let public_node = self.public_node_by_rule.get(&rule.id()).copied();
            for sequence in sequences {
                let source_expr = sequence.source_expr.unwrap_or(rule.expr());
                self.push_production(
                    lhs,
                    rule.id(),
                    sequence,
                    ProductionOrigin::Rule,
                    public_node,
                    ProvenanceSource::GrammarRule {
                        rule: rule.id(),
                        expr: source_expr,
                    },
                );
            }
        }
        Ok(())
    }

    fn lower_expr(
        &mut self,
        owner: RuleId,
        expr: GrammarExprId,
    ) -> Result<Vec<SequenceDraft>, ParserNormalizeError> {
        let expr_value = self.grammar.expr(expr).clone();
        let mut sequences = match expr_value {
            GrammarExpr::Blank => Ok(vec![SequenceDraft::default()]),
            GrammarExpr::StringToken(_) | GrammarExpr::PatternToken { .. } => {
                Ok(vec![SequenceDraft::single(self.terminal_symbol(expr)?)])
            }
            GrammarExpr::Token(_) | GrammarExpr::ImmediateToken(_) => {
                Ok(vec![SequenceDraft::single(self.terminal_symbol(expr)?)])
            }
            GrammarExpr::Symbol(SymbolRef::Rule(rule)) => Ok(vec![SequenceDraft::single(
                ParserSymbol::Nonterminal(NonterminalId::from_index(rule.get() as usize)),
            )]),
            GrammarExpr::Symbol(SymbolRef::External(external)) => Ok(vec![SequenceDraft::single(
                ParserSymbol::External(ExternalId::from_index(external.get() as usize)),
            )]),
            GrammarExpr::Choice(members) => {
                let mut choices = Vec::new();
                for member in members.clone() {
                    choices.extend(self.lower_expr(owner, member)?);
                }
                Ok(choices)
            }
            GrammarExpr::Seq(members) => {
                let mut sequences = vec![SequenceDraft::default()];
                for member in members.clone() {
                    let member_sequences = self.lower_expr(owner, member)?;
                    sequences = combine_sequences(sequences, member_sequences);
                }
                Ok(sequences)
            }
            GrammarExpr::Repeat(content) => {
                let aux = self.add_repeat_auxiliary(owner, expr, content, false)?;
                Ok(vec![SequenceDraft::single(ParserSymbol::Nonterminal(aux))])
            }
            GrammarExpr::Repeat1(content) => {
                let aux = self.add_repeat_auxiliary(owner, expr, content, true)?;
                Ok(vec![SequenceDraft::single(ParserSymbol::Nonterminal(aux))])
            }
            GrammarExpr::Field { field, content } => {
                let mut sequences = self.lower_expr(owner, content)?;
                for sequence in &mut sequences {
                    sequence.apply_field(field);
                }
                Ok(sequences)
            }
            GrammarExpr::Prec {
                assoc,
                value,
                content,
            } => {
                let mut sequences = self.lower_expr(owner, content)?;
                let precedence = static_precedence(&value);
                let associativity = associativity(assoc);
                for sequence in &mut sequences {
                    sequence.static_precedence = Some(precedence.clone());
                    sequence.associativity = associativity;
                }
                Ok(sequences)
            }
            GrammarExpr::PrecDynamic { value, content } => {
                let mut sequences = self.lower_expr(owner, content)?;
                for sequence in &mut sequences {
                    sequence.dynamic_precedence =
                        strongest_dynamic_precedence(sequence.dynamic_precedence, value);
                }
                Ok(sequences)
            }
            GrammarExpr::Alias {
                alias,
                named,
                content,
            } => {
                let mut sequences = self.lower_expr(owner, content)?;
                for sequence in &mut sequences {
                    sequence.apply_alias(alias, named);
                }
                Ok(sequences)
            }
            GrammarExpr::Reserved { context, content } => {
                let reserved_context = self.reserved_context(context)?;
                let mut sequences = self.lower_expr(owner, content)?;
                for sequence in &mut sequences {
                    sequence.apply_reserved_context(reserved_context);
                }
                Ok(sequences)
            }
        }?;
        for sequence in &mut sequences {
            if sequence.source_expr.is_none() {
                sequence.source_expr = Some(expr);
            }
        }
        Ok(sequences)
    }

    fn add_repeat_auxiliary(
        &mut self,
        owner: RuleId,
        repeat_expr: GrammarExprId,
        content: GrammarExprId,
        one_or_more: bool,
    ) -> Result<NonterminalId, ParserNormalizeError> {
        let aux = NonterminalId::from_index(self.parser.symbols.nonterminals.len());
        self.parser.symbols.nonterminals.push(NonterminalSymbol {
            id: aux,
            source_rule: Some(owner),
            name: format!("__snark_repeat_{}_{}", owner.get(), repeat_expr.get()),
            visible: false,
            inline: true,
            origin: NonterminalOrigin::RepeatAuxiliary,
        });

        let content_sequences = self.lower_expr(owner, content)?;
        if content_sequences
            .iter()
            .any(|sequence| sequence.steps.is_empty())
        {
            return Err(ParserNormalizeError::new(
                ParserNormalizeErrorKind::NullableRepeatContent {
                    expr: repeat_expr,
                    content,
                },
            ));
        }
        if !one_or_more {
            self.push_production(
                aux,
                owner,
                SequenceDraft::default(),
                ProductionOrigin::Repeat,
                None,
                ProvenanceSource::RepeatAuxiliary {
                    owner,
                    expr: repeat_expr,
                },
            );
        }
        if one_or_more {
            for content_sequence in &content_sequences {
                self.push_production(
                    aux,
                    owner,
                    content_sequence.clone(),
                    ProductionOrigin::Repeat,
                    None,
                    ProvenanceSource::RepeatAuxiliary {
                        owner,
                        expr: repeat_expr,
                    },
                );
            }
        }
        for mut content_sequence in content_sequences {
            let mut recursive = SequenceDraft::single(ParserSymbol::Nonterminal(aux));
            recursive.append(content_sequence.clone());
            content_sequence = recursive;
            self.push_production(
                aux,
                owner,
                content_sequence,
                ProductionOrigin::Repeat,
                None,
                ProvenanceSource::RepeatAuxiliary {
                    owner,
                    expr: repeat_expr,
                },
            );
        }
        Ok(aux)
    }

    fn push_production(
        &mut self,
        lhs: NonterminalId,
        owner: RuleId,
        sequence: SequenceDraft,
        origin: ProductionOrigin,
        public_node: Option<PublicNodeKindId>,
        provenance_source: ProvenanceSource,
    ) {
        let metadata = ProductionMetadataId::from_index(self.parser.production_metadata.len());
        let provenance = self.push_provenance(provenance_source);
        let (steps, field_map, alias_sequence) = self.materialize_sequence(sequence.clone());
        let dynamic_precedence = sequence.dynamic_precedence;
        let production = Production {
            id: ProductionId::from_index(self.parser.productions.len()),
            lhs,
            steps,
            dynamic_precedence,
            metadata,
        };
        self.parser.productions.push(production);
        self.parser.production_metadata.push(ProductionMetadata {
            id: metadata,
            owner,
            public_node,
            field_map,
            alias_sequence,
            origin,
            static_precedence: sequence.static_precedence,
            associativity: sequence.associativity,
            dynamic_precedence,
            provenance: Some(provenance),
        });
    }

    fn materialize_sequence(
        &mut self,
        sequence: SequenceDraft,
    ) -> (
        Vec<ProductionStep>,
        Option<FieldMapId>,
        Option<AliasSequenceId>,
    ) {
        let mut field_entries = Vec::new();
        let mut alias_entries = Vec::new();
        let steps = sequence
            .steps
            .into_iter()
            .enumerate()
            .map(|(structural_index, step)| {
                if let Some(field) = step.field {
                    field_entries.push(FieldMapEntry {
                        structural_index,
                        field,
                    });
                }
                if let (Some(alias), Some(named)) = (step.alias, step.alias_named) {
                    alias_entries.push(AliasSequenceEntry {
                        structural_index,
                        alias,
                        named,
                    });
                }
                ProductionStep {
                    symbol: step.symbol,
                    field: step.field,
                    alias: step.alias,
                    alias_named: step.alias_named,
                    reserved_context: step.reserved_context,
                    structural_index,
                }
            })
            .collect::<Vec<_>>();
        let field_map = if field_entries.is_empty() {
            None
        } else {
            let id = FieldMapId::from_index(self.parser.field_maps.len());
            self.parser.field_maps.push(FieldMap {
                id,
                entries: field_entries,
            });
            Some(id)
        };
        let alias_sequence = if alias_entries.is_empty() {
            None
        } else {
            let id = AliasSequenceId::from_index(self.parser.alias_sequences.len());
            self.parser.alias_sequences.push(AliasSequence {
                id,
                entries: alias_entries,
            });
            Some(id)
        };
        (steps, field_map, alias_sequence)
    }

    fn push_provenance(&mut self, source: ProvenanceSource) -> ProvenanceId {
        let id = ProvenanceId::from_index(self.parser.provenances.len());
        self.parser.provenances.push(Provenance { id, source });
        id
    }

    fn terminal_symbol(&self, expr: GrammarExprId) -> Result<ParserSymbol, ParserNormalizeError> {
        self.terminal_by_expr
            .get(&expr)
            .copied()
            .map(ParserSymbol::Terminal)
            .ok_or_else(|| {
                ParserNormalizeError::new(ParserNormalizeErrorKind::MissingTerminalExpression {
                    expr,
                })
            })
    }

    fn reserved_context(
        &self,
        context: ReservedSetId,
    ) -> Result<ReservedContextId, ParserNormalizeError> {
        let id = ReservedContextId::from_index(context.get() as usize);
        if self
            .parser
            .reserved_contexts
            .get(id.get() as usize)
            .is_some()
        {
            Ok(id)
        } else {
            Err(ParserNormalizeError::new(
                ParserNormalizeErrorKind::MissingReservedContext { context },
            ))
        }
    }
}

#[derive(Debug, Clone, Default)]
struct SequenceDraft {
    steps: Vec<StepDraft>,
    static_precedence: Option<StaticPrecedence>,
    associativity: Associativity,
    dynamic_precedence: i32,
    source_expr: Option<GrammarExprId>,
}

impl SequenceDraft {
    fn single(symbol: ParserSymbol) -> Self {
        Self {
            steps: vec![StepDraft {
                symbol,
                field: None,
                alias: None,
                alias_named: None,
                reserved_context: None,
            }],
            ..Self::default()
        }
    }

    fn append(&mut self, other: Self) {
        self.steps.extend(other.steps);
        if other.static_precedence.is_some() {
            self.static_precedence = other.static_precedence;
        }
        if other.associativity != Associativity::None {
            self.associativity = other.associativity;
        }
        self.dynamic_precedence =
            strongest_dynamic_precedence(self.dynamic_precedence, other.dynamic_precedence);
        if other.source_expr.is_some() {
            self.source_expr = other.source_expr;
        }
    }

    fn apply_field(&mut self, field: FieldId) {
        for step in &mut self.steps {
            step.field = Some(field);
        }
    }

    fn apply_alias(&mut self, alias: AliasId, named: bool) {
        for step in &mut self.steps {
            step.alias = Some(alias);
            step.alias_named = Some(named);
        }
    }

    fn apply_reserved_context(&mut self, reserved_context: ReservedContextId) {
        for step in &mut self.steps {
            step.reserved_context = Some(reserved_context);
        }
    }
}

#[derive(Debug, Clone)]
struct StepDraft {
    symbol: ParserSymbol,
    field: Option<FieldId>,
    alias: Option<AliasId>,
    alias_named: Option<bool>,
    reserved_context: Option<ReservedContextId>,
}

fn combine_sequences(left: Vec<SequenceDraft>, right: Vec<SequenceDraft>) -> Vec<SequenceDraft> {
    let mut combined = Vec::new();
    for left_sequence in left {
        for right_sequence in &right {
            let mut sequence = left_sequence.clone();
            sequence.append(right_sequence.clone());
            combined.push(sequence);
        }
    }
    combined
}

fn static_precedence(value: &StaticPrecedenceValue) -> StaticPrecedence {
    match value {
        StaticPrecedenceValue::Integer(value) => StaticPrecedence::Integer(*value),
        StaticPrecedenceValue::Name(name) => StaticPrecedence::Named(name.clone()),
    }
}

fn associativity(assoc: PrecedenceAssoc) -> Associativity {
    match assoc {
        PrecedenceAssoc::None => Associativity::None,
        PrecedenceAssoc::Left => Associativity::Left,
        PrecedenceAssoc::Right => Associativity::Right,
    }
}

fn strongest_dynamic_precedence(left: i32, right: i32) -> i32 {
    if right.abs() >= left.abs() {
        right
    } else {
        left
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
    kind: ParserTerminalKind,
    spelling: String,
    source_expr: GrammarExprId,
    lexical_rule: LexicalRuleId,
    lexical_root: Option<GrammarExprId>,
    public_names: Vec<String>,
}

impl TerminalSymbol {
    /// Terminal id.
    pub const fn id(&self) -> TerminalId {
        self.id
    }

    /// Terminal kind.
    pub const fn kind(&self) -> ParserTerminalKind {
        self.kind
    }

    /// Literal spelling or regex source.
    pub fn spelling(&self) -> &str {
        &self.spelling
    }

    /// Grammar expression that introduced this terminal symbol.
    pub const fn source_expr(&self) -> GrammarExprId {
        self.source_expr
    }

    /// Parser-owned lexical rule that describes this terminal.
    pub const fn lexical_rule(&self) -> LexicalRuleId {
        self.lexical_rule
    }

    /// Token/immediate-token wrapper root that introduced this terminal, if any.
    pub const fn lexical_root(&self) -> Option<GrammarExprId> {
        self.lexical_root
    }

    /// Public anonymous names this terminal can contribute to queries.
    pub fn public_names(&self) -> &[String] {
        &self.public_names
    }
}

/// Parser terminal kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ParserTerminalKind {
    /// Literal string token.
    String,
    /// Regex pattern token.
    Pattern,
    /// `token(...)` lexical variable.
    Token,
    /// `token.immediate(...)` lexical variable.
    ImmediateToken,
}

/// Nonterminal symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NonterminalSymbol {
    id: NonterminalId,
    source_rule: Option<RuleId>,
    name: String,
    visible: bool,
    inline: bool,
    origin: NonterminalOrigin,
}

impl NonterminalSymbol {
    /// Nonterminal id.
    pub const fn id(&self) -> NonterminalId {
        self.id
    }

    /// Source validated rule id, when this is not a generated auxiliary symbol.
    pub const fn rule(&self) -> Option<RuleId> {
        self.source_rule
    }

    /// Source validated rule id, when this is not a generated auxiliary symbol.
    pub const fn source_rule(&self) -> Option<RuleId> {
        self.source_rule
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

    /// Origin of this nonterminal symbol.
    pub const fn origin(&self) -> NonterminalOrigin {
        self.origin
    }
}

/// How a nonterminal entered the parser symbol table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum NonterminalOrigin {
    /// Nonterminal came from a validated grammar rule.
    Rule,
    /// Nonterminal was generated while expanding a repetition expression.
    RepeatAuxiliary,
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
    alias_named: Option<bool>,
    reserved_context: Option<ReservedContextId>,
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

    /// Whether the alias at this structural child index is named.
    pub const fn alias_named(&self) -> Option<bool> {
        self.alias_named
    }

    /// Reserved-word context applied at this structural child index.
    pub const fn reserved_context(&self) -> Option<ReservedContextId> {
        self.reserved_context
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
    named: bool,
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

    /// Whether this alias emits a named node/token.
    pub const fn named(&self) -> bool {
        self.named
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
    /// Anonymous literal terminal referenced by queries.
    AnonymousTerminal(TerminalId),
}

/// Parser-owned field declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldDecl {
    id: FieldId,
    name: String,
}

impl FieldDecl {
    /// Field id.
    pub const fn id(&self) -> FieldId {
        self.id
    }

    /// Field name.
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Parser-owned alias declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AliasDecl {
    id: AliasId,
    value: String,
    named: bool,
}

impl AliasDecl {
    /// Alias id.
    pub const fn id(&self) -> AliasId {
        self.id
    }

    /// Alias value.
    pub fn value(&self) -> &str {
        &self.value
    }

    /// Whether this alias is named.
    pub const fn named(&self) -> bool {
        self.named
    }
}

/// Parser-owned lexical rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexicalRule {
    id: LexicalRuleId,
    terminal: TerminalId,
    source: LexicalRuleSource,
}

impl LexicalRule {
    /// Lexical rule id.
    pub const fn id(&self) -> LexicalRuleId {
        self.id
    }

    /// Terminal produced by this lexical rule.
    pub const fn terminal(&self) -> TerminalId {
        self.terminal
    }

    /// Source facts for this lexical rule.
    pub const fn source(&self) -> &LexicalRuleSource {
        &self.source
    }
}

/// Source facts for a parser-owned lexical rule.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum LexicalRuleSource {
    /// Direct literal or regex terminal expression.
    Terminal {
        /// Source expression id retained for provenance.
        expr: GrammarExprId,
        /// Terminal kind.
        kind: ParserTerminalKind,
        /// Literal or regex spelling.
        spelling: String,
    },
    /// Token or immediate-token lexical root.
    TokenRoot {
        /// Wrapper expression id retained for provenance.
        root: GrammarExprId,
        /// Wrapped content expression id retained for provenance.
        content: GrammarExprId,
        /// Terminal kind.
        kind: ParserTerminalKind,
        /// Public literal names visible through this root.
        public_names: Vec<String>,
    },
}

/// Inline declaration retained until the inline-expansion pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InlineRule {
    rule: RuleId,
    nonterminal: NonterminalId,
}

impl InlineRule {
    /// Source inline rule id.
    pub const fn rule(&self) -> RuleId {
        self.rule
    }

    /// Parser nonterminal marked for inline expansion.
    pub const fn nonterminal(&self) -> NonterminalId {
        self.nonterminal
    }
}

/// Parser-generation provenance row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Provenance {
    id: ProvenanceId,
    source: ProvenanceSource,
}

impl Provenance {
    /// Provenance id.
    pub const fn id(&self) -> ProvenanceId {
        self.id
    }

    /// Source fact that introduced the generated row.
    pub const fn source(&self) -> ProvenanceSource {
        self.source
    }
}

/// Source fact that introduced generated parser data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProvenanceSource {
    /// Production came from a grammar rule root expression.
    GrammarRule {
        /// Owning rule.
        rule: RuleId,
        /// Expression lowered for this production.
        expr: GrammarExprId,
    },
    /// Production came from a generated repeat auxiliary.
    RepeatAuxiliary {
        /// Owning grammar rule.
        owner: RuleId,
        /// Repetition expression that introduced the auxiliary.
        expr: GrammarExprId,
    },
}

/// Extra grammar root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtraRoot {
    symbol: ParserSymbol,
}

impl ExtraRoot {
    /// Extra parser symbol root.
    pub const fn symbol(&self) -> ParserSymbol {
        self.symbol
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

impl Default for Associativity {
    fn default() -> Self {
        Self::None
    }
}

/// Reserved-word context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReservedContext {
    id: ReservedContextId,
    name: String,
    entries: Vec<TerminalId>,
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

    /// Reserved word terminal entries.
    pub fn entries(&self) -> &[TerminalId] {
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
        /// Whether this alias emits a named node/token.
        named: bool,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{grammar::RawGrammarJson, lexical::LexicalFacts, validated::ValidatedGrammar};

    fn normalize(input: &str) -> ParserGrammar {
        let raw = RawGrammarJson::from_tree_sitter_json_str(input).unwrap();
        let validated = ValidatedGrammar::from_raw(&raw).unwrap();
        let lexical = LexicalFacts::from_grammar(&validated);
        ParserGrammar::normalize_from_validated(&validated, &lexical).unwrap()
    }

    #[test]
    fn normalizes_rules_tokens_repeats_fields_and_aliases_into_productions() {
        let grammar = normalize(
            r##"{
              "name": "mini",
              "rules": {
                "source_file": {
                  "type": "SEQ",
                  "members": [
                    { "type": "SYMBOL", "name": "item" },
                    {
                      "type": "REPEAT",
                      "content": { "type": "SYMBOL", "name": "item" }
                    }
                  ]
                },
                "item": {
                  "type": "SEQ",
                  "members": [
                    {
                      "type": "ALIAS",
                      "named": true,
                      "value": "thing",
                      "content": {
                        "type": "FIELD",
                        "name": "name",
                        "content": {
                          "type": "TOKEN",
                          "content": { "type": "STRING", "value": "a" }
                        }
                      }
                    },
                    {
                      "type": "IMMEDIATE_TOKEN",
                      "content": { "type": "STRING", "value": "b" }
                    }
                  ]
                }
              }
            }"##,
        );

        assert_eq!(grammar.stage(), ParserGenerationStage::Productions);
        assert_eq!(grammar.symbols().nonterminals().len(), 3);
        assert_eq!(
            grammar.symbols().nonterminals()[2].origin(),
            NonterminalOrigin::RepeatAuxiliary
        );
        assert_eq!(grammar.productions().len(), 4);
        assert_eq!(grammar.provenances().len(), grammar.productions().len());
        assert_eq!(grammar.fields()[0].name(), "name");
        assert_eq!(grammar.aliases()[0].value(), "thing");
        assert!(grammar.aliases()[0].named());
        assert_eq!(grammar.inline_rules().len(), 0);

        let repeat_aux = NonterminalId::from_index(2);
        assert_eq!(grammar.productions()[0].lhs(), repeat_aux);
        assert!(grammar.productions()[0].steps().is_empty());
        assert_eq!(
            grammar.productions()[1]
                .steps()
                .iter()
                .map(ProductionStep::symbol)
                .collect::<Vec<_>>(),
            [
                ParserSymbol::Nonterminal(repeat_aux),
                ParserSymbol::Nonterminal(NonterminalId::from_index(1)),
            ]
        );
        assert_eq!(
            grammar.productions()[2]
                .steps()
                .iter()
                .map(ProductionStep::symbol)
                .collect::<Vec<_>>(),
            [
                ParserSymbol::Nonterminal(NonterminalId::from_index(1)),
                ParserSymbol::Nonterminal(repeat_aux),
            ]
        );

        let token = grammar
            .symbols()
            .terminals()
            .iter()
            .find(|terminal| terminal.kind() == ParserTerminalKind::Token)
            .unwrap();
        let immediate = grammar
            .symbols()
            .terminals()
            .iter()
            .find(|terminal| terminal.kind() == ParserTerminalKind::ImmediateToken)
            .unwrap();
        let item = &grammar.productions()[3];
        assert_eq!(
            item.steps()
                .iter()
                .map(ProductionStep::symbol)
                .collect::<Vec<_>>(),
            [
                ParserSymbol::Terminal(token.id()),
                ParserSymbol::Terminal(immediate.id()),
            ]
        );
        assert_eq!(item.steps()[0].structural_index(), 0);
        assert!(item.steps()[0].field().is_some());
        assert!(item.steps()[0].alias().is_some());
        assert_eq!(item.steps()[0].alias_named(), Some(true));
        assert_eq!(item.steps()[1].structural_index(), 1);
        assert!(item.steps()[1].field().is_none());
        assert!(item.steps()[1].alias().is_none());

        let item_metadata = &grammar.production_metadata()[item.metadata().get() as usize];
        let field_map = item_metadata.field_map().unwrap();
        let alias_sequence = item_metadata.alias_sequence().unwrap();
        assert_eq!(
            grammar.field_maps()[field_map.get() as usize]
                .entries()
                .len(),
            1
        );
        assert_eq!(
            grammar.alias_sequences()[alias_sequence.get() as usize].entries()[0].named(),
            true
        );
        assert!(matches!(
            grammar.provenances()[item_metadata.provenance().unwrap().get() as usize].source(),
            ProvenanceSource::GrammarRule { .. }
        ));
        assert!(grammar.public_node_kinds().iter().any(|kind| {
            kind.name() == "a"
                && matches!(kind.source(), PublicNodeKindSource::AnonymousTerminal(_))
        }));
    }

    #[test]
    fn preserves_precedence_dynamic_precedence_and_reserved_contexts() {
        let grammar = normalize(
            r##"{
              "name": "mini",
              "rules": {
                "source_file": {
                  "type": "SEQ",
                  "members": [
                    {
                      "type": "PREC_LEFT",
                      "value": "tight",
                      "content": {
                        "type": "PREC_DYNAMIC",
                        "value": 3,
                        "content": {
                          "type": "PREC_DYNAMIC",
                          "value": 7,
                          "content": {
                            "type": "RESERVED",
                            "context_name": "default",
                            "content": { "type": "STRING", "value": "a" }
                          }
                        }
                      }
                    },
                    { "type": "STRING", "value": "b" }
                  ]
                  }
              },
              "reserved": {
                "default": [
                  { "type": "STRING", "value": "if" }
                ]
              }
            }"##,
        );

        assert_eq!(grammar.stage(), ParserGenerationStage::Productions);
        assert_eq!(grammar.productions().len(), 1);
        let metadata = &grammar.production_metadata()[0];
        assert_eq!(
            metadata.static_precedence(),
            Some(&StaticPrecedence::Named("tight".to_owned()))
        );
        assert_eq!(metadata.associativity(), Associativity::Left);
        assert_eq!(metadata.dynamic_precedence(), 7);
        assert_eq!(
            grammar.productions()[0].steps()[0].reserved_context(),
            Some(ReservedContextId::from_index(0))
        );
        assert_eq!(grammar.productions()[0].steps()[1].reserved_context(), None);
        assert_eq!(grammar.reserved_contexts()[0].entries().len(), 1);
    }

    #[test]
    fn rejects_nullable_repeat_content_before_table_generation() {
        let raw = RawGrammarJson::from_tree_sitter_json_str(
            r##"{
              "name": "mini",
              "rules": {
                "source_file": {
                  "type": "REPEAT",
                  "content": { "type": "BLANK" }
                }
              }
            }"##,
        )
        .unwrap();
        let validated = ValidatedGrammar::from_raw(&raw).unwrap();
        let lexical = LexicalFacts::from_grammar(&validated);

        let error = ParserGrammar::normalize_from_validated(&validated, &lexical).unwrap_err();

        assert!(matches!(
            error.kind(),
            ParserNormalizeErrorKind::NullableRepeatContent { .. }
        ));
    }

    #[test]
    fn resolves_word_rules_to_terminal_symbols() {
        let grammar = normalize(
            r##"{
              "name": "mini",
              "rules": {
                "source_file": { "type": "SYMBOL", "name": "identifier" },
                "identifier": {
                  "type": "TOKEN",
                  "content": { "type": "PATTERN", "value": "[a-z]+" }
                }
              },
              "word": "identifier"
            }"##,
        );

        let word = grammar.word().unwrap();
        assert_eq!(
            grammar.symbols().terminals()[word.get() as usize].kind(),
            ParserTerminalKind::Token
        );
        assert_eq!(
            grammar.lexical_rules()[grammar.symbols().terminals()[word.get() as usize]
                .lexical_rule()
                .get() as usize]
                .terminal(),
            word
        );
    }
}
