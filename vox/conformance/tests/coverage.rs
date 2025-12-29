//! Spec coverage tests.
//!
//! This test suite generates one test per spec rule. Each test fails unless
//! the rule has an `[impl ...]` or `[verify ...]` annotation in the codebase.
//!
//! These tests are marked as ignored by default because they're expected to fail
//! until we achieve full spec coverage. Run them explicitly with:
//!
//!   cargo nextest run -p rapace-conformance --test coverage --run-ignored=all --no-fail-fast

use facet::Facet;
use libtest_mimic::{Arguments, Failed, Trial};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::Path;

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

    // Get covered rules by scanning the codebase
    let covered = get_covered_rules();

    // Create a test for each rule (all ignored by default)
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
                        "Rule '{}' has no [impl ...] or [verify ...] annotation in the codebase",
                        rule_id_clone
                    )))
                }
            })
            .with_ignored_flag(true)
        })
        .collect();

    libtest_mimic::run(&args, trials).exit();
}

/// Get the set of covered rules by scanning the codebase.
fn get_covered_rules() -> HashSet<String> {
    let mut covered = HashSet::new();

    // Scan files directly for [impl ...] and [verify ...] patterns
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    scan_for_rules(workspace_root, &mut covered);

    covered
}

/// Recursively scan for [impl ...] and [verify ...] patterns.
fn scan_for_rules(dir: &Path, covered: &mut HashSet<String>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();

        // Skip hidden dirs, target, node_modules, etc.
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.starts_with('.') || name == "target" || name == "node_modules" {
            continue;
        }

        if path.is_dir() {
            scan_for_rules(&path, covered);
        } else if path.extension().is_some_and(|e| e == "rs") {
            scan_file(&path, covered);
        }
    }
}

/// Scan a single file for rule references.
fn scan_file(path: &Path, covered: &mut HashSet<String>) {
    let Ok(content) = fs::read_to_string(path) else {
        return;
    };

    // Match patterns like: [impl cancel.deadline.field] or [verify core.stream.intro]
    let impl_re = regex::Regex::new(r"\[impl ([a-z][a-z0-9._-]+)\]").unwrap();
    let verify_re = regex::Regex::new(r"\[verify ([a-z][a-z0-9._-]+)\]").unwrap();

    for cap in impl_re.captures_iter(&content) {
        covered.insert(cap[1].to_string());
    }

    for cap in verify_re.captures_iter(&content) {
        covered.insert(cap[1].to_string());
    }
}
