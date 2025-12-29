//! Spec coverage tests.
//!
//! This test suite generates one test per spec rule. Each test fails unless
//! the rule is covered by either:
//! - A conformance test (from `rapace-conformance --list --format json`)
//! - An `[impl ...]` annotation in implementation code
//!
//! Run with:
//!   cargo nextest run -p rapace-conformance --test coverage --no-fail-fast

use facet::Facet;
use libtest_mimic::{Arguments, Failed, Trial};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::Path;
use std::process::Command;

/// The _rules.json format from dodeca.
#[derive(Facet)]
struct RulesManifest {
    rules: BTreeMap<String, RuleInfo>,
}

/// Info about a single rule.
#[derive(Facet)]
struct RuleInfo {
    url: String,
}

/// Test case from conformance harness.
#[derive(Facet)]
struct TestCase {
    name: String,
    rules: Vec<String>,
}

fn main() {
    let args = Arguments::from_args();

    // Load rules from _rules.json
    let rules_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("docs/public/_rules.json");

    let rules: Vec<String> = if rules_path.exists() {
        let content = fs::read_to_string(&rules_path).expect("failed to read _rules.json");
        let manifest: RulesManifest =
            facet_json::from_str(&content).expect("failed to parse _rules.json");
        manifest.rules.into_keys().collect()
    } else {
        eprintln!(
            "Warning: _rules.json not found at {:?}. Run `ddc build` first.",
            rules_path
        );
        Vec::new()
    };

    // Get covered rules from conformance harness and implementation annotations
    let covered = get_covered_rules();

    // Create a test for each rule - NO IGNORING, let them fail
    let trials: Vec<Trial> = rules
        .into_iter()
        .map(|rule_id| {
            let is_covered = covered.contains(&rule_id);
            let rule_id_clone = rule_id.clone();

            Trial::test(format!("rule.{}", rule_id), move || {
                if is_covered {
                    Ok(())
                } else {
                    Err(Failed::from(format!(
                        "Rule '{}' has no conformance test or [impl ...] annotation",
                        rule_id_clone
                    )))
                }
            })
        })
        .collect();

    libtest_mimic::run(&args, trials).exit();
}

/// Get the set of covered rules from conformance harness and code annotations.
fn get_covered_rules() -> HashSet<String> {
    let mut covered = HashSet::new();

    // 1. Spawn conformance harness to get test list with rules
    let conformance_bin = env!("CARGO_BIN_EXE_rapace-conformance");
    if let Some(output) = Command::new(conformance_bin)
        .args(["--list", "--format", "json"])
        .output()
        .ok()
        .filter(|o| o.status.success())
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        if let Ok(tests) = facet_json::from_str::<Vec<TestCase>>(&stdout) {
            for test in tests {
                for rule in test.rules {
                    covered.insert(rule);
                }
            }
        }
    }

    // 2. Scan implementation code for [impl ...] annotations
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    scan_for_impl_annotations(workspace_root, &mut covered);

    covered
}

/// Recursively scan for [impl ...] patterns in implementation code.
fn scan_for_impl_annotations(dir: &Path, covered: &mut HashSet<String>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();

        // Skip hidden dirs, target, node_modules, conformance (we get that from the binary)
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.starts_with('.')
            || name == "target"
            || name == "node_modules"
            || name == "conformance"
        {
            continue;
        }

        if path.is_dir() {
            scan_for_impl_annotations(&path, covered);
        } else if path.extension().is_some_and(|e| e == "rs") {
            scan_file(&path, covered);
        }
    }
}

/// Scan a single file for [impl ...] annotations.
fn scan_file(path: &Path, covered: &mut HashSet<String>) {
    let Ok(content) = fs::read_to_string(path) else {
        return;
    };

    // Match [impl rule.id] patterns (implementation annotations)
    let impl_re = regex::Regex::new(r"\[impl ([a-z][a-z0-9._-]+)\]").unwrap();

    for cap in impl_re.captures_iter(&content) {
        covered.insert(cap[1].to_string());
    }
}
