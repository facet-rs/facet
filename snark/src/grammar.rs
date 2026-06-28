//! Raw Tree-sitter grammar JSON compatibility model.

use facet::Facet;
use indexmap::IndexMap;

#[cfg(feature = "json-import")]
use crate::diagnostic::{ImportError, JsonDocumentKind};
#[cfg(feature = "json-import")]
use crate::source::{PackageRoot, SourceFile};

type OrderedMap<V> = IndexMap<String, V, std::hash::RandomState>;

/// Raw `grammar.json` source plus decoded grammar data.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
pub struct RawGrammarFile {
    /// Original JSON source.
    pub raw: String,
    /// Decoded raw grammar JSON.
    pub grammar: RawGrammarJson,
}

impl RawGrammarFile {
    /// Import a `src/grammar.json` source file.
    #[cfg(feature = "json-import")]
    pub fn from_source_file(
        root: &PackageRoot,
        source_file: SourceFile<String>,
    ) -> Result<SourceFile<Self>, ImportError> {
        let path = root.join(&source_file.path);
        let source_id = source_file.id;
        let package_path = source_file.path.clone();
        let grammar =
            facet_json::from_str(&source_file.body).map_err(|source| ImportError::Json {
                package_root: Some(root.as_path().to_owned()),
                path: Some(path),
                source_id: Some(source_id),
                package_path: Some(package_path),
                document: JsonDocumentKind::Grammar,
                phase: "decode raw grammar JSON",
                source,
            })?;
        Ok(source_file.map(|raw| Self { raw, grammar }))
    }
}

/// Tree-sitter language name as declared in `grammar.json`.
#[derive(Debug, Clone, Facet, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LanguageName(String);

impl LanguageName {
    /// Create a language name.
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// Borrow the language name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Raw rule name from a Tree-sitter grammar.
#[derive(Debug, Clone, Facet, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RuleName(String);

impl RuleName {
    /// Create a raw rule name.
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// Borrow the rule name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Ordered rule table preserving Tree-sitter source order.
#[derive(Debug, Clone, Default, Facet, PartialEq, Eq)]
#[facet(transparent)]
pub struct RuleTable(OrderedMap<RawRuleJson>);

impl RuleTable {
    /// Number of rules.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether there are no rules.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// First rule in source order, which Tree-sitter treats as the start rule.
    pub fn start_rule(&self) -> Option<(RuleName, &RawRuleJson)> {
        self.0
            .first()
            .map(|(name, rule)| (RuleName::new(name.clone()), rule))
    }

    /// Get a rule by name.
    pub fn get(&self, name: &str) -> Option<&RawRuleJson> {
        self.0.get(name)
    }

    /// Get a rule by source-order index.
    pub fn get_index(&self, index: usize) -> Option<(RuleName, &RawRuleJson)> {
        self.0
            .get_index(index)
            .map(|(name, rule)| (RuleName::new(name.clone()), rule))
    }

    /// Iterate rules in source order.
    pub fn iter(&self) -> impl Iterator<Item = (RuleName, &RawRuleJson)> {
        self.0
            .iter()
            .map(|(name, rule)| (RuleName::new(name.clone()), rule))
    }
}

/// Ordered reserved-word set table.
#[derive(Debug, Clone, Default, Facet, PartialEq, Eq)]
#[facet(transparent)]
pub struct ReservedSetTable(OrderedMap<Vec<RawRuleJson>>);

impl ReservedSetTable {
    /// Number of reserved-word sets.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether there are no reserved-word sets.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Get a reserved-word set by source-order index.
    pub fn get_index(&self, index: usize) -> Option<(&str, &[RawRuleJson])> {
        self.0
            .get_index(index)
            .map(|(name, rules)| (name.as_str(), rules.as_slice()))
    }

    /// Iterate reserved-word sets in source order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &[RawRuleJson])> {
        self.0
            .iter()
            .map(|(name, rules)| (name.as_str(), rules.as_slice()))
    }
}

/// Tree-sitter-compatible raw `grammar.json` surface.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
pub struct RawGrammarJson {
    /// Optional schema URI.
    #[facet(rename = "$schema")]
    pub schema: Option<String>,
    /// Optional inherited grammar name.
    pub inherits: Option<String>,
    /// Grammar name.
    pub name: String,
    /// Named grammar rules in Tree-sitter source order.
    pub rules: RuleTable,
    /// Extra tokens/rules skipped between normal tokens.
    #[facet(default)]
    pub extras: Vec<RawRuleJson>,
    /// Static precedence order declarations.
    #[facet(default)]
    pub precedences: Vec<Vec<PrecedenceEntryJson>>,
    /// Declared GLR conflict sets.
    #[facet(default)]
    pub conflicts: Vec<Vec<String>>,
    /// External tokens accepted from scanner programs.
    #[facet(default)]
    pub externals: Vec<RawRuleJson>,
    /// Rules to inline during lowering.
    #[facet(default)]
    pub inline: Vec<String>,
    /// Hidden rules exposed as supertypes in node metadata.
    #[facet(default)]
    pub supertypes: Vec<String>,
    /// Optional word token used for keyword extraction.
    pub word: Option<String>,
    /// Contextual reserved-word sets.
    #[facet(default)]
    pub reserved: ReservedSetTable,
}

impl RawGrammarJson {
    /// Import a `src/grammar.json` string emitted by Tree-sitter's generator.
    #[cfg(feature = "json-import")]
    pub fn from_tree_sitter_json_str(input: &str) -> Result<Self, ImportError> {
        facet_json::from_str(input).map_err(|source| ImportError::Json {
            package_root: None,
            path: None,
            source_id: None,
            package_path: None,
            document: JsonDocumentKind::Grammar,
            phase: "decode raw grammar JSON",
            source,
        })
    }

    /// The start rule is Tree-sitter's first rule in source order.
    pub fn start_rule(&self) -> Option<(RuleName, &RawRuleJson)> {
        self.rules.start_rule()
    }

    /// Look up a rule by name.
    pub fn rule(&self, name: &str) -> Option<&RawRuleJson> {
        self.rules.get(name)
    }

    /// Grammar language name.
    pub fn language_name(&self) -> LanguageName {
        LanguageName::new(self.name.clone())
    }
}

/// Tree-sitter precedence-order entry.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
#[facet(untagged)]
#[repr(u8)]
pub enum PrecedenceEntryJson {
    /// Named precedence entry.
    Name(String),
    /// Symbol precedence entry.
    Symbol(PrecedenceSymbolJson),
}

/// Tree-sitter precedence-order symbol entry.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
pub struct PrecedenceSymbolJson {
    /// Rule JSON type, expected to be `SYMBOL` at validation time.
    #[facet(rename = "type")]
    pub kind: String,
    /// Referenced symbol name.
    pub name: String,
}

/// Tree-sitter `RuleJSON`, mirrored at the compatibility boundary.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
#[facet(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
#[repr(u8)]
pub enum RawRuleJson {
    /// Alias a rule to a different visible node/token name.
    Alias {
        /// Rule being aliased.
        content: Box<RawRuleJson>,
        /// Whether the alias is named.
        named: bool,
        /// Alias value.
        value: String,
    },
    /// Empty production.
    Blank,
    /// Literal string token.
    String {
        /// Token text.
        value: String,
    },
    /// Regex token pattern.
    Pattern {
        /// Regex source.
        value: String,
        /// Optional regex flags.
        flags: Option<String>,
    },
    /// Reference to another rule or token.
    Symbol {
        /// Referenced symbol name.
        name: String,
    },
    /// Ordered choice.
    Choice {
        /// Choice arms.
        members: Vec<RawRuleJson>,
    },
    /// Named child field.
    Field {
        /// Field name.
        name: String,
        /// Field content.
        content: Box<RawRuleJson>,
    },
    /// Sequence.
    Seq {
        /// Sequence members.
        members: Vec<RawRuleJson>,
    },
    /// Zero-or-more repetition.
    Repeat {
        /// Repeated rule.
        content: Box<RawRuleJson>,
    },
    /// One-or-more repetition.
    Repeat1 {
        /// Repeated rule.
        content: Box<RawRuleJson>,
    },
    /// Dynamic precedence.
    PrecDynamic {
        /// Dynamic precedence value.
        value: i32,
        /// Rule content.
        content: Box<RawRuleJson>,
    },
    /// Left-associative static precedence.
    PrecLeft {
        /// Precedence value.
        value: PrecedenceValue,
        /// Rule content.
        content: Box<RawRuleJson>,
    },
    /// Right-associative static precedence.
    PrecRight {
        /// Precedence value.
        value: PrecedenceValue,
        /// Rule content.
        content: Box<RawRuleJson>,
    },
    /// Static precedence without associativity.
    Prec {
        /// Precedence value.
        value: PrecedenceValue,
        /// Rule content.
        content: Box<RawRuleJson>,
    },
    /// Lexical token wrapper.
    Token {
        /// Token content.
        content: Box<RawRuleJson>,
    },
    /// Token that may not consume leading extras.
    ImmediateToken {
        /// Token content.
        content: Box<RawRuleJson>,
    },
    /// Contextual reserved-word rule.
    Reserved {
        /// Reserved-word context name.
        context_name: String,
        /// Rule content.
        content: Box<RawRuleJson>,
    },
}

/// Tree-sitter precedence value.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
#[facet(untagged)]
#[repr(u8)]
pub enum PrecedenceValue {
    /// Integer precedence.
    Integer(i32),
    /// Named precedence from a precedence ordering.
    Name(String),
}
