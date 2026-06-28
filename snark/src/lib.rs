#![forbid(unsafe_code)]
#![warn(missing_docs)]
//! Tree-sitter-compatible grammar package and parser runtime scaffold.
//!
//! Snark starts at the package boundary: grammar JSON, external scanner
//! declarations, query bundles, corpus fixtures, and incremental parse edits.
//! The first implementation target is `tree-sitter-css`, followed by HTML,
//! JavaScript, and gingembre as a mixed-language parent.

use std::{
    fmt, fs, io,
    path::{Path, PathBuf},
};

use facet::Facet;
use facet_format::{FormatParser, ParseEventKind};
use facet_json::{JsonParser, RawJson};

/// A complete language package as consumed by the parser/highlighter runtime.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
pub struct LanguagePackage {
    /// Stable language name, matching Tree-sitter's `grammar.json` `name`.
    pub name: String,
    /// Tree-sitter-compatible grammar JSON model.
    pub grammar: GrammarJson,
    /// External scanner declarations and program artifacts.
    pub scanners: Vec<ScannerSpec>,
    /// Query files used by highlighting, locals, injections, and tags.
    pub queries: QueryBundle,
    /// Optional `node-types.json` payload for typed node metadata.
    pub node_types_json: Option<String>,
    /// Corpus fixtures used as parse-tree or highlighting oracles.
    pub corpus: Vec<CorpusFixture>,
}

impl LanguagePackage {
    /// Import a Tree-sitter package directory.
    ///
    /// This reads `src/grammar.json`, optional scanner sources, optional query
    /// files, optional `src/node-types.json`, and package corpus fixtures.
    pub fn from_tree_sitter_dir(path: impl AsRef<Path>) -> Result<Self, ImportError> {
        let path = path.as_ref();
        let grammar_path = path.join("src").join("grammar.json");
        let grammar_source =
            fs::read_to_string(&grammar_path).map_err(|source| ImportError::ReadFile {
                path: grammar_path,
                source,
            })?;
        let grammar = GrammarJson::from_tree_sitter_json(&grammar_source)?;
        let scanners = scanner_specs(path, &grammar)?;
        let queries = QueryBundle::from_tree_sitter_dir(path)?;
        let node_types_path = path.join("src").join("node-types.json");
        let node_types_json = read_optional_string(&node_types_path)?;
        let mut corpus = Vec::new();
        collect_corpus(
            path,
            &path.join("test").join("corpus"),
            CorpusKind::Parse,
            &mut corpus,
        )?;
        collect_corpus(
            path,
            &path.join("test").join("highlights"),
            CorpusKind::Highlight,
            &mut corpus,
        )?;

        Ok(Self {
            name: grammar.name.clone(),
            grammar,
            scanners,
            queries,
            node_types_json,
            corpus,
        })
    }
}

/// Tree-sitter-compatible `grammar.json` surface.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
pub struct GrammarJson {
    /// Optional schema URI.
    #[facet(rename = "$schema")]
    pub schema: Option<String>,
    /// Grammar name.
    pub name: String,
    /// Named grammar rules in Tree-sitter source order.
    pub rules: Vec<NamedRule>,
    /// Extra tokens/rules skipped between normal tokens.
    pub extras: Vec<RuleJson>,
    /// Static precedence order declarations.
    pub precedences: Vec<Vec<RuleJson>>,
    /// Declared GLR conflict sets.
    pub conflicts: Vec<Vec<String>>,
    /// External tokens accepted from scanner programs.
    pub externals: Vec<RuleJson>,
    /// Rules to inline during lowering.
    pub inline: Vec<String>,
    /// Hidden rules exposed as supertypes in node metadata.
    pub supertypes: Vec<String>,
    /// Optional word token used for keyword extraction.
    pub word: Option<String>,
    /// Contextual reserved-word sets.
    pub reserved: Vec<ReservedSet>,
}

impl GrammarJson {
    /// Import a `src/grammar.json` document emitted by Tree-sitter's generator.
    pub fn from_tree_sitter_json(input: &str) -> Result<Self, ImportError> {
        let raw: RawGrammarJson<'_> =
            facet_json::from_str_borrowed(input).map_err(ImportError::Deserialize)?;
        let rules = parse_ordered_object(raw.rules.as_str(), |name, value| {
            let rule = facet_json::from_str(value).map_err(ImportError::Deserialize)?;
            Ok(NamedRule {
                name: name.to_owned(),
                rule,
            })
        })?;
        let reserved = match raw.reserved {
            Some(reserved) => parse_ordered_object(reserved.as_str(), |name, value| {
                let rules = facet_json::from_str(value).map_err(ImportError::Deserialize)?;
                Ok(ReservedSet {
                    name: name.to_owned(),
                    rules,
                })
            })?,
            None => Vec::new(),
        };

        Ok(Self {
            schema: raw.schema,
            name: raw.name,
            rules,
            extras: raw.extras,
            precedences: raw.precedences,
            conflicts: raw.conflicts,
            externals: raw.externals,
            inline: raw.inline,
            supertypes: raw.supertypes,
            word: raw.word,
            reserved,
        })
    }

    /// The start rule is Tree-sitter's first rule in source order.
    pub fn start_rule(&self) -> Option<&NamedRule> {
        self.rules.first()
    }

    /// Look up a rule by name.
    pub fn rule(&self, name: &str) -> Option<&RuleJson> {
        self.rules
            .iter()
            .find(|rule| rule.name == name)
            .map(|rule| &rule.rule)
    }
}

#[derive(Debug, Clone, Facet, PartialEq, Eq)]
struct RawGrammarJson<'a> {
    #[facet(rename = "$schema")]
    schema: Option<String>,
    name: String,
    rules: RawJson<'a>,
    #[facet(default)]
    extras: Vec<RuleJson>,
    #[facet(default)]
    precedences: Vec<Vec<RuleJson>>,
    #[facet(default)]
    conflicts: Vec<Vec<String>>,
    #[facet(default)]
    externals: Vec<RuleJson>,
    #[facet(default)]
    inline: Vec<String>,
    #[facet(default)]
    supertypes: Vec<String>,
    word: Option<String>,
    #[facet(default)]
    reserved: Option<RawJson<'a>>,
}

/// A named grammar rule.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
pub struct NamedRule {
    /// Rule name.
    pub name: String,
    /// Rule body.
    pub rule: RuleJson,
}

/// A contextual reserved-word set.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
pub struct ReservedSet {
    /// Reserved-word context name.
    pub name: String,
    /// Rules in this reserved-word set.
    pub rules: Vec<RuleJson>,
}

/// Tree-sitter `RuleJSON`, mirrored at the compatibility boundary.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
#[facet(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
#[repr(u8)]
pub enum RuleJson {
    /// Alias a rule to a different visible node/token name.
    Alias {
        /// Rule being aliased.
        content: Box<RuleJson>,
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
        members: Vec<RuleJson>,
    },
    /// Named child field.
    Field {
        /// Field name.
        name: String,
        /// Field content.
        content: Box<RuleJson>,
    },
    /// Sequence.
    Seq {
        /// Sequence members.
        members: Vec<RuleJson>,
    },
    /// Zero-or-more repetition.
    Repeat {
        /// Repeated rule.
        content: Box<RuleJson>,
    },
    /// One-or-more repetition.
    Repeat1 {
        /// Repeated rule.
        content: Box<RuleJson>,
    },
    /// Dynamic precedence.
    PrecDynamic {
        /// Dynamic precedence value.
        value: i32,
        /// Rule content.
        content: Box<RuleJson>,
    },
    /// Left-associative static precedence.
    PrecLeft {
        /// Precedence value.
        value: PrecedenceValue,
        /// Rule content.
        content: Box<RuleJson>,
    },
    /// Right-associative static precedence.
    PrecRight {
        /// Precedence value.
        value: PrecedenceValue,
        /// Rule content.
        content: Box<RuleJson>,
    },
    /// Static precedence without associativity.
    Prec {
        /// Precedence value.
        value: PrecedenceValue,
        /// Rule content.
        content: Box<RuleJson>,
    },
    /// Lexical token wrapper.
    Token {
        /// Token content.
        content: Box<RuleJson>,
    },
    /// Token that may not consume leading extras.
    ImmediateToken {
        /// Token content.
        content: Box<RuleJson>,
    },
    /// Contextual reserved-word rule.
    Reserved {
        /// Reserved-word context name.
        context_name: String,
        /// Rule content.
        content: Box<RuleJson>,
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

/// External scanner declaration plus eventual executable artifact.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
pub struct ScannerSpec {
    /// Scanner language/source kind.
    pub kind: ScannerKind,
    /// External token names in grammar order.
    pub tokens: Vec<String>,
    /// Source or lowered artifact for the scanner.
    pub artifact: ScannerArtifact,
}

/// Scanner implementation kind.
#[derive(Debug, Clone, Copy, Facet, PartialEq, Eq)]
#[repr(u8)]
pub enum ScannerKind {
    /// Original Tree-sitter C scanner, used only as an import/oracle artifact.
    TreeSitterC,
    /// Original Tree-sitter C++ scanner, used only as an import/oracle artifact.
    TreeSitterCpp,
    /// Snark scanner IR.
    SnarkIr,
    /// Lowered Weavy scanner program.
    Weavy,
}

/// Scanner program artifact.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
#[repr(u8)]
pub enum ScannerArtifact {
    /// Source text.
    Source(String),
    /// Opaque lowered bytes.
    Bytes(Vec<u8>),
    /// Scanner is declared but not implemented yet.
    Missing,
}

/// Query files attached to a language package.
#[derive(Debug, Clone, Default, Facet, PartialEq, Eq)]
pub struct QueryBundle {
    /// `queries/highlights.scm`.
    pub highlights: Option<String>,
    /// `queries/locals.scm`.
    pub locals: Option<String>,
    /// `queries/injections.scm`.
    pub injections: Option<String>,
    /// `queries/tags.scm`.
    pub tags: Option<String>,
}

impl QueryBundle {
    /// Import optional Tree-sitter query files from a package directory.
    pub fn from_tree_sitter_dir(path: impl AsRef<Path>) -> Result<Self, ImportError> {
        let path = path.as_ref().join("queries");
        Ok(Self {
            highlights: read_optional_string(&path.join("highlights.scm"))?,
            locals: read_optional_string(&path.join("locals.scm"))?,
            injections: read_optional_string(&path.join("injections.scm"))?,
            tags: read_optional_string(&path.join("tags.scm"))?,
        })
    }
}

/// A corpus or highlight fixture imported from a language package.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
pub struct CorpusFixture {
    /// Fixture path inside the package.
    pub path: String,
    /// Fixture kind.
    pub kind: CorpusKind,
    /// Fixture source text.
    pub source: String,
}

/// Supported fixture categories.
#[derive(Debug, Clone, Copy, Facet, PartialEq, Eq)]
#[repr(u8)]
pub enum CorpusKind {
    /// Tree-sitter parse corpus fixture.
    Parse,
    /// Highlight fixture with caret assertions.
    Highlight,
}

/// Incremental edit coordinates.
#[derive(Debug, Clone, Copy, Facet, PartialEq, Eq)]
pub struct InputEdit {
    /// Start byte of the edit.
    pub start_byte: u32,
    /// Old end byte.
    pub old_end_byte: u32,
    /// New end byte.
    pub new_end_byte: u32,
    /// Start point of the edit.
    pub start_point: Point,
    /// Old end point.
    pub old_end_point: Point,
    /// New end point.
    pub new_end_point: Point,
}

/// Row/column coordinate.
#[derive(Debug, Clone, Copy, Facet, PartialEq, Eq)]
pub struct Point {
    /// Zero-based row.
    pub row: u32,
    /// Zero-based column in bytes for UTF-8 input.
    pub column: u32,
}

/// Range included in a child language parse.
#[derive(Debug, Clone, Copy, Facet, PartialEq, Eq)]
pub struct IncludedRange {
    /// Start byte.
    pub start_byte: u32,
    /// End byte.
    pub end_byte: u32,
    /// Start point.
    pub start_point: Point,
    /// End point.
    pub end_point: Point,
}

/// Error raised while importing a Tree-sitter package or grammar.
#[derive(Debug)]
pub enum ImportError {
    /// Could not read a file.
    ReadFile {
        /// File path.
        path: PathBuf,
        /// I/O error.
        source: io::Error,
    },
    /// Could not read a directory.
    ReadDir {
        /// Directory path.
        path: PathBuf,
        /// I/O error.
        source: io::Error,
    },
    /// Facet JSON deserialization failed.
    Deserialize(facet_json::DeserializeError),
    /// Facet JSON event parsing failed.
    Parse(facet_format::ParseError),
    /// A JSON document did not have the expected shape.
    Expected {
        /// Expected shape.
        expected: &'static str,
        /// Actual event description.
        actual: &'static str,
    },
    /// A JSON object key was missing a name.
    NamelessKey,
}

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReadFile { path, source } => {
                write!(f, "could not read {}: {}", path.display(), source)
            }
            Self::ReadDir { path, source } => {
                write!(f, "could not read directory {}: {}", path.display(), source)
            }
            Self::Deserialize(source) => {
                write!(f, "could not deserialize Tree-sitter JSON: {source}")
            }
            Self::Parse(source) => write!(f, "could not parse Tree-sitter JSON: {source}"),
            Self::Expected { expected, actual } => {
                write!(f, "expected {expected}, found {actual}")
            }
            Self::NamelessKey => write!(f, "object key did not contain a field name"),
        }
    }
}

impl std::error::Error for ImportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ReadFile { source, .. } | Self::ReadDir { source, .. } => Some(source),
            Self::Deserialize(source) => Some(source),
            Self::Parse(source) => Some(source),
            Self::Expected { .. } | Self::NamelessKey => None,
        }
    }
}

fn scanner_specs(path: &Path, grammar: &GrammarJson) -> Result<Vec<ScannerSpec>, ImportError> {
    let tokens = grammar
        .externals
        .iter()
        .filter_map(external_token_name)
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    let src = path.join("src");
    let mut scanners = Vec::new();
    if let Some(source) = read_optional_string(&src.join("scanner.c"))? {
        scanners.push(ScannerSpec {
            kind: ScannerKind::TreeSitterC,
            tokens: tokens.clone(),
            artifact: ScannerArtifact::Source(source),
        });
    }
    if let Some(source) = read_optional_string(&src.join("scanner.cc"))? {
        scanners.push(ScannerSpec {
            kind: ScannerKind::TreeSitterCpp,
            tokens,
            artifact: ScannerArtifact::Source(source),
        });
    }
    Ok(scanners)
}

fn external_token_name(rule: &RuleJson) -> Option<&str> {
    match rule {
        RuleJson::Symbol { name } => Some(name),
        _ => None,
    }
}

fn read_optional_string(path: &Path) -> Result<Option<String>, ImportError> {
    match fs::read_to_string(path) {
        Ok(source) => Ok(Some(source)),
        Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(ImportError::ReadFile {
            path: path.to_owned(),
            source,
        }),
    }
}

fn collect_corpus(
    package_root: &Path,
    root: &Path,
    kind: CorpusKind,
    fixtures: &mut Vec<CorpusFixture>,
) -> Result<(), ImportError> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(source) => {
            return Err(ImportError::ReadDir {
                path: root.to_owned(),
                source,
            });
        }
    };

    for entry in entries {
        let entry = entry.map_err(|source| ImportError::ReadDir {
            path: root.to_owned(),
            source,
        })?;
        let path = entry.path();
        if path.is_dir() {
            collect_corpus(package_root, &path, kind, fixtures)?;
        } else if path.is_file() {
            let source = fs::read_to_string(&path).map_err(|source| ImportError::ReadFile {
                path: path.clone(),
                source,
            })?;
            let path = path
                .strip_prefix(package_root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            fixtures.push(CorpusFixture { path, kind, source });
        }
    }

    fixtures.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(())
}

fn parse_ordered_object<T>(
    input: &str,
    mut parse_entry: impl FnMut(&str, &str) -> Result<T, ImportError>,
) -> Result<Vec<T>, ImportError> {
    let mut parser = JsonParser::<true>::new(input.as_bytes());
    let Some(event) = parser.next_event().map_err(ImportError::Parse)? else {
        return Err(ImportError::Expected {
            expected: "object",
            actual: "end of input",
        });
    };
    if !matches!(event.kind, ParseEventKind::StructStart(_)) {
        return Err(ImportError::Expected {
            expected: "object",
            actual: event_kind_name(&event.kind),
        });
    }

    let mut values = Vec::new();
    loop {
        let Some(event) = parser.next_event().map_err(ImportError::Parse)? else {
            return Err(ImportError::Expected {
                expected: "object end",
                actual: "end of input",
            });
        };
        match event.kind {
            ParseEventKind::StructEnd => return Ok(values),
            ParseEventKind::FieldKey(key) => {
                let name = key.name().ok_or(ImportError::NamelessKey)?;
                let raw = parser.capture_raw().map_err(ImportError::Parse)?.ok_or(
                    ImportError::Expected {
                        expected: "raw JSON value",
                        actual: "unsupported raw capture",
                    },
                )?;
                values.push(parse_entry(name, raw)?);
            }
            other => {
                return Err(ImportError::Expected {
                    expected: "field key or object end",
                    actual: event_kind_name(&other),
                });
            }
        }
    }
}

fn event_kind_name(kind: &ParseEventKind<'_>) -> &'static str {
    match kind {
        ParseEventKind::StructStart(_) => "object start",
        ParseEventKind::StructEnd => "object end",
        ParseEventKind::FieldKey(_) => "field key",
        ParseEventKind::OrderedField => "ordered field",
        ParseEventKind::SequenceStart(_) => "sequence start",
        ParseEventKind::SequenceEnd => "sequence end",
        ParseEventKind::Scalar(_) => "scalar",
        ParseEventKind::VariantTag(_) => "variant tag",
        _ => "unknown event",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINI_GRAMMAR: &str = r#"{
      "$schema": "https://tree-sitter.github.io/tree-sitter/assets/schemas/grammar.schema.json",
      "name": "mini_css",
      "rules": {
        "stylesheet": {
          "type": "REPEAT",
          "content": { "type": "SYMBOL", "name": "rule_set" }
        },
        "rule_set": {
          "type": "SEQ",
          "members": [
            { "type": "FIELD", "name": "selector", "content": { "type": "SYMBOL", "name": "selector" } },
            { "type": "STRING", "value": "{" },
            { "type": "STRING", "value": "}" }
          ]
        },
        "selector": {
          "type": "CHOICE",
          "members": [
            { "type": "SYMBOL", "name": "_descendant_operator" },
            { "type": "PATTERN", "value": "[a-zA-Z_-]+" }
          ]
        }
      },
      "extras": [{ "type": "PATTERN", "value": "\\s" }],
      "externals": [
        { "type": "SYMBOL", "name": "_descendant_operator" },
        { "type": "SYMBOL", "name": "_pseudo_class_selector_colon" },
        { "type": "SYMBOL", "name": "__error_recovery" }
      ],
      "inline": ["_top_level_item"],
      "reserved": {
        "default": [{ "type": "STRING", "value": "initial" }]
      }
    }"#;

    #[test]
    fn imports_grammar_json_in_rule_order() {
        let grammar = GrammarJson::from_tree_sitter_json(MINI_GRAMMAR).unwrap();

        assert_eq!(grammar.name, "mini_css");
        assert_eq!(
            grammar.start_rule().map(|rule| rule.name.as_str()),
            Some("stylesheet")
        );
        assert_eq!(grammar.rules[1].name, "rule_set");
        assert!(matches!(
            grammar.rule("selector"),
            Some(RuleJson::Choice { .. })
        ));
        assert_eq!(grammar.externals.len(), 3);
        assert_eq!(grammar.reserved[0].name, "default");
    }

    #[test]
    fn imports_precedence_wrapped_rules() {
        let named: RuleJson = facet_json::from_str(
            r#"{
              "type": "PREC_LEFT",
              "value": "selector",
              "content": { "type": "SYMBOL", "name": "selector" }
            }"#,
        )
        .unwrap();
        let integer: RuleJson = facet_json::from_str(
            r#"{
              "type": "PREC",
              "value": 1,
              "content": { "type": "STRING", "value": "!" }
            }"#,
        )
        .unwrap();

        assert!(matches!(
            named,
            RuleJson::PrecLeft {
                value: PrecedenceValue::Name(_),
                ..
            }
        ));
        assert!(matches!(
            integer,
            RuleJson::Prec {
                value: PrecedenceValue::Integer(1),
                ..
            }
        ));
    }

    #[test]
    fn imports_tree_sitter_package_shape() {
        let root = test_package_root("snark-mini-css");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("queries")).unwrap();
        fs::create_dir_all(root.join("test").join("corpus")).unwrap();
        fs::write(root.join("src").join("grammar.json"), MINI_GRAMMAR).unwrap();
        fs::write(
            root.join("src").join("scanner.c"),
            "enum TokenType { DESCENDANT_OP };",
        )
        .unwrap();
        fs::write(root.join("src").join("node-types.json"), "[]").unwrap();
        fs::write(
            root.join("queries").join("highlights.scm"),
            "(tag_name) @tag",
        )
        .unwrap();
        fs::write(
            root.join("test").join("corpus").join("selectors.txt"),
            "==================",
        )
        .unwrap();

        let package = LanguagePackage::from_tree_sitter_dir(&root).unwrap();

        assert_eq!(package.name, "mini_css");
        assert_eq!(package.scanners.len(), 1);
        assert_eq!(package.scanners[0].kind, ScannerKind::TreeSitterC);
        assert_eq!(
            package.scanners[0].tokens,
            vec![
                "_descendant_operator".to_string(),
                "_pseudo_class_selector_colon".to_string(),
                "__error_recovery".to_string()
            ]
        );
        assert_eq!(
            package.queries.highlights.as_deref(),
            Some("(tag_name) @tag")
        );
        assert_eq!(package.node_types_json.as_deref(), Some("[]"));
        assert_eq!(package.corpus[0].path, "test/corpus/selectors.txt");

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn imports_configured_tree_sitter_css_checkout() {
        let Ok(path) = std::env::var("SNARK_TREE_SITTER_CSS") else {
            return;
        };

        let package = LanguagePackage::from_tree_sitter_dir(path).unwrap();

        assert_eq!(package.name, "css");
        assert_eq!(
            package.grammar.start_rule().map(|rule| rule.name.as_str()),
            Some("stylesheet")
        );
        assert_eq!(package.grammar.rules.len(), 66);
        assert_eq!(package.grammar.externals.len(), 3);
        assert_eq!(package.scanners.len(), 1);
        assert_eq!(package.scanners[0].kind, ScannerKind::TreeSitterC);
        assert_eq!(
            package.scanners[0].tokens,
            vec![
                "_descendant_operator".to_string(),
                "_pseudo_class_selector_colon".to_string(),
                "__error_recovery".to_string()
            ]
        );
        assert!(package.queries.highlights.is_some());
    }

    fn test_package_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("{}-{}", name, std::process::id()))
    }
}
