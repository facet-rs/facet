//! Tree-sitter-style parser generator and LR/GLR execution scaffolding.
//!
//! This module is the final-shape parser lane. It is deliberately table- and
//! execution-oriented: validated grammar facts become normalized productions,
//! lexical modes, LR actions, GLR metadata, tree plans, and traceable execution
//! state. It is not a recursive recognizer and it never consumes generated
//! Tree-sitter implementation files.

use std::{
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet, HashMap, VecDeque},
    error::Error,
    fmt,
    sync::Arc,
};

use crate::{
    lexical::{LexicalFacts, TerminalKind},
    runtime_input::{ByteRange, PointRange},
    validated::{
        AliasId, AutoCloseRule as ValidatedAutoCloseRule, FieldId, GrammarExpr, GrammarExprId,
        PrecedenceAssoc, PrecedenceEntry as ValidatedPrecedenceEntry, ReservedSetId, RuleId,
        StaticPrecedenceValue, SymbolRef, ValidatedGrammar, VisibleNodeKind,
    },
};
use smallvec::SmallVec;

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
id_type!(TreeNodeId, "Parser tree node id.");
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
    /// Grammar expressions have been flattened into production-shaped facts.
    ProductionsPrepared,
    /// Productions are ready for LR item-set generation.
    Productions,
    /// LR item sets and action/goto tables have been generated.
    Tables,
    /// Tree, scanner, recovery, and query plans have been attached.
    ExecutionPlans,
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
    public_literal_terminals: Vec<PublicLiteralTerminals>,
    item_preparation: Option<ItemPreparationFacts>,
    glr_plan: GlrPlan,
}

impl ParserGrammar {
    /// Seed parser symbol domains from validated grammar and lexical facts.
    ///
    /// This is not production lowering. The next parser-generator phase must
    /// flatten `ValidatedGrammar` expressions into [`Production`] rows before
    /// item sets or parser execution are valid.
    pub fn seed_from_validated(grammar: &ValidatedGrammar, lexical: &LexicalFacts) -> Self {
        Self::seed(grammar, lexical)
    }

    /// Normalize validated grammar facts into flattened productions.
    ///
    /// This lowers grammar expressions into flat production rows and generated
    /// auxiliary nonterminals. It does not execute the grammar and does not yet
    /// claim LR item-set readiness; inline expansion and nullable validation
    /// are separate parser-generation passes.
    pub fn normalize_from_validated(
        grammar: &ValidatedGrammar,
        lexical: &LexicalFacts,
    ) -> Result<Self, ParserNormalizeError> {
        let mut parser = Self::seed(grammar, lexical);
        parser.validate_materialized_inputs(grammar)?;
        ProductionNormalizer::new(grammar, &mut parser).normalize()?;
        parser.validate_nullable_repeat_content()?;
        parser.add_public_anonymous_terminals_from_productions();
        parser.stage = ParserGenerationStage::ProductionsPrepared;
        Ok(parser)
    }

    /// Prepare normalized productions for LR item-set generation.
    ///
    /// This pass does not build item sets yet. It freezes the graph facts that
    /// item-set generation must consume: inline expansion roots, reachable
    /// nonterminals, productive nonterminals, and nullable nonterminals.
    pub fn prepare_productions_for_items(mut self) -> Result<Self, ParserPrepareError> {
        if self.stage != ParserGenerationStage::ProductionsPrepared {
            return Err(ParserPrepareError::new(
                ParserPrepareErrorKind::WrongStage { stage: self.stage },
            ));
        }
        self.reject_recursive_inline_rules()?;
        let graph = self.production_graph_facts();
        self.reject_nonproductive_reachable_nonterminals(&graph)?;
        self.reject_illegal_nullable_nonterminals(&graph)?;
        let inline_expansions = self.inline_expansion_facts();
        self.item_preparation = Some(ItemPreparationFacts {
            inline_expansions,
            graph,
        });
        self.stage = ParserGenerationStage::Productions;
        Ok(self)
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
            .flat_map(|terminal| {
                terminal
                    .source_exprs()
                    .iter()
                    .copied()
                    .map(move |expr| (expr, terminal.id()))
            })
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
            public_literal_terminals: Vec::new(),
            item_preparation: None,
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

    /// Public anonymous literal-to-terminal mappings.
    pub fn public_literal_terminals(&self) -> &[PublicLiteralTerminals] {
        &self.public_literal_terminals
    }

    /// LR item-generation preparation facts, once the grammar reaches
    /// [`ParserGenerationStage::Productions`].
    pub fn item_preparation(&self) -> Option<&ItemPreparationFacts> {
        self.item_preparation.as_ref()
    }

    /// GLR conflict/recovery plan facts.
    pub const fn glr_plan(&self) -> &GlrPlan {
        &self.glr_plan
    }

    fn add_public_anonymous_terminals_from_productions(&mut self) {
        let mut mappings = Vec::<PublicLiteralTerminals>::new();
        for production in &self.productions {
            for step in &production.steps {
                if let ParserSymbol::Terminal(terminal_id) = step.symbol {
                    let terminal = &self.symbols.terminals[terminal_id.get() as usize];
                    for name in &terminal.public_names {
                        if let Some(mapping) = mappings.iter_mut().find(|mapping| {
                            mapping.literal == *name
                                && matches!(mapping.source, PublicNodeKindSource::AnonymousLiteral)
                        }) {
                            if !mapping.terminals.contains(&terminal_id) {
                                mapping.terminals.push(terminal_id);
                            }
                        } else {
                            mappings.push(PublicLiteralTerminals {
                                literal: name.clone(),
                                terminals: vec![terminal_id],
                                source: PublicNodeKindSource::AnonymousLiteral,
                            });
                        }
                    }
                }
            }
        }
        for mapping in mappings {
            if !self
                .public_node_kinds
                .iter()
                .any(|kind| kind.name == mapping.literal)
            {
                self.public_node_kinds.push(PublicNodeKind {
                    id: PublicNodeKindId::from_index(self.public_node_kinds.len()),
                    name: mapping.literal.clone(),
                    source: mapping.source,
                });
            }
            self.public_literal_terminals.push(mapping);
        }
    }

    fn validate_materialized_inputs(
        &self,
        grammar: &ValidatedGrammar,
    ) -> Result<(), ParserNormalizeError> {
        let terminal_by_expr = self
            .symbols
            .terminals
            .iter()
            .flat_map(|terminal| {
                terminal
                    .source_exprs()
                    .iter()
                    .copied()
                    .map(move |expr| (expr, terminal.id()))
            })
            .collect::<HashMap<_, _>>();
        for expr in grammar.extras() {
            if extra_root_symbol(grammar, &terminal_by_expr, *expr).is_none() {
                return Err(ParserNormalizeError::new(
                    ParserNormalizeErrorKind::UnmaterializedExtraRoot { expr: *expr },
                ));
            }
        }
        for reserved in grammar.reserved_sets() {
            for expr in reserved.entries() {
                if !terminal_by_expr.contains_key(expr) {
                    return Err(ParserNormalizeError::new(
                        ParserNormalizeErrorKind::UnmaterializedReservedEntry {
                            context: reserved.id(),
                            expr: *expr,
                        },
                    ));
                }
            }
        }
        if let Some(rule) = grammar.word() {
            let expr = grammar.rule(rule).expr();
            if !terminal_by_expr.contains_key(&expr) {
                return Err(ParserNormalizeError::new(
                    ParserNormalizeErrorKind::UnmaterializedWord { rule, expr },
                ));
            }
        }
        Ok(())
    }

    fn validate_nullable_repeat_content(&self) -> Result<(), ParserNormalizeError> {
        let nullable = self.nullable_nonterminals();
        for production in &self.productions {
            let metadata = &self.production_metadata[production.metadata.get() as usize];
            let Some(provenance) = metadata.provenance else {
                continue;
            };
            let ProvenanceSource::RepeatAuxiliary { expr, content, .. } =
                self.provenances[provenance.get() as usize].source
            else {
                continue;
            };
            let content_steps = repeat_content_steps(production);
            if content_steps.is_empty() {
                continue;
            }
            if steps_are_nullable(content_steps, &nullable) {
                return Err(ParserNormalizeError::new(
                    ParserNormalizeErrorKind::NullableRepeatContent { expr, content },
                ));
            }
        }
        Ok(())
    }

    fn nullable_nonterminals(&self) -> Vec<bool> {
        let mut nullable = vec![false; self.symbols.nonterminals.len()];
        loop {
            let mut changed = false;
            for production in &self.productions {
                let lhs = production.lhs.get() as usize;
                if nullable[lhs] {
                    continue;
                }
                if steps_are_nullable(&production.steps, &nullable) {
                    nullable[lhs] = true;
                    changed = true;
                }
            }
            if !changed {
                return nullable;
            }
        }
    }

    fn production_graph_facts(&self) -> ProductionGraphFacts {
        let nullable = self.nullable_nonterminals();
        let productive = self.productive_nonterminals();
        let reachable = self.reachable_nonterminals();
        ProductionGraphFacts {
            nullable: ids_from_flags(&nullable),
            productive: ids_from_flags(&productive),
            reachable: ids_from_flags(&reachable),
        }
    }

    fn productive_nonterminals(&self) -> Vec<bool> {
        let mut productive = vec![false; self.symbols.nonterminals.len()];
        loop {
            let mut changed = false;
            for production in &self.productions {
                let lhs = production.lhs.get() as usize;
                if productive[lhs] {
                    continue;
                }
                if production.steps.iter().all(|step| match step.symbol {
                    ParserSymbol::Nonterminal(nonterminal) => productive
                        .get(nonterminal.get() as usize)
                        .copied()
                        .unwrap_or(false),
                    ParserSymbol::Terminal(_)
                    | ParserSymbol::External(_)
                    | ParserSymbol::Eof
                    | ParserSymbol::Internal(_) => true,
                }) {
                    productive[lhs] = true;
                    changed = true;
                }
            }
            if !changed {
                return productive;
            }
        }
    }

    fn reachable_nonterminals(&self) -> Vec<bool> {
        let mut reachable = vec![false; self.symbols.nonterminals.len()];
        let mut stack = vec![self.start];
        while let Some(nonterminal) = stack.pop() {
            let index = nonterminal.get() as usize;
            if reachable[index] {
                continue;
            }
            reachable[index] = true;
            for production in self
                .productions
                .iter()
                .filter(|production| production.lhs == nonterminal)
            {
                for step in &production.steps {
                    if let ParserSymbol::Nonterminal(child) = step.symbol {
                        stack.push(child);
                    }
                }
            }
        }
        reachable
    }

    fn reject_nonproductive_reachable_nonterminals(
        &self,
        graph: &ProductionGraphFacts,
    ) -> Result<(), ParserPrepareError> {
        for nonterminal in &graph.reachable {
            if !graph.productive.contains(nonterminal) {
                return Err(ParserPrepareError::new(
                    ParserPrepareErrorKind::NonproductiveNonterminal {
                        nonterminal: *nonterminal,
                    },
                ));
            }
        }
        Ok(())
    }

    fn reject_illegal_nullable_nonterminals(
        &self,
        graph: &ProductionGraphFacts,
    ) -> Result<(), ParserPrepareError> {
        let mut used = vec![false; self.symbols.nonterminals.len()];
        for production in &self.productions {
            for step in &production.steps {
                if let ParserSymbol::Nonterminal(nonterminal) = step.symbol {
                    used[nonterminal.get() as usize] = true;
                }
            }
        }
        for nonterminal in &graph.nullable {
            if *nonterminal == self.start {
                continue;
            }
            let symbol = &self.symbols.nonterminals[nonterminal.get() as usize];
            if symbol.origin == NonterminalOrigin::RepeatAuxiliary {
                continue;
            }
            if used[nonterminal.get() as usize] {
                return Err(ParserPrepareError::new(
                    ParserPrepareErrorKind::NullableUsedNonterminal {
                        nonterminal: *nonterminal,
                    },
                ));
            }
        }
        Ok(())
    }

    fn reject_recursive_inline_rules(&self) -> Result<(), ParserPrepareError> {
        let mut inline = vec![false; self.symbols.nonterminals.len()];
        for rule in &self.inline_rules {
            inline[rule.nonterminal.get() as usize] = true;
        }
        let mut visit = vec![InlineVisit::Unseen; self.symbols.nonterminals.len()];
        for rule in &self.inline_rules {
            self.visit_inline_rule(rule.nonterminal, &inline, &mut visit)?;
        }
        Ok(())
    }

    fn visit_inline_rule(
        &self,
        nonterminal: NonterminalId,
        inline: &[bool],
        visit: &mut [InlineVisit],
    ) -> Result<(), ParserPrepareError> {
        let index = nonterminal.get() as usize;
        match visit[index] {
            InlineVisit::Active => {
                return Err(ParserPrepareError::new(
                    ParserPrepareErrorKind::RecursiveInline { nonterminal },
                ));
            }
            InlineVisit::Done => return Ok(()),
            InlineVisit::Unseen => {}
        }
        visit[index] = InlineVisit::Active;
        for production in self
            .productions
            .iter()
            .filter(|production| production.lhs == nonterminal)
        {
            for step in &production.steps {
                if let ParserSymbol::Nonterminal(child) = step.symbol
                    && inline.get(child.get() as usize).copied().unwrap_or(false)
                {
                    self.visit_inline_rule(child, inline, visit)?;
                }
            }
        }
        visit[index] = InlineVisit::Done;
        Ok(())
    }

    fn inline_expansion_facts(&self) -> Vec<InlineExpansion> {
        self.inline_rules
            .iter()
            .map(|rule| InlineExpansion {
                rule: rule.rule,
                nonterminal: rule.nonterminal,
                productions: self
                    .productions
                    .iter()
                    .filter(|production| production.lhs == rule.nonterminal)
                    .map(Production::id)
                    .collect(),
            })
            .collect()
    }
}

fn ids_from_flags(flags: &[bool]) -> Vec<NonterminalId> {
    flags
        .iter()
        .enumerate()
        .filter(|(_, flag)| **flag)
        .map(|(index, _)| NonterminalId::from_index(index))
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum InlineVisit {
    Unseen,
    Active,
    Done,
}

fn repeat_content_steps(production: &Production) -> &[ProductionStep] {
    match production.steps.split_first() {
        Some((first, rest)) if first.symbol == ParserSymbol::Nonterminal(production.lhs) => rest,
        _ => &production.steps,
    }
}

fn steps_are_nullable(steps: &[ProductionStep], nullable: &[bool]) -> bool {
    steps.iter().all(|step| match step.symbol {
        ParserSymbol::Nonterminal(nonterminal) => nullable
            .get(nonterminal.get() as usize)
            .copied()
            .unwrap_or(false),
        ParserSymbol::Terminal(_)
        | ParserSymbol::External(_)
        | ParserSymbol::Eof
        | ParserSymbol::Internal(_) => false,
    })
}

fn seed_terminal_symbols(
    grammar: &ValidatedGrammar,
    lexical: &LexicalFacts,
    lexical_rules: &mut Vec<LexicalRule>,
) -> Vec<TerminalSymbol> {
    let mut terminals = Vec::<TerminalSymbol>::new();
    let mut direct_terminal_by_key =
        HashMap::<(ParserTerminalKind, String, Option<String>), TerminalId>::new();
    for terminal in lexical.terminals() {
        let kind = match terminal.kind {
            TerminalKind::String => ParserTerminalKind::String,
            TerminalKind::Pattern => ParserTerminalKind::Pattern,
            TerminalKind::AutoClose => ParserTerminalKind::AutoClose,
        };
        let flags = crate::lex_match::normalized_regex_flags(terminal.flags.as_deref());
        let spelling_key = match terminal.kind {
            TerminalKind::AutoClose => format!("{}#{}", terminal.spelling, terminal.expr.get()),
            TerminalKind::String | TerminalKind::Pattern => terminal.spelling.clone(),
        };
        let key = (kind, spelling_key, flags.clone());
        if let Some(id) = direct_terminal_by_key.get(&key).copied() {
            terminals[id.get() as usize]
                .source_exprs
                .push(terminal.expr);
            continue;
        }
        let id = TerminalId::from_index(terminals.len());
        direct_terminal_by_key.insert(key, id);
        let public_names = match terminal.kind {
            TerminalKind::String => vec![terminal.spelling.clone()],
            TerminalKind::Pattern | TerminalKind::AutoClose => Vec::new(),
        };
        let lexical_rule = push_lexical_rule(
            lexical_rules,
            id,
            LexicalRuleSource::Terminal {
                expr: terminal.expr,
                kind,
                spelling: terminal.spelling.clone(),
                flags: flags.clone(),
            },
        );
        terminals.push(TerminalSymbol {
            id,
            kind,
            spelling: terminal.spelling.clone(),
            flags,
            source_exprs: vec![terminal.expr],
            lexical_rule,
            lexical_root: None,
            public_names,
        });
    }
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
            flags: None,
            source_exprs: vec![root.id],
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
    if let Some(name) = direct_public_literal_name(grammar, expr) {
        names.push(name);
    }
    names.sort();
    names.dedup();
    names
}

fn direct_public_literal_name(grammar: &ValidatedGrammar, expr: GrammarExprId) -> Option<String> {
    match grammar.expr(expr) {
        GrammarExpr::StringToken(value) => Some(value.clone()),
        GrammarExpr::Field { content, .. }
        | GrammarExpr::Token(content)
        | GrammarExpr::ImmediateToken(content)
        | GrammarExpr::Prec { content, .. }
        | GrammarExpr::PrecDynamic { content, .. }
        | GrammarExpr::Alias { content, .. }
        | GrammarExpr::Reserved { content, .. } => direct_public_literal_name(grammar, *content),
        GrammarExpr::Blank
        | GrammarExpr::PatternToken { .. }
        | GrammarExpr::Until { .. }
        | GrammarExpr::Nested { .. }
        | GrammarExpr::AutoClose(_)
        | GrammarExpr::Symbol(_)
        | GrammarExpr::Choice(_)
        | GrammarExpr::Seq(_)
        | GrammarExpr::Repeat(_)
        | GrammarExpr::Repeat1(_) => None,
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
            ParserNormalizeErrorKind::UnmaterializedExtraRoot { expr } => {
                write!(f, "extra expression {} did not materialize", expr.get())
            }
            ParserNormalizeErrorKind::UnmaterializedReservedEntry { context, expr } => {
                write!(
                    f,
                    "reserved context {} entry {} did not materialize",
                    context.get(),
                    expr.get()
                )
            }
            ParserNormalizeErrorKind::UnmaterializedWord { rule, expr } => {
                write!(
                    f,
                    "word rule {} expression {} did not materialize",
                    rule.get(),
                    expr.get()
                )
            }
        }
    }
}

impl Error for ParserNormalizeError {}

/// Error produced while preparing normalized productions for LR item sets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParserPrepareError {
    kind: ParserPrepareErrorKind,
}

impl ParserPrepareError {
    fn new(kind: ParserPrepareErrorKind) -> Self {
        Self { kind }
    }

    /// Error kind.
    pub const fn kind(&self) -> &ParserPrepareErrorKind {
        &self.kind
    }
}

impl fmt::Display for ParserPrepareError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            ParserPrepareErrorKind::WrongStage { stage } => {
                write!(
                    f,
                    "parser grammar is at stage {stage:?}, not ProductionsPrepared"
                )
            }
            ParserPrepareErrorKind::RecursiveInline { nonterminal } => {
                write!(
                    f,
                    "inline nonterminal {} recursively references inline productions",
                    nonterminal.get()
                )
            }
            ParserPrepareErrorKind::NonproductiveNonterminal { nonterminal } => {
                write!(
                    f,
                    "reachable nonterminal {} cannot derive terminal output",
                    nonterminal.get()
                )
            }
            ParserPrepareErrorKind::NullableUsedNonterminal { nonterminal } => {
                write!(
                    f,
                    "used non-start nonterminal {} is nullable before LR generation",
                    nonterminal.get()
                )
            }
        }
    }
}

impl Error for ParserPrepareError {}

/// Error produced while building LR item sets and parse tables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParserTableBuildError {
    kind: ParserTableBuildErrorKind,
}

impl ParserTableBuildError {
    fn new(kind: ParserTableBuildErrorKind) -> Self {
        Self { kind }
    }

    /// Error kind.
    pub const fn kind(&self) -> &ParserTableBuildErrorKind {
        &self.kind
    }
}

impl fmt::Display for ParserTableBuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            ParserTableBuildErrorKind::WrongStage { stage } => {
                write!(f, "parser grammar is at stage {stage:?}, not Productions")
            }
            ParserTableBuildErrorKind::MissingItemPreparation => {
                write!(f, "parser grammar is missing LR item-preparation facts")
            }
            ParserTableBuildErrorKind::NoStartProductions { start } => {
                write!(f, "start nonterminal {} has no productions", start.get())
            }
        }
    }
}

impl Error for ParserTableBuildError {}

/// Parser table-build error kind.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ParserTableBuildErrorKind {
    /// The grammar was not in the expected stage.
    WrongStage {
        /// Current generation stage.
        stage: ParserGenerationStage,
    },
    /// Production graph facts were not prepared.
    MissingItemPreparation,
    /// No productions exist for the start nonterminal.
    NoStartProductions {
        /// Start nonterminal.
        start: NonterminalId,
    },
}

/// Parser production-preparation error kind.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ParserPrepareErrorKind {
    /// The grammar was not in the expected stage.
    WrongStage {
        /// Current generation stage.
        stage: ParserGenerationStage,
    },
    /// Inline declarations are recursive.
    RecursiveInline {
        /// Inline nonterminal involved in the recursion.
        nonterminal: NonterminalId,
    },
    /// A reachable nonterminal cannot derive any terminal/external output.
    NonproductiveNonterminal {
        /// Nonproductive nonterminal.
        nonterminal: NonterminalId,
    },
    /// A used non-start syntax nonterminal is nullable.
    NullableUsedNonterminal {
        /// Nullable nonterminal.
        nonterminal: NonterminalId,
    },
}

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
    /// Extra root could not be represented as a parser symbol.
    UnmaterializedExtraRoot {
        /// Extra expression id.
        expr: GrammarExprId,
    },
    /// Reserved entry could not be represented as a terminal.
    UnmaterializedReservedEntry {
        /// Reserved context id.
        context: ReservedSetId,
        /// Reserved entry expression id.
        expr: GrammarExprId,
    },
    /// Word rule could not be represented as a terminal.
    UnmaterializedWord {
        /// Word rule id.
        rule: RuleId,
        /// Word rule expression id.
        expr: GrammarExprId,
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
            .flat_map(|terminal| {
                terminal
                    .source_exprs()
                    .iter()
                    .copied()
                    .map(move |expr| (expr, terminal.id))
            })
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
            GrammarExpr::StringToken(_)
            | GrammarExpr::PatternToken { .. }
            | GrammarExpr::Until { .. }
            | GrammarExpr::Nested { .. }
            | GrammarExpr::AutoClose(_) => {
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
                    sequence.apply_static_precedence(precedence.clone());
                }
                Ok(sequences)
            }
            GrammarExpr::PrecDynamic { value, content } => {
                let mut sequences = self.lower_expr(owner, content)?;
                for sequence in &mut sequences {
                    sequence.dynamic_precedence =
                        strongest_dynamic_precedence(sequence.dynamic_precedence, Some(value));
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
                    content,
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
                        content,
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
                    content,
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
        let dynamic_precedence = sequence.dynamic_precedence.unwrap_or(0);
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
                    static_precedence: step.static_precedence,
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
    dynamic_precedence: Option<i32>,
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
                static_precedence: None,
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
        if self.source_expr.is_none() {
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

    fn apply_static_precedence(&mut self, precedence: StaticPrecedence) {
        for step in &mut self.steps {
            step.static_precedence = Some(precedence.clone());
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
    static_precedence: Option<StaticPrecedence>,
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

fn strongest_dynamic_precedence(left: Option<i32>, right: Option<i32>) -> Option<i32> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.max(right)),
        (Some(left), None) => Some(left),
        (None, Some(right)) => Some(right),
        (None, None) => None,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
    flags: Option<String>,
    source_exprs: Vec<GrammarExprId>,
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

    /// Regex flags for pattern terminals.
    pub fn flags(&self) -> Option<&str> {
        self.flags.as_deref()
    }

    /// Grammar expression that introduced this terminal symbol.
    pub fn source_expr(&self) -> GrammarExprId {
        self.source_exprs[0]
    }

    /// Grammar expressions that canonicalize to this terminal symbol.
    pub fn source_exprs(&self) -> &[GrammarExprId] {
        &self.source_exprs
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ParserTerminalKind {
    /// Literal string token.
    String,
    /// Regex pattern token.
    Pattern,
    /// Declarative implicit close token.
    AutoClose,
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
    static_precedence: Option<StaticPrecedence>,
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

    /// Static precedence applied at this structural child index.
    pub const fn static_precedence(&self) -> Option<&StaticPrecedence> {
        self.static_precedence.as_ref()
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
    AnonymousLiteral,
}

/// Public anonymous literal spelling and contributing parser terminals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicLiteralTerminals {
    literal: String,
    terminals: Vec<TerminalId>,
    source: PublicNodeKindSource,
}

impl PublicLiteralTerminals {
    /// Literal spelling.
    pub fn literal(&self) -> &str {
        &self.literal
    }

    /// Parser terminals that can produce this public literal spelling.
    pub fn terminals(&self) -> &[TerminalId] {
        &self.terminals
    }

    /// Public node-kind source.
    pub const fn source(&self) -> PublicNodeKindSource {
        self.source
    }
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
        /// Regex flags for pattern terminals.
        flags: Option<String>,
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

/// Facts produced once normalized productions are ready for LR item generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemPreparationFacts {
    inline_expansions: Vec<InlineExpansion>,
    graph: ProductionGraphFacts,
}

impl ItemPreparationFacts {
    /// Inline expansion mappings to be consumed by item-set construction.
    pub fn inline_expansions(&self) -> &[InlineExpansion] {
        &self.inline_expansions
    }

    /// Production graph facts.
    pub const fn graph(&self) -> &ProductionGraphFacts {
        &self.graph
    }
}

/// Inline rule mapped to the productions that must be expanded at use sites.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineExpansion {
    rule: RuleId,
    nonterminal: NonterminalId,
    productions: Vec<ProductionId>,
}

impl InlineExpansion {
    /// Source inline rule.
    pub const fn rule(&self) -> RuleId {
        self.rule
    }

    /// Parser nonterminal marked inline.
    pub const fn nonterminal(&self) -> NonterminalId {
        self.nonterminal
    }

    /// Productions owned by the inline nonterminal.
    pub fn productions(&self) -> &[ProductionId] {
        &self.productions
    }
}

/// Production graph facts used by FIRST/closure and table construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductionGraphFacts {
    nullable: Vec<NonterminalId>,
    productive: Vec<NonterminalId>,
    reachable: Vec<NonterminalId>,
}

impl ProductionGraphFacts {
    /// Nullable nonterminals.
    pub fn nullable(&self) -> &[NonterminalId] {
        &self.nullable
    }

    /// Productive nonterminals.
    pub fn productive(&self) -> &[NonterminalId] {
        &self.productive
    }

    /// Reachable nonterminals from the start symbol.
    pub fn reachable(&self) -> &[NonterminalId] {
        &self.reachable
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
        /// Repeated content expression.
        content: GrammarExprId,
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
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum StaticPrecedence {
    /// Integer precedence.
    Integer(i32),
    /// Named precedence.
    Named(String),
}

/// Production associativity.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum Associativity {
    /// No associativity override.
    #[default]
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
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
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
    /// Each item's lookahead set as `width` interned bitwords, packed in `items`
    /// order. Lets goto grouping OR lookaheads straight into the successor arena
    /// instead of re-interning `items[..].lookahead.symbols()` — the symbols were
    /// themselves just materialized from these bits.
    width: usize,
    lookahead_words: Vec<u64>,
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
    item_sets: Vec<ItemSet>,
    transitions: Vec<ItemTransition>,
    lexical_modes: Vec<LexMode>,
    valid_symbol_sets: Vec<ValidSymbolSet>,
    conflicts: Vec<TableConflict>,
    states: Vec<ParseState>,
}

impl ParseTable {
    /// Build LR item sets and parse-table action rows from prepared productions.
    pub fn from_grammar(grammar: &ParserGrammar) -> Result<Self, ParserTableBuildError> {
        LrTableBuilder::new(grammar)?.build()
    }

    /// LR item sets.
    pub fn item_sets(&self) -> &[ItemSet] {
        &self.item_sets
    }

    /// Item-set transitions.
    pub fn transitions(&self) -> &[ItemTransition] {
        &self.transitions
    }

    /// Lexical modes selected by parse states.
    pub fn lexical_modes(&self) -> &[LexMode] {
        &self.lexical_modes
    }

    /// External scanner valid-symbol sets selected by parse states.
    pub fn valid_symbol_sets(&self) -> &[ValidSymbolSet] {
        &self.valid_symbol_sets
    }

    /// Generated action conflicts retained for GLR dispatch.
    pub fn conflicts(&self) -> &[TableConflict] {
        &self.conflicts
    }

    /// Parse states.
    pub fn states(&self) -> &[ParseState] {
        &self.states
    }
}

/// Transition in the LR item graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemTransition {
    from: ItemSetId,
    symbol: ParserSymbol,
    to: ItemSetId,
}

impl ItemTransition {
    /// Source item set.
    pub const fn from(&self) -> ItemSetId {
        self.from
    }

    /// Symbol advanced by this transition.
    pub const fn symbol(&self) -> ParserSymbol {
        self.symbol
    }

    /// Target item set.
    pub const fn to(&self) -> ItemSetId {
        self.to
    }
}

struct LrTableBuilder<'a> {
    grammar: &'a ParserGrammar,
    productions_by_lhs: Vec<Vec<ProductionId>>,
    first: FirstFacts,
    /// Dense LR-item indexing: `(prod, dot)` maps to `item_base[prod] + dot`, a single
    /// `u32` in `0..item_count`. Lets the closure use flat arrays/flags instead of
    /// hashing `(prod, dot)` tuples. `item_key[dense] == (prod, dot)`, and because the
    /// mapping is monotonic, dense order *is* `(prod, dot)` order (canonical for dedup).
    item_base: Vec<u32>,
    item_key: Vec<(ProductionId, usize)>,
    item_count: usize,
}

impl<'a> LrTableBuilder<'a> {
    fn new(grammar: &'a ParserGrammar) -> Result<Self, ParserTableBuildError> {
        if grammar.stage != ParserGenerationStage::Productions {
            return Err(ParserTableBuildError::new(
                ParserTableBuildErrorKind::WrongStage {
                    stage: grammar.stage,
                },
            ));
        }
        let Some(item_preparation) = grammar.item_preparation.as_ref() else {
            return Err(ParserTableBuildError::new(
                ParserTableBuildErrorKind::MissingItemPreparation,
            ));
        };
        let productions_by_lhs = productions_by_lhs(grammar);
        if productions_by_lhs
            .get(grammar.start.get() as usize)
            .is_none_or(Vec::is_empty)
        {
            return Err(ParserTableBuildError::new(
                ParserTableBuildErrorKind::NoStartProductions {
                    start: grammar.start,
                },
            ));
        }
        let first = FirstFacts::new(grammar, item_preparation.graph());
        let mut item_base = Vec::with_capacity(grammar.productions.len());
        let mut item_key = Vec::new();
        let mut count = 0u32;
        for production in &grammar.productions {
            item_base.push(count);
            // dot ranges 0..=steps.len() (a complete item has dot == steps.len()).
            for dot in 0..=production.steps.len() {
                item_key.push((production.id, dot));
            }
            count += production.steps.len() as u32 + 1;
        }
        let item_count = count as usize;
        Ok(Self {
            grammar,
            productions_by_lhs,
            first,
            item_base,
            item_key,
            item_count,
        })
    }

    /// Dense LR-item index for `(production, dot)`.
    #[inline]
    fn dense(&self, production: ProductionId, dot: usize) -> u32 {
        self.item_base[production.get() as usize] + dot as u32
    }

    fn build(&self) -> Result<ParseTable, ParserTableBuildError> {
        let (item_sets, transitions) = self.item_sets();
        let (states, lexical_modes, valid_symbol_sets, conflicts) =
            self.parse_states(&item_sets, &transitions);
        Ok(ParseTable {
            item_sets,
            transitions,
            lexical_modes,
            valid_symbol_sets,
            conflicts,
            states,
        })
    }

    /// Words per lookahead bitset: the frozen lookahead universe rounded up to u64s.
    fn lookahead_width(&self) -> usize {
        self.first.interner.from_index.len().div_ceil(64).max(1)
    }

    fn item_sets(&self) -> (Vec<ItemSet>, Vec<ItemTransition>) {
        let mut item_sets = Vec::new();
        let mut item_set_keys = FxHashMap::<u64, Vec<ItemSetId>>::default();
        let mut sigs: Vec<Vec<u64>> = Vec::new();
        let mut pool: Vec<ItemMap> = Vec::new();
        let mut sig_buf: Vec<u64> = Vec::new();
        let mut order_buf: Vec<u32> = Vec::new();
        let mut scratch = ClosureScratch {
            queued: vec![false; self.item_count],
            ..ClosureScratch::default()
        };
        let interner = &self.first.interner;
        let width = self.lookahead_width();
        let item_count = self.item_count;
        let mut transitions = Vec::new();
        let mut queue = VecDeque::new();
        let mut start_items = take_map(&mut pool, width, item_count);
        for production in &self.productions_by_lhs[self.grammar.start.get() as usize] {
            start_items.insert(
                self.dense(*production, 0),
                &[LookaheadSymbol::Eof],
                interner,
            );
        }
        let start_items = self.closure(start_items, &mut scratch);
        let start = push_item_set(
            &mut item_sets,
            &mut item_set_keys,
            &mut sigs,
            &mut pool,
            &mut sig_buf,
            &mut order_buf,
            &self.item_key,
            start_items,
            interner,
            &mut queue,
        );
        debug_assert_eq!(start.get(), 0);

        while let Some(from) = queue.pop_front() {
            let grouped = self.group_goto_items(&item_sets[from.get() as usize], &mut pool);
            for (symbol, items) in grouped {
                let target_items = self.closure(items, &mut scratch);
                let to = push_item_set(
                    &mut item_sets,
                    &mut item_set_keys,
                    &mut sigs,
                    &mut pool,
                    &mut sig_buf,
                    &mut order_buf,
                    &self.item_key,
                    target_items,
                    interner,
                    &mut queue,
                );
                transitions.push(ItemTransition { from, symbol, to });
            }
        }

        (item_sets, transitions)
    }

    fn closure(&self, mut map: ItemMap, scratch: &mut ClosureScratch) -> ItemMap {
        // Worklist fixpoint: (re)process an item only when its lookahead set actually
        // grew, so the work is proportional to real propagation instead of
        // items × rounds (the old loop re-scanned and re-collected every key each
        // pass — ~1.5s of BTreeMap churn on gingembre). Inserts only ever target
        // (production, 0), so only those items can grow and get re-queued. All scratch
        // buffers are reused across closures (via the caller's ClosureScratch) so the
        // whole loop is allocation-free after warmup.
        scratch.worklist.clear();
        // `queued` is all-false here (self-clearing across closures). Seed from the
        // items already in the map.
        for &dense in &map.present {
            if !scratch.queued[dense as usize] {
                scratch.queued[dense as usize] = true;
                scratch.worklist.push_back(dense);
            }
        }
        while let Some(dense) = scratch.worklist.pop_front() {
            scratch.queued[dense as usize] = false;
            let (production_id, dot) = self.item_key[dense as usize];
            let production = &self.grammar.productions[production_id.get() as usize];
            let Some(step) = production.steps.get(dot) else {
                continue;
            };
            let ParserSymbol::Nonterminal(nonterminal) = step.symbol else {
                continue;
            };
            // `first` = FIRST(steps after the dot), extended by the item's own lookahead
            // (`fallback`) only when that suffix is entirely nullable. Both suffix facts
            // are precomputed, so this is a bitset copy plus at most one OR — no suffix
            // walk. The whole loop is bitwise OR — no Vec, no interning, no sort.
            let production_index = production_id.get() as usize;
            scratch.first.clear();
            scratch
                .first
                .or_from(&self.first.suffix_first[production_index][dot]);
            if self.first.suffix_nullable[production_index][dot] {
                scratch.fallback.clear();
                if let Some(current) = map.words_of(dense) {
                    scratch.fallback.or_from_slice(current);
                }
                scratch.first.or_from(&scratch.fallback);
            }
            for target in &self.productions_by_lhs[nonterminal.get() as usize] {
                let target_dense = self.dense(*target, 0);
                if map.or_into(target_dense, &scratch.first)
                    && !scratch.queued[target_dense as usize]
                {
                    scratch.queued[target_dense as usize] = true;
                    scratch.worklist.push_back(target_dense);
                }
            }
        }
        map
    }

    fn group_goto_items(
        &self,
        item_set: &ItemSet,
        pool: &mut Vec<ItemMap>,
    ) -> BTreeMap<ParserSymbol, ItemMap> {
        let width = self.lookahead_width();
        let item_count = self.item_count;
        let mut grouped = BTreeMap::<ParserSymbol, ItemMap>::new();
        for (index, item) in item_set.items.iter().enumerate() {
            let production = &self.grammar.productions[item.production.get() as usize];
            let Some(step) = production.steps.get(item.dot) else {
                continue;
            };
            let dense = self.dense(item.production, item.dot + 1);
            // The item's lookahead is already interned in `item_set.lookahead_words`
            // (packed in `items` order) — OR those bits straight in, no re-interning.
            let base = index * item_set.width;
            let words = &item_set.lookahead_words[base..base + item_set.width];
            grouped
                .entry(step.symbol)
                .or_insert_with(|| take_map(pool, width, item_count))
                .or_into_words(dense, words);
        }
        grouped
    }

    fn parse_states(
        &self,
        item_sets: &[ItemSet],
        transitions: &[ItemTransition],
    ) -> (
        Vec<ParseState>,
        Vec<LexMode>,
        Vec<ValidSymbolSet>,
        Vec<TableConflict>,
    ) {
        let mut transitions_by_from = vec![Vec::<ItemTransition>::new(); item_sets.len()];
        for transition in transitions {
            transitions_by_from[transition.from.get() as usize].push(*transition);
        }
        let mut states = Vec::new();
        let mut lexical_modes = Vec::new();
        let mut valid_symbol_sets = Vec::new();
        let mut conflicts = Vec::new();
        for item_set in item_sets {
            let state = ParseStateId::from_index(states.len());
            let mut entries = BTreeMap::<LookaheadSymbol, Vec<ParseAction>>::new();
            let mut shift_precedences =
                BTreeMap::<LookaheadSymbol, Vec<Option<StaticPrecedence>>>::new();
            let mut gotos = Vec::new();
            for transition in &transitions_by_from[item_set.id.get() as usize] {
                match transition.symbol {
                    ParserSymbol::Terminal(_) | ParserSymbol::External(_) => {
                        for (lookahead, precedence) in
                            self.transition_lookaheads(item_set, transition.symbol)
                        {
                            push_action(
                                &mut entries,
                                lookahead,
                                ParseAction::Shift {
                                    state: ParseStateId::from_index(transition.to.get() as usize),
                                    repetition: false,
                                },
                            );
                            let precedences = shift_precedences.entry(lookahead).or_default();
                            if !precedences.contains(&precedence) {
                                precedences.push(precedence);
                            }
                        }
                    }
                    ParserSymbol::Nonterminal(nonterminal) => gotos.push(GotoEntry {
                        nonterminal,
                        state: ParseStateId::from_index(transition.to.get() as usize),
                    }),
                    ParserSymbol::Eof => push_action(
                        &mut entries,
                        LookaheadSymbol::Eof,
                        ParseAction::Shift {
                            state: ParseStateId::from_index(transition.to.get() as usize),
                            repetition: false,
                        },
                    ),
                    ParserSymbol::Internal(_) => {}
                }
            }
            for lookahead in self.extra_lookaheads() {
                push_action(&mut entries, lookahead, ParseAction::ShiftExtra);
            }
            for item in &item_set.items {
                let production = &self.grammar.productions[item.production.get() as usize];
                if item.dot != production.steps.len() {
                    continue;
                }
                for lookahead in item.lookahead.symbols() {
                    if production.lhs == self.grammar.start && *lookahead == LookaheadSymbol::Eof {
                        push_action(
                            &mut entries,
                            *lookahead,
                            ParseAction::Accept {
                                production: production.id,
                                metadata: production.metadata,
                                symbol: production.lhs,
                                child_count: production.steps.len(),
                                dynamic_precedence: production.dynamic_precedence,
                            },
                        );
                    } else {
                        push_action(
                            &mut entries,
                            *lookahead,
                            ParseAction::Reduce {
                                production: production.id,
                                metadata: production.metadata,
                                symbol: production.lhs,
                                child_count: production.steps.len(),
                                dynamic_precedence: production.dynamic_precedence,
                            },
                        );
                    }
                }
            }
            resolve_static_conflicts(&mut entries, &shift_precedences, self.grammar);
            let lex_mode = lex_mode_from_entries(
                &entries,
                &mut lexical_modes,
                &mut valid_symbol_sets,
                self.grammar.word,
            );
            for (lookahead, actions) in &entries {
                if actions.len() > 1 {
                    conflicts.push(TableConflict {
                        id: ConflictId::from_index(conflicts.len()),
                        state,
                        lookahead: *lookahead,
                        actions: actions.clone(),
                    });
                }
            }
            states.push(ParseState {
                id: state,
                item_set: item_set.id,
                entries: entries
                    .into_iter()
                    .map(|(lookahead, actions)| TableEntry { lookahead, actions })
                    .collect(),
                gotos,
                lex_mode,
            });
        }
        (states, lexical_modes, valid_symbol_sets, conflicts)
    }

    fn transition_lookaheads(
        &self,
        item_set: &ItemSet,
        symbol: ParserSymbol,
    ) -> Vec<(LookaheadSymbol, Option<StaticPrecedence>)> {
        let mut lookaheads = Vec::new();
        for item in &item_set.items {
            let production = &self.grammar.productions[item.production.get() as usize];
            let Some(step) = production.steps.get(item.dot) else {
                continue;
            };
            if step.symbol == symbol
                && let Some(lookahead) = lookahead_for_step(step)
            {
                lookaheads.push((lookahead, step.static_precedence.clone()));
            }
        }
        lookaheads.sort();
        lookaheads.dedup();
        lookaheads
    }

    fn extra_lookaheads(&self) -> Vec<LookaheadSymbol> {
        let mut lookaheads = Vec::new();
        for extra in &self.grammar.extra_roots {
            match extra.symbol {
                ParserSymbol::Terminal(terminal) => {
                    lookaheads.push(LookaheadSymbol::Terminal(terminal))
                }
                ParserSymbol::External(external) => {
                    lookaheads.push(LookaheadSymbol::External(external))
                }
                ParserSymbol::Nonterminal(nonterminal) => {
                    extend_lookaheads(
                        &mut lookaheads,
                        &self.first.first[nonterminal.get() as usize],
                    );
                }
                ParserSymbol::Eof | ParserSymbol::Internal(_) => {}
            }
        }
        sorted_lookaheads(lookaheads)
    }
}

/// Builder-local dense index of the lookahead symbols seen while closing one item
/// set. Lets `ItemMap` store lookahead sets as bitsets keyed by index, so a merge is
/// a bitwise OR instead of re-sorting a `Vec` on every insert — which was ~90% of
/// `ParseTable::from_grammar` on heavy grammars (`LookaheadSet::merge` re-sorted the
/// whole set on each of the many LR-closure merges).
#[derive(Debug, Clone, Default)]
struct LookaheadInterner {
    to_index: FxHashMap<LookaheadSymbol, u32>,
    from_index: Vec<LookaheadSymbol>,
}

impl LookaheadInterner {
    fn intern(&mut self, symbol: LookaheadSymbol) -> u32 {
        if let Some(&index) = self.to_index.get(&symbol) {
            return index;
        }
        let index = self.from_index.len() as u32;
        self.from_index.push(symbol);
        self.to_index.insert(symbol, index);
        index
    }

    /// Index of an already-interned symbol. The interner is fully populated with every
    /// possible lookahead symbol (all FIRST sets, every step lookahead, and EOF) before
    /// the closure runs, so this never misses during table construction.
    fn index_of(&self, symbol: LookaheadSymbol) -> u32 {
        self.to_index
            .get(&symbol)
            .copied()
            .expect("lookahead symbol should be pre-interned")
    }
}

/// Growable bitset over interned lookahead indices.
#[derive(Debug, Clone, Default)]
struct LookaheadBitset {
    words: Vec<u64>,
}

impl LookaheadBitset {
    /// Set the bit for `index`; returns whether it was newly set (i.e. the set grew).
    fn set(&mut self, index: u32) -> bool {
        let word = index as usize / 64;
        let bit = 1u64 << (index % 64);
        if word >= self.words.len() {
            self.words.resize(word + 1, 0);
        }
        let newly = self.words[word] & bit == 0;
        self.words[word] |= bit;
        newly
    }

    /// OR `other` into `self`; returns whether `self` gained any bit.
    fn or_from(&mut self, other: &LookaheadBitset) -> bool {
        self.or_from_slice(&other.words)
    }

    /// OR raw words (e.g. an arena row) into `self`; returns whether `self` grew.
    fn or_from_slice(&mut self, other: &[u64]) -> bool {
        if other.len() > self.words.len() {
            self.words.resize(other.len(), 0);
        }
        let mut grew = false;
        for (word, &incoming) in self.words.iter_mut().zip(other) {
            grew |= incoming & !*word != 0;
            *word |= incoming;
        }
        grew
    }

    /// Clear all bits, keeping the allocation for reuse.
    fn clear(&mut self) {
        self.words.clear();
    }
}

/// Reusable scratch for the LR closure fixpoint, threaded across closures so the
/// worklist/queue and FIRST bitsets are allocated once for the whole build instead
/// of per item set.
#[derive(Default)]
struct ClosureScratch {
    /// `queued[dense]` = item is on the worklist. Self-clearing: every set flag is
    /// cleared when its item is popped, so the whole array is false between closures
    /// and needs no reset. Sized to the builder's `item_count`.
    queued: Vec<bool>,
    worklist: std::collections::VecDeque<u32>,
    fallback: LookaheadBitset,
    first: LookaheadBitset,
}

/// One LR item set under construction, dense-indexed. Every lookahead set is exactly
/// `width` u64 words (the lookahead universe is frozen), so all rows live in one flat
/// arena: dense item `d` at `(row_of[d] - 1) * width`. `row_of` (0 = absent) and the
/// `present` list are keyed by the dense item index — no `(prod, dot)` hashing in the
/// closure hot loop.
#[derive(Debug, Clone)]
struct ItemMap {
    width: usize,
    row_of: Vec<u32>,
    present: Vec<u32>,
    words: Vec<u64>,
}

impl ItemMap {
    fn new(width: usize, item_count: usize) -> Self {
        Self {
            width: width.max(1),
            row_of: vec![0; item_count],
            present: Vec::new(),
            words: Vec::new(),
        }
    }

    /// Clear for reuse from the pool, keeping allocations — only touched rows reset.
    fn reset(&mut self) {
        for &dense in &self.present {
            self.row_of[dense as usize] = 0;
        }
        self.present.clear();
        self.words.clear();
    }

    /// Byte offset of dense item `dense`'s row, appending a zeroed row if new.
    fn row_base(&mut self, dense: u32) -> usize {
        let slot = self.row_of[dense as usize];
        if slot != 0 {
            return (slot as usize - 1) * self.width;
        }
        let base = self.words.len();
        self.words.resize(base + self.width, 0);
        self.row_of[dense as usize] = (base / self.width) as u32 + 1;
        self.present.push(dense);
        base
    }

    /// The row words for dense item `dense`, if present.
    fn words_of(&self, dense: u32) -> Option<&[u64]> {
        let slot = self.row_of[dense as usize];
        if slot == 0 {
            return None;
        }
        let base = (slot as usize - 1) * self.width;
        Some(&self.words[base..base + self.width])
    }

    /// Symbol-keyed insert for seeding and goto grouping (not the closure hot loop).
    /// The interner is frozen, so this is a pure lookup + bit set.
    fn insert(
        &mut self,
        dense: u32,
        lookaheads: &[LookaheadSymbol],
        interner: &LookaheadInterner,
    ) -> bool {
        let base = self.row_base(dense);
        let mut changed = false;
        for &symbol in lookaheads {
            let index = interner.index_of(symbol) as usize;
            let word = base + index / 64;
            let bit = 1u64 << (index % 64);
            changed |= self.words[word] & bit == 0;
            self.words[word] |= bit;
        }
        changed
    }

    /// Bitset OR into an item's lookahead set — the LR-closure hot path. `bits` holds
    /// at most `width` words (indices are interned `< universe`). Returns whether the
    /// set grew.
    fn or_into(&mut self, dense: u32, bits: &LookaheadBitset) -> bool {
        self.or_into_words(dense, &bits.words)
    }

    /// OR packed lookahead words (e.g. a stored `ItemSet`'s row) into item `dense`.
    fn or_into_words(&mut self, dense: u32, words: &[u64]) -> bool {
        let base = self.row_base(dense);
        let mut grew = false;
        for (offset, &incoming) in words.iter().enumerate() {
            let word = base + offset;
            grew |= incoming & !self.words[word] != 0;
            self.words[word] |= incoming;
        }
        grew
    }

    /// Present dense indices sorted into `order` — dense order *is* (prod, dot) order,
    /// so this is the canonical iteration order for both dedup and materialization.
    fn sorted_present(&self, order: &mut Vec<u32>) {
        order.clear();
        order.extend_from_slice(&self.present);
        order.sort_unstable();
    }

    /// Materialize this item set into canonical `Vec<LrItem>` (symbols) plus the same
    /// lookaheads packed as `width` bitwords per item, in the same order. Only
    /// genuinely-new states reach here (borrows so the map can be recycled).
    fn materialize(
        &self,
        item_key: &[(ProductionId, usize)],
        interner: &LookaheadInterner,
    ) -> (Vec<LrItem>, Vec<u64>) {
        let mut order = Vec::with_capacity(self.present.len());
        self.sorted_present(&mut order);
        let mut items = Vec::with_capacity(order.len());
        let mut packed = Vec::with_capacity(order.len() * self.width);
        for &dense in &order {
            let (production, dot) = item_key[dense as usize];
            let base = (self.row_of[dense as usize] as usize - 1) * self.width;
            let words = &self.words[base..base + self.width];
            packed.extend_from_slice(words);
            // Ascending bit index = symbol order (the interner is built sorted),
            // so the lookahead set materializes already sorted with no sort.
            let mut symbols = Vec::new();
            for (word_index, &word) in words.iter().enumerate() {
                let mut bits = word;
                while bits != 0 {
                    let index = word_index * 64 + bits.trailing_zeros() as usize;
                    symbols.push(interner.from_index[index]);
                    bits &= bits - 1;
                }
            }
            items.push(LrItem {
                production,
                dot,
                lookahead: LookaheadSet { symbols },
            });
        }
        (items, packed)
    }

    /// Write this item set's dense canonical dedup key into `sig` (cleared first),
    /// using `order` as scratch: the sorted dense indices (already (prod, dot) order,
    /// each uniquely encoding the item) plus each item's lookahead words with trailing
    /// zero words trimmed. Cheap to hash/compare; both buffers reused across item sets.
    fn write_signature(&self, order: &mut Vec<u32>, sig: &mut Vec<u64>) {
        self.sorted_present(order);
        sig.clear();
        for &dense in order.iter() {
            let base = (self.row_of[dense as usize] as usize - 1) * self.width;
            let words = &self.words[base..base + self.width];
            let end = words.iter().rposition(|&w| w != 0).map_or(0, |i| i + 1);
            sig.push(u64::from(dense));
            sig.push(end as u64);
            sig.extend_from_slice(&words[..end]);
        }
    }
}

#[derive(Debug, Clone)]
struct FirstFacts {
    first: Vec<Vec<LookaheadSymbol>>,
    /// Shared, frozen index of every possible lookahead symbol. Built once so the LR
    /// closure works entirely in bitsets (no per-symbol interning in the hot loop).
    interner: LookaheadInterner,
    /// Per (production, dot): FIRST(steps[dot+1..]) as a bitset, and whether that
    /// suffix is entirely nullable (so the item's own lookahead — the "fallback" —
    /// joins it). Both are grammar-static, so the closure hot loop just copies the
    /// cached bitset and maybe ORs the fallback instead of re-walking the suffix.
    suffix_first: Vec<Vec<LookaheadBitset>>,
    suffix_nullable: Vec<Vec<bool>>,
}

impl FirstFacts {
    fn new(grammar: &ParserGrammar, graph: &ProductionGraphFacts) -> Self {
        let mut nullable = vec![false; grammar.symbols.nonterminals.len()];
        for nonterminal in graph.nullable() {
            nullable[nonterminal.get() as usize] = true;
        }
        let mut first = vec![Vec::new(); grammar.symbols.nonterminals.len()];
        loop {
            let mut changed = false;
            for production in &grammar.productions {
                let lhs = production.lhs.get() as usize;
                let symbols = first_of_steps_with_tables(&production.steps, &nullable, &first, &[]);
                changed |= extend_lookaheads(&mut first[lhs], &symbols);
            }
            if !changed {
                break;
            }
        }

        // Freeze a complete lookahead interner: every FIRST-set symbol, every step's
        // lookahead terminal, and EOF — every symbol the closure can ever place in a
        // lookahead set — so `index_of` never misses and the loop needs no interning.
        // Intern in LookaheadSymbol order (via a sorted set) so a bitset's ascending
        // index order IS symbol order — materialized lookahead sets come out sorted with
        // no per-item sort (which dominated the table build outside the closure).
        let mut all_symbols = std::collections::BTreeSet::new();
        for symbols in &first {
            all_symbols.extend(symbols.iter().copied());
        }
        all_symbols.insert(LookaheadSymbol::Eof);
        for production in &grammar.productions {
            for step in &production.steps {
                if let Some(symbol) = lookahead_for_step(step) {
                    all_symbols.insert(symbol);
                }
            }
        }
        let mut interner = LookaheadInterner::default();
        for symbol in all_symbols {
            interner.intern(symbol);
        }
        let first_bits: Vec<LookaheadBitset> = first
            .iter()
            .map(|symbols| {
                let mut bits = LookaheadBitset::default();
                for &symbol in symbols {
                    bits.set(interner.index_of(symbol));
                }
                bits
            })
            .collect();

        // Precompute FIRST(steps[dot+1..]) per (production, dot) — the static part of
        // the closure's lookahead computation — so the hot loop never re-walks a suffix.
        let mut suffix_first = Vec::with_capacity(grammar.productions.len());
        let mut suffix_nullable = Vec::with_capacity(grammar.productions.len());
        for production in &grammar.productions {
            let steps = &production.steps;
            let mut per_dot_first = Vec::with_capacity(steps.len());
            let mut per_dot_nullable = Vec::with_capacity(steps.len());
            for dot in 0..steps.len() {
                let mut bits = LookaheadBitset::default();
                let mut all_nullable = true;
                for step in &steps[dot + 1..] {
                    match lookahead_for_step(step) {
                        Some(symbol) => {
                            bits.set(interner.index_of(symbol));
                            all_nullable = false;
                            break;
                        }
                        None => {
                            let ParserSymbol::Nonterminal(nonterminal) = step.symbol else {
                                unreachable!("non-lookahead parser symbol should be nonterminal");
                            };
                            bits.or_from(&first_bits[nonterminal.get() as usize]);
                            if !nullable[nonterminal.get() as usize] {
                                all_nullable = false;
                                break;
                            }
                        }
                    }
                }
                per_dot_first.push(bits);
                per_dot_nullable.push(all_nullable);
            }
            suffix_first.push(per_dot_first);
            suffix_nullable.push(per_dot_nullable);
        }

        Self {
            first,
            interner,
            suffix_first,
            suffix_nullable,
        }
    }
}

fn productions_by_lhs(grammar: &ParserGrammar) -> Vec<Vec<ProductionId>> {
    let mut by_lhs = vec![Vec::new(); grammar.symbols.nonterminals.len()];
    for production in &grammar.productions {
        by_lhs[production.lhs.get() as usize].push(production.id);
    }
    by_lhs
}

/// Append FIRST(`steps`) — falling through to `fallback` once every step is nullable
/// — to `out`, WITHOUT sorting or deduping. Callers dedup either via a bitset (the LR
/// closure) or a final `sorted_lookaheads` (the wrapper below). Allocation-free given
/// a reused `out`, which is the whole point on the closure hot path.
fn first_of_steps_with_tables_into(
    steps: &[ProductionStep],
    nullable: &[bool],
    first: &[Vec<LookaheadSymbol>],
    fallback: &[LookaheadSymbol],
    out: &mut Vec<LookaheadSymbol>,
) {
    for step in steps {
        match lookahead_for_step(step) {
            Some(
                lookahead @ (LookaheadSymbol::Terminal(_)
                | LookaheadSymbol::External(_)
                | LookaheadSymbol::Eof
                | LookaheadSymbol::ReservedWord { .. }
                | LookaheadSymbol::ErrorRecovery(_)),
            ) => {
                out.push(lookahead);
                return;
            }
            None => {
                let ParserSymbol::Nonterminal(nonterminal) = step.symbol else {
                    unreachable!("non-lookahead parser symbol should be nonterminal");
                };
                out.extend_from_slice(&first[nonterminal.get() as usize]);
                if !nullable[nonterminal.get() as usize] {
                    return;
                }
            }
        }
    }
    out.extend_from_slice(fallback);
}

fn first_of_steps_with_tables(
    steps: &[ProductionStep],
    nullable: &[bool],
    first: &[Vec<LookaheadSymbol>],
    fallback: &[LookaheadSymbol],
) -> Vec<LookaheadSymbol> {
    let mut out = Vec::new();
    first_of_steps_with_tables_into(steps, nullable, first, fallback, &mut out);
    sorted_lookaheads(out)
}

fn lookahead_for_step(step: &ProductionStep) -> Option<LookaheadSymbol> {
    match step.symbol {
        ParserSymbol::Terminal(terminal) => Some(match step.reserved_context {
            Some(context) => LookaheadSymbol::ReservedWord { terminal, context },
            None => LookaheadSymbol::Terminal(terminal),
        }),
        ParserSymbol::External(external) => Some(LookaheadSymbol::External(external)),
        ParserSymbol::Eof => Some(LookaheadSymbol::Eof),
        ParserSymbol::Internal(internal) => Some(LookaheadSymbol::ErrorRecovery(internal)),
        ParserSymbol::Nonterminal(_) => None,
    }
}

/// Pull a cleared `ItemMap` from the pool (buffers retained) or make a fresh one.
fn take_map(pool: &mut Vec<ItemMap>, width: usize, item_count: usize) -> ItemMap {
    match pool.pop() {
        Some(mut map) => {
            map.width = width.max(1);
            map
        }
        None => ItemMap::new(width, item_count),
    }
}

/// Return a consumed `ItemMap` to the pool, keeping its allocations so the next item
/// set reuses them (no realloc after warmup).
fn recycle_map(pool: &mut Vec<ItemMap>, mut map: ItemMap) {
    map.reset();
    pool.push(map);
}

#[allow(clippy::too_many_arguments)]
fn push_item_set(
    item_sets: &mut Vec<ItemSet>,
    item_set_index: &mut FxHashMap<u64, Vec<ItemSetId>>,
    sigs: &mut Vec<Vec<u64>>,
    pool: &mut Vec<ItemMap>,
    sig: &mut Vec<u64>,
    order: &mut Vec<u32>,
    item_key: &[(ProductionId, usize)],
    map: ItemMap,
    interner: &LookaheadInterner,
    queue: &mut VecDeque<ItemSetId>,
) -> ItemSetId {
    // Dedup identical LR item sets (state merging) on a dense bitset *signature* —
    // the sorted (prod, dot) keys plus their lookahead words — hashed to bucket and
    // compared word-for-word only within a bucket. Crucially we dedup *before*
    // materializing `Vec<LrItem>`: most goto targets are duplicates of existing
    // states, so building their symbol vecs was pure waste. Only a genuinely-new
    // state pays materialization (and a sig clone); either way the map's buffers go
    // back to the pool and the sig/keys scratch is reused.
    map.write_signature(order, sig);
    let hash = hash_signature(sig);
    let bucket = item_set_index.entry(hash).or_default();
    for &existing in bucket.iter() {
        if sigs[existing.get() as usize] == *sig {
            recycle_map(pool, map);
            return existing;
        }
    }
    let id = ItemSetId::from_index(item_sets.len());
    bucket.push(id);
    let width = map.width;
    let (items, lookahead_words) = map.materialize(item_key, interner);
    item_sets.push(ItemSet {
        id,
        items,
        width,
        lookahead_words,
    });
    sigs.push(sig.clone());
    recycle_map(pool, map);
    queue.push_back(id);
    id
}

fn hash_signature(sig: &[u64]) -> u64 {
    use std::hash::Hasher;
    let mut hasher = FxHasher::default();
    for &word in sig {
        hasher.write_u64(word);
    }
    hasher.finish()
}

/// Cheap non-cryptographic hasher (FxHash-style) for LR item-set dedup. SipHash's
/// per-state cost dominated the table build; this is a rotate/xor/multiply mixer.
#[derive(Default)]
struct FxHasher {
    hash: u64,
}

impl FxHasher {
    const SEED: u64 = 0x51_7c_c1_b7_27_22_0a_95;

    #[inline]
    fn add(&mut self, word: u64) {
        self.hash = (self.hash.rotate_left(5) ^ word).wrapping_mul(Self::SEED);
    }
}

impl std::hash::Hasher for FxHasher {
    #[inline]
    fn finish(&self) -> u64 {
        self.hash
    }
    #[inline]
    fn write_u64(&mut self, i: u64) {
        self.add(i);
    }
    #[inline]
    fn write_u32(&mut self, i: u32) {
        self.add(u64::from(i));
    }
    #[inline]
    fn write_usize(&mut self, i: usize) {
        self.add(i as u64);
    }
    fn write(&mut self, bytes: &[u8]) {
        let mut chunks = bytes.chunks_exact(8);
        for chunk in chunks.by_ref() {
            self.add(u64::from_le_bytes(chunk.try_into().unwrap()));
        }
        for &byte in chunks.remainder() {
            self.add(u64::from(byte));
        }
    }
}

/// `BuildHasher` for `FxHasher` so the closure's item map/queue can be plain hash
/// collections keyed on integer ids — O(1) with no ordered-tree node splits/memmoves
/// (which dominated the closure after the materialization cut). Deterministic (no
/// random seed), so table builds stay reproducible.
#[derive(Default, Clone)]
struct BuildFxHasher;

impl std::hash::BuildHasher for BuildFxHasher {
    type Hasher = FxHasher;
    fn build_hasher(&self) -> FxHasher {
        FxHasher::default()
    }
}

type FxHashMap<K, V> = std::collections::HashMap<K, V, BuildFxHasher>;

fn sorted_lookaheads(mut symbols: Vec<LookaheadSymbol>) -> Vec<LookaheadSymbol> {
    symbols.sort();
    symbols.dedup();
    symbols
}

fn extend_lookaheads(out: &mut Vec<LookaheadSymbol>, symbols: &[LookaheadSymbol]) -> bool {
    let old_len = out.len();
    out.extend_from_slice(symbols);
    *out = sorted_lookaheads(std::mem::take(out));
    out.len() != old_len
}

fn push_action(
    entries: &mut BTreeMap<LookaheadSymbol, Vec<ParseAction>>,
    lookahead: LookaheadSymbol,
    action: ParseAction,
) {
    let actions = entries.entry(lookahead).or_default();
    if !actions.contains(&action) {
        actions.push(action);
    }
}

fn resolve_static_conflicts(
    entries: &mut BTreeMap<LookaheadSymbol, Vec<ParseAction>>,
    shift_precedences: &BTreeMap<LookaheadSymbol, Vec<Option<StaticPrecedence>>>,
    grammar: &ParserGrammar,
) {
    for (lookahead, actions) in entries.iter_mut() {
        if actions.len() < 2 {
            continue;
        }

        resolve_reduce_reduce_conflicts(actions, grammar);
        if actions.len() < 2
            || !actions
                .iter()
                .any(|action| matches!(action, ParseAction::Shift { .. }))
        {
            continue;
        }

        let shifts = shift_precedences
            .get(lookahead)
            .cloned()
            .unwrap_or_else(|| vec![None]);
        let decisions = actions
            .iter()
            .enumerate()
            .filter_map(|(index, action)| {
                reduce_action_metadata(grammar, action).map(|metadata| {
                    (
                        index,
                        resolve_reduce_against_shifts(grammar, metadata, &shifts),
                    )
                })
            })
            .collect::<Vec<_>>();
        if decisions.is_empty() {
            continue;
        }

        let remove_reduces = decisions
            .iter()
            .filter_map(|(index, decision)| {
                (*decision == StaticConflictDecision::ReduceLoses).then_some(*index)
            })
            .collect::<BTreeSet<_>>();
        let remaining_decisions = decisions
            .iter()
            .filter(|(index, _)| !remove_reduces.contains(index))
            .map(|(_, decision)| *decision)
            .collect::<Vec<_>>();
        let remove_shift = !remaining_decisions.is_empty()
            && remaining_decisions
                .iter()
                .all(|decision| *decision == StaticConflictDecision::ShiftLoses);

        let mut index = 0usize;
        actions.retain(|action| {
            let keep = match action {
                ParseAction::Shift { .. } => !remove_shift,
                ParseAction::Reduce { .. } => !remove_reduces.contains(&index),
                _ => true,
            };
            index += 1;
            keep
        });
    }
}

fn resolve_reduce_reduce_conflicts(actions: &mut Vec<ParseAction>, grammar: &ParserGrammar) {
    let reduce_indices = actions
        .iter()
        .enumerate()
        .filter_map(|(index, action)| reduce_action_metadata(grammar, action).map(|_| index))
        .collect::<Vec<_>>();
    if reduce_indices.len() < 2 {
        return;
    }

    let mut strongest = Vec::<usize>::new();
    for index in reduce_indices {
        let precedence = reduce_action_metadata(grammar, &actions[index])
            .and_then(ProductionMetadata::static_precedence);
        match strongest.first().copied() {
            None => strongest.push(index),
            Some(current_index) => {
                let current = reduce_action_metadata(grammar, &actions[current_index])
                    .and_then(ProductionMetadata::static_precedence);
                match compare_static_precedence_options(grammar, precedence, current) {
                    std::cmp::Ordering::Greater => {
                        strongest.clear();
                        strongest.push(index);
                    }
                    std::cmp::Ordering::Equal => strongest.push(index),
                    std::cmp::Ordering::Less => {}
                }
            }
        }
    }

    let strongest = strongest.into_iter().collect::<BTreeSet<_>>();
    let mut index = 0usize;
    actions.retain(|action| {
        let keep = !matches!(action, ParseAction::Reduce { .. }) || strongest.contains(&index);
        index += 1;
        keep
    });
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StaticConflictDecision {
    ShiftLoses,
    ReduceLoses,
    Unresolved,
}

fn resolve_reduce_against_shifts(
    grammar: &ParserGrammar,
    reduce_metadata: &ProductionMetadata,
    shifts: &[Option<StaticPrecedence>],
) -> StaticConflictDecision {
    let reduce_precedence = reduce_metadata.static_precedence();
    let mut saw_shift_greater = false;
    let mut saw_reduce_greater = false;
    let mut saw_equal = false;
    for shift in shifts {
        match compare_static_precedence_options(grammar, shift.as_ref(), reduce_precedence) {
            std::cmp::Ordering::Greater => saw_shift_greater = true,
            std::cmp::Ordering::Less => saw_reduce_greater = true,
            std::cmp::Ordering::Equal => saw_equal = true,
        }
    }

    match (saw_shift_greater, saw_reduce_greater, saw_equal) {
        (true, false, false) => StaticConflictDecision::ReduceLoses,
        (false, true, false) => StaticConflictDecision::ShiftLoses,
        (false, false, true) => match reduce_metadata.associativity() {
            Associativity::Left => StaticConflictDecision::ShiftLoses,
            Associativity::Right => StaticConflictDecision::ReduceLoses,
            Associativity::None => StaticConflictDecision::Unresolved,
        },
        _ => StaticConflictDecision::Unresolved,
    }
}

fn reduce_action_metadata<'a>(
    grammar: &'a ParserGrammar,
    action: &ParseAction,
) -> Option<&'a ProductionMetadata> {
    let metadata = match *action {
        ParseAction::Reduce { metadata, .. } => metadata,
        _ => return None,
    };
    grammar.production_metadata.get(metadata.get() as usize)
}

fn compare_static_precedence_options(
    grammar: &ParserGrammar,
    left: Option<&StaticPrecedence>,
    right: Option<&StaticPrecedence>,
) -> std::cmp::Ordering {
    match (left, right) {
        (Some(left), Some(right)) => compare_static_precedence(grammar, left, right),
        (Some(StaticPrecedence::Integer(left)), None) => left.cmp(&0),
        (None, Some(StaticPrecedence::Integer(right))) => 0.cmp(right),
        (None, None)
        | (Some(StaticPrecedence::Named(_)), None)
        | (None, Some(StaticPrecedence::Named(_))) => std::cmp::Ordering::Equal,
    }
}

fn compare_static_precedence(
    grammar: &ParserGrammar,
    left: &StaticPrecedence,
    right: &StaticPrecedence,
) -> std::cmp::Ordering {
    match (left, right) {
        (StaticPrecedence::Integer(left), StaticPrecedence::Integer(right)) => left.cmp(right),
        (StaticPrecedence::Named(left), StaticPrecedence::Named(right)) if left == right => {
            std::cmp::Ordering::Equal
        }
        (StaticPrecedence::Named(left), StaticPrecedence::Named(right)) => {
            compare_named_precedence(grammar, left, right)
        }
        _ => std::cmp::Ordering::Equal,
    }
}

fn compare_named_precedence(
    grammar: &ParserGrammar,
    left: &str,
    right: &str,
) -> std::cmp::Ordering {
    for group in &grammar.precedence_groups {
        let left_index = group.entries().iter().position(|entry| match entry {
            PrecedenceGroupEntry::Name(name) => name == left,
            PrecedenceGroupEntry::Nonterminal(_) => false,
        });
        let right_index = group.entries().iter().position(|entry| match entry {
            PrecedenceGroupEntry::Name(name) => name == right,
            PrecedenceGroupEntry::Nonterminal(_) => false,
        });
        if let (Some(left_index), Some(right_index)) = (left_index, right_index) {
            return right_index.cmp(&left_index);
        }
    }
    std::cmp::Ordering::Equal
}

fn lex_mode_from_entries(
    entries: &BTreeMap<LookaheadSymbol, Vec<ParseAction>>,
    lexical_modes: &mut Vec<LexMode>,
    valid_symbol_sets: &mut Vec<ValidSymbolSet>,
    word: Option<TerminalId>,
) -> LexModeId {
    let mut terminals = Vec::new();
    let mut externals = Vec::new();
    let mut reserved_context = None;
    for lookahead in entries.keys() {
        match *lookahead {
            LookaheadSymbol::Terminal(terminal) => terminals.push(terminal),
            LookaheadSymbol::External(external) => externals.push(external),
            LookaheadSymbol::ReservedWord { terminal, context } => {
                terminals.push(terminal);
                reserved_context = Some(context);
            }
            LookaheadSymbol::Eof | LookaheadSymbol::ErrorRecovery(_) => {}
        }
    }
    terminals.sort();
    terminals.dedup();
    externals.sort();
    externals.dedup();
    let id = LexModeId::from_index(lexical_modes.len());
    let valid_symbols = if externals.is_empty() {
        None
    } else {
        let id = ValidSymbolSetId::from_index(valid_symbol_sets.len());
        valid_symbol_sets.push(ValidSymbolSet {
            id,
            externals: externals.clone(),
        });
        Some(id)
    };
    lexical_modes.push(LexMode {
        id,
        terminals,
        externals,
        reserved_context,
        valid_symbols,
        word,
    });
    id
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

/// One generated parse-table action conflict retained for GLR execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableConflict {
    id: ConflictId,
    state: ParseStateId,
    lookahead: LookaheadSymbol,
    actions: Vec<ParseAction>,
}

impl TableConflict {
    /// Conflict id.
    pub const fn id(&self) -> ConflictId {
        self.id
    }

    /// Parse state containing the conflict.
    pub const fn state(&self) -> ParseStateId {
        self.state
    }

    /// Lookahead key for the conflicted action cell.
    pub const fn lookahead(&self) -> LookaheadSymbol {
        self.lookahead
    }

    /// Conflicting actions retained for GLR dispatch.
    pub fn actions(&self) -> &[ParseAction] {
        &self.actions
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CompiledLexMode {
    pub(crate) terminals: Vec<CompiledLexTerminal>,
    pub(crate) external_count: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct CompiledLexTerminal {
    pub(crate) terminal: TerminalId,
    pub(crate) matcher: CompiledTerminalMatcher,
    pub(crate) immediate: bool,
    pub(crate) literal: bool,
    pub(crate) lexical_precedence: i32,
    pub(crate) implicit_precedence: i32,
}

#[derive(Debug, Clone)]
pub(crate) enum CompiledTerminalMatcher {
    Expr(CompiledLexExpr),
    UnsupportedTerminal { terminal: TerminalId },
}

#[derive(Debug, Clone)]
pub(crate) enum CompiledLexExpr {
    Blank,
    String(String),
    Pattern(CompiledLexPattern),
    Until(CompiledUntilMatcher),
    Nested { open: String, close: String },
    AutoClose(Box<AutoCloseSpec>),
    Seq(Vec<CompiledLexExpr>),
    Choice(Vec<CompiledLexExpr>),
    Repeat(Box<CompiledLexExpr>),
    Repeat1(Box<CompiledLexExpr>),
    UnsupportedSymbol(GrammarExprId),
}

pub(crate) type CompiledLexPattern = crate::lex_match::CompiledPattern;

pub(crate) type CompiledUntilMatcher = crate::lex_match::CompiledUntilMatcher;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AutoCloseSpec {
    pub(crate) tag: String,
    pub(crate) open: Option<String>,
    pub(crate) close: Option<String>,
    pub(crate) closed_by: Vec<String>,
    pub(crate) open_node: Option<String>,
    pub(crate) close_node: Option<String>,
    pub(crate) tag_name_node: Option<String>,
    pub(crate) start_prefix: Option<String>,
    pub(crate) end_prefix: Option<String>,
    pub(crate) closed_by_tags: Vec<String>,
    pub(crate) rules: Vec<AutoCloseRuleSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AutoCloseRuleSpec {
    pub(crate) tag: String,
    pub(crate) closed_by_tags: Vec<String>,
}

/// External scanner host used by Weavy parser execution.
pub trait ExternalScannerHost {
    /// Try to scan one external token for a branch-local parser state.
    fn scan(
        &self,
        request: ExternalScanRequest<'_>,
    ) -> Result<Option<ExternalScanResult>, ParserExecutionError>;
}

/// Result of one external scanner host call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExternalScanResult {
    end_byte: usize,
    before: Option<ScannerSnapshotId>,
    after: Option<ScannerSnapshotId>,
}

impl ExternalScanResult {
    /// Build a scanner result for a token ending at `end_byte`.
    pub const fn new(end_byte: usize) -> Self {
        Self {
            end_byte,
            before: None,
            after: None,
        }
    }

    /// Attach serialized scanner state snapshots observed around the call.
    pub const fn with_snapshots(
        mut self,
        before: Option<ScannerSnapshotId>,
        after: Option<ScannerSnapshotId>,
    ) -> Self {
        self.before = before;
        self.after = after;
        self
    }

    /// Accepted token end byte.
    pub const fn end_byte(&self) -> usize {
        self.end_byte
    }

    /// Scanner snapshot before the call.
    pub const fn before(&self) -> Option<ScannerSnapshotId> {
        self.before
    }

    /// Scanner snapshot after the call.
    pub const fn after(&self) -> Option<ScannerSnapshotId> {
        self.after
    }
}

/// Branch-local external scanner request.
#[derive(Debug, Clone, Copy)]
pub struct ExternalScanRequest<'a> {
    state: ParseStateId,
    external: ExternalId,
    external_symbol: &'a ExternalSymbol,
    valid_symbols: Option<&'a ValidSymbolSet>,
    input: &'a str,
    byte_position: usize,
    scanner_snapshot: Option<ScannerSnapshotId>,
}

impl ExternalScanRequest<'_> {
    pub(crate) const fn new<'a>(
        state: ParseStateId,
        external: ExternalId,
        external_symbol: &'a ExternalSymbol,
        valid_symbols: Option<&'a ValidSymbolSet>,
        input: &'a str,
        byte_position: usize,
        scanner_snapshot: Option<ScannerSnapshotId>,
    ) -> ExternalScanRequest<'a> {
        ExternalScanRequest {
            state,
            external,
            external_symbol,
            valid_symbols,
            input,
            byte_position,
            scanner_snapshot,
        }
    }

    /// Parse state requesting the scanner call.
    pub const fn state(&self) -> ParseStateId {
        self.state
    }

    /// External token requested by the lexical mode.
    pub const fn external(&self) -> ExternalId {
        self.external
    }

    /// External symbol metadata.
    pub const fn external_symbol(&self) -> &ExternalSymbol {
        self.external_symbol
    }

    /// Valid-symbol set for this parser state, if any.
    pub const fn valid_symbols(&self) -> Option<&ValidSymbolSet> {
        self.valid_symbols
    }

    /// Source input being parsed.
    pub const fn input(&self) -> &str {
        self.input
    }

    /// Branch-local byte position before scanner execution.
    pub const fn byte_position(&self) -> usize {
        self.byte_position
    }

    /// Branch-local scanner snapshot before scanner execution.
    pub const fn scanner_snapshot(&self) -> Option<ScannerSnapshotId> {
        self.scanner_snapshot
    }
}

#[cfg(test)]
const ASCII_IDENTIFIER_PATTERN: &str = "[A-Za-z_][A-Za-z0-9_]*";
#[cfg(test)]
const GINGEMBRE_IDENTIFIER_PATTERN: &str = crate::lex_match::GINGEMBRE_IDENTIFIER_PATTERN;

fn compile_pattern(pattern: &str, flags: Option<&str>) -> CompiledLexPattern {
    crate::lex_match::compile_pattern(pattern, flags)
}

fn compile_until_markers(markers: &[String]) -> CompiledUntilMatcher {
    crate::lex_match::compile_until_markers(markers)
}

pub(crate) fn compile_lex_modes(
    grammar: &ValidatedGrammar,
    parser: &ParserGrammar,
    table: &ParseTable,
) -> Vec<CompiledLexMode> {
    let mut terminal_cache = vec![None; parser.symbols.terminals.len()];
    table
        .lexical_modes()
        .iter()
        .map(|mode| {
            let terminals = mode
                .terminals()
                .iter()
                .map(|terminal| {
                    let terminal_index = terminal.get() as usize;
                    terminal_cache[terminal_index]
                        .get_or_insert_with(|| {
                            let terminal_row = &parser.symbols.terminals[terminal_index];
                            compile_lex_terminal(grammar, terminal_row)
                        })
                        .clone()
                })
                .collect::<Vec<_>>();
            CompiledLexMode {
                terminals,
                external_count: mode.externals().len(),
            }
        })
        .collect()
}

fn compile_lex_terminal(
    grammar: &ValidatedGrammar,
    terminal: &TerminalSymbol,
) -> CompiledLexTerminal {
    CompiledLexTerminal {
        terminal: terminal.id(),
        matcher: compile_terminal_matcher(grammar, terminal),
        immediate: terminal.kind() == ParserTerminalKind::ImmediateToken,
        literal: terminal.kind() == ParserTerminalKind::String,
        lexical_precedence: terminal_lexical_completion_precedence(grammar, terminal),
        implicit_precedence: terminal_lexical_implicit_precedence(grammar, terminal),
    }
}

fn compile_terminal_matcher(
    grammar: &ValidatedGrammar,
    terminal: &TerminalSymbol,
) -> CompiledTerminalMatcher {
    match terminal.kind() {
        ParserTerminalKind::String => {
            CompiledTerminalMatcher::Expr(CompiledLexExpr::String(terminal.spelling().to_owned()))
        }
        ParserTerminalKind::Pattern => CompiledTerminalMatcher::Expr(CompiledLexExpr::Pattern(
            compile_pattern(terminal.spelling(), terminal.flags()),
        )),
        ParserTerminalKind::AutoClose => match grammar.expr(terminal.source_expr()) {
            GrammarExpr::AutoClose(spec) => {
                CompiledTerminalMatcher::Expr(CompiledLexExpr::AutoClose(Box::new(AutoCloseSpec {
                    tag: spec.tag.clone(),
                    open: spec.open.clone(),
                    close: spec.close.clone(),
                    closed_by: spec.closed_by.clone(),
                    open_node: spec.open_node.clone(),
                    close_node: spec.close_node.clone(),
                    tag_name_node: spec.tag_name_node.clone(),
                    start_prefix: spec.start_prefix.clone(),
                    end_prefix: spec.end_prefix.clone(),
                    closed_by_tags: spec.closed_by_tags.clone(),
                    rules: compile_auto_close_rules(&spec.rules),
                })))
            }
            _ => CompiledTerminalMatcher::UnsupportedTerminal {
                terminal: terminal.id(),
            },
        },
        ParserTerminalKind::Token | ParserTerminalKind::ImmediateToken => {
            let Some(root) = terminal.lexical_root() else {
                return CompiledTerminalMatcher::UnsupportedTerminal {
                    terminal: terminal.id(),
                };
            };
            let (GrammarExpr::Token(content) | GrammarExpr::ImmediateToken(content)) =
                grammar.expr(root)
            else {
                return CompiledTerminalMatcher::UnsupportedTerminal {
                    terminal: terminal.id(),
                };
            };
            CompiledTerminalMatcher::Expr(compile_lex_expr(grammar, *content))
        }
    }
}

fn compile_lex_expr(grammar: &ValidatedGrammar, expr: GrammarExprId) -> CompiledLexExpr {
    compile_lex_expr_inner(grammar, expr, &mut Vec::new())
}

fn compile_auto_close_rules(rules: &[ValidatedAutoCloseRule]) -> Vec<AutoCloseRuleSpec> {
    rules
        .iter()
        .map(|rule| AutoCloseRuleSpec {
            tag: normalize_auto_close_tag(rule.tag()),
            closed_by_tags: rule
                .closed_by_tags()
                .iter()
                .map(|tag| normalize_auto_close_tag(tag))
                .collect(),
        })
        .collect()
}

fn normalize_auto_close_tag(tag: &str) -> String {
    tag.to_ascii_lowercase()
}

fn compile_lex_expr_inner(
    grammar: &ValidatedGrammar,
    expr: GrammarExprId,
    rule_stack: &mut Vec<RuleId>,
) -> CompiledLexExpr {
    match grammar.expr(expr) {
        GrammarExpr::Blank => CompiledLexExpr::Blank,
        GrammarExpr::StringToken(value) => CompiledLexExpr::String(value.clone()),
        GrammarExpr::PatternToken { value, flags } => {
            CompiledLexExpr::Pattern(compile_pattern(value, flags.as_deref()))
        }
        GrammarExpr::Until { markers } => CompiledLexExpr::Until(compile_until_markers(markers)),
        GrammarExpr::Nested { open, close } => CompiledLexExpr::Nested {
            open: open.clone(),
            close: close.clone(),
        },
        GrammarExpr::AutoClose(spec) => CompiledLexExpr::AutoClose(Box::new(AutoCloseSpec {
            tag: spec.tag.clone(),
            open: spec.open.clone(),
            close: spec.close.clone(),
            closed_by: spec.closed_by.clone(),
            open_node: spec.open_node.clone(),
            close_node: spec.close_node.clone(),
            tag_name_node: spec.tag_name_node.clone(),
            start_prefix: spec.start_prefix.clone(),
            end_prefix: spec.end_prefix.clone(),
            closed_by_tags: spec.closed_by_tags.clone(),
            rules: compile_auto_close_rules(&spec.rules),
        })),
        GrammarExpr::Token(content)
        | GrammarExpr::ImmediateToken(content)
        | GrammarExpr::Field { content, .. }
        | GrammarExpr::Prec { content, .. }
        | GrammarExpr::PrecDynamic { content, .. }
        | GrammarExpr::Alias { content, .. }
        | GrammarExpr::Reserved { content, .. } => {
            compile_lex_expr_inner(grammar, *content, rule_stack)
        }
        GrammarExpr::Choice(members) => CompiledLexExpr::Choice(
            members
                .iter()
                .map(|member| compile_lex_expr_inner(grammar, *member, rule_stack))
                .collect(),
        ),
        GrammarExpr::Seq(members) => CompiledLexExpr::Seq(
            members
                .iter()
                .map(|member| compile_lex_expr_inner(grammar, *member, rule_stack))
                .collect(),
        ),
        GrammarExpr::Repeat(content) => CompiledLexExpr::Repeat(Box::new(compile_lex_expr_inner(
            grammar, *content, rule_stack,
        ))),
        GrammarExpr::Repeat1(content) => CompiledLexExpr::Repeat1(Box::new(
            compile_lex_expr_inner(grammar, *content, rule_stack),
        )),
        GrammarExpr::Symbol(SymbolRef::Rule(rule)) => {
            if rule_stack.contains(rule) {
                return CompiledLexExpr::UnsupportedSymbol(expr);
            }
            rule_stack.push(*rule);
            let compiled = compile_lex_expr_inner(grammar, grammar.rule(*rule).expr(), rule_stack);
            rule_stack.pop();
            compiled
        }
        GrammarExpr::Symbol(SymbolRef::External(_)) => CompiledLexExpr::UnsupportedSymbol(expr),
    }
}

pub(crate) fn terminal_lexical_completion_precedence(
    grammar: &ValidatedGrammar,
    terminal: &TerminalSymbol,
) -> i32 {
    terminal
        .lexical_root()
        .and_then(|root| lexical_expr_completion_precedence(grammar, root, &mut Vec::new()))
        .unwrap_or(0)
}

fn lexical_expr_completion_precedence(
    grammar: &ValidatedGrammar,
    expr: GrammarExprId,
    rule_stack: &mut Vec<RuleId>,
) -> Option<i32> {
    match grammar.expr(expr) {
        GrammarExpr::Prec {
            value: StaticPrecedenceValue::Integer(value),
            ..
        } => Some(*value),
        GrammarExpr::Prec {
            value: StaticPrecedenceValue::Name(_),
            content,
            ..
        } => lexical_expr_completion_precedence(grammar, *content, rule_stack),
        GrammarExpr::Token(content)
        | GrammarExpr::ImmediateToken(content)
        | GrammarExpr::Field { content, .. }
        | GrammarExpr::PrecDynamic { content, .. }
        | GrammarExpr::Alias { content, .. }
        | GrammarExpr::Reserved { content, .. } => {
            lexical_expr_completion_precedence(grammar, *content, rule_stack)
        }
        GrammarExpr::Choice(members) | GrammarExpr::Seq(members) => members
            .iter()
            .filter_map(|member| lexical_expr_completion_precedence(grammar, *member, rule_stack))
            .max(),
        GrammarExpr::Repeat(content) | GrammarExpr::Repeat1(content) => {
            lexical_expr_completion_precedence(grammar, *content, rule_stack)
        }
        GrammarExpr::Symbol(SymbolRef::Rule(rule)) => {
            if rule_stack.contains(rule) {
                return None;
            }
            rule_stack.push(*rule);
            let precedence =
                lexical_expr_completion_precedence(grammar, grammar.rule(*rule).expr(), rule_stack);
            rule_stack.pop();
            precedence
        }
        GrammarExpr::Blank
        | GrammarExpr::StringToken(_)
        | GrammarExpr::PatternToken { .. }
        | GrammarExpr::Until { .. }
        | GrammarExpr::Nested { .. }
        | GrammarExpr::AutoClose(_)
        | GrammarExpr::Symbol(SymbolRef::External(_)) => None,
    }
}

pub(crate) fn terminal_lexical_implicit_precedence(
    grammar: &ValidatedGrammar,
    terminal: &TerminalSymbol,
) -> i32 {
    match terminal.kind() {
        ParserTerminalKind::String => 2,
        ParserTerminalKind::Pattern | ParserTerminalKind::AutoClose => 0,
        ParserTerminalKind::Token | ParserTerminalKind::ImmediateToken => terminal
            .lexical_root()
            .map(|root| lexical_expr_implicit_precedence(grammar, root, &mut Vec::new()))
            .unwrap_or(0),
    }
}

fn lexical_expr_implicit_precedence(
    grammar: &ValidatedGrammar,
    expr: GrammarExprId,
    rule_stack: &mut Vec<RuleId>,
) -> i32 {
    match grammar.expr(expr) {
        GrammarExpr::StringToken(_) => 2,
        GrammarExpr::PatternToken { .. }
        | GrammarExpr::Until { .. }
        | GrammarExpr::Nested { .. }
        | GrammarExpr::AutoClose(_) => 0,
        GrammarExpr::ImmediateToken(content) => {
            lexical_expr_implicit_precedence(grammar, *content, rule_stack) + 1
        }
        GrammarExpr::Token(content)
        | GrammarExpr::Field { content, .. }
        | GrammarExpr::Prec { content, .. }
        | GrammarExpr::PrecDynamic { content, .. }
        | GrammarExpr::Alias { content, .. }
        | GrammarExpr::Reserved { content, .. } => {
            lexical_expr_implicit_precedence(grammar, *content, rule_stack)
        }
        GrammarExpr::Symbol(SymbolRef::Rule(rule)) => {
            if rule_stack.contains(rule) {
                return 0;
            }
            rule_stack.push(*rule);
            let precedence =
                lexical_expr_implicit_precedence(grammar, grammar.rule(*rule).expr(), rule_stack);
            rule_stack.pop();
            precedence
        }
        GrammarExpr::Blank
        | GrammarExpr::Symbol(SymbolRef::External(_))
        | GrammarExpr::Choice(_)
        | GrammarExpr::Seq(_)
        | GrammarExpr::Repeat(_)
        | GrammarExpr::Repeat1(_) => 0,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LexMatch {
    pub(crate) end: usize,
    pub(crate) inspected_end: usize,
}

impl LexMatch {
    pub(crate) const fn new(end: usize, inspected_end: usize) -> Self {
        Self { end, inspected_end }
    }
}

/// Byte edit shape used by Tree-sitter-style incremental reparsing.
///
/// `start_byte..old_end_byte` names the replaced range in the previous input.
/// `start_byte..new_end_byte` names the replacement range in the new input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParserInputEdit {
    start_byte: usize,
    old_end_byte: usize,
    new_end_byte: usize,
}

impl ParserInputEdit {
    /// Build a byte edit descriptor.
    pub const fn new(start_byte: usize, old_end_byte: usize, new_end_byte: usize) -> Self {
        Self {
            start_byte,
            old_end_byte,
            new_end_byte,
        }
    }

    /// Start byte shared by the old and new input ranges.
    pub const fn start_byte(self) -> usize {
        self.start_byte
    }

    /// End byte in the old input.
    pub const fn old_end_byte(self) -> usize {
        self.old_end_byte
    }

    /// End byte in the new input.
    pub const fn new_end_byte(self) -> usize {
        self.new_end_byte
    }

    /// Validate that this edit describes the old and new inputs.
    pub fn validate_against(
        &self,
        old_input: &str,
        new_input: &str,
    ) -> Result<(), ParserExecutionError> {
        let valid_order =
            self.start_byte <= self.old_end_byte && self.start_byte <= self.new_end_byte;
        let valid_bounds =
            self.old_end_byte <= old_input.len() && self.new_end_byte <= new_input.len();
        let valid_boundaries = old_input.is_char_boundary(self.start_byte)
            && old_input.is_char_boundary(self.old_end_byte)
            && new_input.is_char_boundary(self.start_byte)
            && new_input.is_char_boundary(self.new_end_byte);
        let valid_context = valid_order
            && valid_bounds
            && valid_boundaries
            && old_input[..self.start_byte] == new_input[..self.start_byte]
            && old_input[self.old_end_byte..] == new_input[self.new_end_byte..];
        if valid_context {
            Ok(())
        } else {
            Err(ParserExecutionError::new(
                ParserExecutionErrorKind::InvalidInputEdit {
                    start_byte: self.start_byte,
                    old_end_byte: self.old_end_byte,
                    new_end_byte: self.new_end_byte,
                    old_input_len: old_input.len(),
                    new_input_len: new_input.len(),
                },
            ))
        }
    }
}

/// Lossless-enough parse tree projection for CST/AST consumers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedCstNode {
    kind: Arc<str>,
    symbol: Option<ParserSymbol>,
    field: Option<Arc<str>>,
    node: Option<TreeNodeId>,
    bytes: ByteRange,
    points: PointRange,
    named: bool,
    visible: bool,
    extra: bool,
    text: Option<ResolvedCstText>,
    children: Vec<Self>,
}

#[derive(Clone)]
struct ResolvedCstText {
    source: Arc<str>,
    bytes: ByteRange,
}

impl ResolvedCstText {
    fn as_str(&self) -> Option<&str> {
        source_slice(&self.source, self.bytes)
    }
}

impl fmt::Debug for ResolvedCstText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.as_str() {
            Some(text) => f.debug_tuple("ResolvedCstText").field(&text).finish(),
            None => f.debug_tuple("ResolvedCstText").field(&self.bytes).finish(),
        }
    }
}

impl PartialEq for ResolvedCstText {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl Eq for ResolvedCstText {}

impl ResolvedCstNode {
    /// Node or terminal kind.
    pub fn kind(&self) -> &str {
        &self.kind
    }

    /// Parser symbol for terminal leaves, if this node came from a shifted token.
    pub const fn symbol(&self) -> Option<ParserSymbol> {
        self.symbol
    }

    /// Field name attached by the grammar, when known.
    pub fn field(&self) -> Option<&str> {
        self.field.as_deref()
    }

    /// Parse tree node id, when this item materialized a tree node.
    pub const fn node(&self) -> Option<TreeNodeId> {
        self.node
    }

    /// Source byte range.
    pub const fn bytes(&self) -> ByteRange {
        self.bytes
    }

    /// Source point range.
    pub const fn points(&self) -> PointRange {
        self.points
    }

    /// Whether this item is named in public traversal.
    pub const fn named(&self) -> bool {
        self.named
    }

    /// Whether this item is visible in public traversal.
    pub const fn visible(&self) -> bool {
        self.visible
    }

    /// Whether this item came from an extra token/node.
    pub const fn extra(&self) -> bool {
        self.extra
    }

    /// Source text for terminal leaves.
    pub fn text(&self) -> Option<&str> {
        self.text.as_ref().and_then(ResolvedCstText::as_str)
    }

    /// Child items, including anonymous terminals.
    pub fn children(&self) -> &[Self] {
        &self.children
    }
}

/// Minimal recursive node view needed by typed AST/lowering materializers.
pub trait ParseNode: Sized {
    /// Node or terminal kind.
    fn kind(&self) -> &str;

    /// Whether this item is named in public traversal.
    fn named(&self) -> bool;

    /// Source text for terminal leaves.
    fn text(&self) -> Option<&str>;

    /// Ordered child items.
    fn children(&self) -> &[Self];

    /// Half-open source byte range `[start, end)`.
    fn byte_range(&self) -> (usize, usize);
}

impl ParseNode for ResolvedCstNode {
    fn kind(&self) -> &str {
        self.kind()
    }

    fn named(&self) -> bool {
        self.named()
    }

    fn text(&self) -> Option<&str> {
        self.text()
    }

    fn children(&self) -> &[Self] {
        self.children()
    }

    fn byte_range(&self) -> (usize, usize) {
        let bytes = self.bytes();
        (bytes.start().get() as usize, bytes.end().get() as usize)
    }
}

/// Arena-backed resolved CST with anonymous terminals and source ranges preserved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedCstTree {
    roots: Vec<usize>,
    items: Vec<ResolvedCstItem>,
}

/// Borrowed handle into a [`ResolvedCstTree`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResolvedCstTreeNode<'a> {
    tree: &'a ResolvedCstTree,
    index: usize,
}

impl ResolvedCstTree {
    /// Number of materialized items in this tree.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether this tree has no items.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Number of root items.
    pub fn root_count(&self) -> usize {
        self.roots.len()
    }

    /// Single root item, when this tree has exactly one root.
    pub fn root(&self) -> Option<ResolvedCstTreeNode<'_>> {
        let [index] = self.roots.as_slice() else {
            return None;
        };
        Some(ResolvedCstTreeNode {
            tree: self,
            index: *index,
        })
    }

    /// Root items in source order.
    pub fn roots(&self) -> impl ExactSizeIterator<Item = ResolvedCstTreeNode<'_>> + '_ {
        self.roots
            .iter()
            .copied()
            .map(|index| ResolvedCstTreeNode { tree: self, index })
    }

    /// Kind for the public root projection.
    pub fn root_kind(&self) -> Option<&str> {
        match self.roots.as_slice() {
            [] => None,
            [index] => Some(self.items[*index].kind.as_ref()),
            _ => Some("ROOT"),
        }
    }

    /// Materialize this arena tree into the owned recursive compatibility shape.
    pub fn to_owned_node(&self) -> Option<ResolvedCstNode> {
        if self.roots.is_empty() {
            return None;
        }
        if self.roots.len() == 1 {
            return Some(build_resolved_node(self.roots[0], &self.items));
        }

        let first = self.roots[0];
        let last = self.roots[self.roots.len() - 1];
        let bytes = ByteRange::new(
            self.items[first].bytes.start(),
            self.items[last].bytes.end(),
        )
        .ok()?;
        let points = PointRange::new(
            self.items[first].points.start(),
            self.items[last].points.end(),
        )
        .ok()?;
        Some(ResolvedCstNode {
            kind: "ROOT".into(),
            symbol: None,
            field: None,
            node: None,
            bytes,
            points,
            named: true,
            visible: true,
            extra: false,
            text: None,
            children: self
                .roots
                .iter()
                .copied()
                .map(|root| build_resolved_node(root, &self.items))
                .collect(),
        })
    }
}

impl<'a> ResolvedCstTreeNode<'a> {
    fn item(&self) -> &'a ResolvedCstItem {
        &self.tree.items[self.index]
    }

    /// Node or terminal kind.
    pub fn kind(&self) -> &'a str {
        self.item().kind.as_ref()
    }

    /// Parser symbol for terminal leaves, if this node came from a shifted token.
    pub fn symbol(&self) -> Option<ParserSymbol> {
        self.item().symbol
    }

    /// Field name attached by the grammar, when known.
    pub fn field(&self) -> Option<&'a str> {
        self.item().field.as_deref()
    }

    /// Parse tree node id, when this item materialized a tree node.
    pub fn node(&self) -> Option<TreeNodeId> {
        self.item().node
    }

    /// Source byte range.
    pub fn bytes(&self) -> ByteRange {
        self.item().bytes
    }

    /// Source point range.
    pub fn points(&self) -> PointRange {
        self.item().points
    }

    /// Whether this item is named in public traversal.
    pub fn named(&self) -> bool {
        self.item().named
    }

    /// Whether this item is visible in public traversal.
    pub fn visible(&self) -> bool {
        self.item().visible
    }

    /// Whether this item came from an extra token/node.
    pub fn extra(&self) -> bool {
        self.item().extra
    }

    /// Source text for terminal leaves.
    pub fn text(&self) -> Option<&'a str> {
        self.item().text.as_ref().and_then(ResolvedCstText::as_str)
    }

    /// Child items in source order.
    pub fn children(&self) -> impl ExactSizeIterator<Item = ResolvedCstTreeNode<'a>> + '_ {
        self.item()
            .children
            .iter()
            .copied()
            .map(|index| ResolvedCstTreeNode {
                tree: self.tree,
                index,
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvedCstItem {
    kind: Arc<str>,
    symbol: Option<ParserSymbol>,
    field: Option<Arc<str>>,
    node: Option<TreeNodeId>,
    bytes: ByteRange,
    points: PointRange,
    named: bool,
    visible: bool,
    extra: bool,
    text: Option<ResolvedCstText>,
    order: usize,
    children: Vec<usize>,
}

pub(crate) struct ResolvedCstBuilder<'a> {
    parser: &'a ParserGrammar,
    input: &'a str,
    source: Arc<str>,
    names: ResolvedCstNames,
    field_by_child: Vec<Option<Arc<str>>>,
    item_indices_by_node: Vec<SmallVec<[usize; 1]>>,
    items: Vec<ResolvedCstItem>,
}

struct ResolvedCstNames {
    fields: Vec<Arc<str>>,
    public_nodes: Vec<Arc<str>>,
    aliases: Vec<Arc<str>>,
    terminals: Vec<Arc<str>>,
    nonterminals: Vec<Arc<str>>,
    externals: Vec<Option<Arc<str>>>,
    eof: Arc<str>,
    error: Arc<str>,
    missing: Arc<str>,
    recovery: Arc<str>,
}

impl ResolvedCstNames {
    fn from_parser(parser: &ParserGrammar) -> Self {
        Self {
            fields: parser
                .fields
                .iter()
                .map(|field| Arc::<str>::from(field.name()))
                .collect(),
            public_nodes: parser
                .public_node_kinds
                .iter()
                .map(|kind| Arc::<str>::from(kind.name()))
                .collect(),
            aliases: parser
                .aliases
                .iter()
                .map(|alias| Arc::<str>::from(alias.value()))
                .collect(),
            terminals: parser
                .symbols
                .terminals
                .iter()
                .map(|terminal| {
                    let name = match terminal.kind() {
                        ParserTerminalKind::String | ParserTerminalKind::AutoClose => {
                            terminal.spelling()
                        }
                        ParserTerminalKind::Pattern
                        | ParserTerminalKind::Token
                        | ParserTerminalKind::ImmediateToken => terminal
                            .public_names()
                            .first()
                            .map(String::as_str)
                            .unwrap_or_else(|| terminal.spelling()),
                    };
                    Arc::<str>::from(name)
                })
                .collect(),
            nonterminals: parser
                .symbols
                .nonterminals
                .iter()
                .map(|nonterminal| Arc::<str>::from(nonterminal.name()))
                .collect(),
            externals: parser
                .symbols
                .externals
                .iter()
                .map(|external| external.name().map(Arc::<str>::from))
                .collect(),
            eof: Arc::<str>::from("EOF"),
            error: Arc::<str>::from("ERROR"),
            missing: Arc::<str>::from("MISSING"),
            recovery: Arc::<str>::from("RECOVERY"),
        }
    }
}

impl<'a> ResolvedCstBuilder<'a> {
    pub(crate) fn with_capacity(
        parser: &'a ParserGrammar,
        input: &'a str,
        capacity: usize,
    ) -> Self {
        Self::with_node_capacity(parser, input, capacity, 0)
    }

    pub(crate) fn with_node_capacity(
        parser: &'a ParserGrammar,
        input: &'a str,
        capacity: usize,
        node_capacity: usize,
    ) -> Self {
        Self {
            parser,
            input,
            source: Arc::<str>::from(input),
            names: ResolvedCstNames::from_parser(parser),
            field_by_child: vec![None; node_capacity],
            item_indices_by_node: Vec::with_capacity(node_capacity),
            items: Vec::with_capacity(capacity),
        }
    }

    pub(crate) fn push(&mut self, event: &TreeEvent) {
        let order = self.items.len();
        match event {
            TreeEvent::Field {
                child: Some(child),
                field,
                ..
            } => {
                if let Some(name) = self.field_name(*field) {
                    self.attach_field(*child, name);
                }
            }
            TreeEvent::Token {
                symbol,
                bytes,
                points,
                extra,
                named,
                ..
            } => {
                self.push_item(ResolvedCstItem {
                    kind: self.symbol_kind(*symbol, source_slice(self.input, *bytes)),
                    symbol: Some(*symbol),
                    field: None,
                    node: None,
                    bytes: *bytes,
                    points: *points,
                    named: *named,
                    visible: true,
                    extra: *extra,
                    text: Some(ResolvedCstText {
                        source: Arc::clone(&self.source),
                        bytes: *bytes,
                    }),
                    order,
                    children: Vec::new(),
                });
            }
            TreeEvent::Reduce {
                node,
                metadata,
                bytes,
                points,
                ..
            } => {
                if let Some(public_node) =
                    self.parser.production_metadata[metadata.get() as usize].public_node()
                {
                    self.push_item(ResolvedCstItem {
                        kind: self.public_node_kind(public_node),
                        symbol: None,
                        field: self.field_for_node(*node),
                        node: Some(*node),
                        bytes: *bytes,
                        points: *points,
                        named: true,
                        visible: true,
                        extra: false,
                        text: None,
                        order,
                        children: Vec::new(),
                    });
                }
            }
            TreeEvent::CloseNode {
                node,
                public_node: Some(public_node),
                bytes,
                points,
                ..
            } => {
                self.push_item(ResolvedCstItem {
                    kind: self.public_node_kind(*public_node),
                    symbol: None,
                    field: self.field_for_node(*node),
                    node: Some(*node),
                    bytes: *bytes,
                    points: *points,
                    named: true,
                    visible: true,
                    extra: true,
                    text: None,
                    order,
                    children: Vec::new(),
                });
            }
            TreeEvent::Error {
                node,
                bytes,
                points,
                ..
            } => {
                self.push_item(ResolvedCstItem {
                    kind: Arc::clone(&self.names.error),
                    symbol: None,
                    field: self.field_for_node(*node),
                    node: Some(*node),
                    bytes: *bytes,
                    points: *points,
                    named: true,
                    visible: true,
                    extra: false,
                    text: None,
                    order,
                    children: Vec::new(),
                });
            }
            TreeEvent::Missing {
                symbol,
                bytes,
                points,
                ..
            } => {
                self.push_item(ResolvedCstItem {
                    kind: self.symbol_kind(*symbol, None),
                    symbol: Some(*symbol),
                    field: None,
                    node: None,
                    bytes: *bytes,
                    points: *points,
                    named: false,
                    visible: true,
                    extra: false,
                    text: None,
                    order,
                    children: Vec::new(),
                });
            }
            TreeEvent::Alias {
                node,
                alias,
                named,
                bytes,
                points,
                ..
            } => {
                self.push_item(ResolvedCstItem {
                    kind: self.alias_kind(*alias),
                    symbol: None,
                    field: self.field_for_node(*node),
                    node: Some(*node),
                    bytes: *bytes,
                    points: *points,
                    named: *named,
                    visible: true,
                    extra: false,
                    text: None,
                    order,
                    children: Vec::new(),
                });
            }
            TreeEvent::OpenNode { .. }
            | TreeEvent::Field { child: None, .. }
            | TreeEvent::CloseNode {
                public_node: None, ..
            }
            | TreeEvent::ReuseNode { .. } => {}
        }
    }

    pub(crate) fn finish(mut self) -> Option<ResolvedCstNode> {
        if self.items.is_empty() {
            return None;
        }

        let mut roots = attach_resolved_children_from_ranges(&mut self.items);
        if roots.is_empty() {
            return None;
        }
        sort_resolved_children(&mut roots, &self.items);
        if roots.len() == 1 {
            return Some(build_resolved_node(roots[0], &self.items));
        }

        let first = roots[0];
        let last = roots[roots.len() - 1];
        let bytes = ByteRange::new(
            self.items[first].bytes.start(),
            self.items[last].bytes.end(),
        )
        .ok()?;
        let points = PointRange::new(
            self.items[first].points.start(),
            self.items[last].points.end(),
        )
        .ok()?;
        Some(ResolvedCstNode {
            kind: "ROOT".into(),
            symbol: None,
            field: None,
            node: None,
            bytes,
            points,
            named: true,
            visible: true,
            extra: false,
            text: None,
            children: roots
                .into_iter()
                .map(|root| build_resolved_node(root, &self.items))
                .collect(),
        })
    }

    pub(crate) fn finish_tree(mut self) -> Option<ResolvedCstTree> {
        if self.items.is_empty() {
            return None;
        }

        let mut roots = attach_resolved_children_from_ranges(&mut self.items);
        if roots.is_empty() {
            return None;
        }
        sort_resolved_children(&mut roots, &self.items);
        for index in 0..self.items.len() {
            let mut children = std::mem::take(&mut self.items[index].children);
            sort_resolved_children(&mut children, &self.items);
            self.items[index].children = children;
        }
        Some(ResolvedCstTree {
            roots,
            items: self.items,
        })
    }

    fn attach_field(&mut self, node: TreeNodeId, name: Arc<str>) {
        let slot = self.ensure_node_slot(node);
        self.field_by_child[slot] = Some(name.clone());
        for index in &self.item_indices_by_node[slot] {
            self.items[*index].field = Some(name.clone());
        }
    }

    fn push_item(&mut self, item: ResolvedCstItem) {
        let node = item.node;
        let index = self.items.len();
        self.items.push(item);
        if let Some(node) = node {
            let slot = self.ensure_node_slot(node);
            self.item_indices_by_node[slot].push(index);
        }
    }

    fn ensure_node_slot(&mut self, node: TreeNodeId) -> usize {
        let slot = node.get() as usize;
        if slot >= self.field_by_child.len() {
            self.field_by_child.resize_with(slot + 1, || None);
        }
        if slot >= self.item_indices_by_node.len() {
            self.item_indices_by_node
                .resize_with(slot + 1, SmallVec::new);
        }
        slot
    }

    fn field_for_node(&self, node: TreeNodeId) -> Option<Arc<str>> {
        self.field_by_child
            .get(node.get() as usize)
            .and_then(Clone::clone)
    }

    fn field_name(&self, field: FieldId) -> Option<Arc<str>> {
        self.names.fields.get(field.get() as usize).cloned()
    }

    fn public_node_kind(&self, public_node: PublicNodeKindId) -> Arc<str> {
        self.names
            .public_nodes
            .get(public_node.get() as usize)
            .cloned()
            .unwrap_or_else(|| {
                Arc::<str>::from(self.parser.public_node_kinds[public_node.get() as usize].name())
            })
    }

    fn alias_kind(&self, alias: AliasId) -> Arc<str> {
        self.names
            .aliases
            .get(alias.get() as usize)
            .cloned()
            .unwrap_or_else(|| Arc::<str>::from(self.parser.aliases[alias.get() as usize].value()))
    }

    fn symbol_kind(&self, symbol: ParserSymbol, token_text: Option<&str>) -> Arc<str> {
        match symbol {
            ParserSymbol::Terminal(terminal) => {
                self.names.terminals[terminal.get() as usize].clone()
            }
            ParserSymbol::Nonterminal(nonterminal) => {
                self.names.nonterminals[nonterminal.get() as usize].clone()
            }
            ParserSymbol::External(external) => self
                .names
                .externals
                .get(external.get() as usize)
                .and_then(Clone::clone)
                .unwrap_or_else(|| Arc::<str>::from(token_text.unwrap_or("<external>"))),
            ParserSymbol::Eof => Arc::clone(&self.names.eof),
            ParserSymbol::Internal(internal) => {
                match self.parser.symbols.internal[internal.get() as usize].kind() {
                    InternalSymbolKind::Error => Arc::clone(&self.names.error),
                    InternalSymbolKind::Missing => Arc::clone(&self.names.missing),
                    InternalSymbolKind::Recovery => Arc::clone(&self.names.recovery),
                }
            }
        }
    }
}

fn resolved_item_contains(parent: &ResolvedCstItem, child: &ResolvedCstItem) -> bool {
    parent.order > child.order
        && parent.bytes.start() <= child.bytes.start()
        && child.bytes.end() <= parent.bytes.end()
}

fn attach_resolved_children_from_ranges(items: &mut [ResolvedCstItem]) -> Vec<usize> {
    let parents = resolved_parent_slots_by_range_sort(items);
    let mut child_counts = vec![0usize; items.len()];
    let mut root_count = 0usize;
    for parent in &parents {
        if let Some(parent) = parent {
            child_counts[*parent] += 1;
        } else {
            root_count += 1;
        }
    }

    for item in items.iter_mut() {
        item.children.clear();
    }
    for (item, child_count) in items.iter_mut().zip(child_counts) {
        item.children.reserve(child_count);
    }

    let mut roots = Vec::<usize>::with_capacity(root_count);
    for (child, parent) in parents.into_iter().enumerate() {
        if let Some(parent) = parent {
            items[parent].children.push(child);
        } else {
            roots.push(child);
        }
    }
    roots
}

fn resolved_parent_slots_by_range_sort(items: &[ResolvedCstItem]) -> Vec<Option<usize>> {
    let mut indices = (0..items.len()).collect::<Vec<_>>();
    indices.sort_unstable_by_key(|index| {
        let item = &items[*index];
        (
            item.bytes.start().get(),
            Reverse(item.bytes.end().get()),
            Reverse(item.order),
            Reverse(*index),
        )
    });

    let mut parents = vec![None; items.len()];
    let mut ancestors = Vec::<usize>::new();
    for child in indices {
        while let Some(parent) = ancestors.last().copied() {
            if resolved_item_contains(&items[parent], &items[child]) {
                break;
            }
            ancestors.pop();
        }
        parents[child] = ancestors.last().copied();
        if items[child].node.is_some() {
            ancestors.push(child);
        }
    }

    #[cfg(debug_assertions)]
    if items.len() <= 4096 {
        debug_assert_eq!(parents, resolved_parent_slots_quadratic(items));
    }

    parents
}

#[cfg(debug_assertions)]
fn resolved_parent_slots_quadratic(items: &[ResolvedCstItem]) -> Vec<Option<usize>> {
    let mut parents = vec![None; items.len()];
    for (child, parent_slot) in parents.iter_mut().enumerate() {
        let mut best = None::<(usize, usize, usize)>;
        for parent in 0..items.len() {
            if parent == child
                || items[parent].node.is_none()
                || !resolved_item_contains(&items[parent], &items[child])
            {
                continue;
            }
            let key = (
                resolved_item_len(&items[parent]),
                items[parent].order,
                parent,
            );
            if best.is_none_or(|best| key < best) {
                best = Some(key);
            }
        }
        *parent_slot = best.map(|(_, _, parent)| parent);
    }
    parents
}

#[cfg(debug_assertions)]
fn resolved_item_len(item: &ResolvedCstItem) -> usize {
    item.bytes.end().get() as usize - item.bytes.start().get() as usize
}

fn build_resolved_node(index: usize, items: &[ResolvedCstItem]) -> ResolvedCstNode {
    let item = &items[index];
    let mut children = item.children.clone();
    sort_resolved_children(&mut children, items);
    ResolvedCstNode {
        kind: item.kind.clone(),
        symbol: item.symbol,
        field: item.field.clone(),
        node: item.node,
        bytes: item.bytes,
        points: item.points,
        named: item.named,
        visible: item.visible,
        extra: item.extra,
        text: item.text.clone(),
        children: children
            .into_iter()
            .map(|child| build_resolved_node(child, items))
            .collect(),
    }
}

fn sort_resolved_children(children: &mut [usize], items: &[ResolvedCstItem]) {
    if children.len() < 2 {
        return;
    }
    children.sort_unstable_by_key(|child| {
        let item = &items[*child];
        (item.bytes.start().get(), item.bytes.end().get(), item.order)
    });
}

fn source_slice(input: &str, bytes: ByteRange) -> Option<&str> {
    let start = bytes.start().get() as usize;
    let end = bytes.end().get() as usize;
    input.get(start..end)
}

pub(crate) fn visit_tree_events_for_version_lineage(
    accepted_version: StackVersionId,
    trace_events: &[TraceEvent],
    tree_events: &[TreeEvent],
    mut visit: impl FnMut(&TreeEvent),
) {
    if trace_events.is_empty() {
        for event in tree_events {
            visit(event);
        }
        return;
    }
    let lineage = stack_version_lineage(accepted_version, trace_events);
    for event in tree_events
        .iter()
        .filter(|event| lineage.contains(&event.version()))
    {
        visit(event);
    }
}

fn stack_version_lineage(
    accepted_version: StackVersionId,
    trace_events: &[TraceEvent],
) -> BTreeSet<StackVersionId> {
    let mut lineage = BTreeSet::new();
    let mut stack = vec![accepted_version];
    while let Some(version) = stack.pop() {
        if !lineage.insert(version) {
            continue;
        }
        if let Some(source) = trace_events.iter().rev().find_map(|event| match event {
            TraceEvent::GlrSplit {
                source, branches, ..
            } if branches.contains(&version) => Some(*source),
            _ => None,
        }) {
            stack.push(source);
        }
    }
    lineage
}

/// Error produced by parser input/scanner support.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParserExecutionError {
    kind: ParserExecutionErrorKind,
}

impl ParserExecutionError {
    fn new(kind: ParserExecutionErrorKind) -> Self {
        Self { kind }
    }

    /// Error kind.
    pub const fn kind(&self) -> &ParserExecutionErrorKind {
        &self.kind
    }
}

impl fmt::Display for ParserExecutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            ParserExecutionErrorKind::InvalidInputEdit {
                start_byte,
                old_end_byte,
                new_end_byte,
                old_input_len,
                new_input_len,
            } => write!(
                f,
                "invalid input edit start={start_byte} old_end={old_end_byte} new_end={new_end_byte} for old input length {old_input_len} and new input length {new_input_len}"
            ),
        }
    }
}

impl Error for ParserExecutionError {}

/// Parser input/scanner support error kind.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ParserExecutionErrorKind {
    /// Incremental edit coordinates did not describe the old and new inputs.
    InvalidInputEdit {
        /// Shared edit start byte.
        start_byte: usize,
        /// Replaced range end in the old input.
        old_end_byte: usize,
        /// Replacement range end in the new input.
        new_end_byte: usize,
        /// Old input byte length.
        old_input_len: usize,
        /// New input byte length.
        new_input_len: usize,
    },
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
    /// Accept the input after completing the root production.
    Accept {
        /// Root production to finalize.
        production: ProductionId,
        /// Metadata row attached to the root production.
        metadata: ProductionMetadataId,
        /// Root nonterminal.
        symbol: NonterminalId,
        /// Structural child count.
        child_count: usize,
        /// Dynamic precedence attached to the root subtree.
        dynamic_precedence: i32,
    },
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
}

/// GLR table facts that are not specific to one stack version.
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
    progress: u32,
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

    /// Branch progress count used as a stable branch-ranking tiebreaker.
    pub const fn progress(&self) -> u32 {
        self.progress
    }

    /// Whether this branch remains active.
    pub const fn active(&self) -> bool {
        self.active
    }
}

/// Tree operation emitted by parser actions.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum TreeEvent {
    /// A node was opened.
    OpenNode {
        /// Stack version that emitted this tree event.
        version: StackVersionId,
        /// Parser tree node.
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
        /// Stack version that emitted this tree event.
        version: StackVersionId,
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
        /// Stack version that emitted this tree event.
        version: StackVersionId,
        /// Reduced production.
        production: ProductionId,
        /// Reduced production metadata.
        metadata: ProductionMetadataId,
        /// Parser tree node.
        node: TreeNodeId,
        /// Byte range.
        bytes: ByteRange,
        /// Point range.
        points: PointRange,
    },
    /// A missing token was inserted by recovery.
    Missing {
        /// Stack version that emitted this tree event.
        version: StackVersionId,
        /// Missing symbol.
        symbol: ParserSymbol,
        /// Byte range.
        bytes: ByteRange,
        /// Point range.
        points: PointRange,
    },
    /// An error node was emitted.
    Error {
        /// Stack version that emitted this tree event.
        version: StackVersionId,
        /// Parser tree node.
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
        /// Stack version that emitted this tree event.
        version: StackVersionId,
        /// Parser tree node.
        node: TreeNodeId,
        /// Public node kind, when this close event materializes a visible node.
        public_node: Option<PublicNodeKindId>,
        /// Byte range.
        bytes: ByteRange,
        /// Point range.
        points: PointRange,
    },
    /// A reusable subtree was accepted.
    ReuseNode {
        /// Stack version that emitted this tree event.
        version: StackVersionId,
        /// Parser tree node.
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
        /// Stack version that emitted this tree event.
        version: StackVersionId,
        /// Parent parser tree node.
        node: TreeNodeId,
        /// Visible child parser tree node, when the fielded step emitted one.
        child: Option<TreeNodeId>,
        /// Field id.
        field: FieldId,
        /// Structural child index.
        structural_index: usize,
    },
    /// An alias emitted or renamed a parser tree node at a structural child index.
    Alias {
        /// Stack version that emitted this tree event.
        version: StackVersionId,
        /// Parser tree node carrying the alias.
        node: TreeNodeId,
        /// Alias id.
        alias: AliasId,
        /// Whether this alias emits a named node/token.
        named: bool,
        /// Structural child index.
        structural_index: usize,
        /// Byte range.
        bytes: ByteRange,
        /// Point range.
        points: PointRange,
    },
}

impl TreeEvent {
    /// Stack version that emitted this tree event.
    pub const fn version(&self) -> StackVersionId {
        match self {
            Self::OpenNode { version, .. }
            | Self::Token { version, .. }
            | Self::Reduce { version, .. }
            | Self::Missing { version, .. }
            | Self::Error { version, .. }
            | Self::CloseNode { version, .. }
            | Self::ReuseNode { version, .. }
            | Self::Field { version, .. }
            | Self::Alias { version, .. } => *version,
        }
    }
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
    /// The branch accepted the input and left the live work queue.
    Accepted,
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
    use std::sync::OnceLock;

    use super::*;
    use crate::{
        corpus::{SexpChild, SexpNode, SexpValue},
        grammar::RawGrammarJson,
        lex_match::{match_pattern, match_pattern_with_flags},
        lexical::LexicalFacts,
        validated::ValidatedGrammar,
    };

    fn normalize(input: &str) -> ParserGrammar {
        let raw = RawGrammarJson::from_tree_sitter_json_str(input).unwrap();
        let validated = ValidatedGrammar::from_raw(&raw).unwrap();
        let lexical = LexicalFacts::from_grammar(&validated);
        ParserGrammar::normalize_from_validated(&validated, &lexical).unwrap()
    }

    fn prepared(input: &str) -> ParserGrammar {
        normalize(input).prepare_productions_for_items().unwrap()
    }

    fn prepared_with_validated(input: &str) -> (ValidatedGrammar, ParserGrammar, ParseTable) {
        let raw = RawGrammarJson::from_tree_sitter_json_str(input).unwrap();
        let validated = ValidatedGrammar::from_raw(&raw).unwrap();
        let lexical = LexicalFacts::from_grammar(&validated);
        let parser = ParserGrammar::normalize_from_validated(&validated, &lexical)
            .unwrap()
            .prepare_productions_for_items()
            .unwrap();
        let table = ParseTable::from_grammar(&parser).unwrap();
        (validated, parser, table)
    }

    fn authored_gingembre_styx() -> &'static str {
        r#"
name gingembre

rules {
    template {type SYMBOL, name interpolation}
    interpolation {
        type SEQ
        members (
            {type SYMBOL, name _open_expr}
            {type SYMBOL, name _expr}
            {type SYMBOL, name _close_expr}
        )
    }
    _open_expr {
        type CHOICE
        members (
            {type STRING, value "{{"}
            {type STRING, value "{{-"}
        )
    }
    _close_expr {
        type CHOICE
        members (
            {type STRING, value "}}"}
            {type STRING, value "-}}"}
        )
    }
    _expr {
        type CHOICE
        members (
            {type SYMBOL, name var_ref}
            {type SYMBOL, name literal}
            {type SYMBOL, name field_expr}
            {type SYMBOL, name call_expr}
            {type SYMBOL, name index_expr}
            {type SYMBOL, name optional_expr}
            {type SYMBOL, name paren_expr}
            {type SYMBOL, name list_lit}
            {type SYMBOL, name dict_lit}
            {type SYMBOL, name filter_expr}
            {type SYMBOL, name test_expr}
            {type SYMBOL, name unary_expr}
            {type SYMBOL, name binary_expr}
        )
    }
    var_ref {
        type SYMBOL
        name _ident
    }
    _postfix_expr {
        type CHOICE
        members (
            {type SYMBOL, name var_ref}
            {type SYMBOL, name literal}
            {type SYMBOL, name field_expr}
            {type SYMBOL, name call_expr}
            {type SYMBOL, name index_expr}
            {type SYMBOL, name optional_expr}
            {type SYMBOL, name paren_expr}
            {type SYMBOL, name list_lit}
            {type SYMBOL, name dict_lit}
        )
    }
    literal {
        type CHOICE
        members (
            {type STRING, value "true"}
            {type STRING, value "false"}
            {type STRING, value "none"}
            {type SYMBOL, name _float}
            {type SYMBOL, name _int}
            {type SYMBOL, name _string}
        )
    }
    field_expr {
        type PREC_LEFT
        value 50
        content {
            type SEQ
            members (
                {type SYMBOL, name _postfix_expr}
                {type STRING, value "."}
                {type SYMBOL, name _ident}
            )
        }
    }
    call_expr {
        type PREC_LEFT
        value 50
        content {
            type SEQ
            members (
                {type SYMBOL, name _postfix_expr}
                {type SYMBOL, name arg_list}
            )
        }
    }
    index_expr {
        type PREC_LEFT
        value 50
        content {
            type SEQ
            members (
                {type SYMBOL, name _postfix_expr}
                {type STRING, value "["}
                {type SYMBOL, name _expr}
                {type STRING, value "]"}
            )
        }
    }
    optional_expr {
        type PREC_LEFT
        value 50
        content {
            type SEQ
            members (
                {type SYMBOL, name _postfix_expr}
                {type STRING, value "?"}
            )
        }
    }
    paren_expr {
        type SEQ
        members (
            {type STRING, value "("}
            {type SYMBOL, name _expr}
            {type STRING, value ")"}
        )
    }
    list_lit {
        type SEQ
        members (
            {type STRING, value "["}
            {type CHOICE, members (
                {type BLANK}
                {type SEQ, members (
                    {type SYMBOL, name _list_item}
                    {type REPEAT, content {
                        type SEQ
                        members (
                            {type STRING, value ","}
                            {type SYMBOL, name _list_item}
                        )
                    }}
                    {type CHOICE, members (
                        {type STRING, value ","}
                        {type BLANK}
                    )}
                )}
            )}
            {type STRING, value "]"}
        )
    }
    _list_item {
        type SYMBOL
        name _expr
    }
    dict_lit {
        type SEQ
        members (
            {type STRING, value "{"}
            {type CHOICE, members (
                {type BLANK}
                {type SEQ, members (
                    {type SYMBOL, name _dict_item}
                    {type REPEAT, content {
                        type SEQ
                        members (
                            {type STRING, value ","}
                            {type SYMBOL, name _dict_item}
                        )
                    }}
                    {type CHOICE, members (
                        {type STRING, value ","}
                        {type BLANK}
                    )}
                )}
            )}
            {type STRING, value "}"}
        )
    }
    _dict_item {
        type SEQ
        members (
            {type SYMBOL, name _expr}
            {type STRING, value ":"}
            {type SYMBOL, name _expr}
        )
    }
    filter_expr {
        type PREC_LEFT
        value 40
        content {
            type SEQ
            members (
                {type SYMBOL, name _expr}
                {type STRING, value "|"}
                {type SYMBOL, name _ident}
                {type CHOICE, members (
                    {type SYMBOL, name arg_list}
                    {type BLANK}
                )}
            )
        }
    }
    test_expr {
        type PREC_LEFT
        value -10
        content {
            type SEQ
            members (
                {type SYMBOL, name _expr}
                {type PATTERN, value r"is\b"}
                {type CHOICE, members (
                    {type PATTERN, value r"not\b"}
                    {type BLANK}
                )}
                {type CHOICE, members (
                    {type SYMBOL, name _ident}
                    {type STRING, value "none"}
                )}
                {type CHOICE, members (
                    {type SYMBOL, name arg_list}
                    {type BLANK}
                )}
            )
        }
    }
    unary_expr {
        type CHOICE
        members (
            {type PREC_RIGHT, value -15, content {
                type SEQ
                members (
                    {type PATTERN, value r"not\b"}
                    {type SYMBOL, name _expr}
                )
            }}
            {type PREC_RIGHT, value 25, content {
                type SEQ
                members (
                    {type STRING, value "-"}
                    {type SYMBOL, name _expr}
                )
            }}
        )
    }
    arg_list {
        type SEQ
        members (
            {type STRING, value "("}
            {type CHOICE, members (
                {type BLANK}
                {type SEQ, members (
                    {type SYMBOL, name _arg_item}
                    {type REPEAT, content {
                        type SEQ
                        members (
                            {type STRING, value ","}
                            {type SYMBOL, name _arg_item}
                        )
                    }}
                    {type CHOICE, members (
                        {type STRING, value ","}
                        {type BLANK}
                    )}
                )}
            )}
            {type STRING, value ")"}
        )
    }
    _arg_item {
        type CHOICE
        members (
            {type SYMBOL, name arg}
            {type SYMBOL, name kw_arg}
        )
    }
    arg {
        type SYMBOL
        name _expr
    }
    kw_arg {
        type SEQ
        members (
            {type SYMBOL, name _ident}
            {type STRING, value "="}
            {type SYMBOL, name _expr}
        )
    }
    binary_expr {
        type CHOICE
        members (
            {type PREC_LEFT, value -30, content {
                type SEQ
                members (
                    {type SYMBOL, name _expr}
                    {type PATTERN, value r"or\b"}
                    {type SYMBOL, name _expr}
                )
            }}
            {type PREC_LEFT, value -20, content {
                type SEQ
                members (
                    {type SYMBOL, name _expr}
                    {type PATTERN, value r"and\b"}
                    {type SYMBOL, name _expr}
                )
            }}
            {type PREC_LEFT, value -10, content {
                type SEQ
                members (
                    {type SYMBOL, name _expr}
                    {type CHOICE, members (
                        {type STRING, value "=="}
                        {type STRING, value "!="}
                        {type STRING, value "<"}
                        {type STRING, value ">"}
                        {type STRING, value "<="}
                        {type STRING, value ">="}
                        {type PATTERN, value r"in\b"}
                        {type SEQ, members (
                            {type PATTERN, value r"not\b"}
                            {type PATTERN, value r"in\b"}
                        )}
                    )}
                    {type SYMBOL, name _expr}
                )
            }}
            {type PREC_LEFT, value 10, content {
                type SEQ
                members (
                    {type SYMBOL, name _expr}
                    {type CHOICE, members (
                        {type STRING, value "+"}
                        {type STRING, value "-"}
                        {type STRING, value "~"}
                    )}
                    {type SYMBOL, name _expr}
                )
            }}
            {type PREC_LEFT, value 20, content {
                type SEQ
                members (
                    {type SYMBOL, name _expr}
                    {type CHOICE, members (
                        {type STRING, value "*"}
                        {type STRING, value "/"}
                        {type STRING, value "//"}
                        {type STRING, value "%"}
                    )}
                    {type SYMBOL, name _expr}
                )
            }}
            {type PREC_RIGHT, value 30, content {
                type SEQ
                members (
                    {type SYMBOL, name _expr}
                    {type STRING, value "**"}
                    {type SYMBOL, name _expr}
                )
            }}
        )
    }
    _ident {
        type TOKEN
        content {
            type PATTERN
            value r"(?!if\b|elif\b|else\b|endif\b|for\b|endfor\b|set\b|endset\b|block\b|endblock\b|extends\b|include\b|import\b|macro\b|endmacro\b|break\b|continue\b|as\b|in\b|is\b|not\b|and\b|or\b|true\b|True\b|false\b|False\b|none\b|None\b)[A-Za-z_][A-Za-z0-9_]*"
        }
    }
    _int {
        type TOKEN
        content {
            type PATTERN
            value r"\d+"
        }
    }
    _float {
        type TOKEN
        content {
            type PATTERN
            value r"\d+\.\d+"
        }
    }
    _string {
        type TOKEN
        content {
            type CHOICE
            members (
                {type PATTERN, value "\"([^\"\\\\]|\\\\.)*\""}
                {type PATTERN, value "'([^'\\\\]|\\\\.)*'"}
            )
        }
    }
}

extras (
    {type PATTERN, value r"\s+"}
)
"#
    }

    fn authored_gingembre_parser_fixture() -> (ValidatedGrammar, ParserGrammar, ParseTable) {
        use facet_styx::RenderError;

        let source = authored_gingembre_styx();
        let raw: RawGrammarJson = facet_styx::from_str(source)
            .unwrap_or_else(|error| panic!("{}", error.render("gingembre-snark.styx", source)));
        let validated = ValidatedGrammar::from_raw(&raw).unwrap();
        let lexical = LexicalFacts::from_grammar(&validated);
        let parser = ParserGrammar::normalize_from_validated(&validated, &lexical)
            .unwrap()
            .prepare_productions_for_items()
            .unwrap();
        let table = ParseTable::from_grammar(&parser).unwrap();
        (validated, parser, table)
    }

    fn authored_gingembre_weavy_fixture() -> &'static (
        ValidatedGrammar,
        ParserGrammar,
        ParseTable,
        crate::lower::weavy::WeavyParsePlan,
    ) {
        static FIXTURE: OnceLock<(
            ValidatedGrammar,
            ParserGrammar,
            ParseTable,
            crate::lower::weavy::WeavyParsePlan,
        )> = OnceLock::new();

        FIXTURE.get_or_init(|| {
            let (validated, parser, table) = authored_gingembre_parser_fixture();
            let plan =
                crate::lower::weavy::WeavyParsePlan::new(&validated, &parser, &table).unwrap();
            (validated, parser, table, plan)
        })
    }

    fn gingembre_named_projection(input: &str) -> SexpNode {
        let parse = gingembre_syntax::parse(input);
        assert!(
            parse.errors.is_empty(),
            "gingembre parser reported errors for {input:?}: {:?}",
            parse.errors
        );
        gingembre_node_projection(parse.syntax()).unwrap()
    }

    fn gingembre_node_projection(node: &gingembre_syntax::ResolvedNode) -> Option<SexpNode> {
        let kind = match node.kind() {
            gingembre_syntax::SyntaxKind::Template => "template",
            gingembre_syntax::SyntaxKind::Interpolation => "interpolation",
            gingembre_syntax::SyntaxKind::Literal => "literal",
            gingembre_syntax::SyntaxKind::VarRef => "var_ref",
            gingembre_syntax::SyntaxKind::FieldExpr => "field_expr",
            gingembre_syntax::SyntaxKind::CallExpr => "call_expr",
            gingembre_syntax::SyntaxKind::IndexExpr => "index_expr",
            gingembre_syntax::SyntaxKind::OptionalExpr => "optional_expr",
            gingembre_syntax::SyntaxKind::ParenExpr => "paren_expr",
            gingembre_syntax::SyntaxKind::ListLit => "list_lit",
            gingembre_syntax::SyntaxKind::DictLit => "dict_lit",
            gingembre_syntax::SyntaxKind::FilterExpr => "filter_expr",
            gingembre_syntax::SyntaxKind::TestExpr => "test_expr",
            gingembre_syntax::SyntaxKind::UnaryExpr => "unary_expr",
            gingembre_syntax::SyntaxKind::BinaryExpr => "binary_expr",
            gingembre_syntax::SyntaxKind::ArgList => "arg_list",
            gingembre_syntax::SyntaxKind::Arg => "arg",
            gingembre_syntax::SyntaxKind::KwArg => "kw_arg",
            _ => return None,
        };
        let children = node
            .children()
            .filter_map(|child| {
                gingembre_node_projection(child).map(|node| SexpChild {
                    field: None,
                    value: SexpValue::Node(node),
                })
            })
            .collect();
        Some(SexpNode {
            kind: kind.to_owned(),
            children,
        })
    }

    fn assert_styx_authored_gingembre_parse(input: &str, expected_sexp: &str) {
        let (_, parser, table, plan) = authored_gingembre_weavy_fixture();
        let tree =
            crate::lower::weavy::parse_prepared_weavy_tree(plan, parser, table, input).unwrap();
        let expected = gingembre_named_projection(input);

        rediff::assert_same!(&tree, &expected);
        assert_eq!(expected.to_sexp(), expected_sexp);
    }

    fn collect_resolved_terminal_texts<'a>(node: &'a ResolvedCstNode, texts: &mut Vec<&'a str>) {
        if let Some(text) = node.text() {
            texts.push(text);
        }
        for child in node.children() {
            collect_resolved_terminal_texts(child, texts);
        }
    }

    fn resolved_terminal_texts(node: &ResolvedCstNode) -> Vec<&str> {
        let mut texts = Vec::new();
        collect_resolved_terminal_texts(node, &mut texts);
        texts
    }

    fn test_resolved_range(start: u32, end: u32) -> (ByteRange, PointRange) {
        use crate::runtime_input::{ByteOffset, PointBytes, Row, Utf8ColumnBytes};

        let bytes = ByteRange::new(ByteOffset::new(start), ByteOffset::new(end)).unwrap();
        let points = PointRange::new(
            PointBytes::new(Row::new(0), Utf8ColumnBytes::new(start)),
            PointBytes::new(Row::new(0), Utf8ColumnBytes::new(end)),
        )
        .unwrap();
        (bytes, points)
    }

    fn test_resolved_item(
        kind: &str,
        node: Option<TreeNodeId>,
        start: u32,
        end: u32,
        order: usize,
    ) -> ResolvedCstItem {
        let (bytes, points) = test_resolved_range(start, end);
        ResolvedCstItem {
            kind: Arc::<str>::from(kind),
            symbol: None,
            field: None,
            node,
            bytes,
            points,
            named: node.is_some(),
            visible: true,
            extra: false,
            text: None,
            order,
            children: Vec::new(),
        }
    }

    #[test]
    fn resolved_cst_attachment_handles_interleaved_event_order_roots() {
        let mut items = vec![
            test_resolved_item("child", None, 0, 1, 0),
            test_resolved_item("later_sibling", None, 5, 6, 1),
            test_resolved_item("parent", Some(TreeNodeId::from_index(0)), 0, 2, 2),
        ];

        let mut roots = attach_resolved_children_from_ranges(&mut items);
        sort_resolved_children(&mut roots, &items);

        assert_eq!(items[2].children, vec![0]);
        assert_eq!(roots, vec![2, 1]);
    }

    fn assert_styx_authored_gingembre_rejects_like_gingembre(input: &str) {
        let gingembre = gingembre_syntax::parse(input);
        assert!(
            !gingembre.errors.is_empty(),
            "gingembre unexpectedly accepted {input:?}"
        );

        let (_, parser, table, plan) = authored_gingembre_weavy_fixture();
        let result = crate::lower::weavy::parse_prepared_weavy_tree(plan, parser, table, input);
        assert!(result.is_err(), "snark unexpectedly accepted {input:?}");
    }

    #[test]
    fn keeps_common_ascii_identifier_pattern_grammar_neutral() {
        assert_eq!(match_pattern(ASCII_IDENTIFIER_PATTERN, "if", 0), Some(2));
        assert_eq!(match_pattern(GINGEMBRE_IDENTIFIER_PATTERN, "if", 0), None);
    }

    #[test]
    fn matches_gingembre_strings_with_escaped_non_ascii_scalars() {
        assert_eq!(
            match_pattern("\"([^\"\\\\]|\\\\.)*\"", r#""\é""#, 0),
            Some(r#""\é""#.len())
        );
        assert_eq!(
            match_pattern("'([^'\\\\]|\\\\.)*'", "'\\é'", 0),
            Some("'\\é'".len())
        );
    }

    #[test]
    fn matches_css_plain_value_regex_subset() {
        assert_eq!(match_pattern("[a-zA-Z]", "http", 0), Some(1));
        assert_eq!(match_pattern("[-_]", "-rest", 0), Some(1));
        assert_eq!(match_pattern("[a-zA-Z0-9-_]", "_", 0), Some(1));
        assert_eq!(match_pattern("[^/\\s,;!{}()\\[\\]]", ":", 0), Some(1));
        assert_eq!(match_pattern("[^/\\s,;!{}()\\[\\]]", "/", 0), None);
        assert_eq!(
            match_pattern("\\/[^\\*\\s,;!{}()\\[\\]]", "/1999", 0),
            Some(2)
        );
        assert_eq!(match_pattern("\\/[^\\*\\s,;!{}()\\[\\]]", "/*", 0), None);
    }

    #[test]
    fn matches_common_word_regex_subset() {
        assert_eq!(match_pattern("\\w+", "www-data", 0), Some(3));
        assert_eq!(match_pattern("\\w+", "_worker1", 0), Some(8));
        assert_eq!(match_pattern("\\w+", "-worker", 0), None);
    }

    #[test]
    fn backtracks_repeated_regex_atoms_for_nginx_arguments() {
        assert_eq!(
            match_pattern("[\\w/\\-\\.]*[A-Za-z][\\w/\\-=,?]+", "www", 0),
            Some(3)
        );
        assert_eq!(
            match_pattern("[\\w/\\-\\.]*[A-Za-z][\\w/\\-=,?]+", "www-data", 0),
            Some(8)
        );
    }

    #[test]
    fn matches_nginx_js_regex_leaf_shapes() {
        assert_eq!(
            match_pattern("#.*\\n", "# https://nginx.org/en/docs/\nuser www-data;", 0),
            Some("# https://nginx.org/en/docs/\n".len())
        );
        assert_eq!(
            match_pattern("[\\s\\p{Zs}\\uFEFF\\u2060\\u200B]", "\u{FEFF}rest", 0),
            Some("\u{FEFF}".len())
        );
        assert_eq!(match_pattern("\\w+:\\/\\/", "http://host", 0), Some(7));
    }

    #[test]
    fn matches_flagged_regex_leaf_shapes() {
        assert_eq!(match_pattern("abc", "ABC", 0), None);
        assert_eq!(
            match_pattern_with_flags("abc", Some("i"), "ABC", 0),
            Some(3)
        );
        assert_eq!(
            match_pattern_with_flags("abc", Some("iu"), "ABC", 0),
            Some(3)
        );
    }

    #[test]
    fn parses_styx_authored_gingembre_interpolation_like_gingembre() {
        assert_styx_authored_gingembre_parse("{{ x }}", "(template (interpolation (var_ref)))");
    }

    #[test]
    fn parses_styx_authored_gingembre_trim_interpolation_like_gingembre() {
        assert_styx_authored_gingembre_parse("{{- x -}}", "(template (interpolation (var_ref)))");
    }

    #[test]
    fn accepted_resolved_tree_preserves_anonymous_gingembre_delimiters() {
        let (_, parser, table, plan) = authored_gingembre_weavy_fixture();
        let input = "{{- x -}}";
        let tree =
            crate::lower::weavy::parse_prepared_weavy_resolved_tree(plan, parser, table, input)
                .unwrap();
        let texts = resolved_terminal_texts(&tree);

        assert_eq!(tree.kind(), "template");
        assert!(texts.contains(&"{{-"), "resolved terminals: {texts:?}");
        assert!(texts.contains(&"-}}"), "resolved terminals: {texts:?}");
    }

    #[test]
    fn accepted_resolved_tree_preserves_anonymous_gingembre_operators() {
        let (_, parser, table, plan) = authored_gingembre_weavy_fixture();
        let input = "{{ 1 + 2 * 3 }}";
        let tree =
            crate::lower::weavy::parse_prepared_weavy_resolved_tree(plan, parser, table, input)
                .unwrap();
        let texts = resolved_terminal_texts(&tree);

        assert_eq!(tree.kind(), "template");
        assert!(texts.contains(&"+"), "resolved terminals: {texts:?}");
        assert!(texts.contains(&"*"), "resolved terminals: {texts:?}");
    }
    #[test]
    fn prepared_weavy_resolved_tree_preserves_anonymous_gingembre_operators() {
        let (_, parser, table, plan) = authored_gingembre_weavy_fixture();
        let input = "{{ 1 + 2 * 3 }}";
        let tree =
            crate::lower::weavy::parse_prepared_weavy_resolved_tree(plan, parser, table, input)
                .unwrap();
        let texts = resolved_terminal_texts(&tree);

        assert_eq!(tree.kind(), "template");
        assert!(texts.contains(&"+"), "resolved terminals: {texts:?}");
        assert!(texts.contains(&"*"), "resolved terminals: {texts:?}");
    }

    #[test]
    fn prepared_weavy_resolved_cst_materializes_like_resolved_tree() {
        let (_, parser, table, plan) = authored_gingembre_weavy_fixture();
        let input = "{{ 1 + 2 * 3 }}";
        let tree =
            crate::lower::weavy::parse_prepared_weavy_resolved_tree(plan, parser, table, input)
                .unwrap();
        let arena =
            crate::lower::weavy::parse_prepared_weavy_resolved_cst(plan, parser, table, input)
                .unwrap();
        let report = crate::lower::weavy::parse_prepared_weavy_resolved_cst_report(
            plan, parser, table, input,
        )
        .unwrap();

        assert_eq!(arena.root_kind(), Some("template"));
        assert_eq!(arena.to_owned_node(), Some(tree.clone()));
        assert_eq!(report.tree().to_owned_node(), Some(tree));
        assert!(report.lexer_stats().lex_call_count > 0);
        assert!(report.snark_stats().intrinsic_count > 0);
        assert_eq!(
            report.execution_lane(),
            crate::lower::weavy::WeavyParseExecutionLane::Direct
        );
    }

    #[test]
    fn parses_styx_authored_gingembre_literals_like_gingembre() {
        assert_styx_authored_gingembre_parse("{{ true }}", "(template (interpolation (literal)))");
        assert_styx_authored_gingembre_parse("{{ none }}", "(template (interpolation (literal)))");
        assert_styx_authored_gingembre_parse("{{ 42 }}", "(template (interpolation (literal)))");
        assert_styx_authored_gingembre_parse("{{ 1.25 }}", "(template (interpolation (literal)))");
        assert_styx_authored_gingembre_parse(
            r#"{{ "x" }}"#,
            "(template (interpolation (literal)))",
        );
        assert_styx_authored_gingembre_parse("{{ 'x' }}", "(template (interpolation (literal)))");
        assert_styx_authored_gingembre_parse(
            r#"{{ "a\"b" }}"#,
            "(template (interpolation (literal)))",
        );
    }

    #[test]
    fn rejects_styx_authored_gingembre_statement_keyword_like_gingembre() {
        assert_styx_authored_gingembre_rejects_like_gingembre("{{ if }}");
    }

    #[test]
    fn rejects_styx_authored_gingembre_word_operator_prefixes_like_gingembre() {
        assert_styx_authored_gingembre_rejects_like_gingembre("{{ a orx }}");
        assert_styx_authored_gingembre_rejects_like_gingembre("{{ a andx }}");
        assert_styx_authored_gingembre_rejects_like_gingembre("{{ a inbox }}");
        assert_styx_authored_gingembre_rejects_like_gingembre("{{ a notin xs }}");
        assert_styx_authored_gingembre_rejects_like_gingembre("{{ value isx }}");
    }

    #[test]
    fn parses_styx_authored_gingembre_field_access_like_gingembre() {
        assert_styx_authored_gingembre_parse(
            "{{ user.name }}",
            "(template (interpolation (field_expr (var_ref))))",
        );
    }

    #[test]
    fn parses_styx_authored_gingembre_call_arguments_like_gingembre() {
        assert_styx_authored_gingembre_parse(
            "{{ greet(user.name, suffix) }}",
            "(template (interpolation (call_expr (var_ref) (arg_list (arg (field_expr (var_ref))) (arg (var_ref))))))",
        );
    }

    #[test]
    fn parses_styx_authored_gingembre_call_arg_shapes_like_gingembre() {
        assert_styx_authored_gingembre_parse(
            "{{ greet() }}",
            "(template (interpolation (call_expr (var_ref) (arg_list))))",
        );
        assert_styx_authored_gingembre_parse(
            "{{ greet(suffix,) }}",
            "(template (interpolation (call_expr (var_ref) (arg_list (arg (var_ref))))))",
        );
        assert_styx_authored_gingembre_parse(
            "{{ greet(name=user.name) }}",
            "(template (interpolation (call_expr (var_ref) (arg_list (kw_arg (field_expr (var_ref)))))))",
        );
    }

    #[test]
    fn parses_styx_authored_gingembre_index_postfix_like_gingembre() {
        assert_styx_authored_gingembre_parse(
            "{{ items[1] }}",
            "(template (interpolation (index_expr (var_ref) (literal))))",
        );
        assert_styx_authored_gingembre_parse(
            "{{ items[1].name }}",
            "(template (interpolation (field_expr (index_expr (var_ref) (literal)))))",
        );
        assert_styx_authored_gingembre_parse(
            "{{ fetch()[0] }}",
            "(template (interpolation (index_expr (call_expr (var_ref) (arg_list)) (literal))))",
        );
    }

    #[test]
    fn parses_styx_authored_gingembre_optional_postfix_like_gingembre() {
        assert_styx_authored_gingembre_parse(
            "{{ user? }}",
            "(template (interpolation (optional_expr (var_ref))))",
        );
        assert_styx_authored_gingembre_parse(
            "{{ user?.name }}",
            "(template (interpolation (field_expr (optional_expr (var_ref)))))",
        );
        assert_styx_authored_gingembre_parse(
            "{{ fetch()[0]? | default(none) }}",
            "(template (interpolation (filter_expr (optional_expr (index_expr (call_expr (var_ref) (arg_list)) (literal))) (arg_list (arg (literal))))))",
        );
    }

    #[test]
    fn parses_styx_authored_gingembre_compound_primaries_like_gingembre() {
        assert_styx_authored_gingembre_parse(
            "{{ (a + b) * c }}",
            "(template (interpolation (binary_expr (paren_expr (binary_expr (var_ref) (var_ref))) (var_ref))))",
        );
        assert_styx_authored_gingembre_parse(
            "{{ [1, user.name, none,] }}",
            "(template (interpolation (list_lit (literal) (field_expr (var_ref)) (literal))))",
        );
        assert_styx_authored_gingembre_parse("{{ [] }}", "(template (interpolation (list_lit)))");
        assert_styx_authored_gingembre_parse("{{ {} }}", "(template (interpolation (dict_lit)))");
        assert_styx_authored_gingembre_parse(
            r#"{{ {"name": user.name, "ok": true,} }}"#,
            "(template (interpolation (dict_lit (literal) (field_expr (var_ref)) (literal) (literal))))",
        );
    }

    #[test]
    fn parses_styx_authored_gingembre_binary_precedence_like_gingembre() {
        assert_styx_authored_gingembre_parse(
            "{{ a + b * c }}",
            "(template (interpolation (binary_expr (var_ref) (binary_expr (var_ref) (var_ref)))))",
        );
        assert_styx_authored_gingembre_parse(
            "{{ a * b + c }}",
            "(template (interpolation (binary_expr (binary_expr (var_ref) (var_ref)) (var_ref))))",
        );
        assert_styx_authored_gingembre_parse(
            "{{ a + b + c }}",
            "(template (interpolation (binary_expr (binary_expr (var_ref) (var_ref)) (var_ref))))",
        );
        assert_styx_authored_gingembre_parse(
            "{{ a - b ~ c }}",
            "(template (interpolation (binary_expr (binary_expr (var_ref) (var_ref)) (var_ref))))",
        );
        assert_styx_authored_gingembre_parse(
            "{{ a / b % c }}",
            "(template (interpolation (binary_expr (binary_expr (var_ref) (var_ref)) (var_ref))))",
        );
        assert_styx_authored_gingembre_parse(
            "{{ a ** b ** c }}",
            "(template (interpolation (binary_expr (var_ref) (binary_expr (var_ref) (var_ref)))))",
        );
        assert_styx_authored_gingembre_parse(
            "{{ a or b and c }}",
            "(template (interpolation (binary_expr (var_ref) (binary_expr (var_ref) (var_ref)))))",
        );
        assert_styx_authored_gingembre_parse(
            "{{ a == b + c }}",
            "(template (interpolation (binary_expr (var_ref) (binary_expr (var_ref) (var_ref)))))",
        );
        assert_styx_authored_gingembre_parse(
            "{{ a not in xs }}",
            "(template (interpolation (binary_expr (var_ref) (var_ref))))",
        );
    }

    #[test]
    fn parses_styx_authored_gingembre_filters_and_tests_like_gingembre() {
        assert_styx_authored_gingembre_parse(
            "{{ name | upper }}",
            "(template (interpolation (filter_expr (var_ref))))",
        );
        assert_styx_authored_gingembre_parse(
            "{{ items | slice(0, 2) }}",
            "(template (interpolation (filter_expr (var_ref) (arg_list (arg (literal)) (arg (literal))))))",
        );
        assert_styx_authored_gingembre_parse(
            "{{ value | default(fallback) is not none }}",
            "(template (interpolation (test_expr (filter_expr (var_ref) (arg_list (arg (var_ref)))))))",
        );
        assert_styx_authored_gingembre_parse(
            "{{ value is none or fallback }}",
            "(template (interpolation (binary_expr (test_expr (var_ref)) (var_ref))))",
        );
        assert_styx_authored_gingembre_parse(
            "{{ a + b is sameas(c) }}",
            "(template (interpolation (test_expr (binary_expr (var_ref) (var_ref)) (arg_list (arg (var_ref))))))",
        );
        assert_styx_authored_gingembre_parse(
            "{{ name | upper ** power }}",
            "(template (interpolation (binary_expr (filter_expr (var_ref)) (var_ref))))",
        );
    }

    #[test]
    fn parses_styx_authored_gingembre_unary_precedence_like_gingembre() {
        assert_styx_authored_gingembre_parse(
            "{{ not a and b }}",
            "(template (interpolation (binary_expr (unary_expr (var_ref)) (var_ref))))",
        );
        assert_styx_authored_gingembre_parse(
            "{{ not a == b }}",
            "(template (interpolation (unary_expr (binary_expr (var_ref) (var_ref)))))",
        );
        assert_styx_authored_gingembre_parse(
            "{{ a * -b }}",
            "(template (interpolation (binary_expr (var_ref) (unary_expr (var_ref)))))",
        );
        assert_styx_authored_gingembre_parse(
            "{{ -a ** b }}",
            "(template (interpolation (unary_expr (binary_expr (var_ref) (var_ref)))))",
        );
    }
    #[test]
    fn parses_styx_authored_gingembre_call_arguments_through_weavy_runtime() {
        let input = "{{ greet(user.name, suffix) }}";
        let (_, parser, table, plan) = authored_gingembre_weavy_fixture();
        let tree =
            crate::lower::weavy::parse_prepared_weavy_tree(plan, parser, table, input).unwrap();

        rediff::assert_same!(&tree, &gingembre_named_projection(input));
        assert_eq!(
            tree.to_sexp(),
            "(template (interpolation (call_expr (var_ref) (arg_list (arg (field_expr (var_ref))) (arg (var_ref))))))"
        );
    }
    #[test]
    fn parses_styx_authored_gingembre_index_postfix_through_weavy_runtime() {
        let input = "{{ items[1].name }}";
        let (_, parser, table, plan) = authored_gingembre_weavy_fixture();
        let tree =
            crate::lower::weavy::parse_prepared_weavy_tree(plan, parser, table, input).unwrap();

        rediff::assert_same!(&tree, &gingembre_named_projection(input));
        assert_eq!(
            tree.to_sexp(),
            "(template (interpolation (field_expr (index_expr (var_ref) (literal)))))"
        );
    }
    #[test]
    fn parses_styx_authored_gingembre_optional_postfix_through_weavy_runtime() {
        let input = "{{ fetch()[0]? | default(none) }}";
        let (_, parser, table, plan) = authored_gingembre_weavy_fixture();
        let tree =
            crate::lower::weavy::parse_prepared_weavy_tree(plan, parser, table, input).unwrap();

        rediff::assert_same!(&tree, &gingembre_named_projection(input));
        assert_eq!(
            tree.to_sexp(),
            "(template (interpolation (filter_expr (optional_expr (index_expr (call_expr (var_ref) (arg_list)) (literal))) (arg_list (arg (literal))))))"
        );
    }
    #[test]
    fn parses_styx_authored_gingembre_compound_primaries_through_weavy_runtime() {
        let input = r#"{{ {"name": user.name, "ok": true,} }}"#;
        let (_, parser, table, plan) = authored_gingembre_weavy_fixture();
        let tree =
            crate::lower::weavy::parse_prepared_weavy_tree(plan, parser, table, input).unwrap();

        rediff::assert_same!(&tree, &gingembre_named_projection(input));
        assert_eq!(
            tree.to_sexp(),
            "(template (interpolation (dict_lit (literal) (field_expr (var_ref)) (literal) (literal))))"
        );
    }
    #[test]
    fn parses_styx_authored_gingembre_filters_and_tests_through_weavy_runtime() {
        let input = "{{ value | default(fallback) is not none }}";
        let (_, parser, table, plan) = authored_gingembre_weavy_fixture();
        let tree =
            crate::lower::weavy::parse_prepared_weavy_tree(plan, parser, table, input).unwrap();

        rediff::assert_same!(&tree, &gingembre_named_projection(input));
        assert_eq!(
            tree.to_sexp(),
            "(template (interpolation (test_expr (filter_expr (var_ref) (arg_list (arg (var_ref)))))))"
        );
    }
    #[test]
    fn parses_styx_authored_gingembre_unary_precedence_through_weavy_runtime() {
        let input = "{{ not a == b }}";
        let (_, parser, table, plan) = authored_gingembre_weavy_fixture();
        let tree =
            crate::lower::weavy::parse_prepared_weavy_tree(plan, parser, table, input).unwrap();

        rediff::assert_same!(&tree, &gingembre_named_projection(input));
        assert_eq!(
            tree.to_sexp(),
            "(template (interpolation (unary_expr (binary_expr (var_ref) (var_ref)))))"
        );
    }
    #[test]
    fn parses_styx_authored_gingembre_binary_precedence_through_weavy_runtime() {
        let input = "{{ a + b * c }}";
        let (_, parser, table, plan) = authored_gingembre_weavy_fixture();
        let tree =
            crate::lower::weavy::parse_prepared_weavy_tree(plan, parser, table, input).unwrap();

        rediff::assert_same!(&tree, &gingembre_named_projection(input));
        assert_eq!(
            tree.to_sexp(),
            "(template (interpolation (binary_expr (var_ref) (binary_expr (var_ref) (var_ref)))))"
        );
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

        assert_eq!(grammar.stage(), ParserGenerationStage::ProductionsPrepared);
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
        assert!(grammar.alias_sequences()[alias_sequence.get() as usize].entries()[0].named());
        assert!(matches!(
            grammar.provenances()[item_metadata.provenance().unwrap().get() as usize].source(),
            ProvenanceSource::GrammarRule { .. }
        ));
        assert!(grammar.public_node_kinds().iter().any(|kind| {
            kind.name() == "a" && matches!(kind.source(), PublicNodeKindSource::AnonymousLiteral)
        }));
        let literal = grammar
            .public_literal_terminals()
            .iter()
            .find(|literal| literal.literal() == "a")
            .unwrap();
        assert_eq!(literal.terminals(), &[token.id()]);
    }

    #[test]
    fn anonymous_literal_provenance_keeps_all_contributing_expressions() {
        let grammar = normalize(
            r##"{
              "name": "mini",
              "rules": {
                "source_file": {
                  "type": "CHOICE",
                  "members": [
                    { "type": "SYMBOL", "name": "left_plus" },
                    { "type": "SYMBOL", "name": "right_plus" }
                  ]
                },
                "left_plus": { "type": "STRING", "value": "+" },
                "right_plus": { "type": "STRING", "value": "+" }
              }
            }"##,
        );

        let public_literal = grammar
            .public_literal_terminals()
            .iter()
            .find(|literal| literal.literal() == "+")
            .unwrap();

        assert_eq!(public_literal.terminals().len(), 1);
        let terminal = &grammar.symbols().terminals()[public_literal.terminals()[0].get() as usize];
        assert_eq!(terminal.source_exprs().len(), 2);
        assert_eq!(
            grammar
                .public_node_kinds()
                .iter()
                .filter(|kind| kind.name() == "+")
                .count(),
            1
        );
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
                        "value": -10,
                        "content": {
                          "type": "PREC_DYNAMIC",
                          "value": 3,
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

        assert_eq!(grammar.stage(), ParserGenerationStage::ProductionsPrepared);
        assert_eq!(grammar.productions().len(), 1);
        let metadata = &grammar.production_metadata()[0];
        assert_eq!(
            metadata.static_precedence(),
            Some(&StaticPrecedence::Named("tight".to_owned()))
        );
        assert_eq!(metadata.associativity(), Associativity::Left);
        assert_eq!(metadata.dynamic_precedence(), 3);
        assert_eq!(
            grammar.productions()[0].steps()[0].reserved_context(),
            Some(ReservedContextId::from_index(0))
        );
        assert_eq!(
            grammar.productions()[0].steps()[0].static_precedence(),
            Some(&StaticPrecedence::Named("tight".to_owned()))
        );
        assert_eq!(grammar.productions()[0].steps()[1].reserved_context(), None);
        assert_eq!(
            grammar.productions()[0].steps()[1].static_precedence(),
            None
        );
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
    fn rejects_repeat_content_that_is_nullable_through_a_symbol() {
        let raw = RawGrammarJson::from_tree_sitter_json_str(
            r##"{
              "name": "mini",
              "rules": {
                "source_file": {
                  "type": "REPEAT",
                  "content": { "type": "SYMBOL", "name": "maybe_item" }
                },
                "maybe_item": {
                  "type": "CHOICE",
                  "members": [
                    { "type": "BLANK" },
                    { "type": "STRING", "value": "x" }
                  ]
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

    #[test]
    fn prepares_productions_for_lr_item_generation() {
        let grammar = normalize(
            r##"{
              "name": "mini",
              "rules": {
                "source_file": { "type": "SYMBOL", "name": "item" },
                "item": { "type": "STRING", "value": "x" }
              },
              "inline": ["item"]
            }"##,
        );

        let prepared = grammar.prepare_productions_for_items().unwrap();

        assert_eq!(prepared.stage(), ParserGenerationStage::Productions);
        let facts = prepared.item_preparation().unwrap();
        assert_eq!(facts.inline_expansions().len(), 1);
        assert_eq!(facts.inline_expansions()[0].productions().len(), 1);
        assert!(facts.graph().nullable().is_empty());
        assert_eq!(facts.graph().reachable().len(), 2);
        assert_eq!(facts.graph().productive().len(), 2);
    }

    #[test]
    fn builds_lr_item_sets_and_parse_table_from_prepared_productions() {
        let grammar = prepared(
            r##"{
              "name": "mini",
              "rules": {
                "source_file": { "type": "SYMBOL", "name": "item" },
                "item": { "type": "STRING", "value": "x" }
              }
            }"##,
        );

        let table = ParseTable::from_grammar(&grammar).unwrap();

        assert_eq!(table.item_sets()[0].id(), ItemSetId::from_index(0));
        assert!(table.item_sets()[0].items().iter().any(|item| {
            item.production() == ProductionId::from_index(0)
                && item.dot() == 0
                && item.lookahead().symbols() == [LookaheadSymbol::Eof]
        }));
        assert!(table.item_sets()[0].items().iter().any(|item| {
            item.production() == ProductionId::from_index(1)
                && item.dot() == 0
                && item.lookahead().symbols() == [LookaheadSymbol::Eof]
        }));
        assert!(table.transitions().iter().any(|transition| {
            transition.from() == ItemSetId::from_index(0)
                && matches!(
                    transition.symbol(),
                    ParserSymbol::Terminal(_) | ParserSymbol::Nonterminal(_)
                )
        }));
        assert!(
            table
                .states()
                .iter()
                .flat_map(ParseState::entries)
                .any(|entry| entry.lookahead() == LookaheadSymbol::Eof
                    && entry.actions().iter().any(|action| {
                        matches!(
                            action,
                            ParseAction::Accept {
                                production: ProductionId(0),
                                symbol: NonterminalId(0),
                                child_count: 1,
                                ..
                            }
                        )
                    }))
        );
        assert!(
            table
                .states()
                .iter()
                .flat_map(ParseState::entries)
                .any(|entry| entry
                    .actions()
                    .iter()
                    .any(|action| matches!(action, ParseAction::Reduce { .. })))
        );
        assert_eq!(table.lexical_modes().len(), table.states().len());
    }

    #[test]
    fn table_generation_requires_prepared_productions() {
        let grammar = normalize(
            r##"{
              "name": "mini",
              "rules": {
                "source_file": { "type": "STRING", "value": "x" }
              }
            }"##,
        );

        let error = ParseTable::from_grammar(&grammar).unwrap_err();

        assert!(matches!(
            error.kind(),
            ParserTableBuildErrorKind::WrongStage {
                stage: ParserGenerationStage::ProductionsPrepared
            }
        ));
    }

    #[test]
    fn lr_closure_propagates_first_set_lookahead_through_suffix() {
        let grammar = prepared(
            r##"{
              "name": "mini",
              "rules": {
                "source_file": {
                  "type": "SEQ",
                  "members": [
                    { "type": "SYMBOL", "name": "wrapper" },
                    { "type": "STRING", "value": ";" }
                  ]
                },
                "wrapper": { "type": "SYMBOL", "name": "item" },
                "item": { "type": "STRING", "value": "x" }
              }
            }"##,
        );

        let table = ParseTable::from_grammar(&grammar).unwrap();
        let semicolon = grammar
            .symbols()
            .terminals()
            .iter()
            .find(|terminal| terminal.spelling() == ";")
            .unwrap()
            .id();

        assert!(table.item_sets()[0].items().iter().any(|item| {
            item.production() == ProductionId::from_index(1)
                && item.dot() == 0
                && item.lookahead().symbols() == [LookaheadSymbol::Terminal(semicolon)]
        }));
    }

    #[test]
    fn table_generation_preserves_reserved_context_lookaheads() {
        let grammar = prepared(
            r##"{
              "name": "mini",
              "rules": {
                "source_file": {
                  "type": "RESERVED",
                  "context_name": "default",
                  "content": { "type": "STRING", "value": "a" }
                }
              },
              "reserved": {
                "default": [
                  { "type": "STRING", "value": "if" }
                ]
              }
            }"##,
        );

        let table = ParseTable::from_grammar(&grammar).unwrap();
        let terminal = grammar
            .symbols()
            .terminals()
            .iter()
            .find(|terminal| terminal.spelling() == "a")
            .unwrap()
            .id();

        assert!(
            table
                .states()
                .iter()
                .flat_map(ParseState::entries)
                .any(|entry| entry.lookahead()
                    == LookaheadSymbol::ReservedWord {
                        terminal,
                        context: ReservedContextId::from_index(0)
                    })
        );
        assert!(
            table
                .lexical_modes()
                .iter()
                .any(|mode| mode.reserved_context() == Some(ReservedContextId::from_index(0)))
        );
    }

    #[test]
    fn table_generation_materializes_external_valid_symbol_sets() {
        let grammar = prepared(
            r##"{
              "name": "mini",
              "rules": {
                "source_file": { "type": "SYMBOL", "name": "_external" }
              },
              "externals": [
                { "type": "SYMBOL", "name": "_external" }
              ]
            }"##,
        );

        let table = ParseTable::from_grammar(&grammar).unwrap();

        assert_eq!(table.valid_symbol_sets().len(), 1);
        assert_eq!(
            table.valid_symbol_sets()[0].externals(),
            &[ExternalId::from_index(0)]
        );
        assert!(
            table
                .lexical_modes()
                .iter()
                .any(|mode| mode.valid_symbols() == Some(ValidSymbolSetId::from_index(0)))
        );
    }

    #[test]
    fn table_generation_adds_shift_extra_actions_for_extra_roots() {
        let grammar = prepared(
            r##"{
              "name": "mini",
              "rules": {
                "source_file": { "type": "STRING", "value": "x" }
              },
              "extras": [
                { "type": "STRING", "value": " " }
              ]
            }"##,
        );

        let table = ParseTable::from_grammar(&grammar).unwrap();
        let extra = grammar
            .symbols()
            .terminals()
            .iter()
            .find(|terminal| terminal.spelling() == " ")
            .unwrap()
            .id();

        assert!(
            table
                .states()
                .iter()
                .flat_map(ParseState::entries)
                .any(
                    |entry| entry.lookahead() == LookaheadSymbol::Terminal(extra)
                        && entry.actions().contains(&ParseAction::ShiftExtra)
                )
        );
    }

    #[test]
    fn weavy_lexer_matches_expected_terminal_matches() {
        let (validated, parser, table) = prepared_with_validated(
            r##"{
              "name": "mini",
              "rules": {
                "source_file": {
                  "type": "CHOICE",
                  "members": [
                    { "type": "STRING", "value": "=>" },
                    { "type": "PATTERN", "value": "[a-z]+" },
                    { "type": "SYMBOL", "name": "complex" },
                    { "type": "SYMBOL", "name": "run" }
                  ]
                },
                "complex": {
                  "type": "TOKEN",
                  "content": {
                    "type": "SEQ",
                    "members": [
                      { "type": "STRING", "value": "a" },
                      {
                        "type": "REPEAT",
                        "content": {
                          "type": "CHOICE",
                          "members": [
                            { "type": "STRING", "value": "b" },
                            { "type": "SYMBOL", "name": "digit" }
                          ]
                        }
                      },
                      { "type": "STRING", "value": "z" }
                    ]
                  }
                },
                "digit": { "type": "PATTERN", "value": "\\d" },
                "run": {
                  "type": "TOKEN",
                  "content": {
                    "type": "REPEAT1",
                    "content": { "type": "PATTERN", "value": "[xy]" }
                  }
                }
              }
            }"##,
        );
        let plan = crate::lower::weavy::WeavyParsePlan::new(&validated, &parser, &table).unwrap();
        let input = "=> abc ab3bz yxy q";
        let byte_positions = [0usize, 3, 7, 8, 14, 18];

        let observed = parser
            .symbols
            .terminals()
            .iter()
            .map(|terminal| {
                Ok((
                    terminal.kind(),
                    terminal.spelling().to_owned(),
                    weavy_terminal_ends(&plan, terminal, input, &byte_positions)?,
                ))
            })
            .collect::<Result<Vec<_>, crate::lower::weavy::WeavyParseError>>()
            .unwrap();

        assert!(observed.contains(&(
            ParserTerminalKind::String,
            "=>".to_owned(),
            vec![Some(2), None, None, None, None, None],
        )));
        assert!(observed.contains(&(
            ParserTerminalKind::Pattern,
            "[a-z]+".to_owned(),
            vec![None, Some(6), Some(9), Some(9), Some(16), None],
        )));
        assert!(
            observed
                .iter()
                .any(|(kind, _, ends)| *kind == ParserTerminalKind::Token
                    && *ends == vec![None, None, Some(12), None, None, None])
        );
        assert!(
            observed
                .iter()
                .any(|(kind, _, ends)| *kind == ParserTerminalKind::Token
                    && *ends == vec![None, None, None, None, Some(16), None])
        );
    }

    fn auto_close_fixture() -> (ValidatedGrammar, ParserGrammar, ParseTable) {
        prepared_with_validated(
            r##"{
              "name": "mini_html",
              "rules": {
                "source_file": {
                  "type": "REPEAT1",
                  "content": { "type": "SYMBOL", "name": "element" }
                },
                "element": {
                  "type": "SEQ",
                  "members": [
                    { "type": "SYMBOL", "name": "_start_p" },
                    {
                      "type": "REPEAT",
                      "content": {
                        "type": "CHOICE",
                        "members": [
                          { "type": "SYMBOL", "name": "text" },
                          { "type": "SYMBOL", "name": "element" }
                        ]
                      }
                    },
                    {
                      "type": "CHOICE",
                      "members": [
                        { "type": "SYMBOL", "name": "_end_p" },
                        { "type": "SYMBOL", "name": "_implicit_end_p" }
                      ]
                    }
                  ]
                },
                "_start_p": { "type": "STRING", "value": "<p>" },
                "_end_p": { "type": "STRING", "value": "</p>" },
                "_implicit_end_p": {
                  "type": "AUTO_CLOSE",
                  "tag": "p",
                  "open": "<p>",
                  "close": "</p>",
                  "closed_by": ["<p>"]
                },
                "text": { "type": "PATTERN", "value": "[a-z]+" }
              },
              "extras": []
            }"##,
        )
    }

    fn auto_close_node_fixture() -> (ValidatedGrammar, ParserGrammar, ParseTable) {
        prepared_with_validated(
            r##"{
              "name": "mini_html_nodes",
              "rules": {
                "document": {
                  "type": "REPEAT1",
                  "content": { "type": "SYMBOL", "name": "element" }
                },
                "element": {
                  "type": "SEQ",
                  "members": [
                    { "type": "SYMBOL", "name": "start_tag" },
                    {
                      "type": "REPEAT",
                      "content": {
                        "type": "CHOICE",
                        "members": [
                          { "type": "SYMBOL", "name": "text" },
                          { "type": "SYMBOL", "name": "element" }
                        ]
                      }
                    },
                    {
                      "type": "CHOICE",
                      "members": [
                        { "type": "SYMBOL", "name": "end_tag" },
                        { "type": "SYMBOL", "name": "_implicit_p_end" }
                      ]
                    }
                  ]
                },
                "start_tag": {
                  "type": "SEQ",
                  "members": [
                    { "type": "STRING", "value": "<" },
                    { "type": "SYMBOL", "name": "tag_name" },
                    { "type": "STRING", "value": ">" }
                  ]
                },
                "end_tag": {
                  "type": "SEQ",
                  "members": [
                    { "type": "STRING", "value": "</" },
                    { "type": "SYMBOL", "name": "tag_name" },
                    { "type": "STRING", "value": ">" }
                  ]
                },
                "_implicit_p_end": {
                  "type": "AUTO_CLOSE",
                  "tag": "implicit_end_tag",
                  "open_node": "start_tag",
                  "close_node": "end_tag",
                  "tag_name_node": "tag_name",
                  "start_prefix": "<",
                  "end_prefix": "</",
                  "rules": [
                    { "tag": "p", "closed_by_tags": ["p", "div"] },
                    { "tag": "li", "closed_by_tags": ["li"] }
                  ]
                },
                "tag_name": { "type": "PATTERN", "value": "[a-z]+" },
                "text": { "type": "PATTERN", "value": "[a-z]+" }
              },
              "extras": []
            }"##,
        )
    }
    #[test]
    fn weavy_runtime_inserts_declarative_auto_close_tokens() {
        let (validated, parser, table) = auto_close_fixture();
        let plan = crate::lower::weavy::WeavyParsePlan::new(&validated, &parser, &table).unwrap();
        let tree = crate::lower::weavy::parse_prepared_weavy_tree(
            &plan,
            &parser,
            &table,
            "<p>one<p>two</p>",
        )
        .unwrap();

        assert_eq!(
            tree.to_sexp(),
            "(source_file (element (text)) (element (text)))"
        );
    }
    #[test]
    fn weavy_runtime_inserts_node_driven_declarative_auto_close_tokens() {
        let (validated, parser, table) = auto_close_node_fixture();
        let plan = crate::lower::weavy::WeavyParsePlan::new(&validated, &parser, &table).unwrap();
        let tree = crate::lower::weavy::parse_prepared_weavy_tree(
            &plan,
            &parser,
            &table,
            "<p>one<div>two</div>",
        )
        .unwrap();

        assert_eq!(
            tree.to_sexp(),
            "(document (element (start_tag (tag_name)) (text)) (element (start_tag (tag_name)) (text) (end_tag (tag_name))))"
        );
    }

    fn flagged_regex_fixture() -> (ValidatedGrammar, ParserGrammar, ParseTable) {
        prepared_with_validated(
            r##"{
              "name": "mini",
              "rules": {
                "source_file": {
                  "type": "SEQ",
                  "members": [
                    { "type": "SYMBOL", "name": "insensitive" },
                    { "type": "SYMBOL", "name": "wrapped" }
                  ]
                },
                "insensitive": {
                  "type": "PATTERN",
                  "value": "abc",
                  "flags": "i"
                },
                "wrapped": {
                  "type": "TOKEN",
                  "content": {
                    "type": "PATTERN",
                    "value": "xyz",
                    "flags": "i"
                  }
                }
              }
            }"##,
        )
    }

    fn extra_comment_reuse_fixture() -> (ValidatedGrammar, ParserGrammar, ParseTable) {
        prepared_with_validated(
            r##"{
              "name": "mini",
              "rules": {
                "source_file": {
                  "type": "SEQ",
                  "members": [
                    { "type": "SYMBOL", "name": "left" },
                    { "type": "SYMBOL", "name": "right" }
                  ]
                },
                "left": { "type": "STRING", "value": "a" },
                "right": { "type": "STRING", "value": "b" },
                "comment": {
                  "type": "TOKEN",
                  "content": {
                    "type": "PATTERN",
                    "value": "#[^\\n]*"
                  }
                }
              },
              "extras": [
                { "type": "SYMBOL", "name": "comment" },
                { "type": "PATTERN", "value": "\\s+" }
              ]
            }"##,
        )
    }

    fn repeated_word_fixture() -> (ValidatedGrammar, ParserGrammar, ParseTable) {
        prepared_with_validated(
            r##"{
              "name": "mini",
              "rules": {
                "source_file": {
                  "type": "REPEAT1",
                  "content": { "type": "SYMBOL", "name": "word" }
                },
                "word": {
                  "type": "PATTERN",
                  "value": "[a-z]+"
                }
              },
              "extras": [
                { "type": "PATTERN", "value": "\\s+" }
              ]
            }"##,
        )
    }

    fn wrapped_extra_reuse_fixture() -> (ValidatedGrammar, ParserGrammar, ParseTable) {
        prepared_with_validated(
            r##"{
              "name": "mini",
              "rules": {
                "source_file": {
                  "type": "SEQ",
                  "members": [
                    { "type": "SYMBOL", "name": "wrapper" },
                    { "type": "SYMBOL", "name": "suffix" }
                  ]
                },
                "wrapper": {
                  "type": "SEQ",
                  "members": [
                    { "type": "SYMBOL", "name": "left" },
                    { "type": "SYMBOL", "name": "right" }
                  ]
                },
                "left": { "type": "STRING", "value": "a" },
                "right": { "type": "STRING", "value": "b" },
                "suffix": { "type": "PATTERN", "value": "[0-9]+" },
                "comment": {
                  "type": "TOKEN",
                  "content": {
                    "type": "PATTERN",
                    "value": "#[^\\n]*"
                  }
                }
              },
              "extras": [
                { "type": "SYMBOL", "name": "comment" },
                { "type": "PATTERN", "value": "\\s+" }
              ]
            }"##,
        )
    }

    fn reused_byte_ranges_from_events(events: &[TreeEvent]) -> Vec<(usize, usize)> {
        events
            .iter()
            .filter_map(|event| match event {
                TreeEvent::ReuseNode { bytes, .. } => {
                    Some((bytes.start().get() as usize, bytes.end().get() as usize))
                }
                _ => None,
            })
            .collect()
    }
    fn weavy_reused_byte_ranges(
        report: &crate::lower::weavy::WeavyParseReport,
    ) -> Vec<(usize, usize)> {
        reused_byte_ranges_from_events(report.tree_events())
    }

    fn weavy_terminal_end(
        plan: &crate::lower::weavy::WeavyParsePlan,
        terminal: &TerminalSymbol,
        input: &str,
        byte_position: usize,
    ) -> Result<Option<usize>, crate::lower::weavy::WeavyParseError> {
        match plan.match_terminal_for_tests(terminal.id(), input, byte_position) {
            Ok(result) => Ok(result.map(|match_| match_.end)),
            Err(crate::lower::weavy::WeavyParseError::MissingTerminal { .. }) => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn weavy_terminal_ends(
        plan: &crate::lower::weavy::WeavyParsePlan,
        terminal: &TerminalSymbol,
        input: &str,
        byte_positions: &[usize],
    ) -> Result<Vec<Option<usize>>, crate::lower::weavy::WeavyParseError> {
        byte_positions
            .iter()
            .map(|byte_position| weavy_terminal_end(plan, terminal, input, *byte_position))
            .collect()
    }

    #[test]
    fn weavy_lexer_preserves_regex_flags() {
        let (validated, parser, table) = flagged_regex_fixture();
        let plan = crate::lower::weavy::WeavyParsePlan::new(&validated, &parser, &table).unwrap();
        let input = "ABCXYZ";
        let byte_positions = [0usize, 3];
        let insensitive = parser
            .symbols
            .terminals()
            .iter()
            .find(|terminal| {
                terminal.kind() == ParserTerminalKind::Pattern
                    && terminal.spelling() == "abc"
                    && terminal.flags() == Some("i")
            })
            .unwrap();
        let wrapped = parser
            .symbols
            .terminals()
            .iter()
            .find(|terminal| terminal.kind() == ParserTerminalKind::Token)
            .unwrap();

        assert_eq!(
            weavy_terminal_ends(&plan, insensitive, input, &byte_positions).unwrap(),
            vec![Some(3), None]
        );
        assert_eq!(
            weavy_terminal_ends(&plan, wrapped, input, &byte_positions).unwrap(),
            vec![None, Some(6)]
        );
    }
    #[test]
    fn weavy_runtime_parse_session_reparse_matches_full_parse_oracle() {
        let (validated, parser, table) = flagged_regex_fixture();
        let plan = crate::lower::weavy::WeavyParsePlan::new(&validated, &parser, &table).unwrap();
        let mut session = crate::lower::weavy::WeavyParseSession::new(&plan, &parser, &table);
        let first = session.parse("ABCXYZ").unwrap().clone();
        assert_eq!(
            first.tree().to_sexp(),
            "(source_file (insensitive) (wrapped))"
        );
        assert!(first.trace_events().is_empty());
        assert_eq!(session.last_input(), Some("ABCXYZ"));

        let edit = ParserInputEdit::new(0, 3, 3);
        let reparsed = session.reparse(edit, "abcXYZ").unwrap().clone();
        let scratch =
            crate::lower::weavy::parse_prepared_weavy_tree(&plan, &parser, &table, "abcXYZ")
                .unwrap();

        rediff::assert_same!(reparsed.tree(), &scratch);
        assert!(reparsed.trace_events().is_empty());
        assert!(
            reparsed
                .tree_events()
                .iter()
                .any(|event| matches!(event, TreeEvent::ReuseNode { .. })),
            "Weavy incremental reparse should reuse at least one accepted subtree"
        );
        assert_eq!(weavy_reused_byte_ranges(&reparsed), vec![(3, 6)]);
        assert_eq!(session.last_input(), Some("abcXYZ"));
    }
    #[test]
    fn weavy_runtime_parse_session_does_not_reuse_node_that_peeked_into_edit() {
        let (validated, parser, table) = flagged_regex_fixture();
        let plan = crate::lower::weavy::WeavyParsePlan::new(&validated, &parser, &table).unwrap();
        let mut session = crate::lower::weavy::WeavyParseSession::new(&plan, &parser, &table);
        session.parse("ABCXYZ").unwrap();

        let edit = ParserInputEdit::new(3, 6, 6);
        let reparsed = session.reparse(edit, "ABCxyz").unwrap().clone();
        let scratch =
            crate::lower::weavy::parse_prepared_weavy_tree(&plan, &parser, &table, "ABCxyz")
                .unwrap();

        rediff::assert_same!(reparsed.tree(), &scratch);
        assert!(
            !reparsed
                .tree_events()
                .iter()
                .any(|event| matches!(event, TreeEvent::ReuseNode { .. })),
            "Weavy must not reuse a node that inspected the edit boundary"
        );
    }
    #[test]
    fn weavy_runtime_parse_session_reuses_suffix_across_edited_extra() {
        let (validated, parser, table) = extra_comment_reuse_fixture();
        let plan = crate::lower::weavy::WeavyParsePlan::new(&validated, &parser, &table).unwrap();
        let mut session = crate::lower::weavy::WeavyParseSession::new(&plan, &parser, &table);
        let first = session.parse("a#old\nb").unwrap().clone();
        assert_eq!(
            first.tree().to_sexp(),
            "(source_file (left) (comment) (right))"
        );

        let edit = ParserInputEdit::new(2, 5, 5);
        let reparsed = session.reparse(edit, "a#new\nb").unwrap().clone();
        let scratch =
            crate::lower::weavy::parse_prepared_weavy_tree(&plan, &parser, &table, "a#new\nb")
                .unwrap();

        rediff::assert_same!(reparsed.tree(), &scratch);
        assert_eq!(
            reparsed.tree().to_sexp(),
            "(source_file (left) (comment) (right))"
        );
        assert_eq!(weavy_reused_byte_ranges(&reparsed), vec![(0, 1), (6, 7)]);
    }
    #[test]
    fn weavy_runtime_parse_session_reuses_node_with_attached_extra() {
        let (validated, parser, table) = wrapped_extra_reuse_fixture();
        let plan = crate::lower::weavy::WeavyParsePlan::new(&validated, &parser, &table).unwrap();
        let mut session = crate::lower::weavy::WeavyParseSession::new(&plan, &parser, &table);
        let first = session.parse("a#old\nb1").unwrap().clone();
        assert_eq!(
            first.tree().to_sexp(),
            "(source_file (wrapper (left) (comment) (right)) (suffix))"
        );

        let edit = ParserInputEdit::new(7, 8, 8);
        let reparsed = session.reparse(edit, "a#old\nb2").unwrap().clone();
        let scratch =
            crate::lower::weavy::parse_prepared_weavy_tree(&plan, &parser, &table, "a#old\nb2")
                .unwrap();

        rediff::assert_same!(reparsed.tree(), &scratch);
        assert_eq!(
            reparsed.tree().to_sexp(),
            "(source_file (wrapper (left) (comment) (right)) (suffix))"
        );
        assert_eq!(weavy_reused_byte_ranges(&reparsed), vec![(0, 7)]);
    }
    #[test]
    fn weavy_runtime_parse_session_does_not_reuse_error_containing_node() {
        let (validated, parser, table) = wrapped_extra_reuse_fixture();
        let plan = crate::lower::weavy::WeavyParsePlan::new(&validated, &parser, &table).unwrap();
        let mut session = crate::lower::weavy::WeavyParseSession::new(&plan, &parser, &table);
        let first = session.parse_recovering("a@\nb1").unwrap().clone();
        assert_eq!(
            first.tree().to_sexp(),
            "(source_file (wrapper (left) (ERROR) (right)) (suffix))"
        );

        let edit = ParserInputEdit::new(4, 5, 5);
        let reparsed = session.reparse_recovering(edit, "a@\nb2").unwrap().clone();
        let scratch = crate::lower::weavy::parse_prepared_weavy_recovering_with_report_and_scanner(
            &plan, &parser, &table, "a@\nb2", None,
        )
        .unwrap();

        rediff::assert_same!(reparsed.tree(), scratch.tree());
        assert!(
            !weavy_reused_byte_ranges(&reparsed).contains(&(0, 4)),
            "wrapper node contains ERROR and must not be reused"
        );
    }
    #[test]
    fn weavy_runtime_recovery_matches_skip_invalid_input_shape() {
        let (validated, parser, table) = wrapped_extra_reuse_fixture();
        let input = "a@\nb1";
        let plan = crate::lower::weavy::WeavyParsePlan::new(&validated, &parser, &table).unwrap();
        let weavy_report =
            crate::lower::weavy::parse_prepared_weavy_recovering_with_report_and_scanner(
                &plan, &parser, &table, input, None,
            )
            .unwrap();

        assert_eq!(
            weavy_report.tree().to_sexp(),
            "(source_file (wrapper (left) (ERROR) (right)) (suffix))"
        );
        assert_eq!(weavy_report.accepted_count(), 1);
        assert_eq!(weavy_report.failure_count(), 0);
        assert!(
            weavy_report
                .accepted_tree_events()
                .iter()
                .any(|event| matches!(event, TreeEvent::Error { .. })),
            "recovering Weavy parse should emit an ERROR tree event"
        );
    }
    #[test]
    fn weavy_runtime_parse_session_does_not_reuse_root_across_boundary_insertion() {
        let (validated, parser, table) = repeated_word_fixture();
        let plan = crate::lower::weavy::WeavyParsePlan::new(&validated, &parser, &table).unwrap();
        let mut session = crate::lower::weavy::WeavyParseSession::new(&plan, &parser, &table);
        let first = session.parse("alpha").unwrap().clone();
        assert_eq!(first.tree().to_sexp(), "(source_file (word))");

        let edit = ParserInputEdit::new(5, 5, 10);
        let reparsed = session.reparse(edit, "alpha beta").unwrap().clone();
        let scratch =
            crate::lower::weavy::parse_prepared_weavy_tree(&plan, &parser, &table, "alpha beta")
                .unwrap();

        rediff::assert_same!(reparsed.tree(), &scratch);
        assert_eq!(reparsed.tree().to_sexp(), "(source_file (word) (word))");
        assert!(weavy_reused_byte_ranges(&reparsed).is_empty());
    }
    #[test]
    fn weavy_runtime_parse_session_rejects_mismatched_edit_context() {
        let (validated, parser, table) = flagged_regex_fixture();
        let plan = crate::lower::weavy::WeavyParsePlan::new(&validated, &parser, &table).unwrap();
        let mut session = crate::lower::weavy::WeavyParseSession::new(&plan, &parser, &table);
        session.parse("ABCXYZ").unwrap();

        let error = session
            .reparse(ParserInputEdit::new(0, 3, 3), "abcXYZZ")
            .unwrap_err();
        assert!(matches!(
            error,
            crate::lower::weavy::WeavyParseError::InvalidInputEdit { .. }
        ));
        assert_eq!(session.last_input(), Some("ABCXYZ"));
    }
    #[test]
    fn weavy_runtime_preserves_regex_flags() {
        let (validated, parser, table) = flagged_regex_fixture();
        let input = "ABCXYZ";
        let plan = crate::lower::weavy::WeavyParsePlan::new(&validated, &parser, &table).unwrap();
        let tree =
            crate::lower::weavy::parse_prepared_weavy_tree(&plan, &parser, &table, input).unwrap();

        assert_eq!(tree.to_sexp(), "(source_file (insensitive) (wrapped))");
    }

    fn lexical_symbol_fixture() -> (ValidatedGrammar, ParserGrammar, ParseTable) {
        prepared_with_validated(
            r##"{
              "name": "mini",
              "rules": {
                "source_file": {
                  "type": "TOKEN",
                  "content": {
                    "type": "SEQ",
                    "members": [
                      { "type": "SYMBOL", "name": "word" },
                      { "type": "STRING", "value": ":" },
                      { "type": "SYMBOL", "name": "digits" }
                    ]
                  }
                },
                "word": { "type": "PATTERN", "value": "[a-z]+" },
                "digits": {
                  "type": "REPEAT1",
                  "content": { "type": "SYMBOL", "name": "digit" }
                },
                "digit": { "type": "PATTERN", "value": "\\d" }
              }
            }"##,
        )
    }
    #[test]
    fn weavy_runtime_resolves_symbol_references_inside_token() {
        let (validated, parser, table) = lexical_symbol_fixture();
        let input = "alpha:123";
        let plan = crate::lower::weavy::WeavyParsePlan::new(&validated, &parser, &table).unwrap();
        let tree =
            crate::lower::weavy::parse_prepared_weavy_tree(&plan, &parser, &table, input).unwrap();

        assert_eq!(tree.to_sexp(), "(source_file)");
    }

    #[test]
    fn lex_compiler_carries_precedence_inside_token_wrapper() {
        let (validated, parser, _table) = prepared_with_validated(
            r##"{
              "name": "mini",
              "rules": {
                "source_file": { "type": "SYMBOL", "name": "context" },
                "context": {
                  "type": "TOKEN",
                  "content": {
                    "type": "PREC",
                    "value": -1,
                    "content": { "type": "SYMBOL", "name": "context_body" }
                  }
                },
                "context_body": { "type": "PATTERN", "value": "[^\\n]+" }
              }
            }"##,
        );

        let compiled = parser
            .symbols
            .terminals()
            .iter()
            .map(|terminal| compile_lex_terminal(&validated, terminal))
            .find(|terminal| terminal.lexical_precedence == -1)
            .unwrap();

        assert_eq!(compiled.lexical_precedence, -1);
    }

    #[test]
    fn lex_compiler_carries_implicit_precedence_through_symbol_reference() {
        let (validated, parser, _table) = prepared_with_validated(
            r##"{
              "name": "mini",
              "rules": {
                "source_file": { "type": "SYMBOL", "name": "context" },
                "context": {
                  "type": "TOKEN",
                  "content": { "type": "SYMBOL", "name": "context_body" }
                },
                "context_body": { "type": "STRING", "value": "x" }
              }
            }"##,
        );

        let compiled = parser
            .symbols
            .terminals()
            .iter()
            .map(|terminal| compile_lex_terminal(&validated, terminal))
            .find(|terminal| terminal.implicit_precedence == 2)
            .unwrap();

        assert_eq!(compiled.implicit_precedence, 2);
    }

    fn lexical_primitives_fixture() -> (ValidatedGrammar, ParserGrammar, ParseTable) {
        prepared_with_validated(
            r##"{
              "name": "mini",
              "rules": {
                "source_file": {
                  "type": "SEQ",
                  "members": [
                    { "type": "SYMBOL", "name": "text" },
                    { "type": "SYMBOL", "name": "comment" }
                  ]
                },
                "text": {
                  "type": "TOKEN",
                  "content": {
                    "type": "UNTIL",
                    "markers": ["{{", "{#"]
                  }
                },
                "comment": {
                  "type": "TOKEN",
                  "content": {
                    "type": "NESTED",
                    "open": "{#",
                    "close": "#}"
                  }
                }
              }
            }"##,
        )
    }

    fn until_reuse_fixture() -> (ValidatedGrammar, ParserGrammar, ParseTable) {
        prepared_with_validated(
            r##"{
              "name": "mini",
              "rules": {
                "source_file": {
                  "type": "SEQ",
                  "members": [
                    { "type": "SYMBOL", "name": "text" },
                    { "type": "SYMBOL", "name": "interpolation" },
                    { "type": "SYMBOL", "name": "text" }
                  ]
                },
                "text": {
                  "type": "TOKEN",
                  "content": {
                    "type": "UNTIL",
                    "markers": ["{{"]
                  }
                },
                "interpolation": {
                  "type": "SEQ",
                  "members": [
                    { "type": "STRING", "value": "{{" },
                    { "type": "SYMBOL", "name": "word" },
                    { "type": "STRING", "value": "}}" }
                  ]
                },
                "word": {
                  "type": "PATTERN",
                  "value": "[a-z]+"
                }
              }
            }"##,
        )
    }

    #[test]
    fn weavy_lexical_primitives_match_until_and_nested_tokens() {
        let (validated, parser, table) = lexical_primitives_fixture();
        let plan = crate::lower::weavy::WeavyParsePlan::new(&validated, &parser, &table).unwrap();
        let input = "hello {# outer {# inner #} done #}";
        let byte_positions = [0usize, 6, 15, 26];

        let observed = parser
            .symbols
            .terminals()
            .iter()
            .map(|terminal| {
                Ok((
                    terminal.kind(),
                    terminal.spelling().to_owned(),
                    weavy_terminal_ends(&plan, terminal, input, &byte_positions)?,
                ))
            })
            .collect::<Result<Vec<_>, crate::lower::weavy::WeavyParseError>>()
            .unwrap();

        assert!(
            observed
                .iter()
                .any(|(kind, _, ends)| *kind == ParserTerminalKind::Token
                    && *ends == vec![Some(6), None, None, Some(34)]),
            "observed: {observed:#?}"
        );
        assert!(
            observed
                .iter()
                .any(|(kind, _, ends)| *kind == ParserTerminalKind::Token
                    && *ends == vec![None, Some(34), Some(26), None]),
            "observed: {observed:#?}"
        );
    }
    #[test]
    fn weavy_runtime_parse_session_reuses_until_text_around_interpolation_edit() {
        let (validated, parser, table) = until_reuse_fixture();
        let plan = crate::lower::weavy::WeavyParsePlan::new(&validated, &parser, &table).unwrap();
        let mut session = crate::lower::weavy::WeavyParseSession::new(&plan, &parser, &table);
        let first = session.parse("hello {{name}} tail").unwrap().clone();
        assert_eq!(
            first.tree().to_sexp(),
            "(source_file (text) (interpolation (word)) (text))"
        );

        let edit = ParserInputEdit::new(8, 12, 13);
        let reparsed = session
            .reparse(edit, "hello {{title}} tail")
            .unwrap()
            .clone();
        let scratch = crate::lower::weavy::parse_prepared_weavy_tree(
            &plan,
            &parser,
            &table,
            "hello {{title}} tail",
        )
        .unwrap();

        rediff::assert_same!(reparsed.tree(), &scratch);
        assert_eq!(
            reparsed.tree().to_sexp(),
            "(source_file (text) (interpolation (word)) (text))"
        );
        assert_eq!(weavy_reused_byte_ranges(&reparsed), vec![(0, 6), (15, 20)]);
    }
    #[test]
    fn weavy_runtime_parse_session_reuse_metadata_survives_reparse_chains() {
        let (validated, parser, table) = until_reuse_fixture();
        let plan = crate::lower::weavy::WeavyParsePlan::new(&validated, &parser, &table).unwrap();
        let mut session = crate::lower::weavy::WeavyParseSession::new(&plan, &parser, &table);
        session.parse("hello {{name}} tail").unwrap();

        let first_reparse = session
            .reparse(ParserInputEdit::new(8, 12, 13), "hello {{title}} tail")
            .unwrap()
            .clone();
        assert_eq!(
            weavy_reused_byte_ranges(&first_reparse),
            vec![(0, 6), (15, 20)]
        );

        let second_reparse = session
            .reparse(ParserInputEdit::new(16, 20, 19), "hello {{title}} end")
            .unwrap()
            .clone();
        let scratch = crate::lower::weavy::parse_prepared_weavy_tree(
            &plan,
            &parser,
            &table,
            "hello {{title}} end",
        )
        .unwrap();

        rediff::assert_same!(second_reparse.tree(), &scratch);
        assert_eq!(
            second_reparse.tree().to_sexp(),
            "(source_file (text) (interpolation (word)) (text))"
        );
        assert!(
            !weavy_reused_byte_ranges(&second_reparse).is_empty(),
            "second reparse should consume reusable metadata produced by the first reparse"
        );

        let (validated, parser, table) = wrapped_extra_reuse_fixture();
        let plan = crate::lower::weavy::WeavyParsePlan::new(&validated, &parser, &table).unwrap();
        let mut session = crate::lower::weavy::WeavyParseSession::new(&plan, &parser, &table);
        session.parse("a#old\nb1").unwrap();

        let first_reparse = session
            .reparse(ParserInputEdit::new(7, 8, 8), "a#old\nb2")
            .unwrap()
            .clone();
        assert_eq!(weavy_reused_byte_ranges(&first_reparse), vec![(0, 7)]);

        let second_reparse = session
            .reparse(ParserInputEdit::new(2, 5, 5), "a#new\nb2")
            .unwrap()
            .clone();
        let scratch =
            crate::lower::weavy::parse_prepared_weavy_tree(&plan, &parser, &table, "a#new\nb2")
                .unwrap();

        rediff::assert_same!(second_reparse.tree(), &scratch);
        assert_eq!(
            second_reparse.tree().to_sexp(),
            "(source_file (wrapper (left) (comment) (right)) (suffix))"
        );
    }
    #[test]
    fn lexical_primitives_parse_through_weavy_runtime() {
        let (validated, parser, table) = lexical_primitives_fixture();
        let input = "hello {# outer {# inner #} done #}";
        let plan = crate::lower::weavy::WeavyParsePlan::new(&validated, &parser, &table).unwrap();
        let tree =
            crate::lower::weavy::parse_prepared_weavy_tree(&plan, &parser, &table, input).unwrap();

        assert_eq!(tree.to_sexp(), "(source_file (text) (comment))");
    }

    #[test]
    fn parse_table_retains_generated_action_conflicts_for_glr() {
        let grammar = prepared(
            r##"{
              "name": "mini",
              "rules": {
                "source_file": {
                  "type": "CHOICE",
                  "members": [
                    { "type": "SYMBOL", "name": "left" },
                    { "type": "SYMBOL", "name": "right" }
                  ]
                },
                "left": { "type": "SYMBOL", "name": "token" },
                "right": { "type": "SYMBOL", "name": "token" },
                "token": { "type": "STRING", "value": "x" }
              }
            }"##,
        );

        let table = ParseTable::from_grammar(&grammar).unwrap();

        assert_eq!(table.conflicts().len(), 1);
        let conflict = &table.conflicts()[0];
        assert_eq!(conflict.lookahead(), LookaheadSymbol::Eof);
        assert_eq!(conflict.actions().len(), 2);
        assert!(
            conflict
                .actions()
                .iter()
                .all(|action| matches!(action, ParseAction::Reduce { .. }))
        );
    }

    fn precedence_fixture() -> ParserGrammar {
        prepared(
            r##"{
              "name": "mini",
              "rules": {
                "source_file": {
                  "type": "CHOICE",
                  "members": [
                    { "type": "SYMBOL", "name": "tight_left" },
                    { "type": "SYMBOL", "name": "loose_left" },
                    { "type": "SYMBOL", "name": "integer_high" },
                    { "type": "SYMBOL", "name": "integer_mid" },
                    { "type": "SYMBOL", "name": "integer_low" },
                    { "type": "SYMBOL", "name": "unlisted_left" }
                  ]
                },
                "tight_left": {
                  "type": "PREC_LEFT",
                  "value": "tight",
                  "content": { "type": "STRING", "value": "x" }
                },
                "loose_left": {
                  "type": "PREC_LEFT",
                  "value": "loose",
                  "content": { "type": "STRING", "value": "x" }
                },
                "integer_high": {
                  "type": "PREC_LEFT",
                  "value": 2,
                  "content": { "type": "STRING", "value": "x" }
                },
                "integer_mid": {
                  "type": "PREC_LEFT",
                  "value": 1,
                  "content": { "type": "STRING", "value": "x" }
                },
                "integer_low": {
                  "type": "PREC_LEFT",
                  "value": -1,
                  "content": { "type": "STRING", "value": "x" }
                },
                "unlisted_left": {
                  "type": "PREC_LEFT",
                  "value": "unlisted",
                  "content": { "type": "STRING", "value": "x" }
                }
              },
              "precedences": [
                ["tight", "loose"]
              ]
            }"##,
        )
    }

    fn metadata_id_with(
        grammar: &ParserGrammar,
        precedence: &StaticPrecedence,
    ) -> ProductionMetadataId {
        grammar
            .production_metadata()
            .iter()
            .find(|metadata| metadata.static_precedence() == Some(precedence))
            .map(ProductionMetadata::id)
            .unwrap()
    }

    fn reduce_action(metadata: ProductionMetadataId) -> ParseAction {
        ParseAction::Reduce {
            production: ProductionId::from_index(0),
            metadata,
            symbol: NonterminalId::from_index(0),
            child_count: 1,
            dynamic_precedence: 0,
        }
    }

    #[test]
    fn named_precedence_groups_treat_earlier_entries_as_stronger() {
        let grammar = precedence_fixture();

        assert_eq!(
            compare_static_precedence(
                &grammar,
                &StaticPrecedence::Named("tight".to_owned()),
                &StaticPrecedence::Named("loose".to_owned())
            ),
            std::cmp::Ordering::Greater
        );
        assert_eq!(
            compare_static_precedence(
                &grammar,
                &StaticPrecedence::Named("loose".to_owned()),
                &StaticPrecedence::Named("tight".to_owned())
            ),
            std::cmp::Ordering::Less
        );
    }

    #[test]
    fn static_conflict_resolution_treats_missing_integer_precedence_as_zero() {
        let grammar = precedence_fixture();
        let low = metadata_id_with(&grammar, &StaticPrecedence::Integer(-1));
        let high = metadata_id_with(&grammar, &StaticPrecedence::Integer(1));
        let lookahead = LookaheadSymbol::Eof;

        let mut entries = BTreeMap::from([(
            lookahead,
            vec![
                ParseAction::Shift {
                    state: ParseStateId::from_index(1),
                    repetition: false,
                },
                reduce_action(low),
            ],
        )]);
        resolve_static_conflicts(
            &mut entries,
            &BTreeMap::from([(lookahead, vec![None])]),
            &grammar,
        );
        assert!(matches!(
            entries[&lookahead].as_slice(),
            [ParseAction::Shift { .. }]
        ));

        let mut entries = BTreeMap::from([(
            lookahead,
            vec![
                ParseAction::Shift {
                    state: ParseStateId::from_index(1),
                    repetition: false,
                },
                reduce_action(high),
            ],
        )]);
        resolve_static_conflicts(
            &mut entries,
            &BTreeMap::from([(lookahead, vec![None])]),
            &grammar,
        );
        assert_eq!(entries[&lookahead], vec![reduce_action(high)]);
    }

    #[test]
    fn static_conflict_resolution_keeps_mixed_shift_precedence_cells_unresolved() {
        let grammar = precedence_fixture();
        let mid = metadata_id_with(&grammar, &StaticPrecedence::Integer(1));
        let lookahead = LookaheadSymbol::Eof;
        let mut entries = BTreeMap::from([(
            lookahead,
            vec![
                ParseAction::Shift {
                    state: ParseStateId::from_index(1),
                    repetition: false,
                },
                reduce_action(mid),
            ],
        )]);
        let shifts = BTreeMap::from([(
            lookahead,
            vec![
                Some(StaticPrecedence::Integer(2)),
                Some(StaticPrecedence::Integer(0)),
            ],
        )]);

        resolve_static_conflicts(&mut entries, &shifts, &grammar);

        assert_eq!(entries[&lookahead].len(), 2);
        assert!(
            entries[&lookahead]
                .iter()
                .any(|action| matches!(action, ParseAction::Shift { .. }))
        );
        assert!(entries[&lookahead].contains(&reduce_action(mid)));
    }

    #[test]
    fn static_conflict_resolution_uses_associativity_for_unordered_named_ties() {
        let grammar = precedence_fixture();
        let unlisted = metadata_id_with(&grammar, &StaticPrecedence::Named("unlisted".to_owned()));
        let lookahead = LookaheadSymbol::Eof;
        let mut entries = BTreeMap::from([(
            lookahead,
            vec![
                ParseAction::Shift {
                    state: ParseStateId::from_index(1),
                    repetition: false,
                },
                reduce_action(unlisted),
            ],
        )]);
        let shifts = BTreeMap::from([(
            lookahead,
            vec![Some(StaticPrecedence::Named("other".to_owned()))],
        )]);

        resolve_static_conflicts(&mut entries, &shifts, &grammar);

        assert_eq!(entries[&lookahead], vec![reduce_action(unlisted)]);
    }

    #[test]
    fn static_conflict_resolution_prunes_lower_precedence_reduce_reduce_actions() {
        let grammar = precedence_fixture();
        let low = metadata_id_with(&grammar, &StaticPrecedence::Integer(-1));
        let high = metadata_id_with(&grammar, &StaticPrecedence::Integer(2));
        let lookahead = LookaheadSymbol::Eof;
        let mut entries =
            BTreeMap::from([(lookahead, vec![reduce_action(low), reduce_action(high)])]);

        resolve_static_conflicts(&mut entries, &BTreeMap::new(), &grammar);

        assert_eq!(entries[&lookahead], vec![reduce_action(high)]);
    }

    #[test]
    fn rejects_recursive_inline_before_item_generation() {
        let grammar = normalize(
            r##"{
              "name": "mini",
              "rules": {
                "source_file": { "type": "SYMBOL", "name": "left" },
                "left": { "type": "SYMBOL", "name": "right" },
                "right": { "type": "SYMBOL", "name": "left" }
              },
              "inline": ["left", "right"]
            }"##,
        );

        let error = grammar.prepare_productions_for_items().unwrap_err();

        assert!(matches!(
            error.kind(),
            ParserPrepareErrorKind::RecursiveInline { .. }
        ));
    }

    #[test]
    fn rejects_nonproductive_reachable_nonterminals_before_item_generation() {
        let grammar = normalize(
            r##"{
              "name": "mini",
              "rules": {
                "source_file": { "type": "SYMBOL", "name": "left" },
                "left": { "type": "SYMBOL", "name": "right" },
                "right": { "type": "SYMBOL", "name": "left" }
              }
            }"##,
        );

        let error = grammar.prepare_productions_for_items().unwrap_err();

        assert!(matches!(
            error.kind(),
            ParserPrepareErrorKind::NonproductiveNonterminal { .. }
        ));
    }

    #[test]
    fn rejects_used_nullable_non_start_syntax_nonterminals_before_item_generation() {
        let grammar = normalize(
            r##"{
              "name": "mini",
              "rules": {
                "source_file": { "type": "SYMBOL", "name": "maybe" },
                "maybe": {
                  "type": "CHOICE",
                  "members": [
                    { "type": "BLANK" },
                    { "type": "STRING", "value": "x" }
                  ]
                }
              }
            }"##,
        );

        let error = grammar.prepare_productions_for_items().unwrap_err();

        assert!(matches!(
            error.kind(),
            ParserPrepareErrorKind::NullableUsedNonterminal { .. }
        ));
    }
}
