//! Test runner for LSP extension test files.
//!
//! Loads test files (in styx format), spawns the extension, runs all test cases,
//! and reports results.
//!
//! # Span Markers
//!
//! Test inputs can include span markers to precisely specify where diagnostics
//! should appear:
//!
//! ```text
//! from alisjdf
//!      ^^^^^^^ "table not found"
//! ```
//!
//! The `^` characters mark the exact span, and the quoted string is the expected
//! message (substring match). Span marker lines are removed from the actual input
//! before sending to the extension.

use std::path::Path;

use facet_styx::RenderError;
use styx_lsp_test_schema::{
    CodeActionExpectations, CompletionExpectations, DefinitionExpectations, DiagnosticExpectation,
    DiagnosticExpectations, HoverExpectations, InlayHintExpectations, TestCase, TestFile,
};

use super::{HarnessError, TestDocument, TestHarness};

/// A parsed span marker from the input.
#[derive(Debug, Clone)]
struct SpanMarker {
    /// Line number the span is on (0-indexed, after marker lines are removed).
    line: u32,
    /// Start column (0-indexed).
    start_col: u32,
    /// End column (0-indexed, exclusive).
    end_col: u32,
    /// Expected message (substring match).
    message: String,
}

/// Parse span markers from input and return cleaned input + markers.
///
/// Span markers are lines containing only whitespace, `^` characters, and an
/// optional quoted message. They refer to the line immediately above them.
fn parse_span_markers(input: &str) -> (String, Vec<SpanMarker>) {
    let mut markers = Vec::new();
    let mut cleaned_lines = Vec::new();
    let lines: Vec<&str> = input.lines().collect();

    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];

        // Check if this line is a span marker (whitespace + ^+ + optional "message")
        if let Some(marker) = try_parse_marker_line(line) {
            // This marker refers to the previous cleaned line
            if !cleaned_lines.is_empty() {
                let target_line = (cleaned_lines.len() - 1) as u32;
                markers.push(SpanMarker {
                    line: target_line,
                    start_col: marker.start_col,
                    end_col: marker.end_col,
                    message: marker.message,
                });
            }
            // Don't add marker line to cleaned output
        } else {
            cleaned_lines.push(line);
        }
        i += 1;
    }

    // Reconstruct the cleaned input, preserving original line endings
    let cleaned = if cleaned_lines.is_empty() {
        String::new()
    } else {
        cleaned_lines.join("\n")
    };

    (cleaned, markers)
}

/// Intermediate marker data from parsing a single line.
struct MarkerLineData {
    start_col: u32,
    end_col: u32,
    message: String,
}

/// Try to parse a line as a span marker.
///
/// Returns `Some(MarkerLineData)` if the line matches the pattern:
/// `<whitespace><^+><whitespace>"<message>"`
fn try_parse_marker_line(line: &str) -> Option<MarkerLineData> {
    // Find the first ^
    let first_caret = line.find('^')?;

    // Everything before should be whitespace
    if !line[..first_caret].chars().all(|c| c.is_whitespace()) {
        return None;
    }

    // Find the extent of the carets
    let caret_end = line[first_caret..]
        .find(|c| c != '^')
        .map(|i| first_caret + i)
        .unwrap_or(line.len());

    if caret_end == first_caret {
        return None; // No carets found
    }

    // Rest of line should be whitespace + optional quoted message
    let rest = &line[caret_end..];
    let rest_trimmed = rest.trim();

    let message = if rest_trimmed.is_empty() {
        // No message - just match any diagnostic at this span
        String::new()
    } else if rest_trimmed.starts_with('"') && rest_trimmed.ends_with('"') && rest_trimmed.len() > 1
    {
        // Extract message from quotes
        rest_trimmed[1..rest_trimmed.len() - 1].to_string()
    } else {
        // Not a valid marker line
        return None;
    };

    Some(MarkerLineData {
        start_col: first_caret as u32,
        end_col: caret_end as u32,
        message,
    })
}

/// Build diagnostic expectations by merging explicit expectations with span markers.
fn build_diagnostic_expectations(
    explicit: Option<&DiagnosticExpectations>,
    span_markers: &[SpanMarker],
) -> Option<DiagnosticExpectations> {
    // If we have neither explicit expectations nor span markers, return None
    if explicit.is_none() && span_markers.is_empty() {
        return None;
    }

    // Start with explicit expectations or defaults
    let mut result = explicit.cloned().unwrap_or(DiagnosticExpectations {
        count: None,
        has: Vec::new(),
        not_has: Vec::new(),
    });

    // Add span markers as diagnostic expectations
    for marker in span_markers {
        result.has.push(DiagnosticExpectation {
            message: marker.message.clone(),
            severity: None,
            line: Some(marker.line + 1), // Convert to 1-indexed
            start_col: Some(marker.start_col),
            end_col: Some(marker.end_col),
        });
    }

    Some(result)
}

/// Result of running a single test case.
#[derive(Debug)]
pub struct TestResult {
    /// Name of the test.
    pub name: String,
    /// Whether it passed.
    pub passed: bool,
    /// Error message if failed.
    pub error: Option<String>,
}

/// Result of running a test file.
#[derive(Debug)]
pub struct TestFileResult {
    /// Path to the test file.
    pub path: String,
    /// Results for each test case.
    pub results: Vec<TestResult>,
}

impl TestFileResult {
    /// Check if all tests passed.
    pub fn all_passed(&self) -> bool {
        self.results.iter().all(|r| r.passed)
    }

    /// Get number of passed tests.
    pub fn passed_count(&self) -> usize {
        self.results.iter().filter(|r| r.passed).count()
    }

    /// Get number of failed tests.
    pub fn failed_count(&self) -> usize {
        self.results.iter().filter(|r| !r.passed).count()
    }

    /// Format as a report string.
    pub fn report(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("Test file: {}\n", self.path));
        out.push_str(&format!(
            "Results: {} passed, {} failed\n\n",
            self.passed_count(),
            self.failed_count()
        ));

        for result in &self.results {
            if result.passed {
                out.push_str(&format!("  ✓ {}\n", result.name));
            } else {
                out.push_str(&format!("  ✗ {}\n", result.name));
                if let Some(err) = &result.error {
                    for line in err.lines() {
                        out.push_str(&format!("      {}\n", line));
                    }
                }
            }
        }

        out
    }
}

/// Run all tests in a test file.
///
/// # Arguments
/// * `bin` - Path to the extension binary
/// * `args` - Arguments to pass to the binary (e.g., `&["lsp-extension"]`)
/// * `test_file_path` - Path to the `.styx` test file
/// * `schema_id` - Schema ID to use for initialization
pub async fn run_test_file(
    bin: &str,
    args: &[&str],
    test_file_path: impl AsRef<Path>,
    schema_id: &str,
) -> Result<TestFileResult, RunnerError> {
    let path = test_file_path.as_ref();

    // Read and parse the test file
    let content = std::fs::read_to_string(path)
        .map_err(|e| RunnerError::ReadFailed(path.display().to_string(), e.to_string()))?;

    let test_file: TestFile = facet_styx::from_str(&content).map_err(|e| {
        RunnerError::ParseFailed(
            path.display().to_string(),
            e.render(path.display().to_string().as_str(), &content),
        )
    })?;

    // Build command
    let mut command = vec![bin];
    command.extend(args);

    // Spawn harness
    let harness = TestHarness::spawn(&command)
        .await
        .map_err(RunnerError::Harness)?;

    // Initialize with a dummy document URI (we'll load real ones per-test)
    harness
        .initialize("file:///init.styx", schema_id)
        .await
        .map_err(RunnerError::Harness)?;

    // Run each test case
    let mut results = Vec::new();
    for (i, test_case) in test_file.tests.iter().enumerate() {
        let result = run_test_case(&harness, test_case, i).await;
        results.push(result);
    }

    // Shutdown
    let _ = harness.shutdown().await;

    Ok(TestFileResult {
        path: path.display().to_string(),
        results,
    })
}

async fn run_test_case(harness: &TestHarness, test_case: &TestCase, index: usize) -> TestResult {
    let name = test_case.name.clone();
    let uri = format!("file:///test_{}.styx", index);

    // Debug: show raw input for span marker tests
    if name.contains("span marker") {
        eprintln!("[DEBUG] Raw input for '{}':", name);
        eprintln!("---BEGIN---");
        eprintln!("{}", test_case.input);
        eprintln!("---END---");
    }

    // Parse span markers from input and get cleaned document
    let (cleaned_input, span_markers) = parse_span_markers(&test_case.input);

    // Load the document (with marker lines removed)
    let doc = TestDocument::new(&uri, &cleaned_input);
    harness.load_document(doc.clone()).await;

    let mut errors = Vec::new();

    // Check completions if expected
    if let Some(ref expected) = test_case.completions {
        if doc.cursor.is_none() {
            errors.push("completions test requires cursor marker (|) in input".to_string());
        } else {
            match harness.completions(&uri).await {
                Ok(completions) => {
                    check_completions(&completions, expected, &mut errors);
                }
                Err(e) => {
                    errors.push(format!("completions request failed: {}", e));
                }
            }
        }
    }

    // Check hover if expected
    if let Some(ref expected) = test_case.hover {
        if doc.cursor.is_none() {
            errors.push("hover test requires cursor marker (|) in input".to_string());
        } else {
            match harness.hover(&uri).await {
                Ok(hover) => {
                    check_hover(&hover, expected, &mut errors);
                }
                Err(e) => {
                    errors.push(format!("hover request failed: {}", e));
                }
            }
        }
    }

    // Build diagnostic expectations from explicit config + span markers
    let diagnostic_expectations =
        build_diagnostic_expectations(test_case.diagnostics.as_ref(), &span_markers);

    // Debug: log span markers if any
    if !span_markers.is_empty() {
        eprintln!(
            "[DEBUG] Test '{}' has {} span markers:",
            name,
            span_markers.len()
        );
        for (i, m) in span_markers.iter().enumerate() {
            eprintln!(
                "  [{}] line={}, cols={}-{}, msg='{}'",
                i, m.line, m.start_col, m.end_col, m.message
            );
        }
    }

    // Check diagnostics if we have any expectations
    if let Some(ref expected) = diagnostic_expectations {
        match harness.diagnostics(&uri).await {
            Ok(diagnostics) => {
                // Debug: log actual diagnostics
                if !span_markers.is_empty() {
                    eprintln!("[DEBUG] Got {} diagnostics:", diagnostics.len());
                    for (i, d) in diagnostics.iter().enumerate() {
                        eprintln!(
                            "  [{}] line={}, cols={}-{}, msg='{}'",
                            i,
                            d.range.start.line,
                            d.range.start.character,
                            d.range.end.character,
                            d.message
                        );
                    }
                }
                check_diagnostics(&diagnostics, expected, &mut errors);
            }
            Err(e) => {
                errors.push(format!("diagnostics request failed: {}", e));
            }
        }
    }

    // Check inlay hints if expected
    if let Some(ref expected) = test_case.inlay_hints {
        match harness.inlay_hints(&uri).await {
            Ok(hints) => {
                check_inlay_hints(&hints, expected, &mut errors);
            }
            Err(e) => {
                errors.push(format!("inlay_hints request failed: {}", e));
            }
        }
    }

    // Check definition if expected
    if let Some(ref expected) = test_case.definition {
        if doc.cursor.is_none() {
            errors.push("definition test requires cursor marker (|) in input".to_string());
        } else {
            match harness.definition(&uri).await {
                Ok(locations) => {
                    check_definition(&locations, expected, &mut errors);
                }
                Err(e) => {
                    errors.push(format!("definition request failed: {}", e));
                }
            }
        }
    }

    // Check code actions if expected
    if let Some(ref expected) = test_case.code_actions {
        if doc.cursor.is_none() {
            errors.push("code_actions test requires cursor marker (|) in input".to_string());
        } else {
            eprintln!("[DEBUG] Requesting code actions for test '{}'", name);
            match harness.code_actions(&uri).await {
                Ok(actions) => {
                    eprintln!("[DEBUG] Got {} code actions", actions.len());
                    for (i, a) in actions.iter().enumerate() {
                        eprintln!(
                            "  [{}] title='{}', kind={:?}, preferred={}",
                            i, a.title, a.kind, a.is_preferred
                        );
                    }
                    check_code_actions(&actions, expected, &mut errors);
                }
                Err(e) => {
                    errors.push(format!("code_actions request failed: {}", e));
                }
            }
        }
    }

    // Handle expect_fail: if test is expected to fail, invert the result
    let (passed, error) = if test_case.expect_fail {
        if errors.is_empty() {
            // Expected failure but passed - that's a failure
            (
                false,
                Some("expected test to fail, but it passed".to_string()),
            )
        } else {
            // Expected failure and got one - that's a pass
            (true, None)
        }
    } else {
        // Normal test
        (
            errors.is_empty(),
            if errors.is_empty() {
                None
            } else {
                Some(errors.join("\n"))
            },
        )
    };

    TestResult {
        name,
        passed,
        error,
    }
}

fn check_completions(
    completions: &[styx_lsp_ext::CompletionItem],
    expected: &CompletionExpectations,
    errors: &mut Vec<String>,
) {
    let labels: Vec<&str> = completions.iter().map(|c| c.label.as_str()).collect();

    // Check 'has' expectations
    for label in &expected.has {
        if !labels.contains(&label.as_str()) {
            errors.push(format!(
                "expected completion '{}' not found (got: {:?})",
                label, labels
            ));
        }
    }

    // Check 'not_has' expectations
    for label in &expected.not_has {
        if labels.contains(&label.as_str()) {
            errors.push(format!("unexpected completion '{}' was present", label));
        }
    }

    // Check detailed item expectations
    for item_exp in &expected.items {
        let item = completions.iter().find(|c| c.label == item_exp.label);
        match item {
            None => {
                errors.push(format!(
                    "expected completion '{}' not found",
                    item_exp.label
                ));
            }
            Some(item) => {
                if let Some(ref expected_detail) = item_exp.detail
                    && item.detail.as_deref() != Some(expected_detail.as_str())
                {
                    errors.push(format!(
                        "completion '{}': expected detail '{}', got {:?}",
                        item_exp.label, expected_detail, item.detail
                    ));
                }
                if let Some(ref expected_doc) = item_exp.documentation {
                    let has_doc = item
                        .documentation
                        .as_ref()
                        .is_some_and(|d| d.contains(expected_doc));
                    if !has_doc {
                        errors.push(format!(
                            "completion '{}': expected documentation containing '{}', got {:?}",
                            item_exp.label, expected_doc, item.documentation
                        ));
                    }
                }
            }
        }
    }
}

fn check_hover(
    hover: &Option<styx_lsp_ext::HoverResult>,
    expected: &HoverExpectations,
    errors: &mut Vec<String>,
) {
    match hover {
        None => {
            if !expected.contains.is_empty() {
                errors.push("expected hover content but got none".to_string());
            }
        }
        Some(result) => {
            for substring in &expected.contains {
                if !result.contents.contains(substring) {
                    errors.push(format!(
                        "hover: expected to contain '{}', got: {}",
                        substring, result.contents
                    ));
                }
            }
            for substring in &expected.not_contains {
                if result.contents.contains(substring) {
                    errors.push(format!(
                        "hover: expected NOT to contain '{}', but it did",
                        substring
                    ));
                }
            }
        }
    }
}

fn check_diagnostics(
    diagnostics: &[styx_lsp_ext::Diagnostic],
    expected: &DiagnosticExpectations,
    errors: &mut Vec<String>,
) {
    // Check count if specified
    if let Some(count) = expected.count
        && diagnostics.len() != count as usize
    {
        errors.push(format!(
            "expected {} diagnostics, got {}",
            count,
            diagnostics.len()
        ));
    }

    // Check 'has' expectations
    for exp in &expected.has {
        let found = diagnostics.iter().any(|d| {
            // Check message (empty message matches any diagnostic)
            if !exp.message.is_empty() && !d.message.contains(&exp.message) {
                return false;
            }
            // Check line if specified (convert from 1-indexed to 0-indexed)
            if let Some(line) = exp.line
                && d.range.start.line != line - 1
            {
                return false;
            }
            // Check start column if specified
            if let Some(start_col) = exp.start_col
                && d.range.start.character != start_col
            {
                return false;
            }
            // Check end column if specified
            if let Some(end_col) = exp.end_col
                && d.range.end.character != end_col
            {
                return false;
            }
            true
        });
        if !found {
            // Build a detailed error message showing what we expected vs what we got
            let has_span = exp.start_col.is_some() || exp.end_col.is_some();

            if has_span {
                // Show detailed span information for span marker expectations
                let matching_by_message: Vec<_> = diagnostics
                    .iter()
                    .filter(|d| exp.message.is_empty() || d.message.contains(&exp.message))
                    .collect();

                if matching_by_message.is_empty() {
                    errors.push(format!(
                        "expected diagnostic containing '{}' not found",
                        exp.message
                    ));
                } else {
                    let expected_line = exp.line.map(|l| l - 1).unwrap_or(0);
                    let expected_start = exp.start_col.unwrap_or(0);
                    let expected_end = exp.end_col.unwrap_or(0);

                    let actual_spans: Vec<_> = matching_by_message
                        .iter()
                        .map(|d| {
                            format!(
                                "line {}:{}-{}",
                                d.range.start.line + 1,
                                d.range.start.character,
                                d.range.end.character
                            )
                        })
                        .collect();

                    errors.push(format!(
                        "expected diagnostic '{}' at line {}:{}-{}, found at [{}]",
                        exp.message,
                        expected_line + 1,
                        expected_start,
                        expected_end,
                        actual_spans.join(", ")
                    ));
                }
            } else if let Some(line) = exp.line {
                // Show actual positions for debugging
                let actual_positions: Vec<_> = diagnostics
                    .iter()
                    .filter(|d| d.message.contains(&exp.message))
                    .map(|d| d.range.start.line + 1) // Convert back to 1-indexed
                    .collect();
                if !actual_positions.is_empty() {
                    errors.push(format!(
                        "expected diagnostic '{}' at line {}, found at lines {:?}",
                        exp.message, line, actual_positions
                    ));
                } else {
                    errors.push(format!(
                        "expected diagnostic containing '{}' at line {} not found",
                        exp.message, line
                    ));
                }
            } else {
                errors.push(format!(
                    "expected diagnostic containing '{}' not found",
                    exp.message
                ));
            }
        }
    }

    // Check 'not_has' expectations
    for substring in &expected.not_has {
        let found = diagnostics.iter().any(|d| d.message.contains(substring));
        if found {
            errors.push(format!(
                "unexpected diagnostic containing '{}' was present",
                substring
            ));
        }
    }
}

fn check_inlay_hints(
    hints: &[styx_lsp_ext::InlayHint],
    expected: &InlayHintExpectations,
    errors: &mut Vec<String>,
) {
    // Check count if specified
    if let Some(count) = expected.count
        && hints.len() != count as usize
    {
        errors.push(format!(
            "expected {} inlay hints, got {}",
            count,
            hints.len()
        ));
    }

    // Check 'has' expectations
    for exp in &expected.has {
        let found = hints.iter().any(|h| {
            if h.label != exp.label {
                return false;
            }
            // Check line if specified (convert from 1-indexed to 0-indexed)
            if let Some(line) = exp.line
                && h.position.line != line - 1
            {
                return false;
            }
            // Check column if specified
            if let Some(col) = exp.col
                && h.position.character != col
            {
                return false;
            }
            true
        });
        if !found {
            let has_position = exp.line.is_some() || exp.col.is_some();
            if has_position {
                // Show actual positions for debugging
                let actual_positions: Vec<_> = hints
                    .iter()
                    .filter(|h| h.label == exp.label)
                    .map(|h| format!("line {}:{}", h.position.line + 1, h.position.character))
                    .collect();

                let expected_pos = match (exp.line, exp.col) {
                    (Some(line), Some(col)) => format!("line {}:{}", line, col),
                    (Some(line), None) => format!("line {}", line),
                    (None, Some(col)) => format!("col {}", col),
                    (None, None) => "unknown".to_string(),
                };

                errors.push(format!(
                    "expected inlay hint '{}' at {}, found at [{}]",
                    exp.label,
                    expected_pos,
                    actual_positions.join(", ")
                ));
            } else {
                errors.push(format!("expected inlay hint '{}' not found", exp.label));
            }
        }
    }
}

fn check_definition(
    locations: &[styx_lsp_ext::Location],
    expected: &DefinitionExpectations,
    errors: &mut Vec<String>,
) {
    // Check if we expected empty results
    if expected.empty && !locations.is_empty() {
        errors.push(format!(
            "expected no definition locations, got {}",
            locations.len()
        ));
        return;
    }

    // Check count if specified
    if let Some(count) = expected.count
        && locations.len() != count as usize
    {
        errors.push(format!(
            "expected {} definition locations, got {}",
            count,
            locations.len()
        ));
    }

    // Check 'has' expectations
    for exp in &expected.has {
        let found = locations.iter().any(|loc| {
            // Check line if specified (convert from 1-indexed to 0-indexed)
            if let Some(line) = exp.line
                && loc.range.start.line != line - 1
            {
                return false;
            }
            // Check URI if specified (substring match)
            if let Some(ref uri) = exp.uri
                && !loc.uri.contains(uri)
            {
                return false;
            }
            true
        });
        if !found {
            let desc = match (&exp.line, &exp.uri) {
                (Some(line), Some(uri)) => format!("line {} in '{}'", line, uri),
                (Some(line), None) => format!("line {}", line),
                (None, Some(uri)) => format!("in '{}'", uri),
                (None, None) => "any location".to_string(),
            };
            errors.push(format!(
                "expected definition at {} not found (got: {:?})",
                desc, locations
            ));
        }
    }
}

fn check_code_actions(
    actions: &[styx_lsp_ext::CodeAction],
    expected: &CodeActionExpectations,
    errors: &mut Vec<String>,
) {
    // Check if we expected empty results
    if expected.empty && !actions.is_empty() {
        errors.push(format!("expected no code actions, got {}", actions.len()));
        return;
    }

    // Check count if specified
    if let Some(count) = expected.count
        && actions.len() != count as usize
    {
        errors.push(format!(
            "expected {} code actions, got {}",
            count,
            actions.len()
        ));
    }

    // Check 'has' expectations
    for exp in &expected.has {
        let found = actions.iter().any(|action| {
            // Check title (substring match)
            if !action.title.contains(&exp.title) {
                return false;
            }
            // Check kind if specified
            if let Some(ref kind_str) = exp.kind {
                let kind_matches = match action.kind {
                    Some(styx_lsp_ext::CodeActionKind::QuickFix) => kind_str == "quickfix",
                    Some(styx_lsp_ext::CodeActionKind::Refactor) => kind_str == "refactor",
                    Some(styx_lsp_ext::CodeActionKind::Source) => kind_str == "source",
                    None => false,
                };
                if !kind_matches {
                    return false;
                }
            }
            // Check preferred if specified
            if let Some(preferred) = exp.preferred
                && action.is_preferred != preferred
            {
                return false;
            }
            true
        });
        if !found {
            let action_titles: Vec<_> = actions.iter().map(|a| a.title.as_str()).collect();
            errors.push(format!(
                "expected code action containing '{}' not found (got: {:?})",
                exp.title, action_titles
            ));
        }
    }

    // Check 'not_has' expectations
    for title in &expected.not_has {
        let found = actions.iter().any(|a| a.title.contains(title));
        if found {
            errors.push(format!(
                "unexpected code action containing '{}' was present",
                title
            ));
        }
    }
}

/// Errors from the test runner.
#[derive(Debug)]
pub enum RunnerError {
    ReadFailed(String, String),
    ParseFailed(String, String),
    Harness(HarnessError),
}

impl std::fmt::Display for RunnerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReadFailed(path, e) => write!(f, "failed to read {}: {}", path, e),
            Self::ParseFailed(path, e) => write!(f, "failed to parse {}: {}", path, e),
            Self::Harness(e) => write!(f, "harness error: {}", e),
        }
    }
}

impl std::error::Error for RunnerError {}

/// Assert that all tests in a file pass. Panics with a report if any fail.
///
/// This is the main entry point for use in `#[tokio::test]` functions.
pub async fn assert_test_file(
    bin: &str,
    args: &[&str],
    test_file_path: impl AsRef<Path>,
    schema_id: &str,
) {
    let result = run_test_file(bin, args, test_file_path, schema_id)
        .await
        .expect("failed to run test file");

    if !result.all_passed() {
        panic!("\n{}", result.report());
    }
}

/// Run all tests in a test file with a custom document URI for initialization.
///
/// This variant allows specifying a document URI that points to a real project
/// directory, enabling extensions that need to find config files (like `.config/dibs.styx`)
/// to work correctly.
///
/// # Arguments
/// * `bin` - Path to the extension binary
/// * `args` - Arguments to pass to the binary (e.g., `&["lsp-extension"]`)
/// * `test_file_path` - Path to the `.styx` test file
/// * `schema_id` - Schema ID to use for initialization
/// * `document_uri` - Document URI to use for initialization (e.g., `file:///path/to/project/queries.styx`)
pub async fn run_test_file_with_uri(
    bin: &str,
    args: &[&str],
    test_file_path: impl AsRef<Path>,
    schema_id: &str,
    document_uri: &str,
) -> Result<TestFileResult, RunnerError> {
    let path = test_file_path.as_ref();

    // Read and parse the test file
    let content = std::fs::read_to_string(path)
        .map_err(|e| RunnerError::ReadFailed(path.display().to_string(), e.to_string()))?;

    let test_file: TestFile = facet_styx::from_str(&content)
        .map_err(|e| RunnerError::ParseFailed(path.display().to_string(), e.to_string()))?;

    // Build command
    let mut command = vec![bin];
    command.extend(args);

    // Spawn harness
    let harness = TestHarness::spawn(&command)
        .await
        .map_err(RunnerError::Harness)?;

    // Initialize with the provided document URI
    harness
        .initialize(document_uri, schema_id)
        .await
        .map_err(RunnerError::Harness)?;

    // Run each test case
    let mut results = Vec::new();
    for (i, test_case) in test_file.tests.iter().enumerate() {
        let result = run_test_case(&harness, test_case, i).await;
        results.push(result);
    }

    // Shutdown
    let _ = harness.shutdown().await;

    Ok(TestFileResult {
        path: path.display().to_string(),
        results,
    })
}

/// Assert that all tests in a file pass, using a custom document URI for initialization.
///
/// This variant allows specifying a document URI that points to a real project
/// directory, enabling extensions that need to find config files to work correctly.
///
/// # Arguments
/// * `bin` - Path to the extension binary
/// * `args` - Arguments to pass to the binary (e.g., `&["lsp-extension"]`)
/// * `test_file_path` - Path to the `.styx` test file
/// * `schema_id` - Schema ID to use for initialization
/// * `document_uri` - Document URI to use for initialization (e.g., `file:///path/to/project/queries.styx`)
pub async fn assert_test_file_with_uri(
    bin: &str,
    args: &[&str],
    test_file_path: impl AsRef<Path>,
    schema_id: &str,
    document_uri: &str,
) {
    let result = run_test_file_with_uri(bin, args, test_file_path, schema_id, document_uri)
        .await
        .expect("failed to run test file");

    if !result.all_passed() {
        panic!("\n{}", result.report());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_span_markers_basic() {
        let input = r#"from alisjdf
     ^^^^^^^ "table not found"
select {id}"#;

        let (cleaned, markers) = parse_span_markers(input);

        assert_eq!(cleaned, "from alisjdf\nselect {id}");
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].line, 0); // refers to line 0 after cleaning
        assert_eq!(markers[0].start_col, 5);
        assert_eq!(markers[0].end_col, 12);
        assert_eq!(markers[0].message, "table not found");
    }

    #[test]
    fn test_parse_span_markers_multiple() {
        let input = r#"from bad_table
     ^^^^^^^^^ "unknown table"
select {bad_col}
        ^^^^^^^ "unknown column""#;

        let (cleaned, markers) = parse_span_markers(input);

        assert_eq!(cleaned, "from bad_table\nselect {bad_col}");
        assert_eq!(markers.len(), 2);

        assert_eq!(markers[0].line, 0);
        assert_eq!(markers[0].start_col, 5);
        assert_eq!(markers[0].message, "unknown table");

        assert_eq!(markers[1].line, 1);
        assert_eq!(markers[1].start_col, 8);
        assert_eq!(markers[1].message, "unknown column");
    }

    #[test]
    fn test_parse_span_markers_no_message() {
        let input = r#"from table
     ^^^^^
select {id}"#;

        let (cleaned, markers) = parse_span_markers(input);

        assert_eq!(cleaned, "from table\nselect {id}");
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].message, ""); // empty message matches any
    }

    #[test]
    fn test_parse_span_markers_not_a_marker() {
        // Lines with content before ^ are not markers
        let input = r#"from table
code ^ here
select {id}"#;

        let (cleaned, markers) = parse_span_markers(input);

        assert_eq!(cleaned, input); // unchanged
        assert_eq!(markers.len(), 0);
    }
}
