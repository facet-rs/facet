//! Validated Snark grammar facts derived from raw Tree-sitter grammar input.

use std::{error::Error, fmt};

use indexmap::IndexMap;

use crate::grammar::{
    LanguageName, PrecedenceEntryJson, PrecedenceValue, RawGrammarJson, RawRuleJson, RuleName,
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
    inline: Vec<SymbolRef>,
    supertypes: Vec<SymbolRef>,
    conflicts: Vec<Vec<SymbolRef>>,
    precedence_groups: Vec<Vec<PrecedenceEntry>>,
    word: Option<SymbolRef>,
    extras: Vec<GrammarExprId>,
    reserved_sets: Vec<ReservedSetDecl>,
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

        let mut externals = Vec::with_capacity(raw.externals.len());
        let mut external_names = OrderedMap::default();
        for (index, rule) in raw.externals.iter().enumerate() {
            let id = ExternalTokenId::from_index(index)?;
            let name = external_name(rule).map(str::to_owned);
            if let Some(name) = &name {
                external_names.insert(name.clone(), id);
            }
            externals.push(ExternalTokenFact {
                id,
                ordinal: ExternalTokenOrdinal(id.get()),
                name,
                rule: rule.clone(),
            });
        }

        let mut builder = ValidationBuilder {
            rules_by_name,
            external_names,
            expressions: Vec::new(),
            fields: Vec::new(),
            fields_by_name: OrderedMap::default(),
            aliases: Vec::new(),
            alias_by_value: OrderedMap::default(),
            visible_node_kinds: OrderedMap::default(),
        };

        for rule in &rules {
            if rule.visible {
                builder.visible_node_kinds.insert(
                    rule.name.as_str().to_owned(),
                    VisibleNodeKind::Rule(rule.id),
                );
            }
        }

        for rule in &mut rules {
            let raw_rule = raw.rule(rule.name.as_str()).ok_or_else(|| {
                GrammarValidationError::new(GrammarValidationErrorKind::UnknownSymbol {
                    name: rule.name.as_str().to_owned(),
                })
            })?;
            rule.expr = builder.lower_rule(raw_rule)?;
        }

        let extras = raw
            .extras
            .iter()
            .map(|rule| builder.lower_rule(rule))
            .collect::<Result<Vec<_>, _>>()?;
        let inline = raw
            .inline
            .iter()
            .map(|name| builder.resolve_symbol(name))
            .collect::<Result<Vec<_>, _>>()?;
        let supertypes = raw
            .supertypes
            .iter()
            .map(|name| builder.resolve_symbol(name))
            .collect::<Result<Vec<_>, _>>()?;
        let conflicts = raw
            .conflicts
            .iter()
            .map(|members| {
                members
                    .iter()
                    .map(|name| builder.resolve_symbol(name))
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
            .map(|name| builder.resolve_symbol(name))
            .transpose()?;
        let reserved_sets = raw
            .reserved
            .iter()
            .map(|(name, rules)| {
                let entries = rules
                    .iter()
                    .map(|rule| builder.lower_rule(rule))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(ReservedSetDecl {
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

    /// Valid-symbol mask width for external scanner calls.
    pub fn external_valid_symbol_mask_width(&self) -> usize {
        self.externals.len()
    }

    /// Number of fields discovered in rule expressions.
    pub fn field_count(&self) -> usize {
        self.fields.len()
    }

    /// Number of named aliases discovered in rule expressions.
    pub fn alias_count(&self) -> usize {
        self.aliases.len()
    }

    /// Number of extra rule expressions.
    pub fn extra_count(&self) -> usize {
        self.extras.len()
    }

    /// Number of inline symbol declarations.
    pub fn inline_count(&self) -> usize {
        self.inline.len()
    }

    /// Number of supertype declarations.
    pub fn supertype_count(&self) -> usize {
        self.supertypes.len()
    }

    /// Number of conflict sets.
    pub fn conflict_count(&self) -> usize {
        self.conflicts.len()
    }

    /// Number of precedence groups.
    pub fn precedence_group_count(&self) -> usize {
        self.precedence_groups.len()
    }

    /// Word-token symbol, if one was declared.
    pub fn word(&self) -> Option<SymbolRef> {
        self.word
    }

    /// Whether a visible corpus node kind is known to this grammar.
    pub fn has_visible_node_kind(&self, kind: &str) -> bool {
        self.visible_node_kinds.contains_key(kind)
    }

    /// Iterate visible node kind names derived from visible rules and named aliases.
    pub fn visible_node_kinds(&self) -> impl Iterator<Item = &str> {
        self.visible_node_kinds.keys().map(String::as_str)
    }
}

struct ValidationBuilder {
    rules_by_name: OrderedMap<RuleId>,
    external_names: OrderedMap<ExternalTokenId>,
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
                value: value.clone(),
                content: self.lower_rule(content)?,
            },
            RawRuleJson::PrecRight { value, content } => GrammarExpr::Prec {
                assoc: PrecedenceAssoc::Right,
                value: value.clone(),
                content: self.lower_rule(content)?,
            },
            RawRuleJson::Prec { value, content } => GrammarExpr::Prec {
                assoc: PrecedenceAssoc::None,
                value: value.clone(),
                content: self.lower_rule(content)?,
            },
            RawRuleJson::Token { content } => GrammarExpr::Token(self.lower_rule(content)?),
            RawRuleJson::ImmediateToken { content } => {
                GrammarExpr::ImmediateToken(self.lower_rule(content)?)
            }
            RawRuleJson::Reserved {
                context_name,
                content,
            } => GrammarExpr::Reserved {
                context_name: context_name.clone(),
                content: self.lower_rule(content)?,
            },
        };
        self.push_expr(expr)
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
                Ok(PrecedenceEntry::Symbol(self.resolve_symbol(&symbol.name)?))
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
        if let Some(id) = self.alias_by_value.get(value) {
            return Ok(*id);
        }
        let id = AliasId::from_index(self.aliases.len())?;
        self.alias_by_value.insert(value.to_owned(), id);
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
    rule: RawRuleJson,
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

    /// Original external token rule.
    pub const fn rule(&self) -> &RawRuleJson {
        &self.rule
    }
}

/// Field name.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FieldName(String);

/// Alias declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AliasDecl {
    id: AliasId,
    value: String,
    named: bool,
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
        value: PrecedenceValue,
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
        /// Context name.
        context_name: String,
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

/// Precedence declaration entry.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(u8)]
pub enum PrecedenceEntry {
    /// Named precedence.
    Name(String),
    /// Symbol precedence.
    Symbol(SymbolRef),
}

/// Reserved-word set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReservedSetDecl {
    name: String,
    entries: Vec<GrammarExprId>,
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
            GrammarValidationErrorKind::InvalidPrecedenceSymbolKind { kind } => {
                write!(f, "expected SYMBOL precedence entry, got `{kind}`")
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
    /// A precedence entry had an unexpected raw rule kind.
    InvalidPrecedenceSymbolKind {
        /// Raw kind string.
        kind: String,
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
}
