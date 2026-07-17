//! Monomorphization of generic value functions. A generic `fn f<T>(…)` is not
//! lowered up front; each call site instantiates and lowers one body per
//! distinct concrete type-argument set (issue #2497). These programs exercise
//! the acceptance cases: a plain type parameter, multiple type parameters,
//! forwarding a type parameter to a generic callee, a nested generic type
//! argument, and inference through a builtin wrapper (`Option<T>`). Every
//! passing program runs through the production ratchet path so the monomorphized
//! bodies are actually executed, not merely lowered. A final case pins the
//! instantiation bound: a generic that recurses at an ever-growing type is
//! rejected with a diagnostic rather than instantiated without limit.

use vix::ratchet::{RunError, run_source};

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

/// A type parameter that appears only inside a builtin generic wrapper is still
/// inferred — `same(Some(1))` recovers `T = Int` through `Option<T>`.
const OPTION_PARAM: &str = r#"
fn same<T>(x: Option<T>) -> Option<T> { x }
#[test]
fn t() -> Stream<Check> {
    yield expect_eq(same(Some(1)), Some(1));
    yield expect_eq(same(Some(true)), Some(true));
}
"#;

#[test]
fn option_type_argument_infers_through_the_wrapper() {
    passing(OPTION_PARAM, 2);
}

/// A generic that forwards its type parameter into an ever-larger type (`grow`
/// at `T` calls itself at `[T]`) would instantiate without bound. The
/// monomorphization limit turns that non-termination into a loud diagnostic.
const POLYMORPHIC_RECURSION: &str = r#"
fn grow<T>(x: T) -> Int { grow([x]) }
#[test]
fn t() -> Stream<Check> {
    yield expect_eq(grow(1), 1);
}
"#;

#[test]
fn unbounded_instantiation_is_rejected() {
    let error = run_source(POLYMORPHIC_RECURSION)
        .expect_err("unbounded monomorphization is rejected, not run");
    match error {
        RunError::Diagnostics(diagnostics) => assert!(
            format!("{diagnostics:?}").contains("generic instantiation limit"),
            "diagnostic names the instantiation limit: {diagnostics:?}"
        ),
        other => panic!("expected a lowering diagnostic, got {other:?}"),
    }
}
