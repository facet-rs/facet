use std::collections::BTreeMap;

use vix::machine::{Machine, MachineArg, NamedArg, RenderedValue};

const ITOA_INDEX: &str = include_str!("fixtures/sparse-index/snapshot-2025-03-04/it/oa/itoa");
const BLAKE3_INDEX: &str = include_str!("fixtures/sparse-index/snapshot-2025-03-04/bl/ak/blake3");

fn rodin_source() -> String {
    std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../rodin/rodin.vix"))
        .expect("read rodin.vix")
}

fn sparse_index_source() -> String {
    std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../rodin/index.vix"))
        .expect("read rodin/index.vix")
}

fn source() -> BTreeMap<String, String> {
    BTreeMap::from([
        (
            "root".to_owned(),
            r#"
use sparse_index::{
    parse_sparse_jsonl,
    sparse_dep_count,
    sparse_index_path,
    sparse_row_cksum,
    sparse_row_count,
};
use rodin::{Index, Problem, root_candidate_count};
use vix::{Map, VersionSet};

pub fn path(name: String) -> String {
    sparse_index_path(name)
}

pub fn row_count(input: String) -> Int {
    sparse_row_count(parse_sparse_jsonl(input))
}

pub fn cksum(input: String, name: String, vers: String) -> String {
    sparse_row_cksum(parse_sparse_jsonl(input), name, vers)
}

pub fn dep_count(input: String, name: String, vers: String) -> Int {
    sparse_dep_count(parse_sparse_jsonl(input), name, vers)
}

fn itoa_static_index() -> Index {
    let names: Map<Int, String> = {};
    let names = names.insert(0, "itoa");
    let version_pkgs: Map<Int, Int> = {};
    let version_pkgs = version_pkgs.insert(0, 0).insert(1, 0);
    let version_values: Map<Int, String> = {};
    let version_values = version_values.insert(0, "1.0.14").insert(1, "1.0.15");
    let guard_clause_ids: Map<Int, Int> = {};
    let guard_tags: Map<Int, String> = {};
    let guard_kinds: Map<Int, Int> = {};
    let guard_pkgs: Map<Int, Int> = {};
    let guard_version_values: Map<Int, String> = {};
    let guard_features: Map<Int, Int> = {};
    let consequent_tags: Map<Int, String> = {};
    let consequent_pkgs: Map<Int, Int> = {};
    let consequent_version_sets: Map<Int, VersionSet> = {};
    let consequent_features: Map<Int, Int> = {};
    let gate_kinds: Map<Int, String> = {};
    let gate_targets: Map<Int, String> = {};
    Index {
        packages: [0],
        names: names,
        version_ids: [0, 1],
        version_pkgs: version_pkgs,
        version_values: version_values,
        clause_ids: [],
        guard_ids: [],
        guard_clause_ids: guard_clause_ids,
        guard_tags: guard_tags,
        guard_kinds: guard_kinds,
        guard_pkgs: guard_pkgs,
        guard_version_values: guard_version_values,
        guard_features: guard_features,
        consequent_tags: consequent_tags,
        consequent_pkgs: consequent_pkgs,
        consequent_version_sets: consequent_version_sets,
        consequent_features: consequent_features,
        gate_kinds: gate_kinds,
        gate_targets: gate_targets,
    }
}

fn itoa_static_problem() -> Problem {
    Problem {
        root_pkg: 0,
        root_req: VersionSet::from_req("^1"),
        root_features: [],
        root_default_feature: 0,
        root_default_features: false,
    }
}

pub fn itoa_root_candidate_count() -> Int {
    root_candidate_count(itoa_static_index(), itoa_static_problem())
}

"#
            .to_owned(),
        ),
        ("rodin".to_owned(), rodin_source()),
        ("sparse_index".to_owned(), sparse_index_source()),
    ])
}

fn machine() -> Machine {
    let mut machine =
        Machine::load_modules("root", source()).expect("sparse index module set loads");
    machine.set_force_molten_copy(true);
    machine
}

fn call_string(machine: &mut Machine, name: &str, args: &[(&str, &str)]) -> String {
    let args = args
        .iter()
        .map(|(name, value)| NamedArg {
            name: (*name).to_owned(),
            value: MachineArg::String((*value).to_owned()),
        })
        .collect::<Vec<_>>();
    let result = machine
        .call(name, &args)
        .unwrap_or_else(|err| panic!("call {name}: {err}"));
    let RenderedValue::String { value } = machine
        .render_result(name, result.0)
        .unwrap_or_else(|err| panic!("render {name}: {err}"))
    else {
        panic!("{name} did not render as String");
    };
    value
}

fn call_int(machine: &mut Machine, name: &str, args: &[(&str, &str)]) -> i64 {
    let args = args
        .iter()
        .map(|(name, value)| NamedArg {
            name: (*name).to_owned(),
            value: MachineArg::String((*value).to_owned()),
        })
        .collect::<Vec<_>>();
    let result = machine
        .call(name, &args)
        .unwrap_or_else(|err| panic!("call {name}: {err}"));
    let RenderedValue::Int { value } = machine
        .render_result(name, result.0)
        .unwrap_or_else(|err| panic!("render {name}: {err}"))
    else {
        panic!("{name} did not render as Int");
    };
    value
}

#[test]
fn sparse_index_paths_for_demo_crates_match_crates_io_rules() {
    let mut machine = machine();
    assert_eq!(call_string(&mut machine, "path", &[("name", "cc")]), "2/cc");
    assert_eq!(
        call_string(&mut machine, "path", &[("name", "itoa")]),
        "it/oa/itoa"
    );
    assert_eq!(
        call_string(&mut machine, "path", &[("name", "blake3")]),
        "bl/ak/blake3"
    );
}

#[test]
fn parses_captured_sparse_jsonl_rows_and_dependency_metadata() {
    let mut machine = machine();
    assert_eq!(
        call_int(&mut machine, "row_count", &[("input", ITOA_INDEX)]),
        34
    );
    assert_eq!(
        call_string(
            &mut machine,
            "cksum",
            &[("input", ITOA_INDEX), ("name", "itoa"), ("vers", "1.0.15")]
        ),
        "4a5f13b858c8d314ee3e8f639011f7ccefe71f97f96e50151fb991f267928e2c"
    );
    assert_eq!(
        call_int(
            &mut machine,
            "dep_count",
            &[
                ("input", BLAKE3_INDEX),
                ("name", "blake3"),
                ("vers", "1.6.1")
            ]
        ),
        18
    );
}

#[test]
#[ignore = "blocked on vix driver host Version frame read in rodin candidates_for: driver.rs:8116"]
fn bridge_static_index_exposes_pinned_itoa_candidates_to_rodin() {
    let mut machine = machine();
    assert_eq!(call_int(&mut machine, "itoa_root_candidate_count", &[]), 2);
}
