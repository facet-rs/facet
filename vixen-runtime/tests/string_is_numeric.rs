//! `String.is_numeric()` — a total predicate that reports whether a string is a
//! non-empty run of ASCII decimal digits. It reads only the receiver's resident
//! bytes (like `contains`), never allocates, and runs identically on the
//! interpreter and JIT lanes (plain and chaos). It is the general string
//! primitive the version substrate needs to classify SemVer prerelease
//! identifiers as numeric or alphanumeric.

use vixen_runtime::ratchet::run_source;

const SOURCE: &str = r#"
#[test]
fn is_numeric() -> Stream<Check> {
    yield expect("42".is_numeric());
    yield expect("007".is_numeric());
    yield expect(!"".is_numeric());
    yield expect(!"4a".is_numeric());
    yield expect(!"-3".is_numeric());
    yield expect(!"1.2".is_numeric());
}
"#;

#[test]
fn string_is_numeric_runs_through_production_path() {
    let report = run_source(SOURCE).expect("is_numeric compiles and runs in production");
    assert!(report.passed(), "checks pass: {:?}", report.plain.checks);
    assert!(report.agrees(), "plain and chaos lanes agree");
    assert_eq!(report.plain.checks.len(), 6);
    assert_eq!(report.plain.checks, report.chaos.checks);
    assert_eq!(report.plain.counters.pure_host_calls, 0);
}
