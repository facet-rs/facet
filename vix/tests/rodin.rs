//! rodin.vix — the version resolver in vix, differentially checked against
//! cargo (the oracle) and rodin-core (the Rust reference during the port).

use vix::machine::Machine;

fn rodin_source() -> String {
    std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../rodin/rodin.vix"))
        .expect("read rodin.vix")
}

// WIP: green once the machine grows `Version` accessors (.major/.minor/.patch).
// The compile loop already validates the full type surface + compat_of's faithful
// lowering; it blocks only on that one host-value accessor.
#[ignore = "pending Version .major/.minor/.patch accessors in the machine"]
#[test]
fn compat_class_matches_rodin_core_from_version() {
    let mut machine = Machine::load(&rodin_source()).expect("rodin.vix loads");
    let value = machine.demand_i64("main", vec![0]).expect("rodin.vix main runs");
    // compat_code: 2.3.4 -> 2 (major bucket) ; 0.5.1 -> 1005 ; 0.0.7 -> 2007
    assert_eq!(value, 2 + 1005 + 2007);
}
