//! rodin.vix — the version resolver in vix, differentially checked against
//! cargo (the oracle) and rodin-core (the Rust reference during the port).

use vix::machine::Machine;

fn rodin_source() -> String {
    std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../rodin/rodin.vix"))
        .expect("read rodin.vix")
}

#[test]
fn compat_class_matches_rodin_core_from_version() {
    let mut machine = Machine::load(&rodin_source()).expect("rodin.vix loads");
    let value = machine
        .demand_i64("main", vec![0])
        .expect("rodin.vix main runs");
    // compat_code: 2.3.4 -> 2 (major bucket) ; 0.5.1 -> 1005 ; 0.0.7 -> 2007
    assert_eq!(value, 2 + 1005 + 2007);
}

// Exercises the Option surface end-to-end: Some/None construction and matching
// both arms, over an Option<Version> payload (the Domain.selected shape).
#[test]
fn option_round_trips_some_and_none() {
    let mut machine = Machine::load(&rodin_source()).expect("rodin.vix loads");
    let value = machine
        .demand_i64("option_probe", vec![0])
        .expect("rodin.vix option_probe runs");
    // Some(2.3.4) -> compat_code 2 ; no_version() None -> fallback 9000
    assert_eq!(value, 2 + 9000);
}

// Exercises the general string primitives (before / after / parse_int) that a
// vix version parser is built from: "2.3.4" -> 2*100 + 3*10 + 4.
#[test]
fn string_primitives_decompose_a_version() {
    let mut machine = Machine::load(&rodin_source()).expect("rodin.vix loads");
    let value = machine
        .demand_i64("string_probe", vec![0])
        .expect("rodin.vix string_probe runs");
    assert_eq!(value, 234);
}

// The glibc-preflight path: strip "GLIBC_", parse "2.35" (2-component), and
// compare with version_lte — all in vix. 2.34.0 <= 2.35.0, so returns .minor.
#[test]
fn version_parse_strip_and_compare_in_vix() {
    let mut machine = Machine::load(&rodin_source()).expect("rodin.vix loads");
    let value = machine
        .demand_i64("version_probe", vec![0])
        .expect("rodin.vix version_probe runs");
    assert_eq!(value, 35);
}

// Operator overloading: `fn >(self: Rank, other: Rank)` makes `a > b` dispatch
// to the user function. hi > lo (5>3) = 1, lo > hi = 0.
#[test]
fn user_type_owns_its_comparison_operator() {
    let mut machine = Machine::load(&rodin_source()).expect("rodin.vix loads");
    let value = machine
        .demand_i64("rank_probe", vec![0])
        .expect("rodin.vix rank_probe runs");
    assert_eq!(value, 1);
}

#[test]
fn dense_state_array_indexed_get_reads_aggregate_elements() {
    let source = r#"
struct Domain {
    active: Bool,
}

fn seed() -> [Domain] {
    [Domain { active: true }, Domain { active: false }]
}

pub fn molten() -> Bool {
    let domains = [Domain { active: true }];
    domains.get(0).active
}

pub fn interned() -> Bool {
    let domains = seed();
    domains.get(0).active
}

pub fn out_of_bounds() -> Bool {
    let domains = seed();
    domains.get(2).active
}
"#;

    let mut machine = Machine::load(source).expect("array indexed get lowers");
    let molten = machine
        .demand_i64("molten", vec![])
        .expect("molten array indexed get runs");
    assert_eq!(molten, 1);

    let interned = machine
        .demand_i64("interned", vec![])
        .expect("interned array indexed get runs");
    assert_eq!(interned, 1);

    let err = machine
        .demand_i64("out_of_bounds", vec![])
        .expect_err("out-of-bounds array indexed get must be loud");
    assert!(err.contains("array index 2 out of bounds 2"), "{err}");
}
