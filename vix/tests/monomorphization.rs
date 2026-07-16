//! Monomorphization of generic value functions. A generic `fn f<T>(…)` is not
//! lowered up front; each call site instantiates and lowers one body per
//! distinct concrete type-argument set (issue #2497). These programs exercise
//! the acceptance cases: a plain type parameter, multiple type parameters,
//! forwarding a type parameter to a generic callee, and a nested generic type
//! argument. Every program runs through the production ratchet path so the
//! monomorphized bodies are actually executed, not merely lowered.

use vix::ratchet::run_source;

fn passing(source: &str, checks: usize) {
    let report = run_source(source).expect("generic program compiles and runs in production");
    assert!(report.passed(), "checks pass: {:?}", report.plain.checks);
    assert!(report.agrees(), "plain and chaos lanes agree");
    assert_eq!(report.plain.checks.len(), checks);
    assert_eq!(report.plain.checks, report.chaos.checks);
}

/// A single type parameter instantiated at three distinct concrete types. Each
/// use lowers its own body, so the identity is monomorphic per type.
const IDENTITY: &str = r#"
fn id<T>(x: T) -> T { x }
#[test]
fn t() -> Stream<Check> {
    yield expect_eq(id(1), 1);
    yield expect(id(true));
    yield expect_eq(id("hi"), "hi");
}
"#;

#[test]
fn identity_monomorphizes_per_concrete_type() {
    passing(IDENTITY, 3);
}

/// A generic function forwarding its type parameter to a generic callee: `twice`
/// at `Int` instantiates `id` at `Int`.
const FORWARDING: &str = r#"
fn id<T>(x: T) -> T { x }
fn twice<T>(x: T) -> T { id(id(x)) }
#[test]
fn t() -> Stream<Check> {
    yield expect_eq(twice(7), 7);
    yield expect_eq(twice("z"), "z");
}
"#;

#[test]
fn type_parameter_forwards_to_generic_callee() {
    passing(FORWARDING, 2);
}

/// Multiple type parameters, inferred from a tuple argument.
const MULTIPLE_PARAMS: &str = r#"
fn fst<A, B>(pair: (A, B)) -> A { pair.0 }
#[test]
fn t() -> Stream<Check> {
    yield expect_eq(fst((3, "x")), 3);
    yield expect(fst((true, 9)));
}
"#;

#[test]
fn multiple_type_parameters_infer_from_a_tuple() {
    passing(MULTIPLE_PARAMS, 2);
}

/// A nested generic type argument: `wrap` is instantiated at `[Int]`, so its
/// body builds an `[[Int]]`.
const NESTED_GENERIC: &str = r#"
fn wrap<T>(x: T) -> [T] { [x] }
#[test]
fn t() -> Stream<Check> {
    yield expect_eq(wrap(1).len(), 1);
    yield expect_eq(wrap([1, 2]).len(), 1);
}
"#;

#[test]
fn nested_generic_type_argument_lowers() {
    passing(NESTED_GENERIC, 2);
}
