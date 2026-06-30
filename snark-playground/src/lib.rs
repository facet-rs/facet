#![forbid(unsafe_code)]
//! Native playground backend for Snark grammar bundles.

use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
};

use facet::Facet;
use margin::{
    Annotation, AnnotationRole, Diagnostics as MarginDiagnostics, LayoutOptions, Report, Severity,
    Source, SourceId, Span, plan,
};
use regex::Regex;
use snark::{
    corpus::{CorpusSource, HighlightAssertion},
    grammar::RawGrammarJson,
    lexical::LexicalFacts,
    manifest::TreeSitterConfig,
    parser::{
        ExternalId, ParseTable, ParserGrammar, ReducedExternalScan, ReducedExternalScanResult,
        ReducedExternalScanner, ReducedParseError, ReducedParseErrorKind, RuntimeInputEdit,
        RuntimeParseReport, RuntimeParser, RuntimeParserPlan, ScannerSnapshotId, TreeEvent,
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
    edit: Option<PlaygroundInputEdit>,
}

#[derive(Debug, Clone, Copy, Facet)]
struct PlaygroundInputEdit {
    start_byte: usize,
    old_end_byte: usize,
    new_end_byte: usize,
}

impl From<PlaygroundInputEdit> for RuntimeInputEdit {
    fn from(edit: PlaygroundInputEdit) -> Self {
        RuntimeInputEdit::new(edit.start_byte, edit.old_end_byte, edit.new_end_byte)
    }
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
    injections: Vec<InjectionOutput>,
    layers: Vec<LayerOutput>,
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
    reuse_node_count: usize,
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
struct InjectionOutput {
    language: String,
    combined: bool,
    include_children: bool,
    text: String,
    start_byte: u32,
    end_byte: u32,
    start_row: u32,
    start_column: u32,
    end_row: u32,
    end_column: u32,
}

#[derive(Debug, Clone, Facet)]
struct LayerOutput {
    language: String,
    combined: bool,
    ranges: Vec<LayerSourceRange>,
    input: String,
    parse: Option<ParseOutput>,
    highlights: Vec<HighlightOutput>,
    injections: Vec<InjectionOutput>,
    layers: Vec<LayerOutput>,
    diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Facet)]
struct LayerSourceRange {
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

impl ScannerSelection {
    fn supports_required_externals(&self, parser: &ParserGrammar) -> bool {
        self.scanner.is_some() || parser.symbols().externals().is_empty()
    }
}

struct PreparedEmbeddedLanguage {
    files: Vec<BundleFile>,
    prepared: Option<PreparedGrammar>,
    scanner_selection: Option<ScannerSelection>,
    injection_regex: Option<Regex>,
    diagnostics: Vec<Diagnostic>,
}

const PLAYGROUND_RECOVERY_STEP_LIMIT: usize = 1_000_000;
const MAX_INJECTION_LAYER_DEPTH: usize = 8;

/// Parse one playground request with Snark and return a JSON response.
pub fn parse_bundle_json(request_json: &str) -> String {
    response_json(playground_response(request_json))
}

/// Prepared playground session that can parse many inputs for one grammar bundle.
pub struct PlaygroundSession {
    files: Vec<BundleFile>,
    bundle: BundleSummary,
    prepared: PreparedGrammar,
    scanner_selection: ScannerSelection,
    embedded_languages: BTreeMap<String, PreparedEmbeddedLanguage>,
    last_input: Option<String>,
    last_report: Option<RuntimeParseReport>,
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
    pub fn parse_json(&mut self, request_json: &str) -> String {
        let request = match facet_json::from_str::<PlaygroundParseRequest>(request_json) {
            Ok(request) => request,
            Err(error) => {
                return response_json(response_with_diagnostic(
                    "request",
                    format!("could not decode playground parse JSON: {error}"),
                ));
            }
        };
        let edit = request.edit.map(RuntimeInputEdit::from);
        if let (Some(edit), Some(old_input)) = (edit, self.last_input.as_deref())
            && let Err(error) = edit.validate_against(old_input, &request.input)
        {
            return response_json(self.diagnostic_response("edit", error.to_string()));
        }
        response_json(self.response(&request.input, request.run_corpus, edit))
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
                injections: Vec::new(),
                layers: Vec::new(),
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
                    injections: Vec::new(),
                    layers: Vec::new(),
                    corpus: Vec::new(),
                    highlight_tests: Vec::new(),
                    tests: TestSummary::not_requested(),
                    limitations: Vec::new(),
                });
            }
        };

        let scanner_selection = scanner_selection(&files, &prepared.parser);
        bundle
            .active_scanner
            .clone_from(&scanner_selection.active_scanner);
        if !scanner_selection.supports_required_externals(&prepared.parser) {
            return Err(PlaygroundResponse {
                ok: false,
                language: Some(prepared.raw.name.clone()),
                diagnostics: vec![diagnostic(
                    "scanner",
                    unsupported_external_scanner_message(&bundle, &prepared.parser),
                    None,
                )],
                bundle,
                parse: None,
                highlights: Vec::new(),
                injections: Vec::new(),
                layers: Vec::new(),
                corpus: Vec::new(),
                highlight_tests: Vec::new(),
                tests: TestSummary::not_requested(),
                limitations: Vec::new(),
            });
        }
        let embedded_languages = prepare_embedded_languages(&files);
        Ok(Self {
            files,
            bundle,
            prepared,
            scanner_selection,
            embedded_languages,
            last_input: None,
            last_report: None,
        })
    }

    fn response(
        &mut self,
        input: &str,
        run_corpus: bool,
        edit: Option<RuntimeInputEdit>,
    ) -> PlaygroundResponse {
        playground_response_for_session(self, input, run_corpus, edit)
    }

    fn diagnostic_response(&self, stage: &str, message: String) -> PlaygroundResponse {
        PlaygroundResponse {
            ok: false,
            language: Some(self.prepared.raw.name.clone()),
            diagnostics: vec![diagnostic(stage, message, None)],
            bundle: self.bundle.clone(),
            parse: None,
            highlights: Vec::new(),
            injections: Vec::new(),
            layers: Vec::new(),
            corpus: Vec::new(),
            highlight_tests: Vec::new(),
            tests: TestSummary::not_requested(),
            limitations: Vec::new(),
        }
    }
}

fn unsupported_external_scanner_message(bundle: &BundleSummary, parser: &ParserGrammar) -> String {
    let externals = parser
        .symbols()
        .externals()
        .iter()
        .map(|external| {
            external
                .name()
                .map_or_else(|| format!("#{}", external.ordinal()), ToOwned::to_owned)
        })
        .collect::<Vec<_>>()
        .join(", ");
    let scanner_files = if bundle.scanner_paths.is_empty() {
        "no scanner source was uploaded".to_owned()
    } else {
        format!(
            "uploaded scanner source(s): {}",
            bundle.scanner_paths.join(", ")
        )
    };
    format!(
        "grammar declares external token(s) [{externals}], but this playground can only execute the source-matched reduced CSS scanner host right now; {scanner_files}"
    )
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
    let mut session = match PlaygroundSession::prepare_files(request.files) {
        Ok(session) => session,
        Err(response) => return response,
    };
    session.response(&input, run_corpus, None)
}

fn playground_response_for_session(
    session: &mut PlaygroundSession,
    input: &str,
    run_corpus: bool,
    edit: Option<RuntimeInputEdit>,
) -> PlaygroundResponse {
    let files = &session.files;
    let prepared = &session.prepared;
    let bundle = session.bundle.clone();
    let mut diagnostics = Vec::new();
    let mut parse = None;
    let mut highlights = Vec::new();
    let mut injections = Vec::new();
    let mut layers = Vec::new();
    let runtime = prepared.runtime();
    let should_parse_input = !input.is_empty() || !run_corpus;
    if should_parse_input {
        match runtime {
            Ok(runtime) => match parse_source_with_optional_recovery(
                runtime,
                session
                    .scanner_selection
                    .scanner
                    .as_ref()
                    .map(|scanner| scanner as &dyn ReducedExternalScanner),
                input,
                edit.and_then(|edit| {
                    Some((
                        session.last_input.as_deref()?,
                        session.last_report.as_ref()?,
                        edit,
                    ))
                }),
            ) {
                Ok(playground_report) => {
                    let report = playground_report.report;
                    let accepted_tree_events = report.accepted_tree_events();
                    let accepted_error_count = count_accepted_errors(&accepted_tree_events);
                    let accepted_missing_count = count_accepted_missing(&accepted_tree_events);
                    parse = Some(parse_output(&report));
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
                    if let Some(query) = query_source_for_kind(
                        files,
                        &prepared.raw.name,
                        PlaygroundQueryKind::Highlights,
                    ) {
                        highlights = highlight_outputs(&query, &prepared.parser, &report, input);
                    }
                    if let Some(query) = query_source_for_kind(
                        files,
                        &prepared.raw.name,
                        PlaygroundQueryKind::Injections,
                    ) {
                        injections = injection_outputs(&query, &prepared.parser, &report, input);
                        layers = layer_outputs(input, &injections, &session.embedded_languages);
                        diagnostics.extend(layer_diagnostics(&layers));
                    }
                    session.last_input = Some(input.to_owned());
                    session.last_report = Some(report);
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
        injections,
        layers,
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

fn count_reused_nodes(events: &[TreeEvent]) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, TreeEvent::ReuseNode { .. }))
        .count()
}

fn parse_output(report: &RuntimeParseReport) -> ParseOutput {
    let accepted_tree_events = report.accepted_tree_events();
    ParseOutput {
        sexp: report.tree().to_sexp(),
        accepted_count: report.accepted_count(),
        failure_count: report.failure_count(),
        max_live_versions: report.max_live_versions(),
        trace_event_count: report.trace_events().len(),
        tree_event_count: report.tree_events().len(),
        reuse_node_count: count_reused_nodes(report.tree_events()),
        accepted_tree_event_count: accepted_tree_events.len(),
        accepted_error_count: count_accepted_errors(&accepted_tree_events),
        accepted_missing_count: count_accepted_missing(&accepted_tree_events),
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

fn prepare_embedded_languages(files: &[BundleFile]) -> BTreeMap<String, PreparedEmbeddedLanguage> {
    let mut grouped = BTreeMap::<String, Vec<BundleFile>>::new();
    for file in files {
        let Some((language, relative)) = embedded_language_relative(&file.path) else {
            continue;
        };
        grouped.entry(language).or_default().push(BundleFile {
            path: relative,
            text: file.text.clone(),
        });
    }

    grouped
        .into_iter()
        .map(|(language, files)| {
            let files = normalize_bundle_files(files);
            (language, prepare_embedded_language(files))
        })
        .collect()
}

fn prepare_embedded_language(files: Vec<BundleFile>) -> PreparedEmbeddedLanguage {
    let mut bundle = summarize_bundle(&files);
    let Some(grammar_file) = find_file(&files, "src/grammar.json") else {
        return PreparedEmbeddedLanguage {
            files,
            prepared: None,
            scanner_selection: None,
            injection_regex: None,
            diagnostics: vec![diagnostic(
                "bundle",
                "embedded language bundle does not contain src/grammar.json".to_owned(),
                None,
            )],
        };
    };
    let prepared = match prepare_grammar(&grammar_file.text) {
        Ok(prepared) => prepared,
        Err((stage, message)) => {
            return PreparedEmbeddedLanguage {
                files,
                prepared: None,
                scanner_selection: None,
                injection_regex: None,
                diagnostics: vec![diagnostic(&stage, message, None)],
            };
        }
    };
    let (injection_regex, diagnostics) = manifest_injection_regex(&files, &prepared.raw.name);
    let scanner_selection = scanner_selection(&files, &prepared.parser);
    bundle
        .active_scanner
        .clone_from(&scanner_selection.active_scanner);
    if !scanner_selection.supports_required_externals(&prepared.parser) {
        return PreparedEmbeddedLanguage {
            files,
            prepared: None,
            scanner_selection: None,
            injection_regex,
            diagnostics: vec![diagnostic(
                "scanner",
                unsupported_external_scanner_message(&bundle, &prepared.parser),
                None,
            )],
        };
    }

    PreparedEmbeddedLanguage {
        files,
        prepared: Some(prepared),
        scanner_selection: Some(scanner_selection),
        injection_regex,
        diagnostics,
    }
}

fn manifest_injection_regex(
    files: &[BundleFile],
    grammar_name: &str,
) -> (Option<Regex>, Vec<Diagnostic>) {
    let Some(manifest) = find_file(files, "tree-sitter.json") else {
        return (None, Vec::new());
    };
    let config = match facet_json::from_str::<TreeSitterConfig>(&manifest.text) {
        Ok(config) => config,
        Err(error) => {
            return (
                None,
                vec![diagnostic(
                    "manifest",
                    format!("could not decode tree-sitter.json: {error}"),
                    None,
                )],
            );
        }
    };
    let grammar_config = config
        .grammars
        .iter()
        .find(|grammar| grammar.name == grammar_name)
        .or_else(|| {
            if config.grammars.len() == 1 {
                config.grammars.first()
            } else {
                None
            }
        });
    let Some(source) = grammar_config.and_then(|grammar| grammar.injection_regex.as_deref()) else {
        return (None, Vec::new());
    };
    match Regex::new(source) {
        Ok(regex) => (Some(regex), Vec::new()),
        Err(error) => (
            None,
            vec![diagnostic(
                "manifest",
                format!("could not compile injection-regex {source:?}: {error}"),
                None,
            )],
        ),
    }
}

fn embedded_language_relative(path: &str) -> Option<(String, String)> {
    let rest = path.strip_prefix("languages/")?;
    let (language, relative) = rest.split_once('/')?;
    if language.is_empty() || relative.is_empty() {
        return None;
    }
    Some((language.to_owned(), relative.to_owned()))
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

#[derive(Debug, Clone, Copy)]
enum PlaygroundQueryKind {
    Highlights,
    Injections,
}

impl PlaygroundQueryKind {
    const fn default_path(self) -> &'static str {
        match self {
            Self::Highlights => "queries/highlights.scm",
            Self::Injections => "queries/injections.scm",
        }
    }
}

fn query_source_for_kind(
    files: &[BundleFile],
    grammar_name: &str,
    kind: PlaygroundQueryKind,
) -> Option<QuerySource> {
    let configured = manifest_query_paths(files, grammar_name, kind);
    let paths = if configured.is_empty() {
        vec![kind.default_path().to_owned()]
    } else {
        configured
    };
    let mut source = String::new();
    for path in paths {
        let Some(file) = find_file(files, &path) else {
            continue;
        };
        if !source.is_empty() && !source.ends_with('\n') {
            source.push('\n');
        }
        source.push_str(&file.text);
    }
    (!source.is_empty()).then_some(QuerySource(source))
}

fn manifest_query_paths(
    files: &[BundleFile],
    grammar_name: &str,
    kind: PlaygroundQueryKind,
) -> Vec<String> {
    let Some(manifest) = find_file(files, "tree-sitter.json") else {
        return Vec::new();
    };
    let Ok(config) = facet_json::from_str::<TreeSitterConfig>(&manifest.text) else {
        return Vec::new();
    };
    let Some(grammar_config) = config
        .grammars
        .iter()
        .find(|grammar| grammar.name == grammar_name)
        .or_else(|| {
            if config.grammars.len() == 1 {
                config.grammars.first()
            } else {
                None
            }
        })
    else {
        return Vec::new();
    };
    let query_paths = match kind {
        PlaygroundQueryKind::Highlights => grammar_config.highlights.as_ref(),
        PlaygroundQueryKind::Injections => grammar_config.injections.as_ref(),
    };
    query_paths
        .into_iter()
        .flat_map(|paths| paths.iter())
        .map(str::to_owned)
        .collect()
}

fn injection_outputs(
    query: &QuerySource,
    parser: &ParserGrammar,
    report: &snark::parser::RuntimeParseReport,
    input: &str,
) -> Vec<InjectionOutput> {
    query
        .execute_runtime_injections(parser, report, input)
        .into_iter()
        .flat_map(|region| {
            let language = region.language().to_owned();
            let combined = region.combined();
            let include_children = region.include_children();
            region
                .chunks()
                .iter()
                .map(move |chunk| {
                    let bytes = chunk.bytes();
                    let points = chunk.points();
                    InjectionOutput {
                        language: language.clone(),
                        combined,
                        include_children,
                        text: chunk.text().to_owned(),
                        start_byte: bytes.start().get(),
                        end_byte: bytes.end().get(),
                        start_row: points.start().row().get(),
                        start_column: points.start().column().get(),
                        end_row: points.end().row().get(),
                        end_column: points.end().column().get(),
                    }
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

#[derive(Debug)]
struct LayerGroup<'a> {
    language: String,
    combined: bool,
    regions: Vec<&'a InjectionOutput>,
}

#[derive(Debug, Clone, Copy)]
struct LayerSegment {
    virtual_start: u32,
    virtual_end: u32,
    host_start: u32,
}

fn layer_outputs(
    host_input: &str,
    injections: &[InjectionOutput],
    embedded_languages: &BTreeMap<String, PreparedEmbeddedLanguage>,
) -> Vec<LayerOutput> {
    layer_outputs_at_depth(host_input, injections, embedded_languages, 0)
}

fn layer_outputs_at_depth(
    host_input: &str,
    injections: &[InjectionOutput],
    embedded_languages: &BTreeMap<String, PreparedEmbeddedLanguage>,
    depth: usize,
) -> Vec<LayerOutput> {
    let mut groups = Vec::<LayerGroup<'_>>::new();
    let mut combined_groups = BTreeMap::<String, usize>::new();
    for injection in injections {
        if injection.combined {
            let index = *combined_groups
                .entry(injection.language.clone())
                .or_insert_with(|| {
                    let index = groups.len();
                    groups.push(LayerGroup {
                        language: injection.language.clone(),
                        combined: true,
                        regions: Vec::new(),
                    });
                    index
                });
            groups[index].regions.push(injection);
        } else {
            groups.push(LayerGroup {
                language: injection.language.clone(),
                combined: false,
                regions: vec![injection],
            });
        }
    }

    groups
        .into_iter()
        .map(|group| layer_output(host_input, group, embedded_languages, depth))
        .collect()
}

fn layer_output(
    host_input: &str,
    group: LayerGroup<'_>,
    embedded_languages: &BTreeMap<String, PreparedEmbeddedLanguage>,
    depth: usize,
) -> LayerOutput {
    let ranges = group
        .regions
        .iter()
        .map(|region| LayerSourceRange {
            text: region.text.clone(),
            start_byte: region.start_byte,
            end_byte: region.end_byte,
            start_row: region.start_row,
            start_column: region.start_column,
            end_row: region.end_row,
            end_column: region.end_column,
        })
        .collect::<Vec<_>>();
    let mut input = String::new();
    let mut segments = Vec::new();
    for region in &group.regions {
        let virtual_start = input.len();
        input.push_str(&region.text);
        let virtual_end = input.len();
        segments.push(LayerSegment {
            virtual_start: virtual_start as u32,
            virtual_end: virtual_end as u32,
            host_start: region.start_byte,
        });
    }

    let Some(language) = embedded_language(embedded_languages, &group.language) else {
        let message = format!(
            "no embedded language bundle for {}; expected languages/{}/src/grammar.json",
            group.language, group.language
        );
        return LayerOutput {
            language: group.language,
            combined: group.combined,
            ranges,
            input,
            parse: None,
            highlights: Vec::new(),
            injections: Vec::new(),
            layers: Vec::new(),
            diagnostics: vec![diagnostic(
                "injection",
                message,
                group
                    .regions
                    .first()
                    .map(|region| injection_diagnostic_span(region)),
            )],
        };
    };
    let (Some(prepared), Some(scanner_selection)) = (
        language.prepared.as_ref(),
        language.scanner_selection.as_ref(),
    ) else {
        return LayerOutput {
            language: group.language,
            combined: group.combined,
            ranges,
            input,
            parse: None,
            highlights: Vec::new(),
            injections: Vec::new(),
            layers: Vec::new(),
            diagnostics: language.diagnostics.clone(),
        };
    };
    let runtime = match prepared.runtime() {
        Ok(runtime) => runtime,
        Err(error) => {
            return LayerOutput {
                language: group.language,
                combined: group.combined,
                ranges,
                input,
                parse: None,
                highlights: Vec::new(),
                injections: Vec::new(),
                layers: Vec::new(),
                diagnostics: vec![diagnostic("runtime", error.to_string(), None)],
            };
        }
    };
    let report = match parse_source_with_optional_recovery(
        runtime,
        scanner_selection
            .scanner
            .as_ref()
            .map(|scanner| scanner as &dyn ReducedExternalScanner),
        &input,
        None,
    ) {
        Ok(report) => report,
        Err(error) => {
            let diagnostic = reduced_error_diagnostic("parse", &error, &input);
            return LayerOutput {
                language: group.language,
                combined: group.combined,
                ranges,
                input,
                parse: None,
                highlights: Vec::new(),
                injections: Vec::new(),
                layers: Vec::new(),
                diagnostics: vec![remap_layer_diagnostic(diagnostic, host_input, &segments)],
            };
        }
    };
    let mut diagnostics = Vec::new();
    let parse = parse_output(&report.report);
    if parse.accepted_error_count > 0 || parse.accepted_missing_count > 0 {
        diagnostics.push(diagnostic(
            "parse",
            format!(
                "accepted injected parse contains {} ERROR node(s) and {} MISSING node(s)",
                parse.accepted_error_count, parse.accepted_missing_count
            ),
            None,
        ));
    }
    let highlights = query_source_for_kind(
        &language.files,
        &prepared.raw.name,
        PlaygroundQueryKind::Highlights,
    )
    .map_or_else(Vec::new, |query| {
        layer_highlight_outputs(
            &query,
            &prepared.parser,
            &report.report,
            &input,
            host_input,
            &segments,
        )
    });
    let injections = query_source_for_kind(
        &language.files,
        &prepared.raw.name,
        PlaygroundQueryKind::Injections,
    )
    .map_or_else(Vec::new, |query| {
        injection_outputs(&query, &prepared.parser, &report.report, &input)
    });
    let child_injections = remap_layer_injections(&injections, host_input, &segments);
    let layers = if child_injections.is_empty() {
        Vec::new()
    } else if depth + 1 >= MAX_INJECTION_LAYER_DEPTH {
        diagnostics.push(diagnostic(
            "injection",
            format!("maximum injection layer depth {MAX_INJECTION_LAYER_DEPTH} reached"),
            None,
        ));
        Vec::new()
    } else {
        layer_outputs_at_depth(host_input, &child_injections, embedded_languages, depth + 1)
    };
    LayerOutput {
        language: group.language,
        combined: group.combined,
        ranges,
        input,
        parse: Some(parse),
        highlights,
        injections: child_injections,
        layers,
        diagnostics,
    }
}

fn layer_diagnostics(layers: &[LayerOutput]) -> Vec<Diagnostic> {
    layers
        .iter()
        .flat_map(|layer| {
            let own = layer.diagnostics.iter().map(|diagnostic| Diagnostic {
                stage: format!("layer/{}", diagnostic.stage),
                message: format!("{}: {}", layer.language, diagnostic.message),
                primary_span: diagnostic.primary_span.clone(),
            });
            own.chain(layer_diagnostics(&layer.layers))
                .collect::<Vec<_>>()
        })
        .collect()
}

fn remap_layer_diagnostic(
    diagnostic: Diagnostic,
    root_input: &str,
    segments: &[LayerSegment],
) -> Diagnostic {
    let primary_span = diagnostic
        .primary_span
        .as_ref()
        .and_then(|span| remap_layer_span(span, root_input, segments));
    Diagnostic {
        primary_span,
        ..diagnostic
    }
}

fn remap_layer_span(
    span: &DiagnosticSpan,
    root_input: &str,
    segments: &[LayerSegment],
) -> Option<DiagnosticSpan> {
    let segment = segment_for_virtual_range(segments, span.start_byte, span.end_byte)?;
    let start_byte = segment.host_start + span.start_byte.saturating_sub(segment.virtual_start);
    let end_byte = segment.host_start + span.end_byte.saturating_sub(segment.virtual_start);
    let (start_row, start_column) = point_for_byte(root_input, start_byte)?;
    let (end_row, end_column) = point_for_byte(root_input, end_byte)?;
    Some(DiagnosticSpan {
        start_byte,
        end_byte,
        start_row,
        start_column,
        end_row,
        end_column,
    })
}

fn remap_layer_injections(
    injections: &[InjectionOutput],
    root_input: &str,
    segments: &[LayerSegment],
) -> Vec<InjectionOutput> {
    injections
        .iter()
        .flat_map(|injection| {
            segments
                .iter()
                .filter_map(move |segment| {
                    let clipped_start = injection.start_byte.max(segment.virtual_start);
                    let clipped_end = injection.end_byte.min(segment.virtual_end);
                    if clipped_start >= clipped_end {
                        return None;
                    }
                    let start_byte =
                        segment.host_start + clipped_start.saturating_sub(segment.virtual_start);
                    let end_byte =
                        segment.host_start + clipped_end.saturating_sub(segment.virtual_start);
                    let (start_row, start_column) = point_for_byte(root_input, start_byte)?;
                    let (end_row, end_column) = point_for_byte(root_input, end_byte)?;
                    Some(InjectionOutput {
                        language: injection.language.clone(),
                        combined: injection.combined,
                        include_children: injection.include_children,
                        text: host_text(root_input, start_byte, end_byte).to_owned(),
                        start_byte,
                        end_byte,
                        start_row,
                        start_column,
                        end_row,
                        end_column,
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

fn embedded_language<'a>(
    embedded_languages: &'a BTreeMap<String, PreparedEmbeddedLanguage>,
    language: &str,
) -> Option<&'a PreparedEmbeddedLanguage> {
    embedded_languages
        .get(language)
        .or_else(|| embedded_languages.get(&language.to_ascii_lowercase()))
        .or_else(|| {
            embedded_languages
                .values()
                .find(|embedded| embedded.matches_injection_language(language))
        })
}

impl PreparedEmbeddedLanguage {
    fn matches_injection_language(&self, language: &str) -> bool {
        self.injection_regex
            .as_ref()
            .is_some_and(|regex| regex.is_match(language))
    }
}

fn layer_highlight_outputs(
    query: &QuerySource,
    parser: &ParserGrammar,
    report: &RuntimeParseReport,
    input: &str,
    host_input: &str,
    segments: &[LayerSegment],
) -> Vec<HighlightOutput> {
    query
        .execute_runtime_highlights(parser, report, input)
        .into_iter()
        .flat_map(|capture| {
            let bytes = capture.bytes();
            let start = bytes.start().get();
            let end = bytes.end().get();
            segments
                .iter()
                .filter_map(move |segment| {
                    let clipped_start = start.max(segment.virtual_start);
                    let clipped_end = end.min(segment.virtual_end);
                    if clipped_start >= clipped_end {
                        return None;
                    }
                    let host_start =
                        segment.host_start + clipped_start.saturating_sub(segment.virtual_start);
                    let host_end =
                        segment.host_start + clipped_end.saturating_sub(segment.virtual_start);
                    let (start_row, start_column) = point_for_byte(host_input, host_start)?;
                    let (end_row, end_column) = point_for_byte(host_input, host_end)?;
                    Some(HighlightOutput {
                        capture_name: capture.capture_name().to_owned(),
                        text: host_text(host_input, host_start, host_end).to_owned(),
                        start_byte: host_start,
                        end_byte: host_end,
                        start_row,
                        start_column,
                        end_row,
                        end_column,
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

fn segment_for_virtual_range(
    segments: &[LayerSegment],
    start: u32,
    end: u32,
) -> Option<LayerSegment> {
    segments
        .iter()
        .copied()
        .find(|segment| segment.virtual_start <= start && end <= segment.virtual_end)
}

fn host_text(input: &str, start: u32, end: u32) -> &str {
    input.get(start as usize..end as usize).unwrap_or("")
}

fn point_for_byte(input: &str, byte: u32) -> Option<(u32, u32)> {
    let byte = byte as usize;
    if byte > input.len() || !input.is_char_boundary(byte) {
        return None;
    }
    let mut row = 0u32;
    let mut column = 0u32;
    for ch in input[..byte].chars() {
        if ch == '\n' {
            row += 1;
            column = 0;
        } else {
            column += ch.len_utf8() as u32;
        }
    }
    Some((row, column))
}

fn injection_diagnostic_span(region: &InjectionOutput) -> DiagnosticSpan {
    DiagnosticSpan {
        start_byte: region.start_byte,
        end_byte: region.end_byte,
        start_row: region.start_row,
        start_column: region.start_column,
        end_row: region.end_row,
        end_column: region.end_column,
    }
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
    let query = query_source_for_kind(files, &prepared.raw.name, PlaygroundQueryKind::Highlights);

    for file in files
        .iter()
        .filter(|file| is_highlight_fixture_path(&file.path))
    {
        let Some(query) = query.as_ref() else {
            results.push(highlight_test_error(
                file,
                "bundle contains highlight fixtures but no highlight query source".to_owned(),
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
    previous: Option<(&str, &RuntimeParseReport, RuntimeInputEdit)>,
) -> Result<PlaygroundParseReport, ReducedParseError> {
    let runtime = match scanner {
        Some(scanner) => runtime.with_external_scanner(scanner),
        None => runtime,
    }
    .with_recovery_step_limit(PLAYGROUND_RECOVERY_STEP_LIMIT);
    let strict = if let Some((old_input, previous_report, edit)) = previous {
        runtime.reparse_compact_with_report(old_input, previous_report, edit, input)
    } else {
        runtime.parse_compact_with_report(input)
    };
    match strict {
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
        injections: Vec::new(),
        layers: Vec::new(),
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
        "grammar/tree-sitter.json" => Some("tree-sitter.json".to_owned()),
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
        "tree-sitter.json" => return Some(path.to_owned()),
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
        "/tree-sitter.json",
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
                path: "langs/group-acorn/css/def/grammar/tree-sitter.json".to_owned(),
                text: String::new(),
            },
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
        assert!(find_file(&files, "tree-sitter.json").is_some());
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
    fn playground_session_reparse_uses_runtime_reuse() {
        let files = vec![
            BundleFile {
                path: "src/grammar.json".to_owned(),
                text: r##"{
              "name": "mini",
              "rules": {
                "source_file": {
                  "type": "SEQ",
                  "members": [
                    { "type": "SYMBOL", "name": "insensitive" },
                    { "type": "SYMBOL", "name": "wrapped" }
                  ]
                },
                "insensitive": {
                  "type": "PATTERN",
                  "value": "abc",
                  "flags": "i"
                },
                "wrapped": {
                  "type": "TOKEN",
                  "content": {
                    "type": "PATTERN",
                    "value": "xyz",
                    "flags": "i"
                  }
                }
              }
            }"##
                .to_owned(),
            },
            BundleFile {
                path: "queries/highlights.scm".to_owned(),
                text: "(insensitive) @variable\n(wrapped) @constant\n".to_owned(),
            },
        ];
        let mut session = PlaygroundSession::prepare_files(files).unwrap();
        let initial = PlaygroundParseRequest {
            input: "ABCXYZ".to_owned(),
            run_corpus: false,
            edit: None,
        };
        let initial = session.parse_json(&facet_json::to_string(&initial).unwrap());
        let initial: PlaygroundResponse = facet_json::from_str(&initial).unwrap();
        assert!(initial.ok, "{:?}", initial.diagnostics);
        assert_eq!(
            initial.parse.as_ref().map(|parse| parse.sexp.as_str()),
            Some("(source_file (insensitive) (wrapped))")
        );
        assert_eq!(
            initial.parse.as_ref().map(|parse| parse.reuse_node_count),
            Some(0)
        );
        assert_eq!(
            initial
                .highlights
                .iter()
                .map(|capture| (capture.capture_name.as_str(), capture.text.as_str()))
                .collect::<Vec<_>>(),
            vec![("variable", "ABC"), ("constant", "XYZ")]
        );

        let reparsed = PlaygroundParseRequest {
            input: "abcXYZ".to_owned(),
            run_corpus: false,
            edit: Some(PlaygroundInputEdit {
                start_byte: 0,
                old_end_byte: 3,
                new_end_byte: 3,
            }),
        };
        let reparsed = session.parse_json(&facet_json::to_string(&reparsed).unwrap());
        let reparsed: PlaygroundResponse = facet_json::from_str(&reparsed).unwrap();

        assert!(reparsed.ok, "{:?}", reparsed.diagnostics);
        assert_eq!(
            reparsed.parse.as_ref().map(|parse| parse.sexp.as_str()),
            Some("(source_file (insensitive) (wrapped))")
        );
        assert_eq!(
            reparsed.parse.as_ref().map(|parse| parse.reuse_node_count),
            Some(1)
        );
        assert_eq!(
            reparsed
                .highlights
                .iter()
                .map(|capture| (capture.capture_name.as_str(), capture.text.as_str()))
                .collect::<Vec<_>>(),
            vec![("variable", "abc"), ("constant", "XYZ")]
        );
    }

    #[test]
    fn playground_session_reparse_refreshes_injected_layers() {
        let files = vec![
            BundleFile {
                path: "src/grammar.json".to_owned(),
                text: r##"{
  "name": "host_session",
  "rules": {
    "document": {
      "type": "SEQ",
      "members": [
        { "type": "SYMBOL", "name": "prefix" },
        { "type": "SYMBOL", "name": "code" }
      ]
    },
    "prefix": { "type": "PATTERN", "value": "x+" },
    "code": {
      "type": "TOKEN",
      "content": { "type": "PATTERN", "value": "[a-z]+" }
    }
  },
  "extras": [],
  "conflicts": [],
  "precedences": [],
  "externals": [],
  "inline": [],
  "supertypes": []
}"##
                .to_owned(),
            },
            BundleFile {
                path: "queries/injections.scm".to_owned(),
                text: r#"((code) @injection.content
  (#set! injection.language "text"))
"#
                .to_owned(),
            },
            BundleFile {
                path: "languages/text/src/grammar.json".to_owned(),
                text: r##"{
  "name": "text",
  "rules": {
    "document": {
      "type": "REPEAT1",
      "content": { "type": "SYMBOL", "name": "word" }
    },
    "word": {
      "type": "TOKEN",
      "content": { "type": "PATTERN", "value": "[a-z]+" }
    }
  },
  "extras": [{ "type": "PATTERN", "value": "\\s+" }],
  "conflicts": [],
  "precedences": [],
  "externals": [],
  "inline": [],
  "supertypes": []
}"##
                .to_owned(),
            },
            BundleFile {
                path: "languages/text/queries/highlights.scm".to_owned(),
                text: "(word) @constant\n".to_owned(),
            },
        ];
        let mut session = PlaygroundSession::prepare_files(files).unwrap();
        let initial = PlaygroundParseRequest {
            input: "xxalpha".to_owned(),
            run_corpus: false,
            edit: None,
        };
        let initial = session.parse_json(&facet_json::to_string(&initial).unwrap());
        let initial: PlaygroundResponse = facet_json::from_str(&initial).unwrap();
        assert!(initial.ok, "{:?}", initial.diagnostics);
        assert_eq!(
            initial.parse.as_ref().map(|parse| parse.sexp.as_str()),
            Some("(document (prefix) (code))")
        );
        assert_eq!(initial.layers.len(), 1);
        assert_eq!(initial.layers[0].language, "text");
        assert_eq!(initial.layers[0].input, "alpha");
        assert_eq!(
            initial.layers[0]
                .highlights
                .iter()
                .map(|capture| (capture.capture_name.as_str(), capture.text.as_str()))
                .collect::<Vec<_>>(),
            vec![("constant", "alpha")]
        );

        let reparsed = PlaygroundParseRequest {
            input: "xxbeta".to_owned(),
            run_corpus: false,
            edit: Some(PlaygroundInputEdit {
                start_byte: 2,
                old_end_byte: 7,
                new_end_byte: 6,
            }),
        };
        let reparsed = session.parse_json(&facet_json::to_string(&reparsed).unwrap());
        let reparsed: PlaygroundResponse = facet_json::from_str(&reparsed).unwrap();

        assert!(reparsed.ok, "{:?}", reparsed.diagnostics);
        assert_eq!(
            reparsed.parse.as_ref().map(|parse| parse.sexp.as_str()),
            Some("(document (prefix) (code))")
        );
        assert_eq!(reparsed.injections.len(), 1);
        assert_eq!(reparsed.injections[0].text, "beta");
        assert_eq!(reparsed.injections[0].start_byte, 2);
        assert_eq!(reparsed.injections[0].end_byte, 6);
        assert_eq!(reparsed.layers.len(), 1);
        assert_eq!(reparsed.layers[0].language, "text");
        assert_eq!(reparsed.layers[0].input, "beta");
        assert_eq!(
            reparsed.layers[0]
                .highlights
                .iter()
                .map(|capture| {
                    (
                        capture.capture_name.as_str(),
                        capture.text.as_str(),
                        capture.start_byte,
                        capture.end_byte,
                    )
                })
                .collect::<Vec<_>>(),
            vec![("constant", "beta", 2, 6)]
        );
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
    fn playground_response_reports_injection_regions() {
        let request = PlaygroundRequest {
            files: vec![
                BundleFile {
                    path: "src/grammar.json".to_owned(),
                    text: r#"{
  "name": "injection_smoke",
  "rules": {
    "document": { "type": "SYMBOL", "name": "lua_code" },
    "lua_code": {
      "type": "TOKEN",
      "content": { "type": "PATTERN", "value": "[A-Za-z_]+" }
    }
  },
  "extras": [],
  "conflicts": [],
  "precedences": [],
  "externals": [],
  "inline": [],
  "supertypes": []
}"#
                    .to_owned(),
                },
                BundleFile {
                    path: "queries/injections.scm".to_owned(),
                    text: r#"((lua_code) @injection.content
  (#set! injection.language "lua")
  (#set! injection.combined)
  (#set! injection.include-children))"#
                        .to_owned(),
                },
                BundleFile {
                    path: "languages/lua/src/grammar.json".to_owned(),
                    text: r#"{
  "name": "lua",
  "rules": {
    "document": { "type": "SYMBOL", "name": "word" },
    "word": {
      "type": "TOKEN",
      "content": { "type": "PATTERN", "value": "[A-Za-z_]+" }
    }
  },
  "extras": [],
  "conflicts": [],
  "precedences": [],
  "externals": [],
  "inline": [],
  "supertypes": []
}"#
                    .to_owned(),
                },
            ],
            input: "print".to_owned(),
            run_corpus: false,
        };

        let response =
            playground_response(&facet_json::to_string(&request).expect("request serializes"));

        assert!(response.ok, "{:?}", response.diagnostics);
        assert_eq!(response.injections.len(), 1);
        let injection = &response.injections[0];
        assert_eq!(injection.language, "lua");
        assert!(injection.combined);
        assert!(injection.include_children);
        assert_eq!(injection.text, "print");
        assert_eq!(injection.start_byte, 0);
        assert_eq!(injection.end_byte, 5);
        assert_eq!(injection.start_row, 0);
        assert_eq!(injection.start_column, 0);
        assert_eq!(injection.end_row, 0);
        assert_eq!(injection.end_column, 5);
    }

    #[test]
    fn playground_response_excludes_injection_content_children_by_default() {
        let request = PlaygroundRequest {
            files: vec![
                BundleFile {
                    path: "src/grammar.json".to_owned(),
                    text: r#"{
  "name": "host",
  "rules": {
    "document": { "type": "SYMBOL", "name": "template" },
    "template": {
      "type": "SEQ",
      "members": [
        { "type": "STRING", "value": "AA" },
        { "type": "SYMBOL", "name": "code" },
        { "type": "STRING", "value": "BB" }
      ]
    },
    "code": {
      "type": "STRING",
      "value": "JS"
    }
  },
  "extras": [],
  "conflicts": [],
  "precedences": [],
  "externals": [],
  "inline": [],
  "supertypes": []
}"#
                    .to_owned(),
                },
                BundleFile {
                    path: "queries/injections.scm".to_owned(),
                    text: r#"((template) @injection.content
  (#set! injection.language "text")
  (#set! injection.combined))"#
                        .to_owned(),
                },
                BundleFile {
                    path: "languages/text/src/grammar.json".to_owned(),
                    text: r#"{
  "name": "text",
  "rules": {
    "document": { "type": "SYMBOL", "name": "word" },
    "word": {
      "type": "TOKEN",
      "content": { "type": "PATTERN", "value": "[A-Z]+" }
    }
  },
  "extras": [],
  "conflicts": [],
  "precedences": [],
  "externals": [],
  "inline": [],
  "supertypes": []
}"#
                    .to_owned(),
                },
                BundleFile {
                    path: "languages/text/queries/highlights.scm".to_owned(),
                    text: "(word) @constant\n".to_owned(),
                },
            ],
            input: "AAJSBB".to_owned(),
            run_corpus: false,
        };

        let response =
            playground_response(&facet_json::to_string(&request).expect("request serializes"));

        assert!(response.ok, "{:?}", response.diagnostics);
        assert_eq!(
            response
                .injections
                .iter()
                .map(|injection| {
                    (
                        injection.language.as_str(),
                        injection.text.as_str(),
                        injection.start_byte,
                        injection.end_byte,
                    )
                })
                .collect::<Vec<_>>(),
            vec![("text", "AA", 0, 2), ("text", "BB", 4, 6)]
        );
        assert_eq!(response.layers.len(), 1);
        let layer = &response.layers[0];
        assert_eq!(layer.language, "text");
        assert!(layer.combined);
        assert_eq!(layer.input, "AABB");
        assert_eq!(
            layer
                .highlights
                .iter()
                .map(|highlight| {
                    (
                        highlight.capture_name.as_str(),
                        highlight.text.as_str(),
                        highlight.start_byte,
                        highlight.end_byte,
                    )
                })
                .collect::<Vec<_>>(),
            vec![("constant", "AA", 0, 2), ("constant", "BB", 4, 6)]
        );
    }

    #[test]
    fn playground_response_parses_injected_language_layers() {
        let request = PlaygroundRequest {
            files: vec![
                BundleFile {
                    path: "src/grammar.json".to_owned(),
                    text: r#"{
  "name": "host",
  "rules": {
    "document": {
      "type": "SEQ",
      "members": [
        { "type": "SYMBOL", "name": "prefix" },
        { "type": "SYMBOL", "name": "code" }
      ]
    },
    "prefix": {
      "type": "TOKEN",
      "content": { "type": "PATTERN", "value": "[a-z]+" }
    },
    "code": {
      "type": "TOKEN",
      "content": { "type": "PATTERN", "value": "[A-Z]+" }
    }
  },
  "extras": [],
  "conflicts": [],
  "precedences": [],
  "externals": [],
  "inline": [],
  "supertypes": []
}"#
                    .to_owned(),
                },
                BundleFile {
                    path: "queries/injections.scm".to_owned(),
                    text: r#"((code) @injection.content
  (#set! injection.language "text"))"#
                        .to_owned(),
                },
                BundleFile {
                    path: "languages/text/src/grammar.json".to_owned(),
                    text: r#"{
  "name": "text",
  "rules": {
    "document": { "type": "SYMBOL", "name": "word" },
    "word": {
      "type": "TOKEN",
      "content": { "type": "PATTERN", "value": "[A-Z]+" }
    }
  },
  "extras": [],
  "conflicts": [],
  "precedences": [],
  "externals": [],
  "inline": [],
  "supertypes": []
}"#
                    .to_owned(),
                },
                BundleFile {
                    path: "languages/text/queries/highlights.scm".to_owned(),
                    text: "(word) @variable\n".to_owned(),
                },
            ],
            input: "xxPRINT".to_owned(),
            run_corpus: false,
        };

        let response =
            playground_response(&facet_json::to_string(&request).expect("request serializes"));

        assert!(response.ok, "{:?}", response.diagnostics);
        assert_eq!(response.layers.len(), 1);
        let layer = &response.layers[0];
        assert_eq!(layer.language, "text");
        assert!(!layer.combined);
        assert!(layer.diagnostics.is_empty(), "{:?}", layer.diagnostics);
        assert_eq!(layer.input, "PRINT");
        assert_eq!(
            layer.parse.as_ref().map(|parse| parse.sexp.as_str()),
            Some("(document (word))")
        );
        assert_eq!(layer.highlights.len(), 1);
        assert_eq!(layer.highlights[0].capture_name, "variable");
        assert_eq!(layer.highlights[0].text, "PRINT");
        assert_eq!(layer.highlights[0].start_byte, 2);
        assert_eq!(layer.highlights[0].end_byte, 7);
    }

    #[test]
    fn playground_response_resolves_dynamic_injection_language_capture() {
        let request = PlaygroundRequest {
            files: vec![
                BundleFile {
                    path: "src/grammar.json".to_owned(),
                    text: r#"{
  "name": "host",
  "rules": {
    "document": { "type": "SYMBOL", "name": "block" },
    "block": {
      "type": "SEQ",
      "members": [
        { "type": "SYMBOL", "name": "lang" },
        { "type": "STRING", "value": ":" },
        { "type": "SYMBOL", "name": "code" }
      ]
    },
    "lang": {
      "type": "TOKEN",
      "content": { "type": "PATTERN", "value": "[a-z]+" }
    },
    "code": {
      "type": "TOKEN",
      "content": { "type": "PATTERN", "value": "[A-Z]+" }
    }
  },
  "extras": [],
  "conflicts": [],
  "precedences": [],
  "externals": [],
  "inline": [],
  "supertypes": []
}"#
                    .to_owned(),
                },
                BundleFile {
                    path: "queries/injections.scm".to_owned(),
                    text: r#"((block
  (lang) @injection.language
  (code) @injection.content))"#
                        .to_owned(),
                },
                BundleFile {
                    path: "languages/demo/src/grammar.json".to_owned(),
                    text: r#"{
  "name": "demo",
  "rules": {
    "document": { "type": "SYMBOL", "name": "word" },
    "word": {
      "type": "TOKEN",
      "content": { "type": "PATTERN", "value": "[A-Z]+" }
    }
  },
  "extras": [],
  "conflicts": [],
  "precedences": [],
  "externals": [],
  "inline": [],
  "supertypes": []
}"#
                    .to_owned(),
                },
                BundleFile {
                    path: "languages/demo/queries/highlights.scm".to_owned(),
                    text: "(word) @constant\n".to_owned(),
                },
            ],
            input: "demo:PRINT".to_owned(),
            run_corpus: false,
        };

        let response =
            playground_response(&facet_json::to_string(&request).expect("request serializes"));

        assert!(response.ok, "{:?}", response.diagnostics);
        assert_eq!(response.injections.len(), 1);
        assert_eq!(response.injections[0].language, "demo");
        assert_eq!(response.injections[0].text, "PRINT");
        assert_eq!(response.injections[0].start_byte, 5);
        assert_eq!(response.layers.len(), 1);
        let layer = &response.layers[0];
        assert_eq!(layer.language, "demo");
        assert!(layer.diagnostics.is_empty(), "{:?}", layer.diagnostics);
        assert_eq!(layer.input, "PRINT");
        assert_eq!(
            layer.parse.as_ref().map(|parse| parse.sexp.as_str()),
            Some("(document (word))")
        );
        assert_eq!(layer.highlights.len(), 1);
        assert_eq!(layer.highlights[0].capture_name, "constant");
        assert_eq!(layer.highlights[0].text, "PRINT");
        assert_eq!(layer.highlights[0].start_byte, 5);
        assert_eq!(layer.highlights[0].end_byte, 10);
    }

    #[test]
    fn playground_response_filters_injection_layers_with_capture_predicates() {
        let request = PlaygroundRequest {
            files: vec![
                BundleFile {
                    path: "src/grammar.json".to_owned(),
                    text: r#"{
  "name": "host",
  "rules": {
    "document": {
      "type": "SEQ",
      "members": [
        { "type": "SYMBOL", "name": "tag" },
        { "type": "STRING", "value": ":" },
        { "type": "SYMBOL", "name": "code" }
      ]
    },
    "tag": {
      "type": "TOKEN",
      "content": { "type": "PATTERN", "value": "[a-z]+" }
    },
    "code": {
      "type": "TOKEN",
      "content": { "type": "PATTERN", "value": "[A-Z]+" }
    }
  },
  "extras": [],
  "conflicts": [],
  "precedences": [],
  "externals": [],
  "inline": [],
  "supertypes": []
}"#
                    .to_owned(),
                },
                BundleFile {
                    path: "queries/injections.scm".to_owned(),
                    text: r#"((document
  (tag) @_name
  (code) @injection.content)
  (#match? @_name ".*(hbs|glimmer).*")
  (#set! injection.language "html"))
((document
  (tag) @_name
  (code) @injection.content)
  (#eq? @_name "sql")
  (#set! injection.language "sql"))"#
                        .to_owned(),
                },
                BundleFile {
                    path: "languages/html/src/grammar.json".to_owned(),
                    text: r#"{
  "name": "html",
  "rules": {
    "document": { "type": "SYMBOL", "name": "word" },
    "word": {
      "type": "TOKEN",
      "content": { "type": "PATTERN", "value": "[A-Z]+" }
    }
  },
  "extras": [],
  "conflicts": [],
  "precedences": [],
  "externals": [],
  "inline": [],
  "supertypes": []
}"#
                    .to_owned(),
                },
                BundleFile {
                    path: "languages/html/queries/highlights.scm".to_owned(),
                    text: "(word) @tag\n".to_owned(),
                },
                BundleFile {
                    path: "languages/sql/src/grammar.json".to_owned(),
                    text: r#"{
  "name": "sql",
  "rules": {
    "document": { "type": "SYMBOL", "name": "word" },
    "word": {
      "type": "TOKEN",
      "content": { "type": "PATTERN", "value": "[A-Z]+" }
    }
  },
  "extras": [],
  "conflicts": [],
  "precedences": [],
  "externals": [],
  "inline": [],
  "supertypes": []
}"#
                    .to_owned(),
                },
            ],
            input: "hbs:PRINT".to_owned(),
            run_corpus: false,
        };

        let response =
            playground_response(&facet_json::to_string(&request).expect("request serializes"));

        assert!(response.ok, "{:?}", response.diagnostics);
        assert_eq!(
            response
                .injections
                .iter()
                .map(|injection| (injection.language.as_str(), injection.text.as_str()))
                .collect::<Vec<_>>(),
            vec![("html", "PRINT")]
        );
        assert_eq!(response.layers.len(), 1);
        let layer = &response.layers[0];
        assert_eq!(layer.language, "html");
        assert_eq!(
            layer.parse.as_ref().map(|parse| parse.sexp.as_str()),
            Some("(document (word))")
        );
        assert_eq!(layer.highlights.len(), 1);
        assert_eq!(layer.highlights[0].capture_name, "tag");
        assert_eq!(layer.highlights[0].text, "PRINT");
        assert_eq!(layer.highlights[0].start_byte, 4);
    }

    #[test]
    fn playground_response_splits_combined_layer_highlights_across_host_ranges() {
        let request = PlaygroundRequest {
            files: vec![
                BundleFile {
                    path: "src/grammar.json".to_owned(),
                    text: r#"{
  "name": "host",
  "rules": {
    "document": { "type": "REPEAT1", "content": { "type": "SYMBOL", "name": "word" } },
    "word": {
      "type": "TOKEN",
      "content": { "type": "PATTERN", "value": "[A-Z]+" }
    }
  },
  "extras": [
    { "type": "PATTERN", "value": "\\s+" }
  ],
  "conflicts": [],
  "precedences": [],
  "externals": [],
  "inline": [],
  "supertypes": []
}"#
                    .to_owned(),
                },
                BundleFile {
                    path: "queries/injections.scm".to_owned(),
                    text: r#"((word) @injection.content
  (#set! injection.language "text")
  (#set! injection.combined))"#
                        .to_owned(),
                },
                BundleFile {
                    path: "languages/text/src/grammar.json".to_owned(),
                    text: r#"{
  "name": "text",
  "rules": {
    "document": { "type": "SYMBOL", "name": "word" },
    "word": {
      "type": "TOKEN",
      "content": { "type": "PATTERN", "value": "[A-Z]+" }
    }
  },
  "extras": [],
  "conflicts": [],
  "precedences": [],
  "externals": [],
  "inline": [],
  "supertypes": []
}"#
                    .to_owned(),
                },
                BundleFile {
                    path: "languages/text/queries/highlights.scm".to_owned(),
                    text: "(word) @variable\n".to_owned(),
                },
            ],
            input: "AA CCC".to_owned(),
            run_corpus: false,
        };

        let response =
            playground_response(&facet_json::to_string(&request).expect("request serializes"));

        assert!(response.ok, "{:?}", response.diagnostics);
        assert_eq!(response.layers.len(), 1);
        let layer = &response.layers[0];
        assert!(layer.combined);
        assert_eq!(layer.input, "AACCC");
        assert_eq!(
            layer
                .highlights
                .iter()
                .map(|highlight| (
                    highlight.capture_name.as_str(),
                    highlight.text.as_str(),
                    highlight.start_byte,
                    highlight.end_byte
                ))
                .collect::<Vec<_>>(),
            vec![("variable", "AA", 0, 2), ("variable", "CCC", 3, 6)]
        );
    }

    #[test]
    fn playground_response_splits_nested_injections_across_combined_layer_ranges() {
        let request = PlaygroundRequest {
            files: vec![
                BundleFile {
                    path: "src/grammar.json".to_owned(),
                    text: r#"{
  "name": "host",
  "rules": {
    "document": { "type": "REPEAT1", "content": { "type": "SYMBOL", "name": "word" } },
    "word": {
      "type": "TOKEN",
      "content": { "type": "PATTERN", "value": "[A-Z]+" }
    }
  },
  "extras": [
    { "type": "PATTERN", "value": "\\s+" }
  ],
  "conflicts": [],
  "precedences": [],
  "externals": [],
  "inline": [],
  "supertypes": []
}"#
                    .to_owned(),
                },
                BundleFile {
                    path: "queries/injections.scm".to_owned(),
                    text: r#"((word) @injection.content
  (#set! injection.language "text")
  (#set! injection.combined))"#
                        .to_owned(),
                },
                BundleFile {
                    path: "languages/text/src/grammar.json".to_owned(),
                    text: r#"{
  "name": "text",
  "rules": {
    "document": { "type": "SYMBOL", "name": "word" },
    "word": {
      "type": "TOKEN",
      "content": { "type": "PATTERN", "value": "[A-Z]+" }
    }
  },
  "extras": [],
  "conflicts": [],
  "precedences": [],
  "externals": [],
  "inline": [],
  "supertypes": []
}"#
                    .to_owned(),
                },
                BundleFile {
                    path: "languages/text/queries/injections.scm".to_owned(),
                    text: r#"((document) @injection.content
  (#set! injection.language "inner")
  (#set! injection.combined))"#
                        .to_owned(),
                },
                BundleFile {
                    path: "languages/inner/src/grammar.json".to_owned(),
                    text: r#"{
  "name": "inner",
  "rules": {
    "document": { "type": "SYMBOL", "name": "word" },
    "word": {
      "type": "TOKEN",
      "content": { "type": "PATTERN", "value": "[A-Z]+" }
    }
  },
  "extras": [],
  "conflicts": [],
  "precedences": [],
  "externals": [],
  "inline": [],
  "supertypes": []
}"#
                    .to_owned(),
                },
                BundleFile {
                    path: "languages/inner/queries/highlights.scm".to_owned(),
                    text: "(word) @constant\n".to_owned(),
                },
            ],
            input: "AA CCC".to_owned(),
            run_corpus: false,
        };

        let response =
            playground_response(&facet_json::to_string(&request).expect("request serializes"));

        assert!(response.ok, "{:?}", response.diagnostics);
        assert_eq!(response.layers.len(), 1);
        let text_layer = &response.layers[0];
        assert_eq!(text_layer.language, "text");
        assert!(text_layer.combined);
        assert_eq!(text_layer.input, "AACCC");
        assert_eq!(text_layer.injections.len(), 2);
        assert_eq!(
            text_layer
                .injections
                .iter()
                .map(|injection| {
                    (
                        injection.language.as_str(),
                        injection.text.as_str(),
                        injection.start_byte,
                        injection.end_byte,
                    )
                })
                .collect::<Vec<_>>(),
            vec![("inner", "AA", 0, 2), ("inner", "CCC", 3, 6),]
        );
        assert_eq!(text_layer.layers.len(), 1);
        let inner_layer = &text_layer.layers[0];
        assert_eq!(inner_layer.language, "inner");
        assert!(inner_layer.combined);
        assert_eq!(inner_layer.input, "AACCC");
        assert_eq!(
            inner_layer
                .highlights
                .iter()
                .map(|highlight| {
                    (
                        highlight.capture_name.as_str(),
                        highlight.text.as_str(),
                        highlight.start_byte,
                        highlight.end_byte,
                    )
                })
                .collect::<Vec<_>>(),
            vec![("constant", "AA", 0, 2), ("constant", "CCC", 3, 6),]
        );
    }

    #[test]
    fn playground_response_resolves_injected_language_by_manifest_regex() {
        let request = PlaygroundRequest {
            files: vec![
                BundleFile {
                    path: "src/grammar.json".to_owned(),
                    text: r#"{
  "name": "host",
  "rules": {
    "document": { "type": "SYMBOL", "name": "code" },
    "code": {
      "type": "TOKEN",
      "content": { "type": "PATTERN", "value": "[A-Z]+" }
    }
  },
  "extras": [],
  "conflicts": [],
  "precedences": [],
  "externals": [],
  "inline": [],
  "supertypes": []
}"#
                    .to_owned(),
                },
                BundleFile {
                    path: "queries/injections.scm".to_owned(),
                    text: r#"((code) @injection.content
  (#set! injection.language "text/x-demo"))"#
                        .to_owned(),
                },
                BundleFile {
                    path: "languages/demo/tree-sitter.json".to_owned(),
                    text: r#"{
  "grammars": [
    {
      "name": "demo",
      "scope": "source.demo",
      "injection-regex": "^text/x-demo$"
    }
  ],
  "metadata": {
    "version": "0.0.0",
    "links": { "repository": "https://example.com/demo" }
  }
}"#
                    .to_owned(),
                },
                BundleFile {
                    path: "languages/demo/src/grammar.json".to_owned(),
                    text: r#"{
  "name": "demo",
  "rules": {
    "document": { "type": "SYMBOL", "name": "word" },
    "word": {
      "type": "TOKEN",
      "content": { "type": "PATTERN", "value": "[A-Z]+" }
    }
  },
  "extras": [],
  "conflicts": [],
  "precedences": [],
  "externals": [],
  "inline": [],
  "supertypes": []
}"#
                    .to_owned(),
                },
            ],
            input: "PRINT".to_owned(),
            run_corpus: false,
        };

        let response =
            playground_response(&facet_json::to_string(&request).expect("request serializes"));

        assert!(response.ok, "{:?}", response.diagnostics);
        assert_eq!(response.layers.len(), 1);
        let layer = &response.layers[0];
        assert_eq!(layer.language, "text/x-demo");
        assert!(layer.diagnostics.is_empty(), "{:?}", layer.diagnostics);
        assert_eq!(
            layer.parse.as_ref().map(|parse| parse.sexp.as_str()),
            Some("(document (word))")
        );
    }

    #[test]
    fn playground_response_uses_manifest_configured_query_paths() {
        let request = PlaygroundRequest {
            files: vec![
                BundleFile {
                    path: "tree-sitter.json".to_owned(),
                    text: r#"{
  "grammars": [
    {
      "name": "host",
      "scope": "source.host",
      "highlights": "queries/root-highlights.scm",
      "injections": "queries/embed.scm"
    }
  ],
  "metadata": {
    "version": "0.0.0",
    "links": { "repository": "https://example.com/host" }
  }
}"#
                    .to_owned(),
                },
                BundleFile {
                    path: "src/grammar.json".to_owned(),
                    text: r#"{
  "name": "host",
  "rules": {
    "document": { "type": "SYMBOL", "name": "code" },
    "code": {
      "type": "TOKEN",
      "content": { "type": "PATTERN", "value": "[A-Z]+" }
    }
  },
  "extras": [],
  "conflicts": [],
  "precedences": [],
  "externals": [],
  "inline": [],
  "supertypes": []
}"#
                    .to_owned(),
                },
                BundleFile {
                    path: "queries/root-highlights.scm".to_owned(),
                    text: "(code) @variable\n".to_owned(),
                },
                BundleFile {
                    path: "queries/embed.scm".to_owned(),
                    text: r#"((code) @injection.content
  (#set! injection.language "demo"))"#
                        .to_owned(),
                },
                BundleFile {
                    path: "languages/demo/tree-sitter.json".to_owned(),
                    text: r#"{
  "grammars": [
    {
      "name": "demo",
      "scope": "source.demo",
      "highlights": ["queries/base.scm", "queries/extra.scm"]
    }
  ],
  "metadata": {
    "version": "0.0.0",
    "links": { "repository": "https://example.com/demo" }
  }
}"#
                    .to_owned(),
                },
                BundleFile {
                    path: "languages/demo/src/grammar.json".to_owned(),
                    text: r#"{
  "name": "demo",
  "rules": {
    "document": { "type": "SYMBOL", "name": "word" },
    "word": {
      "type": "TOKEN",
      "content": { "type": "PATTERN", "value": "[A-Z]+" }
    }
  },
  "extras": [],
  "conflicts": [],
  "precedences": [],
  "externals": [],
  "inline": [],
  "supertypes": []
}"#
                    .to_owned(),
                },
                BundleFile {
                    path: "languages/demo/queries/base.scm".to_owned(),
                    text: "; base query intentionally empty\n".to_owned(),
                },
                BundleFile {
                    path: "languages/demo/queries/extra.scm".to_owned(),
                    text: "(word) @constant\n".to_owned(),
                },
            ],
            input: "PRINT".to_owned(),
            run_corpus: false,
        };

        let response =
            playground_response(&facet_json::to_string(&request).expect("request serializes"));

        assert!(response.ok, "{:?}", response.diagnostics);
        assert_eq!(response.highlights.len(), 1);
        assert_eq!(response.highlights[0].capture_name, "variable");
        assert_eq!(response.injections.len(), 1);
        assert_eq!(response.layers.len(), 1);
        let layer = &response.layers[0];
        assert_eq!(layer.highlights.len(), 1);
        assert_eq!(layer.highlights[0].capture_name, "constant");
        assert_eq!(layer.highlights[0].text, "PRINT");
    }

    #[test]
    fn playground_response_recurses_injected_language_layers() {
        let request = PlaygroundRequest {
            files: vec![
                BundleFile {
                    path: "src/grammar.json".to_owned(),
                    text: r#"{
  "name": "host",
  "rules": {
    "document": {
      "type": "SEQ",
      "members": [
        { "type": "SYMBOL", "name": "prefix" },
        { "type": "SYMBOL", "name": "code" }
      ]
    },
    "prefix": {
      "type": "TOKEN",
      "content": { "type": "PATTERN", "value": "[a-z]+" }
    },
    "code": {
      "type": "TOKEN",
      "content": { "type": "PATTERN", "value": "[A-Z]+" }
    }
  },
  "extras": [],
  "conflicts": [],
  "precedences": [],
  "externals": [],
  "inline": [],
  "supertypes": []
}"#
                    .to_owned(),
                },
                BundleFile {
                    path: "queries/injections.scm".to_owned(),
                    text: r#"((code) @injection.content
  (#set! injection.language "text"))"#
                        .to_owned(),
                },
                BundleFile {
                    path: "languages/text/src/grammar.json".to_owned(),
                    text: r#"{
  "name": "text",
  "rules": {
    "document": { "type": "SYMBOL", "name": "word" },
    "word": {
      "type": "TOKEN",
      "content": { "type": "PATTERN", "value": "[A-Z]+" }
    }
  },
  "extras": [],
  "conflicts": [],
  "precedences": [],
  "externals": [],
  "inline": [],
  "supertypes": []
}"#
                    .to_owned(),
                },
                BundleFile {
                    path: "languages/text/queries/injections.scm".to_owned(),
                    text: r#"((word) @injection.content
  (#set! injection.language "inner"))"#
                        .to_owned(),
                },
                BundleFile {
                    path: "languages/inner/src/grammar.json".to_owned(),
                    text: r#"{
  "name": "inner",
  "rules": {
    "document": { "type": "SYMBOL", "name": "word" },
    "word": {
      "type": "TOKEN",
      "content": { "type": "PATTERN", "value": "[A-Z]+" }
    }
  },
  "extras": [],
  "conflicts": [],
  "precedences": [],
  "externals": [],
  "inline": [],
  "supertypes": []
}"#
                    .to_owned(),
                },
                BundleFile {
                    path: "languages/inner/queries/highlights.scm".to_owned(),
                    text: "(word) @constant\n".to_owned(),
                },
            ],
            input: "xxPRINT".to_owned(),
            run_corpus: false,
        };

        let response =
            playground_response(&facet_json::to_string(&request).expect("request serializes"));

        assert!(response.ok, "{:?}", response.diagnostics);
        assert_eq!(response.layers.len(), 1);
        let text_layer = &response.layers[0];
        assert_eq!(text_layer.language, "text");
        assert_eq!(text_layer.ranges[0].start_byte, 2);
        assert_eq!(text_layer.injections.len(), 1);
        assert_eq!(text_layer.injections[0].language, "inner");
        assert_eq!(text_layer.injections[0].start_byte, 2);
        assert_eq!(text_layer.layers.len(), 1);
        let inner_layer = &text_layer.layers[0];
        assert_eq!(inner_layer.language, "inner");
        assert_eq!(
            inner_layer.parse.as_ref().map(|parse| parse.sexp.as_str()),
            Some("(document (word))")
        );
        assert_eq!(inner_layer.highlights.len(), 1);
        assert_eq!(inner_layer.highlights[0].capture_name, "constant");
        assert_eq!(inner_layer.highlights[0].text, "PRINT");
        assert_eq!(inner_layer.highlights[0].start_byte, 2);
        assert_eq!(inner_layer.highlights[0].end_byte, 7);
    }

    #[test]
    fn playground_response_promotes_injected_layer_diagnostics() {
        let request = PlaygroundRequest {
            files: vec![
                BundleFile {
                    path: "src/grammar.json".to_owned(),
                    text: r#"{
  "name": "host",
  "rules": {
    "document": {
      "type": "SEQ",
      "members": [
        { "type": "SYMBOL", "name": "prefix" },
        { "type": "SYMBOL", "name": "code" }
      ]
    },
    "prefix": {
      "type": "TOKEN",
      "content": { "type": "PATTERN", "value": "[a-z]+" }
    },
    "code": {
      "type": "TOKEN",
      "content": { "type": "PATTERN", "value": "[A-Z]+" }
    }
  },
  "extras": [],
  "conflicts": [],
  "precedences": [],
  "externals": [],
  "inline": [],
  "supertypes": []
}"#
                    .to_owned(),
                },
                BundleFile {
                    path: "queries/injections.scm".to_owned(),
                    text: r#"((code) @injection.content
  (#set! injection.language "digits"))"#
                        .to_owned(),
                },
                BundleFile {
                    path: "languages/digits/src/grammar.json".to_owned(),
                    text: r#"{
  "name": "digits",
  "rules": {
    "document": { "type": "SYMBOL", "name": "number" },
    "number": {
      "type": "TOKEN",
      "content": { "type": "PATTERN", "value": "[0-9]+" }
    }
  },
  "extras": [],
  "conflicts": [],
  "precedences": [],
  "externals": [],
  "inline": [],
  "supertypes": []
}"#
                    .to_owned(),
                },
            ],
            input: "xxPRINT".to_owned(),
            run_corpus: false,
        };

        let response =
            playground_response(&facet_json::to_string(&request).expect("request serializes"));

        assert!(!response.ok);
        assert_eq!(response.layers.len(), 1);
        assert_eq!(response.layers[0].language, "digits");
        assert_eq!(response.layers[0].diagnostics.len(), 1);
        let diagnostic = &response.diagnostics[0];
        assert_eq!(diagnostic.stage, "layer/parse");
        assert!(diagnostic.message.contains("digits:"), "{diagnostic:?}");
        let span = diagnostic
            .primary_span
            .as_ref()
            .expect("diagnostic has span");
        assert_eq!(span.start_byte, 2);
        assert_eq!(span.start_row, 0);
        assert_eq!(span.start_column, 2);
    }

    fn external_scanner_smoke_grammar_json() -> String {
        r#"{
  "name": "needs_scanner",
  "rules": {
    "source_file": { "type": "SYMBOL", "name": "external_token" }
  },
  "extras": [],
  "conflicts": [],
  "precedences": [],
  "externals": [{ "type": "SYMBOL", "name": "external_token" }],
  "inline": [],
  "supertypes": []
}"#
        .to_owned()
    }

    #[test]
    fn rejects_unsupported_external_scanner_bundles_during_prepare() {
        let request = PlaygroundRequest {
            files: vec![
                BundleFile {
                    path: "src/grammar.json".to_owned(),
                    text: external_scanner_smoke_grammar_json(),
                },
                BundleFile {
                    path: "src/scanner.c".to_owned(),
                    text: "void *tree_sitter_needs_scanner_external_scanner_create(void) { return 0; }"
                        .to_owned(),
                },
            ],
            input: "x".to_owned(),
            run_corpus: false,
        };

        let response =
            playground_response(&facet_json::to_string(&request).expect("request serializes"));

        assert!(!response.ok);
        assert_eq!(response.language.as_deref(), Some("needs_scanner"));
        assert_eq!(response.diagnostics[0].stage, "scanner");
        assert!(
            response.diagnostics[0].message.contains("external_token"),
            "{}",
            response.diagnostics[0].message
        );
        assert!(
            response.diagnostics[0]
                .message
                .contains("source-matched reduced CSS scanner host"),
            "{}",
            response.diagnostics[0].message
        );
        assert!(
            response.diagnostics[0].message.contains("src/scanner.c"),
            "{}",
            response.diagnostics[0].message
        );
        assert!(response.parse.is_none());
    }

    #[test]
    fn rejects_external_grammar_without_uploaded_scanner_source() {
        let request = PlaygroundRequest {
            files: vec![BundleFile {
                path: "src/grammar.json".to_owned(),
                text: external_scanner_smoke_grammar_json(),
            }],
            input: "x".to_owned(),
            run_corpus: false,
        };

        let response =
            playground_response(&facet_json::to_string(&request).expect("request serializes"));

        assert!(!response.ok);
        assert_eq!(response.diagnostics[0].stage, "scanner");
        assert!(
            response.diagnostics[0]
                .message
                .contains("no scanner source was uploaded"),
            "{}",
            response.diagnostics[0].message
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
    fn playground_response_honors_highlight_text_predicates() {
        let grammar_json = r##"{
  "$schema": "https://tree-sitter.github.io/tree-sitter/assets/schemas/grammar.schema.json",
  "name": "highlight_predicates",
  "rules": {
    "document": {
      "type": "REPEAT1",
      "content": { "type": "SYMBOL", "name": "word" }
    },
    "word": {
      "type": "TOKEN",
      "content": { "type": "PATTERN", "value": "[A-Za-z_]+" }
    }
  },
  "extras": [{ "type": "PATTERN", "value": "\\s+" }],
  "conflicts": [],
  "precedences": [],
  "externals": [],
  "inline": [],
  "supertypes": []
}"##;
        let highlights = r#"
((word) @constant
  (#match? @constant "^[A-Z_][A-Z_]*$"))

((word) @function.builtin
  (#eq? @function.builtin "require"))

((word) @type.builtin
  (#any-of? @type.builtin "int" "float"))
"#;
        let request = PlaygroundRequest {
            files: vec![
                BundleFile {
                    path: "src/grammar.json".to_owned(),
                    text: grammar_json.to_owned(),
                },
                BundleFile {
                    path: "queries/highlights.scm".to_owned(),
                    text: highlights.to_owned(),
                },
            ],
            input: "FOO require int float lower Mixed".to_owned(),
            run_corpus: false,
        };

        let response =
            playground_response(&facet_json::to_string(&request).expect("request serializes"));

        assert!(response.ok, "{:?}", response.diagnostics);
        assert_eq!(response.language.as_deref(), Some("highlight_predicates"));
        assert_eq!(
            response.parse.as_ref().map(|parse| parse.sexp.as_str()),
            Some("(document (word) (word) (word) (word) (word) (word))")
        );
        let captures = response
            .highlights
            .iter()
            .map(|capture| (capture.capture_name.as_str(), capture.text.as_str()))
            .collect::<Vec<_>>();
        assert_eq!(
            captures,
            vec![
                ("constant", "FOO"),
                ("function.builtin", "require"),
                ("type.builtin", "int"),
                ("type.builtin", "float"),
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
    fn arborium_nginx_sample_reports_dirty_recovered_parse() {
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
                .contains("accepted parse contains")
        );
        assert_eq!(
            response
                .diagnostics
                .first()
                .and_then(|diagnostic| diagnostic.primary_span.as_ref())
                .map(|span| (span.start_row, span.start_column)),
            Some((110, 4))
        );
        let parse = response.parse.as_ref().expect("recovered parse output");
        assert!(parse.accepted_error_count > 0);
        assert_eq!(parse.accepted_missing_count, 0);
        assert!(parse.sexp.contains("(ERROR"));
        assert!(!response.highlights.is_empty());
    }

    #[test]
    fn vendored_playground_samples_parse_from_rust() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../playgrounds/snark/src/bundled");
        let mut grammar_ids = std::fs::read_dir(&root)
            .expect("vendored playground grammar directory should be readable")
            .map(|entry| entry.expect("vendored grammar entry should be readable"))
            .filter(|entry| {
                entry
                    .file_type()
                    .expect("vendored grammar file type should be readable")
                    .is_dir()
            })
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        grammar_ids.sort();

        let mut results = Vec::new();
        for id in grammar_ids {
            let dir = root.join(&id);
            let grammar_path = dir.join("grammar.js");
            let grammar_json = snark_dsl::emit_with_boa(&grammar_path)
                .expect("grammar.js should emit grammar JSON");
            let mut files = read_bundle_files(&dir);
            files.push(BundleFile {
                path: "src/grammar.json".to_owned(),
                text: grammar_json,
            });
            files.sort_by(|left, right| left.path.cmp(&right.path));
            let sample = preferred_sample_file(&files)
                .unwrap_or_else(|| panic!("{id} should have a preferred sample"));
            let request = PlaygroundRequest {
                files,
                input: sample.text.clone(),
                run_corpus: false,
            };
            let response =
                playground_response(&facet_json::to_string(&request).expect("request serializes"));
            results.push(VendoredSampleResult {
                id,
                sample: sample.path,
                ok: response.ok,
                language: response.language,
                error_count: response
                    .parse
                    .as_ref()
                    .map(|parse| parse.accepted_error_count),
                missing_count: response
                    .parse
                    .as_ref()
                    .map(|parse| parse.accepted_missing_count),
                captures: response.highlights.len(),
            });
        }

        let summary = results
            .iter()
            .map(|result| {
                (
                    result.id.as_str(),
                    result.sample.as_str(),
                    result.ok,
                    result.language.as_deref(),
                    result.error_count,
                    result.missing_count,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            summary,
            vec![
                (
                    "capnp",
                    "samples/addressbook.capnp",
                    true,
                    Some("capnp"),
                    Some(0),
                    Some(0)
                ),
                (
                    "cedar",
                    "samples/example.cedar",
                    true,
                    Some("cedar"),
                    Some(0),
                    Some(0)
                ),
                (
                    "cedarschema",
                    "samples/example.cedarschema",
                    true,
                    Some("cedarschema"),
                    Some(0),
                    Some(0),
                ),
                (
                    "diff",
                    "samples/t-apply-1.patch",
                    true,
                    Some("diff"),
                    Some(0),
                    Some(0)
                ),
                (
                    "dot",
                    "samples/crazy.gv",
                    true,
                    Some("dot"),
                    Some(0),
                    Some(0)
                ),
                (
                    "gingembre",
                    "samples/blog-index.html",
                    true,
                    Some("gingembre"),
                    Some(0),
                    Some(0),
                ),
                (
                    "gitattributes",
                    "samples/example.gitattributes",
                    true,
                    Some("gitattributes"),
                    Some(0),
                    Some(0),
                ),
                (
                    "graphql",
                    "samples/starwars_schema.graphql",
                    true,
                    Some("graphql"),
                    Some(0),
                    Some(0),
                ),
                (
                    "json",
                    "samples/package.json",
                    true,
                    Some("json"),
                    Some(0),
                    Some(0)
                ),
                (
                    "nginx",
                    "samples/basic.conf",
                    true,
                    Some("nginx"),
                    Some(0),
                    Some(0)
                ),
                (
                    "proto",
                    "samples/addressbook.proto",
                    true,
                    Some("proto"),
                    Some(0),
                    Some(0)
                ),
                (
                    "thrift",
                    "samples/tutorial.thrift",
                    true,
                    Some("thrift"),
                    Some(0),
                    Some(0)
                ),
                (
                    "yuri",
                    "samples/example.yuri",
                    true,
                    Some("yuri"),
                    Some(0),
                    Some(0)
                ),
            ],
        );
        assert!(
            results.iter().all(|result| result.captures > 0),
            "{results:#?}"
        );
    }

    #[test]
    fn all_non_error_vendored_playground_samples_parse_from_rust() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../playgrounds/snark/src/bundled");
        let mut grammar_ids = std::fs::read_dir(&root)
            .expect("vendored playground grammar directory should be readable")
            .map(|entry| entry.expect("vendored grammar entry should be readable"))
            .filter(|entry| {
                entry
                    .file_type()
                    .expect("vendored grammar file type should be readable")
                    .is_dir()
            })
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        grammar_ids.sort();

        let mut failures = Vec::new();
        for id in grammar_ids {
            let dir = root.join(&id);
            let grammar_path = dir.join("grammar.js");
            let grammar_json = snark_dsl::emit_with_boa(&grammar_path)
                .expect("grammar.js should emit grammar JSON");
            let mut files = read_bundle_files(&dir);
            files.push(BundleFile {
                path: "src/grammar.json".to_owned(),
                text: grammar_json,
            });
            files.sort_by(|left, right| left.path.cmp(&right.path));

            let samples = files
                .iter()
                .filter(|file| file.path.starts_with("samples/"))
                .filter(|file| !is_error_sample(&file.path))
                .cloned()
                .collect::<Vec<_>>();
            for sample in samples {
                let request = PlaygroundRequest {
                    files: files.clone(),
                    input: sample.text,
                    run_corpus: false,
                };
                let response = playground_response(
                    &facet_json::to_string(&request).expect("request serializes"),
                );
                let parse = response.parse.as_ref();
                if !response.ok
                    || parse.is_none_or(|parse| {
                        parse.accepted_error_count > 0 || parse.accepted_missing_count > 0
                    })
                {
                    failures.push(VendoredSampleResult {
                        id: id.clone(),
                        sample: sample.path,
                        ok: response.ok,
                        language: response.language,
                        error_count: parse.map(|parse| parse.accepted_error_count),
                        missing_count: parse.map(|parse| parse.accepted_missing_count),
                        captures: response.highlights.len(),
                    });
                }
            }
        }

        assert!(failures.is_empty(), "{failures:#?}");
    }

    #[derive(Debug)]
    struct VendoredSampleResult {
        id: String,
        sample: String,
        ok: bool,
        language: Option<String>,
        error_count: Option<usize>,
        missing_count: Option<usize>,
        captures: usize,
    }

    fn read_bundle_files(root: &std::path::Path) -> Vec<BundleFile> {
        let mut files = Vec::new();
        read_bundle_files_inner(root, root, &mut files);
        files
    }

    fn read_bundle_files_inner(
        root: &std::path::Path,
        dir: &std::path::Path,
        files: &mut Vec<BundleFile>,
    ) {
        let mut entries = std::fs::read_dir(dir)
            .expect("vendored bundle directory should be readable")
            .map(|entry| entry.expect("vendored bundle entry should be readable"))
            .collect::<Vec<_>>();
        entries.sort_by_key(std::fs::DirEntry::path);
        for entry in entries {
            let path = entry.path();
            if entry
                .file_type()
                .expect("vendored bundle file type should be readable")
                .is_dir()
            {
                read_bundle_files_inner(root, &path, files);
                continue;
            }
            let relative = path
                .strip_prefix(root)
                .expect("vendored file should be under bundle root")
                .to_string_lossy()
                .replace('\\', "/");
            files.push(BundleFile {
                path: relative,
                text: std::fs::read_to_string(&path).unwrap_or_else(|error| {
                    panic!("{} should be readable: {error}", path.display())
                }),
            });
        }
    }

    fn preferred_sample_file(files: &[BundleFile]) -> Option<BundleFile> {
        let mut samples = files
            .iter()
            .filter(|file| file.path.starts_with("samples/"))
            .cloned()
            .collect::<Vec<_>>();
        samples.sort_by(|left, right| {
            let left_error = is_error_sample(&left.path);
            let right_error = is_error_sample(&right.path);
            left_error
                .cmp(&right_error)
                .then_with(|| left.path.cmp(&right.path))
        });
        samples.into_iter().next()
    }

    fn is_error_sample(path: &str) -> bool {
        let lower = path.to_ascii_lowercase();
        lower.contains("error") || lower.contains("invalid") || lower.contains("fail")
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
