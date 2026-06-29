#![forbid(unsafe_code)]
//! WebAssembly bindings for Snark playgrounds.

use std::cell::RefCell;

use facet::Facet;
use snark::{
    corpus::CorpusSource,
    grammar::RawGrammarJson,
    lexical::LexicalFacts,
    parser::{
        ExternalId, ParseTable, ParserGrammar, ReducedExternalScan, ReducedExternalScanResult,
        ReducedExternalScanner, ReducedParseError, RuntimeParseReport, RuntimeParser,
        ScannerSnapshotId,
    },
    query::QuerySource,
    validated::ValidatedGrammar,
};
use wasm_bindgen::prelude::*;

#[derive(Debug, Clone, Facet)]
struct PlaygroundRequest {
    files: Vec<BundleFile>,
    input: String,
    run_corpus: bool,
}

#[derive(Debug, Clone, Facet)]
struct BundleFile {
    path: String,
    text: String,
}

#[derive(Debug, Clone, Facet)]
struct PlaygroundResponse {
    ok: bool,
    language: Option<String>,
    diagnostics: Vec<Diagnostic>,
    bundle: BundleSummary,
    parse: Option<ParseOutput>,
    highlights: Vec<HighlightOutput>,
    corpus: Vec<CorpusOutput>,
    limitations: Vec<String>,
}

#[derive(Debug, Clone, Facet)]
struct Diagnostic {
    stage: String,
    message: String,
}

#[derive(Debug, Clone, Facet)]
struct BundleSummary {
    grammar_path: Option<String>,
    grammar_js_path: Option<String>,
    query_paths: Vec<String>,
    corpus_paths: Vec<String>,
    sample_paths: Vec<String>,
    generated_files_ignored: Vec<String>,
    scanner_paths: Vec<String>,
    active_scanner: Option<String>,
}

#[derive(Debug, Clone, Facet)]
struct ParseOutput {
    sexp: String,
    accepted_count: usize,
    failure_count: usize,
    max_live_versions: usize,
    trace_event_count: usize,
    tree_event_count: usize,
    accepted_tree_event_count: usize,
}

#[derive(Debug, Clone, Facet)]
struct HighlightOutput {
    capture_name: String,
    text: String,
    start_byte: u32,
    end_byte: u32,
    start_row: u32,
    start_column: u32,
    end_row: u32,
    end_column: u32,
}

#[derive(Debug, Clone, Facet)]
struct CorpusOutput {
    path: String,
    case_name: String,
    passed: bool,
    input: String,
    expected: String,
    actual: Option<String>,
    error: Option<String>,
}

struct PreparedGrammar {
    raw: RawGrammarJson,
    validated: ValidatedGrammar,
    parser: ParserGrammar,
    table: ParseTable,
}

struct ScannerSelection {
    scanner: Option<CssBundleExternalScanner>,
    active_scanner: Option<String>,
}

/// Parse one request with Snark and return a JSON response.
#[wasm_bindgen(js_name = parseBundle)]
pub fn parse_bundle(request_json: &str) -> String {
    response_json(playground_response(request_json))
}

fn playground_response(request_json: &str) -> PlaygroundResponse {
    let request = match facet_json::from_str::<PlaygroundRequest>(request_json) {
        Ok(request) => request,
        Err(error) => {
            return response_with_diagnostic(
                "request",
                format!("could not decode playground request JSON: {error}"),
            );
        }
    };
    let files = normalize_bundle_files(request.files);
    let mut bundle = summarize_bundle(&files);
    let Some(grammar_file) = find_file(&files, "src/grammar.json") else {
        let message = match &bundle.grammar_js_path {
            Some(path) => format!(
                "bundle contains {path}, but this WASM entrypoint consumes src/grammar.json; the playground shell should evaluate grammar.js into an in-memory src/grammar.json before calling parseBundle"
            ),
            None => "bundle does not contain src/grammar.json".to_owned(),
        };
        return PlaygroundResponse {
            ok: false,
            language: None,
            diagnostics: vec![Diagnostic {
                stage: "bundle".to_owned(),
                message,
            }],
            bundle,
            parse: None,
            highlights: Vec::new(),
            corpus: Vec::new(),
            limitations: limitations(&files, None),
        };
    };

    let prepared = match prepare_grammar(&grammar_file.text) {
        Ok(prepared) => prepared,
        Err((stage, message)) => {
            return PlaygroundResponse {
                ok: false,
                language: None,
                diagnostics: vec![Diagnostic { stage, message }],
                bundle,
                parse: None,
                highlights: Vec::new(),
                corpus: Vec::new(),
                limitations: limitations(&files, None),
            };
        }
    };

    let mut diagnostics = Vec::new();
    let mut parse = None;
    let mut highlights = Vec::new();
    let scanner_selection = scanner_selection(&files, &prepared.parser);
    bundle.active_scanner = scanner_selection.active_scanner.clone();
    let runtime = RuntimeParser::new(&prepared.validated, &prepared.parser, &prepared.table);
    match runtime {
        Ok(runtime) => match parse_with_optional_scanner(
            runtime,
            scanner_selection
                .scanner
                .as_ref()
                .map(|scanner| scanner as &dyn ReducedExternalScanner),
            &request.input,
        ) {
            Ok(report) => {
                let accepted_tree_event_count = report.accepted_tree_events().len();
                parse = Some(ParseOutput {
                    sexp: report.tree().to_sexp(),
                    accepted_count: report.accepted_count(),
                    failure_count: report.failure_count(),
                    max_live_versions: report.max_live_versions(),
                    trace_event_count: report.trace_events().len(),
                    tree_event_count: report.tree_events().len(),
                    accepted_tree_event_count,
                });
                if let Some(query_file) = find_file(&files, "queries/highlights.scm") {
                    highlights = highlight_outputs(
                        &QuerySource(query_file.text.clone()),
                        &prepared.parser,
                        &report,
                        &request.input,
                    );
                }
            }
            Err(error) => diagnostics.push(Diagnostic {
                stage: "parse".to_owned(),
                message: error.to_string(),
            }),
        },
        Err(error) => diagnostics.push(Diagnostic {
            stage: "runtime".to_owned(),
            message: error.to_string(),
        }),
    }

    let corpus = if request.run_corpus {
        run_corpus_cases(&files, &prepared)
    } else {
        Vec::new()
    };

    PlaygroundResponse {
        ok: diagnostics.is_empty(),
        language: Some(prepared.raw.name.clone()),
        diagnostics,
        bundle,
        parse,
        highlights,
        corpus,
        limitations: limitations(&files, scanner_selection.active_scanner.as_deref()),
    }
}

fn prepare_grammar(grammar_json: &str) -> Result<PreparedGrammar, (String, String)> {
    let raw = facet_json::from_str::<RawGrammarJson>(grammar_json)
        .map_err(|error| ("grammar".to_owned(), error.to_string()))?;
    let validated = ValidatedGrammar::from_raw(&raw)
        .map_err(|error| ("validate".to_owned(), error.to_string()))?;
    let lexical = LexicalFacts::from_grammar(&validated);
    let parser = ParserGrammar::normalize_from_validated(&validated, &lexical)
        .map_err(|error| ("normalize".to_owned(), error.to_string()))?
        .prepare_productions_for_items()
        .map_err(|error| ("prepare".to_owned(), error.to_string()))?;
    let table = ParseTable::from_grammar(&parser)
        .map_err(|error| ("table".to_owned(), error.to_string()))?;
    Ok(PreparedGrammar {
        raw,
        validated,
        parser,
        table,
    })
}

fn scanner_selection(files: &[BundleFile], parser: &ParserGrammar) -> ScannerSelection {
    let Some(scanner_file) = find_file(files, "src/scanner.c") else {
        return ScannerSelection {
            scanner: None,
            active_scanner: None,
        };
    };
    if scanner_file.text != snark_scanner_host::CSS_SCANNER_SOURCE {
        return ScannerSelection {
            scanner: None,
            active_scanner: None,
        };
    }
    let css_externals = parser
        .symbols()
        .externals()
        .iter()
        .map(|external| (external.ordinal(), external.name()))
        .collect::<Vec<_>>();
    if css_externals
        != [
            (0, Some("_descendant_operator")),
            (1, Some("_pseudo_class_selector_colon")),
            (2, Some("__error_recovery")),
        ]
    {
        return ScannerSelection {
            scanner: None,
            active_scanner: None,
        };
    }
    ScannerSelection {
        scanner: Some(CssBundleExternalScanner::new(parser)),
        active_scanner: Some(
            "built-in compiled reduced CSS scanner matched from src/scanner.c".to_owned(),
        ),
    }
}

struct CssBundleExternalScanner {
    scanner: RefCell<snark_scanner_host::CssScanner>,
    external_ordinals: Vec<(ExternalId, usize)>,
    snapshots: RefCell<Vec<Vec<u8>>>,
}

impl CssBundleExternalScanner {
    fn new(parser: &ParserGrammar) -> Self {
        Self {
            scanner: RefCell::new(snark_scanner_host::CssScanner::new()),
            external_ordinals: parser
                .symbols()
                .externals()
                .iter()
                .map(|external| (external.id(), external.ordinal() as usize))
                .collect(),
            snapshots: RefCell::new(vec![Vec::new()]),
        }
    }

    fn ordinal_for(&self, external: ExternalId) -> Option<usize> {
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

impl ReducedExternalScanner for CssBundleExternalScanner {
    fn scan(
        &self,
        request: ReducedExternalScan<'_>,
    ) -> Result<Option<ReducedExternalScanResult>, ReducedParseError> {
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

fn highlight_outputs(
    query: &QuerySource,
    parser: &ParserGrammar,
    report: &snark::parser::RuntimeParseReport,
    input: &str,
) -> Vec<HighlightOutput> {
    query
        .execute_runtime_highlights(parser, report, input)
        .into_iter()
        .map(|capture| {
            let bytes = capture.bytes();
            let points = capture.points();
            HighlightOutput {
                capture_name: capture.capture_name().to_owned(),
                text: capture.text().to_owned(),
                start_byte: bytes.start().get(),
                end_byte: bytes.end().get(),
                start_row: points.start().row().get(),
                start_column: points.start().column().get(),
                end_row: points.end().row().get(),
                end_column: points.end().column().get(),
            }
        })
        .collect()
}

fn run_corpus_cases(files: &[BundleFile], prepared: &PreparedGrammar) -> Vec<CorpusOutput> {
    let mut results = Vec::new();
    let scanner_selection = scanner_selection(files, &prepared.parser);
    for file in files
        .iter()
        .filter(|file| file.path.starts_with("test/corpus/") && file.path.ends_with(".txt"))
    {
        let source = CorpusSource(file.text.clone());
        let cases = match source.parse_cases() {
            Ok(cases) => cases,
            Err(error) => {
                results.push(CorpusOutput {
                    path: file.path.clone(),
                    case_name: "<fixture>".to_owned(),
                    passed: false,
                    input: String::new(),
                    expected: String::new(),
                    actual: None,
                    error: Some(error.to_string()),
                });
                continue;
            }
        };
        for case in cases {
            let runtime =
                match RuntimeParser::new(&prepared.validated, &prepared.parser, &prepared.table) {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        results.push(CorpusOutput {
                            path: file.path.clone(),
                            case_name: case.name,
                            passed: false,
                            input: case.input,
                            expected: case.expected.to_sexp(),
                            actual: None,
                            error: Some(error.to_string()),
                        });
                        continue;
                    }
                };
            match parse_with_optional_scanner(
                runtime,
                scanner_selection
                    .scanner
                    .as_ref()
                    .map(|scanner| scanner as &dyn ReducedExternalScanner),
                &case.input,
            ) {
                Ok(report) => {
                    let actual = report.tree().to_sexp();
                    let expected = case.expected.to_sexp();
                    results.push(CorpusOutput {
                        path: file.path.clone(),
                        case_name: case.name,
                        passed: actual == expected,
                        input: case.input,
                        expected,
                        actual: Some(actual),
                        error: None,
                    });
                }
                Err(error) => results.push(CorpusOutput {
                    path: file.path.clone(),
                    case_name: case.name,
                    passed: false,
                    input: case.input,
                    expected: case.expected.to_sexp(),
                    actual: None,
                    error: Some(error.to_string()),
                }),
            }
        }
    }
    results
}

fn parse_with_optional_scanner<'a>(
    runtime: RuntimeParser<'a>,
    scanner: Option<&'a dyn ReducedExternalScanner>,
    input: &str,
) -> Result<RuntimeParseReport, ReducedParseError> {
    match scanner {
        Some(scanner) => runtime
            .with_external_scanner(scanner)
            .parse_with_report(input),
        None => runtime.parse_with_report(input),
    }
}

fn response_with_diagnostic(stage: &str, message: String) -> PlaygroundResponse {
    PlaygroundResponse {
        ok: false,
        language: None,
        diagnostics: vec![Diagnostic {
            stage: stage.to_owned(),
            message,
        }],
        bundle: BundleSummary {
            grammar_path: None,
            grammar_js_path: None,
            query_paths: Vec::new(),
            corpus_paths: Vec::new(),
            sample_paths: Vec::new(),
            generated_files_ignored: Vec::new(),
            scanner_paths: Vec::new(),
            active_scanner: None,
        },
        parse: None,
        highlights: Vec::new(),
        corpus: Vec::new(),
        limitations: vec![
            "Only src/grammar.json, handwritten scanner sources, queries, and corpus/highlight fixtures are accepted as bundle inputs.".to_owned(),
        ],
    }
}

fn response_json(response: PlaygroundResponse) -> String {
    facet_json::to_string_pretty(&response).expect("playground response serializes to JSON")
}

fn summarize_bundle(files: &[BundleFile]) -> BundleSummary {
    BundleSummary {
        grammar_path: find_file(files, "src/grammar.json").map(|file| file.path.clone()),
        grammar_js_path: find_file(files, "grammar.js").map(|file| file.path.clone()),
        query_paths: files
            .iter()
            .filter(|file| file.path.starts_with("queries/"))
            .map(|file| file.path.clone())
            .collect(),
        corpus_paths: files
            .iter()
            .filter(|file| {
                file.path.starts_with("test/corpus/")
                    || file.path.starts_with("test/highlight/")
                    || file.path.starts_with("test/highlights/")
            })
            .map(|file| file.path.clone())
            .collect(),
        sample_paths: files
            .iter()
            .filter(|file| file.path.starts_with("samples/"))
            .map(|file| file.path.clone())
            .collect(),
        generated_files_ignored: files
            .iter()
            .filter(|file| is_generated_artifact(&file.path))
            .map(|file| file.path.clone())
            .collect(),
        scanner_paths: files
            .iter()
            .filter(|file| file.path == "src/scanner.c" || file.path == "src/scanner.cc")
            .map(|file| file.path.clone())
            .collect(),
        active_scanner: None,
    }
}

fn limitations(files: &[BundleFile], active_scanner: Option<&str>) -> Vec<String> {
    let mut limitations = vec![
        "This playground executes Snark's current RuntimeParser path and returns its corpus-normalized S-expression projection.".to_owned(),
        "Generated Tree-sitter files such as src/parser.c and src/node-types.json are ignored.".to_owned(),
        "Recovery, incremental reuse, and complete Tree-sitter query semantics are not implemented in this runtime slice.".to_owned(),
    ];
    if find_file(files, "grammar.js").is_some() {
        limitations.push(
            "Tree-sitter grammar.js is source DSL. The playground shell evaluates it before this WASM parser receives src/grammar.json.".to_owned(),
        );
    }
    if files
        .iter()
        .any(|file| file.path == "src/scanner.c" || file.path == "src/scanner.cc")
    {
        if let Some(active_scanner) = active_scanner {
            limitations.push(format!(
                "External scanner support is source-gated in this build: {active_scanner}."
            ));
        } else {
            limitations.push(
                "Uploaded scanner.c/scanner.cc sources are reported, but the browser runtime only executes scanners that have an explicit source-matched host adapter.".to_owned(),
            );
        }
    }
    limitations
}

fn normalize_bundle_files(files: Vec<BundleFile>) -> Vec<BundleFile> {
    files
        .into_iter()
        .map(|file| BundleFile {
            path: normalize_bundle_path(&file.path),
            text: file.text,
        })
        .collect()
}

fn find_file<'a>(files: &'a [BundleFile], path: &str) -> Option<&'a BundleFile> {
    files.iter().find(|file| file.path == path)
}

fn normalize_bundle_path(path: &str) -> String {
    let path = normalize_path(path);
    if let Some(relative) = arborium_def_relative(&path) {
        if let Some(mapped) = normalize_arborium_def_path(relative) {
            return mapped;
        }
    }
    if let Some(mapped) = normalize_package_path(&path) {
        return mapped;
    }
    path
}

fn normalize_path(path: &str) -> String {
    let mut path = path.replace('\\', "/");
    while let Some(stripped) = path.strip_prefix("./") {
        path = stripped.to_owned();
    }
    path
}

fn arborium_def_relative(path: &str) -> Option<&str> {
    path.strip_prefix("def/")
        .or_else(|| path.split_once("/def/").map(|(_, relative)| relative))
}

fn normalize_arborium_def_path(relative: &str) -> Option<String> {
    match relative {
        "grammar/grammar.js" => Some("grammar.js".to_owned()),
        "grammar/grammar.json" | "grammar/src/grammar.json" => Some("src/grammar.json".to_owned()),
        "grammar/scanner.c" => Some("src/scanner.c".to_owned()),
        "grammar/scanner.cc" => Some("src/scanner.cc".to_owned()),
        "grammar/src/parser.c" => Some("src/parser.c".to_owned()),
        "grammar/src/parser.cc" => Some("src/parser.cc".to_owned()),
        "grammar/src/parser.h" => Some("src/parser.h".to_owned()),
        "grammar/src/node-types.json" => Some("src/node-types.json".to_owned()),
        "grammar/bindings/node/binding.cc" => Some("bindings/node/binding.cc".to_owned()),
        _ => {
            if relative.starts_with("queries/")
                || relative.starts_with("test/corpus/")
                || relative.starts_with("test/highlight/")
                || relative.starts_with("test/highlights/")
            {
                Some(relative.to_owned())
            } else if let Some(sample) = relative.strip_prefix("samples/") {
                Some(format!("samples/{sample}"))
            } else if relative.starts_with("sample.") {
                Some(format!("samples/{relative}"))
            } else {
                None
            }
        }
    }
}

fn normalize_package_path(path: &str) -> Option<String> {
    match path {
        "grammar.js"
        | "src/grammar.json"
        | "src/scanner.c"
        | "src/scanner.cc"
        | "src/parser.c"
        | "src/parser.cc"
        | "src/parser.h"
        | "src/node-types.json"
        | "bindings/node/binding.cc" => return Some(path.to_owned()),
        _ => {}
    }

    for suffix in [
        "/grammar.js",
        "/src/grammar.json",
        "/src/scanner.c",
        "/src/scanner.cc",
        "/src/parser.c",
        "/src/parser.cc",
        "/src/parser.h",
        "/src/node-types.json",
        "/bindings/node/binding.cc",
    ] {
        if path.ends_with(suffix) {
            return Some(suffix.trim_start_matches('/').to_owned());
        }
    }

    for token in [
        "/queries/",
        "/test/corpus/",
        "/test/highlight/",
        "/test/highlights/",
        "/samples/",
    ] {
        if let Some((_, relative)) = path.split_once(token) {
            return Some(format!("{}{relative}", token.trim_start_matches('/')));
        }
    }

    None
}

fn is_generated_artifact(path: &str) -> bool {
    matches!(
        path,
        "src/parser.c"
            | "src/parser.cc"
            | "src/parser.h"
            | "src/node-types.json"
            | "bindings/node/binding.cc"
    ) || path.ends_with("/src/parser.c")
        || path.ends_with("/src/parser.cc")
        || path.ends_with("/src/parser.h")
        || path.ends_with("/src/node-types.json")
        || path.ends_with("/bindings/node/binding.cc")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_arborium_def_bundle_paths() {
        let files = normalize_bundle_files(vec![
            BundleFile {
                path: "langs/group-acorn/css/def/grammar/grammar.js".to_owned(),
                text: "module.exports = grammar({ name: 'css', rules: { stylesheet: _ => '' } });"
                    .to_owned(),
            },
            BundleFile {
                path: "langs/group-acorn/css/def/grammar/scanner.c".to_owned(),
                text: String::new(),
            },
            BundleFile {
                path: "langs/group-acorn/css/def/queries/highlights.scm".to_owned(),
                text: String::new(),
            },
            BundleFile {
                path: "langs/group-acorn/css/def/sample.css".to_owned(),
                text: String::new(),
            },
            BundleFile {
                path: "prepared-css/samples/showcase.css".to_owned(),
                text: String::new(),
            },
            BundleFile {
                path: "langs/group-acorn/css/def/grammar/src/node-types.json".to_owned(),
                text: String::new(),
            },
        ]);

        let summary = summarize_bundle(&files);
        assert_eq!(summary.grammar_js_path.as_deref(), Some("grammar.js"));
        assert_eq!(summary.scanner_paths, vec!["src/scanner.c"]);
        assert_eq!(summary.query_paths, vec!["queries/highlights.scm"]);
        assert!(find_file(&files, "samples/sample.css").is_some());
        assert!(find_file(&files, "samples/showcase.css").is_some());
        assert_eq!(
            summary.sample_paths,
            vec!["samples/sample.css", "samples/showcase.css"]
        );
        assert_eq!(summary.generated_files_ignored, vec!["src/node-types.json"]);
    }

    #[test]
    fn normalizes_prepared_bundle_root_paths() {
        let files = normalize_bundle_files(vec![
            BundleFile {
                path: "snark-json/src/grammar.json".to_owned(),
                text: "{}".to_owned(),
            },
            BundleFile {
                path: "snark-json/queries/highlights.scm".to_owned(),
                text: String::new(),
            },
            BundleFile {
                path: "snark-json/test/corpus/main.txt".to_owned(),
                text: String::new(),
            },
            BundleFile {
                path: "snark-json/samples/sample.json".to_owned(),
                text: String::new(),
            },
            BundleFile {
                path: "snark-json/src/parser.c".to_owned(),
                text: String::new(),
            },
        ]);

        let summary = summarize_bundle(&files);
        assert_eq!(summary.grammar_path.as_deref(), Some("src/grammar.json"));
        assert_eq!(summary.query_paths, vec!["queries/highlights.scm"]);
        assert_eq!(summary.corpus_paths, vec!["test/corpus/main.txt"]);
        assert_eq!(summary.sample_paths, vec!["samples/sample.json"]);
        assert_eq!(summary.generated_files_ignored, vec!["src/parser.c"]);
    }

    #[test]
    fn reports_grammar_js_bundle_as_needing_in_memory_grammar_json() {
        let request = PlaygroundRequest {
            files: vec![BundleFile {
                path: "langs/group-acorn/json/def/grammar/grammar.js".to_owned(),
                text: "module.exports = grammar({ name: 'json', rules: { document: _ => '' } });"
                    .to_owned(),
            }],
            input: String::new(),
            run_corpus: false,
        };
        let response =
            playground_response(&facet_json::to_string(&request).expect("request serializes"));

        assert!(!response.ok);
        assert_eq!(
            response.bundle.grammar_js_path.as_deref(),
            Some("grammar.js")
        );
        assert!(
            response.diagnostics[0]
                .message
                .contains("playground shell should evaluate grammar.js")
        );
    }

    #[test]
    fn uses_source_matched_css_scanner_for_runtime_parse() {
        let files = vec![
            BundleFile {
                path: "src/grammar.json".to_owned(),
                text: include_str!(
                    "../../snark/tests/fixtures/packages/tree-sitter-css-reduced/src/grammar.json"
                )
                .to_owned(),
            },
            BundleFile {
                path: "src/scanner.c".to_owned(),
                text: snark_scanner_host::CSS_SCANNER_SOURCE.to_owned(),
            },
            BundleFile {
                path: "queries/highlights.scm".to_owned(),
                text: include_str!(
                    "../../snark/tests/fixtures/packages/tree-sitter-css-reduced/queries/highlights.scm"
                )
                .to_owned(),
            },
        ];

        let request = PlaygroundRequest {
            files: files.clone(),
            input: "a:hover { color: red; }\n".to_owned(),
            run_corpus: false,
        };

        let response =
            playground_response(&facet_json::to_string(&request).expect("request serializes"));

        assert!(response.ok, "{:?}", response.diagnostics);
        assert_eq!(response.language.as_deref(), Some("css"));
        assert_eq!(
            response.bundle.active_scanner.as_deref(),
            Some("built-in compiled reduced CSS scanner matched from src/scanner.c")
        );
        assert_eq!(
            response.parse.as_ref().map(|parse| parse.sexp.as_str()),
            Some(
                "(stylesheet (rule_set (selectors (pseudo_class_selector (tag_name) (class_name (identifier)))) (block (declaration (property_name) (plain_value)))))"
            )
        );
        assert!(
            response
                .highlights
                .iter()
                .any(|capture| capture.capture_name == "punctuation.delimiter"
                    && capture.text == ":")
        );

        let request = PlaygroundRequest {
            files,
            input: "@namespace svg url(http://www.w3.org/1999/xhtml);\n".to_owned(),
            run_corpus: false,
        };
        let response =
            playground_response(&facet_json::to_string(&request).expect("request serializes"));

        assert!(response.ok, "{:?}", response.diagnostics);
        assert_eq!(
            response.parse.as_ref().map(|parse| parse.sexp.as_str()),
            Some(
                "(stylesheet (namespace_statement (namespace_name) (call_expression (function_name) (arguments (plain_value)))))"
            )
        );
    }
}
