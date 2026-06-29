#![forbid(unsafe_code)]
//! Native playground backend for Snark grammar bundles.

use std::{cell::RefCell, collections::BTreeSet};

use facet::Facet;
use margin::{
    Annotation, AnnotationRole, Diagnostics as MarginDiagnostics, LayoutOptions, Report, Severity,
    Source, SourceId, Span, plan,
};
use snark::{
    corpus::{CorpusSource, HighlightAssertion},
    grammar::RawGrammarJson,
    lexical::LexicalFacts,
    parser::{
        ExternalId, ParseTable, ParserGrammar, ReducedExternalScan, ReducedExternalScanResult,
        ReducedExternalScanner, ReducedParseError, ReducedParseErrorKind, RuntimeParseReport,
        RuntimeParser, RuntimeParserPlan, ScannerSnapshotId, TreeEvent,
    },
    query::QuerySource,
    runtime_input::{PointBytes, PointRange, Row, Utf8ColumnBytes},
    validated::ValidatedGrammar,
};
#[derive(Debug, Clone, Facet)]
struct PlaygroundRequest {
    files: Vec<BundleFile>,
    input: String,
    run_corpus: bool,
}

#[derive(Debug, Clone, Facet)]
struct PlaygroundSessionRequest {
    files: Vec<BundleFile>,
}

#[derive(Debug, Clone, Facet)]
struct PlaygroundParseRequest {
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
    highlight_tests: Vec<HighlightTestOutput>,
    tests: TestSummary,
    limitations: Vec<String>,
}

#[derive(Debug, Clone, Facet)]
struct Diagnostic {
    stage: String,
    message: String,
    primary_span: Option<DiagnosticSpan>,
}

#[derive(Debug, Clone, Facet)]
struct DiagnosticSpan {
    start_byte: u32,
    end_byte: u32,
    start_row: u32,
    start_column: u32,
    end_row: u32,
    end_column: u32,
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
    accepted_error_count: usize,
    accepted_missing_count: usize,
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

#[derive(Debug, Clone, Facet)]
struct TestSummary {
    requested: bool,
    corpus_passed: usize,
    corpus_failed: usize,
    highlight_assertions_passed: usize,
    highlight_assertions_failed: usize,
    highlight_fixture_errors: usize,
}

impl TestSummary {
    fn not_requested() -> Self {
        Self {
            requested: false,
            corpus_passed: 0,
            corpus_failed: 0,
            highlight_assertions_passed: 0,
            highlight_assertions_failed: 0,
            highlight_fixture_errors: 0,
        }
    }

    fn from_results(
        requested: bool,
        corpus: &[CorpusOutput],
        highlight_tests: &[HighlightTestOutput],
    ) -> Self {
        Self {
            requested,
            corpus_passed: corpus.iter().filter(|case| case.passed).count(),
            corpus_failed: corpus.iter().filter(|case| !case.passed).count(),
            highlight_assertions_passed: highlight_tests
                .iter()
                .map(|fixture| fixture.passed_count)
                .sum(),
            highlight_assertions_failed: highlight_tests
                .iter()
                .map(|fixture| fixture.failed_count)
                .sum(),
            highlight_fixture_errors: highlight_tests
                .iter()
                .filter(|fixture| fixture.error.is_some())
                .count(),
        }
    }
}

#[derive(Debug, Clone, Facet)]
struct HighlightTestOutput {
    path: String,
    passed: bool,
    input: String,
    assertion_count: usize,
    passed_count: usize,
    failed_count: usize,
    assertions: Vec<HighlightAssertionOutput>,
    error: Option<String>,
}

#[derive(Debug, Clone, Facet)]
struct HighlightAssertionOutput {
    capture_name: String,
    negative: bool,
    passed: bool,
    row: u32,
    column: u32,
    length: u32,
    observed_captures: Vec<String>,
    message: Option<String>,
}

struct PreparedGrammar {
    raw: RawGrammarJson,
    validated: ValidatedGrammar,
    parser: ParserGrammar,
    table: ParseTable,
    runtime_plan: RuntimeParserPlan,
}

impl PreparedGrammar {
    fn runtime(&self) -> Result<RuntimeParser<'_>, ReducedParseError> {
        RuntimeParser::new_with_plan(
            &self.validated,
            &self.parser,
            &self.table,
            &self.runtime_plan,
        )
    }
}

struct ScannerSelection {
    scanner: Option<CssBundleExternalScanner>,
    active_scanner: Option<String>,
}

const PLAYGROUND_RECOVERY_STEP_LIMIT: usize = 50_000;

/// Parse one playground request with Snark and return a JSON response.
pub fn parse_bundle_json(request_json: &str) -> String {
    response_json(playground_response(request_json))
}

/// Prepared playground session that can parse many inputs for one grammar bundle.
pub struct PlaygroundSession {
    files: Vec<BundleFile>,
    bundle: BundleSummary,
    prepared: PreparedGrammar,
}

impl PlaygroundSession {
    /// Prepare one grammar bundle for repeated parsing.
    pub fn prepare_json(request_json: &str) -> Result<Self, String> {
        let request = facet_json::from_str::<PlaygroundSessionRequest>(request_json)
            .map_err(|error| format!("could not decode playground session JSON: {error}"))?;
        Self::prepare_files(request.files).map_err(|response| {
            response
                .diagnostics
                .first()
                .map(|diagnostic| diagnostic.message.clone())
                .unwrap_or_else(|| "could not prepare playground session".to_owned())
        })
    }

    /// Parse one input with this prepared session and return a JSON response.
    pub fn parse_json(&self, request_json: &str) -> String {
        let request = match facet_json::from_str::<PlaygroundParseRequest>(request_json) {
            Ok(request) => request,
            Err(error) => {
                return response_json(response_with_diagnostic(
                    "request",
                    format!("could not decode playground parse JSON: {error}"),
                ));
            }
        };
        response_json(self.response(&request.input, request.run_corpus))
    }

    fn prepare_files(files: Vec<BundleFile>) -> Result<Self, PlaygroundResponse> {
        let files = normalize_bundle_files(files);
        let mut bundle = summarize_bundle(&files);
        let Some(grammar_file) = find_file(&files, "src/grammar.json") else {
            let message = match &bundle.grammar_js_path {
                Some(path) => format!(
                    "bundle contains {path}, but the playground backend consumes src/grammar.json; the playground shell should evaluate grammar.js into an in-memory src/grammar.json before parsing"
                ),
                None => "bundle does not contain src/grammar.json".to_owned(),
            };
            return Err(PlaygroundResponse {
                ok: false,
                language: None,
                diagnostics: vec![Diagnostic {
                    stage: "bundle".to_owned(),
                    message,
                    primary_span: None,
                }],
                bundle,
                parse: None,
                highlights: Vec::new(),
                corpus: Vec::new(),
                highlight_tests: Vec::new(),
                tests: TestSummary::not_requested(),
                limitations: Vec::new(),
            });
        };

        let prepared = match prepare_grammar(&grammar_file.text) {
            Ok(prepared) => prepared,
            Err((stage, message)) => {
                return Err(PlaygroundResponse {
                    ok: false,
                    language: None,
                    diagnostics: vec![diagnostic(&stage, message, None)],
                    bundle,
                    parse: None,
                    highlights: Vec::new(),
                    corpus: Vec::new(),
                    highlight_tests: Vec::new(),
                    tests: TestSummary::not_requested(),
                    limitations: Vec::new(),
                });
            }
        };

        let scanner_selection = scanner_selection(&files, &prepared.parser);
        bundle.active_scanner = scanner_selection.active_scanner;
        Ok(Self {
            files,
            bundle,
            prepared,
        })
    }

    fn response(&self, input: &str, run_corpus: bool) -> PlaygroundResponse {
        playground_response_for_session(self, input, run_corpus)
    }
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
    let input = request.input;
    let run_corpus = request.run_corpus;
    let session = match PlaygroundSession::prepare_files(request.files) {
        Ok(session) => session,
        Err(response) => return response,
    };
    session.response(&input, run_corpus)
}

fn playground_response_for_session(
    session: &PlaygroundSession,
    input: &str,
    run_corpus: bool,
) -> PlaygroundResponse {
    let files = &session.files;
    let prepared = &session.prepared;
    let bundle = session.bundle.clone();
    let mut diagnostics = Vec::new();
    let mut parse = None;
    let mut highlights = Vec::new();
    let scanner_selection = scanner_selection(files, &prepared.parser);
    let runtime = prepared.runtime();
    let should_parse_input = !input.is_empty() || !run_corpus;
    if should_parse_input {
        match runtime {
            Ok(runtime) => match parse_source_with_optional_recovery(
                runtime,
                scanner_selection
                    .scanner
                    .as_ref()
                    .map(|scanner| scanner as &dyn ReducedExternalScanner),
                input,
            ) {
                Ok(playground_report) => {
                    let report = playground_report.report;
                    let accepted_tree_events = report.accepted_tree_events();
                    let accepted_tree_event_count = accepted_tree_events.len();
                    let accepted_error_count = count_accepted_errors(&accepted_tree_events);
                    let accepted_missing_count = count_accepted_missing(&accepted_tree_events);
                    parse = Some(ParseOutput {
                        sexp: report.tree().to_sexp(),
                        accepted_count: report.accepted_count(),
                        failure_count: report.failure_count(),
                        max_live_versions: report.max_live_versions(),
                        trace_event_count: report.trace_events().len(),
                        tree_event_count: report.tree_events().len(),
                        accepted_tree_event_count,
                        accepted_error_count,
                        accepted_missing_count,
                    });
                    if accepted_error_count > 0 || accepted_missing_count > 0 {
                        diagnostics.push(diagnostic(
                            "parse",
                            format!(
                                "accepted parse contains {accepted_error_count} ERROR node(s) and {accepted_missing_count} MISSING node(s)"
                            ),
                            playground_report
                                .strict_error
                                .as_ref()
                                .and_then(|error| reduced_error_byte(error))
                                .and_then(|byte| diagnostic_span(input, byte))
                                .or_else(|| accepted_problem_span(&accepted_tree_events, input)),
                        ));
                    }
                    if let Some(query_file) = find_file(files, "queries/highlights.scm") {
                        highlights = highlight_outputs(
                            &QuerySource(query_file.text.clone()),
                            &prepared.parser,
                            &report,
                            input,
                        );
                    }
                }
                Err(error) => diagnostics.push(reduced_error_diagnostic("parse", &error, input)),
            },
            Err(error) => diagnostics.push(diagnostic("runtime", error.to_string(), None)),
        }
    }

    let corpus = if run_corpus {
        run_corpus_cases(files, prepared)
    } else {
        Vec::new()
    };
    let highlight_tests = if run_corpus {
        run_highlight_tests(files, prepared)
    } else {
        Vec::new()
    };
    let tests = TestSummary::from_results(run_corpus, &corpus, &highlight_tests);

    PlaygroundResponse {
        ok: diagnostics.is_empty(),
        language: Some(prepared.raw.name.clone()),
        diagnostics,
        bundle,
        parse,
        highlights,
        corpus,
        highlight_tests,
        tests,
        limitations: Vec::new(),
    }
}

fn count_accepted_errors(events: &[TreeEvent]) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, TreeEvent::Error { .. }))
        .count()
}

fn count_accepted_missing(events: &[TreeEvent]) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, TreeEvent::Missing { .. }))
        .count()
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
    let runtime_plan = RuntimeParserPlan::new(&validated, &parser, &table)
        .map_err(|error| ("runtime".to_owned(), error.to_string()))?;
    Ok(PreparedGrammar {
        raw,
        validated,
        parser,
        table,
        runtime_plan,
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
            let runtime = match prepared.runtime() {
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
            match parse_strict_with_optional_scanner(
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

fn run_highlight_tests(
    files: &[BundleFile],
    prepared: &PreparedGrammar,
) -> Vec<HighlightTestOutput> {
    let mut results = Vec::new();
    let scanner_selection = scanner_selection(files, &prepared.parser);
    let query =
        find_file(files, "queries/highlights.scm").map(|file| QuerySource(file.text.clone()));

    for file in files
        .iter()
        .filter(|file| is_highlight_fixture_path(&file.path))
    {
        let Some(query) = query.as_ref() else {
            results.push(highlight_test_error(
                file,
                "bundle contains highlight fixtures but no queries/highlights.scm".to_owned(),
            ));
            continue;
        };
        let assertions = match CorpusSource(file.text.clone()).parse_css_highlight_assertions() {
            Ok(assertions) => assertions,
            Err(error) => {
                results.push(highlight_test_error(file, error.to_string()));
                continue;
            }
        };
        let runtime = match prepared.runtime() {
            Ok(runtime) => runtime,
            Err(error) => {
                results.push(highlight_test_error(file, error.to_string()));
                continue;
            }
        };
        let report = match parse_strict_with_optional_scanner(
            runtime,
            scanner_selection
                .scanner
                .as_ref()
                .map(|scanner| scanner as &dyn ReducedExternalScanner),
            &file.text,
        ) {
            Ok(report) => report,
            Err(error) => {
                results.push(highlight_test_error(file, error.to_string()));
                continue;
            }
        };
        let captures = query.execute_runtime_highlights(&prepared.parser, &report, &file.text);
        let assertion_outputs = assertions
            .iter()
            .map(|assertion| highlight_assertion_output(assertion, &captures))
            .collect::<Vec<_>>();
        let passed_count = assertion_outputs
            .iter()
            .filter(|assertion| assertion.passed)
            .count();
        let failed_count = assertion_outputs.len() - passed_count;
        results.push(HighlightTestOutput {
            path: file.path.clone(),
            passed: failed_count == 0,
            input: file.text.clone(),
            assertion_count: assertion_outputs.len(),
            passed_count,
            failed_count,
            assertions: assertion_outputs,
            error: None,
        });
    }

    results
}

fn highlight_test_error(file: &BundleFile, error: String) -> HighlightTestOutput {
    HighlightTestOutput {
        path: file.path.clone(),
        passed: false,
        input: file.text.clone(),
        assertion_count: 0,
        passed_count: 0,
        failed_count: 0,
        assertions: Vec::new(),
        error: Some(error),
    }
}

fn highlight_assertion_output(
    assertion: &HighlightAssertion,
    captures: &[snark::query::HighlightCapture],
) -> HighlightAssertionOutput {
    let observed_captures = captures
        .iter()
        .filter(|capture| capture_matches_highlight_assertion(capture.points(), assertion))
        .map(|capture| format!("@{} {:?}", capture.capture_name(), capture.text()))
        .collect::<Vec<_>>();
    let matched = captures.iter().any(|capture| {
        capture.capture_name() == assertion.expected_capture_name
            && capture_matches_highlight_assertion(capture.points(), assertion)
    });
    let passed = if assertion.negative {
        !matched
    } else {
        matched
    };
    let message = if passed {
        None
    } else if assertion.negative {
        Some(format!(
            "unexpected capture @{} covered assertion range",
            assertion.expected_capture_name
        ))
    } else {
        Some(format!(
            "missing capture @{} covering assertion range",
            assertion.expected_capture_name
        ))
    };

    HighlightAssertionOutput {
        capture_name: assertion.expected_capture_name.clone(),
        negative: assertion.negative,
        passed,
        row: u32::try_from(assertion.position.row).expect("highlight row fits in u32"),
        column: u32::try_from(assertion.position.column).expect("highlight column fits in u32"),
        length: u32::try_from(assertion.length).expect("highlight length fits in u32"),
        observed_captures,
        message,
    }
}

fn capture_matches_highlight_assertion(points: PointRange, assertion: &HighlightAssertion) -> bool {
    let assertion = highlight_assertion_range(assertion);
    points.start() <= assertion.start() && assertion.end() <= points.end()
}

fn highlight_assertion_range(assertion: &HighlightAssertion) -> PointRange {
    let start = PointBytes::new(
        Row::new(u32::try_from(assertion.position.row).expect("highlight row fits in u32")),
        Utf8ColumnBytes::new(
            u32::try_from(assertion.position.column).expect("highlight column fits in u32"),
        ),
    );
    let end = PointBytes::new(
        Row::new(u32::try_from(assertion.position.row).expect("highlight row fits in u32")),
        Utf8ColumnBytes::new(
            u32::try_from(assertion.position.column + assertion.length)
                .expect("highlight end column fits in u32"),
        ),
    );
    PointRange::new(start, end).expect("highlight assertion range is not reversed")
}

fn parse_strict_with_optional_scanner<'a>(
    runtime: RuntimeParser<'a>,
    scanner: Option<&'a dyn ReducedExternalScanner>,
    input: &str,
) -> Result<RuntimeParseReport, ReducedParseError> {
    let runtime = match scanner {
        Some(scanner) => runtime.with_external_scanner(scanner),
        None => runtime,
    };
    runtime.parse_compact_with_report(input)
}

fn parse_source_with_optional_recovery<'a>(
    runtime: RuntimeParser<'a>,
    scanner: Option<&'a dyn ReducedExternalScanner>,
    input: &str,
) -> Result<PlaygroundParseReport, ReducedParseError> {
    let runtime = match scanner {
        Some(scanner) => runtime.with_external_scanner(scanner),
        None => runtime,
    }
    .with_recovery_step_limit(PLAYGROUND_RECOVERY_STEP_LIMIT);
    match runtime.parse_compact_with_report(input) {
        Ok(report) => Ok(PlaygroundParseReport {
            report,
            strict_error: None,
        }),
        Err(strict_error) => match runtime.parse_recovering_compact_with_report(input) {
            Ok(report) => Ok(PlaygroundParseReport {
                report,
                strict_error: Some(strict_error),
            }),
            Err(_) => Err(strict_error),
        },
    }
}

struct PlaygroundParseReport {
    report: RuntimeParseReport,
    strict_error: Option<ReducedParseError>,
}

fn diagnostic(stage: &str, message: String, primary_span: Option<DiagnosticSpan>) -> Diagnostic {
    Diagnostic {
        stage: stage.to_owned(),
        message,
        primary_span,
    }
}

fn accepted_problem_span(events: &[TreeEvent], input: &str) -> Option<DiagnosticSpan> {
    events.iter().find_map(|event| match event {
        TreeEvent::Error { bytes, .. } | TreeEvent::Missing { bytes, .. } => {
            diagnostic_span(input, bytes.start().get() as usize)
        }
        _ => None,
    })
}

fn reduced_error_diagnostic(stage: &str, error: &ReducedParseError, input: &str) -> Diagnostic {
    let span = reduced_error_byte(error).and_then(|byte| diagnostic_span(input, byte));
    diagnostic(stage, error.to_string(), span)
}

fn reduced_error_byte(error: &ReducedParseError) -> Option<usize> {
    match error.kind() {
        ReducedParseErrorKind::NoToken { byte_position, .. }
        | ReducedParseErrorKind::NoAction { byte_position, .. }
        | ReducedParseErrorKind::TrailingInput { byte_position } => Some(*byte_position),
        _ => error.trace().last().map(|step| step.byte_position),
    }
}

fn diagnostic_span(input: &str, byte: usize) -> Option<DiagnosticSpan> {
    let start = char_boundary_at_or_before(input, byte.min(input.len()));
    let end = next_char_boundary(input, start);
    let source_id = SourceId("source".to_owned());
    let source = Source {
        id: source_id.clone(),
        name: "source".to_owned(),
        hyperlink: None,
        text: input.to_owned(),
    };
    let diagnostics = MarginDiagnostics {
        sources: vec![source],
        reports: vec![Report {
            severity: Severity::Error,
            title: "parse diagnostic".to_owned(),
            annotations: vec![Annotation {
                spans: vec![Span::new(source_id.0.as_str(), start, end)],
                role: AnnotationRole::PrimaryLabel,
                syntax_class: None,
                message: None,
                priority: 100,
            }],
            notes: Vec::new(),
            sections: Vec::new(),
        }],
    };
    let plan = plan(&diagnostics, &LayoutOptions::default()).ok()?;
    let segment = plan
        .reports
        .first()?
        .windows
        .first()?
        .annotations
        .first()?
        .segments
        .first()?;
    Some(DiagnosticSpan {
        start_byte: usize_to_u32(start)?,
        end_byte: usize_to_u32(end)?,
        start_row: usize_to_u32(segment.line_number.checked_sub(1)?)?,
        start_column: usize_to_u32(segment.start_column)?,
        end_row: usize_to_u32(segment.line_number.checked_sub(1)?)?,
        end_column: usize_to_u32(segment.end_column)?,
    })
}

fn char_boundary_at_or_before(input: &str, mut byte: usize) -> usize {
    while byte > 0 && !input.is_char_boundary(byte) {
        byte -= 1;
    }
    byte
}

fn next_char_boundary(input: &str, start: usize) -> usize {
    if start >= input.len() {
        return start;
    }
    input[start..]
        .chars()
        .next()
        .map_or(start, |ch| start + ch.len_utf8())
}

fn usize_to_u32(value: usize) -> Option<u32> {
    u32::try_from(value).ok()
}

fn response_with_diagnostic(stage: &str, message: String) -> PlaygroundResponse {
    PlaygroundResponse {
        ok: false,
        language: None,
        diagnostics: vec![diagnostic(stage, message, None)],
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
        highlight_tests: Vec::new(),
        tests: TestSummary::not_requested(),
        limitations: Vec::new(),
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

fn normalize_bundle_files(files: Vec<BundleFile>) -> Vec<BundleFile> {
    let normalized_paths = files
        .iter()
        .map(|file| normalize_path(&file.path))
        .collect::<Vec<_>>();
    let context = NormalizationContext::from_paths(&normalized_paths);

    files
        .into_iter()
        .zip(normalized_paths)
        .map(|(file, normalized_path)| BundleFile {
            path: normalize_bundle_path(&normalized_path, &context),
            text: file.text,
        })
        .collect()
}

fn find_file<'a>(files: &'a [BundleFile], path: &str) -> Option<&'a BundleFile> {
    files.iter().find(|file| file.path == path)
}

fn is_highlight_fixture_path(path: &str) -> bool {
    path.starts_with("test/highlight/") || path.starts_with("test/highlights/")
}

#[derive(Debug)]
struct NormalizationContext {
    arborium_roots: BTreeSet<String>,
    package_roots: BTreeSet<String>,
}

impl NormalizationContext {
    fn from_paths(paths: &[String]) -> Self {
        Self {
            arborium_roots: paths
                .iter()
                .filter_map(|path| arborium_root(path))
                .collect(),
            package_roots: paths.iter().filter_map(|path| package_root(path)).collect(),
        }
    }
}

fn normalize_bundle_path(path: &str, context: &NormalizationContext) -> String {
    let path = normalize_path(path);
    if let Some(relative) = arborium_def_relative(&path, context) {
        if let Some(mapped) = normalize_arborium_def_path(relative) {
            return mapped;
        }
    }
    if is_ambiguous_arborium_def_path(&path, context) {
        return path;
    }
    if let Some(relative) = package_root_relative(&path, context) {
        if let Some(mapped) = normalize_package_path(relative) {
            return mapped;
        }
    }
    if is_ambiguous_package_path(&path, context) {
        return path;
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

fn arborium_root(path: &str) -> Option<String> {
    if path.starts_with("def/grammar/grammar.json")
        || path.starts_with("def/grammar/src/grammar.json")
        || path.starts_with("def/grammar/grammar.js")
    {
        return Some(String::new());
    }

    for marker in [
        "/def/grammar/grammar.json",
        "/def/grammar/src/grammar.json",
        "/def/grammar/grammar.js",
    ] {
        if let Some(index) = path.find(marker) {
            return Some(path[..index].to_owned());
        }
    }
    None
}

fn package_root(path: &str) -> Option<String> {
    if matches!(path, "grammar.js" | "grammar.json" | "src/grammar.json") {
        return Some(String::new());
    }
    if path.ends_with("/def/grammar/grammar.json")
        || path.ends_with("/def/grammar/grammar.js")
        || path.ends_with("/def/grammar/src/grammar.json")
    {
        return None;
    }
    if let Some(root) = path.strip_suffix("/grammar.js") {
        return Some(root.to_owned());
    }
    if let Some(root) = path.strip_suffix("/src/grammar.json") {
        return Some(root.to_owned());
    }
    if let Some(root) = path.strip_suffix("/grammar.json") {
        return Some(root.to_owned());
    }
    None
}

fn arborium_def_relative<'a>(path: &'a str, context: &NormalizationContext) -> Option<&'a str> {
    if let Some(relative) = path.strip_prefix("def/") {
        return Some(relative);
    }

    let marker = "/def/";
    let index = path.find(marker)?;
    if context.arborium_roots.len() != 1 {
        return None;
    }

    Some(&path[index + marker.len()..])
}

fn is_ambiguous_arborium_def_path(path: &str, context: &NormalizationContext) -> bool {
    path.contains("/def/") && context.arborium_roots.len() != 1
}

fn package_root_relative<'a>(path: &'a str, context: &NormalizationContext) -> Option<&'a str> {
    if context.package_roots.len() != 1 {
        return None;
    }
    let root = context.package_roots.iter().next()?;
    if root.is_empty() {
        return None;
    }
    path.strip_prefix(&format!("{root}/"))
}

fn is_ambiguous_package_path(path: &str, context: &NormalizationContext) -> bool {
    context.package_roots.len() > 1
        && context
            .package_roots
            .iter()
            .any(|root| !root.is_empty() && path.starts_with(&format!("{root}/")))
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
        "grammar.json" => return Some("src/grammar.json".to_owned()),
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
        "/grammar.json",
        "/src/scanner.c",
        "/src/scanner.cc",
        "/src/parser.c",
        "/src/parser.cc",
        "/src/parser.h",
        "/src/node-types.json",
        "/bindings/node/binding.cc",
    ] {
        if path.ends_with(suffix) {
            if suffix == "/grammar.json" {
                return Some("src/grammar.json".to_owned());
            }
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
    fn normalizes_arborium_def_grammar_json_paths() {
        let files = normalize_bundle_files(vec![
            BundleFile {
                path: "langs/group-acorn/css/def/grammar/grammar.json".to_owned(),
                text: "{}".to_owned(),
            },
            BundleFile {
                path: "langs/group-acorn/css/def/queries/highlights.scm".to_owned(),
                text: String::new(),
            },
            BundleFile {
                path: "langs/group-acorn/css/def/sample.css".to_owned(),
                text: String::new(),
            },
        ]);

        let summary = summarize_bundle(&files);
        assert_eq!(summary.grammar_path.as_deref(), Some("src/grammar.json"));
        assert_eq!(summary.query_paths, vec!["queries/highlights.scm"]);
        assert_eq!(summary.sample_paths, vec!["samples/sample.css"]);
    }

    #[test]
    fn normalizes_package_root_grammar_json_paths() {
        let files = normalize_bundle_files(vec![
            BundleFile {
                path: "tree-sitter-nginx/grammar.json".to_owned(),
                text: "{}".to_owned(),
            },
            BundleFile {
                path: "tree-sitter-nginx/queries/highlights.scm".to_owned(),
                text: String::new(),
            },
            BundleFile {
                path: "tree-sitter-nginx/samples/nginx.conf".to_owned(),
                text: String::new(),
            },
        ]);

        let summary = summarize_bundle(&files);
        assert_eq!(summary.grammar_path.as_deref(), Some("src/grammar.json"));
        assert_eq!(summary.query_paths, vec!["queries/highlights.scm"]);
        assert_eq!(summary.sample_paths, vec!["samples/nginx.conf"]);
    }

    #[test]
    fn preserves_arborium_paths_when_parent_upload_contains_multiple_grammar_roots() {
        let files = normalize_bundle_files(vec![
            BundleFile {
                path: "langs/group-acorn/css/def/grammar/grammar.js".to_owned(),
                text: "module.exports = grammar({ name: 'css', rules: { stylesheet: _ => '' } });"
                    .to_owned(),
            },
            BundleFile {
                path: "langs/group-acorn/css/def/queries/highlights.scm".to_owned(),
                text: String::new(),
            },
            BundleFile {
                path: "langs/group-acorn/json/def/grammar/grammar.js".to_owned(),
                text: "module.exports = grammar({ name: 'json', rules: { document: _ => '' } });"
                    .to_owned(),
            },
            BundleFile {
                path: "langs/group-acorn/json/def/queries/highlights.scm".to_owned(),
                text: String::new(),
            },
        ]);

        assert!(find_file(&files, "grammar.js").is_none());
        assert!(find_file(&files, "queries/highlights.scm").is_none());
        assert!(find_file(&files, "langs/group-acorn/css/def/grammar/grammar.js").is_some());
        assert!(find_file(&files, "langs/group-acorn/json/def/grammar/grammar.js").is_some());
        assert!(find_file(&files, "langs/group-acorn/css/def/queries/highlights.scm").is_some());
        assert!(find_file(&files, "langs/group-acorn/json/def/queries/highlights.scm").is_some());
    }

    #[test]
    fn preserves_package_paths_when_parent_upload_contains_multiple_grammar_roots() {
        let files = normalize_bundle_files(vec![
            BundleFile {
                path: "packages/tree-sitter-css/src/grammar.json".to_owned(),
                text: "{}".to_owned(),
            },
            BundleFile {
                path: "packages/tree-sitter-css/queries/highlights.scm".to_owned(),
                text: String::new(),
            },
            BundleFile {
                path: "packages/tree-sitter-json/src/grammar.json".to_owned(),
                text: "{}".to_owned(),
            },
            BundleFile {
                path: "packages/tree-sitter-json/queries/highlights.scm".to_owned(),
                text: String::new(),
            },
        ]);

        assert!(find_file(&files, "src/grammar.json").is_none());
        assert!(find_file(&files, "queries/highlights.scm").is_none());
        assert!(find_file(&files, "packages/tree-sitter-css/src/grammar.json").is_some());
        assert!(find_file(&files, "packages/tree-sitter-json/src/grammar.json").is_some());
        assert!(find_file(&files, "packages/tree-sitter-css/queries/highlights.scm").is_some());
        assert!(find_file(&files, "packages/tree-sitter-json/queries/highlights.scm").is_some());
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
            BundleFile {
                path: "test/highlight/test_css.css".to_owned(),
                text: include_str!(
                    "../../snark/tests/fixtures/packages/tree-sitter-css-reduced/test/highlight/test_css.css"
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
            files: files.clone(),
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

        let request = PlaygroundRequest {
            files,
            input: "a:hover { color: red; }\n".to_owned(),
            run_corpus: true,
        };
        let response =
            playground_response(&facet_json::to_string(&request).expect("request serializes"));

        assert!(response.ok, "{:?}", response.diagnostics);
        assert_eq!(response.corpus.len(), 0);
        assert_eq!(response.highlight_tests.len(), 1);
        assert_eq!(
            response.highlight_tests[0].path,
            "test/highlight/test_css.css"
        );
        assert_eq!(response.highlight_tests[0].assertion_count, 37);
        assert_eq!(response.highlight_tests[0].failed_count, 0);
        assert_eq!(response.highlight_tests[0].passed_count, 37);
        assert!(response.tests.requested);
        assert_eq!(response.tests.corpus_passed, 0);
        assert_eq!(response.tests.corpus_failed, 0);
        assert_eq!(response.tests.highlight_assertions_passed, 37);
        assert_eq!(response.tests.highlight_assertions_failed, 0);
        assert_eq!(response.tests.highlight_fixture_errors, 0);

        let request = PlaygroundRequest {
            files: vec![
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
                BundleFile {
                    path: "test/highlight/test_css.css".to_owned(),
                    text: include_str!(
                        "../../snark/tests/fixtures/packages/tree-sitter-css-reduced/test/highlight/test_css.css"
                    )
                    .to_owned(),
                },
            ],
            input: String::new(),
            run_corpus: true,
        };
        let response =
            playground_response(&facet_json::to_string(&request).expect("request serializes"));

        assert!(response.ok, "{:?}", response.diagnostics);
        assert!(response.parse.is_none());
        assert_eq!(response.highlight_tests.len(), 1);
        assert_eq!(response.tests.highlight_assertions_passed, 37);
        assert_eq!(response.tests.highlight_assertions_failed, 0);
        assert_eq!(response.tests.highlight_fixture_errors, 0);
    }

    #[test]
    fn parses_nginx_shaped_pattern_bundle_through_playground_response() {
        let request = PlaygroundRequest {
            files: vec![
                BundleFile {
                    path: "src/grammar.json".to_owned(),
                    text: r##"{
  "$schema": "https://tree-sitter.github.io/tree-sitter/assets/schemas/grammar.schema.json",
  "name": "nginx_smoke",
  "rules": {
    "conf": {
      "type": "REPEAT",
      "content": { "type": "SYMBOL", "name": "directive" }
    },
    "comment": {
      "type": "TOKEN",
      "content": {
        "type": "PATTERN",
        "value": "#.*\\n"
      }
    },
    "directive": {
      "type": "SEQ",
      "members": [
        { "type": "SYMBOL", "name": "identifier" },
        {
          "type": "REPEAT",
          "content": { "type": "SYMBOL", "name": "argument" }
        },
        { "type": "STRING", "value": ";" }
      ]
    },
    "argument": {
      "type": "CHOICE",
      "members": [
        { "type": "SYMBOL", "name": "identifier" },
        { "type": "SYMBOL", "name": "number" },
        { "type": "SYMBOL", "name": "generic" }
      ]
    },
    "identifier": {
      "type": "PATTERN",
      "value": "\\w+"
    },
    "number": {
      "type": "PATTERN",
      "value": "\\d+"
    },
    "generic": {
      "type": "PATTERN",
      "value": "[\\w/\\-\\.]*[A-Za-z][\\w/\\-=,?]+"
    }
  },
  "extras": [
    { "type": "SYMBOL", "name": "comment" },
    { "type": "PATTERN", "value": "[\\s\\p{Zs}\\uFEFF\\u2060\\u200B]" }
  ],
  "conflicts": [],
  "precedences": [],
  "externals": [],
  "inline": [],
  "supertypes": []
}"##
                    .to_owned(),
                },
                BundleFile {
                    path: "queries/highlights.scm".to_owned(),
                    text: "(identifier) @variable\n(generic) @string\n(comment) @comment\n"
                        .to_owned(),
                },
            ],
            input:
                "# https://nginx.org/en/docs/ngx_core_module.html#user\nuser www-data;\npid /var/run/nginx.pid;\n"
                    .to_owned(),
            run_corpus: false,
        };

        let response =
            playground_response(&facet_json::to_string(&request).expect("request serializes"));

        assert!(response.ok, "{:?}", response.diagnostics);
        assert_eq!(response.language.as_deref(), Some("nginx_smoke"));
        assert_eq!(
            response.parse.as_ref().map(|parse| parse.sexp.as_str()),
            Some(
                "(conf (comment) (directive (identifier) (argument (generic))) (directive (identifier) (argument (generic))))"
            )
        );
        let parse = response.parse.as_ref().expect("parse output");
        assert_eq!(parse.accepted_error_count, 0);
        assert_eq!(parse.accepted_missing_count, 0);
        let mut captures = response
            .highlights
            .iter()
            .map(|capture| {
                (
                    capture.start_byte,
                    capture.capture_name.as_str(),
                    capture.text.as_str(),
                )
            })
            .collect::<Vec<_>>();
        captures.sort();
        assert_eq!(
            captures
                .into_iter()
                .map(|(_, capture_name, text)| (capture_name, text))
                .collect::<Vec<_>>(),
            vec![
                (
                    "comment",
                    "# https://nginx.org/en/docs/ngx_core_module.html#user\n",
                ),
                ("variable", "user"),
                ("string", "www-data"),
                ("variable", "pid"),
                ("string", "/var/run/nginx.pid"),
            ]
        );
    }

    #[test]
    fn rejects_nginx_shaped_block_errors_from_grammar_js_bundles() {
        let grammar_json = r##"{
  "$schema": "https://tree-sitter.github.io/tree-sitter/assets/schemas/grammar.schema.json",
  "name": "nginx_recovery_smoke",
  "rules": {
    "conf": {
      "type": "REPEAT",
      "content": { "type": "SYMBOL", "name": "_directives" }
    },
    "comment": {
      "type": "TOKEN",
      "content": { "type": "PATTERN", "value": "#.*\\n" }
    },
    "_directives": {
      "type": "CHOICE",
      "members": [
        { "type": "SYMBOL", "name": "simple_directive" },
        { "type": "SYMBOL", "name": "block_directive" }
      ]
    },
    "directive": {
      "type": "PATTERN",
      "value": "\\w+"
    },
    "simple_directive": {
      "type": "SEQ",
      "members": [
        { "type": "FIELD", "name": "name", "content": { "type": "SYMBOL", "name": "directive" } },
        { "type": "REPEAT", "content": { "type": "SYMBOL", "name": "param" } },
        { "type": "STRING", "value": ";" }
      ]
    },
    "block_directive": {
      "type": "SEQ",
      "members": [
        { "type": "FIELD", "name": "name", "content": { "type": "SYMBOL", "name": "directive" } },
        { "type": "REPEAT", "content": { "type": "SYMBOL", "name": "param" } },
        { "type": "SYMBOL", "name": "block" }
      ]
    },
    "block": {
      "type": "SEQ",
      "members": [
        { "type": "STRING", "value": "{" },
        { "type": "REPEAT", "content": { "type": "SYMBOL", "name": "_directives" } },
        { "type": "STRING", "value": "}" }
      ]
    },
    "param": {
      "type": "CHOICE",
      "members": [
        { "type": "SYMBOL", "name": "string" },
        { "type": "SYMBOL", "name": "generic" }
      ]
    },
    "generic": {
      "type": "PATTERN",
      "value": "[\\w/\\-\\.]*[A-Za-z][\\w/\\-=,?]+"
    },
    "string_content": {
      "type": "PATTERN",
      "value": "[^\\\"]"
    },
    "string": {
      "type": "SEQ",
      "members": [
        { "type": "STRING", "value": "\"" },
        { "type": "REPEAT", "content": { "type": "SYMBOL", "name": "string_content" } },
        { "type": "STRING", "value": "\"" }
      ]
    }
  },
  "extras": [
    { "type": "SYMBOL", "name": "comment" },
    { "type": "PATTERN", "value": "[\\s\\p{Zs}\\uFEFF\\u2060\\u200B]" }
  ],
  "conflicts": [],
  "precedences": [],
  "externals": [],
  "inline": [],
  "supertypes": []
}"##;
        let request = PlaygroundRequest {
            files: vec![
                BundleFile {
                    path: "grammar.js".to_owned(),
                    text: "module.exports = grammar({ name: 'nginx_recovery_smoke', rules: {} });"
                        .to_owned(),
                },
                BundleFile {
                    path: "src/grammar.json".to_owned(),
                    text: grammar_json.to_owned(),
                },
                BundleFile {
                    path: "queries/highlights.scm".to_owned(),
                    text: "(directive) @function\n(generic) @string\n(string) @string\n"
                        .to_owned(),
                },
            ],
            input: "map type {\n  default \"ok\";\n  \"\" \"no-store\";\n  after value;\n}\nmap other {\n  \"\" \"same-origin\";\n  tail value;\n}\n".to_owned(),
            run_corpus: false,
        };

        let response =
            playground_response(&facet_json::to_string(&request).expect("request serializes"));

        assert!(!response.ok);
        assert_eq!(response.diagnostics[0].stage, "parse");
        assert!(
            response.diagnostics[0]
                .message
                .contains("accepted parse contains")
        );
        let parse = response.parse.as_ref().expect("recovered parse output");
        assert!(parse.accepted_error_count > 0);
        assert_eq!(parse.accepted_missing_count, 0);
        assert!(parse.sexp.contains("(ERROR"));
        assert!(!response.highlights.is_empty());
    }

    #[test]
    fn arborium_nginx_sample_reports_parse_failure() {
        let def = std::path::Path::new("/Users/amos/oss/arborium/langs/group-maple/nginx/def");
        if !def.exists() {
            return;
        }

        let grammar_js = std::fs::read_to_string(def.join("grammar/grammar.js"))
            .expect("nginx grammar.js should be readable");
        let grammar_json = snark_dsl::emit_with_boa(&def.join("grammar/grammar.js"))
            .expect("nginx grammar.js should emit grammar JSON");
        let highlights = std::fs::read_to_string(def.join("queries/highlights.scm"))
            .expect("nginx highlights should be readable");
        let sample = std::fs::read_to_string(def.join("samples/nginx.conf"))
            .expect("nginx sample should be readable");

        let request = PlaygroundRequest {
            files: vec![
                BundleFile {
                    path: "grammar.js".to_owned(),
                    text: grammar_js,
                },
                BundleFile {
                    path: "src/grammar.json".to_owned(),
                    text: grammar_json,
                },
                BundleFile {
                    path: "queries/highlights.scm".to_owned(),
                    text: highlights,
                },
            ],
            input: sample,
            run_corpus: false,
        };

        let response =
            playground_response(&facet_json::to_string(&request).expect("request serializes"));

        assert!(!response.ok);
        assert_eq!(response.language.as_deref(), Some("nginx"));
        assert_eq!(response.diagnostics[0].stage, "parse");
        assert!(
            response.diagnostics[0]
                .message
                .contains("could not lex a token")
        );
        assert_eq!(
            response
                .diagnostics
                .first()
                .and_then(|diagnostic| diagnostic.primary_span.as_ref())
                .map(|span| (span.start_row, span.start_column)),
            Some((110, 4))
        );
        assert!(response.parse.is_none());
        assert!(response.highlights.is_empty());
    }

    #[test]
    fn parse_errors_include_margin_resolved_primary_span() {
        let input = "ok\n#";
        let span = diagnostic_span(input, 3).expect("diagnostic byte resolves through margin");
        assert_eq!(span.start_byte, 3);
        assert_eq!(span.end_byte, 4);
        assert_eq!(span.start_row, 1);
        assert_eq!(span.start_column, 0);
        assert_eq!(span.end_row, 1);
        assert_eq!(span.end_column, 1);

        let request = PlaygroundRequest {
            files: vec![BundleFile {
                path: "src/grammar.json".to_owned(),
                text: r#"{
  "$schema": "https://tree-sitter.github.io/tree-sitter/assets/schemas/grammar.schema.json",
  "name": "diagnostic_smoke",
  "rules": {
    "document": {
      "type": "REPEAT1",
      "content": { "type": "SYMBOL", "name": "word" }
    },
    "word": {
      "type": "PATTERN",
      "value": "\\w+"
    }
  },
  "extras": [{ "type": "PATTERN", "value": "\\s" }],
  "conflicts": [],
  "precedences": [],
  "externals": [],
  "inline": [],
  "supertypes": []
}"#
                .to_owned(),
            }],
            input: input.to_owned(),
            run_corpus: false,
        };

        let response =
            playground_response(&facet_json::to_string(&request).expect("request serializes"));

        assert!(!response.ok);
        assert_eq!(response.diagnostics[0].stage, "parse");
        assert_eq!(
            response.diagnostics[0].primary_span.as_ref().map(|span| (
                span.start_byte,
                span.end_byte,
                span.start_row,
                span.start_column,
                span.end_row,
                span.end_column,
            )),
            Some((3, 4, 1, 0, 1, 1))
        );
    }
}
