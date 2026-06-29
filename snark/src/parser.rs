//! Tree-sitter-style parser generator and LR/GLR runtime scaffolding.
//!
//! This module is the final-shape parser lane. It is deliberately table- and
//! runtime-oriented: validated grammar facts become normalized productions,
//! lexical modes, LR actions, GLR metadata, tree plans, and traceable runtime
//! state. It is not a recursive recognizer and it never consumes generated
//! Tree-sitter implementation files.

#[cfg(any(test, feature = "weavy-lowering"))]
use std::sync::{Mutex, OnceLock};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet, HashMap, VecDeque},
    error::Error,
    fmt,
};

use regex::{Regex, RegexSet};

use crate::{
    corpus::{SexpChild, SexpNode, SexpValue},
    lexical::{LexicalFacts, TerminalKind},
    runtime_input::{ByteOffset, ByteRange, PointBytes, PointRange, Row, Utf8ColumnBytes},
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
id_type!(ReducedBranchId, "Reduced parser branch id.");
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
    /// Grammar expressions have been flattened into production-shaped facts.
    ProductionsPrepared,
    /// Productions are ready for LR item-set generation.
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
    public_literal_terminals: Vec<PublicLiteralTerminals>,
    item_preparation: Option<ItemPreparationFacts>,
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
                if let ParserSymbol::Nonterminal(child) = step.symbol {
                    if inline.get(child.get() as usize).copied().unwrap_or(false) {
                        self.visit_inline_rule(child, inline, visit)?;
                    }
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
        .filter_map(|(index, flag)| flag.then(|| NonterminalId::from_index(index)))
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
        };
        let flags = normalized_regex_flags(terminal.flags.as_deref());
        let key = (kind, terminal.spelling.clone(), flags.clone());
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
            TerminalKind::Pattern => Vec::new(),
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
            | GrammarExpr::Nested { .. } => {
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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

    fn merge(&mut self, symbols: &[LookaheadSymbol]) -> bool {
        let old_len = self.symbols.len();
        self.symbols.extend_from_slice(symbols);
        self.symbols = sorted_lookaheads(std::mem::take(&mut self.symbols));
        self.symbols.len() != old_len
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
        Ok(Self {
            grammar,
            productions_by_lhs,
            first,
        })
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

    fn item_sets(&self) -> (Vec<ItemSet>, Vec<ItemTransition>) {
        let mut item_sets = Vec::new();
        let mut item_set_keys = BTreeMap::<ItemSetKey, ItemSetId>::new();
        let mut transitions = Vec::new();
        let mut queue = VecDeque::new();
        let mut start_items = ItemMap::default();
        for production in &self.productions_by_lhs[self.grammar.start.get() as usize] {
            start_items.insert(*production, 0, &[LookaheadSymbol::Eof]);
        }
        let start_items = self.closure(start_items.into_items());
        let start = push_item_set(&mut item_sets, &mut item_set_keys, start_items, &mut queue);
        debug_assert_eq!(start.get(), 0);

        while let Some(from) = queue.pop_front() {
            let grouped = self.group_goto_items(&item_sets[from.get() as usize]);
            for (symbol, items) in grouped {
                let target_items = self.closure(items);
                let to =
                    push_item_set(&mut item_sets, &mut item_set_keys, target_items, &mut queue);
                transitions.push(ItemTransition { from, symbol, to });
            }
        }

        (item_sets, transitions)
    }

    fn closure(&self, items: Vec<LrItem>) -> Vec<LrItem> {
        let mut map = ItemMap::from_items(items);
        loop {
            let snapshot = map.clone().into_items();
            let mut changed = false;
            for item in snapshot {
                let production = &self.grammar.productions[item.production.get() as usize];
                let Some(step) = production.steps.get(item.dot) else {
                    continue;
                };
                let ParserSymbol::Nonterminal(nonterminal) = step.symbol else {
                    continue;
                };
                let lookaheads = self.lookahead_after_dot(production, item.dot, &item.lookahead);
                for production in &self.productions_by_lhs[nonterminal.get() as usize] {
                    changed |= map.insert(*production, 0, &lookaheads);
                }
            }
            if !changed {
                return map.into_items();
            }
        }
    }

    fn lookahead_after_dot(
        &self,
        production: &Production,
        dot: usize,
        current: &LookaheadSet,
    ) -> Vec<LookaheadSymbol> {
        self.first
            .first_of_steps(&production.steps[dot + 1..], current.symbols())
    }

    fn group_goto_items(&self, item_set: &ItemSet) -> BTreeMap<ParserSymbol, Vec<LrItem>> {
        let mut grouped = BTreeMap::<ParserSymbol, ItemMap>::new();
        for item in &item_set.items {
            let production = &self.grammar.productions[item.production.get() as usize];
            let Some(step) = production.steps.get(item.dot) else {
                continue;
            };
            grouped.entry(step.symbol).or_default().insert(
                item.production,
                item.dot + 1,
                item.lookahead.symbols(),
            );
        }
        grouped
            .into_iter()
            .map(|(symbol, items)| (symbol, items.into_items()))
            .collect()
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
                    ParserSymbol::Internal(internal) => push_action(
                        &mut entries,
                        LookaheadSymbol::ErrorRecovery(internal),
                        ParseAction::Recover,
                    ),
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
            if step.symbol == symbol {
                if let Some(lookahead) = lookahead_for_step(step) {
                    lookaheads.push((lookahead, step.static_precedence.clone()));
                }
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

type ItemSetKey = Vec<(ProductionId, usize, Vec<LookaheadSymbol>)>;

#[derive(Debug, Clone, Default)]
struct ItemMap {
    items: BTreeMap<(ProductionId, usize), LookaheadSet>,
}

impl ItemMap {
    fn from_items(items: Vec<LrItem>) -> Self {
        let mut map = Self::default();
        for item in items {
            map.insert(item.production, item.dot, item.lookahead.symbols());
        }
        map
    }

    fn insert(
        &mut self,
        production: ProductionId,
        dot: usize,
        lookaheads: &[LookaheadSymbol],
    ) -> bool {
        self.items
            .entry((production, dot))
            .or_default()
            .merge(lookaheads)
    }

    fn into_items(self) -> Vec<LrItem> {
        self.items
            .into_iter()
            .map(|((production, dot), lookahead)| LrItem {
                production,
                dot,
                lookahead,
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
struct FirstFacts {
    nullable: Vec<bool>,
    first: Vec<Vec<LookaheadSymbol>>,
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
                return Self { nullable, first };
            }
        }
    }

    fn first_of_steps(
        &self,
        steps: &[ProductionStep],
        fallback: &[LookaheadSymbol],
    ) -> Vec<LookaheadSymbol> {
        first_of_steps_with_tables(steps, &self.nullable, &self.first, fallback)
    }
}

fn productions_by_lhs(grammar: &ParserGrammar) -> Vec<Vec<ProductionId>> {
    let mut by_lhs = vec![Vec::new(); grammar.symbols.nonterminals.len()];
    for production in &grammar.productions {
        by_lhs[production.lhs.get() as usize].push(production.id);
    }
    by_lhs
}

fn first_of_steps_with_tables(
    steps: &[ProductionStep],
    nullable: &[bool],
    first: &[Vec<LookaheadSymbol>],
    fallback: &[LookaheadSymbol],
) -> Vec<LookaheadSymbol> {
    let mut out = Vec::new();
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
                return sorted_lookaheads(out);
            }
            None => {
                let ParserSymbol::Nonterminal(nonterminal) = step.symbol else {
                    unreachable!("non-lookahead parser symbol should be nonterminal");
                };
                extend_lookaheads(&mut out, &first[nonterminal.get() as usize]);
                if !nullable[nonterminal.get() as usize] {
                    return sorted_lookaheads(out);
                }
            }
        }
    }
    extend_lookaheads(&mut out, fallback);
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

fn push_item_set(
    item_sets: &mut Vec<ItemSet>,
    item_set_keys: &mut BTreeMap<ItemSetKey, ItemSetId>,
    items: Vec<LrItem>,
    queue: &mut VecDeque<ItemSetId>,
) -> ItemSetId {
    let key = item_set_key(&items);
    if let Some(id) = item_set_keys.get(&key).copied() {
        return id;
    }
    let id = ItemSetId::from_index(item_sets.len());
    item_set_keys.insert(key, id);
    item_sets.push(ItemSet { id, items });
    queue.push_back(id);
    id
}

fn item_set_key(items: &[LrItem]) -> ItemSetKey {
    items
        .iter()
        .map(|item| (item.production, item.dot, item.lookahead.symbols.clone()))
        .collect()
}

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

/// Reduced named-node parser used to drive corpus S-expression oracles.
///
/// This is the first executable LR slice: it consumes Snark-generated parser
/// tables and emits the named-node subset of Tree-sitter corpus S-expressions
/// currently supported by this reduced oracle. It is not the final recoverable,
/// incremental, field/atom-complete GLR runtime.
pub struct ReducedParser<'a> {
    #[cfg(test)]
    grammar: &'a ValidatedGrammar,
    parser: &'a ParserGrammar,
    table: &'a ParseTable,
    external_scanner: Option<&'a dyn ReducedExternalScanner>,
    runtime_plan: RuntimePlanRef<'a>,
    #[cfg(test)]
    regex_patterns: HashMap<(String, Option<String>), Option<Regex>>,
    lex_cache: RefCell<HashMap<LexCacheKey, Result<ReducedToken, ReducedParseError>>>,
    lex_mode_cache:
        RefCell<HashMap<LexModeCacheKey, Result<Vec<ReducedTokenCandidate>, ReducedParseError>>>,
}

#[derive(Debug)]
enum RuntimePlanRef<'a> {
    Owned(RuntimeParserPlan),
    Borrowed(&'a RuntimeParserPlan),
}

impl RuntimePlanRef<'_> {
    fn get(&self) -> &RuntimeParserPlan {
        match self {
            Self::Owned(plan) => plan,
            Self::Borrowed(plan) => plan,
        }
    }
}

/// Reusable runtime parser setup for a prepared grammar/table.
///
/// This carries the grammar-derived lexer automata and first-set facts that are
/// expensive to rebuild for every parse. It is intentionally tied to the
/// grammar/table pair used to construct it; callers must keep those inputs
/// stable while reusing the plan.
#[derive(Debug)]
pub struct RuntimeParserPlan {
    first: Option<FirstFacts>,
    compiled_lex_modes: Vec<CompiledLexMode>,
}

impl RuntimeParserPlan {
    /// Build a reusable runtime plan over validated grammar facts and a parse table.
    pub fn new(
        grammar: &ValidatedGrammar,
        parser: &ParserGrammar,
        table: &ParseTable,
    ) -> Result<Self, ReducedParseError> {
        if parser.stage() != ParserGenerationStage::Productions {
            return Err(ReducedParseError::new(ReducedParseErrorKind::WrongStage {
                stage: parser.stage(),
            }));
        }
        let first = parser
            .item_preparation
            .as_ref()
            .map(|item_preparation| FirstFacts::new(parser, item_preparation.graph()));
        let compiled_lex_modes = compile_lex_modes(grammar, parser, table);
        Ok(Self {
            first,
            compiled_lex_modes,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct LexCacheKey {
    state: ParseStateId,
    byte_position: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct LexModeCacheKey {
    mode: LexModeId,
    byte_position: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct CompiledLexMode {
    pub(crate) terminals: Vec<CompiledLexTerminal>,
    pub(crate) direct_pattern_set: Option<CompiledLexPatternSet>,
}

#[derive(Debug, Clone)]
pub(crate) struct CompiledLexTerminal {
    pub(crate) terminal: TerminalId,
    pub(crate) matcher: CompiledTerminalMatcher,
    pub(crate) immediate: bool,
    pub(crate) literal: bool,
    pub(crate) lexical_precedence: i32,
    pub(crate) implicit_precedence: i32,
    pub(crate) direct_pattern_index: Option<usize>,
}

#[derive(Debug, Clone)]
pub(crate) struct CompiledLexPatternSet {
    pub(crate) regex_set: RegexSet,
    pub(crate) terminal_indices: Vec<usize>,
}

#[derive(Debug, Clone)]
pub(crate) enum CompiledTerminalMatcher {
    Expr(CompiledLexExpr),
    UnsupportedTerminal {
        terminal: TerminalId,
        spelling: String,
    },
}

#[derive(Debug, Clone)]
pub(crate) enum CompiledLexExpr {
    Blank,
    String(String),
    Pattern(CompiledLexPattern),
    Until { markers: Vec<String> },
    Nested { open: String, close: String },
    Seq(Vec<CompiledLexExpr>),
    Choice(Vec<CompiledLexExpr>),
    Repeat(Box<CompiledLexExpr>),
    Repeat1(Box<CompiledLexExpr>),
    UnsupportedSymbol(GrammarExprId),
}

#[derive(Debug, Clone)]
pub(crate) struct CompiledLexPattern {
    pub(crate) source: String,
    flags: Option<String>,
    regex: Option<Regex>,
}

/// External scanner host used by the reduced parser oracle.
pub trait ReducedExternalScanner {
    /// Try to scan one external token for a branch-local parser state.
    fn scan(
        &self,
        request: ReducedExternalScan<'_>,
    ) -> Result<Option<ReducedExternalScanResult>, ReducedParseError>;
}

/// Result of one reduced external scanner call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReducedExternalScanResult {
    end_byte: usize,
    before: Option<ScannerSnapshotId>,
    after: Option<ScannerSnapshotId>,
}

impl ReducedExternalScanResult {
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
pub struct ReducedExternalScan<'a> {
    state: ParseStateId,
    external: ExternalId,
    external_symbol: &'a ExternalSymbol,
    valid_symbols: Option<&'a ValidSymbolSet>,
    input: &'a str,
    byte_position: usize,
    scanner_snapshot: Option<ScannerSnapshotId>,
}

impl ReducedExternalScan<'_> {
    #[cfg(feature = "weavy-lowering")]
    pub(crate) const fn new<'a>(
        state: ParseStateId,
        external: ExternalId,
        external_symbol: &'a ExternalSymbol,
        valid_symbols: Option<&'a ValidSymbolSet>,
        input: &'a str,
        byte_position: usize,
        scanner_snapshot: Option<ScannerSnapshotId>,
    ) -> ReducedExternalScan<'a> {
        ReducedExternalScan {
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

/// Successful reduced parse plus branch/conflict evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReducedParseReport {
    tree: SexpNode,
    accepted_count: usize,
    failure_count: usize,
    max_live_branches: usize,
    conflict_steps: Vec<ReducedConflictStep>,
    branch_parents: Vec<ReducedBranchParent>,
    branch_results: Vec<ReducedBranchResult>,
}

impl ReducedParseReport {
    /// Reduced Tree-sitter-style S-expression tree.
    pub const fn tree(&self) -> &SexpNode {
        &self.tree
    }

    /// Number of accepted branches before identical-tree coalescing.
    pub const fn accepted_count(&self) -> usize {
        self.accepted_count
    }

    /// Number of branch failures observed while exploring the table.
    pub const fn failure_count(&self) -> usize {
        self.failure_count
    }

    /// Maximum number of queued live branches observed.
    pub const fn max_live_branches(&self) -> usize {
        self.max_live_branches
    }

    /// Multi-action table cells reached during branch execution.
    pub fn conflict_steps(&self) -> &[ReducedConflictStep] {
        &self.conflict_steps
    }

    /// Parent links for branches created by runtime forks.
    pub fn branch_parents(&self) -> &[ReducedBranchParent] {
        &self.branch_parents
    }

    /// Final accepted/failed outcomes by branch id.
    pub fn branch_results(&self) -> &[ReducedBranchResult] {
        &self.branch_results
    }

    /// Final outcomes for a branch or any descendant branch.
    pub fn branch_descendant_results(&self, branch: ReducedBranchId) -> Vec<ReducedBranchResult> {
        self.branch_results
            .iter()
            .copied()
            .filter(|result| self.branch_descends_from(result.branch, branch))
            .collect()
    }

    fn branch_descends_from(&self, mut branch: ReducedBranchId, ancestor: ReducedBranchId) -> bool {
        loop {
            if branch == ancestor {
                return true;
            }
            let Some(parent) = self
                .branch_parents
                .iter()
                .find(|link| link.branch == branch)
                .and_then(|link| link.parent)
            else {
                return false;
            };
            branch = parent;
        }
    }
}

/// One multi-action table cell reached by the reduced parser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReducedConflictStep {
    /// Branch that reached this conflict.
    pub branch: ReducedBranchId,
    /// Parse state containing the conflict.
    pub state: ParseStateId,
    /// Input byte offset before selecting the conflicted action.
    pub byte_position: usize,
    /// Lookahead that selected the conflicted action cell.
    pub lookahead: LookaheadSymbol,
    /// Actions explored from this cell.
    pub actions: Vec<ParseAction>,
    /// Outcome produced by each explored action.
    pub outcomes: Vec<ReducedConflictActionOutcome>,
}

/// Parent link for a branch created by a runtime fork.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReducedBranchParent {
    /// Child branch id.
    pub branch: ReducedBranchId,
    /// Parent branch id. The initial branch has no parent.
    pub parent: Option<ReducedBranchId>,
}

/// Final outcome for one reduced parser branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReducedBranchResult {
    /// Branch that reached a terminal outcome.
    pub branch: ReducedBranchId,
    /// Terminal branch outcome.
    pub outcome: ReducedBranchFinalOutcome,
}

/// Terminal branch outcome in the reduced parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ReducedBranchFinalOutcome {
    /// Branch accepted the input and produced the report tree.
    Accepted,
    /// Branch failed or was retired.
    Failed,
}

/// Outcome for one action in a conflicted action cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReducedConflictActionOutcome {
    /// Action selected from the conflicted cell.
    pub action: ParseAction,
    /// Immediate result of applying the action.
    pub result: ReducedConflictActionResult,
}

/// Immediate result of applying one conflicted action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ReducedConflictActionResult {
    /// Action produced a live branch for later input.
    Branch(ReducedBranchId),
    /// Action accepted immediately.
    Accepted(ReducedBranchId),
    /// Action failed immediately.
    Failed(ReducedBranchId),
}

impl<'a> ReducedParser<'a> {
    /// Build a reduced parser over validated grammar facts and generated tables.
    pub fn new(
        grammar: &'a ValidatedGrammar,
        parser: &'a ParserGrammar,
        table: &'a ParseTable,
    ) -> Result<Self, ReducedParseError> {
        let runtime_plan = RuntimeParserPlan::new(grammar, parser, table)?;
        Self::with_runtime_plan(grammar, parser, table, RuntimePlanRef::Owned(runtime_plan))
    }

    fn with_runtime_plan(
        grammar: &'a ValidatedGrammar,
        parser: &'a ParserGrammar,
        table: &'a ParseTable,
        runtime_plan: RuntimePlanRef<'a>,
    ) -> Result<Self, ReducedParseError> {
        if parser.stage() != ParserGenerationStage::Productions {
            return Err(ReducedParseError::new(ReducedParseErrorKind::WrongStage {
                stage: parser.stage(),
            }));
        }
        #[cfg(not(test))]
        let _ = grammar;
        Ok(Self {
            #[cfg(test)]
            grammar,
            parser,
            table,
            external_scanner: None,
            runtime_plan,
            #[cfg(test)]
            regex_patterns: compile_regex_patterns(grammar, parser),
            lex_cache: RefCell::new(HashMap::new()),
            lex_mode_cache: RefCell::new(HashMap::new()),
        })
    }

    /// Build a reduced parser that borrows a reusable runtime plan.
    pub fn new_with_plan(
        grammar: &'a ValidatedGrammar,
        parser: &'a ParserGrammar,
        table: &'a ParseTable,
        runtime_plan: &'a RuntimeParserPlan,
    ) -> Result<Self, ReducedParseError> {
        Self::with_runtime_plan(
            grammar,
            parser,
            table,
            RuntimePlanRef::Borrowed(runtime_plan),
        )
    }

    /// Attach a reduced external scanner host.
    pub fn with_external_scanner(mut self, scanner: &'a dyn ReducedExternalScanner) -> Self {
        self.external_scanner = Some(scanner);
        self
    }

    /// Parse one input into a reduced Tree-sitter-style S-expression node.
    pub fn parse(&self, input: &str) -> Result<SexpNode, ReducedParseError> {
        self.parse_with_report(input).map(|report| report.tree)
    }

    /// Parse one input and return branch/conflict evidence for oracle tests.
    pub fn parse_with_report(&self, input: &str) -> Result<ReducedParseReport, ReducedParseError> {
        self.clear_lex_cache();
        let mut branches = VecDeque::from([ReducedBranch {
            id: ReducedBranchId::from_index(0),
            stack: vec![ReducedStackEntry {
                state: ParseStateId::from_index(0),
                fragment: None,
                extra: false,
            }],
            byte_position: 0,
            scanner_snapshot: None,
            trace: Vec::new(),
        }]);
        let mut accepted = Vec::<(SexpNode, Vec<ReducedTraceStep>)>::new();
        let mut failures = Vec::<ReducedParseError>::new();
        let mut conflict_steps = Vec::new();
        let mut branch_parents = vec![ReducedBranchParent {
            branch: ReducedBranchId::from_index(0),
            parent: None,
        }];
        let mut branch_results = Vec::new();
        let mut next_branch_index = 1usize;
        let mut step_count = 0usize;
        let step_limit = self.reduced_step_limit(input);
        let mut max_live_branches = branches.len();

        while let Some(branch) = branches.pop_front() {
            step_count += 1;
            if step_count > step_limit {
                return Err(
                    ReducedParseError::new(ReducedParseErrorKind::BranchStepLimit {
                        limit: step_limit,
                    })
                    .with_trace(branch.trace),
                );
            }

            for outcome in self.step_branch(
                branch,
                input,
                &mut conflict_steps,
                &mut branch_parents,
                &mut next_branch_index,
            ) {
                match outcome {
                    ReducedStepOutcome::Branch(branch) => branches.push_back(branch),
                    ReducedStepOutcome::Accepted {
                        branch,
                        node,
                        trace,
                    } => {
                        branch_results.push(ReducedBranchResult {
                            branch,
                            outcome: ReducedBranchFinalOutcome::Accepted,
                        });
                        accepted.push((node, trace));
                    }
                    ReducedStepOutcome::Failed { branch, error } => {
                        branch_results.push(ReducedBranchResult {
                            branch,
                            outcome: ReducedBranchFinalOutcome::Failed,
                        });
                        failures.push(error);
                    }
                }
            }
            max_live_branches = max_live_branches.max(branches.len());
        }

        let Some((first_node, first_trace)) = accepted.first().cloned() else {
            return Err(select_reduced_failure(failures).unwrap_or_else(|| {
                ReducedParseError::new(ReducedParseErrorKind::NoViableBranch { failure_count: 0 })
            }));
        };
        if accepted.iter().all(|(node, _)| *node == first_node) {
            return Ok(ReducedParseReport {
                tree: first_node,
                accepted_count: accepted.len(),
                failure_count: failures.len(),
                max_live_branches,
                conflict_steps,
                branch_parents,
                branch_results,
            });
        }

        Err(
            ReducedParseError::new(ReducedParseErrorKind::AmbiguousParse {
                accepted_count: accepted.len(),
                accepted: accepted.iter().map(|(node, _)| node.to_sexp()).collect(),
            })
            .with_trace(first_trace),
        )
    }

    fn parse_state(&self, state: ParseStateId) -> Result<&ParseState, ReducedParseError> {
        self.table
            .states()
            .get(state.get() as usize)
            .ok_or_else(|| ReducedParseError::new(ReducedParseErrorKind::MissingState { state }))
    }

    fn clear_lex_cache(&self) {
        self.lex_cache.borrow_mut().clear();
        self.lex_mode_cache.borrow_mut().clear();
    }

    fn reduced_step_limit(&self, input: &str) -> usize {
        let input_budget = input.len().saturating_mul(4096);
        let table_budget = self.table.states().len().saturating_mul(64);
        10_000usize.max(input_budget.saturating_add(table_budget))
    }

    fn reduced_recovery_step_limit(&self, input: &str) -> usize {
        let input_budget = input.len().saturating_mul(96);
        let table_budget = self.table.states().len().saturating_mul(64);
        10_000usize
            .max(input_budget.saturating_add(table_budget))
            .min(500_000)
    }

    fn step_branch(
        &self,
        branch: ReducedBranch,
        input: &str,
        conflict_steps: &mut Vec<ReducedConflictStep>,
        branch_parents: &mut Vec<ReducedBranchParent>,
        next_branch_index: &mut usize,
    ) -> Vec<ReducedStepOutcome> {
        let source_branch = branch.id;
        let state = match branch.stack.last() {
            Some(entry) => entry.state,
            None => {
                return vec![ReducedStepOutcome::Failed {
                    branch: source_branch,
                    error: ReducedParseError::new(ReducedParseErrorKind::EmptyStack)
                        .with_trace(branch.trace),
                }];
            }
        };
        let state_row = match self.parse_state(state) {
            Ok(state_row) => state_row,
            Err(error) => {
                return vec![ReducedStepOutcome::Failed {
                    branch: source_branch,
                    error: error.with_trace(branch.trace),
                }];
            }
        };
        let token = match self.lex(
            state_row,
            input,
            branch.byte_position,
            branch.scanner_snapshot,
        ) {
            Ok(token) => token,
            Err(error) => {
                return vec![ReducedStepOutcome::Failed {
                    branch: source_branch,
                    error: error.with_trace(branch.trace),
                }];
            }
        };
        let Some(entry) = state_row
            .entries()
            .iter()
            .find(|entry| entry.lookahead() == token.lookahead)
        else {
            return vec![ReducedStepOutcome::Failed {
                branch: source_branch,
                error: ReducedParseError::new(ReducedParseErrorKind::NoAction {
                    state,
                    lookahead: token.lookahead,
                    byte_position: branch.byte_position,
                })
                .with_trace(branch.trace),
            }];
        };

        let is_conflict = entry.actions().len() > 1;
        let mut conflict_outcomes = Vec::new();
        let conflict_byte_position = branch.byte_position;

        let mut outcomes = Vec::new();
        for action in entry.actions() {
            let mut branch = branch.clone();
            if is_conflict {
                let child = ReducedBranchId::from_index(*next_branch_index);
                *next_branch_index += 1;
                branch.id = child;
                branch_parents.push(ReducedBranchParent {
                    branch: child,
                    parent: Some(source_branch),
                });
            }
            branch.trace.push(ReducedTraceStep {
                state,
                byte_position: branch.byte_position,
                lookahead: token.lookahead,
                action: *action,
            });
            let outcome = self.apply_action(branch, token, *action, input);
            if is_conflict {
                conflict_outcomes.push(ReducedConflictActionOutcome {
                    action: *action,
                    result: match &outcome {
                        ReducedStepOutcome::Branch(branch) => {
                            ReducedConflictActionResult::Branch(branch.id)
                        }
                        ReducedStepOutcome::Accepted { branch, .. } => {
                            ReducedConflictActionResult::Accepted(*branch)
                        }
                        ReducedStepOutcome::Failed { branch, .. } => {
                            ReducedConflictActionResult::Failed(*branch)
                        }
                    },
                });
            }
            outcomes.push(outcome);
        }

        if is_conflict {
            conflict_steps.push(ReducedConflictStep {
                branch: source_branch,
                state,
                byte_position: conflict_byte_position,
                lookahead: token.lookahead,
                actions: entry.actions().to_vec(),
                outcomes: conflict_outcomes,
            });
        }
        outcomes
    }

    fn apply_action(
        &self,
        mut branch: ReducedBranch,
        token: ReducedToken,
        action: ParseAction,
        input: &str,
    ) -> ReducedStepOutcome {
        match action {
            ParseAction::Shift { state, .. } => {
                branch.byte_position = token.end;
                branch.commit_scanner_snapshot(token);
                branch.stack.push(ReducedStackEntry {
                    state,
                    fragment: Some(ReducedFragment::Hidden(Vec::new())),
                    extra: false,
                });
                ReducedStepOutcome::Branch(branch)
            }
            ParseAction::ShiftExtra => {
                branch.byte_position = token.end;
                branch.commit_scanner_snapshot(token);
                if let Some(fragment) = self.extra_fragment(token.lookahead) {
                    let Some(state) = branch.stack.last().map(|entry| entry.state) else {
                        return ReducedStepOutcome::Failed {
                            branch: branch.id,
                            error: ReducedParseError::new(ReducedParseErrorKind::EmptyStack)
                                .with_trace(branch.trace),
                        };
                    };
                    branch.stack.push(ReducedStackEntry {
                        state,
                        fragment: Some(fragment),
                        extra: true,
                    });
                }
                ReducedStepOutcome::Branch(branch)
            }
            ParseAction::Reduce {
                production,
                metadata,
                symbol,
                child_count,
                ..
            } => {
                let reduction = match self.reduce_fragment(
                    production,
                    metadata,
                    child_count,
                    &mut branch.stack,
                    false,
                ) {
                    Ok(reduction) => reduction,
                    Err(error) => {
                        return ReducedStepOutcome::Failed {
                            branch: branch.id,
                            error: error.with_trace(branch.trace),
                        };
                    }
                };
                let head_state = match branch.stack.last() {
                    Some(entry) => entry.state,
                    None => {
                        return ReducedStepOutcome::Failed {
                            branch: branch.id,
                            error: ReducedParseError::new(ReducedParseErrorKind::EmptyStack)
                                .with_trace(branch.trace),
                        };
                    }
                };
                let goto_state = match self.goto_state(head_state, symbol) {
                    Ok(state) => state,
                    Err(error) => {
                        return ReducedStepOutcome::Failed {
                            branch: branch.id,
                            error: error.with_trace(branch.trace),
                        };
                    }
                };
                branch.stack.push(ReducedStackEntry {
                    state: goto_state,
                    fragment: Some(reduction.fragment),
                    extra: false,
                });
                for fragment in reduction.trailing_extras {
                    branch.stack.push(ReducedStackEntry {
                        state: goto_state,
                        fragment: Some(fragment),
                        extra: true,
                    });
                }
                ReducedStepOutcome::Branch(branch)
            }
            ParseAction::Accept {
                production,
                metadata,
                child_count,
                ..
            } => {
                if token.lookahead != LookaheadSymbol::Eof || branch.byte_position != input.len() {
                    return ReducedStepOutcome::Failed {
                        branch: branch.id,
                        error: ReducedParseError::new(ReducedParseErrorKind::TrailingInput {
                            byte_position: branch.byte_position,
                        })
                        .with_trace(branch.trace),
                    };
                }
                let reduction = match self.reduce_fragment(
                    production,
                    metadata,
                    child_count,
                    &mut branch.stack,
                    true,
                ) {
                    Ok(reduction) => reduction,
                    Err(error) => {
                        return ReducedStepOutcome::Failed {
                            branch: branch.id,
                            error: error.with_trace(branch.trace),
                        };
                    }
                };
                match reduction.fragment {
                    ReducedFragment::Node(node) => {
                        let node = match self.finish_accepted_root(node, &mut branch.stack) {
                            Ok(node) => node,
                            Err(error) => {
                                return ReducedStepOutcome::Failed {
                                    branch: branch.id,
                                    error: error.with_trace(branch.trace),
                                };
                            }
                        };
                        ReducedStepOutcome::Accepted {
                            branch: branch.id,
                            node,
                            trace: branch.trace,
                        }
                    }
                    ReducedFragment::Hidden(_) => ReducedStepOutcome::Failed {
                        branch: branch.id,
                        error: ReducedParseError::new(ReducedParseErrorKind::AcceptedHiddenRoot)
                            .with_trace(branch.trace),
                    },
                }
            }
            ParseAction::Recover => {
                let state = branch
                    .stack
                    .last()
                    .map(|entry| entry.state)
                    .unwrap_or(ParseStateId::from_index(0));
                ReducedStepOutcome::Failed {
                    branch: branch.id,
                    error: ReducedParseError::new(ReducedParseErrorKind::UnsupportedRecovery {
                        state,
                    })
                    .with_trace(branch.trace),
                }
            }
        }
    }

    fn lex(
        &self,
        state: &ParseState,
        input: &str,
        byte_position: usize,
        scanner_snapshot: Option<ScannerSnapshotId>,
    ) -> Result<ReducedToken, ReducedParseError> {
        if byte_position == input.len() {
            return Ok(ReducedToken {
                lookahead: LookaheadSymbol::Eof,
                end: byte_position,
                inspected_end: byte_position,
                scanner: None,
            });
        }
        let mode = self
            .table
            .lexical_modes()
            .get(state.lex_mode().get() as usize)
            .ok_or_else(|| {
                ReducedParseError::new(ReducedParseErrorKind::MissingLexMode {
                    mode: state.lex_mode(),
                })
            })?;
        if !mode.externals().is_empty() {
            return self.lex_uncached(state, mode, input, byte_position, scanner_snapshot);
        }
        let key = LexCacheKey {
            state: state.id(),
            byte_position,
        };
        if let Some(cached) = self.lex_cache.borrow().get(&key).cloned() {
            return cached;
        }
        let result = self.lex_uncached(state, mode, input, byte_position, scanner_snapshot);
        self.lex_cache.borrow_mut().insert(key, result.clone());
        result
    }

    fn lex_uncached(
        &self,
        state: &ParseState,
        mode: &LexMode,
        input: &str,
        byte_position: usize,
        scanner_snapshot: Option<ScannerSnapshotId>,
    ) -> Result<ReducedToken, ReducedParseError> {
        let mut best = None::<ReducedTokenCandidate>;
        let mut best_rejected = None::<ReducedTokenCandidate>;
        for candidate in self.match_terminal_candidates_for_mode(mode, input, byte_position)? {
            let terminal = match candidate.lookahead {
                LookaheadSymbol::Terminal(terminal) => terminal,
                LookaheadSymbol::ReservedWord { terminal, .. } => terminal,
                LookaheadSymbol::External(_) => {
                    push_reduced_candidate(&mut best_rejected, candidate);
                    continue;
                }
                LookaheadSymbol::Eof | LookaheadSymbol::ErrorRecovery(_) => {
                    push_reduced_candidate(&mut best_rejected, candidate);
                    continue;
                }
            };
            let Some(lookahead) = self.lookahead_for_terminal(state, terminal) else {
                push_reduced_candidate(&mut best_rejected, candidate);
                continue;
            };
            push_reduced_candidate(
                &mut best,
                ReducedTokenCandidate {
                    lookahead,
                    extra: self.lookahead_shifts_only_extra(state, lookahead),
                    ..candidate
                },
            );
        }
        if let Some(candidate) = best {
            best_rejected = best_rejected.filter(|rejected| {
                reduced_candidate_order(*rejected, candidate) == ReducedCandidateOrder::Greater
            });
        }
        for external in mode.externals() {
            let Some(scanner_result) = self.match_external(
                state,
                mode,
                *external,
                input,
                byte_position,
                scanner_snapshot,
            )?
            else {
                continue;
            };
            let candidate = ReducedTokenCandidate {
                lookahead: LookaheadSymbol::External(*external),
                end: scanner_result.end_byte(),
                inspected_end: scanner_result.end_byte(),
                extra: false,
                external: true,
                immediate: false,
                literal: true,
                lexical_precedence: 0,
                implicit_precedence: 0,
                scanner: Some(scanner_result),
            };
            if !state
                .entries()
                .iter()
                .any(|entry| entry.lookahead() == candidate.lookahead)
            {
                push_reduced_candidate(&mut best_rejected, candidate);
                continue;
            }
            push_reduced_candidate(&mut best, candidate);
        }
        if let Some(candidate) = best {
            return Ok(ReducedToken {
                lookahead: candidate.lookahead,
                end: candidate.end,
                inspected_end: candidate.inspected_end,
                scanner: candidate.scanner,
            });
        }
        if let Some(rejected) = best_rejected {
            return Err(ReducedParseError::new(ReducedParseErrorKind::NoAction {
                state: state.id(),
                lookahead: rejected.lookahead,
                byte_position,
            }));
        }
        Err(ReducedParseError::new(ReducedParseErrorKind::NoToken {
            state: state.id(),
            byte_position,
            expected: self.mode_token_spellings(mode).into_iter().collect(),
        }))
    }

    fn match_terminal_candidates_for_mode(
        &self,
        mode: &LexMode,
        input: &str,
        byte_position: usize,
    ) -> Result<Vec<ReducedTokenCandidate>, ReducedParseError> {
        let key = LexModeCacheKey {
            mode: mode.id(),
            byte_position,
        };
        if let Some(cached) = self.lex_mode_cache.borrow().get(&key).cloned() {
            return cached;
        }
        let result = self.match_terminal_candidates_for_mode_uncached(mode, input, byte_position);
        self.lex_mode_cache.borrow_mut().insert(key, result.clone());
        result
    }

    fn match_terminal_candidates_for_mode_uncached(
        &self,
        mode: &LexMode,
        input: &str,
        byte_position: usize,
    ) -> Result<Vec<ReducedTokenCandidate>, ReducedParseError> {
        let mut candidates = Vec::new();
        let compiled_mode = &self.runtime_plan.get().compiled_lex_modes[mode.id().get() as usize];
        let direct_pattern_ends =
            self.match_compiled_direct_pattern_set(compiled_mode, input, byte_position)?;
        for terminal_row in &compiled_mode.terminals {
            let Some(match_) = self.match_compiled_terminal_with_set(
                terminal_row,
                input,
                byte_position,
                &direct_pattern_ends,
            )?
            else {
                continue;
            };
            if match_.end == byte_position {
                continue;
            }
            candidates.push(ReducedTokenCandidate {
                lookahead: LookaheadSymbol::Terminal(terminal_row.terminal),
                end: match_.end,
                inspected_end: match_.inspected_end,
                extra: false,
                external: false,
                immediate: terminal_row.immediate,
                literal: terminal_row.literal,
                lexical_precedence: terminal_row.lexical_precedence,
                implicit_precedence: terminal_row.implicit_precedence,
                scanner: None,
            });
        }
        Ok(candidates)
    }

    fn lookahead_for_terminal(
        &self,
        state: &ParseState,
        terminal: TerminalId,
    ) -> Option<LookaheadSymbol> {
        state
            .entries()
            .iter()
            .find_map(|entry| match entry.lookahead() {
                LookaheadSymbol::Terminal(candidate) if candidate == terminal => {
                    Some(entry.lookahead())
                }
                LookaheadSymbol::ReservedWord {
                    terminal: candidate,
                    ..
                } if candidate == terminal => Some(entry.lookahead()),
                _ => None,
            })
    }

    fn mode_token_spellings(&self, mode: &LexMode) -> Vec<String> {
        let mut spellings: Vec<String> = mode
            .terminals()
            .iter()
            .map(|terminal| self.parser.symbols.terminals[terminal.get() as usize].spelling())
            .map(str::to_owned)
            .collect();
        spellings.extend(mode.externals().iter().map(|external| {
            self.parser.symbols.externals[external.get() as usize]
                .name()
                .unwrap_or("<anonymous-external>")
                .to_owned()
        }));
        spellings
    }

    #[cfg(test)]
    fn match_terminal(
        &self,
        terminal: &TerminalSymbol,
        input: &str,
        byte_position: usize,
    ) -> Result<Option<usize>, ReducedParseError> {
        match terminal.kind() {
            ParserTerminalKind::String => Ok(input[byte_position..]
                .starts_with(terminal.spelling())
                .then_some(byte_position + terminal.spelling().len())),
            ParserTerminalKind::Pattern => {
                Ok(self.match_pattern(terminal.spelling(), terminal.flags(), input, byte_position))
            }
            ParserTerminalKind::Token | ParserTerminalKind::ImmediateToken => {
                let Some(root) = terminal.lexical_root() else {
                    return Err(ReducedParseError::new(
                        ReducedParseErrorKind::UnsupportedTerminal {
                            terminal: terminal.id(),
                            spelling: terminal.spelling().to_owned(),
                        },
                    ));
                };
                let (GrammarExpr::Token(content) | GrammarExpr::ImmediateToken(content)) =
                    self.grammar.expr(root)
                else {
                    return Err(ReducedParseError::new(
                        ReducedParseErrorKind::UnsupportedTerminal {
                            terminal: terminal.id(),
                            spelling: terminal.spelling().to_owned(),
                        },
                    ));
                };
                self.match_lexical_expr(*content, input, byte_position)
            }
        }
    }

    fn match_compiled_terminal(
        &self,
        terminal: &CompiledLexTerminal,
        input: &str,
        byte_position: usize,
    ) -> Result<Option<LexMatch>, ReducedParseError> {
        match &terminal.matcher {
            CompiledTerminalMatcher::Expr(expr) => {
                self.match_compiled_lex_expr(expr, input, byte_position)
            }
            CompiledTerminalMatcher::UnsupportedTerminal { terminal, spelling } => Err(
                ReducedParseError::new(ReducedParseErrorKind::UnsupportedTerminal {
                    terminal: *terminal,
                    spelling: spelling.clone(),
                }),
            ),
        }
    }

    fn match_compiled_terminal_with_set(
        &self,
        terminal: &CompiledLexTerminal,
        input: &str,
        byte_position: usize,
        direct_pattern_ends: &[Option<LexMatch>],
    ) -> Result<Option<LexMatch>, ReducedParseError> {
        if let Some(set_index) = terminal.direct_pattern_index {
            return Ok(direct_pattern_ends.get(set_index).copied().flatten());
        }
        self.match_compiled_terminal(terminal, input, byte_position)
    }

    fn match_compiled_direct_pattern_set(
        &self,
        mode: &CompiledLexMode,
        input: &str,
        byte_position: usize,
    ) -> Result<Vec<Option<LexMatch>>, ReducedParseError> {
        let Some(pattern_set) = &mode.direct_pattern_set else {
            return Ok(Vec::new());
        };
        let mut ends = vec![None; pattern_set.terminal_indices.len()];
        let Some(haystack) = input.get(byte_position..) else {
            return Ok(ends);
        };
        let matches = pattern_set.regex_set.matches(haystack);
        for set_index in matches.iter() {
            let terminal_index = pattern_set.terminal_indices[set_index];
            let terminal = &mode.terminals[terminal_index];
            ends[set_index] = self.match_compiled_terminal(terminal, input, byte_position)?;
        }
        Ok(ends)
    }

    fn match_compiled_lex_expr(
        &self,
        expr: &CompiledLexExpr,
        input: &str,
        byte_position: usize,
    ) -> Result<Option<LexMatch>, ReducedParseError> {
        match expr {
            CompiledLexExpr::Blank => Ok(Some(LexMatch::new(byte_position, byte_position))),
            CompiledLexExpr::String(value) => Ok(input[byte_position..]
                .starts_with(value)
                .then_some(LexMatch::new(
                    byte_position + value.len(),
                    byte_position + value.len(),
                ))),
            CompiledLexExpr::Pattern(pattern) => {
                Ok(match_compiled_pattern(pattern, input, byte_position))
            }
            CompiledLexExpr::Until { markers } => Ok(match_until_markers_with_inspection(
                markers.iter().map(String::as_str),
                input,
                byte_position,
            )),
            CompiledLexExpr::Nested { open, close } => Ok(match_nested_delimiters_with_inspection(
                open,
                close,
                input,
                byte_position,
            )),
            CompiledLexExpr::Seq(members) => {
                let mut position = byte_position;
                let mut inspected_end = byte_position;
                for member in members {
                    let Some(match_) = self.match_compiled_lex_expr(member, input, position)?
                    else {
                        return Ok(None);
                    };
                    position = match_.end;
                    inspected_end = inspected_end.max(match_.inspected_end);
                }
                Ok(Some(LexMatch::new(position, inspected_end)))
            }
            CompiledLexExpr::Choice(members) => {
                let mut best = None::<LexMatch>;
                for member in members {
                    if let Some(match_) =
                        self.match_compiled_lex_expr(member, input, byte_position)?
                        && best.is_none_or(|best| match_.end > best.end)
                    {
                        best = Some(match_);
                    }
                }
                Ok(best)
            }
            CompiledLexExpr::Repeat(content) => {
                let mut position = byte_position;
                let mut inspected_end = byte_position;
                while let Some(match_) = self.match_compiled_lex_expr(content, input, position)? {
                    inspected_end = inspected_end.max(match_.inspected_end);
                    if match_.end == position {
                        break;
                    }
                    position = match_.end;
                }
                Ok(Some(LexMatch::new(position, inspected_end)))
            }
            CompiledLexExpr::Repeat1(content) => {
                let Some(first) = self.match_compiled_lex_expr(content, input, byte_position)?
                else {
                    return Ok(None);
                };
                let mut position = first.end;
                let mut inspected_end = first.inspected_end;
                if position == byte_position {
                    return Ok(None);
                }
                while let Some(match_) = self.match_compiled_lex_expr(content, input, position)? {
                    inspected_end = inspected_end.max(match_.inspected_end);
                    if match_.end == position {
                        break;
                    }
                    position = match_.end;
                }
                Ok(Some(LexMatch::new(position, inspected_end)))
            }
            CompiledLexExpr::UnsupportedSymbol(expr) => Err(ReducedParseError::new(
                ReducedParseErrorKind::UnsupportedLexicalSymbol { expr: *expr },
            )),
        }
    }

    fn match_external(
        &self,
        state: &ParseState,
        mode: &LexMode,
        external: ExternalId,
        input: &str,
        byte_position: usize,
        scanner_snapshot: Option<ScannerSnapshotId>,
    ) -> Result<Option<ReducedExternalScanResult>, ReducedParseError> {
        let Some(scanner) = self.external_scanner else {
            return Err(ReducedParseError::new(
                ReducedParseErrorKind::UnsupportedExternalScanner {
                    state: state.id(),
                    external_count: mode.externals().len(),
                },
            ));
        };
        let external_row = &self.parser.symbols.externals[external.get() as usize];
        let valid_symbols = mode
            .valid_symbols()
            .map(|valid_symbols| &self.table.valid_symbol_sets()[valid_symbols.get() as usize]);
        scanner.scan(ReducedExternalScan {
            state: state.id(),
            external,
            external_symbol: external_row,
            valid_symbols,
            input,
            byte_position,
            scanner_snapshot,
        })
    }

    #[cfg(test)]
    fn match_lexical_expr(
        &self,
        expr: GrammarExprId,
        input: &str,
        byte_position: usize,
    ) -> Result<Option<usize>, ReducedParseError> {
        self.match_lexical_expr_inner(expr, input, byte_position, &mut Vec::new())
    }

    #[cfg(test)]
    fn match_lexical_expr_inner(
        &self,
        expr: GrammarExprId,
        input: &str,
        byte_position: usize,
        rule_stack: &mut Vec<RuleId>,
    ) -> Result<Option<usize>, ReducedParseError> {
        match self.grammar.expr(expr) {
            GrammarExpr::Blank => Ok(Some(byte_position)),
            GrammarExpr::StringToken(value) => Ok(input[byte_position..]
                .starts_with(value)
                .then_some(byte_position + value.len())),
            GrammarExpr::PatternToken { value, flags } => {
                Ok(self.match_pattern(value, flags.as_deref(), input, byte_position))
            }
            GrammarExpr::Until { markers } => Ok(match_until_markers(
                markers.iter().map(String::as_str),
                input,
                byte_position,
            )),
            GrammarExpr::Nested { open, close } => {
                Ok(match_nested_delimiters(open, close, input, byte_position))
            }
            GrammarExpr::Token(content)
            | GrammarExpr::ImmediateToken(content)
            | GrammarExpr::Field { content, .. }
            | GrammarExpr::Prec { content, .. }
            | GrammarExpr::PrecDynamic { content, .. }
            | GrammarExpr::Alias { content, .. }
            | GrammarExpr::Reserved { content, .. } => {
                self.match_lexical_expr_inner(*content, input, byte_position, rule_stack)
            }
            GrammarExpr::Choice(members) => {
                let mut best = None;
                for member in members {
                    if let Some(end) =
                        self.match_lexical_expr_inner(*member, input, byte_position, rule_stack)?
                    {
                        if best.is_none_or(|best| end > best) {
                            best = Some(end);
                        }
                    }
                }
                Ok(best)
            }
            GrammarExpr::Seq(members) => {
                let mut position = byte_position;
                for member in members {
                    let Some(end) =
                        self.match_lexical_expr_inner(*member, input, position, rule_stack)?
                    else {
                        return Ok(None);
                    };
                    position = end;
                }
                Ok(Some(position))
            }
            GrammarExpr::Repeat(content) => {
                let mut position = byte_position;
                while let Some(end) =
                    self.match_lexical_expr_inner(*content, input, position, rule_stack)?
                {
                    if end == position {
                        break;
                    }
                    position = end;
                }
                Ok(Some(position))
            }
            GrammarExpr::Repeat1(content) => {
                let Some(mut position) =
                    self.match_lexical_expr_inner(*content, input, byte_position, rule_stack)?
                else {
                    return Ok(None);
                };
                if position == byte_position {
                    return Ok(None);
                }
                while let Some(end) =
                    self.match_lexical_expr_inner(*content, input, position, rule_stack)?
                {
                    if end == position {
                        break;
                    }
                    position = end;
                }
                Ok(Some(position))
            }
            GrammarExpr::Symbol(SymbolRef::Rule(rule)) => {
                if rule_stack.contains(rule) {
                    return Err(ReducedParseError::new(
                        ReducedParseErrorKind::UnsupportedLexicalSymbol { expr },
                    ));
                }
                rule_stack.push(*rule);
                let result = self.match_lexical_expr_inner(
                    self.grammar.rule(*rule).expr(),
                    input,
                    byte_position,
                    rule_stack,
                );
                rule_stack.pop();
                result
            }
            GrammarExpr::Symbol(SymbolRef::External(_)) => Err(ReducedParseError::new(
                ReducedParseErrorKind::UnsupportedLexicalSymbol { expr },
            )),
        }
    }

    #[cfg(test)]
    fn match_pattern(
        &self,
        pattern: &str,
        flags: Option<&str>,
        input: &str,
        byte_position: usize,
    ) -> Option<usize> {
        if regex_flags_are_empty(flags)
            && let Some(result) = match_known_pattern(pattern, input, byte_position)
        {
            return result;
        }
        self.match_regex_leaf(pattern, flags, input, byte_position)
    }

    #[cfg(test)]
    fn match_regex_leaf(
        &self,
        pattern: &str,
        flags: Option<&str>,
        input: &str,
        byte_position: usize,
    ) -> Option<usize> {
        let haystack = input.get(byte_position..)?;
        let key = (pattern.to_owned(), normalized_regex_flags(flags));
        let regex = self.regex_patterns.get(&key)?.as_ref()?;
        regex
            .find(haystack)
            .filter(|match_| match_.start() == 0)
            .map(|match_| byte_position + match_.end())
    }

    fn lookahead_shifts_only_extra(&self, state: &ParseState, lookahead: LookaheadSymbol) -> bool {
        state
            .entries()
            .iter()
            .find(|entry| entry.lookahead() == lookahead)
            .is_some_and(|entry| {
                entry
                    .actions()
                    .iter()
                    .all(|action| matches!(action, ParseAction::ShiftExtra))
            })
    }

    fn reduce_fragment(
        &self,
        production: ProductionId,
        metadata: ProductionMetadataId,
        child_count: usize,
        stack: &mut Vec<ReducedStackEntry>,
        include_trailing_extras: bool,
    ) -> Result<ReducedReduction, ReducedParseError> {
        let production_row = &self.parser.productions[production.get() as usize];
        let metadata_row = &self.parser.production_metadata[metadata.get() as usize];
        let mut children = Vec::new();
        let mut trailing_extras = Vec::new();
        if !include_trailing_extras {
            while stack.last().is_some_and(|entry| entry.extra) {
                let entry = stack
                    .pop()
                    .ok_or_else(|| ReducedParseError::new(ReducedParseErrorKind::EmptyStack))?;
                let Some(fragment) = entry.fragment else {
                    return Err(ReducedParseError::new(ReducedParseErrorKind::EmptyStack));
                };
                trailing_extras.push(fragment);
            }
            trailing_extras.reverse();
        }
        let mut popped = Vec::new();
        let mut remaining_children = child_count;
        while remaining_children > 0 {
            let entry = stack
                .pop()
                .ok_or_else(|| ReducedParseError::new(ReducedParseErrorKind::EmptyStack))?;
            let Some(fragment) = entry.fragment else {
                return Err(ReducedParseError::new(ReducedParseErrorKind::EmptyStack));
            };
            if !entry.extra {
                remaining_children -= 1;
            }
            popped.push((entry.extra, fragment));
        }
        popped.reverse();
        let mut steps = production_row.steps().iter();
        for (extra, fragment) in popped {
            let mut step_children = fragment.into_children();
            if !extra {
                let Some(step) = steps.next() else {
                    return Err(ReducedParseError::new(ReducedParseErrorKind::EmptyStack));
                };
                if let (Some(alias), Some(true)) = (step.alias(), step.alias_named()) {
                    let alias_name = self.parser.aliases[alias.get() as usize].value.clone();
                    if step_children.is_empty() {
                        step_children.push(SexpChild {
                            field: None,
                            value: SexpValue::Node(SexpNode {
                                kind: alias_name,
                                children: Vec::new(),
                            }),
                        });
                    } else {
                        for child in &mut step_children {
                            if let SexpValue::Node(node) = &mut child.value {
                                node.kind.clone_from(&alias_name);
                            }
                        }
                    }
                }
            }
            children.extend(step_children);
        }

        if let Some(public_node) = metadata_row.public_node() {
            let kind = self.parser.public_node_kinds[public_node.get() as usize]
                .name()
                .to_owned();
            Ok(ReducedReduction {
                fragment: ReducedFragment::Node(SexpNode { kind, children }),
                trailing_extras,
            })
        } else {
            Ok(ReducedReduction {
                fragment: ReducedFragment::Hidden(children),
                trailing_extras,
            })
        }
    }

    fn extra_fragment(&self, lookahead: LookaheadSymbol) -> Option<ReducedFragment> {
        let first = self.runtime_plan.get().first.as_ref()?;
        for extra in &self.parser.extra_roots {
            let ParserSymbol::Nonterminal(nonterminal) = extra.symbol else {
                continue;
            };
            if !first.first[nonterminal.get() as usize].contains(&lookahead) {
                continue;
            }
            let Some(kind) = self
                .parser
                .public_node_kinds
                .iter()
                .find(|kind| kind.source() == PublicNodeKindSource::Rule(nonterminal))
            else {
                continue;
            };
            let kind = kind.name().to_owned();
            return Some(ReducedFragment::Node(SexpNode {
                kind,
                children: Vec::new(),
            }));
        }
        None
    }

    fn finish_accepted_root(
        &self,
        mut node: SexpNode,
        stack: &mut Vec<ReducedStackEntry>,
    ) -> Result<SexpNode, ReducedParseError> {
        let mut leading_children = Vec::new();
        for entry in stack.drain(..) {
            match (entry.extra, entry.fragment) {
                (_, None) => {}
                (true, Some(fragment)) => leading_children.extend(fragment.into_children()),
                (false, Some(_)) => {
                    return Err(ReducedParseError::new(
                        ReducedParseErrorKind::UnreducedStackEntry { state: entry.state },
                    ));
                }
            }
        }
        if !leading_children.is_empty() {
            leading_children.extend(node.children);
            node.children = leading_children;
        }
        Ok(node)
    }

    fn goto_state(
        &self,
        state: ParseStateId,
        nonterminal: NonterminalId,
    ) -> Result<ParseStateId, ReducedParseError> {
        let state_row = self.parse_state(state)?;
        state_row
            .gotos()
            .iter()
            .find(|goto| goto.nonterminal() == nonterminal)
            .map(GotoEntry::state)
            .ok_or_else(|| {
                ReducedParseError::new(ReducedParseErrorKind::MissingGoto { state, nonterminal })
            })
    }
}

#[cfg(test)]
const ASCII_IDENTIFIER_PATTERN: &str = "[A-Za-z_][A-Za-z0-9_]*";
const GINGEMBRE_IDENTIFIER_PATTERN: &str = "(?!if\\b|elif\\b|else\\b|endif\\b|for\\b|endfor\\b|set\\b|endset\\b|block\\b|endblock\\b|extends\\b|include\\b|import\\b|macro\\b|endmacro\\b|break\\b|continue\\b|as\\b|in\\b|is\\b|not\\b|and\\b|or\\b|true\\b|True\\b|false\\b|False\\b|none\\b|None\\b)[A-Za-z_][A-Za-z0-9_]*";

#[cfg(test)]
pub(crate) fn match_pattern(pattern: &str, input: &str, byte_position: usize) -> Option<usize> {
    match_pattern_with_flags(pattern, None, input, byte_position)
}

#[cfg(any(test, feature = "weavy-lowering"))]
pub(crate) fn match_pattern_with_flags(
    pattern: &str,
    flags: Option<&str>,
    input: &str,
    byte_position: usize,
) -> Option<usize> {
    if regex_flags_are_empty(flags)
        && let Some(result) = match_known_pattern(pattern, input, byte_position)
    {
        return result;
    }
    match_cached_regex_leaf(pattern, flags, input, byte_position)
}

fn match_known_pattern(pattern: &str, input: &str, byte_position: usize) -> Option<Option<usize>> {
    match pattern {
        "-?(\\d)*n\\s*(\\+\\s*\\d+)?" => {
            Some(match_css_nth_functional_notation(input, byte_position))
        }
        GINGEMBRE_IDENTIFIER_PATTERN => Some(match_gingembre_identifier(input, byte_position)),
        "[0-9a-fA-F]{1,6}\\s?" => Some(match_css_hex_escape_tail(input, byte_position)),
        ".*" => Some(match_json_line_comment_tail(input, byte_position)),
        "[^*]*\\*+([^/*][^*]*\\*+)*" => Some(match_json_block_comment_body(input, byte_position)),
        "(--|-?[a-zA-Z_\\xA0-\\xFF])[a-zA-Z0-9-_\\xA0-\\xFF]*" => {
            Some(match_css_identifier(input, byte_position))
        }
        "and\\b" => Some(match_ascii_keyword(input, byte_position, "and")),
        "in\\b" => Some(match_ascii_keyword(input, byte_position, "in")),
        "is\\b" => Some(match_ascii_keyword(input, byte_position, "is")),
        "not\\b" => Some(match_ascii_keyword(input, byte_position, "not")),
        "or\\b" => Some(match_ascii_keyword(input, byte_position, "or")),
        _ => None,
    }
}

#[cfg(test)]
fn compile_regex_patterns(
    grammar: &ValidatedGrammar,
    parser: &ParserGrammar,
) -> HashMap<(String, Option<String>), Option<Regex>> {
    let mut patterns = HashMap::new();
    for (_, expr) in grammar.expressions() {
        if let GrammarExpr::PatternToken { value, flags } = expr {
            let key = (value.clone(), normalized_regex_flags(flags.as_deref()));
            patterns
                .entry(key)
                .or_insert_with(|| compile_regex_leaf(value, flags.as_deref()));
        }
    }
    for terminal in &parser.symbols.terminals {
        if terminal.kind() == ParserTerminalKind::Pattern {
            let key = (
                terminal.spelling().to_owned(),
                normalized_regex_flags(terminal.flags()),
            );
            patterns
                .entry(key)
                .or_insert_with(|| compile_regex_leaf(terminal.spelling(), terminal.flags()));
        }
    }
    patterns
}

fn compile_pattern(pattern: &str, flags: Option<&str>) -> CompiledLexPattern {
    CompiledLexPattern {
        source: pattern.to_owned(),
        flags: normalized_regex_flags(flags),
        regex: compile_regex_leaf(pattern, flags),
    }
}

pub(crate) fn match_compiled_pattern(
    pattern: &CompiledLexPattern,
    input: &str,
    byte_position: usize,
) -> Option<LexMatch> {
    if pattern.flags.is_none()
        && let Some(result) = match_known_pattern(&pattern.source, input, byte_position)
    {
        return result.map(|end| LexMatch::new(end, pattern_inspected_end(input, end)));
    }
    let haystack = input.get(byte_position..)?;
    pattern
        .regex
        .as_ref()?
        .find(haystack)
        .filter(|match_| match_.start() == 0)
        .map(|match_| {
            let end = byte_position + match_.end();
            LexMatch::new(end, pattern_inspected_end(input, end))
        })
}

pub(crate) fn compile_lex_modes(
    grammar: &ValidatedGrammar,
    parser: &ParserGrammar,
    table: &ParseTable,
) -> Vec<CompiledLexMode> {
    table
        .lexical_modes()
        .iter()
        .map(|mode| {
            let mut terminals = mode
                .terminals()
                .iter()
                .map(|terminal| {
                    let terminal_row = &parser.symbols.terminals[terminal.get() as usize];
                    compile_lex_terminal(grammar, terminal_row)
                })
                .collect::<Vec<_>>();
            let direct_pattern_set = compile_direct_pattern_set(&mut terminals);
            CompiledLexMode {
                terminals,
                direct_pattern_set,
            }
        })
        .collect()
}

fn compile_direct_pattern_set(
    terminals: &mut [CompiledLexTerminal],
) -> Option<CompiledLexPatternSet> {
    let mut regex_sources = Vec::new();
    let mut terminal_indices = Vec::new();
    for (terminal_index, terminal) in terminals.iter().enumerate() {
        let CompiledTerminalMatcher::Expr(CompiledLexExpr::Pattern(pattern)) = &terminal.matcher
        else {
            continue;
        };
        let Some(regex) = &pattern.regex else {
            continue;
        };
        regex_sources.push(regex.as_str().to_owned());
        terminal_indices.push(terminal_index);
    }
    if regex_sources.is_empty() {
        return None;
    }
    let regex_set = RegexSet::new(&regex_sources).ok()?;
    for (set_index, terminal_index) in terminal_indices.iter().copied().enumerate() {
        terminals[terminal_index].direct_pattern_index = Some(set_index);
    }
    Some(CompiledLexPatternSet {
        regex_set,
        terminal_indices,
    })
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
        direct_pattern_index: None,
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
        ParserTerminalKind::Token | ParserTerminalKind::ImmediateToken => {
            let Some(root) = terminal.lexical_root() else {
                return CompiledTerminalMatcher::UnsupportedTerminal {
                    terminal: terminal.id(),
                    spelling: terminal.spelling().to_owned(),
                };
            };
            let (GrammarExpr::Token(content) | GrammarExpr::ImmediateToken(content)) =
                grammar.expr(root)
            else {
                return CompiledTerminalMatcher::UnsupportedTerminal {
                    terminal: terminal.id(),
                    spelling: terminal.spelling().to_owned(),
                };
            };
            CompiledTerminalMatcher::Expr(compile_lex_expr(grammar, *content))
        }
    }
}

fn compile_lex_expr(grammar: &ValidatedGrammar, expr: GrammarExprId) -> CompiledLexExpr {
    compile_lex_expr_inner(grammar, expr, &mut Vec::new())
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
        GrammarExpr::Until { markers } => CompiledLexExpr::Until {
            markers: markers.clone(),
        },
        GrammarExpr::Nested { open, close } => CompiledLexExpr::Nested {
            open: open.clone(),
            close: close.clone(),
        },
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
        | GrammarExpr::Symbol(SymbolRef::External(_)) => None,
    }
}

pub(crate) fn terminal_lexical_implicit_precedence(
    grammar: &ValidatedGrammar,
    terminal: &TerminalSymbol,
) -> i32 {
    match terminal.kind() {
        ParserTerminalKind::String => 2,
        ParserTerminalKind::Pattern => 0,
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
        | GrammarExpr::Nested { .. } => 0,
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

#[cfg(any(test, feature = "weavy-lowering"))]
pub(crate) fn match_until_markers<'a>(
    markers: impl IntoIterator<Item = &'a str>,
    input: &str,
    byte_position: usize,
) -> Option<usize> {
    match_until_markers_with_inspection(markers, input, byte_position).map(|match_| match_.end)
}

fn match_until_markers_with_inspection<'a>(
    markers: impl IntoIterator<Item = &'a str>,
    input: &str,
    byte_position: usize,
) -> Option<LexMatch> {
    let haystack = input.get(byte_position..)?;
    let markers = markers
        .into_iter()
        .filter(|marker| !marker.is_empty())
        .collect::<Vec<_>>();
    if markers.iter().any(|marker| haystack.starts_with(*marker)) {
        return None;
    }
    let end_and_marker_len = markers
        .iter()
        .filter_map(|marker| haystack.find(*marker).map(|offset| (offset, marker.len())))
        .min()
        .map_or((input.len() - byte_position, 0), |pair| pair);
    let end = byte_position + end_and_marker_len.0;
    let inspected_end = end + end_and_marker_len.1;
    (end > byte_position).then_some(LexMatch::new(end, inspected_end))
}

#[cfg(any(test, feature = "weavy-lowering"))]
pub(crate) fn match_nested_delimiters(
    open: &str,
    close: &str,
    input: &str,
    byte_position: usize,
) -> Option<usize> {
    match_nested_delimiters_with_inspection(open, close, input, byte_position)
        .map(|match_| match_.end)
}

fn match_nested_delimiters_with_inspection(
    open: &str,
    close: &str,
    input: &str,
    byte_position: usize,
) -> Option<LexMatch> {
    if open.is_empty() || close.is_empty() {
        return None;
    }
    let haystack = input.get(byte_position..)?;
    if !haystack.starts_with(open) {
        return None;
    }
    let mut position = byte_position + open.len();
    let mut depth = 1usize;
    while position < input.len() {
        let rest = input.get(position..)?;
        if rest.starts_with(close) {
            position += close.len();
            depth -= 1;
            if depth == 0 {
                return Some(LexMatch::new(position, position));
            }
            continue;
        }
        if rest.starts_with(open) {
            position += open.len();
            depth += 1;
            continue;
        }
        position += rest.chars().next()?.len_utf8();
    }
    Some(LexMatch::new(input.len(), input.len()))
}

fn pattern_inspected_end(input: &str, end: usize) -> usize {
    if end >= input.len() {
        return input.len();
    }
    input[end..]
        .chars()
        .next()
        .map_or(end, |ch| end + ch.len_utf8())
}

#[cfg(any(test, feature = "weavy-lowering"))]
fn match_cached_regex_leaf(
    pattern: &str,
    flags: Option<&str>,
    input: &str,
    byte_position: usize,
) -> Option<usize> {
    let haystack = input.get(byte_position..)?;
    let regex = cached_regex(pattern, flags)?;
    regex
        .find(haystack)
        .filter(|match_| match_.start() == 0)
        .map(|match_| byte_position + match_.end())
}

#[cfg(any(test, feature = "weavy-lowering"))]
fn cached_regex(pattern: &str, flags: Option<&str>) -> Option<Regex> {
    static CACHE: OnceLock<Mutex<HashMap<(String, Option<String>), Option<Regex>>>> =
        OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let key = (pattern.to_owned(), normalized_regex_flags(flags));

    {
        let cache = cache.lock().expect("regex cache poisoned");
        if let Some(regex) = cache.get(&key) {
            return regex.clone();
        }
    }

    let compiled = compile_regex_leaf(pattern, flags);
    let mut cache = cache.lock().expect("regex cache poisoned");
    let entry = cache.entry(key).or_insert_with(|| compiled.clone());
    entry.clone()
}

fn compile_regex_leaf(pattern: &str, flags: Option<&str>) -> Option<Regex> {
    Regex::new(&anchored_regex_source(pattern, flags)?).ok()
}

fn anchored_regex_source(pattern: &str, flags: Option<&str>) -> Option<String> {
    let body = rust_regex_source(pattern);
    let flags = rust_regex_flags(flags)?;
    Some(if flags.is_empty() {
        format!("\\A(?:{})", body)
    } else {
        format!("\\A(?{}:{})", flags, body)
    })
}

fn normalized_regex_flags(flags: Option<&str>) -> Option<String> {
    flags.filter(|flags| !flags.is_empty()).map(str::to_owned)
}

#[cfg(any(test, feature = "weavy-lowering"))]
fn regex_flags_are_empty(flags: Option<&str>) -> bool {
    flags.is_none_or(str::is_empty)
}

fn rust_regex_flags(flags: Option<&str>) -> Option<String> {
    let mut rust_flags = String::new();
    for flag in flags.unwrap_or("").chars() {
        match flag {
            'i' | 'm' | 's' if !rust_flags.contains(flag) => rust_flags.push(flag),
            'i' | 'm' | 's' | 'u' | 'g' | 'y' | 'd' => {}
            _ => return None,
        }
    }
    Some(rust_flags)
}

fn rust_regex_source(pattern: &str) -> String {
    let mut out = String::with_capacity(pattern.len());
    let mut chars = pattern.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }

        let Some(escaped) = chars.next() else {
            out.push('\\');
            break;
        };

        if escaped == '/' {
            out.push('/');
            continue;
        }

        if escaped == 'u' {
            let mut hex = String::with_capacity(4);
            for _ in 0..4 {
                let Some(hex_ch) = chars.peek().copied().filter(|ch| ch.is_ascii_hexdigit()) else {
                    out.push('\\');
                    out.push('u');
                    out.push_str(&hex);
                    out.extend(chars);
                    return out;
                };
                chars.next();
                hex.push(hex_ch);
            }
            out.push_str("\\u{");
            out.push_str(&hex);
            out.push('}');
            continue;
        }

        out.push('\\');
        out.push(escaped);
    }
    out
}

fn match_ascii_keyword(input: &str, byte_position: usize, keyword: &str) -> Option<usize> {
    if !input[byte_position..].starts_with(keyword) {
        return None;
    }
    let end = byte_position + keyword.len();
    if input[end..]
        .as_bytes()
        .first()
        .is_some_and(|byte| *byte == b'_' || byte.is_ascii_alphanumeric())
    {
        return None;
    }
    Some(end)
}

fn match_gingembre_identifier(input: &str, byte_position: usize) -> Option<usize> {
    let end = match_ascii_identifier(input, byte_position)?;
    let word = &input[byte_position..end];
    if is_gingembre_keyword(word) {
        return None;
    }
    Some(end)
}

fn match_ascii_identifier(input: &str, byte_position: usize) -> Option<usize> {
    let bytes = input[byte_position..].as_bytes();
    let first = bytes.first().copied()?;
    if first != b'_' && !first.is_ascii_alphabetic() {
        return None;
    }
    let len = bytes
        .iter()
        .take_while(|byte| **byte == b'_' || byte.is_ascii_alphanumeric())
        .count();
    Some(byte_position + len)
}

fn is_gingembre_keyword(word: &str) -> bool {
    matches!(
        word,
        "if" | "elif"
            | "else"
            | "endif"
            | "for"
            | "endfor"
            | "set"
            | "endset"
            | "block"
            | "endblock"
            | "extends"
            | "include"
            | "import"
            | "macro"
            | "endmacro"
            | "break"
            | "continue"
            | "as"
            | "in"
            | "is"
            | "not"
            | "and"
            | "or"
            | "true"
            | "True"
            | "false"
            | "False"
            | "none"
            | "None"
    )
}

fn match_json_line_comment_tail(input: &str, byte_position: usize) -> Option<usize> {
    Some(
        input[byte_position..]
            .find(['\n', '\r'])
            .map_or(input.len(), |offset| byte_position + offset),
    )
}

fn match_json_block_comment_body(input: &str, byte_position: usize) -> Option<usize> {
    input[byte_position..]
        .find("*/")
        .map(|offset| byte_position + offset + 1)
}

fn match_while(
    input: &str,
    byte_position: usize,
    predicate: impl Fn(char) -> bool,
    min_chars: usize,
) -> Option<usize> {
    let mut position = byte_position;
    let mut count = 0usize;
    for ch in input[byte_position..].chars() {
        if !predicate(ch) {
            break;
        }
        position += ch.len_utf8();
        count += 1;
    }
    (count >= min_chars).then_some(position)
}

fn match_css_identifier(input: &str, byte_position: usize) -> Option<usize> {
    let rest = &input[byte_position..];
    if rest.starts_with("--") {
        let mut position = byte_position + 2;
        while let Some(ch) = input[position..]
            .chars()
            .next()
            .filter(|ch| css_ident_continue(*ch))
        {
            position += ch.len_utf8();
        }
        return Some(position);
    }
    let mut chars = rest.char_indices();
    let (first_offset, first) = chars.next()?;
    debug_assert_eq!(first_offset, 0);
    let mut position = byte_position;
    if first == '-' {
        position += first.len_utf8();
        let next = input[position..].chars().next()?;
        if !css_ident_start(next) {
            return None;
        }
        position += next.len_utf8();
    } else if css_ident_start(first) {
        position += first.len_utf8();
    } else {
        return None;
    }
    while let Some(ch) = input[position..]
        .chars()
        .next()
        .filter(|ch| css_ident_continue(*ch))
    {
        position += ch.len_utf8();
    }
    Some(position)
}

fn match_css_hex_escape_tail(input: &str, byte_position: usize) -> Option<usize> {
    let mut position = byte_position;
    let mut count = 0usize;
    while count < 6 {
        let Some(ch) = input[position..]
            .chars()
            .next()
            .filter(|ch| ch.is_ascii_hexdigit())
        else {
            break;
        };
        position += ch.len_utf8();
        count += 1;
    }
    if count == 0 {
        return None;
    }
    if let Some(ch) = input[position..]
        .chars()
        .next()
        .filter(|ch| ch.is_whitespace())
    {
        position += ch.len_utf8();
    }
    Some(position)
}

fn match_css_nth_functional_notation(input: &str, byte_position: usize) -> Option<usize> {
    let mut position = byte_position;
    if input[position..].starts_with('-') {
        position += '-'.len_utf8();
    }
    while let Some(ch) = input[position..]
        .chars()
        .next()
        .filter(|ch| ch.is_ascii_digit())
    {
        position += ch.len_utf8();
    }
    if !input[position..].starts_with('n') {
        return None;
    }
    position += 'n'.len_utf8();
    position = skip_pattern_whitespace(input, position);
    if input[position..].starts_with('+') {
        position += '+'.len_utf8();
        position = skip_pattern_whitespace(input, position);
        let digits = match_while(input, position, |ch| ch.is_ascii_digit(), 1)?;
        position = digits;
    }
    Some(position)
}

fn css_ident_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_' || !ch.is_ascii()
}

fn css_ident_continue(ch: char) -> bool {
    css_ident_start(ch) || ch.is_ascii_digit() || ch == '-'
}

fn skip_pattern_whitespace(input: &str, byte_position: usize) -> usize {
    let mut position = byte_position;
    while let Some(ch) = input[position..]
        .chars()
        .next()
        .filter(|ch| ch.is_whitespace())
    {
        position += ch.len_utf8();
    }
    position
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ReducedToken {
    lookahead: LookaheadSymbol,
    end: usize,
    inspected_end: usize,
    scanner: Option<ReducedExternalScanResult>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ReducedTokenCandidate {
    lookahead: LookaheadSymbol,
    end: usize,
    inspected_end: usize,
    extra: bool,
    external: bool,
    immediate: bool,
    literal: bool,
    lexical_precedence: i32,
    implicit_precedence: i32,
    scanner: Option<ReducedExternalScanResult>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LexMatch {
    pub(crate) end: usize,
    pub(crate) inspected_end: usize,
}

impl LexMatch {
    const fn new(end: usize, inspected_end: usize) -> Self {
        Self { end, inspected_end }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReducedCandidateOrder {
    Less,
    Equal,
    Greater,
}

fn reduced_candidate_order(
    left: ReducedTokenCandidate,
    right: ReducedTokenCandidate,
) -> ReducedCandidateOrder {
    if left.immediate && !left.extra && right.extra {
        return ReducedCandidateOrder::Greater;
    }
    if left.extra && right.immediate && !right.extra {
        return ReducedCandidateOrder::Less;
    }
    if left.end == right.end && left.external && !right.external {
        return ReducedCandidateOrder::Greater;
    }
    if left.end == right.end && !left.external && right.external {
        return ReducedCandidateOrder::Less;
    }
    match left.lexical_precedence.cmp(&right.lexical_precedence) {
        std::cmp::Ordering::Greater => return ReducedCandidateOrder::Greater,
        std::cmp::Ordering::Less => return ReducedCandidateOrder::Less,
        std::cmp::Ordering::Equal => {}
    }
    match left.end.cmp(&right.end) {
        std::cmp::Ordering::Greater => ReducedCandidateOrder::Greater,
        std::cmp::Ordering::Less => ReducedCandidateOrder::Less,
        std::cmp::Ordering::Equal if left.external && !right.external => {
            ReducedCandidateOrder::Greater
        }
        std::cmp::Ordering::Equal if !left.external && right.external => {
            ReducedCandidateOrder::Less
        }
        std::cmp::Ordering::Equal if left.implicit_precedence > right.implicit_precedence => {
            ReducedCandidateOrder::Greater
        }
        std::cmp::Ordering::Equal if left.implicit_precedence < right.implicit_precedence => {
            ReducedCandidateOrder::Less
        }
        std::cmp::Ordering::Equal if left.literal && !right.literal => {
            ReducedCandidateOrder::Greater
        }
        std::cmp::Ordering::Equal if !left.literal && right.literal => ReducedCandidateOrder::Less,
        std::cmp::Ordering::Equal => ReducedCandidateOrder::Equal,
    }
}

fn push_reduced_candidate(
    candidate_slot: &mut Option<ReducedTokenCandidate>,
    candidate: ReducedTokenCandidate,
) {
    match candidate_slot {
        Some(current)
            if reduced_candidate_order(*current, candidate) != ReducedCandidateOrder::Less => {}
        _ => *candidate_slot = Some(candidate),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReducedBranch {
    id: ReducedBranchId,
    stack: Vec<ReducedStackEntry>,
    byte_position: usize,
    scanner_snapshot: Option<ScannerSnapshotId>,
    trace: Vec<ReducedTraceStep>,
}

impl ReducedBranch {
    fn commit_scanner_snapshot(&mut self, token: ReducedToken) {
        if let Some(scanner) = token.scanner {
            self.scanner_snapshot = scanner.after();
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ReducedStepOutcome {
    Branch(ReducedBranch),
    Accepted {
        branch: ReducedBranchId,
        node: SexpNode,
        trace: Vec<ReducedTraceStep>,
    },
    Failed {
        branch: ReducedBranchId,
        error: ReducedParseError,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReducedStackEntry {
    state: ParseStateId,
    fragment: Option<ReducedFragment>,
    extra: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ReducedFragment {
    Hidden(Vec<SexpChild>),
    Node(SexpNode),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReducedReduction {
    fragment: ReducedFragment,
    trailing_extras: Vec<ReducedFragment>,
}

impl ReducedFragment {
    fn into_children(self) -> Vec<SexpChild> {
        match self {
            Self::Hidden(children) => children,
            Self::Node(node) => vec![SexpChild {
                field: None,
                value: SexpValue::Node(node),
            }],
        }
    }
}

/// First runtime stack/tree parser over generated Snark parse tables.
///
/// This runtime executes the same grammar-derived action table as
/// [`ReducedParser`], but keeps branch ids in the runtime `StackVersionId`
/// domain and records tree construction through `TreeNodeId` plus structured
/// `TraceEvent` / `TreeEvent` rows before normalizing the accepted runtime tree
/// to the corpus S-expression view.
pub struct RuntimeParser<'a> {
    reduced: ReducedParser<'a>,
    recovery_step_limit: Option<usize>,
}

/// Byte edit shape used by Tree-sitter-style incremental reparsing.
///
/// `start_byte..old_end_byte` names the replaced range in the previous input.
/// `start_byte..new_end_byte` names the replacement range in the new input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeInputEdit {
    start_byte: usize,
    old_end_byte: usize,
    new_end_byte: usize,
}

impl RuntimeInputEdit {
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
    ) -> Result<(), ReducedParseError> {
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
            Err(ReducedParseError::new(
                ReducedParseErrorKind::InvalidInputEdit {
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

/// Persistent runtime parse session for the incremental parser seam.
///
/// This session preserves the previous input and accepted report, then builds a
/// conservative index of reusable named-node subtrees for reparses. Full parses
/// remain the differential oracle for reuse tests.
pub struct RuntimeParseSession<'a> {
    parser: RuntimeParser<'a>,
    last_input: Option<String>,
    last_report: Option<RuntimeParseReport>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct RuntimeReuseKey {
    byte_position: usize,
    entry_state: ParseStateId,
    scanner_snapshot: Option<ScannerSnapshotId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RuntimeReusableNode {
    node: TreeNodeId,
    tree: SexpNode,
    symbol: NonterminalId,
    entry_state: ParseStateId,
    entry_scanner_snapshot: Option<ScannerSnapshotId>,
    exit_scanner_snapshot: Option<ScannerSnapshotId>,
    start_byte: usize,
    end_byte: usize,
    lookahead_end_byte: usize,
    contains_error: bool,
}

#[derive(Debug, Clone)]
struct RuntimeReuseIndex {
    nodes: HashMap<RuntimeReuseKey, RuntimeReusableNode>,
}

impl RuntimeReuseIndex {
    fn from_report(report: &RuntimeParseReport, edit: RuntimeInputEdit) -> Self {
        let delta = edit.new_end_byte as isize - edit.old_end_byte as isize;
        let mut nodes = HashMap::new();
        for node in report.reusable_nodes.iter().filter(|node| {
            !node.contains_error
                && edit
                    .reuse_position(node.start_byte, node.end_byte)
                    .is_some()
        }) {
            let Some((start_byte, end_byte)) = edit.reuse_position(node.start_byte, node.end_byte)
            else {
                continue;
            };
            let reusable_before_edit =
                node.end_byte <= edit.start_byte && node.lookahead_end_byte <= edit.start_byte;
            let reusable_after_edit = node.start_byte >= edit.old_end_byte;
            if !reusable_before_edit && !reusable_after_edit {
                continue;
            }
            let lookahead_end_byte = shift_byte(node.lookahead_end_byte, edit.old_end_byte, delta);
            let shifted = RuntimeReusableNode {
                start_byte,
                end_byte,
                lookahead_end_byte,
                ..node.clone()
            };
            let key = RuntimeReuseKey {
                byte_position: shifted.start_byte,
                entry_state: shifted.entry_state,
                scanner_snapshot: shifted.entry_scanner_snapshot,
            };
            nodes.entry(key).or_insert(shifted);
        }
        Self { nodes }
    }

    fn get(
        &self,
        byte_position: usize,
        entry_state: ParseStateId,
        scanner_snapshot: Option<ScannerSnapshotId>,
    ) -> Option<&RuntimeReusableNode> {
        self.nodes.get(&RuntimeReuseKey {
            byte_position,
            entry_state,
            scanner_snapshot,
        })
    }
}

fn shift_byte(byte: usize, old_end_byte: usize, delta: isize) -> usize {
    if byte >= old_end_byte {
        byte.saturating_add_signed(delta)
    } else {
        byte
    }
}

impl<'a> RuntimeParseSession<'a> {
    /// Start a persistent runtime parse session.
    pub const fn new(parser: RuntimeParser<'a>) -> Self {
        Self {
            parser,
            last_input: None,
            last_report: None,
        }
    }

    /// Last input accepted by this session.
    pub fn last_input(&self) -> Option<&str> {
        self.last_input.as_deref()
    }

    /// Last report accepted by this session.
    pub const fn last_report(&self) -> Option<&RuntimeParseReport> {
        self.last_report.as_ref()
    }

    /// Parse a full input and make it the new session baseline.
    pub fn parse_compact(
        &mut self,
        input: impl Into<String>,
    ) -> Result<&RuntimeParseReport, ReducedParseError> {
        let input = input.into();
        let report = self.parser.parse_compact_with_report(&input)?;
        self.last_input = Some(input);
        self.last_report = Some(report);
        Ok(self
            .last_report
            .as_ref()
            .expect("session report was just installed"))
    }

    /// Reparse after an edit and make the new input the session baseline.
    ///
    /// This is the stable API seam for incremental reuse. It validates that the
    /// edit describes the old and new inputs, builds a conservative reusable-node
    /// index from the previous accepted report, and reparses with full-parse tree
    /// equivalence as the oracle.
    pub fn reparse_compact(
        &mut self,
        edit: RuntimeInputEdit,
        new_input: impl Into<String>,
    ) -> Result<&RuntimeParseReport, ReducedParseError> {
        let new_input = new_input.into();
        if let Some(old_input) = self.last_input.as_deref() {
            edit.validate_against(old_input, &new_input)?;
        }
        let reuse_index = self
            .last_report
            .as_ref()
            .map(|report| RuntimeReuseIndex::from_report(report, edit));
        let report = if let Some(reuse_index) = reuse_index.as_ref() {
            self.parser
                .parse_compact_with_reuse_index(&new_input, reuse_index)?
        } else {
            self.parser.parse_compact_with_report(&new_input)?
        };
        self.last_input = Some(new_input);
        self.last_report = Some(report);
        Ok(self
            .last_report
            .as_ref()
            .expect("session report was just installed"))
    }
}

impl RuntimeInputEdit {
    fn reuse_position(&self, start_byte: usize, end_byte: usize) -> Option<(usize, usize)> {
        if self.old_end_byte == self.start_byte {
            if start_byte < self.start_byte && self.start_byte < end_byte {
                return None;
            }
        } else if start_byte < self.old_end_byte && self.start_byte < end_byte {
            return None;
        }
        let delta = self.new_end_byte as isize - self.old_end_byte as isize;
        Some((
            shift_byte(start_byte, self.old_end_byte, delta),
            shift_byte(end_byte, self.old_end_byte, delta),
        ))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuntimeRecoveryMode {
    Strict,
    SkipInvalidInput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuntimeTraceDetail {
    Full,
    Lineage,
}

fn push_full_runtime_trace(
    trace_events: &mut Vec<TraceEvent>,
    detail: RuntimeTraceDetail,
    event: TraceEvent,
) {
    if detail == RuntimeTraceDetail::Full {
        trace_events.push(event);
    }
}

impl<'a> RuntimeParser<'a> {
    /// Build a runtime parser over validated grammar facts and generated tables.
    pub fn new(
        grammar: &'a ValidatedGrammar,
        parser: &'a ParserGrammar,
        table: &'a ParseTable,
    ) -> Result<Self, ReducedParseError> {
        Ok(Self {
            reduced: ReducedParser::new(grammar, parser, table)?,
            recovery_step_limit: None,
        })
    }

    /// Build a runtime parser that borrows a reusable runtime plan.
    pub fn new_with_plan(
        grammar: &'a ValidatedGrammar,
        parser: &'a ParserGrammar,
        table: &'a ParseTable,
        runtime_plan: &'a RuntimeParserPlan,
    ) -> Result<Self, ReducedParseError> {
        Ok(Self {
            reduced: ReducedParser::new_with_plan(grammar, parser, table, runtime_plan)?,
            recovery_step_limit: None,
        })
    }

    /// Attach a reduced external scanner host for this first runtime slice.
    pub fn with_external_scanner(mut self, scanner: &'a dyn ReducedExternalScanner) -> Self {
        self.reduced = self.reduced.with_external_scanner(scanner);
        self
    }

    /// Bound recovery branch execution for latency-sensitive callers.
    pub fn with_recovery_step_limit(mut self, limit: usize) -> Self {
        self.recovery_step_limit = Some(limit);
        self
    }

    /// Parse one input and return runtime stack/tree evidence.
    pub fn parse_with_report(&self, input: &str) -> Result<RuntimeParseReport, ReducedParseError> {
        self.parse_with_report_mode(input, RuntimeRecoveryMode::Strict, RuntimeTraceDetail::Full)
    }

    /// Parse one input, retaining only trace events needed for accepted lineage filtering.
    pub fn parse_compact_with_report(
        &self,
        input: &str,
    ) -> Result<RuntimeParseReport, ReducedParseError> {
        self.parse_with_report_mode(
            input,
            RuntimeRecoveryMode::Strict,
            RuntimeTraceDetail::Lineage,
        )
    }

    /// Parse one input, preserving recoverable error ranges as `ERROR` nodes.
    pub fn parse_recovering_with_report(
        &self,
        input: &str,
    ) -> Result<RuntimeParseReport, ReducedParseError> {
        self.parse_with_report_mode(
            input,
            RuntimeRecoveryMode::SkipInvalidInput,
            RuntimeTraceDetail::Full,
        )
    }

    /// Parse with recovery, retaining only trace events needed for accepted lineage filtering.
    pub fn parse_recovering_compact_with_report(
        &self,
        input: &str,
    ) -> Result<RuntimeParseReport, ReducedParseError> {
        self.parse_with_report_mode(
            input,
            RuntimeRecoveryMode::SkipInvalidInput,
            RuntimeTraceDetail::Lineage,
        )
    }

    fn parse_with_report_mode(
        &self,
        input: &str,
        recovery: RuntimeRecoveryMode,
        trace_detail: RuntimeTraceDetail,
    ) -> Result<RuntimeParseReport, ReducedParseError> {
        self.parse_with_report_mode_reuse(input, recovery, trace_detail, None)
    }

    fn parse_compact_with_reuse_index(
        &self,
        input: &str,
        reuse_index: &RuntimeReuseIndex,
    ) -> Result<RuntimeParseReport, ReducedParseError> {
        self.parse_with_report_mode_reuse(
            input,
            RuntimeRecoveryMode::Strict,
            RuntimeTraceDetail::Lineage,
            Some(reuse_index),
        )
    }

    fn parse_with_report_mode_reuse(
        &self,
        input: &str,
        recovery: RuntimeRecoveryMode,
        trace_detail: RuntimeTraceDetail,
        reuse_index: Option<&RuntimeReuseIndex>,
    ) -> Result<RuntimeParseReport, ReducedParseError> {
        self.reduced.clear_lex_cache();
        let mut tree_store = RuntimeTreeStore::default();
        let mut trace_events = Vec::new();
        let mut tree_events = Vec::new();
        push_full_runtime_trace(
            &mut trace_events,
            trace_detail,
            TraceEvent::ParseStart {
                id: TraceEventId::from_index(0),
                state: ParseStateId::from_index(0),
            },
        );
        let initial_state_enter_id = next_trace_id(&trace_events);
        push_full_runtime_trace(
            &mut trace_events,
            trace_detail,
            TraceEvent::StateEnter {
                id: initial_state_enter_id,
                version: StackVersionId::from_index(0),
                state: ParseStateId::from_index(0),
            },
        );
        let line_index = InputLineIndex::new(input);

        let initial_branch = RuntimeBranch {
            version: StackVersionId::from_index(0),
            stack: vec![RuntimeStackEntry {
                state: ParseStateId::from_index(0),
                fragment: None,
                extra: false,
                end_byte: 0,
            }],
            byte_position: 0,
            scanner_snapshot: None,
            error_cost: 0,
            trace: Vec::new(),
            tree_events: Vec::new(),
            reusable_nodes: Vec::new(),
        };
        let mut queued_recovery_costs = HashMap::new();
        if recovery == RuntimeRecoveryMode::SkipInvalidInput {
            queued_recovery_costs.insert(RuntimeBranchKey::from_branch(&initial_branch), 0);
        }
        let mut branches = VecDeque::from([initial_branch]);
        let mut accepted = Vec::<(
            StackVersionId,
            SexpNode,
            Vec<ReducedTraceStep>,
            u32,
            Vec<TreeEvent>,
            Vec<RuntimeReusableNode>,
        )>::new();
        let mut failures = Vec::<ReducedParseError>::new();
        let mut next_version_index = 1usize;
        let mut next_lookahead_index = 0usize;
        let mut step_count = 0usize;
        let step_limit = match recovery {
            RuntimeRecoveryMode::Strict => self.reduced.reduced_step_limit(input),
            RuntimeRecoveryMode::SkipInvalidInput => self
                .recovery_step_limit
                .unwrap_or_else(|| self.reduced.reduced_recovery_step_limit(input)),
        };
        let mut max_live_versions = branches.len();

        while let Some(branch) = branches.pop_front() {
            if recovery == RuntimeRecoveryMode::SkipInvalidInput {
                let key = RuntimeBranchKey::from_branch(&branch);
                if queued_recovery_costs
                    .get(&key)
                    .is_some_and(|best_cost| branch.error_cost > *best_cost)
                {
                    push_full_runtime_trace(
                        &mut trace_events,
                        trace_detail,
                        TraceEvent::GlrRetire {
                            version: branch.version,
                            reason: BranchRetireReason::Dominated,
                        },
                    );
                    continue;
                }
            }
            step_count += 1;
            if step_count > step_limit {
                push_full_runtime_trace(
                    &mut trace_events,
                    trace_detail,
                    TraceEvent::GlrRetire {
                        version: branch.version,
                        reason: BranchRetireReason::Limit,
                    },
                );
                trace_events.push(TraceEvent::ParseFinish {
                    id: next_trace_id(&trace_events),
                    outcome: ParseOutcome::Failed,
                });
                return Err(
                    ReducedParseError::new(ReducedParseErrorKind::BranchStepLimit {
                        limit: step_limit,
                    })
                    .with_trace(branch.trace),
                );
            }

            for outcome in self.step_runtime_branch(
                branch,
                input,
                &line_index,
                &mut tree_store,
                &mut trace_events,
                &mut tree_events,
                &mut next_version_index,
                &mut next_lookahead_index,
                recovery,
                trace_detail,
                reuse_index,
            ) {
                match outcome {
                    RuntimeStepOutcome::Branch(branch) => enqueue_runtime_branch(
                        branch,
                        recovery,
                        trace_detail,
                        &mut queued_recovery_costs,
                        &mut trace_events,
                        &mut branches,
                    ),
                    RuntimeStepOutcome::Accepted {
                        version,
                        node,
                        trace,
                        error_cost,
                        tree_events,
                        reusable_nodes,
                    } => {
                        push_full_runtime_trace(
                            &mut trace_events,
                            trace_detail,
                            TraceEvent::GlrRetire {
                                version,
                                reason: BranchRetireReason::Accepted,
                            },
                        );
                        accepted.push((
                            version,
                            node,
                            trace,
                            error_cost,
                            tree_events,
                            reusable_nodes,
                        ));
                    }
                    RuntimeStepOutcome::Failed { version, error } => {
                        push_full_runtime_trace(
                            &mut trace_events,
                            trace_detail,
                            TraceEvent::GlrRetire {
                                version,
                                reason: BranchRetireReason::NoAction,
                            },
                        );
                        failures.push(error);
                    }
                }
            }
            max_live_versions = max_live_versions.max(branches.len());
        }

        let Some(min_error_cost) = accepted
            .iter()
            .map(|(_, _, _, error_cost, _, _)| *error_cost)
            .min()
        else {
            trace_events.push(TraceEvent::ParseFinish {
                id: next_trace_id(&trace_events),
                outcome: ParseOutcome::Failed,
            });
            return Err(select_reduced_failure(failures).unwrap_or_else(|| {
                ReducedParseError::new(ReducedParseErrorKind::NoViableBranch { failure_count: 0 })
            }));
        };
        let best_accepted = accepted
            .iter()
            .filter(|(_, _, _, error_cost, _, _)| *error_cost == min_error_cost)
            .collect::<Vec<_>>();
        let Some((
            first_version,
            first_node,
            first_trace,
            _,
            first_tree_events,
            first_reusable_nodes,
        )) = best_accepted.first().map(|accepted| (*accepted).clone())
        else {
            unreachable!("accepted branches have a minimum recovery cost");
        };
        if best_accepted
            .iter()
            .all(|(_, node, _, _, _, _)| *node == first_node)
        {
            let accepted_tree_events = if trace_detail == RuntimeTraceDetail::Full {
                tree_events_for_version_lineage(first_version, &trace_events, &tree_events)
            } else {
                first_tree_events.clone()
            };
            let reusable_nodes =
                mark_reusable_nodes_with_errors(first_reusable_nodes, &accepted_tree_events);
            trace_events.push(TraceEvent::ParseFinish {
                id: next_trace_id(&trace_events),
                outcome: if min_error_cost == 0 {
                    ParseOutcome::Accepted
                } else {
                    ParseOutcome::Recovered
                },
            });
            return Ok(RuntimeParseReport {
                tree: first_node,
                tree_store,
                reusable_nodes,
                trace_events,
                tree_events: if trace_detail == RuntimeTraceDetail::Full {
                    tree_events
                } else {
                    first_tree_events
                },
                accepted_version: first_version,
                accepted_count: best_accepted.len(),
                failure_count: failures.len(),
                max_live_versions,
            });
        }

        trace_events.push(TraceEvent::ParseFinish {
            id: next_trace_id(&trace_events),
            outcome: ParseOutcome::Failed,
        });
        Err(
            ReducedParseError::new(ReducedParseErrorKind::AmbiguousParse {
                accepted_count: best_accepted.len(),
                accepted: best_accepted
                    .iter()
                    .map(|(_, node, _, _, _, _)| node.to_sexp())
                    .collect(),
            })
            .with_trace(first_trace),
        )
    }

    fn step_runtime_branch(
        &self,
        branch: RuntimeBranch,
        input: &str,
        line_index: &InputLineIndex,
        tree_store: &mut RuntimeTreeStore,
        trace_events: &mut Vec<TraceEvent>,
        tree_events: &mut Vec<TreeEvent>,
        next_version_index: &mut usize,
        next_lookahead_index: &mut usize,
        recovery: RuntimeRecoveryMode,
        trace_detail: RuntimeTraceDetail,
        reuse_index: Option<&RuntimeReuseIndex>,
    ) -> Vec<RuntimeStepOutcome> {
        let source_version = branch.version;
        let state = match branch.stack.last() {
            Some(entry) => entry.state,
            None => {
                return vec![RuntimeStepOutcome::Failed {
                    version: source_version,
                    error: ReducedParseError::new(ReducedParseErrorKind::EmptyStack)
                        .with_trace(branch.trace),
                }];
            }
        };
        let state_row = match self.reduced.parse_state(state) {
            Ok(state_row) => state_row,
            Err(error) => {
                return vec![RuntimeStepOutcome::Failed {
                    version: source_version,
                    error: error.with_trace(branch.trace),
                }];
            }
        };
        if let Some(reuse_index) = reuse_index
            && let Some(branch) = self.try_reuse_runtime_node(
                branch.clone(),
                reuse_index,
                input,
                line_index,
                tree_store,
                trace_events,
                tree_events,
                trace_detail,
            )
        {
            return vec![RuntimeStepOutcome::Branch(branch)];
        }
        let token = match self.reduced.lex(
            state_row,
            input,
            branch.byte_position,
            branch.scanner_snapshot,
        ) {
            Ok(token) => token,
            Err(error) => {
                if recovery == RuntimeRecoveryMode::SkipInvalidInput
                    && matches!(error.kind(), ReducedParseErrorKind::NoToken { .. })
                    && input[branch.byte_position..].starts_with(['{', '}'])
                    && let Some(branch) = self.recover_runtime_to_viable_stack(
                        branch.clone(),
                        input,
                        trace_events,
                        trace_detail,
                    )
                {
                    return vec![RuntimeStepOutcome::Branch(branch)];
                }
                if recovery == RuntimeRecoveryMode::SkipInvalidInput
                    && matches!(error.kind(), ReducedParseErrorKind::NoToken { .. })
                    && let Some(branch) = self.recover_runtime_no_token(
                        branch.clone(),
                        input,
                        line_index,
                        tree_store,
                        trace_events,
                        tree_events,
                        trace_detail,
                    )
                {
                    return vec![RuntimeStepOutcome::Branch(branch)];
                }
                return vec![RuntimeStepOutcome::Failed {
                    version: source_version,
                    error: error.with_trace(branch.trace),
                }];
            }
        };
        let lookahead = LookaheadTokenId::from_index(*next_lookahead_index);
        *next_lookahead_index += 1;
        push_full_runtime_trace(
            trace_events,
            trace_detail,
            TraceEvent::Lex {
                version: source_version,
                mode: state_row.lex_mode(),
                lookahead,
            },
        );
        if let LookaheadSymbol::External(_) = token.lookahead {
            let mode = &self.reduced.table.lexical_modes()[state_row.lex_mode().get() as usize];
            if let Some(valid_symbols) = mode.valid_symbols() {
                let scanner = token.scanner;
                push_full_runtime_trace(
                    trace_events,
                    trace_detail,
                    TraceEvent::ExternalScanner {
                        version: source_version,
                        valid_symbols,
                        before: scanner.and_then(|scanner| scanner.before()),
                        after: scanner.and_then(|scanner| scanner.after()),
                        result: Some(lookahead),
                    },
                );
            }
        }
        let Some(entry) = state_row
            .entries()
            .iter()
            .find(|entry| entry.lookahead() == token.lookahead)
        else {
            if recovery == RuntimeRecoveryMode::SkipInvalidInput
                && let Some(branch) = self.recover_runtime_to_action_state(
                    branch.clone(),
                    token.lookahead,
                    trace_events,
                    trace_detail,
                )
            {
                return vec![RuntimeStepOutcome::Branch(branch)];
            }
            return vec![RuntimeStepOutcome::Failed {
                version: source_version,
                error: ReducedParseError::new(ReducedParseErrorKind::NoAction {
                    state,
                    lookahead: token.lookahead,
                    byte_position: branch.byte_position,
                })
                .with_trace(branch.trace),
            }];
        };

        if entry.actions().len() > 1 {
            let branches = (0..entry.actions().len())
                .map(|_| {
                    let version = StackVersionId::from_index(*next_version_index);
                    *next_version_index += 1;
                    version
                })
                .collect::<Vec<_>>();
            let conflict = self.conflict_id(state, token.lookahead, entry.actions());
            trace_events.push(TraceEvent::GlrSplit {
                source: source_version,
                conflict,
                branches: branches.clone(),
            });
            let action_count = entry.actions().len();
            let mut branch_slot = Some(branch);
            let mut outcomes = Vec::with_capacity(action_count);
            for (index, (action, version)) in entry.actions().iter().zip(branches).enumerate() {
                let mut branch = if index + 1 == action_count {
                    branch_slot
                        .take()
                        .expect("last split arm consumes source branch")
                } else {
                    branch_slot
                        .as_ref()
                        .expect("source branch remains until last split arm")
                        .clone()
                };
                branch.version = version;
                branch.trace.push(ReducedTraceStep {
                    state,
                    byte_position: branch.byte_position,
                    lookahead: token.lookahead,
                    action: *action,
                });
                outcomes.push(self.apply_runtime_action(
                    branch,
                    token,
                    lookahead,
                    *action,
                    input,
                    line_index,
                    tree_store,
                    trace_events,
                    tree_events,
                    trace_detail,
                ));
            }
            return outcomes;
        }

        let action = entry.actions()[0];
        let mut branch = branch;
        branch.trace.push(ReducedTraceStep {
            state,
            byte_position: branch.byte_position,
            lookahead: token.lookahead,
            action,
        });
        vec![self.apply_runtime_action(
            branch,
            token,
            lookahead,
            action,
            input,
            line_index,
            tree_store,
            trace_events,
            tree_events,
            trace_detail,
        )]
    }

    fn try_reuse_runtime_node(
        &self,
        mut branch: RuntimeBranch,
        reuse_index: &RuntimeReuseIndex,
        input: &str,
        line_index: &InputLineIndex,
        tree_store: &mut RuntimeTreeStore,
        trace_events: &mut Vec<TraceEvent>,
        tree_events: &mut Vec<TreeEvent>,
        trace_detail: RuntimeTraceDetail,
    ) -> Option<RuntimeBranch> {
        let entry_state = branch.stack.last().map(|entry| entry.state)?;
        let reusable =
            reuse_index.get(branch.byte_position, entry_state, branch.scanner_snapshot)?;
        let goto_state = self.reduced.goto_state(entry_state, reusable.symbol).ok()?;
        let node = tree_store.push(reusable.tree.clone());
        let (bytes, points) =
            input_ranges(input, line_index, reusable.start_byte, reusable.end_byte);
        let tree_event = TreeEvent::ReuseNode {
            version: branch.version,
            node,
            bytes,
            points,
            scanner_snapshot: reusable.entry_scanner_snapshot,
        };
        if trace_detail == RuntimeTraceDetail::Full {
            tree_events.push(tree_event.clone());
            trace_events.push(TraceEvent::Tree(tree_event));
        } else {
            branch.tree_events.push(tree_event);
        }
        push_full_runtime_trace(
            trace_events,
            trace_detail,
            TraceEvent::StateEnter {
                id: next_trace_id(trace_events),
                version: branch.version,
                state: goto_state,
            },
        );
        branch.stack.push(RuntimeStackEntry {
            state: goto_state,
            fragment: Some(RuntimeFragment::Node {
                node,
                start_byte: reusable.start_byte,
                end_byte: reusable.end_byte,
                lookahead_end_byte: reusable.lookahead_end_byte,
                start_scanner_snapshot: reusable.entry_scanner_snapshot,
            }),
            extra: false,
            end_byte: reusable.end_byte,
        });
        branch.byte_position = reusable.end_byte;
        branch.scanner_snapshot = reusable.exit_scanner_snapshot;
        branch.reusable_nodes.push(RuntimeReusableNode {
            node,
            tree: reusable.tree.clone(),
            symbol: reusable.symbol,
            entry_state,
            entry_scanner_snapshot: reusable.entry_scanner_snapshot,
            exit_scanner_snapshot: reusable.exit_scanner_snapshot,
            start_byte: reusable.start_byte,
            end_byte: reusable.end_byte,
            lookahead_end_byte: reusable.lookahead_end_byte,
            contains_error: false,
        });
        Some(branch)
    }

    fn apply_runtime_action(
        &self,
        mut branch: RuntimeBranch,
        token: ReducedToken,
        lookahead: LookaheadTokenId,
        action: ParseAction,
        input: &str,
        line_index: &InputLineIndex,
        tree_store: &mut RuntimeTreeStore,
        trace_events: &mut Vec<TraceEvent>,
        tree_events: &mut Vec<TreeEvent>,
        trace_detail: RuntimeTraceDetail,
    ) -> RuntimeStepOutcome {
        match action {
            ParseAction::Shift { state, .. } => {
                let start = branch.byte_position;
                let start_scanner_snapshot = branch.scanner_snapshot;
                branch.byte_position = token.end;
                branch.commit_scanner_snapshot(token);
                let (bytes, points) = input_ranges(input, line_index, start, token.end);
                let tree_event = TreeEvent::Token {
                    version: branch.version,
                    symbol: lookahead_parser_symbol(token.lookahead),
                    lookahead,
                    bytes,
                    points,
                    extra: false,
                    named: false,
                    keyword: KeywordStatus::Unchecked,
                };
                if trace_detail == RuntimeTraceDetail::Full {
                    tree_events.push(tree_event.clone());
                    trace_events.push(TraceEvent::Tree(tree_event));
                } else {
                    branch.tree_events.push(tree_event);
                }
                push_full_runtime_trace(
                    trace_events,
                    trace_detail,
                    TraceEvent::Shift {
                        version: branch.version,
                        lookahead,
                        state,
                    },
                );
                push_full_runtime_trace(
                    trace_events,
                    trace_detail,
                    TraceEvent::StateEnter {
                        id: next_trace_id(trace_events),
                        version: branch.version,
                        state,
                    },
                );
                branch.stack.push(RuntimeStackEntry {
                    state,
                    fragment: Some(RuntimeFragment::Hidden {
                        children: Vec::new(),
                        visible_nodes: Vec::new(),
                        start_byte: start,
                        end_byte: token.end,
                        lookahead_end_byte: token.inspected_end,
                        start_scanner_snapshot,
                    }),
                    extra: false,
                    end_byte: token.end,
                });
                RuntimeStepOutcome::Branch(branch)
            }
            ParseAction::ShiftExtra => {
                let start = branch.byte_position;
                let start_scanner_snapshot = branch.scanner_snapshot;
                branch.byte_position = token.end;
                branch.commit_scanner_snapshot(token);
                let fragment = if trace_detail == RuntimeTraceDetail::Full {
                    self.runtime_extra_fragment(
                        branch.version,
                        token.lookahead,
                        tree_store,
                        tree_events,
                        input,
                        line_index,
                        start,
                        token.end,
                        token.inspected_end,
                        start_scanner_snapshot,
                    )
                } else {
                    self.runtime_extra_fragment(
                        branch.version,
                        token.lookahead,
                        tree_store,
                        &mut branch.tree_events,
                        input,
                        line_index,
                        start,
                        token.end,
                        token.inspected_end,
                        start_scanner_snapshot,
                    )
                };
                if let Some(fragment) = fragment {
                    let Some(state) = branch.stack.last().map(|entry| entry.state) else {
                        return RuntimeStepOutcome::Failed {
                            version: branch.version,
                            error: ReducedParseError::new(ReducedParseErrorKind::EmptyStack)
                                .with_trace(branch.trace),
                        };
                    };
                    branch.stack.push(RuntimeStackEntry {
                        state,
                        fragment: Some(fragment),
                        extra: true,
                        end_byte: token.end,
                    });
                }
                RuntimeStepOutcome::Branch(branch)
            }
            ParseAction::Reduce {
                production,
                metadata,
                symbol,
                child_count,
                ..
            } => {
                let reduction = match if trace_detail == RuntimeTraceDetail::Full {
                    self.runtime_reduce_fragment(
                        branch.version,
                        production,
                        metadata,
                        child_count,
                        &mut branch.stack,
                        tree_store,
                        tree_events,
                        input,
                        line_index,
                        false,
                    )
                } else {
                    self.runtime_reduce_fragment(
                        branch.version,
                        production,
                        metadata,
                        child_count,
                        &mut branch.stack,
                        tree_store,
                        &mut branch.tree_events,
                        input,
                        line_index,
                        false,
                    )
                } {
                    Ok(reduction) => reduction,
                    Err(error) => {
                        return RuntimeStepOutcome::Failed {
                            version: branch.version,
                            error: error.with_trace(branch.trace),
                        };
                    }
                };
                push_full_runtime_trace(
                    trace_events,
                    trace_detail,
                    TraceEvent::Reduce {
                        version: branch.version,
                        production,
                        metadata,
                    },
                );
                let head_state = match branch.stack.last() {
                    Some(entry) => entry.state,
                    None => {
                        return RuntimeStepOutcome::Failed {
                            version: branch.version,
                            error: ReducedParseError::new(ReducedParseErrorKind::EmptyStack)
                                .with_trace(branch.trace),
                        };
                    }
                };
                let goto_state = match self.reduced.goto_state(head_state, symbol) {
                    Ok(state) => state,
                    Err(error) => {
                        return RuntimeStepOutcome::Failed {
                            version: branch.version,
                            error: error.with_trace(branch.trace),
                        };
                    }
                };
                let (_start_byte, end_byte) = reduction.fragment.byte_range();
                if let RuntimeFragment::Node {
                    node,
                    start_byte,
                    end_byte,
                    lookahead_end_byte,
                    start_scanner_snapshot,
                } = &reduction.fragment
                    && start_byte < end_byte
                {
                    branch.reusable_nodes.push(RuntimeReusableNode {
                        node: *node,
                        tree: tree_store.node(*node).clone(),
                        symbol,
                        entry_state: head_state,
                        entry_scanner_snapshot: *start_scanner_snapshot,
                        exit_scanner_snapshot: branch.scanner_snapshot,
                        start_byte: *start_byte,
                        end_byte: *end_byte,
                        lookahead_end_byte: *lookahead_end_byte,
                        contains_error: false,
                    });
                }
                branch.stack.push(RuntimeStackEntry {
                    state: goto_state,
                    fragment: Some(reduction.fragment),
                    extra: false,
                    end_byte,
                });
                for fragment in reduction.trailing_extras {
                    let (_, end_byte) = fragment.byte_range();
                    branch.stack.push(RuntimeStackEntry {
                        state: goto_state,
                        fragment: Some(fragment),
                        extra: true,
                        end_byte,
                    });
                }
                push_full_runtime_trace(
                    trace_events,
                    trace_detail,
                    TraceEvent::StateEnter {
                        id: next_trace_id(trace_events),
                        version: branch.version,
                        state: goto_state,
                    },
                );
                RuntimeStepOutcome::Branch(branch)
            }
            ParseAction::Accept {
                production,
                metadata,
                child_count,
                ..
            } => {
                if token.lookahead != LookaheadSymbol::Eof || branch.byte_position != input.len() {
                    return RuntimeStepOutcome::Failed {
                        version: branch.version,
                        error: ReducedParseError::new(ReducedParseErrorKind::TrailingInput {
                            byte_position: branch.byte_position,
                        })
                        .with_trace(branch.trace),
                    };
                }
                let reduction = match if trace_detail == RuntimeTraceDetail::Full {
                    self.runtime_reduce_fragment(
                        branch.version,
                        production,
                        metadata,
                        child_count,
                        &mut branch.stack,
                        tree_store,
                        tree_events,
                        input,
                        line_index,
                        true,
                    )
                } else {
                    self.runtime_reduce_fragment(
                        branch.version,
                        production,
                        metadata,
                        child_count,
                        &mut branch.stack,
                        tree_store,
                        &mut branch.tree_events,
                        input,
                        line_index,
                        true,
                    )
                } {
                    Ok(reduction) => reduction,
                    Err(error) => {
                        return RuntimeStepOutcome::Failed {
                            version: branch.version,
                            error: error.with_trace(branch.trace),
                        };
                    }
                };
                push_full_runtime_trace(
                    trace_events,
                    trace_detail,
                    TraceEvent::Reduce {
                        version: branch.version,
                        production,
                        metadata,
                    },
                );
                match reduction.fragment {
                    RuntimeFragment::Node {
                        node,
                        start_byte: _,
                        end_byte: _,
                        lookahead_end_byte: _,
                        ..
                    } => {
                        let root =
                            match self.finish_runtime_root(node, &mut branch.stack, tree_store) {
                                Ok(node) => node,
                                Err(error) => {
                                    return RuntimeStepOutcome::Failed {
                                        version: branch.version,
                                        error: error.with_trace(branch.trace),
                                    };
                                }
                            };
                        RuntimeStepOutcome::Accepted {
                            version: branch.version,
                            node: root,
                            trace: branch.trace,
                            error_cost: branch.error_cost,
                            tree_events: branch.tree_events,
                            reusable_nodes: branch.reusable_nodes,
                        }
                    }
                    RuntimeFragment::Hidden { .. } => RuntimeStepOutcome::Failed {
                        version: branch.version,
                        error: ReducedParseError::new(ReducedParseErrorKind::AcceptedHiddenRoot)
                            .with_trace(branch.trace),
                    },
                }
            }
            ParseAction::Recover => {
                let state = branch
                    .stack
                    .last()
                    .map(|entry| entry.state)
                    .unwrap_or(ParseStateId::from_index(0));
                push_full_runtime_trace(
                    trace_events,
                    trace_detail,
                    TraceEvent::Recover {
                        version: branch.version,
                        state,
                    },
                );
                RuntimeStepOutcome::Failed {
                    version: branch.version,
                    error: ReducedParseError::new(ReducedParseErrorKind::UnsupportedRecovery {
                        state,
                    })
                    .with_trace(branch.trace),
                }
            }
        }
    }

    fn recover_runtime_to_viable_stack(
        &self,
        mut branch: RuntimeBranch,
        input: &str,
        trace_events: &mut Vec<TraceEvent>,
        trace_detail: RuntimeTraceDetail,
    ) -> Option<RuntimeBranch> {
        if self.reduced.external_scanner.is_some() || branch.stack.len() <= 1 {
            return None;
        }
        let original_len = branch.stack.len();
        for len in (1..original_len).rev() {
            let state = branch.stack[len - 1].state;
            let state_row = self.reduced.parse_state(state).ok()?;
            if self
                .reduced
                .lex(
                    state_row,
                    input,
                    branch.byte_position,
                    branch.scanner_snapshot,
                )
                .is_ok()
            {
                branch.stack.truncate(len);
                branch.error_cost = branch
                    .error_cost
                    .saturating_add(u32::try_from(original_len - len).unwrap_or(u32::MAX));
                push_full_runtime_trace(
                    trace_events,
                    trace_detail,
                    TraceEvent::Recover {
                        version: branch.version,
                        state,
                    },
                );
                return Some(branch);
            }
        }
        None
    }

    fn recover_runtime_to_action_state(
        &self,
        mut branch: RuntimeBranch,
        lookahead: LookaheadSymbol,
        trace_events: &mut Vec<TraceEvent>,
        trace_detail: RuntimeTraceDetail,
    ) -> Option<RuntimeBranch> {
        let original_len = branch.stack.len();
        if original_len <= 1 {
            return None;
        }
        for len in (1..original_len).rev() {
            let state = branch.stack[len - 1].state;
            let state_row = self.reduced.parse_state(state).ok()?;
            if state_row
                .entries()
                .iter()
                .any(|entry| entry.lookahead() == lookahead)
            {
                branch.stack.truncate(len);
                branch.error_cost = branch
                    .error_cost
                    .saturating_add(u32::try_from(original_len - len).unwrap_or(u32::MAX));
                push_full_runtime_trace(
                    trace_events,
                    trace_detail,
                    TraceEvent::Recover {
                        version: branch.version,
                        state,
                    },
                );
                return Some(branch);
            }
        }
        None
    }

    fn recover_runtime_no_token(
        &self,
        mut branch: RuntimeBranch,
        input: &str,
        line_index: &InputLineIndex,
        tree_store: &mut RuntimeTreeStore,
        trace_events: &mut Vec<TraceEvent>,
        tree_events: &mut Vec<TreeEvent>,
        trace_detail: RuntimeTraceDetail,
    ) -> Option<RuntimeBranch> {
        let state = branch.stack.last().map(|entry| entry.state)?;
        let start_byte = branch.byte_position;
        let end_byte = runtime_recovery_end(input, start_byte)?;
        let node = tree_store.push(SexpNode {
            kind: "ERROR".to_owned(),
            children: Vec::new(),
        });
        let (bytes, points) = input_ranges(input, line_index, start_byte, end_byte);
        push_full_runtime_trace(
            trace_events,
            trace_detail,
            TraceEvent::Recover {
                version: branch.version,
                state,
            },
        );
        let tree_event = TreeEvent::Error {
            version: branch.version,
            node,
            bytes,
            points,
            error_cost: u32::try_from(end_byte - start_byte).unwrap_or(u32::MAX),
        };
        if trace_detail == RuntimeTraceDetail::Full {
            tree_events.push(tree_event);
        } else {
            branch.tree_events.push(tree_event);
        }
        branch.error_cost = branch
            .error_cost
            .saturating_add(u32::try_from(end_byte - start_byte).unwrap_or(u32::MAX));
        branch.stack.push(RuntimeStackEntry {
            state,
            fragment: Some(RuntimeFragment::Node {
                node,
                start_byte,
                end_byte,
                lookahead_end_byte: end_byte,
                start_scanner_snapshot: branch.scanner_snapshot,
            }),
            extra: true,
            end_byte,
        });
        branch.byte_position = end_byte;
        Some(branch)
    }

    fn runtime_reduce_fragment(
        &self,
        version: StackVersionId,
        production: ProductionId,
        metadata: ProductionMetadataId,
        child_count: usize,
        stack: &mut Vec<RuntimeStackEntry>,
        tree_store: &mut RuntimeTreeStore,
        tree_events: &mut Vec<TreeEvent>,
        input: &str,
        line_index: &InputLineIndex,
        include_trailing_extras: bool,
    ) -> Result<RuntimeReduction, ReducedParseError> {
        let production_row = &self.reduced.parser.productions[production.get() as usize];
        let metadata_row = &self.reduced.parser.production_metadata[metadata.get() as usize];
        let mut children = Vec::new();
        let mut visible_nodes = Vec::new();
        let mut trailing_extras = Vec::new();
        if !include_trailing_extras {
            while stack.last().is_some_and(|entry| entry.extra) {
                let entry = stack
                    .pop()
                    .ok_or_else(|| ReducedParseError::new(ReducedParseErrorKind::EmptyStack))?;
                let Some(fragment) = entry.fragment else {
                    return Err(ReducedParseError::new(ReducedParseErrorKind::EmptyStack));
                };
                trailing_extras.push(fragment);
            }
            trailing_extras.reverse();
        }
        let mut popped = Vec::new();
        let mut remaining_children = child_count;
        while remaining_children > 0 {
            let entry = stack
                .pop()
                .ok_or_else(|| ReducedParseError::new(ReducedParseErrorKind::EmptyStack))?;
            let Some(fragment) = entry.fragment else {
                return Err(ReducedParseError::new(ReducedParseErrorKind::EmptyStack));
            };
            if !entry.extra {
                remaining_children -= 1;
            }
            popped.push((entry.extra, fragment));
        }
        popped.reverse();
        let start_byte = popped
            .first()
            .map(|(_, fragment)| fragment.byte_range().0)
            .unwrap_or_else(|| stack.last().map(|entry| entry.end_byte).unwrap_or(0));
        let end_byte = popped
            .last()
            .map(|(_, fragment)| fragment.byte_range().1)
            .unwrap_or(start_byte);
        let lookahead_end_byte = popped
            .iter()
            .map(|(_, fragment)| fragment.lookahead_end_byte())
            .max()
            .unwrap_or(end_byte);
        let start_scanner_snapshot = popped
            .first()
            .map(|(_, fragment)| fragment.start_scanner_snapshot())
            .unwrap_or(None);
        let mut steps = production_row.steps().iter();
        let mut structural_index = 0usize;
        let mut field_events = Vec::new();
        for (extra, fragment) in popped {
            let alias_range = fragment.byte_range();
            let mut step_visible_nodes = fragment.visible_nodes().to_vec();
            let mut field_child = fragment.single_visible_node();
            let mut step_children = fragment.into_children(tree_store);
            if !extra {
                let Some(step) = steps.next() else {
                    return Err(ReducedParseError::new(ReducedParseErrorKind::EmptyStack));
                };
                if let (Some(alias), Some(named)) = (step.alias(), step.alias_named()) {
                    let alias_name = self.reduced.parser.aliases[alias.get() as usize]
                        .value
                        .clone();
                    if named {
                        if step_children.is_empty() {
                            step_children.push(SexpChild {
                                field: None,
                                value: SexpValue::Node(SexpNode {
                                    kind: alias_name.clone(),
                                    children: Vec::new(),
                                }),
                            });
                        } else {
                            for child in &mut step_children {
                                if let SexpValue::Node(node) = &mut child.value {
                                    node.kind.clone_from(&alias_name);
                                }
                            }
                        }
                    }
                    let alias_node = tree_store.push(SexpNode {
                        kind: alias_name,
                        children: Vec::new(),
                    });
                    let (bytes, points) =
                        input_ranges(input, line_index, alias_range.0, alias_range.1);
                    tree_events.push(TreeEvent::Alias {
                        version,
                        node: alias_node,
                        alias,
                        named,
                        structural_index,
                        bytes,
                        points,
                    });
                    if named {
                        field_child = Some(alias_node);
                        step_visible_nodes.clear();
                        step_visible_nodes.push(alias_node);
                    }
                }
                if let Some(field) = step.field() {
                    field_events.push((structural_index, field, field_child));
                }
                structural_index += 1;
            }
            visible_nodes.extend(step_visible_nodes);
            children.extend(step_children);
        }

        if let Some(public_node) = metadata_row.public_node() {
            let kind = self.reduced.parser.public_node_kinds[public_node.get() as usize]
                .name()
                .to_owned();
            let node = tree_store.push(SexpNode { kind, children });
            let (bytes, points) = input_ranges(input, line_index, start_byte, end_byte);
            let event = TreeEvent::Reduce {
                version,
                production,
                metadata,
                node,
                bytes,
                points,
            };
            tree_events.push(event);
            for (structural_index, field, child) in field_events {
                tree_events.push(TreeEvent::Field {
                    version,
                    node,
                    child,
                    field,
                    structural_index,
                });
            }
            Ok(RuntimeReduction {
                fragment: RuntimeFragment::Node {
                    node,
                    start_byte,
                    end_byte,
                    lookahead_end_byte,
                    start_scanner_snapshot,
                },
                trailing_extras,
            })
        } else {
            Ok(RuntimeReduction {
                fragment: RuntimeFragment::Hidden {
                    children,
                    visible_nodes,
                    start_byte,
                    end_byte,
                    lookahead_end_byte,
                    start_scanner_snapshot,
                },
                trailing_extras,
            })
        }
    }

    fn runtime_extra_fragment(
        &self,
        version: StackVersionId,
        lookahead: LookaheadSymbol,
        tree_store: &mut RuntimeTreeStore,
        tree_events: &mut Vec<TreeEvent>,
        input: &str,
        line_index: &InputLineIndex,
        start_byte: usize,
        end_byte: usize,
        lookahead_end_byte: usize,
        start_scanner_snapshot: Option<ScannerSnapshotId>,
    ) -> Option<RuntimeFragment> {
        let ReducedFragment::Node(node) = self.reduced.extra_fragment(lookahead)? else {
            return None;
        };
        let public_node = self
            .reduced
            .parser
            .public_node_kinds
            .iter()
            .find(|kind| kind.name() == node.kind)
            .map(PublicNodeKind::id);
        let node = tree_store.push(node);
        let (bytes, points) = input_ranges(input, line_index, start_byte, end_byte);
        tree_events.push(TreeEvent::CloseNode {
            version,
            node,
            public_node,
            bytes,
            points,
        });
        Some(RuntimeFragment::Node {
            node,
            start_byte,
            end_byte,
            lookahead_end_byte,
            start_scanner_snapshot,
        })
    }

    fn finish_runtime_root(
        &self,
        node: TreeNodeId,
        stack: &mut Vec<RuntimeStackEntry>,
        tree_store: &RuntimeTreeStore,
    ) -> Result<SexpNode, ReducedParseError> {
        let mut root = tree_store.node(node).clone();
        let mut leading_children = Vec::new();
        for entry in stack.drain(..) {
            match (entry.extra, entry.fragment) {
                (_, None) => {}
                (true, Some(fragment)) => {
                    leading_children.extend(fragment.into_children(tree_store));
                }
                (false, Some(_)) => {
                    return Err(ReducedParseError::new(
                        ReducedParseErrorKind::UnreducedStackEntry { state: entry.state },
                    ));
                }
            }
        }
        if !leading_children.is_empty() {
            leading_children.extend(root.children);
            root.children = leading_children;
        }
        Ok(root)
    }

    fn conflict_id(
        &self,
        state: ParseStateId,
        lookahead: LookaheadSymbol,
        actions: &[ParseAction],
    ) -> ConflictId {
        self.reduced
            .table
            .conflicts()
            .iter()
            .find(|conflict| {
                conflict.state() == state
                    && conflict.lookahead() == lookahead
                    && conflict.actions() == actions
            })
            .map(TableConflict::id)
            .unwrap_or_else(|| ConflictId::from_index(0))
    }
}

fn runtime_recovery_end(input: &str, start_byte: usize) -> Option<usize> {
    if start_byte >= input.len() {
        return None;
    }
    if input[start_byte..].starts_with(['{', '}']) {
        return None;
    }
    let mut end = start_byte;
    for ch in input[start_byte..].chars() {
        let previous_end = end;
        end += ch.len_utf8();
        if ch == '}' {
            return Some(if previous_end == start_byte {
                end
            } else {
                previous_end
            });
        }
        if matches!(ch, ';' | '\n') {
            return Some(end);
        }
    }
    (end > start_byte).then_some(end)
}

/// Runtime parse result with structured stack/tree evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeParseReport {
    tree: SexpNode,
    tree_store: RuntimeTreeStore,
    reusable_nodes: Vec<RuntimeReusableNode>,
    trace_events: Vec<TraceEvent>,
    tree_events: Vec<TreeEvent>,
    accepted_version: StackVersionId,
    accepted_count: usize,
    failure_count: usize,
    max_live_versions: usize,
}

impl RuntimeParseReport {
    /// Corpus-normalized accepted runtime tree.
    pub const fn tree(&self) -> &SexpNode {
        &self.tree
    }

    /// Structured parser trace events emitted during runtime execution.
    pub fn trace_events(&self) -> &[TraceEvent] {
        &self.trace_events
    }

    /// Runtime tree events emitted during runtime execution.
    pub fn tree_events(&self) -> &[TreeEvent] {
        &self.tree_events
    }

    /// Stack version whose accepted tree was returned as the corpus projection.
    pub const fn accepted_version(&self) -> StackVersionId {
        self.accepted_version
    }

    /// Tree events emitted by the accepted branch lineage.
    pub fn accepted_tree_events(&self) -> Vec<TreeEvent> {
        tree_events_for_version_lineage(
            self.accepted_version,
            &self.trace_events,
            &self.tree_events,
        )
    }

    /// Number of accepted runtime branches before identical-tree coalescing.
    pub const fn accepted_count(&self) -> usize {
        self.accepted_count
    }

    /// Number of branch failures observed while exploring the runtime table.
    pub const fn failure_count(&self) -> usize {
        self.failure_count
    }

    /// Maximum number of queued live runtime stack versions observed.
    pub const fn max_live_versions(&self) -> usize {
        self.max_live_versions
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RuntimeBranch {
    version: StackVersionId,
    stack: Vec<RuntimeStackEntry>,
    byte_position: usize,
    scanner_snapshot: Option<ScannerSnapshotId>,
    error_cost: u32,
    trace: Vec<ReducedTraceStep>,
    tree_events: Vec<TreeEvent>,
    reusable_nodes: Vec<RuntimeReusableNode>,
}

impl RuntimeBranch {
    fn commit_scanner_snapshot(&mut self, token: ReducedToken) {
        if let Some(scanner) = token.scanner {
            self.scanner_snapshot = scanner.after();
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct RuntimeBranchKey {
    byte_position: usize,
    scanner_snapshot: Option<ScannerSnapshotId>,
    stack: Vec<ParseStateId>,
}

impl RuntimeBranchKey {
    fn from_branch(branch: &RuntimeBranch) -> Self {
        Self {
            byte_position: branch.byte_position,
            scanner_snapshot: branch.scanner_snapshot,
            stack: branch.stack.iter().map(|entry| entry.state).collect(),
        }
    }
}

fn enqueue_runtime_branch(
    branch: RuntimeBranch,
    recovery: RuntimeRecoveryMode,
    trace_detail: RuntimeTraceDetail,
    queued_recovery_costs: &mut HashMap<RuntimeBranchKey, u32>,
    trace_events: &mut Vec<TraceEvent>,
    branches: &mut VecDeque<RuntimeBranch>,
) {
    if recovery == RuntimeRecoveryMode::Strict {
        branches.push_back(branch);
        return;
    }
    let key = RuntimeBranchKey::from_branch(&branch);
    match queued_recovery_costs.get(&key).copied() {
        Some(best_cost) if branch.error_cost > best_cost => {
            push_full_runtime_trace(
                trace_events,
                trace_detail,
                TraceEvent::GlrRetire {
                    version: branch.version,
                    reason: BranchRetireReason::Dominated,
                },
            );
        }
        Some(best_cost) if branch.error_cost == best_cost => push_full_runtime_trace(
            trace_events,
            trace_detail,
            TraceEvent::GlrRetire {
                version: branch.version,
                reason: BranchRetireReason::Dominated,
            },
        ),
        _ => {
            queued_recovery_costs.insert(key, branch.error_cost);
            branches.push_back(branch);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RuntimeStepOutcome {
    Branch(RuntimeBranch),
    Accepted {
        version: StackVersionId,
        node: SexpNode,
        trace: Vec<ReducedTraceStep>,
        error_cost: u32,
        tree_events: Vec<TreeEvent>,
        reusable_nodes: Vec<RuntimeReusableNode>,
    },
    Failed {
        version: StackVersionId,
        error: ReducedParseError,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RuntimeStackEntry {
    state: ParseStateId,
    fragment: Option<RuntimeFragment>,
    extra: bool,
    end_byte: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RuntimeFragment {
    Hidden {
        children: Vec<SexpChild>,
        visible_nodes: Vec<TreeNodeId>,
        start_byte: usize,
        end_byte: usize,
        lookahead_end_byte: usize,
        start_scanner_snapshot: Option<ScannerSnapshotId>,
    },
    Node {
        node: TreeNodeId,
        start_byte: usize,
        end_byte: usize,
        lookahead_end_byte: usize,
        start_scanner_snapshot: Option<ScannerSnapshotId>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RuntimeReduction {
    fragment: RuntimeFragment,
    trailing_extras: Vec<RuntimeFragment>,
}

impl RuntimeFragment {
    fn visible_nodes(&self) -> &[TreeNodeId] {
        match self {
            Self::Hidden { visible_nodes, .. } => visible_nodes,
            Self::Node { node, .. } => std::slice::from_ref(node),
        }
    }

    fn single_visible_node(&self) -> Option<TreeNodeId> {
        let visible_nodes = self.visible_nodes();
        if visible_nodes.len() == 1 {
            Some(visible_nodes[0])
        } else {
            None
        }
    }

    fn into_children(self, tree_store: &RuntimeTreeStore) -> Vec<SexpChild> {
        match self {
            Self::Hidden { children, .. } => children,
            Self::Node { node, .. } => vec![SexpChild {
                field: None,
                value: SexpValue::Node(tree_store.node(node).clone()),
            }],
        }
    }

    const fn byte_range(&self) -> (usize, usize) {
        match self {
            Self::Hidden {
                start_byte,
                end_byte,
                ..
            }
            | Self::Node {
                start_byte,
                end_byte,
                ..
            } => (*start_byte, *end_byte),
        }
    }

    const fn lookahead_end_byte(&self) -> usize {
        match self {
            Self::Hidden {
                lookahead_end_byte, ..
            }
            | Self::Node {
                lookahead_end_byte, ..
            } => *lookahead_end_byte,
        }
    }

    const fn start_scanner_snapshot(&self) -> Option<ScannerSnapshotId> {
        match self {
            Self::Hidden {
                start_scanner_snapshot,
                ..
            }
            | Self::Node {
                start_scanner_snapshot,
                ..
            } => *start_scanner_snapshot,
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct RuntimeTreeStore {
    nodes: Vec<SexpNode>,
}

impl RuntimeTreeStore {
    fn push(&mut self, node: SexpNode) -> TreeNodeId {
        let id = TreeNodeId::from_index(self.nodes.len());
        self.nodes.push(node);
        id
    }

    fn node(&self, id: TreeNodeId) -> &SexpNode {
        &self.nodes[id.get() as usize]
    }
}

fn next_trace_id(events: &[TraceEvent]) -> TraceEventId {
    TraceEventId::from_index(events.len())
}

pub(crate) fn tree_events_for_version_lineage(
    accepted_version: StackVersionId,
    trace_events: &[TraceEvent],
    tree_events: &[TreeEvent],
) -> Vec<TreeEvent> {
    let lineage = stack_version_lineage(accepted_version, trace_events);
    tree_events
        .iter()
        .filter(|event| lineage.contains(&event.version()))
        .cloned()
        .collect()
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

fn mark_reusable_nodes_with_errors(
    mut nodes: Vec<RuntimeReusableNode>,
    tree_events: &[TreeEvent],
) -> Vec<RuntimeReusableNode> {
    let error_ranges = tree_events
        .iter()
        .filter_map(|event| match event {
            TreeEvent::Error { bytes, .. } | TreeEvent::Missing { bytes, .. } => {
                Some((bytes.start().get() as usize, bytes.end().get() as usize))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    if error_ranges.is_empty() {
        return nodes;
    }
    for node in &mut nodes {
        node.contains_error = error_ranges.iter().any(|(start, end)| {
            node.start_byte <= *start && *end <= node.end_byte && *start < *end
        });
    }
    nodes
}

fn lookahead_parser_symbol(lookahead: LookaheadSymbol) -> ParserSymbol {
    match lookahead {
        LookaheadSymbol::Terminal(terminal) | LookaheadSymbol::ReservedWord { terminal, .. } => {
            ParserSymbol::Terminal(terminal)
        }
        LookaheadSymbol::External(external) => ParserSymbol::External(external),
        LookaheadSymbol::Eof => ParserSymbol::Eof,
        LookaheadSymbol::ErrorRecovery(internal) => ParserSymbol::Internal(internal),
    }
}

struct InputLineIndex {
    line_starts: Vec<usize>,
}

impl InputLineIndex {
    fn new(input: &str) -> Self {
        let mut line_starts = vec![0];
        for (byte, ch) in input.char_indices() {
            if ch == '\n' {
                line_starts.push(byte + ch.len_utf8());
            }
        }
        Self { line_starts }
    }

    fn point_at(&self, input: &str, byte: usize) -> PointBytes {
        let byte = byte.min(input.len());
        let row = self
            .line_starts
            .partition_point(|line_start| *line_start <= byte)
            .saturating_sub(1);
        let line_start = self.line_starts[row];
        PointBytes::new(
            Row::new(u32::try_from(row).expect("runtime row fits u32")),
            Utf8ColumnBytes::new(
                u32::try_from(byte - line_start).expect("runtime UTF-8 column fits u32"),
            ),
        )
    }
}

fn input_ranges(
    input: &str,
    line_index: &InputLineIndex,
    start: usize,
    end: usize,
) -> (ByteRange, PointRange) {
    let start = start.min(input.len());
    let end = end.min(input.len()).max(start);
    let bytes = ByteRange::new(
        ByteOffset::new(u32::try_from(start).expect("runtime byte offset fits u32")),
        ByteOffset::new(u32::try_from(end).expect("runtime byte offset fits u32")),
    )
    .expect("runtime byte range is ordered");
    let points = PointRange::new(
        line_index.point_at(input, start),
        line_index.point_at(input, end),
    )
    .expect("runtime point range is ordered");
    (bytes, points)
}

fn select_reduced_failure(failures: Vec<ReducedParseError>) -> Option<ReducedParseError> {
    let failure_count = failures.len();
    failures
        .into_iter()
        .max_by_key(|error| {
            error
                .trace
                .last()
                .map(|step| (step.byte_position, error.trace.len()))
                .unwrap_or((0, 0))
        })
        .or_else(|| {
            Some(ReducedParseError::new(
                ReducedParseErrorKind::NoViableBranch { failure_count },
            ))
        })
}

/// Error produced by the reduced parser slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReducedParseError {
    kind: ReducedParseErrorKind,
    trace: Vec<ReducedTraceStep>,
}

impl ReducedParseError {
    fn new(kind: ReducedParseErrorKind) -> Self {
        Self {
            kind,
            trace: Vec::new(),
        }
    }

    /// Error kind.
    pub const fn kind(&self) -> &ReducedParseErrorKind {
        &self.kind
    }

    fn with_trace(mut self, trace: Vec<ReducedTraceStep>) -> Self {
        self.trace = trace;
        self
    }

    /// Reduced parser trace collected before the failure.
    pub fn trace(&self) -> &[ReducedTraceStep] {
        &self.trace
    }
}

/// One selected action in the reduced parser trace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReducedTraceStep {
    /// Parse state before selecting the action.
    pub state: ParseStateId,
    /// Input byte offset before selecting the action.
    pub byte_position: usize,
    /// Lookahead selected by the lexical mode.
    pub lookahead: LookaheadSymbol,
    /// Action explored by the reduced parser branch.
    pub action: ParseAction,
}

impl fmt::Display for ReducedParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            ReducedParseErrorKind::WrongStage { stage } => {
                write!(f, "parser grammar is at stage {stage:?}, not Productions")
            }
            ReducedParseErrorKind::EmptyStack => write!(f, "reduced parser stack was empty"),
            ReducedParseErrorKind::MissingState { state } => {
                write!(f, "parse state {} is missing", state.get())
            }
            ReducedParseErrorKind::MissingLexMode { mode } => {
                write!(f, "lexical mode {} is missing", mode.get())
            }
            ReducedParseErrorKind::NoToken {
                state,
                byte_position,
                expected,
            } => write!(
                f,
                "state {} could not lex a token at byte {}; expected one of {:?}",
                state.get(),
                byte_position,
                expected
            ),
            ReducedParseErrorKind::NoAction {
                state,
                lookahead,
                byte_position,
            } => write!(
                f,
                "state {} has no action for {lookahead:?} at byte {}",
                state.get(),
                byte_position
            ),
            ReducedParseErrorKind::AmbiguousAction {
                state,
                lookahead,
                action_count,
            } => write!(
                f,
                "state {} has {} actions for {lookahead:?}",
                state.get(),
                action_count
            ),
            ReducedParseErrorKind::UnsupportedExternalScanner {
                state,
                external_count,
            } => write!(
                f,
                "state {} requires {} external scanner candidates",
                state.get(),
                external_count
            ),
            ReducedParseErrorKind::UnsupportedTerminal { terminal, spelling } => write!(
                f,
                "terminal {} is not supported by the reduced parser: {spelling}",
                terminal.get()
            ),
            ReducedParseErrorKind::UnsupportedLexicalSymbol { expr } => write!(
                f,
                "lexical expression {} contains a symbol reference unsupported by this slice",
                expr.get()
            ),
            ReducedParseErrorKind::MissingGoto { state, nonterminal } => write!(
                f,
                "state {} has no goto for nonterminal {}",
                state.get(),
                nonterminal.get()
            ),
            ReducedParseErrorKind::TrailingInput { byte_position } => {
                write!(f, "input remains after byte {byte_position}")
            }
            ReducedParseErrorKind::AcceptedHiddenRoot => {
                write!(f, "accepted parse did not produce a visible root node")
            }
            ReducedParseErrorKind::UnreducedStackEntry { state } => write!(
                f,
                "accepted parse left an unreduced stack entry in state {}",
                state.get()
            ),
            ReducedParseErrorKind::UnsupportedRecovery { state } => {
                write!(f, "state {} requires recovery", state.get())
            }
            ReducedParseErrorKind::NoViableBranch { failure_count } => {
                write!(
                    f,
                    "all reduced parser branches failed ({failure_count} failures)"
                )
            }
            ReducedParseErrorKind::BranchStepLimit { limit } => {
                write!(f, "reduced parser exceeded branch step limit {limit}")
            }
            ReducedParseErrorKind::InvalidInputEdit {
                start_byte,
                old_end_byte,
                new_end_byte,
                old_input_len,
                new_input_len,
            } => write!(
                f,
                "invalid input edit start={start_byte} old_end={old_end_byte} new_end={new_end_byte} for old input length {old_input_len} and new input length {new_input_len}"
            ),
            ReducedParseErrorKind::AmbiguousParse {
                accepted_count,
                accepted,
            } => write!(
                f,
                "reduced parser accepted {accepted_count} different reduced trees: {accepted:?}"
            ),
        }
    }
}

impl Error for ReducedParseError {}

/// Reduced parser error kind.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ReducedParseErrorKind {
    /// Parser grammar is not in the required stage.
    WrongStage {
        /// Current stage.
        stage: ParserGenerationStage,
    },
    /// Runtime stack became empty.
    EmptyStack,
    /// Parse state id was missing.
    MissingState {
        /// Missing state.
        state: ParseStateId,
    },
    /// Lexical mode id was missing.
    MissingLexMode {
        /// Missing lexical mode.
        mode: LexModeId,
    },
    /// No token candidate matched the input.
    NoToken {
        /// Current state.
        state: ParseStateId,
        /// Current byte offset.
        byte_position: usize,
        /// Terminal spellings accepted by the state's lexical mode.
        expected: Vec<String>,
    },
    /// No parse action existed for a lookahead.
    NoAction {
        /// Current state.
        state: ParseStateId,
        /// Lookahead.
        lookahead: LookaheadSymbol,
        /// Current byte offset.
        byte_position: usize,
    },
    /// The action cell requires GLR/conflict support.
    AmbiguousAction {
        /// Current state.
        state: ParseStateId,
        /// Lookahead.
        lookahead: LookaheadSymbol,
        /// Number of actions in the cell.
        action_count: usize,
    },
    /// The current state needs external scanner execution.
    UnsupportedExternalScanner {
        /// Current state.
        state: ParseStateId,
        /// External scanner candidates in the state.
        external_count: usize,
    },
    /// The reduced parser cannot match this terminal.
    UnsupportedTerminal {
        /// Terminal id.
        terminal: TerminalId,
        /// Terminal spelling.
        spelling: String,
    },
    /// The reduced lexical evaluator does not execute symbol references.
    UnsupportedLexicalSymbol {
        /// Source expression id.
        expr: GrammarExprId,
    },
    /// No goto entry existed for a reduction.
    MissingGoto {
        /// State after popping reduced children.
        state: ParseStateId,
        /// Reduced nonterminal.
        nonterminal: NonterminalId,
    },
    /// Accept was reached before all bytes were consumed.
    TrailingInput {
        /// Remaining byte offset.
        byte_position: usize,
    },
    /// Accept did not produce a visible root.
    AcceptedHiddenRoot,
    /// Accept left a non-extra stack entry outside the accepted root.
    UnreducedStackEntry {
        /// State carried by the leftover stack entry.
        state: ParseStateId,
    },
    /// Recovery is outside this reduced parser slice.
    UnsupportedRecovery {
        /// Current state.
        state: ParseStateId,
    },
    /// No branch accepted the input.
    NoViableBranch {
        /// Number of branch failures observed.
        failure_count: usize,
    },
    /// Reduced branch execution exceeded its guard.
    BranchStepLimit {
        /// Step limit that was exceeded.
        limit: usize,
    },
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
    /// More than one distinct reduced tree was accepted.
    AmbiguousParse {
        /// Number of accepted branches.
        accepted_count: usize,
        /// Accepted reduced S-expression projections.
        accepted: Vec<String>,
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

    /// Runtime progress count used as a stable branch-ranking tiebreaker.
    pub const fn progress(&self) -> u32 {
        self.progress
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
        /// Stack version that emitted this tree event.
        version: StackVersionId,
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
        /// Runtime tree node.
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
        /// Stack version that emitted this tree event.
        version: StackVersionId,
        /// Runtime tree node.
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
        /// Stack version that emitted this tree event.
        version: StackVersionId,
        /// Parent runtime tree node.
        node: TreeNodeId,
        /// Visible child runtime tree node, when the fielded step emitted one.
        child: Option<TreeNodeId>,
        /// Field id.
        field: FieldId,
        /// Structural child index.
        structural_index: usize,
    },
    /// An alias emitted or renamed a runtime tree node at a structural child index.
    Alias {
        /// Stack version that emitted this tree event.
        version: StackVersionId,
        /// Runtime tree node carrying the alias.
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
    use super::*;
    use crate::{
        corpus::{SexpChild, SexpNode, SexpValue},
        grammar::RawGrammarJson,
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

    fn authored_gingembre_runtime_fixture() -> (ValidatedGrammar, ParserGrammar, ParseTable) {
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

    fn gingembre_named_projection(input: &str) -> SexpNode {
        let parse = gingembre_syntax::parse(input);
        assert!(
            parse.errors.is_empty(),
            "gingembre parser reported errors for {input:?}: {:?}",
            parse.errors
        );
        let root = gingembre_node_projection(parse.syntax()).unwrap();
        root
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
                gingembre_node_projection(&child).map(|node| SexpChild {
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

    fn assert_styx_authored_gingembre_runtime(input: &str, expected_sexp: &str) {
        let (validated, parser, table) = authored_gingembre_runtime_fixture();
        let report = RuntimeParser::new(&validated, &parser, &table)
            .unwrap()
            .parse_with_report(input)
            .unwrap();
        let expected = gingembre_named_projection(input);

        rediff::assert_same!(report.tree(), &expected);
        assert_eq!(expected.to_sexp(), expected_sexp);
        assert_eq!(report.accepted_count(), 1);
        assert_eq!(report.failure_count(), 0);
    }

    fn assert_styx_authored_gingembre_rejects_like_gingembre(input: &str) {
        let gingembre = gingembre_syntax::parse(input);
        assert!(
            !gingembre.errors.is_empty(),
            "gingembre unexpectedly accepted {input:?}"
        );

        let (validated, parser, table) = authored_gingembre_runtime_fixture();
        let result = RuntimeParser::new(&validated, &parser, &table)
            .unwrap()
            .parse_with_report(input);
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
        assert_styx_authored_gingembre_runtime("{{ x }}", "(template (interpolation (var_ref)))");
    }

    #[test]
    fn parses_styx_authored_gingembre_trim_interpolation_like_gingembre() {
        assert_styx_authored_gingembre_runtime("{{- x -}}", "(template (interpolation (var_ref)))");
    }

    #[test]
    fn parses_styx_authored_gingembre_literals_like_gingembre() {
        assert_styx_authored_gingembre_runtime(
            "{{ true }}",
            "(template (interpolation (literal)))",
        );
        assert_styx_authored_gingembre_runtime(
            "{{ none }}",
            "(template (interpolation (literal)))",
        );
        assert_styx_authored_gingembre_runtime("{{ 42 }}", "(template (interpolation (literal)))");
        assert_styx_authored_gingembre_runtime(
            "{{ 1.25 }}",
            "(template (interpolation (literal)))",
        );
        assert_styx_authored_gingembre_runtime(
            r#"{{ "x" }}"#,
            "(template (interpolation (literal)))",
        );
        assert_styx_authored_gingembre_runtime("{{ 'x' }}", "(template (interpolation (literal)))");
        assert_styx_authored_gingembre_runtime(
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
        assert_styx_authored_gingembre_runtime(
            "{{ user.name }}",
            "(template (interpolation (field_expr (var_ref))))",
        );
    }

    #[test]
    fn parses_styx_authored_gingembre_call_arguments_like_gingembre() {
        assert_styx_authored_gingembre_runtime(
            "{{ greet(user.name, suffix) }}",
            "(template (interpolation (call_expr (var_ref) (arg_list (arg (field_expr (var_ref))) (arg (var_ref))))))",
        );
    }

    #[test]
    fn parses_styx_authored_gingembre_call_arg_shapes_like_gingembre() {
        assert_styx_authored_gingembre_runtime(
            "{{ greet() }}",
            "(template (interpolation (call_expr (var_ref) (arg_list))))",
        );
        assert_styx_authored_gingembre_runtime(
            "{{ greet(suffix,) }}",
            "(template (interpolation (call_expr (var_ref) (arg_list (arg (var_ref))))))",
        );
        assert_styx_authored_gingembre_runtime(
            "{{ greet(name=user.name) }}",
            "(template (interpolation (call_expr (var_ref) (arg_list (kw_arg (field_expr (var_ref)))))))",
        );
    }

    #[test]
    fn parses_styx_authored_gingembre_index_postfix_like_gingembre() {
        assert_styx_authored_gingembre_runtime(
            "{{ items[1] }}",
            "(template (interpolation (index_expr (var_ref) (literal))))",
        );
        assert_styx_authored_gingembre_runtime(
            "{{ items[1].name }}",
            "(template (interpolation (field_expr (index_expr (var_ref) (literal)))))",
        );
        assert_styx_authored_gingembre_runtime(
            "{{ fetch()[0] }}",
            "(template (interpolation (index_expr (call_expr (var_ref) (arg_list)) (literal))))",
        );
    }

    #[test]
    fn parses_styx_authored_gingembre_optional_postfix_like_gingembre() {
        assert_styx_authored_gingembre_runtime(
            "{{ user? }}",
            "(template (interpolation (optional_expr (var_ref))))",
        );
        assert_styx_authored_gingembre_runtime(
            "{{ user?.name }}",
            "(template (interpolation (field_expr (optional_expr (var_ref)))))",
        );
        assert_styx_authored_gingembre_runtime(
            "{{ fetch()[0]? | default(none) }}",
            "(template (interpolation (filter_expr (optional_expr (index_expr (call_expr (var_ref) (arg_list)) (literal))) (arg_list (arg (literal))))))",
        );
    }

    #[test]
    fn parses_styx_authored_gingembre_compound_primaries_like_gingembre() {
        assert_styx_authored_gingembre_runtime(
            "{{ (a + b) * c }}",
            "(template (interpolation (binary_expr (paren_expr (binary_expr (var_ref) (var_ref))) (var_ref))))",
        );
        assert_styx_authored_gingembre_runtime(
            "{{ [1, user.name, none,] }}",
            "(template (interpolation (list_lit (literal) (field_expr (var_ref)) (literal))))",
        );
        assert_styx_authored_gingembre_runtime("{{ [] }}", "(template (interpolation (list_lit)))");
        assert_styx_authored_gingembre_runtime("{{ {} }}", "(template (interpolation (dict_lit)))");
        assert_styx_authored_gingembre_runtime(
            r#"{{ {"name": user.name, "ok": true,} }}"#,
            "(template (interpolation (dict_lit (literal) (field_expr (var_ref)) (literal) (literal))))",
        );
    }

    #[test]
    fn parses_styx_authored_gingembre_binary_precedence_like_gingembre() {
        assert_styx_authored_gingembre_runtime(
            "{{ a + b * c }}",
            "(template (interpolation (binary_expr (var_ref) (binary_expr (var_ref) (var_ref)))))",
        );
        assert_styx_authored_gingembre_runtime(
            "{{ a * b + c }}",
            "(template (interpolation (binary_expr (binary_expr (var_ref) (var_ref)) (var_ref))))",
        );
        assert_styx_authored_gingembre_runtime(
            "{{ a + b + c }}",
            "(template (interpolation (binary_expr (binary_expr (var_ref) (var_ref)) (var_ref))))",
        );
        assert_styx_authored_gingembre_runtime(
            "{{ a - b ~ c }}",
            "(template (interpolation (binary_expr (binary_expr (var_ref) (var_ref)) (var_ref))))",
        );
        assert_styx_authored_gingembre_runtime(
            "{{ a / b % c }}",
            "(template (interpolation (binary_expr (binary_expr (var_ref) (var_ref)) (var_ref))))",
        );
        assert_styx_authored_gingembre_runtime(
            "{{ a ** b ** c }}",
            "(template (interpolation (binary_expr (var_ref) (binary_expr (var_ref) (var_ref)))))",
        );
        assert_styx_authored_gingembre_runtime(
            "{{ a or b and c }}",
            "(template (interpolation (binary_expr (var_ref) (binary_expr (var_ref) (var_ref)))))",
        );
        assert_styx_authored_gingembre_runtime(
            "{{ a == b + c }}",
            "(template (interpolation (binary_expr (var_ref) (binary_expr (var_ref) (var_ref)))))",
        );
        assert_styx_authored_gingembre_runtime(
            "{{ a not in xs }}",
            "(template (interpolation (binary_expr (var_ref) (var_ref))))",
        );
    }

    #[test]
    fn parses_styx_authored_gingembre_filters_and_tests_like_gingembre() {
        assert_styx_authored_gingembre_runtime(
            "{{ name | upper }}",
            "(template (interpolation (filter_expr (var_ref))))",
        );
        assert_styx_authored_gingembre_runtime(
            "{{ items | slice(0, 2) }}",
            "(template (interpolation (filter_expr (var_ref) (arg_list (arg (literal)) (arg (literal))))))",
        );
        assert_styx_authored_gingembre_runtime(
            "{{ value | default(fallback) is not none }}",
            "(template (interpolation (test_expr (filter_expr (var_ref) (arg_list (arg (var_ref)))))))",
        );
        assert_styx_authored_gingembre_runtime(
            "{{ value is none or fallback }}",
            "(template (interpolation (binary_expr (test_expr (var_ref)) (var_ref))))",
        );
        assert_styx_authored_gingembre_runtime(
            "{{ a + b is sameas(c) }}",
            "(template (interpolation (test_expr (binary_expr (var_ref) (var_ref)) (arg_list (arg (var_ref))))))",
        );
        assert_styx_authored_gingembre_runtime(
            "{{ name | upper ** power }}",
            "(template (interpolation (binary_expr (filter_expr (var_ref)) (var_ref))))",
        );
    }

    #[test]
    fn parses_styx_authored_gingembre_unary_precedence_like_gingembre() {
        assert_styx_authored_gingembre_runtime(
            "{{ not a and b }}",
            "(template (interpolation (binary_expr (unary_expr (var_ref)) (var_ref))))",
        );
        assert_styx_authored_gingembre_runtime(
            "{{ not a == b }}",
            "(template (interpolation (unary_expr (binary_expr (var_ref) (var_ref)))))",
        );
        assert_styx_authored_gingembre_runtime(
            "{{ a * -b }}",
            "(template (interpolation (binary_expr (var_ref) (unary_expr (var_ref)))))",
        );
        assert_styx_authored_gingembre_runtime(
            "{{ -a ** b }}",
            "(template (interpolation (unary_expr (binary_expr (var_ref) (var_ref)))))",
        );
    }

    #[cfg(feature = "weavy-lowering")]
    #[test]
    fn parses_styx_authored_gingembre_call_arguments_through_weavy_runtime() {
        let input = "{{ greet(user.name, suffix) }}";
        let (validated, parser, table) = authored_gingembre_runtime_fixture();
        let runtime_report = RuntimeParser::new(&validated, &parser, &table)
            .unwrap()
            .parse_with_report(input)
            .unwrap();
        let plan = crate::lower::weavy::lower_reduced_parser(&parser, &table).unwrap();
        let weavy_report = crate::lower::weavy::parse_runtime_with_report(
            &plan, &validated, &parser, &table, input,
        )
        .unwrap();

        rediff::assert_same!(weavy_report.tree(), runtime_report.tree());
        rediff::assert_same!(weavy_report.tree(), &gingembre_named_projection(input));
        assert_eq!(
            weavy_report.tree().to_sexp(),
            "(template (interpolation (call_expr (var_ref) (arg_list (arg (field_expr (var_ref))) (arg (var_ref))))))"
        );
        assert_eq!(weavy_report.trace_events(), runtime_report.trace_events());
        assert_eq!(weavy_report.tree_events(), runtime_report.tree_events());
        assert!(weavy_report.stats().block_call_count > 0);
    }

    #[cfg(feature = "weavy-lowering")]
    #[test]
    fn parses_styx_authored_gingembre_index_postfix_through_weavy_runtime() {
        let input = "{{ items[1].name }}";
        let (validated, parser, table) = authored_gingembre_runtime_fixture();
        let runtime_report = RuntimeParser::new(&validated, &parser, &table)
            .unwrap()
            .parse_with_report(input)
            .unwrap();
        let plan = crate::lower::weavy::lower_reduced_parser(&parser, &table).unwrap();
        let weavy_report = crate::lower::weavy::parse_runtime_with_report(
            &plan, &validated, &parser, &table, input,
        )
        .unwrap();

        rediff::assert_same!(weavy_report.tree(), runtime_report.tree());
        rediff::assert_same!(weavy_report.tree(), &gingembre_named_projection(input));
        assert_eq!(
            weavy_report.tree().to_sexp(),
            "(template (interpolation (field_expr (index_expr (var_ref) (literal)))))"
        );
        assert_eq!(weavy_report.trace_events(), runtime_report.trace_events());
        assert_eq!(weavy_report.tree_events(), runtime_report.tree_events());
        assert!(weavy_report.stats().block_call_count > 0);
    }

    #[cfg(feature = "weavy-lowering")]
    #[test]
    fn parses_styx_authored_gingembre_optional_postfix_through_weavy_runtime() {
        let input = "{{ fetch()[0]? | default(none) }}";
        let (validated, parser, table) = authored_gingembre_runtime_fixture();
        let runtime_report = RuntimeParser::new(&validated, &parser, &table)
            .unwrap()
            .parse_with_report(input)
            .unwrap();
        let plan = crate::lower::weavy::lower_reduced_parser(&parser, &table).unwrap();
        let weavy_report = crate::lower::weavy::parse_runtime_with_report(
            &plan, &validated, &parser, &table, input,
        )
        .unwrap();

        rediff::assert_same!(weavy_report.tree(), runtime_report.tree());
        rediff::assert_same!(weavy_report.tree(), &gingembre_named_projection(input));
        assert_eq!(
            weavy_report.tree().to_sexp(),
            "(template (interpolation (filter_expr (optional_expr (index_expr (call_expr (var_ref) (arg_list)) (literal))) (arg_list (arg (literal))))))"
        );
        assert_eq!(weavy_report.trace_events(), runtime_report.trace_events());
        assert_eq!(weavy_report.tree_events(), runtime_report.tree_events());
        assert!(weavy_report.stats().block_call_count > 0);
    }

    #[cfg(feature = "weavy-lowering")]
    #[test]
    fn parses_styx_authored_gingembre_compound_primaries_through_weavy_runtime() {
        let input = r#"{{ {"name": user.name, "ok": true,} }}"#;
        let (validated, parser, table) = authored_gingembre_runtime_fixture();
        let runtime_report = RuntimeParser::new(&validated, &parser, &table)
            .unwrap()
            .parse_with_report(input)
            .unwrap();
        let plan = crate::lower::weavy::lower_reduced_parser(&parser, &table).unwrap();
        let weavy_report = crate::lower::weavy::parse_runtime_with_report(
            &plan, &validated, &parser, &table, input,
        )
        .unwrap();

        rediff::assert_same!(weavy_report.tree(), runtime_report.tree());
        rediff::assert_same!(weavy_report.tree(), &gingembre_named_projection(input));
        assert_eq!(
            weavy_report.tree().to_sexp(),
            "(template (interpolation (dict_lit (literal) (field_expr (var_ref)) (literal) (literal))))"
        );
        assert_eq!(weavy_report.trace_events(), runtime_report.trace_events());
        assert_eq!(weavy_report.tree_events(), runtime_report.tree_events());
        assert!(weavy_report.stats().block_call_count > 0);
    }

    #[cfg(feature = "weavy-lowering")]
    #[test]
    fn parses_styx_authored_gingembre_filters_and_tests_through_weavy_runtime() {
        let input = "{{ value | default(fallback) is not none }}";
        let (validated, parser, table) = authored_gingembre_runtime_fixture();
        let runtime_report = RuntimeParser::new(&validated, &parser, &table)
            .unwrap()
            .parse_with_report(input)
            .unwrap();
        let plan = crate::lower::weavy::lower_reduced_parser(&parser, &table).unwrap();
        let weavy_report = crate::lower::weavy::parse_runtime_with_report(
            &plan, &validated, &parser, &table, input,
        )
        .unwrap();

        rediff::assert_same!(weavy_report.tree(), runtime_report.tree());
        rediff::assert_same!(weavy_report.tree(), &gingembre_named_projection(input));
        assert_eq!(
            weavy_report.tree().to_sexp(),
            "(template (interpolation (test_expr (filter_expr (var_ref) (arg_list (arg (var_ref)))))))"
        );
        assert_eq!(weavy_report.trace_events(), runtime_report.trace_events());
        assert_eq!(weavy_report.tree_events(), runtime_report.tree_events());
        assert!(weavy_report.stats().block_call_count > 0);
    }

    #[cfg(feature = "weavy-lowering")]
    #[test]
    fn parses_styx_authored_gingembre_unary_precedence_through_weavy_runtime() {
        let input = "{{ not a == b }}";
        let (validated, parser, table) = authored_gingembre_runtime_fixture();
        let runtime_report = RuntimeParser::new(&validated, &parser, &table)
            .unwrap()
            .parse_with_report(input)
            .unwrap();
        let plan = crate::lower::weavy::lower_reduced_parser(&parser, &table).unwrap();
        let weavy_report = crate::lower::weavy::parse_runtime_with_report(
            &plan, &validated, &parser, &table, input,
        )
        .unwrap();

        rediff::assert_same!(weavy_report.tree(), runtime_report.tree());
        rediff::assert_same!(weavy_report.tree(), &gingembre_named_projection(input));
        assert_eq!(
            weavy_report.tree().to_sexp(),
            "(template (interpolation (unary_expr (binary_expr (var_ref) (var_ref)))))"
        );
        assert_eq!(weavy_report.trace_events(), runtime_report.trace_events());
        assert_eq!(weavy_report.tree_events(), runtime_report.tree_events());
        assert!(weavy_report.stats().block_call_count > 0);
    }

    #[cfg(feature = "weavy-lowering")]
    #[test]
    fn parses_styx_authored_gingembre_binary_precedence_through_weavy_runtime() {
        let input = "{{ a + b * c }}";
        let (validated, parser, table) = authored_gingembre_runtime_fixture();
        let runtime_report = RuntimeParser::new(&validated, &parser, &table)
            .unwrap()
            .parse_with_report(input)
            .unwrap();
        let plan = crate::lower::weavy::lower_reduced_parser(&parser, &table).unwrap();
        let weavy_report = crate::lower::weavy::parse_runtime_with_report(
            &plan, &validated, &parser, &table, input,
        )
        .unwrap();

        rediff::assert_same!(weavy_report.tree(), runtime_report.tree());
        rediff::assert_same!(weavy_report.tree(), &gingembre_named_projection(input));
        assert_eq!(
            weavy_report.tree().to_sexp(),
            "(template (interpolation (binary_expr (var_ref) (binary_expr (var_ref) (var_ref)))))"
        );
        assert_eq!(weavy_report.trace_events(), runtime_report.trace_events());
        assert_eq!(weavy_report.tree_events(), runtime_report.tree_events());
        assert!(weavy_report.stats().block_call_count > 0);
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
        assert_eq!(
            grammar.alias_sequences()[alias_sequence.get() as usize].entries()[0].named(),
            true
        );
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
                && item.lookahead().symbols() == &[LookaheadSymbol::Eof]
        }));
        assert!(table.item_sets()[0].items().iter().any(|item| {
            item.production() == ProductionId::from_index(1)
                && item.dot() == 0
                && item.lookahead().symbols() == &[LookaheadSymbol::Eof]
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
                && item.lookahead().symbols() == &[LookaheadSymbol::Terminal(semicolon)]
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
    fn compiled_lexer_matches_interpreted_terminal_matcher() {
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
        let reduced = ReducedParser::new(&validated, &parser, &table).unwrap();
        let input = "=> abc ab3bz yxy q";

        for terminal in parser.symbols.terminals() {
            let compiled = compile_lex_terminal(&validated, terminal);
            for byte_position in [0usize, 3, 7, 8, 14, 18] {
                let interpreted = reduced.match_terminal(terminal, input, byte_position);
                let compiled = reduced
                    .match_compiled_terminal(&compiled, input, byte_position)
                    .map(|result| result.map(|match_| match_.end));
                assert_eq!(
                    compiled,
                    interpreted,
                    "terminal `{}` at byte {byte_position}",
                    terminal.spelling()
                );
            }
        }
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

    fn reused_byte_ranges(report: &RuntimeParseReport) -> Vec<(usize, usize)> {
        report
            .tree_events()
            .iter()
            .filter_map(|event| match event {
                TreeEvent::ReuseNode { bytes, .. } => {
                    Some((bytes.start().get() as usize, bytes.end().get() as usize))
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn runtime_lexer_preserves_regex_flags() {
        let (validated, parser, table) = flagged_regex_fixture();
        let reduced = ReducedParser::new(&validated, &parser, &table).unwrap();
        let input = "ABCXYZ";

        for terminal in parser.symbols.terminals() {
            let compiled = compile_lex_terminal(&validated, terminal);
            for byte_position in [0usize, 3] {
                let interpreted = reduced.match_terminal(terminal, input, byte_position);
                let compiled = reduced
                    .match_compiled_terminal(&compiled, input, byte_position)
                    .map(|result| result.map(|match_| match_.end));
                assert_eq!(
                    compiled,
                    interpreted,
                    "terminal `{}` at byte {byte_position}",
                    terminal.spelling()
                );
            }
        }

        let report = RuntimeParser::new(&validated, &parser, &table)
            .unwrap()
            .parse_with_report(input)
            .unwrap();
        assert_eq!(
            report.tree().to_sexp(),
            "(source_file (insensitive) (wrapped))"
        );
        assert_eq!(report.accepted_count(), 1);
        assert_eq!(report.failure_count(), 0);
    }

    #[test]
    fn runtime_parser_can_reuse_compiled_runtime_plan() {
        let (validated, parser, table) = flagged_regex_fixture();
        let plan = RuntimeParserPlan::new(&validated, &parser, &table).unwrap();
        let input = "ABCXYZ";
        let fresh = RuntimeParser::new(&validated, &parser, &table)
            .unwrap()
            .parse_compact_with_report(input)
            .unwrap();
        let reused = RuntimeParser::new_with_plan(&validated, &parser, &table, &plan)
            .unwrap()
            .parse_compact_with_report(input)
            .unwrap();

        rediff::assert_same!(reused.tree(), fresh.tree());
        assert_eq!(reused.trace_events(), fresh.trace_events());
        assert_eq!(reused.tree_events(), fresh.tree_events());
    }

    #[test]
    fn runtime_parse_session_reparse_matches_full_parse_oracle() {
        let (validated, parser, table) = flagged_regex_fixture();
        let runtime = RuntimeParser::new(&validated, &parser, &table).unwrap();
        let mut session = RuntimeParseSession::new(runtime);
        let first = session.parse_compact("ABCXYZ").unwrap().clone();
        assert_eq!(
            first.tree().to_sexp(),
            "(source_file (insensitive) (wrapped))"
        );
        assert_eq!(session.last_input(), Some("ABCXYZ"));

        let edit = RuntimeInputEdit::new(0, 3, 3);
        let reparsed = session.reparse_compact(edit, "abcXYZ").unwrap().clone();
        let scratch = RuntimeParser::new(&validated, &parser, &table)
            .unwrap()
            .parse_compact_with_report("abcXYZ")
            .unwrap();

        rediff::assert_same!(reparsed.tree(), scratch.tree());
        assert!(
            reparsed
                .tree_events()
                .iter()
                .any(|event| matches!(event, TreeEvent::ReuseNode { .. })),
            "incremental reparse should reuse at least one accepted subtree"
        );
        assert_eq!(reused_byte_ranges(&reparsed), vec![(3, 6)]);
        assert_eq!(session.last_input(), Some("abcXYZ"));
    }

    #[test]
    fn runtime_parse_session_does_not_reuse_node_that_peeked_into_edit() {
        let (validated, parser, table) = flagged_regex_fixture();
        let runtime = RuntimeParser::new(&validated, &parser, &table).unwrap();
        let mut session = RuntimeParseSession::new(runtime);
        session.parse_compact("ABCXYZ").unwrap();

        let edit = RuntimeInputEdit::new(3, 6, 6);
        let reparsed = session.reparse_compact(edit, "ABCxyz").unwrap().clone();
        let scratch = RuntimeParser::new(&validated, &parser, &table)
            .unwrap()
            .parse_compact_with_report("ABCxyz")
            .unwrap();

        rediff::assert_same!(reparsed.tree(), scratch.tree());
        assert!(
            !reparsed
                .tree_events()
                .iter()
                .any(|event| matches!(event, TreeEvent::ReuseNode { .. })),
            "node that inspected the edit boundary must not be reused"
        );
    }

    #[test]
    fn runtime_parse_session_reuses_suffix_across_edited_extra() {
        let (validated, parser, table) = extra_comment_reuse_fixture();
        let runtime = RuntimeParser::new(&validated, &parser, &table).unwrap();
        let mut session = RuntimeParseSession::new(runtime);
        let first = session.parse_compact("a#old\nb").unwrap().clone();
        assert_eq!(
            first.tree().to_sexp(),
            "(source_file (left) (comment) (right))"
        );

        let edit = RuntimeInputEdit::new(2, 5, 5);
        let reparsed = session.reparse_compact(edit, "a#new\nb").unwrap().clone();
        let scratch = RuntimeParser::new(&validated, &parser, &table)
            .unwrap()
            .parse_compact_with_report("a#new\nb")
            .unwrap();

        rediff::assert_same!(reparsed.tree(), scratch.tree());
        assert_eq!(
            reparsed.tree().to_sexp(),
            "(source_file (left) (comment) (right))"
        );
        assert_eq!(reused_byte_ranges(&reparsed), vec![(0, 1), (6, 7)]);
    }

    #[test]
    fn runtime_parse_session_rejects_mismatched_edit_context() {
        let (validated, parser, table) = flagged_regex_fixture();
        let runtime = RuntimeParser::new(&validated, &parser, &table).unwrap();
        let mut session = RuntimeParseSession::new(runtime);
        session.parse_compact("ABCXYZ").unwrap();

        let error = session
            .reparse_compact(RuntimeInputEdit::new(0, 3, 3), "abcXYZZ")
            .unwrap_err();
        assert!(matches!(
            error.kind(),
            ReducedParseErrorKind::InvalidInputEdit { .. }
        ));
        assert_eq!(session.last_input(), Some("ABCXYZ"));
    }

    #[cfg(feature = "weavy-lowering")]
    #[test]
    fn runtime_lexer_preserves_regex_flags_through_weavy_runtime() {
        let (validated, parser, table) = flagged_regex_fixture();
        let input = "ABCXYZ";
        let runtime_report = RuntimeParser::new(&validated, &parser, &table)
            .unwrap()
            .parse_with_report(input)
            .unwrap();
        let plan = crate::lower::weavy::lower_reduced_parser(&parser, &table).unwrap();
        let weavy_report = crate::lower::weavy::parse_runtime_with_report(
            &plan, &validated, &parser, &table, input,
        )
        .unwrap();

        rediff::assert_same!(weavy_report.tree(), runtime_report.tree());
        assert_eq!(
            weavy_report.tree().to_sexp(),
            "(source_file (insensitive) (wrapped))"
        );
        assert_eq!(weavy_report.trace_events(), runtime_report.trace_events());
        assert_eq!(weavy_report.tree_events(), runtime_report.tree_events());
        assert!(weavy_report.stats().block_call_count > 0);
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
    fn runtime_lexer_resolves_symbol_references_inside_token() {
        let (validated, parser, table) = lexical_symbol_fixture();
        let report = RuntimeParser::new(&validated, &parser, &table)
            .unwrap()
            .parse_with_report("alpha:123")
            .unwrap();

        assert_eq!(report.tree().to_sexp(), "(source_file)");
        assert_eq!(report.accepted_count(), 1);
        assert_eq!(report.failure_count(), 0);
    }

    #[cfg(feature = "weavy-lowering")]
    #[test]
    fn runtime_lexer_resolves_symbol_references_inside_token_through_weavy_runtime() {
        let (validated, parser, table) = lexical_symbol_fixture();
        let input = "alpha:123";
        let runtime_report = RuntimeParser::new(&validated, &parser, &table)
            .unwrap()
            .parse_with_report(input)
            .unwrap();
        let plan = crate::lower::weavy::lower_reduced_parser(&parser, &table).unwrap();
        let weavy_report = crate::lower::weavy::parse_runtime_with_report(
            &plan, &validated, &parser, &table, input,
        )
        .unwrap();

        rediff::assert_same!(weavy_report.tree(), runtime_report.tree());
        assert_eq!(weavy_report.tree().to_sexp(), "(source_file)");
        assert_eq!(weavy_report.trace_events(), runtime_report.trace_events());
        assert_eq!(weavy_report.tree_events(), runtime_report.tree_events());
        assert!(weavy_report.stats().block_call_count > 0);
    }

    #[test]
    fn compiled_lexer_carries_precedence_inside_token_wrapper() {
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
    fn compiled_lexer_carries_implicit_precedence_through_symbol_reference() {
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

    #[test]
    fn lexical_primitives_parse_until_and_nested_tokens() {
        let (validated, parser, table) = lexical_primitives_fixture();
        let reduced = ReducedParser::new(&validated, &parser, &table).unwrap();
        let input = "hello {# outer {# inner #} done #}";

        for terminal in parser.symbols.terminals() {
            let compiled = compile_lex_terminal(&validated, terminal);
            for byte_position in [0usize, 6, 15, 26] {
                let interpreted = reduced.match_terminal(terminal, input, byte_position);
                let compiled = reduced
                    .match_compiled_terminal(&compiled, input, byte_position)
                    .map(|result| result.map(|match_| match_.end));
                assert_eq!(
                    compiled,
                    interpreted,
                    "terminal `{}` at byte {byte_position}",
                    terminal.spelling()
                );
            }
        }

        let report = RuntimeParser::new(&validated, &parser, &table)
            .unwrap()
            .parse_with_report(input)
            .unwrap();

        assert_eq!(report.tree().to_sexp(), "(source_file (text) (comment))");
        assert_eq!(report.accepted_count(), 1);
        assert_eq!(report.failure_count(), 0);
    }

    #[cfg(feature = "weavy-lowering")]
    #[test]
    fn lexical_primitives_parse_through_weavy_runtime() {
        let (validated, parser, table) = lexical_primitives_fixture();
        let input = "hello {# outer {# inner #} done #}";
        let runtime_report = RuntimeParser::new(&validated, &parser, &table)
            .unwrap()
            .parse_with_report(input)
            .unwrap();
        let plan = crate::lower::weavy::lower_reduced_parser(&parser, &table).unwrap();
        let weavy_report = crate::lower::weavy::parse_runtime_with_report(
            &plan, &validated, &parser, &table, input,
        )
        .unwrap();

        rediff::assert_same!(weavy_report.tree(), runtime_report.tree());
        assert_eq!(
            weavy_report.tree().to_sexp(),
            "(source_file (text) (comment))"
        );
        assert_eq!(weavy_report.trace_events(), runtime_report.trace_events());
        assert_eq!(weavy_report.tree_events(), runtime_report.tree_events());
        assert!(weavy_report.stats().block_call_count > 0);
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
    fn reduced_lexer_prefers_higher_implicit_precedence_for_equal_length_candidates() {
        let direct_string = ReducedTokenCandidate {
            lookahead: LookaheadSymbol::Terminal(TerminalId::from_index(0)),
            end: 1,
            inspected_end: 1,
            extra: false,
            external: false,
            immediate: false,
            literal: true,
            lexical_precedence: 0,
            implicit_precedence: 2,
            scanner: None,
        };
        let immediate_string = ReducedTokenCandidate {
            lookahead: LookaheadSymbol::Terminal(TerminalId::from_index(1)),
            end: 1,
            inspected_end: 1,
            extra: false,
            external: false,
            immediate: true,
            literal: false,
            lexical_precedence: 0,
            implicit_precedence: 3,
            scanner: None,
        };

        assert_eq!(
            reduced_candidate_order(immediate_string, direct_string),
            ReducedCandidateOrder::Greater
        );
        assert_eq!(
            reduced_candidate_order(direct_string, immediate_string),
            ReducedCandidateOrder::Less
        );
    }

    #[test]
    fn reduced_lexer_prefers_explicit_lexical_precedence_before_length() {
        let structured_line = ReducedTokenCandidate {
            lookahead: LookaheadSymbol::Terminal(TerminalId::from_index(0)),
            end: 1,
            inspected_end: 1,
            extra: false,
            external: false,
            immediate: false,
            literal: true,
            lexical_precedence: 0,
            implicit_precedence: 2,
            scanner: None,
        };
        let low_precedence_context = ReducedTokenCandidate {
            lookahead: LookaheadSymbol::Terminal(TerminalId::from_index(1)),
            end: 32,
            inspected_end: 32,
            extra: false,
            external: false,
            immediate: false,
            literal: false,
            lexical_precedence: -1,
            implicit_precedence: 0,
            scanner: None,
        };

        assert_eq!(
            reduced_candidate_order(structured_line, low_precedence_context),
            ReducedCandidateOrder::Greater
        );
        assert_eq!(
            reduced_candidate_order(low_precedence_context, structured_line),
            ReducedCandidateOrder::Less
        );
    }

    #[test]
    fn reduced_lexer_prefers_external_candidate_before_internal_precedence() {
        let internal_string = ReducedTokenCandidate {
            lookahead: LookaheadSymbol::Terminal(TerminalId::from_index(0)),
            end: 1,
            inspected_end: 1,
            extra: false,
            external: false,
            immediate: false,
            literal: true,
            lexical_precedence: 0,
            implicit_precedence: 2,
            scanner: None,
        };
        let external_token = ReducedTokenCandidate {
            lookahead: LookaheadSymbol::External(ExternalId::from_index(0)),
            end: 1,
            inspected_end: 1,
            extra: false,
            external: true,
            immediate: false,
            literal: true,
            lexical_precedence: 0,
            implicit_precedence: 0,
            scanner: None,
        };

        assert_eq!(
            reduced_candidate_order(external_token, internal_string),
            ReducedCandidateOrder::Greater
        );
        assert_eq!(
            reduced_candidate_order(internal_string, external_token),
            ReducedCandidateOrder::Less
        );
    }

    #[test]
    fn reduced_lexer_prefers_immediate_content_over_longer_extra() {
        let immediate_content = ReducedTokenCandidate {
            lookahead: LookaheadSymbol::Terminal(TerminalId::from_index(0)),
            end: 2,
            inspected_end: 2,
            extra: false,
            external: false,
            immediate: true,
            literal: false,
            lexical_precedence: 0,
            implicit_precedence: 3,
            scanner: None,
        };
        let comment_extra = ReducedTokenCandidate {
            lookahead: LookaheadSymbol::Terminal(TerminalId::from_index(1)),
            end: 8,
            inspected_end: 8,
            extra: true,
            external: false,
            immediate: false,
            literal: false,
            lexical_precedence: 0,
            implicit_precedence: 0,
            scanner: None,
        };

        assert_eq!(
            reduced_candidate_order(immediate_content, comment_extra),
            ReducedCandidateOrder::Greater
        );
        assert_eq!(
            reduced_candidate_order(comment_extra, immediate_content),
            ReducedCandidateOrder::Less
        );
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
