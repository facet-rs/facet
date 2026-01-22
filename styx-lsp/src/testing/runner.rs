//! Test runner for LSP extension test files.
//!
//! Loads test files (in styx format), spawns the extension, runs all test cases,
//! and reports results.

use std::path::Path;

use styx_lsp_test_schema::{
    CompletionExpectations, DefinitionExpectations, DiagnosticExpectations, HoverExpectations,
    InlayHintExpectations, TestCase, TestFile,
};

use super::{HarnessError, TestDocument, TestHarness};

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

    let test_file: TestFile = facet_styx::from_str(&content)
        .map_err(|e| RunnerError::ParseFailed(path.display().to_string(), e.to_string()))?;

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

    // Load the document
    let doc = TestDocument::new(&uri, &test_case.input);
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

    // Check diagnostics if expected
    if let Some(ref expected) = test_case.diagnostics {
        match harness.diagnostics(&uri).await {
            Ok(diagnostics) => {
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

    TestResult {
        name,
        passed: errors.is_empty(),
        error: if errors.is_empty() {
            None
        } else {
            Some(errors.join("\n"))
        },
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
        let found = diagnostics.iter().any(|d| d.message.contains(&exp.message));
        if !found {
            errors.push(format!(
                "expected diagnostic containing '{}' not found",
                exp.message
            ));
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
        let found = hints.iter().any(|h| h.label == exp.label);
        if !found {
            errors.push(format!("expected inlay hint '{}' not found", exp.label));
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
