use std::cmp::Ordering;

use semver::Version as SemverVersion;
use vix::machine::{Machine, MachineArg, NamedArg};

fn source() -> &'static str {
    r#"
use vix::{Ordering, parse_version, version_cmp};

pub fn cmp(left: String, right: String) -> Int {
    match version_cmp(parse_version(left), parse_version(right)) {
        Ordering::Less => 0,
        Ordering::Equal => 1,
        Ordering::Greater => 2,
    }
}
"#
}

fn vix_cmp(machine: &mut Machine, left: &str, right: &str) -> i64 {
    machine
        .call(
            "cmp",
            &[
                NamedArg {
                    name: "left".to_string(),
                    value: MachineArg::String(left.to_string()),
                },
                NamedArg {
                    name: "right".to_string(),
                    value: MachineArg::String(right.to_string()),
                },
            ],
        )
        .unwrap_or_else(|err| panic!("vix cmp({left:?}, {right:?}) failed: {err}"))
        .0
}

fn oracle_cmp(left: &str, right: &str) -> i64 {
    match SemverVersion::parse(left)
        .unwrap_or_else(|err| panic!("semver parse({left:?}) failed: {err}"))
        .cmp_precedence(&SemverVersion::parse(right).unwrap_or_else(|err| {
            panic!("semver parse({right:?}) failed: {err}");
        })) {
        Ordering::Less => 0,
        Ordering::Equal => 1,
        Ordering::Greater => 2,
    }
}

#[test]
fn vix_version_precedence_matches_semver_crate() {
    let versions = [
        "0.1.0-alpha",
        "0.1.0",
        "1.0.0-alpha",
        "1.0.0-alpha.1",
        "1.0.0-alpha.beta",
        "1.0.0-beta",
        "1.0.0-beta.2",
        "1.0.0-beta.11",
        "1.0.0-rc.1",
        "1.0.0",
        "1.0.0+001",
        "1.0.0+build.2",
        "1.0.0-alpha+build.1",
        "1.0.0-alpha+build.2",
        "1.0.0-1",
        "1.0.0-1.alpha",
        "1.0.0-alpha.0",
        "1.0.0-alpha.0a",
        "1.0.0-alpha.00a",
        "1.0.0-alpha.1.2.3",
        "1.0.0-alpha.1.2.3.4",
        "1.0.0-alpha.1.2.beta",
        "1.2.3",
        "1.2.3-alpha",
        "1.2.3-alpha.1+sha.001",
        "1.2.3+sha.001",
        "1.2.4-alpha",
        "1.2.4",
        "2.0.0-alpha",
        "2.0.0",
    ];
    let spec_chain = [
        "1.0.0-alpha",
        "1.0.0-alpha.1",
        "1.0.0-alpha.beta",
        "1.0.0-beta",
        "1.0.0-beta.2",
        "1.0.0-beta.11",
        "1.0.0-rc.1",
        "1.0.0",
    ];

    for pair in spec_chain.windows(2) {
        assert_eq!(oracle_cmp(pair[0], pair[1]), 0);
    }

    let mut checked = 0usize;
    let mut machine = Machine::load(source()).expect("semver differential vix source loads");
    for left in versions {
        for right in versions {
            let expected = oracle_cmp(left, right);
            let actual = vix_cmp(&mut machine, left, right);
            assert_eq!(
                actual, expected,
                "vix/semver divergence for {left:?} vs {right:?}"
            );
            checked += 1;
        }
    }

    assert!(checked >= 100, "checked only {checked} version pairs");
}
