//! Tree-sitter package import boundary.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use crate::{
    corpus::{CorpusFixture, CorpusKind, CorpusSource},
    diagnostic::ImportError,
    grammar::RawGrammarFile,
    manifest::{QueryPaths, TreeSitterConfigJson, TreeSitterGrammarConfig},
    node_types::NodeTypesJson,
    query::{QueryBundle, QueryFile, QuerySource, WellKnownQuery},
    scanner::{ExternalTokenTable, ScannerSource, TreeSitterScanner, TreeSitterScannerKind},
    source::{
        PackageRelativePath, PackageRoot, SourceFile, SourceIdAllocator,
        read_optional_source_string, read_source_string,
    },
};

/// Imported inputs for one Tree-sitter grammar entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedGrammar {
    /// Optional manifest grammar entry that selected this grammar.
    pub config: Option<TreeSitterGrammarConfig>,
    /// Grammar directory relative to the package root.
    pub path: Option<PackageRelativePath>,
    /// Raw grammar JSON with source provenance.
    pub grammar: SourceFile<RawGrammarFile>,
    /// Optional `node-types.json`.
    pub node_types_json: Option<SourceFile<NodeTypesJson>>,
    /// Imported Tree-sitter scanners.
    pub scanners: Vec<TreeSitterScanner>,
    /// Imported query sources.
    pub queries: QueryBundle,
    /// Imported corpus and highlight fixture sources.
    pub corpus: Vec<CorpusFixture>,
}

impl ImportedGrammar {
    /// Language name from `grammar.json`.
    pub fn language_name(&self) -> &str {
        &self.grammar.body.grammar.name
    }
}

/// Imported Tree-sitter package inputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedPackage {
    /// Package root.
    pub root: PackageRoot,
    /// Optional `tree-sitter.json`.
    pub manifest: Option<SourceFile<TreeSitterConfigJson>>,
    /// Imported grammars in manifest order, or the single legacy root grammar.
    pub grammars: Vec<ImportedGrammar>,
}

impl ImportedPackage {
    /// Language name from `grammar.json`.
    pub fn language_name(&self) -> &str {
        self.grammars[0].language_name()
    }

    /// First imported grammar.
    pub fn first_grammar(&self) -> &ImportedGrammar {
        &self.grammars[0]
    }
}

/// Filesystem importer for Tree-sitter package layout.
#[derive(Debug, Clone)]
pub struct TreeSitterPackageImporter {
    root: PackageRoot,
}

impl TreeSitterPackageImporter {
    /// Create an importer rooted at a Tree-sitter package directory.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: PackageRoot::new(root),
        }
    }

    /// Import package inputs.
    pub fn import(self) -> Result<ImportedPackage, ImportError> {
        let root = PackageRoot::from_existing_dir(self.root.as_path())?;
        let mut ids = SourceIdAllocator::new();
        let manifest = read_optional_source_string(&root, &rel("tree-sitter.json")?, &mut ids)?
            .map(|source| TreeSitterConfigJson::from_source_file(&root, source))
            .transpose()?;
        let grammar_configs = manifest
            .as_ref()
            .map(|manifest| manifest.body.config.grammars.clone())
            .unwrap_or_else(|| vec![default_grammar_config()]);
        if grammar_configs.is_empty() {
            return Err(ImportError::NoGrammarsInManifest {
                package_root: root.as_path().to_owned(),
            });
        }
        let mut grammars = Vec::with_capacity(grammar_configs.len());
        for config in grammar_configs {
            grammars.push(import_grammar(&root, Some(config), &mut ids)?);
        }

        Ok(ImportedPackage {
            root,
            manifest,
            grammars,
        })
    }
}

fn default_grammar_config() -> TreeSitterGrammarConfig {
    TreeSitterGrammarConfig {
        name: String::new(),
        camelcase: None,
        title: None,
        scope: String::new(),
        path: None,
        external_files: None,
        file_types: None,
        highlights: None,
        injections: None,
        locals: None,
        tags: None,
        injection_regex: None,
        first_line_regex: None,
        content_regex: None,
        class_name: None,
    }
}

fn import_grammar(
    root: &PackageRoot,
    config: Option<TreeSitterGrammarConfig>,
    ids: &mut SourceIdAllocator,
) -> Result<ImportedGrammar, ImportError> {
    let path = config
        .as_ref()
        .and_then(|config| config.path.as_deref())
        .map(grammar_base_path)
        .transpose()?
        .flatten();
    let grammar_source =
        read_source_string(root, &rel_under(path.as_ref(), "src/grammar.json")?, ids)?;
    let grammar = RawGrammarFile::from_source_file(root, grammar_source)?;
    let node_types_json =
        read_optional_source_string(root, &rel_under(path.as_ref(), "src/node-types.json")?, ids)?
            .map(|source| NodeTypesJson::from_source_file(root, source))
            .transpose()?;
    let scanners = import_scanners(root, path.as_ref(), &grammar.body.grammar, ids)?;
    let queries = import_queries(root, path.as_ref(), config.as_ref(), ids)?;
    let corpus = import_corpus(root, path.as_ref(), ids)?;

    Ok(ImportedGrammar {
        config,
        path,
        grammar,
        node_types_json,
        scanners,
        queries,
        corpus,
    })
}

fn import_scanners(
    root: &PackageRoot,
    base: Option<&PackageRelativePath>,
    grammar: &crate::grammar::RawGrammarJson,
    ids: &mut SourceIdAllocator,
) -> Result<Vec<TreeSitterScanner>, ImportError> {
    let externals = ExternalTokenTable::from_rules(&grammar.externals)?;
    let mut scanners = Vec::new();

    if let Some(source) =
        read_optional_source_string(root, &rel_under(base, "src/scanner.c")?, ids)?
    {
        scanners.push(TreeSitterScanner {
            kind: TreeSitterScannerKind::C,
            source: source.map(ScannerSource),
            externals: externals.clone(),
        });
    }

    if let Some(source) =
        read_optional_source_string(root, &rel_under(base, "src/scanner.cc")?, ids)?
    {
        scanners.push(TreeSitterScanner {
            kind: TreeSitterScannerKind::Cpp,
            source: source.map(ScannerSource),
            externals,
        });
    }

    Ok(scanners)
}

fn import_queries(
    root: &PackageRoot,
    base: Option<&PackageRelativePath>,
    config: Option<&TreeSitterGrammarConfig>,
    ids: &mut SourceIdAllocator,
) -> Result<QueryBundle, ImportError> {
    let mut files = Vec::new();
    let mut seen_paths = Vec::new();

    import_query_category(
        root,
        base,
        query_paths(
            config.and_then(|config| config.highlights.as_ref()),
            WellKnownQuery::Highlights,
        ),
        WellKnownQuery::Highlights,
        ids,
        &mut seen_paths,
        &mut files,
    )?;
    import_query_category(
        root,
        base,
        query_paths(
            config.and_then(|config| config.locals.as_ref()),
            WellKnownQuery::Locals,
        ),
        WellKnownQuery::Locals,
        ids,
        &mut seen_paths,
        &mut files,
    )?;
    import_query_category(
        root,
        base,
        query_paths(
            config.and_then(|config| config.injections.as_ref()),
            WellKnownQuery::Injections,
        ),
        WellKnownQuery::Injections,
        ids,
        &mut seen_paths,
        &mut files,
    )?;
    import_query_category(
        root,
        base,
        query_paths(
            config.and_then(|config| config.tags.as_ref()),
            WellKnownQuery::Tags,
        ),
        WellKnownQuery::Tags,
        ids,
        &mut seen_paths,
        &mut files,
    )?;

    let queries_dir = root.join(&rel_under(base, "queries")?);
    let mut unknown_paths = Vec::new();
    collect_relative_file_paths(root, &queries_dir, &mut unknown_paths)?;
    unknown_paths.sort();
    for path in unknown_paths {
        if seen_paths.iter().any(|seen| seen == &path) {
            continue;
        }
        let source = read_source_string(root, &path, ids)?;
        files.push(QueryFile {
            category: None,
            configured: false,
            source: source.map(QuerySource),
        });
    }

    Ok(QueryBundle { files })
}

fn query_paths(configured: Option<&QueryPaths>, query: WellKnownQuery) -> Vec<String> {
    configured
        .map(|paths| paths.as_slice().to_vec())
        .unwrap_or_else(|| vec![format!("queries/{}", query.filename())])
}

fn import_query_category(
    root: &PackageRoot,
    base: Option<&PackageRelativePath>,
    paths: Vec<String>,
    category: WellKnownQuery,
    ids: &mut SourceIdAllocator,
    seen_paths: &mut Vec<PackageRelativePath>,
    files: &mut Vec<QueryFile>,
) -> Result<(), ImportError> {
    for path in paths {
        let relative = rel_under(base, &path)?;
        let Some(source) = read_optional_source_string(root, &relative, ids)? else {
            continue;
        };
        seen_paths.push(relative);
        files.push(QueryFile {
            category: Some(category),
            configured: true,
            source: source.map(QuerySource),
        });
    }
    Ok(())
}

fn import_corpus(
    root: &PackageRoot,
    base: Option<&PackageRelativePath>,
    ids: &mut SourceIdAllocator,
) -> Result<Vec<CorpusFixture>, ImportError> {
    let mut fixtures = Vec::new();
    collect_corpus_dir(
        root,
        base,
        "test/corpus",
        CorpusKind::Parse,
        ids,
        &mut fixtures,
    )?;
    collect_corpus_dir(
        root,
        base,
        "test/highlight",
        CorpusKind::Highlight,
        ids,
        &mut fixtures,
    )?;
    collect_corpus_dir(
        root,
        base,
        "test/highlights",
        CorpusKind::Highlight,
        ids,
        &mut fixtures,
    )?;
    fixtures.sort_by(|left, right| left.source.path.cmp(&right.source.path));
    Ok(fixtures)
}

fn collect_corpus_dir(
    root: &PackageRoot,
    base: Option<&PackageRelativePath>,
    relative: &str,
    kind: CorpusKind,
    ids: &mut SourceIdAllocator,
    fixtures: &mut Vec<CorpusFixture>,
) -> Result<(), ImportError> {
    let dir = root.join(&rel_under(base, relative)?);
    collect_source_files(root, &dir, ids, &mut |source| {
        fixtures.push(CorpusFixture {
            kind,
            source: source.map(CorpusSource),
        });
    })
}

fn collect_source_files<T>(
    root: &PackageRoot,
    dir: &Path,
    ids: &mut SourceIdAllocator,
    push: &mut T,
) -> Result<(), ImportError>
where
    T: FnMut(SourceFile<String>),
{
    let mut relative_paths = Vec::new();
    collect_relative_file_paths(root, dir, &mut relative_paths)?;
    relative_paths.sort();

    for relative in relative_paths {
        let source = read_source_string(root, &relative, ids)?;
        push(source);
    }
    Ok(())
}

fn collect_relative_file_paths(
    root: &PackageRoot,
    dir: &Path,
    paths: &mut Vec<PackageRelativePath>,
) -> Result<(), ImportError> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(source) => {
            return Err(ImportError::ReadDir {
                package_root: root.as_path().to_owned(),
                path: dir.to_owned(),
                source,
            });
        }
    };

    let mut entries =
        entries
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| ImportError::ReadDir {
                package_root: root.as_path().to_owned(),
                path: dir.to_owned(),
                source,
            })?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        let file_type = entry.file_type().map_err(|source| ImportError::ReadDir {
            package_root: root.as_path().to_owned(),
            path: dir.to_owned(),
            source,
        })?;
        if file_type.is_dir() {
            collect_relative_file_paths(root, &path, paths)?;
        } else if file_type.is_file() || file_type.is_symlink() {
            let relative =
                path.strip_prefix(root.as_path())
                    .map_err(|_| ImportError::PathOutsidePackage {
                        package_root: root.as_path().to_owned(),
                        path: path.clone(),
                    })?;
            let relative = PackageRelativePath::new(relative)?;
            paths.push(relative);
        }
    }
    Ok(())
}

fn rel(path: &str) -> Result<PackageRelativePath, ImportError> {
    PackageRelativePath::new(Path::new(path))
}

fn grammar_base_path(path: &str) -> Result<Option<PackageRelativePath>, ImportError> {
    if path.is_empty() || path == "." {
        Ok(None)
    } else {
        PackageRelativePath::new(Path::new(path)).map(Some)
    }
}

fn rel_under(
    base: Option<&PackageRelativePath>,
    path: &str,
) -> Result<PackageRelativePath, ImportError> {
    match base {
        Some(base) => PackageRelativePath::new(Path::new(base.as_str()).join(path)),
        None => rel(path),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        cell::{Cell, RefCell},
        collections::BTreeSet,
        fs,
    };

    use crate::{
        corpus::{HighlightAssertion, HighlightPoint, SexpChild, SexpNode, SexpValue},
        diagnostic::DiagnosticCode,
        grammar::{PrecedenceValue, RawGrammarJson, RawRuleJson},
        lexical::{LeadingExtrasPolicy, LexicalFacts, LexicalRootKind, ScannerHostOperation},
        parser::{
            LookaheadSymbol, ParseStateId, ParseTable, ParserGenerationStage, ParserGrammar,
            ReducedExternalScan, ReducedExternalScanResult, ReducedExternalScanner,
            ReducedParseReport, ReducedParser, RuntimeParser, ScannerSnapshotId,
        },
        query::{HighlightCapture, WellKnownQuery},
        runtime_input::{PointBytes, PointRange, Row, Utf8ColumnBytes},
        scanner::TreeSitterScannerKind,
        validated::{ExternalTokenDecl, ValidatedGrammar},
    };
    use rediff::assert_same;

    use super::*;

    const CSS_FIXTURE: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/packages/tree-sitter-css-reduced"
    );
    const JSON_FIXTURE: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/packages/tree-sitter-json-reduced"
    );

    const MINI_GRAMMAR: &str = r#"{
      "$schema": "https://tree-sitter.github.io/tree-sitter/assets/schemas/grammar.schema.json",
      "name": "mini_css",
      "inherits": "css_base",
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
        { "type": "STRING", "value": "@nest" },
        { "type": "SYMBOL", "name": "__error_recovery" }
      ],
      "inline": ["_top_level_item"],
      "reserved": {
        "default": [{ "type": "STRING", "value": "initial" }]
      }
    }"#;

    #[test]
    fn imports_raw_grammar_json_in_rule_order() {
        let grammar = RawGrammarJson::from_tree_sitter_json_str(MINI_GRAMMAR).unwrap();

        assert_eq!(grammar.name, "mini_css");
        assert_eq!(grammar.inherits.as_deref(), Some("css_base"));
        assert_eq!(
            grammar
                .start_rule()
                .map(|(name, _)| name.as_str().to_owned()),
            Some("stylesheet".to_string())
        );
        assert_eq!(
            grammar
                .rules
                .get_index(1)
                .map(|(name, _)| name.as_str().to_owned()),
            Some("rule_set".to_string())
        );
        assert!(matches!(
            grammar.rule("selector"),
            Some(RawRuleJson::Choice { .. })
        ));
        assert_eq!(grammar.externals.len(), 3);
        assert_eq!(
            grammar.reserved.get_index(0).map(|(name, _)| name),
            Some("default")
        );
    }

    #[test]
    fn imports_precedence_wrapped_rules() {
        let named: RawRuleJson = facet_json::from_str(
            r#"{
              "type": "PREC_LEFT",
              "value": "selector",
              "content": { "type": "SYMBOL", "name": "selector" }
            }"#,
        )
        .unwrap();
        let integer: RawRuleJson = facet_json::from_str(
            r#"{
              "type": "PREC",
              "value": 1,
              "content": { "type": "STRING", "value": "!" }
            }"#,
        )
        .unwrap();

        assert!(matches!(
            named,
            RawRuleJson::PrecLeft {
                value: PrecedenceValue::Name(_),
                ..
            }
        ));
        assert!(matches!(
            integer,
            RawRuleJson::Prec {
                value: PrecedenceValue::Integer(1),
                ..
            }
        ));
    }

    #[test]
    fn imports_named_precedence_entries() {
        let grammar = RawGrammarJson::from_tree_sitter_json_str(
            r#"{
              "name": "prec_names",
              "rules": {
                "source": { "type": "STRING", "value": "x" }
              },
              "precedences": [
                [
                  "property_name",
                  { "type": "SYMBOL", "name": "call" }
                ]
              ]
            }"#,
        )
        .unwrap();

        assert_eq!(grammar.precedences.len(), 1);
    }

    #[test]
    fn imports_pinned_tree_sitter_css_fixture() {
        let package = TreeSitterPackageImporter::new(CSS_FIXTURE)
            .import()
            .unwrap();
        let grammar = package.first_grammar();

        assert_eq!(package.language_name(), "css");
        let validated = ValidatedGrammar::from_raw(&grammar.grammar.body.grammar).unwrap();
        assert_eq!(validated.language_name().as_str(), "css");
        assert_eq!(validated.rule_count(), 66);
        assert_eq!(
            validated.rule(validated.start_rule()).name().as_str(),
            "stylesheet"
        );
        assert_eq!(validated.external_count(), 3);
        assert_eq!(validated.external_valid_symbol_mask_width(), 3);
        assert_eq!(validated.extra_count(), 3);
        assert_eq!(validated.inline_count(), 2);
        assert_eq!(validated.field_count(), 0);
        assert_eq!(validated.conflict_count(), 0);
        assert_eq!(validated.precedence_group_count(), 0);
        assert_eq!(validated.supertype_count(), 0);
        assert_eq!(
            validated
                .externals()
                .iter()
                .map(|external| external.name().unwrap_or("<anonymous>"))
                .collect::<Vec<_>>(),
            [
                "_descendant_operator",
                "_pseudo_class_selector_colon",
                "__error_recovery",
            ]
        );
        assert!(validated.has_visible_node_kind("function_name"));
        let lexical = LexicalFacts::from_grammar(&validated);
        assert_eq!(lexical.valid_symbol_mask_width(), 3);
        assert_eq!(lexical.scanner_abi().valid_symbol_mask_width(), 3);
        assert!(lexical.scanner_abi().supports_serialized_state());
        assert_eq!(
            lexical.scanner_abi().operations(),
            [
                ScannerHostOperation::ReadLookahead,
                ScannerHostOperation::Advance { skip: false },
                ScannerHostOperation::Advance { skip: true },
                ScannerHostOperation::MarkEnd,
                ScannerHostOperation::SetResultSymbol,
                ScannerHostOperation::IsAtEnd,
                ScannerHostOperation::Serialize,
                ScannerHostOperation::Deserialize,
            ]
        );
        assert_eq!(lexical.extra_roots().len(), 3);
        assert!(lexical.lexical_roots().iter().any(|root| {
            root.kind == LexicalRootKind::ImmediateToken
                && root.leading_extras == LeadingExtrasPolicy::Forbidden
        }));
        assert!(
            lexical
                .terminals()
                .iter()
                .any(|terminal| terminal.spelling == "\\d+")
        );
        assert_eq!(
            lexical
                .external_tokens()
                .iter()
                .map(|token| (
                    token.ordinal().get(),
                    token.name().unwrap_or("<anonymous>"),
                    token.declaration().clone()
                ))
                .collect::<Vec<_>>(),
            [
                (
                    0,
                    "_descendant_operator",
                    ExternalTokenDecl::Symbol {
                        name: "_descendant_operator".to_owned(),
                    },
                ),
                (
                    1,
                    "_pseudo_class_selector_colon",
                    ExternalTokenDecl::Symbol {
                        name: "_pseudo_class_selector_colon".to_owned(),
                    },
                ),
                (
                    2,
                    "__error_recovery",
                    ExternalTokenDecl::Symbol {
                        name: "__error_recovery".to_owned(),
                    },
                ),
            ]
        );
        let parser_grammar = ParserGrammar::seed_from_validated(&validated, &lexical);
        assert_eq!(parser_grammar.stage(), ParserGenerationStage::SymbolDomains);
        assert_eq!(parser_grammar.start().get(), validated.start_rule().get());
        assert_eq!(parser_grammar.symbols().nonterminals().len(), 66);
        assert_eq!(parser_grammar.symbols().externals().len(), 3);
        let _ = parser_grammar.symbols().eof();
        assert_eq!(parser_grammar.symbols().internal().len(), 3);
        assert!(parser_grammar.production_metadata().is_empty());
        assert!(parser_grammar.field_maps().is_empty());
        assert!(parser_grammar.alias_sequences().is_empty());
        assert!(parser_grammar.reserved_contexts().is_empty());
        assert!(parser_grammar.valid_symbol_sets().is_empty());
        assert_eq!(parser_grammar.extra_roots().len(), 3);
        assert!(parser_grammar.word().is_none());
        assert!(parser_grammar.supertypes().is_empty());
        assert!(parser_grammar.precedence_groups().is_empty());
        assert!(parser_grammar.glr_plan().conflicts().is_empty());
        assert!(
            parser_grammar
                .public_node_kinds()
                .iter()
                .any(|kind| kind.name() == "stylesheet")
        );
        assert!(
            parser_grammar
                .public_node_kinds()
                .iter()
                .any(|kind| kind.name() == "function_name")
        );
        assert!(
            parser_grammar
                .symbols()
                .terminals()
                .iter()
                .any(|terminal| terminal.spelling() == "\\d+")
        );
        assert!(
            parser_grammar
                .symbols()
                .nonterminals()
                .iter()
                .any(|symbol| symbol.name() == "stylesheet" && symbol.visible())
        );
        assert!(
            parser_grammar
                .symbols()
                .nonterminals()
                .iter()
                .any(|symbol| symbol.name() == "_block_item" && symbol.inline())
        );
        let normalized_parser_grammar =
            ParserGrammar::normalize_from_validated(&validated, &lexical).unwrap();
        assert_eq!(
            normalized_parser_grammar.stage(),
            ParserGenerationStage::ProductionsPrepared
        );
        assert!(!normalized_parser_grammar.productions().is_empty());
        assert_eq!(normalized_parser_grammar.symbols().externals().len(), 3);
        assert!(
            normalized_parser_grammar
                .symbols()
                .terminals()
                .iter()
                .any(|terminal| terminal.kind() == crate::parser::ParserTerminalKind::Token)
        );
        assert!(
            normalized_parser_grammar.symbols().terminals().iter().any(
                |terminal| terminal.kind() == crate::parser::ParserTerminalKind::ImmediateToken
            )
        );
        assert!(
            normalized_parser_grammar
                .symbols()
                .nonterminals()
                .iter()
                .any(|symbol| symbol.origin() == crate::parser::NonterminalOrigin::RepeatAuxiliary)
        );
        assert!(!normalized_parser_grammar.alias_sequences().is_empty());
        assert!(!normalized_parser_grammar.provenances().is_empty());
        assert!(normalized_parser_grammar.fields().is_empty());
        assert!(!normalized_parser_grammar.aliases().is_empty());
        assert!(!normalized_parser_grammar.lexical_rules().is_empty());
        assert_eq!(normalized_parser_grammar.inline_rules().len(), 2);
        let prepared_parser_grammar = normalized_parser_grammar
            .clone()
            .prepare_productions_for_items()
            .unwrap();
        assert_eq!(
            prepared_parser_grammar.stage(),
            ParserGenerationStage::Productions
        );
        assert_eq!(
            prepared_parser_grammar
                .item_preparation()
                .unwrap()
                .inline_expansions()
                .len(),
            2
        );
        assert!(
            prepared_parser_grammar
                .item_preparation()
                .unwrap()
                .graph()
                .reachable()
                .contains(&prepared_parser_grammar.start())
        );
        assert!(
            normalized_parser_grammar
                .public_node_kinds()
                .iter()
                .any(|kind| kind.name() == "~")
        );
        let highlights_query = grammar
            .queries
            .well_known(WellKnownQuery::Highlights)
            .unwrap();
        let query_literals = highlights_query.body.anonymous_node_literals();
        assert_eq!(
            query_literals,
            [
                "#",
                "$=",
                "*",
                "*=",
                "+",
                ",",
                "-",
                ".",
                "/",
                ":",
                "::",
                ";",
                "=",
                ">",
                "@charset",
                "@import",
                "@keyframes",
                "@media",
                "@namespace",
                "@supports",
                "^=",
                "and",
                "not",
                "only",
                "or",
                "|=",
                "~",
                "~=",
                "{",
                "}",
                "(",
                ")",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect::<BTreeSet<_>>()
        );
        for literal in &query_literals {
            assert!(
                normalized_parser_grammar
                    .public_node_kinds()
                    .iter()
                    .any(|kind| kind.name() == literal.as_str()),
                "missing query-visible literal {literal}"
            );
            assert!(
                normalized_parser_grammar
                    .public_literal_terminals()
                    .iter()
                    .any(|mapping| mapping.literal() == literal.as_str()
                        && !mapping.terminals().is_empty()),
                "missing terminal provenance for query-visible literal {literal}"
            );
        }
        let query_named_nodes = highlights_query.body.named_node_references();
        for node in &query_named_nodes {
            assert!(
                validated.has_visible_node_kind(node)
                    || normalized_parser_grammar
                        .public_node_kinds()
                        .iter()
                        .any(|kind| kind.name() == node.as_str()),
                "missing query-visible named node {node}"
            );
        }
        assert_eq!(
            grammar
                .grammar
                .body
                .grammar
                .start_rule()
                .map(|(name, _)| name.as_str().to_owned()),
            Some("stylesheet".to_string())
        );
        assert_eq!(grammar.grammar.body.grammar.rules.len(), 66);
        assert_eq!(grammar.grammar.body.grammar.externals.len(), 3);
        assert!(package.manifest.is_some());
        assert!(grammar.node_types_json.is_some());
        let manifest = &package.manifest.as_ref().unwrap().body.config;
        assert_eq!(manifest.grammars[0].name, "css");
        assert_eq!(manifest.grammars[0].scope, "source.css");
        assert_eq!(
            manifest.grammars[0].highlights.as_ref().unwrap().as_slice(),
            ["queries/highlights.scm"]
        );
        let node_types = &grammar.node_types_json.as_ref().unwrap().body.node_types;
        assert!(
            node_types
                .iter()
                .any(|node| node.kind == "stylesheet" && node.root)
        );
        assert_eq!(grammar.scanners.len(), 1);
        assert_eq!(grammar.scanners[0].kind, TreeSitterScannerKind::C);
        assert_eq!(grammar.scanners[0].externals.len(), 3);
        assert!(
            grammar
                .queries
                .well_known(WellKnownQuery::Highlights)
                .is_some()
        );
        assert!(
            grammar.corpus.iter().any(|fixture| fixture
                .source
                .path
                .as_str()
                .starts_with("test/highlight/"))
        );
        let highlight_fixture = grammar
            .corpus
            .iter()
            .find(|fixture| fixture.source.path.as_str() == "test/highlight/test_css.css")
            .unwrap();
        assert_eq!(highlight_fixture.kind, CorpusKind::Highlight);
        let highlight_assertions = highlight_fixture.parse_css_highlight_assertions().unwrap();
        assert_eq!(highlight_assertions, css_highlight_assertions());
        let highlight_captures = highlights_query.body.capture_names();
        for assertion in &highlight_assertions {
            assert!(
                highlight_captures.contains(&assertion.expected_capture_name),
                "query missing capture `{}`",
                assertion.expected_capture_name
            );
        }
        let parse_fixture = grammar
            .corpus
            .iter()
            .find(|fixture| fixture.source.path.as_str() == "test/corpus/stylesheets.txt")
            .unwrap();
        assert_eq!(parse_fixture.kind, CorpusKind::Parse);
        let cases = parse_fixture.parse_cases().unwrap();
        assert_eq!(cases.len(), 1);
        assert_eq!(cases[0].name, "Rule sets");
        assert!(cases[0].attributes.is_empty());
        assert_eq!(cases[0].expected.kind, "stylesheet");
        assert_eq!(cases[0].expected.children.len(), 1);
        let SexpValue::Node(rule_set) = &cases[0].expected.children[0].value else {
            panic!("stylesheet child should be rule_set node");
        };
        assert_eq!(rule_set.kind, "rule_set");
        assert_eq!(rule_set.children.len(), 2);
        let SexpValue::Node(selectors) = &rule_set.children[0].value else {
            panic!("rule_set first child should be selectors node");
        };
        assert_eq!(selectors.kind, "selectors");
        let SexpValue::Node(block) = &rule_set.children[1].value else {
            panic!("rule_set second child should be block node");
        };
        assert_eq!(block.kind, "block");
        assert_eq!(
            cases[0].expected.to_sexp(),
            "(stylesheet (rule_set (selectors (id_selector (id_name))) (block (declaration (property_name) (integer_value (unit))))))"
        );
        let parse_paths = grammar
            .corpus
            .iter()
            .filter(|fixture| fixture.kind == CorpusKind::Parse)
            .map(|fixture| fixture.source.path.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            parse_paths,
            [
                "test/corpus/declarations.txt",
                "test/corpus/selectors.txt",
                "test/corpus/statements.txt",
                "test/corpus/stylesheets.txt",
            ]
        );
        let parse_case_count = grammar
            .corpus
            .iter()
            .filter(|fixture| fixture.kind == CorpusKind::Parse)
            .map(|fixture| fixture.parse_cases().unwrap().len())
            .sum::<usize>();
        assert_eq!(parse_case_count, 40);
        let mut expected_node_kinds = BTreeSet::new();
        for fixture in grammar
            .corpus
            .iter()
            .filter(|fixture| fixture.kind == CorpusKind::Parse)
        {
            for case in fixture.parse_cases().unwrap() {
                collect_node_kinds(&case.expected, &mut expected_node_kinds);
            }
        }
        for kind in [
            "import_statement",
            "namespace_statement",
            "keyframes_statement",
            "media_statement",
            "supports_statement",
            "scope_statement",
            "postcss_statement",
            "pseudo_class_selector",
            "pseudo_element_selector",
            "descendant_selector",
            "escape_sequence",
        ] {
            assert!(expected_node_kinds.contains(kind), "missing `{kind}`");
        }
        for kind in &expected_node_kinds {
            assert!(
                matches!(kind.as_str(), "ERROR" | "MISSING")
                    || validated.has_visible_node_kind(kind),
                "expected corpus node kind `{kind}` to be a visible grammar node"
            );
        }

        let mut source_ids = Vec::new();
        if let Some(file) = &package.manifest {
            source_ids.push((file.path.as_str().to_owned(), file.id.get()));
        }
        source_ids.push((
            grammar.grammar.path.as_str().to_owned(),
            grammar.grammar.id.get(),
        ));
        if let Some(file) = &grammar.node_types_json {
            source_ids.push((file.path.as_str().to_owned(), file.id.get()));
        }
        for scanner in &grammar.scanners {
            source_ids.push((
                scanner.source.path.as_str().to_owned(),
                scanner.source.id.get(),
            ));
        }
        for file in grammar.queries.iter() {
            source_ids.push((file.path.as_str().to_owned(), file.id.get()));
        }
        for fixture in &grammar.corpus {
            source_ids.push((
                fixture.source.path.as_str().to_owned(),
                fixture.source.id.get(),
            ));
        }

        assert_eq!(
            source_ids,
            [
                ("tree-sitter.json".to_string(), 0),
                ("src/grammar.json".to_string(), 1),
                ("src/node-types.json".to_string(), 2),
                ("src/scanner.c".to_string(), 3),
                ("queries/highlights.scm".to_string(), 4),
                ("test/corpus/declarations.txt".to_string(), 5),
                ("test/corpus/selectors.txt".to_string(), 6),
                ("test/corpus/statements.txt".to_string(), 7),
                ("test/corpus/stylesheets.txt".to_string(), 8),
                ("test/highlight/test_css.css".to_string(), 9),
            ]
        );
    }

    #[test]
    fn executes_pinned_css_highlight_assertions_through_runtime_query() {
        let package = TreeSitterPackageImporter::new(CSS_FIXTURE)
            .import()
            .unwrap();
        let grammar = package.first_grammar();
        let validated = ValidatedGrammar::from_raw(&grammar.grammar.body.grammar).unwrap();
        let lexical = LexicalFacts::from_grammar(&validated);
        let parser_grammar = ParserGrammar::normalize_from_validated(&validated, &lexical)
            .unwrap()
            .prepare_productions_for_items()
            .unwrap();
        let parse_table = ParseTable::from_grammar(&parser_grammar).unwrap();
        let highlights_query = grammar
            .queries
            .well_known(WellKnownQuery::Highlights)
            .unwrap();
        let highlight_fixture = grammar
            .corpus
            .iter()
            .find(|fixture| fixture.source.path.as_str() == "test/highlight/test_css.css")
            .unwrap();
        let assertions = highlight_fixture.parse_css_highlight_assertions().unwrap();
        let scanner = CssReducedExternalScanner::new(grammar, &parser_grammar);
        let report = RuntimeParser::new(&validated, &parser_grammar, &parse_table)
            .unwrap()
            .with_external_scanner(&scanner)
            .parse_with_report(&highlight_fixture.source.body.0)
            .unwrap();
        let captures = highlights_query.body.execute_runtime_highlights(
            &parser_grammar,
            &report,
            &highlight_fixture.source.body.0,
        );

        assert_css_highlight_assertions_covered(&assertions, &captures);
        assert!(
            captures.iter().any(|capture| {
                capture.capture_name() == "variable" && capture.text().starts_with("--")
            }),
            "expected #match? to capture custom-property values"
        );
        assert!(
            captures
                .iter()
                .filter(|capture| capture.capture_name() == "variable")
                .all(|capture| capture.text().starts_with("--")),
            "expected #match? to reject non-custom-property variable captures: {:?}",
            captures
                .iter()
                .filter(|capture| capture.capture_name() == "variable")
                .collect::<Vec<_>>()
        );
    }

    #[cfg(feature = "weavy-lowering")]
    #[test]
    fn executes_pinned_css_highlight_assertions_through_weavy_runtime_query() {
        let package = TreeSitterPackageImporter::new(CSS_FIXTURE)
            .import()
            .unwrap();
        let grammar = package.first_grammar();
        let validated = ValidatedGrammar::from_raw(&grammar.grammar.body.grammar).unwrap();
        let lexical = LexicalFacts::from_grammar(&validated);
        let parser_grammar = ParserGrammar::normalize_from_validated(&validated, &lexical)
            .unwrap()
            .prepare_productions_for_items()
            .unwrap();
        let parse_table = ParseTable::from_grammar(&parser_grammar).unwrap();
        let highlights_query = grammar
            .queries
            .well_known(WellKnownQuery::Highlights)
            .unwrap();
        let highlight_fixture = grammar
            .corpus
            .iter()
            .find(|fixture| fixture.source.path.as_str() == "test/highlight/test_css.css")
            .unwrap();
        let assertions = highlight_fixture.parse_css_highlight_assertions().unwrap();
        let scanner = CssReducedExternalScanner::new(grammar, &parser_grammar);
        let runtime_report = RuntimeParser::new(&validated, &parser_grammar, &parse_table)
            .unwrap()
            .with_external_scanner(&scanner)
            .parse_with_report(&highlight_fixture.source.body.0)
            .unwrap();
        let plan =
            crate::lower::weavy::lower_reduced_parser(&parser_grammar, &parse_table).unwrap();
        let weavy_report = crate::lower::weavy::parse_runtime_with_report_and_scanner(
            &plan,
            &validated,
            &parser_grammar,
            &parse_table,
            &highlight_fixture.source.body.0,
            Some(&scanner),
        )
        .unwrap();
        let captures = highlights_query
            .body
            .execute_runtime_highlights_from_tree_events(
                &parser_grammar,
                &weavy_report.accepted_tree_events(),
                &highlight_fixture.source.body.0,
            );

        assert_same!(weavy_report.tree(), runtime_report.tree());
        assert_eq!(weavy_report.trace_events(), runtime_report.trace_events());
        assert_eq!(weavy_report.tree_events(), runtime_report.tree_events());
        assert_css_highlight_assertions_covered(&assertions, &captures);
        assert!(
            captures
                .iter()
                .filter(|capture| capture.capture_name() == "variable")
                .all(|capture| capture.text().starts_with("--")),
            "expected #match? to reject non-custom-property variable captures: {:?}",
            captures
                .iter()
                .filter(|capture| capture.capture_name() == "variable")
                .collect::<Vec<_>>()
        );
        assert!(weavy_report.stats().step_count > 0);
        assert!(weavy_report.stats().block_call_count > 0);
    }

    #[test]
    fn parses_pinned_css_universal_selector_corpus_case() {
        let package = TreeSitterPackageImporter::new(CSS_FIXTURE)
            .import()
            .unwrap();
        let grammar = package.first_grammar();
        let validated = ValidatedGrammar::from_raw(&grammar.grammar.body.grammar).unwrap();
        let lexical = LexicalFacts::from_grammar(&validated);
        let parser_grammar = ParserGrammar::normalize_from_validated(&validated, &lexical)
            .unwrap()
            .prepare_productions_for_items()
            .unwrap();
        let parse_table = ParseTable::from_grammar(&parser_grammar).unwrap();
        let selector_fixture = grammar
            .corpus
            .iter()
            .find(|fixture| fixture.source.path.as_str() == "test/corpus/selectors.txt")
            .unwrap();
        let selector_cases = selector_fixture.parse_cases().unwrap();

        assert_eq!(selector_cases[0].name, "Universal selectors");
        let actual_tree = parse_reduced_or_panic(
            grammar,
            &validated,
            &parser_grammar,
            &parse_table,
            &selector_cases[0].input,
        );

        assert_same!(actual_tree, selector_cases[0].expected);
        assert_eq!(
            actual_tree.to_sexp(),
            "(stylesheet (rule_set (selectors (universal_selector)) (block)))"
        );
    }

    #[cfg(feature = "weavy-lowering")]
    #[test]
    fn parses_pinned_css_universal_selector_through_weavy_lowering() {
        let package = TreeSitterPackageImporter::new(CSS_FIXTURE)
            .import()
            .unwrap();
        let grammar = package.first_grammar();
        let validated = ValidatedGrammar::from_raw(&grammar.grammar.body.grammar).unwrap();
        let lexical = LexicalFacts::from_grammar(&validated);
        let parser_grammar = ParserGrammar::normalize_from_validated(&validated, &lexical)
            .unwrap()
            .prepare_productions_for_items()
            .unwrap();
        let parse_table = ParseTable::from_grammar(&parser_grammar).unwrap();
        let selector_fixture = grammar
            .corpus
            .iter()
            .find(|fixture| fixture.source.path.as_str() == "test/corpus/selectors.txt")
            .unwrap();
        let selector_cases = selector_fixture.parse_cases().unwrap();

        assert_eq!(selector_cases[0].name, "Universal selectors");
        let rust_report = parse_reduced_report_or_panic(
            grammar,
            &validated,
            &parser_grammar,
            &parse_table,
            &selector_cases[0].input,
        );
        let plan =
            crate::lower::weavy::lower_reduced_parser(&parser_grammar, &parse_table).unwrap();
        let (weavy_tree, stats) = crate::lower::weavy::parse_reduced_with_plan(
            &plan,
            &validated,
            &parser_grammar,
            &parse_table,
            &selector_cases[0].input,
        )
        .unwrap();

        assert_same!(&weavy_tree, rust_report.tree());
        assert_same!(weavy_tree, selector_cases[0].expected);
        assert!(stats.step_count > 0);
        assert!(stats.block_call_count > 0);
    }

    #[test]
    fn parses_pinned_css_type_selectors_corpus_case() {
        let package = TreeSitterPackageImporter::new(CSS_FIXTURE)
            .import()
            .unwrap();
        let grammar = package.first_grammar();
        let validated = ValidatedGrammar::from_raw(&grammar.grammar.body.grammar).unwrap();
        let lexical = LexicalFacts::from_grammar(&validated);
        let parser_grammar = ParserGrammar::normalize_from_validated(&validated, &lexical)
            .unwrap()
            .prepare_productions_for_items()
            .unwrap();
        let parse_table = ParseTable::from_grammar(&parser_grammar).unwrap();
        let selector_fixture = grammar
            .corpus
            .iter()
            .find(|fixture| fixture.source.path.as_str() == "test/corpus/selectors.txt")
            .unwrap();
        let selector_cases = selector_fixture.parse_cases().unwrap();

        assert_eq!(selector_cases[1].name, "Type selectors");
        let actual_tree = parse_reduced_or_panic(
            grammar,
            &validated,
            &parser_grammar,
            &parse_table,
            &selector_cases[1].input,
        );

        assert_same!(actual_tree, selector_cases[1].expected);
        assert_eq!(
            actual_tree.to_sexp(),
            "(stylesheet (rule_set (selectors (tag_name) (tag_name)) (block)) (rule_set (selectors (tag_name) (tag_name) (tag_name) (tag_name)) (block)))"
        );
    }

    #[test]
    fn parses_pinned_css_class_selectors_corpus_case() {
        let package = TreeSitterPackageImporter::new(CSS_FIXTURE)
            .import()
            .unwrap();
        let grammar = package.first_grammar();
        let validated = ValidatedGrammar::from_raw(&grammar.grammar.body.grammar).unwrap();
        let lexical = LexicalFacts::from_grammar(&validated);
        let parser_grammar = ParserGrammar::normalize_from_validated(&validated, &lexical)
            .unwrap()
            .prepare_productions_for_items()
            .unwrap();
        let parse_table = ParseTable::from_grammar(&parser_grammar).unwrap();
        let selector_fixture = grammar
            .corpus
            .iter()
            .find(|fixture| fixture.source.path.as_str() == "test/corpus/selectors.txt")
            .unwrap();
        let selector_cases = selector_fixture.parse_cases().unwrap();

        assert_eq!(selector_cases[2].name, "Class selectors");
        let actual_tree = parse_reduced_or_panic(
            grammar,
            &validated,
            &parser_grammar,
            &parse_table,
            &selector_cases[2].input,
        );

        assert_same!(actual_tree, selector_cases[2].expected);
        assert_eq!(
            actual_tree.to_sexp(),
            "(stylesheet (rule_set (selectors (class_selector (class_name (identifier)))) (block)) (rule_set (selectors (class_selector (tag_name) (class_name (identifier))) (class_selector (class_selector (class_name (identifier))) (class_name (identifier)))) (block)))"
        );
    }

    #[cfg(feature = "weavy-lowering")]
    #[test]
    fn parses_pinned_css_class_selectors_through_weavy_lowering() {
        let package = TreeSitterPackageImporter::new(CSS_FIXTURE)
            .import()
            .unwrap();
        let grammar = package.first_grammar();
        let validated = ValidatedGrammar::from_raw(&grammar.grammar.body.grammar).unwrap();
        let lexical = LexicalFacts::from_grammar(&validated);
        let parser_grammar = ParserGrammar::normalize_from_validated(&validated, &lexical)
            .unwrap()
            .prepare_productions_for_items()
            .unwrap();
        let parse_table = ParseTable::from_grammar(&parser_grammar).unwrap();
        let selector_fixture = grammar
            .corpus
            .iter()
            .find(|fixture| fixture.source.path.as_str() == "test/corpus/selectors.txt")
            .unwrap();
        let selector_cases = selector_fixture.parse_cases().unwrap();

        assert_eq!(selector_cases[2].name, "Class selectors");
        let rust_report = parse_reduced_report_or_panic(
            grammar,
            &validated,
            &parser_grammar,
            &parse_table,
            &selector_cases[2].input,
        );
        let plan =
            crate::lower::weavy::lower_reduced_parser(&parser_grammar, &parse_table).unwrap();
        let (weavy_tree, stats) = crate::lower::weavy::parse_reduced_with_plan(
            &plan,
            &validated,
            &parser_grammar,
            &parse_table,
            &selector_cases[2].input,
        )
        .unwrap();

        assert_same!(&weavy_tree, rust_report.tree());
        assert_same!(weavy_tree, selector_cases[2].expected);
        assert!(stats.step_count > 0);
        assert!(stats.block_call_count > 0);
    }

    #[test]
    fn parses_pinned_css_id_selectors_corpus_case() {
        let package = TreeSitterPackageImporter::new(CSS_FIXTURE)
            .import()
            .unwrap();
        let grammar = package.first_grammar();
        let validated = ValidatedGrammar::from_raw(&grammar.grammar.body.grammar).unwrap();
        let lexical = LexicalFacts::from_grammar(&validated);
        let parser_grammar = ParserGrammar::normalize_from_validated(&validated, &lexical)
            .unwrap()
            .prepare_productions_for_items()
            .unwrap();
        let parse_table = ParseTable::from_grammar(&parser_grammar).unwrap();
        let selector_fixture = grammar
            .corpus
            .iter()
            .find(|fixture| fixture.source.path.as_str() == "test/corpus/selectors.txt")
            .unwrap();
        let selector_cases = selector_fixture.parse_cases().unwrap();

        assert_eq!(selector_cases[3].name, "Id selectors");
        let actual_tree = parse_reduced_or_panic(
            grammar,
            &validated,
            &parser_grammar,
            &parse_table,
            &selector_cases[3].input,
        );

        assert_same!(actual_tree, selector_cases[3].expected);
        assert_eq!(
            actual_tree.to_sexp(),
            "(stylesheet (rule_set (selectors (id_selector (id_name)) (id_selector (tag_name) (id_name))) (block)))"
        );
    }

    #[test]
    fn parses_pinned_css_attribute_selectors_corpus_case() {
        let package = TreeSitterPackageImporter::new(CSS_FIXTURE)
            .import()
            .unwrap();
        let grammar = package.first_grammar();
        let validated = ValidatedGrammar::from_raw(&grammar.grammar.body.grammar).unwrap();
        let lexical = LexicalFacts::from_grammar(&validated);
        let parser_grammar = ParserGrammar::normalize_from_validated(&validated, &lexical)
            .unwrap()
            .prepare_productions_for_items()
            .unwrap();
        let parse_table = ParseTable::from_grammar(&parser_grammar).unwrap();
        let selector_fixture = grammar
            .corpus
            .iter()
            .find(|fixture| fixture.source.path.as_str() == "test/corpus/selectors.txt")
            .unwrap();
        let selector_cases = selector_fixture.parse_cases().unwrap();

        assert_eq!(selector_cases[4].name, "Attribute selectors");
        let actual_tree = parse_reduced_or_panic(
            grammar,
            &validated,
            &parser_grammar,
            &parse_table,
            &selector_cases[4].input,
        );

        assert_same!(actual_tree, selector_cases[4].expected);
        assert_eq!(
            actual_tree.to_sexp(),
            "(stylesheet (rule_set (selectors (attribute_selector (attribute_name))) (block)) (rule_set (selectors (attribute_selector (attribute_name) (plain_value))) (block)) (rule_set (selectors (attribute_selector (attribute_name) (plain_value))) (block)) (rule_set (selectors (attribute_selector (tag_name) (attribute_name))) (block)))"
        );
    }

    #[test]
    fn parses_pinned_css_pseudo_class_selectors_corpus_case() {
        let package = TreeSitterPackageImporter::new(CSS_FIXTURE)
            .import()
            .unwrap();
        let grammar = package.first_grammar();
        let validated = ValidatedGrammar::from_raw(&grammar.grammar.body.grammar).unwrap();
        let lexical = LexicalFacts::from_grammar(&validated);
        let parser_grammar = ParserGrammar::normalize_from_validated(&validated, &lexical)
            .unwrap()
            .prepare_productions_for_items()
            .unwrap();
        let parse_table = ParseTable::from_grammar(&parser_grammar).unwrap();
        let selector_fixture = grammar
            .corpus
            .iter()
            .find(|fixture| fixture.source.path.as_str() == "test/corpus/selectors.txt")
            .unwrap();
        let selector_cases = selector_fixture.parse_cases().unwrap();

        assert_eq!(selector_cases[5].name, "Pseudo-class selectors");
        let actual_tree = parse_reduced_or_panic(
            grammar,
            &validated,
            &parser_grammar,
            &parse_table,
            &selector_cases[5].input,
        );

        assert_same!(actual_tree, selector_cases[5].expected);
    }

    #[test]
    fn parses_pinned_css_pseudo_class_selectors_through_runtime_scanner_snapshots() {
        let package = TreeSitterPackageImporter::new(CSS_FIXTURE)
            .import()
            .unwrap();
        let grammar = package.first_grammar();
        let validated = ValidatedGrammar::from_raw(&grammar.grammar.body.grammar).unwrap();
        let lexical = LexicalFacts::from_grammar(&validated);
        let parser_grammar = ParserGrammar::normalize_from_validated(&validated, &lexical)
            .unwrap()
            .prepare_productions_for_items()
            .unwrap();
        let parse_table = ParseTable::from_grammar(&parser_grammar).unwrap();
        let selector_fixture = grammar
            .corpus
            .iter()
            .find(|fixture| fixture.source.path.as_str() == "test/corpus/selectors.txt")
            .unwrap();
        let selector_cases = selector_fixture.parse_cases().unwrap();

        assert_eq!(selector_cases[5].name, "Pseudo-class selectors");
        let css_scanner = CssReducedExternalScanner::new(grammar, &parser_grammar);
        let scanner = RecordingCssReducedExternalScanner::new(&css_scanner);
        let runtime_report = RuntimeParser::new(&validated, &parser_grammar, &parse_table)
            .unwrap()
            .with_external_scanner(&scanner)
            .parse_with_report(&selector_cases[5].input)
            .unwrap();

        assert_same!(runtime_report.tree(), &selector_cases[5].expected);
        assert!(
            scanner.calls.get() > 0,
            "expected RuntimeParser to invoke the reduced external scanner"
        );
        assert!(
            scanner.accepted_pseudo_class_selector_colon.get() > 0,
            "expected RuntimeParser to accept the pseudo-class selector colon external"
        );
        assert_eq!(
            scanner.missing_valid_symbols.get(),
            0,
            "expected every runtime scanner call to carry a valid-symbol mask"
        );
        assert_eq!(
            scanner.invalid_symbol_requests.get(),
            0,
            "expected runtime scanner calls to respect the valid-symbol mask"
        );
        assert!(
            scanner.requests_with_snapshot.get() > 0,
            "expected a later runtime scanner request to receive the stateless CSS snapshot marker committed by an accepted external token"
        );
        let scanner_events = runtime_report.trace_events().iter().filter_map(|event| {
            if let crate::parser::TraceEvent::ExternalScanner {
                before,
                after,
                result,
                ..
            } = event
            {
                Some((*before, *after, *result))
            } else {
                None
            }
        });
        let scanner_events = scanner_events.collect::<Vec<_>>();
        assert!(
            !scanner_events.is_empty(),
            "expected runtime execution to trace external-scanner calls"
        );
        assert!(
            scanner_events
                .iter()
                .all(|(before, after, result)| before.is_some()
                    && after.is_some()
                    && result.is_some()),
            "expected runtime accepted external-scanner results to carry stateless CSS snapshot markers"
        );
        assert!(
            scanner_events
                .iter()
                .all(|(before, after, _)| before == after),
            "expected stateless CSS scanner snapshots to round-trip without changing ids"
        );
        assert!(
            scanner_events
                .iter()
                .any(|(before, _, _)| before.is_some_and(|before| before.get() == 0)),
            "expected runtime scanner traces to observe the committed stateless CSS snapshot marker"
        );
    }

    #[cfg(feature = "weavy-lowering")]
    #[test]
    fn parses_pinned_css_pseudo_class_selectors_through_weavy_external_scanner() {
        let package = TreeSitterPackageImporter::new(CSS_FIXTURE)
            .import()
            .unwrap();
        let grammar = package.first_grammar();
        let validated = ValidatedGrammar::from_raw(&grammar.grammar.body.grammar).unwrap();
        let lexical = LexicalFacts::from_grammar(&validated);
        let parser_grammar = ParserGrammar::normalize_from_validated(&validated, &lexical)
            .unwrap()
            .prepare_productions_for_items()
            .unwrap();
        let parse_table = ParseTable::from_grammar(&parser_grammar).unwrap();
        let selector_fixture = grammar
            .corpus
            .iter()
            .find(|fixture| fixture.source.path.as_str() == "test/corpus/selectors.txt")
            .unwrap();
        let selector_cases = selector_fixture.parse_cases().unwrap();

        assert_eq!(selector_cases[5].name, "Pseudo-class selectors");
        let rust_report = parse_reduced_report_or_panic(
            grammar,
            &validated,
            &parser_grammar,
            &parse_table,
            &selector_cases[5].input,
        );
        let plan =
            crate::lower::weavy::lower_reduced_parser(&parser_grammar, &parse_table).unwrap();
        let css_scanner = CssReducedExternalScanner::new(grammar, &parser_grammar);
        let scanner = RecordingCssReducedExternalScanner::new(&css_scanner);
        let weavy_report = crate::lower::weavy::parse_reduced_with_report_and_scanner(
            &plan,
            &validated,
            &parser_grammar,
            &parse_table,
            &selector_cases[5].input,
            Some(&scanner),
        )
        .unwrap();

        assert_same!(weavy_report.tree(), rust_report.tree());
        assert_same!(weavy_report.tree(), &selector_cases[5].expected);
        assert!(
            scanner.calls.get() > 0,
            "expected Weavy to invoke the reduced external scanner"
        );
        assert!(
            scanner.accepted.get() > 0,
            "expected Weavy to accept at least one reduced external token"
        );
        assert!(
            scanner.accepted_pseudo_class_selector_colon.get() > 0,
            "expected Weavy to accept the pseudo-class selector colon external"
        );
        assert_eq!(
            scanner.missing_valid_symbols.get(),
            0,
            "expected every Weavy scanner call to carry a valid-symbol mask"
        );
        assert_eq!(
            scanner.invalid_symbol_requests.get(),
            0,
            "expected Weavy scanner calls to respect the valid-symbol mask"
        );
        assert!(weavy_report.stats().step_count > 0);
        assert!(weavy_report.stats().block_call_count > 0);
    }

    #[cfg(feature = "weavy-lowering")]
    #[test]
    fn parses_pinned_css_pseudo_class_selectors_through_weavy_runtime_scanner_snapshots() {
        let package = TreeSitterPackageImporter::new(CSS_FIXTURE)
            .import()
            .unwrap();
        let grammar = package.first_grammar();
        let validated = ValidatedGrammar::from_raw(&grammar.grammar.body.grammar).unwrap();
        let lexical = LexicalFacts::from_grammar(&validated);
        let parser_grammar = ParserGrammar::normalize_from_validated(&validated, &lexical)
            .unwrap()
            .prepare_productions_for_items()
            .unwrap();
        let parse_table = ParseTable::from_grammar(&parser_grammar).unwrap();
        let selector_fixture = grammar
            .corpus
            .iter()
            .find(|fixture| fixture.source.path.as_str() == "test/corpus/selectors.txt")
            .unwrap();
        let selector_cases = selector_fixture.parse_cases().unwrap();

        assert_eq!(selector_cases[5].name, "Pseudo-class selectors");
        let runtime_css_scanner = CssReducedExternalScanner::new(grammar, &parser_grammar);
        let runtime_scanner = RecordingCssReducedExternalScanner::new(&runtime_css_scanner);
        let runtime_report = RuntimeParser::new(&validated, &parser_grammar, &parse_table)
            .unwrap()
            .with_external_scanner(&runtime_scanner)
            .parse_with_report(&selector_cases[5].input)
            .unwrap();
        let plan =
            crate::lower::weavy::lower_reduced_parser(&parser_grammar, &parse_table).unwrap();
        let weavy_css_scanner = CssReducedExternalScanner::new(grammar, &parser_grammar);
        let weavy_scanner = RecordingCssReducedExternalScanner::new(&weavy_css_scanner);
        let weavy_report = crate::lower::weavy::parse_runtime_with_report_and_scanner(
            &plan,
            &validated,
            &parser_grammar,
            &parse_table,
            &selector_cases[5].input,
            Some(&weavy_scanner),
        )
        .unwrap();

        assert_same!(weavy_report.tree(), runtime_report.tree());
        assert_same!(weavy_report.tree(), &selector_cases[5].expected);
        assert_eq!(weavy_report.trace_events(), runtime_report.trace_events());
        assert_eq!(weavy_report.tree_events(), runtime_report.tree_events());
        assert!(
            runtime_scanner.accepted_pseudo_class_selector_colon.get() > 0,
            "expected RuntimeParser to accept the pseudo-class selector colon external"
        );
        assert_eq!(
            weavy_scanner.accepted_pseudo_class_selector_colon.get(),
            runtime_scanner.accepted_pseudo_class_selector_colon.get(),
            "expected Weavy runtime to accept the same pseudo-class selector colon count"
        );
        assert_eq!(
            weavy_scanner.missing_valid_symbols.get(),
            0,
            "expected every Weavy runtime scanner call to carry a valid-symbol mask"
        );
        assert_eq!(
            weavy_scanner.invalid_symbol_requests.get(),
            0,
            "expected Weavy runtime scanner calls to respect the valid-symbol mask"
        );
        assert!(
            weavy_scanner.requests_with_snapshot.get() > 0,
            "expected a later Weavy runtime scanner request to receive the stateless CSS snapshot marker committed by an accepted external token"
        );
        let scanner_events = weavy_report.trace_events().iter().filter_map(|event| {
            if let crate::parser::TraceEvent::ExternalScanner {
                before,
                after,
                result,
                ..
            } = event
            {
                Some((*before, *after, *result))
            } else {
                None
            }
        });
        let scanner_events = scanner_events.collect::<Vec<_>>();
        assert!(
            !scanner_events.is_empty(),
            "expected Weavy runtime execution to trace external-scanner calls"
        );
        assert!(
            scanner_events
                .iter()
                .all(|(before, after, result)| before.is_some()
                    && after.is_some()
                    && result.is_some()),
            "expected Weavy runtime accepted external-scanner results to carry stateless CSS snapshot markers"
        );
        assert!(
            scanner_events
                .iter()
                .all(|(before, after, _)| before == after),
            "expected stateless CSS scanner snapshots to round-trip without changing ids"
        );
        assert!(
            scanner_events
                .iter()
                .any(|(before, _, _)| before.is_some_and(|before| before.get() == 0)),
            "expected Weavy runtime scanner traces to observe the committed stateless CSS snapshot marker"
        );
        assert!(weavy_report.stats().step_count > 0);
        assert!(weavy_report.stats().block_call_count > 0);
    }

    #[test]
    fn parses_pinned_css_nth_child_selectors_corpus_case() {
        let package = TreeSitterPackageImporter::new(CSS_FIXTURE)
            .import()
            .unwrap();
        let grammar = package.first_grammar();
        let validated = ValidatedGrammar::from_raw(&grammar.grammar.body.grammar).unwrap();
        let lexical = LexicalFacts::from_grammar(&validated);
        let parser_grammar = ParserGrammar::normalize_from_validated(&validated, &lexical)
            .unwrap()
            .prepare_productions_for_items()
            .unwrap();
        let parse_table = ParseTable::from_grammar(&parser_grammar).unwrap();
        let selector_fixture = grammar
            .corpus
            .iter()
            .find(|fixture| fixture.source.path.as_str() == "test/corpus/selectors.txt")
            .unwrap();
        let selector_cases = selector_fixture.parse_cases().unwrap();

        assert_eq!(
            selector_cases[6].name,
            ":nth-child and :nth-last-child selectors"
        );
        let actual_tree = parse_reduced_or_panic(
            grammar,
            &validated,
            &parser_grammar,
            &parse_table,
            &selector_cases[6].input,
        );

        assert_same!(actual_tree, selector_cases[6].expected);
    }

    #[test]
    fn parses_pinned_css_pseudo_element_selectors_corpus_case() {
        let package = TreeSitterPackageImporter::new(CSS_FIXTURE)
            .import()
            .unwrap();
        let grammar = package.first_grammar();
        let validated = ValidatedGrammar::from_raw(&grammar.grammar.body.grammar).unwrap();
        let lexical = LexicalFacts::from_grammar(&validated);
        let parser_grammar = ParserGrammar::normalize_from_validated(&validated, &lexical)
            .unwrap()
            .prepare_productions_for_items()
            .unwrap();
        let parse_table = ParseTable::from_grammar(&parser_grammar).unwrap();
        let selector_fixture = grammar
            .corpus
            .iter()
            .find(|fixture| fixture.source.path.as_str() == "test/corpus/selectors.txt")
            .unwrap();
        let selector_cases = selector_fixture.parse_cases().unwrap();

        assert_eq!(selector_cases[7].name, "Pseudo-element selectors");
        let actual_tree = parse_reduced_or_panic(
            grammar,
            &validated,
            &parser_grammar,
            &parse_table,
            &selector_cases[7].input,
        );

        assert_same!(actual_tree, selector_cases[7].expected);
    }

    #[test]
    fn parses_pinned_css_slotted_pseudo_element_corpus_case() {
        let package = TreeSitterPackageImporter::new(CSS_FIXTURE)
            .import()
            .unwrap();
        let grammar = package.first_grammar();
        let validated = ValidatedGrammar::from_raw(&grammar.grammar.body.grammar).unwrap();
        let lexical = LexicalFacts::from_grammar(&validated);
        let parser_grammar = ParserGrammar::normalize_from_validated(&validated, &lexical)
            .unwrap()
            .prepare_productions_for_items()
            .unwrap();
        let parse_table = ParseTable::from_grammar(&parser_grammar).unwrap();
        let selector_fixture = grammar
            .corpus
            .iter()
            .find(|fixture| fixture.source.path.as_str() == "test/corpus/selectors.txt")
            .unwrap();
        let selector_cases = selector_fixture.parse_cases().unwrap();

        assert_eq!(selector_cases[8].name, "::slotted pseudo element");
        let actual_tree = parse_reduced_or_panic(
            grammar,
            &validated,
            &parser_grammar,
            &parse_table,
            &selector_cases[8].input,
        );

        assert_same!(actual_tree, selector_cases[8].expected);
    }

    #[test]
    fn parses_pinned_css_child_selectors_corpus_case() {
        let package = TreeSitterPackageImporter::new(CSS_FIXTURE)
            .import()
            .unwrap();
        let grammar = package.first_grammar();
        let validated = ValidatedGrammar::from_raw(&grammar.grammar.body.grammar).unwrap();
        let lexical = LexicalFacts::from_grammar(&validated);
        let parser_grammar = ParserGrammar::normalize_from_validated(&validated, &lexical)
            .unwrap()
            .prepare_productions_for_items()
            .unwrap();
        let parse_table = ParseTable::from_grammar(&parser_grammar).unwrap();
        let selector_fixture = grammar
            .corpus
            .iter()
            .find(|fixture| fixture.source.path.as_str() == "test/corpus/selectors.txt")
            .unwrap();
        let selector_cases = selector_fixture.parse_cases().unwrap();

        assert_eq!(selector_cases[9].name, "Child selectors");
        let actual_tree = parse_reduced_or_panic(
            grammar,
            &validated,
            &parser_grammar,
            &parse_table,
            &selector_cases[9].input,
        );

        assert_same!(actual_tree, selector_cases[9].expected);
    }

    #[test]
    fn parses_pinned_css_descendant_selectors_corpus_case() {
        let package = TreeSitterPackageImporter::new(CSS_FIXTURE)
            .import()
            .unwrap();
        let grammar = package.first_grammar();
        let validated = ValidatedGrammar::from_raw(&grammar.grammar.body.grammar).unwrap();
        let lexical = LexicalFacts::from_grammar(&validated);
        let parser_grammar = ParserGrammar::normalize_from_validated(&validated, &lexical)
            .unwrap()
            .prepare_productions_for_items()
            .unwrap();
        let parse_table = ParseTable::from_grammar(&parser_grammar).unwrap();
        let selector_fixture = grammar
            .corpus
            .iter()
            .find(|fixture| fixture.source.path.as_str() == "test/corpus/selectors.txt")
            .unwrap();
        let selector_cases = selector_fixture.parse_cases().unwrap();

        assert_eq!(selector_cases[10].name, "Descendant selectors");
        let actual_tree = parse_reduced_or_panic(
            grammar,
            &validated,
            &parser_grammar,
            &parse_table,
            &selector_cases[10].input,
        );

        assert_same!(actual_tree, selector_cases[10].expected);
    }

    #[cfg(feature = "weavy-lowering")]
    #[test]
    fn parses_pinned_css_descendant_selectors_through_weavy_external_scanner() {
        let package = TreeSitterPackageImporter::new(CSS_FIXTURE)
            .import()
            .unwrap();
        let grammar = package.first_grammar();
        let validated = ValidatedGrammar::from_raw(&grammar.grammar.body.grammar).unwrap();
        let lexical = LexicalFacts::from_grammar(&validated);
        let parser_grammar = ParserGrammar::normalize_from_validated(&validated, &lexical)
            .unwrap()
            .prepare_productions_for_items()
            .unwrap();
        let parse_table = ParseTable::from_grammar(&parser_grammar).unwrap();
        let selector_fixture = grammar
            .corpus
            .iter()
            .find(|fixture| fixture.source.path.as_str() == "test/corpus/selectors.txt")
            .unwrap();
        let selector_cases = selector_fixture.parse_cases().unwrap();

        assert_eq!(selector_cases[10].name, "Descendant selectors");
        let rust_report = parse_reduced_report_or_panic(
            grammar,
            &validated,
            &parser_grammar,
            &parse_table,
            &selector_cases[10].input,
        );
        let plan =
            crate::lower::weavy::lower_reduced_parser(&parser_grammar, &parse_table).unwrap();
        let css_scanner = CssReducedExternalScanner::new(grammar, &parser_grammar);
        let scanner = RecordingCssReducedExternalScanner::new(&css_scanner);
        let weavy_report = crate::lower::weavy::parse_reduced_with_report_and_scanner(
            &plan,
            &validated,
            &parser_grammar,
            &parse_table,
            &selector_cases[10].input,
            Some(&scanner),
        )
        .unwrap();

        assert_same!(weavy_report.tree(), rust_report.tree());
        assert_same!(weavy_report.tree(), &selector_cases[10].expected);
        assert!(
            scanner.calls.get() > 0,
            "expected Weavy to invoke the reduced external scanner"
        );
        assert!(
            scanner.accepted_descendant_operator.get() > 0,
            "expected Weavy to accept the descendant operator external"
        );
        assert_eq!(
            scanner.missing_valid_symbols.get(),
            0,
            "expected every Weavy scanner call to carry a valid-symbol mask"
        );
        assert_eq!(
            scanner.invalid_symbol_requests.get(),
            0,
            "expected Weavy scanner calls to respect the valid-symbol mask"
        );
        assert!(weavy_report.stats().step_count > 0);
        assert!(weavy_report.stats().block_call_count > 0);
    }

    #[test]
    fn parses_pinned_css_nesting_selectors_corpus_case() {
        let package = TreeSitterPackageImporter::new(CSS_FIXTURE)
            .import()
            .unwrap();
        let grammar = package.first_grammar();
        let validated = ValidatedGrammar::from_raw(&grammar.grammar.body.grammar).unwrap();
        let lexical = LexicalFacts::from_grammar(&validated);
        let parser_grammar = ParserGrammar::normalize_from_validated(&validated, &lexical)
            .unwrap()
            .prepare_productions_for_items()
            .unwrap();
        let parse_table = ParseTable::from_grammar(&parser_grammar).unwrap();
        let selector_fixture = grammar
            .corpus
            .iter()
            .find(|fixture| fixture.source.path.as_str() == "test/corpus/selectors.txt")
            .unwrap();
        let selector_cases = selector_fixture.parse_cases().unwrap();

        assert_eq!(selector_cases[11].name, "Nesting selectors");
        let actual_tree = parse_reduced_or_panic(
            grammar,
            &validated,
            &parser_grammar,
            &parse_table,
            &selector_cases[11].input,
        );

        assert_same!(actual_tree, selector_cases[11].expected);
    }

    #[test]
    fn parses_pinned_css_sibling_selectors_corpus_case() {
        let package = TreeSitterPackageImporter::new(CSS_FIXTURE)
            .import()
            .unwrap();
        let grammar = package.first_grammar();
        let validated = ValidatedGrammar::from_raw(&grammar.grammar.body.grammar).unwrap();
        let lexical = LexicalFacts::from_grammar(&validated);
        let parser_grammar = ParserGrammar::normalize_from_validated(&validated, &lexical)
            .unwrap()
            .prepare_productions_for_items()
            .unwrap();
        let parse_table = ParseTable::from_grammar(&parser_grammar).unwrap();
        let selector_fixture = grammar
            .corpus
            .iter()
            .find(|fixture| fixture.source.path.as_str() == "test/corpus/selectors.txt")
            .unwrap();
        let selector_cases = selector_fixture.parse_cases().unwrap();

        assert_eq!(selector_cases[12].name, "Sibling selectors");
        let actual_tree = parse_reduced_or_panic(
            grammar,
            &validated,
            &parser_grammar,
            &parse_table,
            &selector_cases[12].input,
        );

        assert_same!(actual_tree, selector_cases[12].expected);
    }

    #[test]
    fn parses_pinned_css_not_selector_corpus_case() {
        let package = TreeSitterPackageImporter::new(CSS_FIXTURE)
            .import()
            .unwrap();
        let grammar = package.first_grammar();
        let validated = ValidatedGrammar::from_raw(&grammar.grammar.body.grammar).unwrap();
        let lexical = LexicalFacts::from_grammar(&validated);
        let parser_grammar = ParserGrammar::normalize_from_validated(&validated, &lexical)
            .unwrap()
            .prepare_productions_for_items()
            .unwrap();
        let parse_table = ParseTable::from_grammar(&parser_grammar).unwrap();
        let selector_fixture = grammar
            .corpus
            .iter()
            .find(|fixture| fixture.source.path.as_str() == "test/corpus/selectors.txt")
            .unwrap();
        let selector_cases = selector_fixture.parse_cases().unwrap();

        assert_eq!(selector_cases[13].name, "The :not selector");
        let actual_tree = parse_reduced_or_panic(
            grammar,
            &validated,
            &parser_grammar,
            &parse_table,
            &selector_cases[13].input,
        );

        assert_same!(actual_tree, selector_cases[13].expected);
    }

    #[test]
    fn parses_pinned_css_nested_combinators_corpus_case() {
        let package = TreeSitterPackageImporter::new(CSS_FIXTURE)
            .import()
            .unwrap();
        let grammar = package.first_grammar();
        let validated = ValidatedGrammar::from_raw(&grammar.grammar.body.grammar).unwrap();
        let lexical = LexicalFacts::from_grammar(&validated);
        let parser_grammar = ParserGrammar::normalize_from_validated(&validated, &lexical)
            .unwrap()
            .prepare_productions_for_items()
            .unwrap();
        let parse_table = ParseTable::from_grammar(&parser_grammar).unwrap();
        let selector_fixture = grammar
            .corpus
            .iter()
            .find(|fixture| fixture.source.path.as_str() == "test/corpus/selectors.txt")
            .unwrap();
        let selector_cases = selector_fixture.parse_cases().unwrap();

        assert_eq!(selector_cases[14].name, "Nested combinators");
        let actual_tree = parse_reduced_or_panic(
            grammar,
            &validated,
            &parser_grammar,
            &parse_table,
            &selector_cases[14].input,
        );

        assert_same!(actual_tree, selector_cases[14].expected);
    }

    #[test]
    fn parses_pinned_css_escape_sequences_corpus_case() {
        let package = TreeSitterPackageImporter::new(CSS_FIXTURE)
            .import()
            .unwrap();
        let grammar = package.first_grammar();
        let validated = ValidatedGrammar::from_raw(&grammar.grammar.body.grammar).unwrap();
        let lexical = LexicalFacts::from_grammar(&validated);
        let parser_grammar = ParserGrammar::normalize_from_validated(&validated, &lexical)
            .unwrap()
            .prepare_productions_for_items()
            .unwrap();
        let parse_table = ParseTable::from_grammar(&parser_grammar).unwrap();
        let selector_fixture = grammar
            .corpus
            .iter()
            .find(|fixture| fixture.source.path.as_str() == "test/corpus/selectors.txt")
            .unwrap();
        let selector_cases = selector_fixture.parse_cases().unwrap();

        assert_eq!(selector_cases[15].name, "Escape sequences");
        let actual_tree = parse_reduced_or_panic(
            grammar,
            &validated,
            &parser_grammar,
            &parse_table,
            &selector_cases[15].input,
        );

        assert_same!(actual_tree, selector_cases[15].expected);
    }

    #[test]
    fn parses_pinned_css_function_calls_declaration_corpus_case() {
        let package = TreeSitterPackageImporter::new(CSS_FIXTURE)
            .import()
            .unwrap();
        let grammar = package.first_grammar();
        let validated = ValidatedGrammar::from_raw(&grammar.grammar.body.grammar).unwrap();
        let lexical = LexicalFacts::from_grammar(&validated);
        let parser_grammar = ParserGrammar::normalize_from_validated(&validated, &lexical)
            .unwrap()
            .prepare_productions_for_items()
            .unwrap();
        let parse_table = ParseTable::from_grammar(&parser_grammar).unwrap();
        let declaration_fixture = grammar
            .corpus
            .iter()
            .find(|fixture| fixture.source.path.as_str() == "test/corpus/declarations.txt")
            .unwrap();
        let declaration_cases = declaration_fixture.parse_cases().unwrap();

        assert_eq!(declaration_cases[0].name, "Function calls");
        let actual_tree = parse_reduced_or_panic(
            grammar,
            &validated,
            &parser_grammar,
            &parse_table,
            &declaration_cases[0].input,
        );

        assert_same!(actual_tree, declaration_cases[0].expected);
    }

    #[test]
    fn parses_pinned_css_multi_value_call_arguments_declaration_corpus_case() {
        let package = TreeSitterPackageImporter::new(CSS_FIXTURE)
            .import()
            .unwrap();
        let grammar = package.first_grammar();
        let validated = ValidatedGrammar::from_raw(&grammar.grammar.body.grammar).unwrap();
        let lexical = LexicalFacts::from_grammar(&validated);
        let parser_grammar = ParserGrammar::normalize_from_validated(&validated, &lexical)
            .unwrap()
            .prepare_productions_for_items()
            .unwrap();
        let parse_table = ParseTable::from_grammar(&parser_grammar).unwrap();
        let declaration_fixture = grammar
            .corpus
            .iter()
            .find(|fixture| fixture.source.path.as_str() == "test/corpus/declarations.txt")
            .unwrap();
        let declaration_cases = declaration_fixture.parse_cases().unwrap();

        assert_eq!(
            declaration_cases[1].name,
            "Calls where each argument has multiple values"
        );
        let actual_tree = parse_reduced_or_panic(
            grammar,
            &validated,
            &parser_grammar,
            &parse_table,
            &declaration_cases[1].input,
        );

        assert_same!(actual_tree, declaration_cases[1].expected);
    }

    #[test]
    fn parses_pinned_css_important_declarations_via_glr_conflict() {
        let package = TreeSitterPackageImporter::new(CSS_FIXTURE)
            .import()
            .unwrap();
        let grammar = package.first_grammar();
        let validated = ValidatedGrammar::from_raw(&grammar.grammar.body.grammar).unwrap();
        let lexical = LexicalFacts::from_grammar(&validated);
        let parser_grammar = ParserGrammar::normalize_from_validated(&validated, &lexical)
            .unwrap()
            .prepare_productions_for_items()
            .unwrap();
        let parse_table = ParseTable::from_grammar(&parser_grammar).unwrap();
        let declaration_fixture = grammar
            .corpus
            .iter()
            .find(|fixture| fixture.source.path.as_str() == "test/corpus/declarations.txt")
            .unwrap();
        let declaration_cases = declaration_fixture.parse_cases().unwrap();

        assert_eq!(declaration_cases[7].name, "Important declarations");
        let report = parse_reduced_report_or_panic(
            grammar,
            &validated,
            &parser_grammar,
            &parse_table,
            &declaration_cases[7].input,
        );

        assert_same!(report.tree(), &declaration_cases[7].expected);
        let important_conflict = report.conflict_steps().iter().find(|step| {
            if step.actions.len() < 2
                || !step
                    .actions
                    .iter()
                    .all(|action| matches!(action, crate::parser::ParseAction::Reduce { .. }))
            {
                return false;
            }

            let mut saw_accepted_descendant = false;
            let mut saw_failed_descendant = false;
            for outcome in &step.outcomes {
                let branch = match outcome.result {
                    crate::parser::ReducedConflictActionResult::Branch(branch)
                    | crate::parser::ReducedConflictActionResult::Accepted(branch)
                    | crate::parser::ReducedConflictActionResult::Failed(branch) => branch,
                };
                for result in report.branch_descendant_results(branch) {
                    match result.outcome {
                        crate::parser::ReducedBranchFinalOutcome::Accepted => {
                            saw_accepted_descendant = true;
                        }
                        crate::parser::ReducedBranchFinalOutcome::Failed => {
                            saw_failed_descendant = true;
                        }
                    }
                }
            }

            saw_accepted_descendant && saw_failed_descendant
        });
        assert!(
            important_conflict.is_some(),
            "expected a reduce/reduce fork with accepted and failed descendants, got conflicts {:#?} and branch results {:#?}",
            report.conflict_steps(),
            report.branch_results()
        );
        assert!(
            important_conflict.unwrap().outcomes.len() > 1,
            "expected more than one action outcome for the important declaration conflict"
        );
        assert!(
            report
                .branch_parents()
                .iter()
                .any(|link| link.parent.is_some()),
            "expected branch lineage for runtime fork"
        );
        assert!(
            !report.branch_results().is_empty(),
            "expected terminal branch outcomes for runtime fork"
        );
        assert!(
            report.max_live_branches() > 1,
            "expected more than one live reduced parser branch"
        );
        assert!(
            report.failure_count() > 0,
            "expected at least one forked branch to be retired by later input"
        );
    }

    #[test]
    fn parses_pinned_css_important_declarations_through_runtime_stack_tree() {
        let package = TreeSitterPackageImporter::new(CSS_FIXTURE)
            .import()
            .unwrap();
        let grammar = package.first_grammar();
        let validated = ValidatedGrammar::from_raw(&grammar.grammar.body.grammar).unwrap();
        let lexical = LexicalFacts::from_grammar(&validated);
        let parser_grammar = ParserGrammar::normalize_from_validated(&validated, &lexical)
            .unwrap()
            .prepare_productions_for_items()
            .unwrap();
        let parse_table = ParseTable::from_grammar(&parser_grammar).unwrap();
        let declaration_fixture = grammar
            .corpus
            .iter()
            .find(|fixture| fixture.source.path.as_str() == "test/corpus/declarations.txt")
            .unwrap();
        let declaration_cases = declaration_fixture.parse_cases().unwrap();

        assert_eq!(declaration_cases[7].name, "Important declarations");
        let rust_report = parse_reduced_report_or_panic(
            grammar,
            &validated,
            &parser_grammar,
            &parse_table,
            &declaration_cases[7].input,
        );
        let scanner = CssReducedExternalScanner::new(grammar, &parser_grammar);
        let runtime_report = RuntimeParser::new(&validated, &parser_grammar, &parse_table)
            .unwrap()
            .with_external_scanner(&scanner)
            .parse_with_report(&declaration_cases[7].input)
            .unwrap();

        assert_same!(runtime_report.tree(), rust_report.tree());
        assert_same!(runtime_report.tree(), &declaration_cases[7].expected);
        assert!(
            runtime_report
                .trace_events()
                .iter()
                .any(|event| matches!(event, crate::parser::TraceEvent::GlrSplit { .. })),
            "expected runtime stack execution to emit a GLR split"
        );
        assert!(
            runtime_report
                .trace_events()
                .iter()
                .any(|event| matches!(event, crate::parser::TraceEvent::GlrRetire { .. })),
            "expected runtime stack execution to retire a branch"
        );
        assert!(
            runtime_report
                .tree_events()
                .iter()
                .any(|event| matches!(event, crate::parser::TreeEvent::Reduce { .. })),
            "expected runtime tree execution to emit reduce tree events"
        );
        assert!(
            runtime_report.max_live_versions() > 1,
            "expected more than one live runtime stack version"
        );
        assert!(
            runtime_report.failure_count() > 0,
            "expected at least one runtime forked branch to fail"
        );
    }

    #[test]
    fn executes_css_important_declaration_highlight_on_accepted_glr_lineage() {
        let package = TreeSitterPackageImporter::new(CSS_FIXTURE)
            .import()
            .unwrap();
        let grammar = package.first_grammar();
        let validated = ValidatedGrammar::from_raw(&grammar.grammar.body.grammar).unwrap();
        let lexical = LexicalFacts::from_grammar(&validated);
        let parser_grammar = ParserGrammar::normalize_from_validated(&validated, &lexical)
            .unwrap()
            .prepare_productions_for_items()
            .unwrap();
        let parse_table = ParseTable::from_grammar(&parser_grammar).unwrap();
        let highlights_query = grammar
            .queries
            .well_known(WellKnownQuery::Highlights)
            .unwrap();
        let declaration_fixture = grammar
            .corpus
            .iter()
            .find(|fixture| fixture.source.path.as_str() == "test/corpus/declarations.txt")
            .unwrap();
        let declaration_cases = declaration_fixture.parse_cases().unwrap();

        assert_eq!(declaration_cases[7].name, "Important declarations");
        let scanner = CssReducedExternalScanner::new(grammar, &parser_grammar);
        let runtime_report = RuntimeParser::new(&validated, &parser_grammar, &parse_table)
            .unwrap()
            .with_external_scanner(&scanner)
            .parse_with_report(&declaration_cases[7].input)
            .unwrap();
        let captures = highlights_query.body.execute_runtime_highlights(
            &parser_grammar,
            &runtime_report,
            &declaration_cases[7].input,
        );

        assert_same!(runtime_report.tree(), &declaration_cases[7].expected);
        assert!(
            runtime_report.max_live_versions() > 1,
            "expected highlight oracle input to exercise a runtime GLR fork"
        );
        assert!(
            runtime_report.failure_count() > 0,
            "expected accepted-lineage query input to retire a failed GLR branch"
        );
        assert_important_keyword_capture(&captures);
    }

    #[cfg(feature = "weavy-lowering")]
    #[test]
    fn parses_pinned_css_important_declarations_through_weavy_runtime_stack_tree() {
        let package = TreeSitterPackageImporter::new(CSS_FIXTURE)
            .import()
            .unwrap();
        let grammar = package.first_grammar();
        let validated = ValidatedGrammar::from_raw(&grammar.grammar.body.grammar).unwrap();
        let lexical = LexicalFacts::from_grammar(&validated);
        let parser_grammar = ParserGrammar::normalize_from_validated(&validated, &lexical)
            .unwrap()
            .prepare_productions_for_items()
            .unwrap();
        let parse_table = ParseTable::from_grammar(&parser_grammar).unwrap();
        let declaration_fixture = grammar
            .corpus
            .iter()
            .find(|fixture| fixture.source.path.as_str() == "test/corpus/declarations.txt")
            .unwrap();
        let declaration_cases = declaration_fixture.parse_cases().unwrap();

        assert_eq!(declaration_cases[7].name, "Important declarations");
        let scanner = CssReducedExternalScanner::new(grammar, &parser_grammar);
        let runtime_report = RuntimeParser::new(&validated, &parser_grammar, &parse_table)
            .unwrap()
            .with_external_scanner(&scanner)
            .parse_with_report(&declaration_cases[7].input)
            .unwrap();
        let plan =
            crate::lower::weavy::lower_reduced_parser(&parser_grammar, &parse_table).unwrap();
        let weavy_report = crate::lower::weavy::parse_runtime_with_report_and_scanner(
            &plan,
            &validated,
            &parser_grammar,
            &parse_table,
            &declaration_cases[7].input,
            Some(&scanner),
        )
        .unwrap();

        assert_same!(weavy_report.tree(), runtime_report.tree());
        assert_same!(weavy_report.tree(), &declaration_cases[7].expected);
        assert_eq!(
            weavy_report.accepted_count(),
            runtime_report.accepted_count()
        );
        assert_eq!(weavy_report.failure_count(), runtime_report.failure_count());
        assert_eq!(
            weavy_report.max_live_versions(),
            runtime_report.max_live_versions()
        );
        assert_eq!(weavy_report.trace_events(), runtime_report.trace_events());
        assert_eq!(weavy_report.tree_events(), runtime_report.tree_events());
        assert!(
            weavy_report
                .trace_events()
                .iter()
                .any(|event| matches!(event, crate::parser::TraceEvent::GlrSplit { .. })),
            "expected Weavy-carried runtime execution to emit a GLR split"
        );
        assert!(
            weavy_report
                .trace_events()
                .iter()
                .any(|event| matches!(event, crate::parser::TraceEvent::GlrRetire { .. })),
            "expected Weavy-carried runtime execution to retire a branch"
        );
        assert!(
            weavy_report
                .tree_events()
                .iter()
                .any(|event| matches!(event, crate::parser::TreeEvent::Reduce { .. })),
            "expected Weavy-carried runtime tree execution to emit reduce tree events"
        );
        assert!(weavy_report.stats().step_count > 0);
        assert!(weavy_report.stats().block_call_count > 0);
    }

    #[cfg(feature = "weavy-lowering")]
    #[test]
    fn executes_css_important_declaration_highlight_through_weavy_glr_lineage() {
        let package = TreeSitterPackageImporter::new(CSS_FIXTURE)
            .import()
            .unwrap();
        let grammar = package.first_grammar();
        let validated = ValidatedGrammar::from_raw(&grammar.grammar.body.grammar).unwrap();
        let lexical = LexicalFacts::from_grammar(&validated);
        let parser_grammar = ParserGrammar::normalize_from_validated(&validated, &lexical)
            .unwrap()
            .prepare_productions_for_items()
            .unwrap();
        let parse_table = ParseTable::from_grammar(&parser_grammar).unwrap();
        let highlights_query = grammar
            .queries
            .well_known(WellKnownQuery::Highlights)
            .unwrap();
        let declaration_fixture = grammar
            .corpus
            .iter()
            .find(|fixture| fixture.source.path.as_str() == "test/corpus/declarations.txt")
            .unwrap();
        let declaration_cases = declaration_fixture.parse_cases().unwrap();

        assert_eq!(declaration_cases[7].name, "Important declarations");
        let scanner = CssReducedExternalScanner::new(grammar, &parser_grammar);
        let runtime_report = RuntimeParser::new(&validated, &parser_grammar, &parse_table)
            .unwrap()
            .with_external_scanner(&scanner)
            .parse_with_report(&declaration_cases[7].input)
            .unwrap();
        let plan =
            crate::lower::weavy::lower_reduced_parser(&parser_grammar, &parse_table).unwrap();
        let weavy_report = crate::lower::weavy::parse_runtime_with_report_and_scanner(
            &plan,
            &validated,
            &parser_grammar,
            &parse_table,
            &declaration_cases[7].input,
            Some(&scanner),
        )
        .unwrap();
        let runtime_captures = highlights_query.body.execute_runtime_highlights(
            &parser_grammar,
            &runtime_report,
            &declaration_cases[7].input,
        );
        let weavy_captures = highlights_query
            .body
            .execute_runtime_highlights_from_tree_events(
                &parser_grammar,
                &weavy_report.accepted_tree_events(),
                &declaration_cases[7].input,
            );

        assert_same!(weavy_report.tree(), runtime_report.tree());
        assert_same!(weavy_report.tree(), &declaration_cases[7].expected);
        assert_eq!(weavy_report.trace_events(), runtime_report.trace_events());
        assert_eq!(weavy_report.tree_events(), runtime_report.tree_events());
        assert_eq!(
            weavy_report.accepted_tree_events(),
            runtime_report.accepted_tree_events()
        );
        assert_eq!(weavy_captures, runtime_captures);
        assert_important_keyword_capture(&weavy_captures);
        assert!(
            weavy_report.max_live_versions() > 1,
            "expected Weavy highlight oracle input to exercise a runtime GLR fork"
        );
        assert!(
            weavy_report.failure_count() > 0,
            "expected Weavy accepted-lineage query input to retire a failed GLR branch"
        );
        assert!(weavy_report.stats().step_count > 0);
        assert!(weavy_report.stats().block_call_count > 0);
    }

    #[cfg(feature = "weavy-lowering")]
    #[test]
    fn parses_pinned_css_important_declarations_through_weavy_glr_conflict() {
        let package = TreeSitterPackageImporter::new(CSS_FIXTURE)
            .import()
            .unwrap();
        let grammar = package.first_grammar();
        let validated = ValidatedGrammar::from_raw(&grammar.grammar.body.grammar).unwrap();
        let lexical = LexicalFacts::from_grammar(&validated);
        let parser_grammar = ParserGrammar::normalize_from_validated(&validated, &lexical)
            .unwrap()
            .prepare_productions_for_items()
            .unwrap();
        let parse_table = ParseTable::from_grammar(&parser_grammar).unwrap();
        let declaration_fixture = grammar
            .corpus
            .iter()
            .find(|fixture| fixture.source.path.as_str() == "test/corpus/declarations.txt")
            .unwrap();
        let declaration_cases = declaration_fixture.parse_cases().unwrap();

        assert_eq!(declaration_cases[7].name, "Important declarations");
        let rust_report = parse_reduced_report_or_panic(
            grammar,
            &validated,
            &parser_grammar,
            &parse_table,
            &declaration_cases[7].input,
        );
        let plan =
            crate::lower::weavy::lower_reduced_parser(&parser_grammar, &parse_table).unwrap();
        let weavy_report = crate::lower::weavy::parse_reduced_with_report(
            &plan,
            &validated,
            &parser_grammar,
            &parse_table,
            &declaration_cases[7].input,
        )
        .unwrap();

        assert_same!(weavy_report.tree(), rust_report.tree());
        assert_same!(weavy_report.tree(), &declaration_cases[7].expected);
        let important_conflict = weavy_report.conflict_steps().iter().find(|step| {
            if step.actions.len() < 2
                || !step
                    .actions
                    .iter()
                    .all(|action| matches!(action, crate::parser::ParseAction::Reduce { .. }))
            {
                return false;
            }

            let mut saw_accepted_descendant = false;
            let mut saw_failed_descendant = false;
            for outcome in &step.outcomes {
                let branch = match outcome.result {
                    crate::parser::ReducedConflictActionResult::Branch(branch)
                    | crate::parser::ReducedConflictActionResult::Accepted(branch)
                    | crate::parser::ReducedConflictActionResult::Failed(branch) => branch,
                };
                for result in weavy_report.branch_descendant_results(branch) {
                    match result.outcome {
                        crate::parser::ReducedBranchFinalOutcome::Accepted => {
                            saw_accepted_descendant = true;
                        }
                        crate::parser::ReducedBranchFinalOutcome::Failed => {
                            saw_failed_descendant = true;
                        }
                    }
                }
            }

            saw_accepted_descendant && saw_failed_descendant
        });
        assert!(
            important_conflict.is_some(),
            "expected a Weavy reduce/reduce fork with accepted and failed descendants, got conflicts {:#?} and branch results {:#?}",
            weavy_report.conflict_steps(),
            weavy_report.branch_results()
        );
        assert!(
            important_conflict.unwrap().outcomes.len() > 1,
            "expected more than one Weavy action outcome for the important declaration conflict"
        );
        assert!(
            weavy_report
                .branch_parents()
                .iter()
                .any(|link| link.parent.is_some()),
            "expected Weavy branch lineage for runtime fork"
        );
        assert!(
            !weavy_report.branch_results().is_empty(),
            "expected terminal Weavy branch outcomes for runtime fork"
        );
        assert!(
            weavy_report.max_live_branches() > 1,
            "expected more than one live reduced Weavy branch"
        );
        assert!(
            weavy_report.failure_count() > 0,
            "expected at least one Weavy forked branch to be retired by later input"
        );
        assert!(weavy_report.stats().step_count > 0);
        assert!(weavy_report.stats().block_call_count > 0);
    }

    #[test]
    fn parses_pinned_json_arrays_corpus_case_without_external_scanner() {
        let package = TreeSitterPackageImporter::new(JSON_FIXTURE)
            .import()
            .unwrap();
        let grammar = package.first_grammar();
        assert_eq!(grammar.language_name(), "json");
        let validated = ValidatedGrammar::from_raw(&grammar.grammar.body.grammar).unwrap();
        let lexical = LexicalFacts::from_grammar(&validated);
        assert_eq!(validated.external_count(), 0);
        let parser_grammar = ParserGrammar::normalize_from_validated(&validated, &lexical)
            .unwrap()
            .prepare_productions_for_items()
            .unwrap();
        let parse_table = ParseTable::from_grammar(&parser_grammar).unwrap();
        let main_fixture = grammar
            .corpus
            .iter()
            .find(|fixture| fixture.source.path.as_str() == "test/corpus/main.txt")
            .unwrap();
        let cases = main_fixture.parse_cases().unwrap();

        assert_eq!(cases[0].name, "Arrays");
        let actual_tree = parse_reduced_without_external_scanner_or_panic(
            &validated,
            &parser_grammar,
            &parse_table,
            &cases[0].input,
        );

        assert_same!(actual_tree, cases[0].expected);
    }

    #[test]
    fn parses_pinned_json_string_content_corpus_case_without_external_scanner() {
        let package = TreeSitterPackageImporter::new(JSON_FIXTURE)
            .import()
            .unwrap();
        let grammar = package.first_grammar();
        assert_eq!(grammar.language_name(), "json");
        let validated = ValidatedGrammar::from_raw(&grammar.grammar.body.grammar).unwrap();
        let lexical = LexicalFacts::from_grammar(&validated);
        assert_eq!(validated.external_count(), 0);
        let parser_grammar = ParserGrammar::normalize_from_validated(&validated, &lexical)
            .unwrap()
            .prepare_productions_for_items()
            .unwrap();
        let parse_table = ParseTable::from_grammar(&parser_grammar).unwrap();
        let main_fixture = grammar
            .corpus
            .iter()
            .find(|fixture| fixture.source.path.as_str() == "test/corpus/main.txt")
            .unwrap();
        let cases = main_fixture.parse_cases().unwrap();

        assert_eq!(cases[1].name, "String content");
        let actual_tree = parse_reduced_without_external_scanner_or_panic(
            &validated,
            &parser_grammar,
            &parse_table,
            &cases[1].input,
        );

        assert_same!(actual_tree, cases[1].expected);
    }

    #[test]
    fn parses_pinned_json_comments_corpus_case_with_visible_extras() {
        let package = TreeSitterPackageImporter::new(JSON_FIXTURE)
            .import()
            .unwrap();
        let grammar = package.first_grammar();
        assert_eq!(grammar.language_name(), "json");
        let validated = ValidatedGrammar::from_raw(&grammar.grammar.body.grammar).unwrap();
        let lexical = LexicalFacts::from_grammar(&validated);
        assert_eq!(validated.external_count(), 0);
        let parser_grammar = ParserGrammar::normalize_from_validated(&validated, &lexical)
            .unwrap()
            .prepare_productions_for_items()
            .unwrap();
        let parse_table = ParseTable::from_grammar(&parser_grammar).unwrap();
        let main_fixture = grammar
            .corpus
            .iter()
            .find(|fixture| fixture.source.path.as_str() == "test/corpus/main.txt")
            .unwrap();
        let cases = main_fixture.parse_cases().unwrap();

        assert_eq!(cases[4].name, "Comments");
        let actual_tree = parse_reduced_without_external_scanner_or_panic(
            &validated,
            &parser_grammar,
            &parse_table,
            &cases[4].input,
        );

        assert_same!(actual_tree, cases[4].expected);
    }

    #[test]
    fn executes_json_key_highlights_with_field_constraint() {
        let package = TreeSitterPackageImporter::new(JSON_FIXTURE)
            .import()
            .unwrap();
        let grammar = package.first_grammar();
        assert_eq!(grammar.language_name(), "json");
        let validated = ValidatedGrammar::from_raw(&grammar.grammar.body.grammar).unwrap();
        let lexical = LexicalFacts::from_grammar(&validated);
        assert_eq!(validated.external_count(), 0);
        let parser_grammar = ParserGrammar::normalize_from_validated(&validated, &lexical)
            .unwrap()
            .prepare_productions_for_items()
            .unwrap();
        let parse_table = ParseTable::from_grammar(&parser_grammar).unwrap();
        let highlights_query = grammar
            .queries
            .well_known(WellKnownQuery::Highlights)
            .unwrap();
        let main_fixture = grammar
            .corpus
            .iter()
            .find(|fixture| fixture.source.path.as_str() == "test/corpus/main.txt")
            .unwrap();
        let cases = main_fixture.parse_cases().unwrap();

        assert_eq!(cases[0].name, "Arrays");
        let runtime_report = RuntimeParser::new(&validated, &parser_grammar, &parse_table)
            .unwrap()
            .parse_with_report(&cases[0].input)
            .unwrap();
        let captures = highlights_query.body.execute_runtime_highlights(
            &parser_grammar,
            &runtime_report,
            &cases[0].input,
        );
        let field_regression_query = QuerySource(
            r#"
(pair
  key: (_) @json.key
  value: (_) @json.value)
"#
            .to_owned(),
        );
        let field_captures = field_regression_query.execute_runtime_highlights(
            &parser_grammar,
            &runtime_report,
            &cases[0].input,
        );

        assert_same!(runtime_report.tree(), &cases[0].expected);
        assert_eq!(
            capture_texts(&captures, "string.special.key"),
            ["\"stuff\""]
        );
        assert_eq!(
            capture_texts(&captures, "string"),
            ["\"stuff\"", "\"good\""],
            "the broad string query should still see both key and value strings"
        );
        assert_eq!(capture_texts(&field_captures, "json.key"), ["\"stuff\""]);
        assert_eq!(capture_texts(&field_captures, "json.value"), ["\"good\""]);
    }

    #[cfg(feature = "weavy-lowering")]
    #[test]
    fn executes_json_key_highlights_through_weavy_runtime_events() {
        let package = TreeSitterPackageImporter::new(JSON_FIXTURE)
            .import()
            .unwrap();
        let grammar = package.first_grammar();
        assert_eq!(grammar.language_name(), "json");
        let validated = ValidatedGrammar::from_raw(&grammar.grammar.body.grammar).unwrap();
        let lexical = LexicalFacts::from_grammar(&validated);
        assert_eq!(validated.external_count(), 0);
        let parser_grammar = ParserGrammar::normalize_from_validated(&validated, &lexical)
            .unwrap()
            .prepare_productions_for_items()
            .unwrap();
        let parse_table = ParseTable::from_grammar(&parser_grammar).unwrap();
        let highlights_query = grammar
            .queries
            .well_known(WellKnownQuery::Highlights)
            .unwrap();
        let main_fixture = grammar
            .corpus
            .iter()
            .find(|fixture| fixture.source.path.as_str() == "test/corpus/main.txt")
            .unwrap();
        let cases = main_fixture.parse_cases().unwrap();

        assert_eq!(cases[0].name, "Arrays");
        let runtime_report = RuntimeParser::new(&validated, &parser_grammar, &parse_table)
            .unwrap()
            .parse_with_report(&cases[0].input)
            .unwrap();
        let plan =
            crate::lower::weavy::lower_reduced_parser(&parser_grammar, &parse_table).unwrap();
        let weavy_report = crate::lower::weavy::parse_runtime_with_report(
            &plan,
            &validated,
            &parser_grammar,
            &parse_table,
            &cases[0].input,
        )
        .unwrap();
        let runtime_captures = highlights_query.body.execute_runtime_highlights(
            &parser_grammar,
            &runtime_report,
            &cases[0].input,
        );
        let field_regression_query = QuerySource(
            r#"
(pair
  key: (_) @json.key
  value: (_) @json.value)
"#
            .to_owned(),
        );
        let runtime_field_captures = field_regression_query.execute_runtime_highlights(
            &parser_grammar,
            &runtime_report,
            &cases[0].input,
        );
        let weavy_captures = highlights_query
            .body
            .execute_runtime_highlights_from_tree_events(
                &parser_grammar,
                &weavy_report.accepted_tree_events(),
                &cases[0].input,
            );
        let weavy_field_captures = field_regression_query
            .execute_runtime_highlights_from_tree_events(
                &parser_grammar,
                &weavy_report.accepted_tree_events(),
                &cases[0].input,
            );

        assert_same!(weavy_report.tree(), runtime_report.tree());
        assert_same!(weavy_report.tree(), &cases[0].expected);
        assert_eq!(weavy_report.trace_events(), runtime_report.trace_events());
        assert_eq!(weavy_report.tree_events(), runtime_report.tree_events());
        assert_eq!(
            weavy_report.accepted_tree_events(),
            runtime_report.accepted_tree_events()
        );
        assert_eq!(weavy_captures, runtime_captures);
        assert_eq!(weavy_field_captures, runtime_field_captures);
        assert_eq!(
            capture_texts(&weavy_captures, "string.special.key"),
            ["\"stuff\""]
        );
        assert_eq!(
            capture_texts(&weavy_field_captures, "json.value"),
            ["\"good\""]
        );
        assert!(weavy_report.stats().step_count > 0);
        assert!(weavy_report.stats().block_call_count > 0);
    }

    #[test]
    fn parses_json_leading_visible_extra_at_document_root() {
        let package = TreeSitterPackageImporter::new(JSON_FIXTURE)
            .import()
            .unwrap();
        let grammar = package.first_grammar();
        assert_eq!(grammar.language_name(), "json");
        let validated = ValidatedGrammar::from_raw(&grammar.grammar.body.grammar).unwrap();
        let lexical = LexicalFacts::from_grammar(&validated);
        assert_eq!(validated.external_count(), 0);
        let parser_grammar = ParserGrammar::normalize_from_validated(&validated, &lexical)
            .unwrap()
            .prepare_productions_for_items()
            .unwrap();
        let parse_table = ParseTable::from_grammar(&parser_grammar).unwrap();

        let actual_tree = parse_reduced_without_external_scanner_or_panic(
            &validated,
            &parser_grammar,
            &parse_table,
            "// leading\n{\"a\": 1}",
        );
        let expected = sexp_node(
            "document",
            vec![
                sexp_node("comment", Vec::new()),
                sexp_node(
                    "object",
                    vec![sexp_node(
                        "pair",
                        vec![
                            sexp_node("string", vec![sexp_node("string_content", Vec::new())]),
                            sexp_node("number", Vec::new()),
                        ],
                    )],
                ),
            ],
        );

        assert_same!(actual_tree, expected);
    }

    #[cfg(feature = "weavy-lowering")]
    #[test]
    fn parses_pinned_json_comments_through_weavy_lowering() {
        let package = TreeSitterPackageImporter::new(JSON_FIXTURE)
            .import()
            .unwrap();
        let grammar = package.first_grammar();
        assert_eq!(grammar.language_name(), "json");
        let validated = ValidatedGrammar::from_raw(&grammar.grammar.body.grammar).unwrap();
        let lexical = LexicalFacts::from_grammar(&validated);
        assert_eq!(validated.external_count(), 0);
        let parser_grammar = ParserGrammar::normalize_from_validated(&validated, &lexical)
            .unwrap()
            .prepare_productions_for_items()
            .unwrap();
        let parse_table = ParseTable::from_grammar(&parser_grammar).unwrap();
        let main_fixture = grammar
            .corpus
            .iter()
            .find(|fixture| fixture.source.path.as_str() == "test/corpus/main.txt")
            .unwrap();
        let cases = main_fixture.parse_cases().unwrap();

        assert_eq!(cases[4].name, "Comments");
        let rust_report = unwrap_reduced_report_or_panic(
            ReducedParser::new(&validated, &parser_grammar, &parse_table)
                .unwrap()
                .parse_with_report(&cases[4].input),
            &parser_grammar,
            &parse_table,
        );
        let plan =
            crate::lower::weavy::lower_reduced_parser(&parser_grammar, &parse_table).unwrap();
        let (weavy_tree, stats) = crate::lower::weavy::parse_reduced_with_plan(
            &plan,
            &validated,
            &parser_grammar,
            &parse_table,
            &cases[4].input,
        )
        .unwrap();

        assert_same!(&weavy_tree, rust_report.tree());
        assert_same!(weavy_tree, cases[4].expected);
        assert!(stats.step_count > 0);
        assert!(stats.block_call_count > 0);
    }

    #[cfg(feature = "weavy-lowering")]
    #[test]
    fn parses_json_leading_visible_extra_through_weavy_lowering() {
        let package = TreeSitterPackageImporter::new(JSON_FIXTURE)
            .import()
            .unwrap();
        let grammar = package.first_grammar();
        assert_eq!(grammar.language_name(), "json");
        let validated = ValidatedGrammar::from_raw(&grammar.grammar.body.grammar).unwrap();
        let lexical = LexicalFacts::from_grammar(&validated);
        assert_eq!(validated.external_count(), 0);
        let parser_grammar = ParserGrammar::normalize_from_validated(&validated, &lexical)
            .unwrap()
            .prepare_productions_for_items()
            .unwrap();
        let parse_table = ParseTable::from_grammar(&parser_grammar).unwrap();
        let input = "// leading\n{\"a\": 1}";
        let rust_report = unwrap_reduced_report_or_panic(
            ReducedParser::new(&validated, &parser_grammar, &parse_table)
                .unwrap()
                .parse_with_report(input),
            &parser_grammar,
            &parse_table,
        );
        let plan =
            crate::lower::weavy::lower_reduced_parser(&parser_grammar, &parse_table).unwrap();
        let (weavy_tree, stats) = crate::lower::weavy::parse_reduced_with_plan(
            &plan,
            &validated,
            &parser_grammar,
            &parse_table,
            input,
        )
        .unwrap();

        assert_same!(&weavy_tree, rust_report.tree());
        assert_eq!(
            weavy_tree.to_sexp(),
            "(document (comment) (object (pair (string (string_content)) (number))))"
        );
        assert!(stats.step_count > 0);
        assert!(stats.block_call_count > 0);
    }

    fn collect_node_kinds(node: &SexpNode, out: &mut BTreeSet<String>) {
        out.insert(node.kind.clone());
        for child in &node.children {
            if let SexpValue::Node(child) = &child.value {
                collect_node_kinds(child, out);
            }
        }
    }

    fn sexp_node(kind: &str, children: Vec<SexpNode>) -> SexpNode {
        SexpNode {
            kind: kind.to_owned(),
            children: children
                .into_iter()
                .map(|node| SexpChild {
                    field: None,
                    value: SexpValue::Node(node),
                })
                .collect(),
        }
    }

    fn highlight_assertion_range(assertion: &HighlightAssertion) -> PointRange {
        let start = PointBytes::new(
            Row::new(u32::try_from(assertion.position.row).unwrap()),
            Utf8ColumnBytes::new(u32::try_from(assertion.position.column).unwrap()),
        );
        let end = PointBytes::new(
            Row::new(u32::try_from(assertion.position.row).unwrap()),
            Utf8ColumnBytes::new(
                u32::try_from(assertion.position.column + assertion.length).unwrap(),
            ),
        );
        PointRange::new(start, end).unwrap()
    }

    fn capture_matches_highlight_assertion(
        points: PointRange,
        assertion: &HighlightAssertion,
    ) -> bool {
        let assertion = highlight_assertion_range(assertion);
        points.start() <= assertion.start() && assertion.end() <= points.end()
    }

    fn assert_css_highlight_assertions_covered(
        assertions: &[HighlightAssertion],
        captures: &[HighlightCapture],
    ) {
        assert_eq!(assertions.len(), 37);
        for assertion in assertions {
            let matched = captures.iter().any(|capture| {
                capture.capture_name() == assertion.expected_capture_name
                    && capture_matches_highlight_assertion(capture.points(), assertion)
            });
            if assertion.negative {
                assert!(
                    !matched,
                    "unexpected negative highlight capture `{}` at {:?}",
                    assertion.expected_capture_name,
                    highlight_assertion_range(assertion)
                );
                continue;
            }
            if !matched {
                panic!(
                    "missing highlight capture `{}` at {:?}, produced captures at that range: {:?}, same-name captures: {:?}",
                    assertion.expected_capture_name,
                    highlight_assertion_range(assertion),
                    captures
                        .iter()
                        .filter(|capture| {
                            capture_matches_highlight_assertion(capture.points(), assertion)
                        })
                        .map(|capture| (capture.capture_name(), capture.text()))
                        .collect::<Vec<_>>(),
                    captures
                        .iter()
                        .filter(|capture| capture.capture_name() == assertion.expected_capture_name)
                        .collect::<Vec<_>>()
                );
            }
        }
    }

    fn assert_important_keyword_capture(captures: &[HighlightCapture]) {
        let important = captures
            .iter()
            .filter(|capture| capture.capture_name() == "keyword" && capture.text() == "!important")
            .collect::<Vec<_>>();
        assert_eq!(
            important.len(),
            1,
            "expected exactly one keyword capture for !important, got keyword captures {:?}",
            captures
                .iter()
                .filter(|capture| capture.capture_name() == "keyword")
                .collect::<Vec<_>>()
        );
    }

    fn capture_texts<'a>(captures: &'a [HighlightCapture], capture_name: &str) -> Vec<&'a str> {
        captures
            .iter()
            .filter(|capture| capture.capture_name() == capture_name)
            .map(|capture| capture.text())
            .collect()
    }

    fn parse_reduced_or_panic(
        grammar: &ImportedGrammar,
        validated: &ValidatedGrammar,
        parser_grammar: &ParserGrammar,
        parse_table: &ParseTable,
        input: &str,
    ) -> SexpNode {
        parse_reduced_report_or_panic(grammar, validated, parser_grammar, parse_table, input)
            .tree()
            .clone()
    }

    fn parse_reduced_without_external_scanner_or_panic(
        validated: &ValidatedGrammar,
        parser_grammar: &ParserGrammar,
        parse_table: &ParseTable,
        input: &str,
    ) -> SexpNode {
        unwrap_reduced_report_or_panic(
            ReducedParser::new(validated, parser_grammar, parse_table)
                .unwrap()
                .parse_with_report(input),
            parser_grammar,
            parse_table,
        )
        .tree()
        .clone()
    }

    fn parse_reduced_report_or_panic(
        grammar: &ImportedGrammar,
        validated: &ValidatedGrammar,
        parser_grammar: &ParserGrammar,
        parse_table: &ParseTable,
        input: &str,
    ) -> ReducedParseReport {
        let scanner = CssReducedExternalScanner::new(grammar, parser_grammar);
        unwrap_reduced_report_or_panic(
            ReducedParser::new(validated, parser_grammar, parse_table)
                .unwrap()
                .with_external_scanner(&scanner)
                .parse_with_report(input),
            parser_grammar,
            parse_table,
        )
    }

    fn unwrap_reduced_report_or_panic(
        result: Result<ReducedParseReport, crate::parser::ReducedParseError>,
        parser_grammar: &ParserGrammar,
        parse_table: &ParseTable,
    ) -> ReducedParseReport {
        result.unwrap_or_else(|error| {
            let state = match error.kind() {
                crate::parser::ReducedParseErrorKind::NoToken { state, .. }
                | crate::parser::ReducedParseErrorKind::NoAction { state, .. }
                | crate::parser::ReducedParseErrorKind::AmbiguousAction { state, .. }
                | crate::parser::ReducedParseErrorKind::UnsupportedExternalScanner {
                    state, ..
                }
                | crate::parser::ReducedParseErrorKind::UnsupportedRecovery { state }
                | crate::parser::ReducedParseErrorKind::MissingState { state } => Some(*state),
                crate::parser::ReducedParseErrorKind::MissingGoto { state, .. } => Some(*state),
                _ => None,
            };
            let mut states = Vec::new();
            if let Some(state) = state {
                states.push(state);
            }
            if let Some(last) = error.trace().last() {
                if !states.contains(&last.state) {
                    states.push(last.state);
                }
            }
            for trace in error.trace().iter().rev().take(12) {
                if !states.contains(&trace.state) {
                    states.push(trace.state);
                }
            }
            let state_dump = states
                .into_iter()
                .map(|state| describe_parse_state(parser_grammar, parse_table, state))
                .collect::<Vec<_>>()
                .join("\n\n");
            let trace_tail = error
                .trace()
                .iter()
                .rev()
                .take(12)
                .copied()
                .collect::<Vec<_>>();
            panic!(
                "kind={:#?}\ntrace_tail={trace_tail:#?}\n{state_dump}",
                error.kind()
            );
        })
    }

    struct CssReducedExternalScanner {
        scanner: RefCell<snark_scanner_host::CssScanner>,
        external_ordinals: Vec<(crate::parser::ExternalId, usize)>,
        snapshots: RefCell<Vec<Vec<u8>>>,
    }

    impl CssReducedExternalScanner {
        fn new(grammar: &ImportedGrammar, parser_grammar: &ParserGrammar) -> Self {
            let scanner = grammar
                .scanners
                .iter()
                .find(|scanner| scanner.kind == TreeSitterScannerKind::C)
                .expect("reduced CSS fixture should import src/scanner.c");
            assert_eq!(
                scanner.source.body.0,
                snark_scanner_host::CSS_SCANNER_SOURCE,
                "compiled scanner fixture must match imported scanner.c"
            );
            let imported_externals = scanner
                .externals
                .iter()
                .map(|external| {
                    (
                        external.ordinal().get(),
                        external.name().map(|name| name.as_str()),
                    )
                })
                .collect::<Vec<_>>();
            assert_eq!(
                imported_externals,
                [
                    (0, Some("_descendant_operator")),
                    (1, Some("_pseudo_class_selector_colon")),
                    (2, Some("__error_recovery")),
                ],
                "compiled CSS scanner ordinal order must match imported externals"
            );
            let external_ordinals = parser_grammar
                .symbols()
                .externals()
                .iter()
                .map(|external| (external.id(), external.ordinal() as usize))
                .collect::<Vec<_>>();
            Self {
                scanner: RefCell::new(snark_scanner_host::CssScanner::new()),
                external_ordinals,
                snapshots: RefCell::new(vec![Vec::new()]),
            }
        }

        fn ordinal_for(&self, external: crate::parser::ExternalId) -> Option<usize> {
            self.external_ordinals
                .iter()
                .find_map(|(candidate, ordinal)| (*candidate == external).then_some(*ordinal))
        }

        fn valid_symbol_mask(&self, request: ReducedExternalScan<'_>) -> Option<Vec<bool>> {
            let width = self
                .external_ordinals
                .iter()
                .map(|(_, ordinal)| *ordinal)
                .max()
                .map_or(0, |ordinal| ordinal + 1);
            let mut mask = vec![false; width];
            if let Some(valid_symbols) = request.valid_symbols() {
                for external in valid_symbols.externals() {
                    let ordinal = self.ordinal_for(*external)?;
                    mask[ordinal] = true;
                }
            } else {
                let ordinal = self.ordinal_for(request.external())?;
                mask[ordinal] = true;
            }
            Some(mask)
        }

        fn snapshot_bytes(&self, snapshot: Option<ScannerSnapshotId>) -> Vec<u8> {
            let snapshot = snapshot.unwrap_or_else(|| ScannerSnapshotId::from_index(0));
            self.snapshots
                .borrow()
                .get(snapshot.get() as usize)
                .unwrap_or_else(|| panic!("scanner snapshot {} should be interned", snapshot.get()))
                .clone()
        }

        fn intern_snapshot(&self, bytes: &[u8]) -> ScannerSnapshotId {
            let mut snapshots = self.snapshots.borrow_mut();
            if let Some(index) = snapshots.iter().position(|snapshot| snapshot == bytes) {
                return ScannerSnapshotId::from_index(index);
            }
            let index = snapshots.len();
            snapshots.push(bytes.to_vec());
            ScannerSnapshotId::from_index(index)
        }
    }

    impl ReducedExternalScanner for CssReducedExternalScanner {
        fn scan(
            &self,
            request: ReducedExternalScan<'_>,
        ) -> Result<Option<ReducedExternalScanResult>, crate::parser::ReducedParseError> {
            let Some(mask) = self.valid_symbol_mask(request) else {
                return Ok(None);
            };
            let Some(request_ordinal) = self.ordinal_for(request.external()) else {
                return Ok(None);
            };
            if request_ordinal >= mask.len() || !mask[request_ordinal] {
                return Ok(None);
            }
            let before = request
                .scanner_snapshot()
                .unwrap_or_else(|| ScannerSnapshotId::from_index(0));
            let snapshot = self.snapshot_bytes(Some(before));
            let scan = self
                .scanner
                .borrow_mut()
                .scan(request.input(), request.byte_position(), &mask, &snapshot)
                .expect("CSS valid-symbol mask width should match imported external ordinals");
            if !scan.accepted() || scan.result_symbol() != Some(request_ordinal) {
                return Ok(None);
            }
            let after = self.intern_snapshot(scan.serialized_state());
            Ok(Some(
                ReducedExternalScanResult::new(scan.end_byte())
                    .with_snapshots(Some(before), Some(after)),
            ))
        }
    }

    struct RecordingCssReducedExternalScanner<'a> {
        inner: &'a dyn ReducedExternalScanner,
        calls: Cell<usize>,
        accepted: Cell<usize>,
        accepted_pseudo_class_selector_colon: Cell<usize>,
        accepted_descendant_operator: Cell<usize>,
        requests_with_snapshot: Cell<usize>,
        missing_valid_symbols: Cell<usize>,
        invalid_symbol_requests: Cell<usize>,
    }

    impl<'a> RecordingCssReducedExternalScanner<'a> {
        fn new(inner: &'a dyn ReducedExternalScanner) -> Self {
            Self {
                inner,
                calls: Cell::new(0),
                accepted: Cell::new(0),
                accepted_pseudo_class_selector_colon: Cell::new(0),
                accepted_descendant_operator: Cell::new(0),
                requests_with_snapshot: Cell::new(0),
                missing_valid_symbols: Cell::new(0),
                invalid_symbol_requests: Cell::new(0),
            }
        }
    }

    impl ReducedExternalScanner for RecordingCssReducedExternalScanner<'_> {
        fn scan(
            &self,
            request: ReducedExternalScan<'_>,
        ) -> Result<Option<ReducedExternalScanResult>, crate::parser::ReducedParseError> {
            self.calls.set(self.calls.get() + 1);
            if request.scanner_snapshot().is_some() {
                self.requests_with_snapshot
                    .set(self.requests_with_snapshot.get() + 1);
            }
            match request.valid_symbols() {
                Some(valid_symbols) if valid_symbols.externals().contains(&request.external()) => {}
                Some(_) => {
                    self.invalid_symbol_requests
                        .set(self.invalid_symbol_requests.get() + 1);
                    return Ok(None);
                }
                None => {
                    self.missing_valid_symbols
                        .set(self.missing_valid_symbols.get() + 1);
                }
            }
            let result = self.inner.scan(request)?;
            if result.is_some() {
                self.accepted.set(self.accepted.get() + 1);
                match request.external_symbol().name() {
                    Some("_pseudo_class_selector_colon") => self
                        .accepted_pseudo_class_selector_colon
                        .set(self.accepted_pseudo_class_selector_colon.get() + 1),
                    Some("_descendant_operator") => self
                        .accepted_descendant_operator
                        .set(self.accepted_descendant_operator.get() + 1),
                    _ => {}
                }
            }
            Ok(result)
        }
    }

    fn describe_parse_state(
        parser_grammar: &ParserGrammar,
        parse_table: &ParseTable,
        state: ParseStateId,
    ) -> String {
        let Some(state_row) = parse_table.states().get(state.get() as usize) else {
            return format!("state={} <missing>", state.get());
        };
        let entries = state_row
            .entries()
            .iter()
            .map(|entry| {
                format!(
                    "  {} => {:?}",
                    describe_lookahead(parser_grammar, entry.lookahead()),
                    entry.actions()
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let item_set = &parse_table.item_sets()[state_row.item_set().get() as usize];
        let items = item_set
            .items()
            .iter()
            .map(|item| {
                let production = &parser_grammar.productions()[item.production().get() as usize];
                format!(
                    "  production={} dot={}\n{production:#?}",
                    item.production().get(),
                    item.dot()
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            "state={} item_set={}\nentries:\n{entries}\nitems:\n{items}",
            state_row.id().get(),
            state_row.item_set().get()
        )
    }

    fn describe_lookahead(parser_grammar: &ParserGrammar, lookahead: LookaheadSymbol) -> String {
        match lookahead {
            LookaheadSymbol::Terminal(terminal) => {
                let terminal_row = &parser_grammar.symbols().terminals()[terminal.get() as usize];
                format!(
                    "terminal#{} {:?} {:?} root={:?}",
                    terminal.get(),
                    terminal_row.kind(),
                    terminal_row.spelling(),
                    terminal_row.lexical_root()
                )
            }
            LookaheadSymbol::External(external) => {
                let external_row = &parser_grammar.symbols().externals()[external.get() as usize];
                format!(
                    "external#{} {:?}",
                    external.get(),
                    external_row.name().unwrap_or("<anonymous-external>")
                )
            }
            LookaheadSymbol::Eof => "eof".to_owned(),
            LookaheadSymbol::ReservedWord { terminal, context } => {
                let terminal_row = &parser_grammar.symbols().terminals()[terminal.get() as usize];
                format!(
                    "reserved terminal#{} {:?} {:?} context#{}",
                    terminal.get(),
                    terminal_row.kind(),
                    terminal_row.spelling(),
                    context.get()
                )
            }
            LookaheadSymbol::ErrorRecovery(internal) => {
                format!("error-recovery#{}", internal.get())
            }
        }
    }

    fn css_highlight_assertions() -> Vec<HighlightAssertion> {
        [
            (0, 0, "punctuation.delimiter"),
            (0, 1, "attribute"),
            (3, 2, "property"),
            (3, 12, "punctuation.delimiter"),
            (3, 13, "string.special"),
            (7, 15, "function"),
            (7, 20, "punctuation.delimiter"),
            (17, 0, "tag"),
            (19, 15, "function"),
            (19, 18, "punctuation.bracket"),
            (19, 19, "variable"),
            (33, 0, "punctuation.delimiter"),
            (33, 3, "property"),
            (33, 6, "punctuation.bracket"),
            (37, 2, "property"),
            (37, 13, "punctuation.delimiter"),
            (37, 25, "punctuation.delimiter"),
            (41, 6, "punctuation.delimiter"),
            (41, 20, "number"),
            (41, 21, "type"),
            (41, 25, "operator"),
            (41, 28, "number"),
            (41, 33, "string"),
            (41, 48, "punctuation.delimiter"),
            (49, 2, "property"),
            (49, 9, "punctuation.delimiter"),
            (49, 11, "number"),
            (49, 12, "type"),
            (54, 0, "punctuation.bracket"),
            (57, 0, "keyword"),
            (57, 7, "punctuation.bracket"),
            (57, 11, "property"),
            (57, 20, "number"),
            (57, 23, "type"),
            (57, 25, "punctuation.bracket"),
            (64, 2, "punctuation.delimiter"),
            (64, 3, "property"),
        ]
        .into_iter()
        .map(|(row, column, expected_capture_name)| HighlightAssertion {
            position: HighlightPoint { row, column },
            length: 1,
            negative: false,
            expected_capture_name: expected_capture_name.to_owned(),
        })
        .collect()
    }

    #[test]
    fn imports_synthetic_package_with_literal_external() {
        let root = test_package_root("snark-mini-css");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("queries")).unwrap();
        fs::create_dir_all(root.join("test").join("corpus")).unwrap();
        fs::create_dir_all(root.join("test").join("highlight")).unwrap();
        fs::write(
            root.join("tree-sitter.json"),
            r#"{
              "grammars": [
                {
                  "name": "mini_css",
                  "scope": "source.mini-css",
                  "highlights": "queries/highlights.scm"
                }
              ],
              "metadata": {
                "version": "0.0.0",
                "links": { "repository": "https://example.com/snark-mini-css" }
              }
            }"#,
        )
        .unwrap();
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
            root.join("queries").join("brackets.scm"),
            "(block) @bracket",
        )
        .unwrap();
        fs::write(
            root.join("test").join("corpus").join("selectors.txt"),
            "==================",
        )
        .unwrap();
        fs::write(
            root.join("test").join("highlight").join("test_css.css"),
            "a { color: red }",
        )
        .unwrap();

        let package = TreeSitterPackageImporter::new(&root).import().unwrap();
        let grammar = package.first_grammar();

        assert_eq!(package.language_name(), "mini_css");
        assert!(package.manifest.is_some());
        assert_eq!(grammar.scanners.len(), 1);
        assert_eq!(grammar.scanners[0].kind, TreeSitterScannerKind::C);
        assert_eq!(grammar.scanners[0].externals.len(), 3);
        let first_external = grammar.scanners[0].externals.get(0).unwrap();
        let second_external = grammar.scanners[0].externals.get(1).unwrap();
        assert_eq!(first_external.ordinal().get(), 0);
        assert_eq!(
            first_external.name().map(|name| name.as_str()),
            Some("_descendant_operator")
        );
        assert_eq!(second_external.ordinal().get(), 1);
        assert!(second_external.name().is_none());
        assert!(
            grammar
                .queries
                .well_known(WellKnownQuery::Highlights)
                .is_some()
        );
        assert!(
            grammar
                .queries
                .iter()
                .any(|file| file.path.as_str() == "queries/brackets.scm")
        );
        assert_eq!(
            grammar
                .node_types_json
                .as_ref()
                .map(|file| file.body.raw.as_str()),
            Some("[]")
        );
        assert!(
            grammar
                .corpus
                .iter()
                .any(|fixture| fixture.source.path.as_str() == "test/highlight/test_css.css")
        );
        assert!(
            grammar
                .corpus
                .iter()
                .any(|fixture| fixture.source.path.as_str() == "test/corpus/selectors.txt")
        );

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn imports_manifest_grammar_paths_and_configured_query_order() {
        let root = test_package_root("snark-multi-grammar");
        let _ = fs::remove_dir_all(&root);
        let first = root.join("grammars").join("first");
        let second = root.join("grammars").join("second");
        for grammar_root in [&first, &second] {
            fs::create_dir_all(grammar_root.join("src")).unwrap();
            fs::create_dir_all(grammar_root.join("queries")).unwrap();
            fs::write(grammar_root.join("src").join("grammar.json"), MINI_GRAMMAR).unwrap();
            fs::write(grammar_root.join("src").join("node-types.json"), "[]").unwrap();
        }
        fs::write(first.join("queries").join("highlights.scm"), "(a) @tag").unwrap();
        fs::write(second.join("queries").join("base.scm"), "(base) @tag").unwrap();
        fs::write(second.join("queries").join("extra.scm"), "(extra) @tag").unwrap();
        fs::write(
            second.join("queries").join("brackets.scm"),
            "(block) @bracket",
        )
        .unwrap();
        fs::write(
            root.join("tree-sitter.json"),
            r#"{
              "grammars": [
                {
                  "name": "first",
                  "scope": "source.first",
                  "path": "grammars/first"
                },
                {
                  "name": "second",
                  "scope": "source.second",
                  "path": "grammars/second",
                  "highlights": ["queries/base.scm", "queries/extra.scm"]
                }
              ],
              "metadata": {
                "version": "0.0.0",
                "links": { "repository": "https://example.com/snark-multi-grammar" }
              }
            }"#,
        )
        .unwrap();

        let package = TreeSitterPackageImporter::new(&root).import().unwrap();

        assert_eq!(package.grammars.len(), 2);
        assert_eq!(
            package.grammars[0].grammar.path.as_str(),
            "grammars/first/src/grammar.json"
        );
        assert_eq!(
            package.grammars[1].grammar.path.as_str(),
            "grammars/second/src/grammar.json"
        );
        let highlight_paths = package.grammars[1]
            .queries
            .well_known_files(WellKnownQuery::Highlights)
            .map(|file| file.path.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            highlight_paths,
            [
                "grammars/second/queries/base.scm",
                "grammars/second/queries/extra.scm"
            ]
        );
        assert!(
            package.grammars[1]
                .queries
                .iter_files()
                .any(|file| file.category.is_none()
                    && file.source.path.as_str() == "grammars/second/queries/brackets.scm")
        );

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn malformed_grammar_json_reports_source_diagnostic() {
        let root = test_package_root("snark-bad-json");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src").join("grammar.json"), r#"{"name": "bad""#).unwrap();

        let error = TreeSitterPackageImporter::new(&root).import().unwrap_err();
        let diagnostic = error.diagnostic();

        assert_eq!(diagnostic.code, DiagnosticCode::JsonDecode);
        assert_eq!(
            diagnostic.primary_span.map(|span| span.source_id.get()),
            Some(0)
        );
        assert!(
            diagnostic
                .notes
                .iter()
                .any(|note| note == "package path: src/grammar.json")
        );

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn malformed_manifest_json_reports_manifest_diagnostic() {
        let root = test_package_root("snark-bad-manifest");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("tree-sitter.json"), r#"{"grammars":"nope"}"#).unwrap();

        let error = TreeSitterPackageImporter::new(&root).import().unwrap_err();
        let diagnostic = error.diagnostic();

        assert_eq!(diagnostic.code, DiagnosticCode::JsonDecode);
        assert_eq!(
            diagnostic.primary_span.map(|span| span.source_id.get()),
            Some(0)
        );
        assert!(
            diagnostic
                .notes
                .iter()
                .any(|note| note == "package path: tree-sitter.json")
        );

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn empty_manifest_grammar_list_is_an_import_error() {
        let root = test_package_root("snark-empty-manifest");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("tree-sitter.json"),
            r#"{
              "grammars": [],
              "metadata": {
                "version": "0.0.0",
                "links": { "repository": "https://example.com/empty" }
              }
            }"#,
        )
        .unwrap();

        let error = TreeSitterPackageImporter::new(&root).import().unwrap_err();
        let diagnostic = error.diagnostic();

        assert_eq!(diagnostic.code, DiagnosticCode::NoGrammars);

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn malformed_node_types_json_reports_node_types_diagnostic() {
        let root = test_package_root("snark-bad-node-types");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src").join("grammar.json"), MINI_GRAMMAR).unwrap();
        fs::write(root.join("src").join("node-types.json"), r#"[{"type": 1}]"#).unwrap();

        let error = TreeSitterPackageImporter::new(&root).import().unwrap_err();
        let diagnostic = error.diagnostic();

        assert_eq!(diagnostic.code, DiagnosticCode::JsonDecode);
        assert!(
            diagnostic
                .notes
                .iter()
                .any(|note| note == "package path: src/node-types.json")
        );

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn package_root_must_be_directory() {
        let root = test_package_root("snark-root-file");
        let _ = fs::remove_file(&root);
        fs::write(&root, "not a directory").unwrap();

        let error = TreeSitterPackageImporter::new(&root).import().unwrap_err();
        let diagnostic = error.diagnostic();

        assert_eq!(diagnostic.code, DiagnosticCode::InvalidPackageRoot);

        fs::remove_file(&root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn source_reads_reject_symlink_escape() {
        let root = test_package_root("snark-symlink-escape");
        let outside = test_package_root("snark-symlink-outside");
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_file(&outside);
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("queries")).unwrap();
        fs::write(root.join("src").join("grammar.json"), MINI_GRAMMAR).unwrap();
        fs::write(&outside, "(escaped) @tag").unwrap();
        std::os::unix::fs::symlink(&outside, root.join("queries").join("highlights.scm")).unwrap();

        let error = TreeSitterPackageImporter::new(&root).import().unwrap_err();
        let diagnostic = error.diagnostic();

        assert_eq!(diagnostic.code, DiagnosticCode::PathOutsidePackage);

        fs::remove_dir_all(&root).unwrap();
        fs::remove_file(&outside).unwrap();
    }

    #[test]
    fn imports_configured_tree_sitter_css_checkout() {
        let Ok(path) = std::env::var("SNARK_TREE_SITTER_CSS") else {
            return;
        };

        let package = TreeSitterPackageImporter::new(path).import().unwrap();
        let grammar = package.first_grammar();

        assert_eq!(package.language_name(), "css");
        assert_eq!(
            grammar
                .grammar
                .body
                .grammar
                .start_rule()
                .map(|(name, _)| name.as_str().to_owned()),
            Some("stylesheet".to_string())
        );
        assert_eq!(grammar.grammar.body.grammar.rules.len(), 66);
        assert_eq!(grammar.grammar.body.grammar.externals.len(), 3);
        assert!(package.manifest.is_some());
        assert_eq!(grammar.scanners.len(), 1);
        assert_eq!(grammar.scanners[0].kind, TreeSitterScannerKind::C);
        assert_eq!(grammar.scanners[0].externals.len(), 3);
        assert!(
            grammar
                .queries
                .well_known(WellKnownQuery::Highlights)
                .is_some()
        );
        assert!(
            grammar.corpus.iter().any(|fixture| fixture
                .source
                .path
                .as_str()
                .starts_with("test/highlight/"))
        );
    }

    fn test_package_root(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("{}-{}", name, std::process::id()))
    }
}
