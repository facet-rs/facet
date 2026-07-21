#![cfg(target_os = "macos")]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use vix::reloc_selection::{
    Analysis, SelectionOutcome, analyze_target_deps, compare_test_reachability,
};

const TESTS: &[&str] = &[
    "local_arithmetic_is_isolated",
    "local_string_is_isolated",
    "local_table_is_isolated",
    "hash_direct_uses_lib_a",
    "hash_pipeline_uses_lib_a",
    "generic_instantiation_uses_lib_a",
];

#[derive(Clone, Copy, Debug)]
enum EditKind {
    Baseline,
    HashBody,
    GenericBody,
    LineCommentOnly,
}

#[test]
fn relocation_walk_selects_only_reaching_tests_and_masks_panic_locations() {
    let root = tempfile::Builder::new()
        .prefix("vix-reloc-selection-")
        .tempdir()
        .expect("temp fixture root");
    let baseline = build_scenario(root.path(), "baseline", EditKind::Baseline);
    baseline.cleanup_target();

    let hash_body = build_scenario(root.path(), "hash-body", EditKind::HashBody);
    assert_selection(
        &baseline.analysis,
        &hash_body.analysis,
        [
            ("local_arithmetic_is_isolated", false),
            ("local_string_is_isolated", false),
            ("local_table_is_isolated", false),
            ("hash_direct_uses_lib_a", true),
            ("hash_pipeline_uses_lib_a", true),
            ("generic_instantiation_uses_lib_a", false),
        ],
    );
    run_skipped_tests(
        &hash_body.dir,
        &hash_body.test_binary,
        &selected_tests(&baseline.analysis, &hash_body.analysis, false),
    );
    hash_body.cleanup_target();

    let generic_body = build_scenario(root.path(), "generic-body", EditKind::GenericBody);
    assert_selection(
        &baseline.analysis,
        &generic_body.analysis,
        [
            ("local_arithmetic_is_isolated", false),
            ("local_string_is_isolated", false),
            ("local_table_is_isolated", false),
            ("hash_direct_uses_lib_a", false),
            ("hash_pipeline_uses_lib_a", false),
            ("generic_instantiation_uses_lib_a", true),
        ],
    );
    run_skipped_tests(
        &generic_body.dir,
        &generic_body.test_binary,
        &selected_tests(&baseline.analysis, &generic_body.analysis, false),
    );
    generic_body.cleanup_target();

    let line_comment = build_scenario(root.path(), "line-comment", EditKind::LineCommentOnly);
    assert!(
        !line_comment.analysis.masked_edges.is_empty(),
        "line-comment scenario should identify panic Location metadata edges"
    );
    assert_selection(
        &baseline.analysis,
        &line_comment.analysis,
        [
            ("local_arithmetic_is_isolated", false),
            ("local_string_is_isolated", false),
            ("local_table_is_isolated", false),
            ("hash_direct_uses_lib_a", false),
            ("hash_pipeline_uses_lib_a", false),
            ("generic_instantiation_uses_lib_a", false),
        ],
    );
    run_skipped_tests(
        &line_comment.dir,
        &line_comment.test_binary,
        &selected_tests(&baseline.analysis, &line_comment.analysis, false),
    );
    line_comment.cleanup_target();
}

struct ScenarioBuild {
    dir: PathBuf,
    test_binary: PathBuf,
    analysis: Analysis,
}

impl ScenarioBuild {
    fn cleanup_target(&self) {
        let target = self.dir.join("target");
        if target.exists() {
            fs::remove_dir_all(&target).expect("remove generated fixture target");
        }
    }
}

fn build_scenario(root: &Path, name: &str, edit: EditKind) -> ScenarioBuild {
    let dir = root.join(name);
    write_fixture(&dir).expect("write fixture");
    apply_edit(&dir, edit).expect("apply edit");
    let target_dir = dir.join("target");
    let rustflags = "-C debuginfo=0 -C split-debuginfo=off -C save-temps=yes -C codegen-units=8 -C link-dead-code=no -C symbol-mangling-version=v0";
    let output = Command::new("cargo")
        .arg("test")
        .arg("--no-run")
        .arg("-p")
        .arg("test_crate")
        .arg("--test")
        .arg("selection")
        .env("CARGO_TARGET_DIR", &target_dir)
        .env("CARGO_INCREMENTAL", "0")
        .env("RUSTFLAGS", rustflags)
        .current_dir(&dir)
        .output()
        .expect("spawn cargo build");
    assert!(
        output.status.success(),
        "{name} build failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let deps_dir = target_dir.join("debug/deps");
    let analysis = analyze_target_deps(&deps_dir).expect("analyze target deps");
    assert!(
        !analysis.atoms.is_empty(),
        "{name} produced no analyzable atoms"
    );
    for test in TESTS {
        analysis
            .find_test_root(test)
            .unwrap_or_else(|| panic!("{name} missing test root {test}"));
    }
    let test_binary = find_test_binary(&deps_dir).expect("test binary");
    ScenarioBuild {
        dir,
        test_binary,
        analysis,
    }
}

fn assert_selection<const N: usize>(
    before: &Analysis,
    after: &Analysis,
    expected: [(&str, bool); N],
) {
    for (test, should_run) in expected {
        let outcome = compare_test_reachability(before, after, test);
        match (should_run, outcome) {
            (true, SelectionOutcome::Reached { changed_hashes }) => {
                assert!(
                    !changed_hashes.is_empty(),
                    "{test} reached no changed hashes"
                );
            }
            (false, SelectionOutcome::NotReached) => {}
            (expected, other) => panic!("{test}: expected run={expected}, got {other:?}"),
        }
    }
}

fn selected_tests(before: &Analysis, after: &Analysis, selected: bool) -> Vec<&'static str> {
    TESTS
        .iter()
        .copied()
        .filter(|test| {
            matches!(
                (selected, compare_test_reachability(before, after, test)),
                (true, SelectionOutcome::Reached { .. })
                    | (true, SelectionOutcome::UnknownRun { .. })
                    | (false, SelectionOutcome::NotReached)
            )
        })
        .collect()
}

fn run_skipped_tests(scenario_dir: &Path, binary: &Path, tests: &[&str]) {
    assert!(!tests.is_empty(), "expected skipped tests to execute");
    for test in tests {
        let output = Command::new(binary)
            .arg("--exact")
            .arg(test)
            .arg("--nocapture")
            .current_dir(scenario_dir)
            .output()
            .unwrap_or_else(|err| panic!("run skipped test {test}: {err}"));
        assert!(
            output.status.success(),
            "skipped test {test} failed:\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

fn find_test_binary(deps_dir: &Path) -> Result<PathBuf, String> {
    let mut candidates = Vec::new();
    for entry in fs::read_dir(deps_dir).map_err(|err| err.to_string())? {
        let entry = entry.map_err(|err| err.to_string())?;
        let path = entry.path();
        if entry.file_type().map_err(|err| err.to_string())?.is_file()
            && path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("selection-") && !name.ends_with(".d"))
            && path.extension().is_none()
        {
            candidates.push(path);
        }
    }
    candidates.sort();
    candidates
        .pop()
        .ok_or_else(|| format!("no selection test binary in {deps_dir:?}"))
}

fn write_fixture(root: &Path) -> Result<(), String> {
    let files: BTreeMap<&str, &str> = BTreeMap::from([
        ("Cargo.toml", WORKSPACE_MANIFEST),
        ("lib_a/Cargo.toml", LIB_A_MANIFEST),
        ("lib_a/src/lib.rs", LIB_A_SOURCE),
        ("test_crate/Cargo.toml", TEST_CRATE_MANIFEST),
        ("test_crate/src/lib.rs", TEST_CRATE_SOURCE),
        ("test_crate/tests/selection.rs", TEST_CRATE_SELECTION_TESTS),
    ]);
    for (relative, contents) in files {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        fs::write(path, contents).map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn apply_edit(root: &Path, edit: EditKind) -> Result<(), String> {
    let lib_path = root.join("lib_a/src/lib.rs");
    let mut source = fs::read_to_string(&lib_path).map_err(|err| err.to_string())?;
    match edit {
        EditKind::Baseline => {}
        EditKind::HashBody => {
            source = source.replace(
                "let rotated = input.rotate_left(7);\n    rotated ^ 0x9e37_79b9_7f4a_7c15",
                "let rotated = input.rotate_left(13);\n    rotated.wrapping_add(0xa076_1d64_78bd_642f)",
            );
        }
        EditKind::GenericBody => {
            source = source.replace(
                "let value = input.into();\n    value.wrapping_mul(41).rotate_left(3) ^ 0xfeed_face_cafe_babe",
                "let value = input.into();\n    value.wrapping_mul(97).rotate_left(5) ^ 0x0123_4567_89ab_cdef",
            );
        }
        EditKind::LineCommentOnly => {
            source = source.replace(
                "\npub fn hash_stuff",
                "\n// shifts panic Location line metadata without changing executed values\npub fn hash_stuff",
            );
        }
    }
    fs::write(lib_path, source).map_err(|err| err.to_string())
}

const WORKSPACE_MANIFEST: &str = r#"
[workspace]
members = ["lib_a", "test_crate"]
resolver = "3"
"#;

const LIB_A_MANIFEST: &str = r#"
[package]
name = "lib_a"
version = "0.1.0"
edition = "2024"
"#;

const TEST_CRATE_MANIFEST: &str = r#"
[package]
name = "test_crate"
version = "0.1.0"
edition = "2024"

[dependencies]
lib_a = { path = "../lib_a" }
"#;

const LIB_A_SOURCE: &str = r#"
pub fn hash_stuff(input: u64) -> u64 {
    let rotated = input.rotate_left(7);
    rotated ^ 0x9e37_79b9_7f4a_7c15
}

pub fn stable_mix(input: u64) -> u64 {
    input.wrapping_mul(13).rotate_right(5) ^ 0x55aa_55aa_1234_5678
}

pub fn hash_pipeline(input: u64) -> u64 {
    hash_stuff(input).wrapping_add(stable_mix(input ^ 0x55aa))
}

pub fn generic_fold<T: Into<u64>>(input: T) -> u64 {
    let value = input.into();
    value.wrapping_mul(41).rotate_left(3) ^ 0xfeed_face_cafe_babe
}
"#;

const TEST_CRATE_SOURCE: &str = r#"
pub fn local_double(value: u64) -> u64 {
    value * 2
}

pub fn local_word_score(word: &str) -> u64 {
    let bytes = word.as_bytes();
    u64::from(bytes[0]) + u64::from(bytes[1]) + u64::from(bytes[2]) + u64::from(bytes[3]) + u64::from(bytes[4])
}

pub fn local_table_pick(index: usize) -> usize {
    match index {
        0 => 0,
        1 | 2 => 1,
        3 => 2,
        4 => 3,
        5 => 5,
        6 => 8,
        _ => 13,
    }
}
"#;

const TEST_CRATE_SELECTION_TESTS: &str = r#"
use test_crate::{local_double, local_table_pick, local_word_score};

#[test]
fn local_arithmetic_is_isolated() {
    assert_eq!(local_double(21), 42);
}

#[test]
fn local_string_is_isolated() {
    assert_eq!(local_word_score("facet"), 515);
}

#[test]
fn local_table_is_isolated() {
    assert_eq!(local_table_pick(5), 5);
}

#[test]
fn hash_direct_uses_lib_a() {
    assert_eq!(lib_a::hash_stuff(7), 0x9e37_79b9_7f4a_7c15 ^ 896);
}

#[test]
fn hash_pipeline_uses_lib_a() {
    let expected = lib_a::hash_stuff(9).wrapping_add(lib_a::stable_mix(9 ^ 0x55aa));
    assert_eq!(lib_a::hash_pipeline(9), expected);
}

#[test]
fn generic_instantiation_uses_lib_a() {
    assert_eq!(
        lib_a::generic_fold(11_u64),
        11_u64.wrapping_mul(41).rotate_left(3) ^ 0xfeed_face_cafe_babe
    );
}
"#;
