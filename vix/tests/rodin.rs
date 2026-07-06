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
    let value = machine.demand_i64("main", vec![0]).expect("rodin.vix main runs");
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
