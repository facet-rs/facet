//! Validated Snark grammar facts derived from raw Tree-sitter grammar input.

use std::{error::Error, fmt};

use indexmap::IndexMap;

use crate::grammar::{
    LanguageName, PrecedenceEntryJson, PrecedenceValue as RawPrecedenceValue, RawAutoCloseRuleJson,
    RawGrammarJson, RawRuleJson, RuleName,
};

type OrderedMap<V> = IndexMap<String, V, std::hash::RandomState>;

/// Grammar after raw Tree-sitter symbols are resolved into Snark ids.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedGrammar {
    language: LanguageName,
    rules: Vec<RuleDecl>,
    rules_by_name: OrderedMap<RuleId>,
    start_rule: RuleId,
    expressions: Vec<GrammarExpr>,
    externals: Vec<ExternalTokenFact>,
    external_names: OrderedMap<ExternalTokenId>,
    fields: Vec<FieldName>,
    fields_by_name: OrderedMap<FieldId>,
    aliases: Vec<AliasDecl>,
    alias_by_value: OrderedMap<AliasId>,
    inline: Vec<RuleId>,
    supertypes: Vec<RuleId>,
    conflicts: Vec<Vec<RuleId>>,
    precedence_groups: Vec<Vec<PrecedenceEntry>>,
    word: Option<RuleId>,
    extras: Vec<GrammarExprId>,
    reserved_sets: Vec<ReservedSetDecl>,
    reserved_sets_by_name: OrderedMap<ReservedSetId>,
    visible_node_kinds: OrderedMap<VisibleNodeKind>,
}

impl ValidatedGrammar {
    /// Validate a raw Tree-sitter grammar into Snark grammar facts.
    pub fn from_raw(raw: &RawGrammarJson) -> Result<Self, GrammarValidationError> {
        let start_rule = raw
            .start_rule()
            .map(|(name, _)| name)
            .ok_or_else(|| GrammarValidationError::new(GrammarValidationErrorKind::NoRules))?;

        let mut rules = Vec::with_capacity(raw.rules.len());
        let mut rules_by_name = OrderedMap::default();
        for (index, (name, _)) in raw.rules.iter().enumerate() {
            let id = RuleId::from_index(index)?;
            rules_by_name.insert(name.as_str().to_owned(), id);
            rules.push(RuleDecl {
                id,
                name: name.clone(),
                expr: GrammarExprId(0),
                visible: is_visible_rule_name(name.as_str()),
            });
        }

        let mut external_names = OrderedMap::default();
        for (index, rule) in raw.externals.iter().enumerate() {
            let id = ExternalTokenId::from_index(index)?;
            let name = external_name(rule).map(str::to_owned);
            if let Some(name) = &name {
                external_names.insert(name.clone(), id);
            }
        }

        let mut reserved_sets_by_name = OrderedMap::default();
        for (index, (name, _)) in raw.reserved.iter().enumerate() {
            reserved_sets_by_name.insert(name.to_owned(), ReservedSetId::from_index(index)?);
        }

        let mut builder = ValidationBuilder {
            rules_by_name,
            external_names,
            reserved_sets_by_name,
            expressions: Vec::new(),
            fields: Vec::new(),
            fields_by_name: OrderedMap::default(),
            aliases: Vec::new(),
            alias_by_value: OrderedMap::default(),
            visible_node_kinds: OrderedMap::default(),
        };

        let inline = raw
            .inline
            .iter()
            .map(|name| builder.resolve_rule_ref(name, "inline"))
            .collect::<Result<Vec<_>, _>>()?;

        for rule in &mut rules {
            let raw_rule = raw.rule(rule.name.as_str()).ok_or_else(|| {
                GrammarValidationError::new(GrammarValidationErrorKind::UnknownSymbol {
                    name: rule.name.as_str().to_owned(),
                })
            })?;
            rule.expr = builder.lower_rule(raw_rule)?;
        }

        for rule in &rules {
            if rule.visible && !inline.contains(&rule.id) {
                builder.visible_node_kinds.insert(
                    rule.name.as_str().to_owned(),
                    VisibleNodeKind::Rule(rule.id),
                );
            }
        }

        let externals = raw
            .externals
            .iter()
            .enumerate()
            .map(|(index, rule)| {
                let id = ExternalTokenId::from_index(index)?;
                Ok(ExternalTokenFact {
                    id,
                    ordinal: ExternalTokenOrdinal(id.get()),
                    name: external_name(rule).map(str::to_owned),
                    declaration: builder.lower_external_declaration(rule)?,
                })
            })
            .collect::<Result<Vec<_>, GrammarValidationError>>()?;
        let extras = raw
            .extras
            .iter()
            .map(|rule| builder.lower_rule(rule))
            .collect::<Result<Vec<_>, _>>()?;
        let supertypes = raw
            .supertypes
            .iter()
            .map(|name| builder.resolve_rule_ref(name, "supertypes"))
            .collect::<Result<Vec<_>, _>>()?;
        let conflicts = raw
            .conflicts
            .iter()
            .map(|members| {
                members
                    .iter()
                    .map(|name| builder.resolve_rule_ref(name, "conflicts"))
                    .collect::<Result<Vec<_>, _>>()
            })
            .collect::<Result<Vec<_>, _>>()?;
        let precedence_groups = raw
            .precedences
            .iter()
            .map(|group| {
                group
                    .iter()
                    .map(|entry| builder.lower_precedence_entry(entry))
                    .collect::<Result<Vec<_>, _>>()
            })
            .collect::<Result<Vec<_>, _>>()?;
        let word = raw
            .word
            .as_ref()
            .map(|name| builder.resolve_rule_ref(name, "word"))
            .transpose()?;
        let reserved_sets = raw
            .reserved
            .iter()
            .map(|(name, rules)| {
                let id = builder.resolve_reserved_set(name)?;
                let entries = rules
                    .iter()
                    .map(|rule| builder.lower_rule(rule))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(ReservedSetDecl {
                    id,
                    name: name.to_owned(),
                    entries,
                })
            })
            .collect::<Result<Vec<_>, GrammarValidationError>>()?;

        let start_rule = *builder
            .rules_by_name
            .get(start_rule.as_str())
            .ok_or_else(|| {
                GrammarValidationError::new(GrammarValidationErrorKind::UnknownSymbol {
                    name: start_rule.as_str().to_owned(),
                })
            })?;

        Ok(Self {
            language: raw.language_name(),
            rules_by_name: builder.rules_by_name,
            start_rule,
            external_names: builder.external_names,
            rules,
            expressions: builder.expressions,
            externals,
            fields: builder.fields,
            fields_by_name: builder.fields_by_name,
            aliases: builder.aliases,
            alias_by_value: builder.alias_by_value,
            inline,
            supertypes,
            conflicts,
            precedence_groups,
            word,
            extras,
            reserved_sets,
            reserved_sets_by_name: builder.reserved_sets_by_name,
            visible_node_kinds: builder.visible_node_kinds,
        })
    }

    /// Language name.
    pub fn language_name(&self) -> &LanguageName {
        &self.language
    }

    /// Start rule id.
    pub fn start_rule(&self) -> RuleId {
        self.start_rule
    }

    /// Number of grammar rules.
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    /// Rule by id.
    pub fn rule(&self, id: RuleId) -> &RuleDecl {
        &self.rules[id.get() as usize]
    }

    /// Iterate rule declarations in source order.
    pub fn rules(&self) -> impl Iterator<Item = &RuleDecl> {
        self.rules.iter()
    }

    /// Resolve a rule name.
    pub fn resolve_rule(&self, name: &str) -> Option<RuleId> {
        self.rules_by_name.get(name).copied()
    }

    /// Number of external scanner tokens.
    pub fn external_count(&self) -> usize {
        self.externals.len()
    }

    /// External token facts.
    pub fn externals(&self) -> &[ExternalTokenFact] {
        &self.externals
    }

    /// Grammar expression by id.
    pub fn expr(&self, id: GrammarExprId) -> &GrammarExpr {
        &self.expressions[id.get() as usize]
    }

    /// Iterate grammar expressions in arena order.
    pub fn expressions(&self) -> impl Iterator<Item = (GrammarExprId, &GrammarExpr)> {
        self.expressions
            .iter()
            .enumerate()
            .map(|(index, expr)| (GrammarExprId(index as u32), expr))
    }

    /// Valid-symbol mask width for external scanner calls.
    pub fn external_valid_symbol_mask_width(&self) -> usize {
        self.externals.len()
    }

    /// Number of fields discovered in rule expressions.
    pub fn field_count(&self) -> usize {
        self.fields.len()
    }

    /// Field by id.
    pub fn field(&self, id: FieldId) -> &FieldName {
        &self.fields[id.get() as usize]
    }

    /// Iterate fields in id order.
    pub fn fields(&self) -> impl Iterator<Item = (FieldId, &FieldName)> {
        self.fields
            .iter()
            .enumerate()
            .map(|(index, field)| (FieldId(index as u32), field))
    }

    /// Number of named aliases discovered in rule expressions.
    pub fn alias_count(&self) -> usize {
        self.aliases.len()
    }

    /// Alias by id.
    pub fn alias(&self, id: AliasId) -> &AliasDecl {
        &self.aliases[id.get() as usize]
    }

    /// Iterate aliases in id order.
    pub fn aliases(&self) -> impl Iterator<Item = &AliasDecl> {
        self.aliases.iter()
    }

    /// Number of extra rule expressions.
    pub fn extra_count(&self) -> usize {
        self.extras.len()
    }

    /// Extra expressions.
    pub fn extras(&self) -> &[GrammarExprId] {
        &self.extras
    }

    /// Number of inline symbol declarations.
    pub fn inline_count(&self) -> usize {
        self.inline.len()
    }

    /// Inline declarations.
    pub fn inline(&self) -> &[RuleId] {
        &self.inline
    }

    /// Number of supertype declarations.
    pub fn supertype_count(&self) -> usize {
        self.supertypes.len()
    }

    /// Supertype declarations.
    pub fn supertypes(&self) -> &[RuleId] {
        &self.supertypes
    }

    /// Number of conflict sets.
    pub fn conflict_count(&self) -> usize {
        self.conflicts.len()
    }

    /// Conflict sets.
    pub fn conflicts(&self) -> &[Vec<RuleId>] {
        &self.conflicts
    }

    /// Number of precedence groups.
    pub fn precedence_group_count(&self) -> usize {
        self.precedence_groups.len()
    }

    /// Static precedence groups.
    pub fn precedence_groups(&self) -> &[Vec<PrecedenceEntry>] {
        &self.precedence_groups
    }

    /// Reserved-word set declarations.
    pub fn reserved_sets(&self) -> &[ReservedSetDecl] {
        &self.reserved_sets
    }

    /// Resolve a reserved-word set name.
    pub fn resolve_reserved_set(&self, name: &str) -> Option<ReservedSetId> {
        self.reserved_sets_by_name.get(name).copied()
    }

    /// Word-token symbol, if one was declared.
    pub fn word(&self) -> Option<RuleId> {
        self.word
    }

    /// Whether a visible corpus node kind is known to this grammar.
    pub fn has_visible_node_kind(&self, kind: &str) -> bool {
        self.visible_node_kinds.contains_key(kind)
    }

    /// Source of a visible corpus node kind.
    pub fn visible_node_kind(&self, kind: &str) -> Option<VisibleNodeKind> {
        self.visible_node_kinds.get(kind).copied()
    }

    /// Iterate visible node kind names derived from visible rules and named aliases.
    pub fn visible_node_kinds(&self) -> impl Iterator<Item = &str> {
        self.visible_node_kinds.keys().map(String::as_str)
    }
}

struct ValidationBuilder {
    rules_by_name: OrderedMap<RuleId>,
    external_names: OrderedMap<ExternalTokenId>,
    reserved_sets_by_name: OrderedMap<ReservedSetId>,
    expressions: Vec<GrammarExpr>,
    fields: Vec<FieldName>,
    fields_by_name: OrderedMap<FieldId>,
    aliases: Vec<AliasDecl>,
    alias_by_value: OrderedMap<AliasId>,
    visible_node_kinds: OrderedMap<VisibleNodeKind>,
}

impl ValidationBuilder {
    fn resolve_symbol(&self, name: &str) -> Result<SymbolRef, GrammarValidationError> {
        if let Some(id) = self.rules_by_name.get(name) {
            return Ok(SymbolRef::Rule(*id));
        }
        if let Some(id) = self.external_names.get(name) {
            return Ok(SymbolRef::External(*id));
        }
        Err(GrammarValidationError::new(
            GrammarValidationErrorKind::UnknownSymbol {
                name: name.to_owned(),
            },
        ))
    }

    fn resolve_rule_ref(
        &self,
        name: &str,
        context: &'static str,
    ) -> Result<RuleId, GrammarValidationError> {
        if let Some(id) = self.rules_by_name.get(name) {
            return Ok(*id);
        }
        if self.external_names.contains_key(name) {
            return Err(GrammarValidationError::new(
                GrammarValidationErrorKind::ExternalWhereRuleRequired {
                    name: name.to_owned(),
                    context,
                },
            ));
        }
        Err(GrammarValidationError::new(
            GrammarValidationErrorKind::UnknownRule {
                name: name.to_owned(),
                context,
            },
        ))
    }

    fn lower_external_declaration(
        &mut self,
        rule: &RawRuleJson,
    ) -> Result<ExternalTokenDecl, GrammarValidationError> {
        match rule {
            RawRuleJson::Symbol { name } => Ok(ExternalTokenDecl::Symbol { name: name.clone() }),
            RawRuleJson::String { value } => Ok(ExternalTokenDecl::StringToken {
                value: value.clone(),
            }),
            RawRuleJson::Pattern { value, flags } => Ok(ExternalTokenDecl::PatternToken {
                value: value.clone(),
                flags: flags.clone(),
            }),
            other => Err(GrammarValidationError::new(
                GrammarValidationErrorKind::UnsupportedExternalDeclaration {
                    kind: raw_rule_kind(other),
                },
            )),
        }
    }

    fn lower_rule(&mut self, rule: &RawRuleJson) -> Result<GrammarExprId, GrammarValidationError> {
        let expr = match rule {
            RawRuleJson::Alias {
                content,
                named,
                value,
            } => {
                let content = self.lower_rule(content)?;
                let alias = self.intern_alias(value, *named)?;
                GrammarExpr::Alias {
                    alias,
                    named: *named,
                    content,
                }
            }
            RawRuleJson::Blank => GrammarExpr::Blank,
            RawRuleJson::String { value } => GrammarExpr::StringToken(value.clone()),
            RawRuleJson::Pattern { value, flags } => GrammarExpr::PatternToken {
                value: value.clone(),
                flags: flags.clone(),
            },
            RawRuleJson::Until { markers } => GrammarExpr::Until {
                markers: markers.clone(),
            },
            RawRuleJson::Nested { open, close } => GrammarExpr::Nested {
                open: open.clone(),
                close: close.clone(),
            },
            RawRuleJson::AutoClose {
                tag,
                open,
                close,
                closed_by,
                open_node,
                close_node,
                tag_name_node,
                start_prefix,
                end_prefix,
                closed_by_tags,
                rules,
            } => GrammarExpr::AutoClose {
                tag: tag.clone(),
                open: open.clone(),
                close: close.clone(),
                closed_by: closed_by.clone(),
                open_node: open_node.clone(),
                close_node: close_node.clone(),
                tag_name_node: tag_name_node.clone(),
                start_prefix: start_prefix.clone(),
                end_prefix: end_prefix.clone(),
                closed_by_tags: closed_by_tags.clone(),
                rules: rules.iter().map(AutoCloseRule::from).collect(),
            },
            RawRuleJson::Symbol { name } => GrammarExpr::Symbol(self.resolve_symbol(name)?),
            RawRuleJson::Choice { members } => {
                let members = members
                    .iter()
                    .map(|member| self.lower_rule(member))
                    .collect::<Result<Vec<_>, _>>()?;
                GrammarExpr::Choice(members)
            }
            RawRuleJson::Field { name, content } => {
                let field = self.intern_field(name)?;
                let content = self.lower_rule(content)?;
                GrammarExpr::Field { field, content }
            }
            RawRuleJson::Seq { members } => {
                let members = members
                    .iter()
                    .map(|member| self.lower_rule(member))
                    .collect::<Result<Vec<_>, _>>()?;
                GrammarExpr::Seq(members)
            }
            RawRuleJson::Repeat { content } => GrammarExpr::Repeat(self.lower_rule(content)?),
            RawRuleJson::Repeat1 { content } => GrammarExpr::Repeat1(self.lower_rule(content)?),
            RawRuleJson::PrecDynamic { value, content } => GrammarExpr::PrecDynamic {
                value: *value,
                content: self.lower_rule(content)?,
            },
            RawRuleJson::PrecLeft { value, content } => GrammarExpr::Prec {
                assoc: PrecedenceAssoc::Left,
                value: StaticPrecedenceValue::from_raw(value),
                content: self.lower_rule(content)?,
            },
            RawRuleJson::PrecRight { value, content } => GrammarExpr::Prec {
                assoc: PrecedenceAssoc::Right,
                value: StaticPrecedenceValue::from_raw(value),
                content: self.lower_rule(content)?,
            },
            RawRuleJson::Prec { value, content } => GrammarExpr::Prec {
                assoc: PrecedenceAssoc::None,
                value: StaticPrecedenceValue::from_raw(value),
                content: self.lower_rule(content)?,
            },
            RawRuleJson::Token { content } => GrammarExpr::Token(self.lower_rule(content)?),
            RawRuleJson::ImmediateToken { content } => {
                GrammarExpr::ImmediateToken(self.lower_rule(content)?)
            }
            RawRuleJson::Reserved {
                context_name,
                content,
            } => {
                let context = self.resolve_reserved_set(context_name)?;
                GrammarExpr::Reserved {
                    context,
                    content: self.lower_rule(content)?,
                }
            }
        };
        self.push_expr(expr)
    }

    fn resolve_reserved_set(&self, name: &str) -> Result<ReservedSetId, GrammarValidationError> {
        self.reserved_sets_by_name
            .get(name)
            .copied()
            .ok_or_else(|| {
                GrammarValidationError::new(GrammarValidationErrorKind::UnknownReservedContext {
                    name: name.to_owned(),
                })
            })
    }

    fn lower_precedence_entry(
        &self,
        entry: &PrecedenceEntryJson,
    ) -> Result<PrecedenceEntry, GrammarValidationError> {
        match entry {
            PrecedenceEntryJson::Name(name) => Ok(PrecedenceEntry::Name(name.clone())),
            PrecedenceEntryJson::Symbol(symbol) => {
                if symbol.kind != "SYMBOL" {
                    return Err(GrammarValidationError::new(
                        GrammarValidationErrorKind::InvalidPrecedenceSymbolKind {
                            kind: symbol.kind.clone(),
                        },
                    ));
                }
                Ok(PrecedenceEntry::Symbol(
                    self.resolve_rule_ref(&symbol.name, "precedences")?,
                ))
            }
        }
    }

    fn push_expr(&mut self, expr: GrammarExpr) -> Result<GrammarExprId, GrammarValidationError> {
        let id = GrammarExprId::from_index(self.expressions.len())?;
        self.expressions.push(expr);
        Ok(id)
    }

    fn intern_field(&mut self, name: &str) -> Result<FieldId, GrammarValidationError> {
        if let Some(id) = self.fields_by_name.get(name) {
            return Ok(*id);
        }
        let id = FieldId::from_index(self.fields.len())?;
        self.fields_by_name.insert(name.to_owned(), id);
        self.fields.push(FieldName(name.to_owned()));
        Ok(id)
    }

    fn intern_alias(
        &mut self,
        value: &str,
        named: bool,
    ) -> Result<AliasId, GrammarValidationError> {
        let key = alias_key(value, named);
        if let Some(id) = self.alias_by_value.get(&key) {
            return Ok(*id);
        }
        let id = AliasId::from_index(self.aliases.len())?;
        self.alias_by_value.insert(key, id);
        self.aliases.push(AliasDecl {
            id,
            value: value.to_owned(),
            named,
        });
        if named {
            self.visible_node_kinds
                .insert(value.to_owned(), VisibleNodeKind::Alias(id));
        }
        Ok(id)
    }
}

/// Grammar rule id in source order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RuleId(u32);

impl RuleId {
    fn from_index(index: usize) -> Result<Self, GrammarValidationError> {
        u32::try_from(index).map(Self).map_err(|_| {
            GrammarValidationError::new(GrammarValidationErrorKind::IdOverflow {
                domain: "rule",
                index,
            })
        })
    }

    /// Numeric id.
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// External token id in `externals` order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ExternalTokenId(u32);

impl ExternalTokenId {
    fn from_index(index: usize) -> Result<Self, GrammarValidationError> {
        u32::try_from(index).map(Self).map_err(|_| {
            GrammarValidationError::new(GrammarValidationErrorKind::IdOverflow {
                domain: "external",
                index,
            })
        })
    }

    /// Numeric id.
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// External token ordinal in scanner valid-symbol masks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ExternalTokenOrdinal(u32);

impl ExternalTokenOrdinal {
    /// Numeric ordinal.
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Field id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FieldId(u32);

impl FieldId {
    fn from_index(index: usize) -> Result<Self, GrammarValidationError> {
        u32::try_from(index).map(Self).map_err(|_| {
            GrammarValidationError::new(GrammarValidationErrorKind::IdOverflow {
                domain: "field",
                index,
            })
        })
    }

    /// Numeric id.
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Alias id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AliasId(u32);

impl AliasId {
    fn from_index(index: usize) -> Result<Self, GrammarValidationError> {
        u32::try_from(index).map(Self).map_err(|_| {
            GrammarValidationError::new(GrammarValidationErrorKind::IdOverflow {
                domain: "alias",
                index,
            })
        })
    }

    /// Numeric id.
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Rule-expression arena id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GrammarExprId(u32);

impl GrammarExprId {
    fn from_index(index: usize) -> Result<Self, GrammarValidationError> {
        u32::try_from(index).map(Self).map_err(|_| {
            GrammarValidationError::new(GrammarValidationErrorKind::IdOverflow {
                domain: "grammar expression",
                index,
            })
        })
    }

    /// Numeric id.
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Reserved-word context id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReservedSetId(u32);

impl ReservedSetId {
    fn from_index(index: usize) -> Result<Self, GrammarValidationError> {
        u32::try_from(index).map(Self).map_err(|_| {
            GrammarValidationError::new(GrammarValidationErrorKind::IdOverflow {
                domain: "reserved set",
                index,
            })
        })
    }

    /// Numeric id.
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Resolved symbol reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SymbolRef {
    /// Rule reference.
    Rule(RuleId),
    /// External scanner token reference.
    External(ExternalTokenId),
}

/// One validated rule declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleDecl {
    id: RuleId,
    name: RuleName,
    expr: GrammarExprId,
    visible: bool,
}

impl RuleDecl {
    /// Rule id.
    pub const fn id(&self) -> RuleId {
        self.id
    }

    /// Rule name.
    pub fn name(&self) -> &RuleName {
        &self.name
    }

    /// Root expression id for this rule.
    pub const fn expr(&self) -> GrammarExprId {
        self.expr
    }

    /// Whether this rule contributes a visible named node by default.
    pub const fn visible(&self) -> bool {
        self.visible
    }
}

/// External scanner token fact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalTokenFact {
    id: ExternalTokenId,
    ordinal: ExternalTokenOrdinal,
    name: Option<String>,
    declaration: ExternalTokenDecl,
}

impl ExternalTokenFact {
    /// External token id.
    pub const fn id(&self) -> ExternalTokenId {
        self.id
    }

    /// Scanner ordinal.
    pub const fn ordinal(&self) -> ExternalTokenOrdinal {
        self.ordinal
    }

    /// Optional symbolic name.
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Validated external token declaration.
    pub const fn declaration(&self) -> &ExternalTokenDecl {
        &self.declaration
    }
}

/// Validated external scanner token declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(u8)]
pub enum ExternalTokenDecl {
    /// Named scanner token from a `SYMBOL` external entry.
    Symbol {
        /// External token name.
        name: String,
    },
    /// Literal external token.
    StringToken {
        /// Literal token text.
        value: String,
    },
    /// Regex external token.
    PatternToken {
        /// Regex source.
        value: String,
        /// Regex flags.
        flags: Option<String>,
    },
}

/// Field name.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FieldName(String);

impl FieldName {
    /// Field name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Alias declaration.
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

    /// Whether this alias declaration is named.
    pub const fn named(&self) -> bool {
        self.named
    }
}

/// One declarative implicit-close content-model row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoCloseRule {
    tag: String,
    closed_by_tags: Vec<String>,
}

impl AutoCloseRule {
    /// Open tag this row applies to.
    pub fn tag(&self) -> &str {
        &self.tag
    }

    /// Start tag names that implicitly close `tag`.
    pub fn closed_by_tags(&self) -> &[String] {
        &self.closed_by_tags
    }
}

impl From<&RawAutoCloseRuleJson> for AutoCloseRule {
    fn from(rule: &RawAutoCloseRuleJson) -> Self {
        Self {
            tag: rule.tag.clone(),
            closed_by_tags: rule.closed_by_tags.clone(),
        }
    }
}

/// Validated grammar expression.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(u8)]
pub enum GrammarExpr {
    /// Empty production.
    Blank,
    /// Literal token.
    StringToken(String),
    /// Regex token.
    PatternToken {
        /// Regex source.
        value: String,
        /// Regex flags.
        flags: Option<String>,
    },
    /// Consume until one of the marker strings, or EOF.
    Until {
        /// Marker strings that terminate the token without being consumed.
        markers: Vec<String>,
    },
    /// Balanced nested delimiter token.
    Nested {
        /// Opening delimiter.
        open: String,
        /// Closing delimiter.
        close: String,
    },
    /// Declarative implicit close token for tag-stack grammars.
    AutoClose {
        /// Element/tag name this token implicitly closes.
        tag: String,
        /// Literal opening marker that pushes this tag.
        open: Option<String>,
        /// Literal explicit closing marker that pops this tag.
        close: Option<String>,
        /// Literal markers that trigger this implicit close when this tag is open.
        closed_by: Vec<String>,
        /// Public node kind whose reduced range pushes its tag-name child.
        open_node: Option<String>,
        /// Public node kind whose reduced range pops its tag-name child.
        close_node: Option<String>,
        /// Public child node kind that carries the tag-name text.
        tag_name_node: Option<String>,
        /// Prefix that begins a start tag at the current lexer position.
        start_prefix: Option<String>,
        /// Prefix that begins an end tag node range.
        end_prefix: Option<String>,
        /// Tag names that trigger this implicit close after `start_prefix`.
        closed_by_tags: Vec<String>,
        /// Content-model relation rows for one table-driven implicit-close token.
        rules: Vec<AutoCloseRule>,
    },
    /// Resolved symbol reference.
    Symbol(SymbolRef),
    /// Ordered choice.
    Choice(Vec<GrammarExprId>),
    /// Sequence.
    Seq(Vec<GrammarExprId>),
    /// Zero-or-more repetition.
    Repeat(GrammarExprId),
    /// One-or-more repetition.
    Repeat1(GrammarExprId),
    /// Field wrapper.
    Field {
        /// Field id.
        field: FieldId,
        /// Wrapped expression.
        content: GrammarExprId,
    },
    /// Lexical token wrapper.
    Token(GrammarExprId),
    /// Immediate lexical token wrapper.
    ImmediateToken(GrammarExprId),
    /// Static precedence wrapper.
    Prec {
        /// Associativity.
        assoc: PrecedenceAssoc,
        /// Static precedence value.
        value: StaticPrecedenceValue,
        /// Wrapped expression.
        content: GrammarExprId,
    },
    /// Dynamic precedence wrapper.
    PrecDynamic {
        /// Dynamic precedence value.
        value: i32,
        /// Wrapped expression.
        content: GrammarExprId,
    },
    /// Alias wrapper.
    Alias {
        /// Alias id.
        alias: AliasId,
        /// Whether the alias is named.
        named: bool,
        /// Wrapped expression.
        content: GrammarExprId,
    },
    /// Reserved-word context wrapper.
    Reserved {
        /// Reserved-word context.
        context: ReservedSetId,
        /// Wrapped expression.
        content: GrammarExprId,
    },
}

/// Static precedence associativity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PrecedenceAssoc {
    /// No associativity.
    None,
    /// Left associative.
    Left,
    /// Right associative.
    Right,
}

/// Validated static precedence value.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(u8)]
pub enum StaticPrecedenceValue {
    /// Integer precedence.
    Integer(i32),
    /// Named precedence.
    Name(String),
}

impl StaticPrecedenceValue {
    fn from_raw(value: &RawPrecedenceValue) -> Self {
        match value {
            RawPrecedenceValue::Integer(value) => Self::Integer(*value),
            RawPrecedenceValue::Name(name) => Self::Name(name.clone()),
        }
    }
}

/// Precedence declaration entry.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(u8)]
pub enum PrecedenceEntry {
    /// Named precedence.
    Name(String),
    /// Symbol precedence.
    Symbol(RuleId),
}

/// Reserved-word set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReservedSetDecl {
    id: ReservedSetId,
    name: String,
    entries: Vec<GrammarExprId>,
}

impl ReservedSetDecl {
    /// Reserved-word set id.
    pub const fn id(&self) -> ReservedSetId {
        self.id
    }

    /// Reserved-word set name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Entry expressions.
    pub fn entries(&self) -> &[GrammarExprId] {
        &self.entries
    }
}

/// Source of a visible node kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum VisibleNodeKind {
    /// Visible rule.
    Rule(RuleId),
    /// Named alias.
    Alias(AliasId),
}

/// Error while validating grammar facts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrammarValidationError {
    /// Error kind.
    pub kind: GrammarValidationErrorKind,
}

impl GrammarValidationError {
    fn new(kind: GrammarValidationErrorKind) -> Self {
        Self { kind }
    }
}

impl fmt::Display for GrammarValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            GrammarValidationErrorKind::NoRules => f.write_str("grammar has no rules"),
            GrammarValidationErrorKind::UnknownSymbol { name } => {
                write!(f, "unknown grammar symbol `{name}`")
            }
            GrammarValidationErrorKind::UnknownRule { name, context } => {
                write!(f, "unknown grammar rule `{name}` in {context}")
            }
            GrammarValidationErrorKind::ExternalWhereRuleRequired { name, context } => {
                write!(
                    f,
                    "external token `{name}` used where {context} requires a rule"
                )
            }
            GrammarValidationErrorKind::UnknownReservedContext { name } => {
                write!(f, "unknown reserved-word context `{name}`")
            }
            GrammarValidationErrorKind::InvalidPrecedenceSymbolKind { kind } => {
                write!(f, "expected SYMBOL precedence entry, got `{kind}`")
            }
            GrammarValidationErrorKind::UnsupportedExternalDeclaration { kind } => {
                write!(f, "unsupported external token declaration `{kind}`")
            }
            GrammarValidationErrorKind::IdOverflow { domain, index } => {
                write!(f, "{domain} id overflow at index {index}")
            }
        }
    }
}

impl Error for GrammarValidationError {}

/// Validation failure category.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(u8)]
pub enum GrammarValidationErrorKind {
    /// The grammar had no start rule.
    NoRules,
    /// A symbol reference could not be resolved as a rule or external token.
    UnknownSymbol {
        /// Unknown symbol name.
        name: String,
    },
    /// A rule-only context referenced a missing rule.
    UnknownRule {
        /// Unknown rule name.
        name: String,
        /// Validation context.
        context: &'static str,
    },
    /// A rule-only context referenced an external token.
    ExternalWhereRuleRequired {
        /// External token name.
        name: String,
        /// Validation context.
        context: &'static str,
    },
    /// A reserved-word wrapper referred to an undeclared reserved set.
    UnknownReservedContext {
        /// Unknown reserved context name.
        name: String,
    },
    /// A precedence entry had an unexpected raw rule kind.
    InvalidPrecedenceSymbolKind {
        /// Raw kind string.
        kind: String,
    },
    /// An external token declaration is outside Snark's validated scanner ABI slice.
    UnsupportedExternalDeclaration {
        /// Raw rule kind.
        kind: &'static str,
    },
    /// A dense id could not fit in Snark's id width.
    IdOverflow {
        /// Id domain name.
        domain: &'static str,
        /// Source-order index.
        index: usize,
    },
}

fn is_visible_rule_name(name: &str) -> bool {
    !name.starts_with('_')
}

fn external_name(rule: &RawRuleJson) -> Option<&str> {
    match rule {
        RawRuleJson::Symbol { name } => Some(name),
        _ => None,
    }
}

fn raw_rule_kind(rule: &RawRuleJson) -> &'static str {
    match rule {
        RawRuleJson::Alias { .. } => "ALIAS",
        RawRuleJson::AutoClose { .. } => "AUTO_CLOSE",
        RawRuleJson::Blank => "BLANK",
        RawRuleJson::Choice { .. } => "CHOICE",
        RawRuleJson::Field { .. } => "FIELD",
        RawRuleJson::ImmediateToken { .. } => "IMMEDIATE_TOKEN",
        RawRuleJson::Nested { .. } => "NESTED",
        RawRuleJson::Pattern { .. } => "PATTERN",
        RawRuleJson::Prec { .. } => "PREC",
        RawRuleJson::PrecDynamic { .. } => "PREC_DYNAMIC",
        RawRuleJson::PrecLeft { .. } => "PREC_LEFT",
        RawRuleJson::PrecRight { .. } => "PREC_RIGHT",
        RawRuleJson::Repeat { .. } => "REPEAT",
        RawRuleJson::Repeat1 { .. } => "REPEAT1",
        RawRuleJson::Reserved { .. } => "RESERVED",
        RawRuleJson::Seq { .. } => "SEQ",
        RawRuleJson::String { .. } => "STRING",
        RawRuleJson::Symbol { .. } => "SYMBOL",
        RawRuleJson::Token { .. } => "TOKEN",
        RawRuleJson::Until { .. } => "UNTIL",
    }
}

fn alias_key(value: &str, named: bool) -> String {
    format!("{named}\0{value}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grammar::RawGrammarJson;

    #[test]
    fn rejects_unknown_symbols() {
        let raw = RawGrammarJson::from_tree_sitter_json_str(
            r#"{
              "name": "bad",
              "rules": {
                "source": { "type": "SYMBOL", "name": "missing" }
              }
            }"#,
        )
        .unwrap();

        let error = ValidatedGrammar::from_raw(&raw).unwrap_err();

        assert_eq!(
            error.kind,
            GrammarValidationErrorKind::UnknownSymbol {
                name: "missing".to_owned()
            }
        );
    }

    #[test]
    fn aliases_are_distinguished_by_namedness() {
        let raw = RawGrammarJson::from_tree_sitter_json_str(
            r#"{
              "name": "aliases",
              "rules": {
                "source": {
                  "type": "CHOICE",
                  "members": [
                    {
                      "type": "ALIAS",
                      "value": "same_name",
                      "named": false,
                      "content": { "type": "SYMBOL", "name": "_hidden" }
                    },
                    {
                      "type": "ALIAS",
                      "value": "same_name",
                      "named": true,
                      "content": { "type": "SYMBOL", "name": "_hidden" }
                    }
                  ]
                },
                "_hidden": { "type": "STRING", "value": "x" }
              }
            }"#,
        )
        .unwrap();

        let grammar = ValidatedGrammar::from_raw(&raw).unwrap();

        assert_eq!(grammar.alias_count(), 2);
        assert!(grammar.has_visible_node_kind("same_name"));
    }

    #[test]
    fn reserved_wrappers_must_reference_declared_contexts() {
        let raw = RawGrammarJson::from_tree_sitter_json_str(
            r#"{
              "name": "reserved",
              "rules": {
                "source": {
                  "type": "RESERVED",
                  "context_name": "missing",
                  "content": { "type": "STRING", "value": "x" }
                }
              }
            }"#,
        )
        .unwrap();

        let error = ValidatedGrammar::from_raw(&raw).unwrap_err();

        assert_eq!(
            error.kind,
            GrammarValidationErrorKind::UnknownReservedContext {
                name: "missing".to_owned()
            }
        );
    }

    #[test]
    fn visible_inline_rules_are_not_emitted_node_kinds() {
        let raw = RawGrammarJson::from_tree_sitter_json_str(
            r#"{
              "name": "inline_visible",
              "rules": {
                "source": { "type": "SYMBOL", "name": "helper" },
                "helper": { "type": "STRING", "value": "x" }
              },
              "inline": ["helper"]
            }"#,
        )
        .unwrap();

        let grammar = ValidatedGrammar::from_raw(&raw).unwrap();

        assert!(grammar.has_visible_node_kind("source"));
        assert!(!grammar.has_visible_node_kind("helper"));
    }

    #[test]
    fn rule_only_contexts_reject_external_tokens() {
        let raw = RawGrammarJson::from_tree_sitter_json_str(
            r#"{
              "name": "bad_inline",
              "rules": {
                "source": { "type": "STRING", "value": "x" }
              },
              "externals": [{ "type": "SYMBOL", "name": "external_token" }],
              "inline": ["external_token"]
            }"#,
        )
        .unwrap();

        let error = ValidatedGrammar::from_raw(&raw).unwrap_err();

        assert_eq!(
            error.kind,
            GrammarValidationErrorKind::ExternalWhereRuleRequired {
                name: "external_token".to_owned(),
                context: "inline",
            }
        );
    }

    #[test]
    fn external_declarations_are_not_self_referential_expressions() {
        let raw = RawGrammarJson::from_tree_sitter_json_str(
            r#"{
              "name": "externals",
              "rules": {
                "source": { "type": "SYMBOL", "name": "external_token" }
              },
              "externals": [
                { "type": "SYMBOL", "name": "external_token" },
                { "type": "STRING", "value": "@nest" }
              ]
            }"#,
        )
        .unwrap();

        let grammar = ValidatedGrammar::from_raw(&raw).unwrap();

        assert_eq!(
            grammar.externals()[0].declaration(),
            &ExternalTokenDecl::Symbol {
                name: "external_token".to_owned()
            }
        );
        assert_eq!(
            grammar.externals()[1].declaration(),
            &ExternalTokenDecl::StringToken {
                value: "@nest".to_owned()
            }
        );
    }

    #[test]
    fn complex_external_declarations_are_rejected_before_expression_lowering() {
        let raw = RawGrammarJson::from_tree_sitter_json_str(
            r#"{
              "name": "externals",
              "rules": {
                "source": { "type": "STRING", "value": "x" }
              },
              "externals": [
                {
                  "type": "TOKEN",
                  "content": { "type": "SYMBOL", "name": "external_token" }
                }
              ]
            }"#,
        )
        .unwrap();

        let error = ValidatedGrammar::from_raw(&raw).unwrap_err();

        assert_eq!(
            error.kind,
            GrammarValidationErrorKind::UnsupportedExternalDeclaration { kind: "TOKEN" }
        );
    }
}
