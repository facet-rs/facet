//! `must_use` sees through branch tails.
//!
//! A collection `+` in the tail position of an `if` or `match` arm is a RESULT,
//! not a discarded value — the arm's output is reached through the branching
//! op's region (`MatchArm::output` / `ControlRegion::output`), never through
//! `node.inputs`, so a lint that walks inputs alone reads every one of them as
//! dropped. That false positive is not academic: the vixen stdlib is injected
//! into every compilation, so two branch-tail `+`s in `stdlib/strings.vix` put
//! a spurious warning on programs that never mention them.
//!
//! Rung 144 pins the true positive. This pins the negative half, and the two
//! together are what say the lint means anything.

use vixen_runtime::default_compiler;

fn warnings(source: &str) -> Vec<String> {
    let compilation = default_compiler()
        .compile(source)
        .unwrap_or_else(|diagnostics| panic!("compiles: {diagnostics:#?}"));
    compilation
        .warnings
        .entries
        .iter()
        .map(|warning| format!("{warning:?}"))
        .collect()
}

#[test]
fn a_branch_tail_addition_is_a_result_not_a_discard() {
    let source = r#"
fn tail_if(xs: [Int]) -> [Int] {
    if xs.len() == 0 { xs } else { xs + 1 }
}

fn tail_match(xs: [Int]) -> [Int] {
    match xs.split_last() {
        Some((_last, _rest)) => xs + 1,
        None => xs,
    }
}

fn tail_guarded(xs: [Int]) -> [Int] {
    match xs.len() {
        n if n == 0 => xs + 1,
        _other => xs,
    }
}

fn plain_tail(xs: [Int]) -> [Int] {
    xs + 1
}
"#;
    assert_eq!(
        warnings(source),
        Vec::<String>::new(),
        "a branch tail is used exactly as much as a function-body tail is"
    );
}

#[test]
fn a_genuinely_discarded_addition_still_warns() {
    // Rung 144's shape: the result is bound and then not returned. The fix must
    // not have bought its silence by weakening the lint into uselessness.
    let source = r#"
fn unchanged(xs: [Int]) -> [Int] {
    let ignored = xs + 4;
    xs
}
"#;
    let warnings = warnings(source);
    assert_eq!(warnings.len(), 1, "one discarded `+`: {warnings:?}");
    assert!(
        warnings[0].contains("UnusedMustUse"),
        "the discard is still reported: {warnings:?}"
    );
}

#[test]
fn a_discard_inside_a_branch_still_warns() {
    // The narrow case the fix must not swallow: the arm's OUTPUT is used, but a
    // second addition inside the same arm is not.
    let source = r#"
fn unchanged(xs: [Int]) -> [Int] {
    if xs.len() == 0 {
        xs
    } else {
        let ignored = xs + 4;
        xs + 1
    }
}
"#;
    let warnings = warnings(source);
    assert_eq!(warnings.len(), 1, "one discarded `+`: {warnings:?}");
    assert!(
        warnings[0].contains("UnusedMustUse"),
        "the in-arm discard is still reported: {warnings:?}"
    );
}
