use std::cmp::Ordering;

use semver::{Version as SemverVersion, VersionReq};
use vix::machine::{Machine, MachineArg, NamedArg, RenderedValue};

fn source() -> &'static str {
    r#"
use vix::{Ordering, VersionSet, parse_version, version_cmp};

pub fn cmp(left: String, right: String) -> Int {
    match version_cmp(parse_version(left), parse_version(right)) {
        Ordering::Less => 0,
        Ordering::Equal => 1,
        Ordering::Greater => 2,
    }
}

pub fn req_matches(req: String, v: String) -> Bool {
    VersionSet::from_req(req).contains(version(v))
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

fn vix_req_matches(machine: &mut Machine, req: &str, version: &str) -> bool {
    let result = machine
        .call(
            "req_matches",
            &[
                NamedArg {
                    name: "req".to_string(),
                    value: MachineArg::String(req.to_string()),
                },
                NamedArg {
                    name: "v".to_string(),
                    value: MachineArg::String(version.to_string()),
                },
            ],
        )
        .unwrap_or_else(|err| panic!("vix req_matches({req:?}, {version:?}) failed: {err}"));
    let RenderedValue::Bool { value } = machine
        .render_result("req_matches", result.0)
        .unwrap_or_else(|err| panic!("render req_matches({req:?}, {version:?}): {err}"))
    else {
        panic!("req_matches did not render as Bool");
    };
    value
}

fn oracle_req_matches(req: &str, version: &str) -> bool {
    VersionReq::parse(req)
        .unwrap_or_else(|err| panic!("semver req parse({req:?}) failed: {err}"))
        .matches(
            &SemverVersion::parse(version)
                .unwrap_or_else(|err| panic!("semver parse({version:?}) failed: {err}")),
        )
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

#[test]
fn vix_version_req_matching_matches_semver_crate() {
    let reqs = [
        "*",
        "1",
        "1.2",
        "1.2.3",
        "=1.2.3",
        "=0.50.0-rc.0",
        "=1.2.3-alpha.1",
        "^1.2.3",
        "^0.2.3",
        "^0.0.3",
        "~1.2",
        "~1.2.3",
        ">=1, <2",
        ">=1.2.3, <1.4",
        ">1.2.3",
        "<=1.2.3",
        "^1.2.3-alpha.1",
    ];
    let versions = [
        "0.0.2",
        "0.0.3-alpha.1",
        "0.0.3",
        "0.0.4",
        "0.2.2",
        "0.2.3",
        "0.2.9",
        "0.3.0-alpha.1",
        "0.3.0",
        "0.50.0-rc.0",
        "0.50.0",
        "1.0.0",
        "1.2.2",
        "1.2.3-alpha.0",
        "1.2.3-alpha.1",
        "1.2.3-beta.1",
        "1.2.3",
        "1.2.4",
        "1.3.0",
        "1.4.0",
        "2.0.0-alpha.1",
        "2.0.0",
    ];

    let mut checked = 0usize;
    let mut machine = Machine::load(source()).expect("semver req differential vix source loads");
    for req in reqs {
        for version in versions {
            let expected = oracle_req_matches(req, version);
            let actual = vix_req_matches(&mut machine, req, version);
            assert_eq!(
                actual, expected,
                "vix/semver req divergence for {req:?} vs {version:?}"
            );
            checked += 1;
        }
    }

    assert!(checked >= 100, "checked only {checked} req/version pairs");
}
