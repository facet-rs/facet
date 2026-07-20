//! Structural total order for enums and arrays, on the production verified path.
//!
//! Enum order is declaration/variant order first (`E::A < E::B`), then
//! lexicographic payload order within equal variants. Array order is
//! lexicographic by element with array length as the exhausted-prefix tie-break
//! (`[1,2] < [1,2,3]`). Both recurse through nested products/enums/arrays and
//! reuse the machine's structural comparison — no host comparator, no
//! handle/hash order.

use vixen_runtime::ratchet::run_source;

fn all_pass(source: &str, expected_checks: usize) {
    let report = run_source(source).expect("compiles and runs through the production path");
    assert!(report.passed(), "checks pass: {:?}", report.plain.checks);
    assert!(report.agrees(), "plain and chaos lanes agree");
    assert_eq!(report.plain.checks.len(), expected_checks);
    assert_eq!(report.plain.checks, report.chaos.checks);
    assert_eq!(report.plain.counters.pure_host_calls, 0);
}

#[test]
fn enum_declaration_and_payload_order() {
    all_pass(
        r#"
enum E { A, B(Int), C(Int) }
#[test]
fn t() -> Stream<Check> {
    yield expect(E::A < E::B(0));
    yield expect(E::B(9) < E::C(0));
    yield expect(E::B(1) < E::B(2));
    yield expect(E::B(2) > E::B(1));
    yield expect_eq(E::B(5) <=> E::B(5), Ordering::Equal);
    yield expect(!(E::C(0) < E::B(9)));
}
"#,
        6,
    );
}

#[test]
fn array_lexicographic_and_length_tiebreak() {
    all_pass(
        r#"
#[test]
fn t() -> Stream<Check> {
    let empty: [Int] = [];
    yield expect([1, 2] < [1, 3]);
    yield expect([1, 2] < [1, 2, 3]);
    yield expect([1, 2, 3] > [1, 2]);
    yield expect_eq([1, 2, 3] <=> [1, 2, 3], Ordering::Equal);
    yield expect(empty < [0]);
    yield expect([2] > [1, 9, 9]);
}
"#,
        6,
    );
}

#[test]
fn nested_array_of_enums_orders_structurally() {
    all_pass(
        r#"
enum Tag { Lo, Hi(Int) }
#[test]
fn t() -> Stream<Check> {
    yield expect([Tag::Lo, Tag::Hi(1)] < [Tag::Lo, Tag::Hi(2)]);
    yield expect([Tag::Lo] < [Tag::Hi(0)]);
    yield expect([Tag::Hi(1), Tag::Lo] > [Tag::Lo, Tag::Hi(9)]);
}
"#,
        3,
    );
}
