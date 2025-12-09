//! facet-diff Showcase
//!
//! Demonstrates structural diffing capabilities with miette-based diagnostics.
//!
//! Run with: cargo run -p facet-diff --features miette --example diff_showcase

use facet::Facet;
use facet_diff::tree_diff;
use facet_pretty::{PathSegment, PrettyPrinter};
use facet_reflect::Peek;
use facet_showcase::{Language, OutputMode, ShowcaseRunner, ansi_to_html};
use miette::{Diagnostic, GraphicalReportHandler, GraphicalTheme, LabeledSpan, Severity};
use owo_colors::OwoColorize;
use std::error::Error;
use std::fmt;

/// Maximum number of changes to show before truncating
const MAX_LABELS: usize = 10;

// ============================================================================
// Diagnostic type for diffs
// ============================================================================

struct DiffDiagnostic {
    source_code: String,
    labels: Vec<LabeledSpan>,
    message: String,
    truncated_count: usize,
}

impl fmt::Debug for DiffDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DiffDiagnostic")
    }
}

impl fmt::Display for DiffDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)?;
        if self.truncated_count > 0 {
            write!(f, " (and {} more changes)", self.truncated_count)?;
        }
        Ok(())
    }
}

impl Error for DiffDiagnostic {}

impl Diagnostic for DiffDiagnostic {
    fn source_code(&self) -> Option<&dyn miette::SourceCode> {
        Some(&self.source_code)
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = LabeledSpan> + '_>> {
        Some(Box::new(self.labels.clone().into_iter()))
    }

    fn severity(&self) -> Option<Severity> {
        Some(Severity::Warning)
    }
}

fn render_diagnostic(diag: &dyn Diagnostic) -> String {
    let mut buf = String::new();
    let handler = GraphicalReportHandler::new_themed(GraphicalTheme::unicode());
    handler.render_report(&mut buf, diag).unwrap();
    buf
}

// ============================================================================
// Path conversion between facet-diff and facet-pretty
// ============================================================================

fn convert_path(diff_path: &facet_diff::Path) -> Vec<PathSegment> {
    diff_path
        .0
        .iter()
        .map(|seg| match seg {
            facet_diff::PathSegment::Field(name) => PathSegment::Field(name.clone()),
            facet_diff::PathSegment::Index(i) => PathSegment::Index(*i),
            facet_diff::PathSegment::Key(k) => PathSegment::Key(k.clone()),
            facet_diff::PathSegment::Variant(v) => PathSegment::Variant(v.clone()),
        })
        .collect()
}

// ============================================================================
// Core diff-to-miette logic
// ============================================================================

/// Check if a path is a prefix of another path
fn is_prefix_of(prefix: &facet_diff::Path, path: &facet_diff::Path) -> bool {
    if prefix.0.len() >= path.0.len() {
        return false;
    }
    prefix.0.iter().zip(path.0.iter()).all(|(a, b)| a == b)
}

/// Filter operations to only leaf-level changes (no parent containers)
fn filter_to_leaves(ops: &[facet_diff::EditOp]) -> Vec<&facet_diff::EditOp> {
    let paths: Vec<_> = ops
        .iter()
        .filter_map(|op| match op {
            facet_diff::EditOp::Insert { path, .. } => Some(path),
            facet_diff::EditOp::Delete { path, .. } => Some(path),
            facet_diff::EditOp::Update { path, .. } => Some(path),
            facet_diff::EditOp::Move { old_path, .. } => Some(old_path),
            _ => None,
        })
        .collect();

    ops.iter()
        .filter(|op| {
            let path = match op {
                facet_diff::EditOp::Insert { path, .. } => path,
                facet_diff::EditOp::Delete { path, .. } => path,
                facet_diff::EditOp::Update { path, .. } => path,
                facet_diff::EditOp::Move { old_path, .. } => old_path,
                _ => return false,
            };
            // Keep this op only if no other op has this path as a prefix
            !paths.iter().any(|other| is_prefix_of(path, other))
        })
        .collect()
}

/// Build a miette diagnostic from a diff between two values
fn build_diff_diagnostic<'a, T: facet::Facet<'a>>(
    before: &'a T,
    after: &'a T,
    message: &str,
) -> DiffDiagnostic {
    let printer = PrettyPrinter::new();
    let formatted_before = printer.format_peek_with_spans(Peek::new(before));
    let formatted_after = printer.format_peek_with_spans(Peek::new(after));

    let all_ops = tree_diff(before, after);
    let ops = filter_to_leaves(&all_ops);

    let mut labels = Vec::new();
    let mut truncated_count = 0;

    for op in ops {
        if labels.len() >= MAX_LABELS {
            truncated_count += 1;
            continue;
        }

        match op {
            facet_diff::EditOp::Insert { path, .. } => {
                let pretty_path = convert_path(path);
                // For inserts, show in the "after" formatted value
                // But we're showing "before", so we note that this was added
                if let Some(after_span) = formatted_after.spans.get(&pretty_path) {
                    let after_text = &formatted_after.text[after_span.value.0..after_span.value.1];
                    // Find corresponding location in before, or use root
                    if let Some(before_span) = formatted_before.spans.get(&pretty_path) {
                        labels.push(LabeledSpan::new(
                            Some(format!("→ {}", after_text)),
                            before_span.value.0,
                            before_span.value.1 - before_span.value.0,
                        ));
                    }
                }
            }
            facet_diff::EditOp::Delete { path, .. } => {
                let pretty_path = convert_path(path);
                if let Some(span) = formatted_before.spans.get(&pretty_path) {
                    labels.push(LabeledSpan::new(
                        Some("removed".to_string()),
                        span.value.0,
                        span.value.1 - span.value.0,
                    ));
                }
            }
            facet_diff::EditOp::Update { path, .. } => {
                let pretty_path = convert_path(path);
                if let Some(before_span) = formatted_before.spans.get(&pretty_path) {
                    let after_text =
                        if let Some(after_span) = formatted_after.spans.get(&pretty_path) {
                            formatted_after.text[after_span.value.0..after_span.value.1].to_string()
                        } else {
                            "?".to_string()
                        };
                    labels.push(LabeledSpan::new(
                        Some(format!("→ {}", after_text)),
                        before_span.value.0,
                        before_span.value.1 - before_span.value.0,
                    ));
                }
            }
            facet_diff::EditOp::Move { .. } => {
                // Moves are complex - skip for now
            }
            _ => {
                // Handle any future variants
            }
        }
    }

    DiffDiagnostic {
        source_code: formatted_before.text,
        labels,
        message: message.to_string(),
        truncated_count,
    }
}

// ============================================================================
// Test data structures
// ============================================================================

#[derive(Facet, Clone)]
struct Config {
    name: String,
    version: u32,
    debug: bool,
    features: Vec<String>,
}

#[derive(Facet, Clone)]
struct User {
    name: String,
    email: Option<String>,
    age: u32,
}

/// A deeply nested structure for testing deep trees
#[derive(Facet, Clone)]
struct DeepTree {
    level1: Level1,
}

#[derive(Facet, Clone)]
struct Level1 {
    level2: Level2,
}

#[derive(Facet, Clone)]
struct Level2 {
    level3: Level3,
}

#[derive(Facet, Clone)]
struct Level3 {
    level4: Level4,
}

#[derive(Facet, Clone)]
struct Level4 {
    value: String,
}

// ============================================================================
// Showcase scenarios
// ============================================================================

fn main() {
    let mut runner = ShowcaseRunner::new("Structural Diff with Miette").language(Language::Rust);
    let mode = runner.mode();

    runner.header();
    runner.intro(
        "[`facet-diff`](https://docs.rs/facet-diff) provides structural diffing for any `Facet` type, \
        with miette-powered diagnostics that point directly to what changed."
    );

    scenario_basic_struct(&mut runner, mode);
    scenario_deep_tree(&mut runner, mode);
    scenario_wide_list(&mut runner, mode);
    scenario_many_changes(&mut runner, mode);

    runner.footer();
}

fn scenario_basic_struct(runner: &mut ShowcaseRunner, mode: OutputMode) {
    let before = User {
        name: "Alice".into(),
        email: Some("alice@example.com".into()),
        age: 30,
    };

    let after = User {
        name: "Alice".into(),
        email: Some("alice@newdomain.com".into()),
        age: 31,
    };

    let diag = build_diff_diagnostic(&before, &after, "User changed");
    let rendered = render_diagnostic(&diag);

    print_scenario(
        runner,
        mode,
        "Basic Struct Changes",
        "Simple struct with a few field changes. Miette points to each changed value.",
        &rendered,
    );
}

fn scenario_deep_tree(runner: &mut ShowcaseRunner, mode: OutputMode) {
    let before = DeepTree {
        level1: Level1 {
            level2: Level2 {
                level3: Level3 {
                    level4: Level4 {
                        value: "original".into(),
                    },
                },
            },
        },
    };

    let after = DeepTree {
        level1: Level1 {
            level2: Level2 {
                level3: Level3 {
                    level4: Level4 {
                        value: "modified".into(),
                    },
                },
            },
        },
    };

    let diag = build_diff_diagnostic(&before, &after, "Deep tree changed");
    let rendered = render_diagnostic(&diag);

    print_scenario(
        runner,
        mode,
        "Deep Tree",
        "A change deep in a nested structure. Note: the tree diff algorithm currently \
        reports changes at the struct level rather than the leaf level, so the entire \
        subtree is shown as changed.",
        &rendered,
    );
}

fn scenario_wide_list(runner: &mut ShowcaseRunner, mode: OutputMode) {
    // A list with many items, but only one changes
    let before: Vec<i32> = (0..20).collect();
    let mut after = before.clone();
    after[15] = 999;

    let diag = build_diff_diagnostic(&before, &after, "List element changed");
    let rendered = render_diagnostic(&diag);

    print_scenario(
        runner,
        mode,
        "Wide List (Single Change)",
        "A list with 20 elements where only one changes. Miette highlights just the changed element.",
        &rendered,
    );
}

fn scenario_many_changes(runner: &mut ShowcaseRunner, mode: OutputMode) {
    // Many changes to trigger truncation
    let before: Vec<i32> = (0..30).collect();
    let after: Vec<i32> = (0..30)
        .map(|i| if i % 2 == 0 { i * 10 } else { i })
        .collect();

    let diag = build_diff_diagnostic(&before, &after, "Many list changes");
    let rendered = render_diagnostic(&diag);

    print_scenario(
        runner,
        mode,
        "Many Changes (Truncated)",
        &format!(
            "When there are more than {} changes, we truncate and show a count. \
            This prevents overwhelming output for large diffs.",
            MAX_LABELS
        ),
        &rendered,
    );
}

fn print_scenario(
    _runner: &mut ShowcaseRunner,
    mode: OutputMode,
    name: &str,
    description: &str,
    rendered_diff: &str,
) {
    match mode {
        OutputMode::Terminal => {
            println!();
            println!("{}", "═".repeat(78).dimmed());
            println!("{} {}", "SCENARIO:".bold().cyan(), name.bold().white());
            println!("{}", "─".repeat(78).dimmed());
            println!("{}", description.dimmed());
            println!("{}", "═".repeat(78).dimmed());
            println!();
            print!("{rendered_diff}");
        }
        OutputMode::Markdown => {
            println!();
            println!("## {name}");
            println!();
            println!("<section class=\"scenario\">");
            println!("<p class=\"description\">{description}</p>");
            println!("<div class=\"diff-output\">");
            println!("<pre><code>{}</code></pre>", ansi_to_html(rendered_diff));
            println!("</div>");
            println!("</section>");
        }
    }
}
