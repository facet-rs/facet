//! Tree-sitter package import boundary.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use facet::Facet;

use crate::{
    corpus::{CorpusFixture, CorpusKind, CorpusSource},
    diagnostic::ImportError,
    grammar::RawGrammarJson,
    query::{QueryBundle, QuerySource},
    scanner::{
        ExternalTokenDecl, ExternalTokenOrdinal, ScannerSource, TreeSitterScanner,
        TreeSitterScannerKind,
    },
    source::{
        PackageRelativePath, PackageRoot, SourceFile, SourceIdAllocator, TreeSitterConfigJson,
        read_optional_source_string, read_source_string,
    },
};

/// Raw generated `src/parser.c` source.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
pub struct ParserC(pub String);

/// Raw generated `src/node-types.json` source.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
pub struct NodeTypesJson(pub String);

/// Imported Tree-sitter package artifacts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedPackage {
    /// Package root.
    pub root: PackageRoot,
    /// Optional `tree-sitter.json`.
    pub manifest: Option<SourceFile<TreeSitterConfigJson>>,
    /// Raw grammar JSON with source provenance.
    pub grammar: SourceFile<RawGrammarJson>,
    /// Optional generated parser source used as an oracle artifact.
    pub parser_c: Option<SourceFile<ParserC>>,
    /// Optional `node-types.json`.
    pub node_types_json: Option<SourceFile<NodeTypesJson>>,
    /// Imported Tree-sitter scanners.
    pub scanners: Vec<TreeSitterScanner>,
    /// Imported query sources, including unknown query files.
    pub queries: QueryBundle,
    /// Imported corpus and highlight fixture sources.
    pub corpus: Vec<CorpusFixture>,
}

impl ImportedPackage {
    /// Language name from `grammar.json`.
    pub fn language_name(&self) -> &str {
        &self.grammar.body.name
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
        let mut ids = SourceIdAllocator::new();
        let manifest = read_optional_typed(
            &self.root,
            "tree-sitter.json",
            &mut ids,
            TreeSitterConfigJson,
        )?;
        let grammar_source = read_source_string(&self.root, &rel("src/grammar.json")?, &mut ids)?;
        let grammar = RawGrammarJson::from_source_file(&self.root, grammar_source)?;
        let parser_c = read_optional_typed(&self.root, "src/parser.c", &mut ids, ParserC)?;
        let node_types_json =
            read_optional_typed(&self.root, "src/node-types.json", &mut ids, NodeTypesJson)?;
        let scanners = import_scanners(&self.root, &grammar.body, &mut ids)?;
        let queries = import_queries(&self.root, &mut ids)?;
        let corpus = import_corpus(&self.root, &mut ids)?;

        Ok(ImportedPackage {
            root: self.root,
            manifest,
            grammar,
            parser_c,
            node_types_json,
            scanners,
            queries,
            corpus,
        })
    }
}

fn read_optional_typed<T>(
    root: &PackageRoot,
    relative: &str,
    ids: &mut SourceIdAllocator,
    wrap: impl FnOnce(String) -> T,
) -> Result<Option<SourceFile<T>>, ImportError> {
    Ok(read_optional_source_string(root, &rel(relative)?, ids)?.map(|source| source.map(wrap)))
}

fn import_scanners(
    root: &PackageRoot,
    grammar: &RawGrammarJson,
    ids: &mut SourceIdAllocator,
) -> Result<Vec<TreeSitterScanner>, ImportError> {
    let externals = grammar
        .externals
        .iter()
        .cloned()
        .enumerate()
        .map(|(index, rule)| ExternalTokenDecl::new(ExternalTokenOrdinal::new(index as u32), rule))
        .collect::<Vec<_>>();
    let mut scanners = Vec::new();

    if let Some(source) = read_optional_source_string(root, &rel("src/scanner.c")?, ids)? {
        scanners.push(TreeSitterScanner {
            kind: TreeSitterScannerKind::C,
            source: source.map(ScannerSource),
            externals: externals.clone(),
        });
    }

    if let Some(source) = read_optional_source_string(root, &rel("src/scanner.cc")?, ids)? {
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
    ids: &mut SourceIdAllocator,
) -> Result<QueryBundle, ImportError> {
    let queries_dir = root.join(&rel("queries")?);
    let mut files = Vec::new();
    collect_source_files(root, &queries_dir, ids, &mut |source| {
        files.push(source.map(QuerySource));
    })?;
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(QueryBundle { files })
}

fn import_corpus(
    root: &PackageRoot,
    ids: &mut SourceIdAllocator,
) -> Result<Vec<CorpusFixture>, ImportError> {
    let mut fixtures = Vec::new();
    collect_corpus_dir(root, "test/corpus", CorpusKind::Parse, ids, &mut fixtures)?;
    collect_corpus_dir(
        root,
        "test/highlight",
        CorpusKind::Highlight,
        ids,
        &mut fixtures,
    )?;
    collect_corpus_dir(
        root,
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
    relative: &str,
    kind: CorpusKind,
    ids: &mut SourceIdAllocator,
    fixtures: &mut Vec<CorpusFixture>,
) -> Result<(), ImportError> {
    let dir = root.join(&rel(relative)?);
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

    for entry in entries {
        let entry = entry.map_err(|source| ImportError::ReadDir {
            package_root: root.as_path().to_owned(),
            path: dir.to_owned(),
            source,
        })?;
        let path = entry.path();
        if path.is_dir() {
            collect_source_files(root, &path, ids, push)?;
        } else if path.is_file() {
            let relative = path.strip_prefix(root.as_path()).unwrap_or(&path);
            let relative = PackageRelativePath::new(relative)?;
            let source = read_source_string(root, &relative, ids)?;
            push(source);
        }
    }
    Ok(())
}

fn rel(path: &str) -> Result<PackageRelativePath, ImportError> {
    PackageRelativePath::new(Path::new(path))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::{
        grammar::{PrecedenceValue, RawRuleJson},
        query::WellKnownQuery,
        scanner::TreeSitterScannerKind,
    };

    use super::*;

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
    fn imports_tree_sitter_package_artifacts() {
        let root = test_package_root("snark-mini-css");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("queries")).unwrap();
        fs::create_dir_all(root.join("test").join("corpus")).unwrap();
        fs::create_dir_all(root.join("test").join("highlight")).unwrap();
        fs::write(root.join("tree-sitter.json"), r#"{"grammars":[]}"#).unwrap();
        fs::write(root.join("src").join("grammar.json"), MINI_GRAMMAR).unwrap();
        fs::write(root.join("src").join("parser.c"), "/* parser */").unwrap();
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

        assert_eq!(package.language_name(), "mini_css");
        assert!(package.manifest.is_some());
        assert!(package.parser_c.is_some());
        assert_eq!(package.scanners.len(), 1);
        assert_eq!(package.scanners[0].kind, TreeSitterScannerKind::C);
        assert_eq!(package.scanners[0].externals.len(), 3);
        assert_eq!(package.scanners[0].externals[0].ordinal.get(), 0);
        assert_eq!(
            package.scanners[0].externals[0]
                .name
                .as_ref()
                .map(|name| name.as_str()),
            Some("_descendant_operator")
        );
        assert_eq!(package.scanners[0].externals[1].ordinal.get(), 1);
        assert!(package.scanners[0].externals[1].name.is_none());
        assert!(
            package
                .queries
                .well_known(WellKnownQuery::Highlights)
                .is_some()
        );
        assert!(
            package
                .queries
                .iter()
                .any(|file| file.path.as_str() == "queries/brackets.scm")
        );
        assert_eq!(
            package
                .node_types_json
                .as_ref()
                .map(|file| file.body.0.as_str()),
            Some("[]")
        );
        assert!(
            package
                .corpus
                .iter()
                .any(|fixture| fixture.source.path.as_str() == "test/highlight/test_css.css")
        );
        assert!(
            package
                .corpus
                .iter()
                .any(|fixture| fixture.source.path.as_str() == "test/corpus/selectors.txt")
        );

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn imports_configured_tree_sitter_css_checkout() {
        let Ok(path) = std::env::var("SNARK_TREE_SITTER_CSS") else {
            return;
        };

        let package = TreeSitterPackageImporter::new(path).import().unwrap();

        assert_eq!(package.language_name(), "css");
        assert_eq!(
            package
                .grammar
                .body
                .start_rule()
                .map(|(name, _)| name.as_str().to_owned()),
            Some("stylesheet".to_string())
        );
        assert_eq!(package.grammar.body.rules.len(), 66);
        assert_eq!(package.grammar.body.externals.len(), 3);
        assert!(package.manifest.is_some());
        assert!(package.parser_c.is_some());
        assert_eq!(package.scanners.len(), 1);
        assert_eq!(package.scanners[0].kind, TreeSitterScannerKind::C);
        assert_eq!(package.scanners[0].externals.len(), 3);
        assert!(
            package
                .queries
                .well_known(WellKnownQuery::Highlights)
                .is_some()
        );
        assert!(
            package.corpus.iter().any(|fixture| fixture
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
