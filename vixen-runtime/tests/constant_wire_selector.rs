//! A described-wire call-site selector must match an invocation whose
//! argument is an injected declared constant. The selector side
//! (`wire_argument_literal`) encodes such a call as
//! `WireArg::Constant { schema, bytes }`; the ACTUAL side — the invocation
//! recorders (`invocation_provenance`, `record_authored_wire_demands`) —
//! must encode the lowered `Op::DeclaredConst` argument identically, or the
//! realized demand carries `arguments: None` and `demanded_once(f(const))`
//! observes 0 for a function that ran once.

use vix::binding::ConstantSurfaceDecl;
use vix::compiler::CompilerConfig;
use vixen_runtime::ratchet::{run_source, run_source_with_config};

/// A rail-generic constant surface: nothing fixture-shaped about it.
const PINNED_NOTE: &[ConstantSurfaceDecl] = &[ConstantSurfaceDecl {
    name: "pinned_note",
    literal_params: 1,
    result: || vix::vir::Type::String,
    encode: |args| {
        let mut bytes = b"note\0".to_vec();
        bytes.extend(args[0].as_bytes());
        bytes
    },
    root: false,
}];

/// The generic shape of the regression: a helper invoked once with a
/// declared-constant argument, selected by the exact same spelling.
#[test]
fn a_call_site_selector_matches_a_declared_constant_argument() {
    const PROGRAM: &str = r#"
fn inspect(note: String) -> Int {
    7
}

#[test]
fn t() -> Stream<Check> {
    let value = inspect(pinned_note("subject"));
    yield expect_eq(value, 7);
    yield demanded_once(inspect(pinned_note("subject")));
}
"#;
    let report = run_source_with_config(
        PROGRAM,
        CompilerConfig {
            constants: PINNED_NOTE,
            ..vixen_runtime::default_config()
        },
    )
    .expect("the declared-constant selector source runs");
    assert!(
        report.passed(),
        "the invocation is realized once and the selector observes it: {report:#?}"
    );
}

/// The reviewed repro verbatim, as a second pin through the harness's own
/// fixture spelling: `inspect` runs once and `demanded_once` observes 1 —
/// never 0 against a `CallSite([Constant {..}])` selector.
#[test]
fn a_call_site_selector_matches_a_fixture_tree_argument() {
    const PROGRAM: &str = r#"
fn inspect(tree: Tree) -> Int {
    7
}

#[test]
fn t() -> Stream<Check> {
    let value = inspect(fixture_tree("small-crate"));
    yield expect_eq(value, 7);
    yield demanded_once(inspect(fixture_tree("small-crate")));
}
"#;
    let report = run_source(PROGRAM).expect("the fixture-selector source runs");
    assert!(
        report.passed(),
        "inspect runs once and the selector observes it: {report:#?}"
    );
}
