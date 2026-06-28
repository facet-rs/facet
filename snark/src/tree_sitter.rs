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

/// Imported artifacts for one Tree-sitter grammar entry.
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

/// Imported Tree-sitter package artifacts.
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

    /// Import package artifacts.
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
    use std::fs;

    use crate::{
        diagnostic::DiagnosticCode,
        grammar::{PrecedenceValue, RawGrammarJson, RawRuleJson},
        query::WellKnownQuery,
        scanner::TreeSitterScannerKind,
    };

    use super::*;

    const CSS_FIXTURE: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/packages/tree-sitter-css-reduced"
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
                ("test/highlight/test_css.css".to_string(), 5),
            ]
        );
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
