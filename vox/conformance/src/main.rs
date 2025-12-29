//! Rapace conformance reference peer.
//!
//! This binary acts as a reference peer for conformance testing.
//! Implementations spawn this peer and communicate via stdin/stdout.
//!
//! # Usage
//!
//! Run a specific test case:
//! ```bash
//! rapace-conformance --case handshake.valid_hello_exchange
//! ```
//!
//! List all test cases:
//! ```bash
//! rapace-conformance --list
//! ```
//!
//! List test cases for a category:
//! ```bash
//! rapace-conformance --list --category handshake
//! ```
//!
//! Show rules covered by tests:
//! ```bash
//! rapace-conformance --list --show-rules
//! ```
//!
//! # Exit Codes
//!
//! - 0: Test passed
//! - 1: Test failed (protocol violation)
//! - 2: Internal error (bug in peer)

use clap::Parser;
use facet::Facet;
use rapace_conformance::tests;

#[derive(Parser, Debug)]
#[command(name = "rapace-conformance")]
#[command(about = "Rapace protocol conformance reference peer")]
struct Args {
    /// Run a specific test case (e.g., "handshake.valid_hello_exchange")
    #[arg(long)]
    case: Option<String>,

    /// List available test cases
    #[arg(long)]
    list: bool,

    /// Filter by category (handshake, frame, channel, call, control, error)
    #[arg(long)]
    category: Option<String>,

    /// Show spec rules covered by each test
    #[arg(long)]
    show_rules: bool,

    /// Output format (text, json)
    #[arg(long, default_value = "text")]
    format: String,
}

/// JSON output for a test case listing.
#[derive(Facet)]
struct TestCaseJson {
    name: String,
    rules: Vec<String>,
}

/// JSON output for a test result.
#[derive(Facet)]
struct TestResultJson {
    test: String,
    passed: bool,
    error: Option<String>,
}

fn main() {
    let args = Args::parse();

    if args.list {
        list_tests(&args);
        return;
    }

    if let Some(case) = &args.case {
        run_test(case, &args);
    } else {
        eprintln!("Usage: rapace-conformance --case <test_name>");
        eprintln!("       rapace-conformance --list");
        std::process::exit(2);
    }
}

fn list_tests(args: &Args) {
    let tests = if let Some(category) = &args.category {
        tests::list_category(category)
    } else {
        tests::list_all()
    };

    if args.format == "json" {
        let output: Vec<TestCaseJson> = tests
            .iter()
            .map(|(name, rules)| TestCaseJson {
                name: name.clone(),
                rules: rules.iter().map(|s| s.to_string()).collect(),
            })
            .collect();
        println!("{}", facet_json::to_string(&output));
    } else {
        println!("Available test cases:\n");

        let mut current_category = "";
        for (name, rules) in &tests {
            let category = name.split('.').next().unwrap_or("");
            if category != current_category {
                if !current_category.is_empty() {
                    println!();
                }
                println!("## {}", category);
                current_category = category;
            }

            if args.show_rules {
                println!("  {} [{}]", name, rules.join(", "));
            } else {
                println!("  {}", name);
            }
        }

        println!("\nTotal: {} tests", tests.len());

        // Count unique rules
        let mut all_rules: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for (_, rules) in &tests {
            all_rules.extend(rules.iter().copied());
        }
        println!("Rules covered: {}", all_rules.len());
    }
}

fn run_test(case: &str, args: &Args) {
    let result = tests::run(case);

    if args.format == "json" {
        let output = TestResultJson {
            test: case.to_string(),
            passed: result.passed,
            error: result.error.clone(),
        };
        println!("{}", facet_json::to_string(&output));
    } else if !result.passed
        && let Some(error) = &result.error
    {
        eprintln!("FAIL: {}", case);
        eprintln!("  {}", error);
    }

    std::process::exit(if result.passed { 0 } else { 1 });
}
