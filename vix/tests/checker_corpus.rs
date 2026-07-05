use facet::Facet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Facet, Debug)]
struct CorpusEntry {
    schema_version: u32,
    id: String,
    outcome: String,
    description: String,
    sources: Vec<CorpusSource>,
    assertions: Vec<CorpusAssertion>,
}

#[derive(Facet, Debug)]
struct CorpusSource {
    path: String,
    module: String,
}

#[derive(Facet, Debug)]
struct CorpusAssertion {
    query: String,
    subject: String,
    expect: String,
    span_start: u32,
    span_end: u32,
}

fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/checker-corpus")
}

fn corpus_json_files() -> Vec<PathBuf> {
    let mut files = Vec::new();
    for dir in ["accept", "reject"] {
        let dir = corpus_root().join(dir);
        for entry in fs::read_dir(&dir).unwrap_or_else(|err| panic!("read {dir:?}: {err}")) {
            let path = entry.expect("directory entry").path();
            if path.extension().is_some_and(|ext| ext == "json") {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

fn load_entry(path: &Path) -> CorpusEntry {
    let text = fs::read_to_string(path).unwrap_or_else(|err| panic!("read {path:?}: {err}"));
    facet_json::from_str(&text).unwrap_or_else(|err| panic!("parse {path:?}: {err}"))
}

#[test]
fn checker_corpus_entries_are_real_facet_json() {
    let files = corpus_json_files();
    assert!(!files.is_empty(), "checker corpus is not empty");

    let root = corpus_root();
    for file in files {
        let entry = load_entry(&file);
        assert_eq!(entry.schema_version, 1, "{file:?}");
        assert!(
            matches!(entry.outcome.as_str(), "accept" | "reject"),
            "{file:?}"
        );
        assert!(!entry.id.is_empty(), "{file:?}");
        assert!(!entry.description.is_empty(), "{file:?}");
        assert!(!entry.sources.is_empty(), "{file:?}");
        assert!(!entry.assertions.is_empty(), "{file:?}");

        for source in &entry.sources {
            assert!(!source.module.is_empty(), "{file:?}");
            let source_path = root.join(&source.path);
            assert!(
                source_path.exists(),
                "{file:?} references missing {source_path:?}"
            );
            assert!(
                source_path.extension().is_some_and(|ext| ext == "vix"),
                "{file:?} source is not .vix: {source_path:?}"
            );
        }

        for assertion in &entry.assertions {
            assert!(!assertion.query.is_empty(), "{file:?}");
            assert!(!assertion.subject.is_empty(), "{file:?}");
            assert!(!assertion.expect.is_empty(), "{file:?}");
            assert!(
                assertion.span_start <= assertion.span_end,
                "{file:?} invalid span in {assertion:?}"
            );
        }
    }
}

#[test]
#[ignore = "pending vix-written checker binary"]
fn checker_corpus_matches_vix_checker() {
    let files = corpus_json_files();
    panic!(
        "wire {} checker corpus entries to the Vix checker once it exists",
        files.len()
    );
}
