//! Quick spike to see what a miette-based diff might look like
//!
//! Run with: cargo run -p facet-diff --features miette --example miette_diff

use facet::Facet;
use facet_diff::tree_diff;
use facet_pretty::PrettyPrinter;
use facet_reflect::Peek;
use miette::{
    Diagnostic, GraphicalReportHandler, GraphicalTheme, LabeledSpan, NamedSource, Severity,
};
use std::error::Error;
use std::fmt;

#[derive(Facet, Debug, Clone)]
struct Person {
    name: String,
    age: u32,
    email: String,
}

/// A simple diagnostic showing a single change
struct DiffDiagnostic {
    source_name: String,
    source_code: String,
    labels: Vec<LabeledSpan>,
    message: String,
}

impl fmt::Debug for DiffDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DiffDiagnostic")
    }
}

impl fmt::Display for DiffDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
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

fn render(diag: &dyn Diagnostic) -> String {
    let mut buf = String::new();
    let handler = GraphicalReportHandler::new_themed(GraphicalTheme::unicode());
    handler.render_report(&mut buf, diag).unwrap();
    buf
}

fn main() {
    let before = Person {
        name: "Alice".to_string(),
        age: 30,
        email: "alice@example.com".to_string(),
    };

    let after = Person {
        name: "Alice".to_string(),
        age: 31,                                  // Changed!
        email: "alice@newdomain.com".to_string(), // Changed!
    };

    // Get the edit operations
    let ops = tree_diff(&before, &after);

    println!("=== Edit Operations ===");
    for op in &ops {
        println!("  {:?}", op);
    }
    println!();

    // Pretty print with spans
    let printer = PrettyPrinter::new();
    let formatted_before = printer.format_peek_with_spans(Peek::new(&before));
    let formatted_after = printer.format_peek_with_spans(Peek::new(&after));

    println!("=== Before (with spans) ===");
    println!("{}", formatted_before.text);
    println!("\nSpans:");
    for (path, span) in &formatted_before.spans {
        println!("  {:?} => key:{:?} value:{:?}", path, span.key, span.value);
    }
    println!();

    println!("=== After (with spans) ===");
    println!("{}", formatted_after.text);
    println!();

    // Now let's try showing this with miette
    // We'll show the "before" value with the changed fields highlighted

    let mut labels = Vec::new();

    // Find spans for fields that changed
    // For now, let's just manually highlight the age and email fields
    use facet_pretty::PathSegment;
    use std::borrow::Cow;

    let age_path = vec![PathSegment::Field(Cow::Borrowed("age"))];
    let email_path = vec![PathSegment::Field(Cow::Borrowed("email"))];

    if let Some(span) = formatted_before.spans.get(&age_path) {
        labels.push(LabeledSpan::new(
            Some("→ 31".to_string()),
            span.value.0,
            span.value.1 - span.value.0,
        ));
    }

    if let Some(span) = formatted_before.spans.get(&email_path) {
        // For strings, let's try to narrow down to the differing characters
        // The value span includes the quotes, so "alice@example.com" is at span.value
        // Let's find where the strings actually differ
        let before_str = "alice@example.com";
        let after_str = "alice@newdomain.com";

        // Find common prefix length
        let prefix_len = before_str
            .chars()
            .zip(after_str.chars())
            .take_while(|(a, b)| a == b)
            .count();

        // Find common suffix length (from the parts that differ)
        let before_rest: String = before_str.chars().skip(prefix_len).collect();
        let after_rest: String = after_str.chars().skip(prefix_len).collect();
        let suffix_len = before_rest
            .chars()
            .rev()
            .zip(after_rest.chars().rev())
            .take_while(|(a, b)| a == b)
            .count();

        // The differing part in "before" is from prefix_len to (len - suffix_len)
        let diff_start = prefix_len;
        let diff_end = before_str.len() - suffix_len;

        // Adjust span: +1 for opening quote, then add diff_start
        let narrow_start = span.value.0 + 1 + diff_start;
        let narrow_end = span.value.0 + 1 + diff_end;

        // Extract the "after" differing part for the label
        let after_diff: String = after_str
            .chars()
            .skip(prefix_len)
            .take(after_str.len() - prefix_len - suffix_len)
            .collect();

        labels.push(LabeledSpan::new(
            Some(format!("→ {}", after_diff)),
            narrow_start,
            narrow_end - narrow_start,
        ));
    }

    let diag = DiffDiagnostic {
        source_name: "before".to_string(),
        source_code: formatted_before.text.clone(),
        labels,
        message: "Values differ".to_string(),
    };

    println!("=== Miette Diagnostic (showing 'before' with changes highlighted) ===");
    println!("{}", render(&diag));
}
