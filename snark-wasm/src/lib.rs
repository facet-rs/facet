#![forbid(unsafe_code)]
//! WebAssembly bindings for Snark playgrounds.

use facet::Facet;
use snark::{
    corpus::CorpusSource,
    grammar::RawGrammarJson,
    lexical::LexicalFacts,
    parser::{ParseTable, ParserGrammar, RuntimeParser},
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
    generated_files_ignored: Vec<String>,
    scanner_paths: Vec<String>,
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
    let bundle = summarize_bundle(&files);
    let Some(grammar_file) = find_file(&files, "src/grammar.json") else {
        let message = match &bundle.grammar_js_path {
            Some(path) => format!(
                "bundle contains {path}, but Snark's browser runtime consumes src/grammar.json; convert grammar.js with the snark-wasm Node converter and reload the emitted bundle"
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
            limitations: limitations(&files),
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
                limitations: limitations(&files),
            };
        }
    };

    let mut diagnostics = Vec::new();
    let mut parse = None;
    let mut highlights = Vec::new();
    let runtime = RuntimeParser::new(&prepared.validated, &prepared.parser, &prepared.table);
    match runtime {
        Ok(runtime) => match runtime.parse_with_report(&request.input) {
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
        limitations: limitations(&files),
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
            match runtime.parse_with_report(&case.input) {
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
            generated_files_ignored: Vec::new(),
            scanner_paths: Vec::new(),
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
    }
}

fn limitations(files: &[BundleFile]) -> Vec<String> {
    let mut limitations = vec![
        "This playground executes Snark's current RuntimeParser path and returns its corpus-normalized S-expression projection.".to_owned(),
        "Generated Tree-sitter files such as src/parser.c and src/node-types.json are ignored.".to_owned(),
        "Recovery, incremental reuse, and complete Tree-sitter query semantics are not implemented in this runtime slice.".to_owned(),
    ];
    if find_file(files, "grammar.js").is_some() {
        limitations.push(
            "Tree-sitter grammar.js is source DSL. Convert it to src/grammar.json outside the browser with the snark-wasm Node converter before parsing.".to_owned(),
        );
    }
    if files
        .iter()
        .any(|file| file.path == "src/scanner.c" || file.path == "src/scanner.cc")
    {
        limitations.push(
            "Uploaded scanner.c/scanner.cc sources are reported, but the browser runtime does not compile or execute external scanners yet.".to_owned(),
        );
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
                path: "langs/group-acorn/css/def/grammar/src/node-types.json".to_owned(),
                text: String::new(),
            },
        ]);

        let summary = summarize_bundle(&files);
        assert_eq!(summary.grammar_js_path.as_deref(), Some("grammar.js"));
        assert_eq!(summary.scanner_paths, vec!["src/scanner.c"]);
        assert_eq!(summary.query_paths, vec!["queries/highlights.scm"]);
        assert!(find_file(&files, "samples/sample.css").is_some());
        assert_eq!(
            summary.generated_files_ignored,
            vec!["langs/group-acorn/css/def/grammar/src/node-types.json"]
        );
    }

    #[test]
    fn reports_grammar_js_bundle_as_needing_conversion() {
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
                .contains("convert grammar.js")
        );
    }
}
