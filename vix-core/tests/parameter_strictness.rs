//! Descriptor-independent parameter strictness analysis.
//!
//! A parameter is a demand wire unless its consumption is proven on every
//! reachable path to the function's output. Strict parameters may be evaluated
//! eagerly (a compiler-owned strictness optimization); wire parameters must be
//! passed unevaluated and demanded only where the callee consumes them. This
//! classification is a pure property of the function body and never depends on
//! trace descriptors.

use std::collections::BTreeSet;

use vix::compiler::Compiler;

/// The wire parameter names of a named function in `source`.
fn wire_parameter_names(source: &str, function: &str) -> BTreeSet<String> {
    let module = Compiler::default()
        .compile(source)
        .expect("source compiles through the canonical surface");
    let function = module
        .module
        .functions
        .iter()
        .find(|candidate| candidate.name == function)
        .expect("function is present");
    let wires = function.wire_parameters();
    function
        .parameters
        .iter()
        .filter(|parameter| wires.contains(&parameter.id))
        .map(|parameter| parameter.name.clone())
        .collect()
}

/// A conditional selector: each argument is consumed on only one branch, so both
/// are wires; the condition is consumed on every path and stays strict.
#[test]
fn conditional_selector_parameters_are_wires() {
    const SOURCE: &str = r#"
fn pick(flag: Bool) where { a: Int, b: Int } -> Int {
    if flag { a } else { b }
}
#[test]
fn t() -> Stream<Check> {
    yield expect_eq(pick(true) where { a: 1, b: 2 }, 1);
}
"#;
    assert_eq!(
        wire_parameter_names(SOURCE, "pick"),
        BTreeSet::from(["a".to_owned(), "b".to_owned()]),
        "each selected-on-one-branch argument is a wire; flag is strict",
    );
}

/// An unconditional arithmetic body consumes every parameter on every path, so
/// none are wires — they stay strict and may be evaluated eagerly.
#[test]
fn unconditionally_consumed_parameters_are_strict() {
    const SOURCE: &str = r#"
fn add(a: Int) where { b: Int } -> Int { a + b }
#[test]
fn t() -> Stream<Check> {
    yield expect_eq(add(3) where { b: 4 }, 7);
}
"#;
    assert!(
        wire_parameter_names(SOURCE, "add").is_empty(),
        "both operands are consumed on every path",
    );
}

/// A parameter consumed in every match arm is strict; one consumed in only some
/// arms is a wire.
#[test]
fn match_arm_consumption_is_path_sensitive() {
    const SOURCE: &str = r#"
enum Choice { Left, Right }
fn choose(c: Choice) where { shared: Int, only_left: Int } -> Int {
    match c {
        Choice::Left => shared + only_left,
        Choice::Right => shared,
    }
}
#[test]
fn t() -> Stream<Check> {
    yield expect_eq(choose(Choice::Right) where { shared: 5, only_left: 9 }, 5);
}
"#;
    assert_eq!(
        wire_parameter_names(SOURCE, "choose"),
        BTreeSet::from(["only_left".to_owned()]),
        "shared is consumed in both arms (strict); only_left in one (wire)",
    );
}
