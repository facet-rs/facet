//! `fail payload` — the authored half of the failure plane.
//!
//! The machine already raised typed failures of its own (`MissingKey`,
//! `DivisionByZero`, …); these assert the program can raise one, that the
//! payload is an ordinary interned value with a schema and a content hash, that
//! the address the program never writes is attached anyway, and that everything
//! the machine's own failures get — laziness, `?`, propagation across calls,
//! memoization — a raise gets too.
//!
//! r[impl machine.error.failure-is-a-value]

use vix::runtime::FailureValue;
use vix::vir::{EnumType, EnumVariant, RecordField, RecordType, Type, VariantPayload};
use vixen_runtime::ratchet::{
    SnapshotExpectations, run_source, run_source_rerun_audit, run_source_with_snapshots_and_lane,
};
use weavy::exec::LaneRequest;

/// The schema a payload of this shape interns under, spelled as the VIR type
/// the surface declaration lowers to. Comparing against it asserts the payload
/// kept its own identity rather than becoming an opaque marker.
fn record_schema(name: &str, field: &str) -> vix::schema::SchemaRef {
    Type::Record(RecordType::new(
        name,
        vec![RecordField {
            name: field.to_owned(),
            ty: Type::String,
        }],
    ))
    .schema_ref()
}

/// The raise is a value: the demand answers `Failed(f)` with a payload that
/// interned like anything else, and the machine attached the raising island's
/// recipe and the raise's stable source site.
#[test]
fn a_raise_is_a_typed_failure_carrying_its_payload() {
    let run = run_source(
        r#"
struct MissingWorkspaceField { field: String }
fn edition(present: Bool) -> String {
    if present { "2021" } else { fail MissingWorkspaceField { field: "workspace.package.edition" } }
}
#[test]
fn t() -> Stream<Check> {
    yield expect_eq(edition(false), "2021");
}
"#,
    )
    .expect("a raise is a language failure, never a machine crash");
    assert!(!run.passed(), "the demanded raise fails the check");
    assert!(run.agrees(), "plain and chaos agree on the raised failure");
    let failure = run.plain.checks[0]
        .failure
        .clone()
        .expect("the demanded raise published a typed failure");
    let FailureValue::Raised { payload, .. } = failure else {
        panic!("an authored fail is FailureValue::Raised, got {failure:?}");
    };
    assert_eq!(
        payload.schema,
        record_schema("MissingWorkspaceField", "field"),
        "the payload interned under its own schema, not an opaque marker",
    );
}

/// Rung 056's rule holds for raises: the arm that raises is not entered, so
/// describing a `fail` costs nothing until something demands it.
#[test]
fn an_undemanded_raise_never_happens() {
    let run = run_source(
        r#"
struct Boom { why: String }
fn edition(present: Bool) -> String {
    if present { "2021" } else { fail Boom { why: "unreachable" } }
}
#[test]
fn t() -> Stream<Check> {
    yield expect_eq(edition(true), "2021");
}
"#,
    )
    .expect("the untaken raise runs");
    assert!(run.passed(), "the arm that raises was never entered");
    assert!(run.agrees());
}

/// A raise crosses a call boundary with its payload and its address intact: the
/// callee's outcome is forwarded into the caller's rather than flattened.
#[test]
fn a_raise_propagates_through_a_call() {
    let run = run_source(
        r#"
struct MalformedDirective { line: String }
fn malformed(line: String) -> String { fail MalformedDirective { line } }
fn directive(line: String) -> String {
    if line == "cargo:rerun" { line } else { malformed(line) }
}
#[test]
fn t() -> Stream<Check> {
    yield expect_eq(directive("cargo:rerun"), "cargo:rerun");
    yield expect_eq(directive("nonsense"), "nonsense");
}
"#,
    )
    .expect("a raise inside a callee is a language failure");
    assert!(run.agrees());
    assert!(
        run.plain.checks[0].passed,
        "the branch that does not raise still produces its value"
    );
    let failure = run.plain.checks[1]
        .failure
        .clone()
        .expect("the raise crossed the call boundary as a failure");
    let FailureValue::Raised { payload, .. } = failure else {
        panic!("a forwarded raise is still FailureValue::Raised, got {failure:?}");
    };
    assert_eq!(
        payload.schema,
        record_schema("MalformedDirective", "line"),
        "forwarding kept the authored payload, not a re-raised machine failure",
    );
}

/// Two raises of the same payload type at different sites are different
/// failures, because the source site is part of what the failure *is*.
///
/// r[impl machine.error.failure-source-site-identity]
#[test]
fn two_raise_sites_are_two_failures() {
    let run = run_source(
        r#"
struct Missing { field: String }
fn first() -> String { fail Missing { field: "a" } }
fn second() -> String { fail Missing { field: "a" } }
#[test]
fn t() -> Stream<Check> {
    yield expect_eq(first(), "x");
    yield expect_eq(second(), "x");
}
"#,
    )
    .expect("both raises run");
    assert!(run.agrees());
    let sites = run
        .plain
        .checks
        .iter()
        .map(|check| match check.failure.clone() {
            Some(FailureValue::Raised { recipe, site, .. }) => (recipe, site),
            other => panic!("expected a raise, got {other:?}"),
        })
        .collect::<Vec<_>>();
    assert_ne!(
        sites[0], sites[1],
        "distinct raise sites carry distinct source-site identities",
    );
}

/// `?` catches a raise the same way it catches a machine failure: the operand's
/// edge becomes `Result`, and the program branches on it without the failure
/// poisoning the demand that looked.
#[test]
fn a_raise_is_caught_by_the_try_operator() {
    let run = run_source(
        r#"
struct Missing { field: String }
fn edition(present: Bool) -> String {
    if present { "2021" } else { fail Missing { field: "edition" } }
}
#[test]
fn t() -> Stream<Check> {
    yield match edition(false)? {
        Ok(_)  => expect(false),
        Err(_) => expect(true),
    };
    yield match edition(true)? {
        Ok(edition) => expect_eq(edition, "2021"),
        Err(_)      => expect(false),
    };
}
"#,
    )
    .expect("catching a raise runs");
    assert!(run.passed(), "`?` caught the raise and the value alike");
    assert!(run.agrees());
}

/// A raise is an ordinary memo entry, with its read-set. A run that failed
/// yesterday fails today from the memo — the identical failure value, without
/// re-entering the computation that raised it.
///
/// r[impl machine.error.failures-are-cached-and-cut-off]
#[test]
fn a_raise_is_memoized_across_runs() {
    let audit = run_source_rerun_audit(
        r#"
struct Missing { field: String }
fn edition() -> String { fail Missing { field: "edition" } }
#[test]
fn t() -> Stream<Check> {
    yield match edition()? {
        Ok(_)  => expect(false),
        Err(_) => expect(true),
    };
}
"#,
    )
    .expect("a raise reruns against one store");
    assert!(
        audit.first.checks.iter().all(|check| check.passed),
        "the first run caught the raise",
    );
    assert!(
        audit.second.checks.iter().all(|check| check.passed),
        "the second run caught the same raise",
    );
    assert_eq!(
        audit.second.counters.memo_misses, 0,
        "the second run answered the raise entirely from the memo: {:?}",
        audit.second.counters,
    );
    assert!(
        audit.second.counters.memo_hits_exact > 0,
        "a failed demand is an ordinary memo entry: {:?}",
        audit.second.counters,
    );
    assert!(!audit.nondeterministic);
}

/// The shape `cargo_manifest.vix` and `crate.vix` are written in: an inherited
/// field that is not there is a raise in the arm that discovers it, and the arm
/// that finds the literal still produces its value.
#[test]
fn a_raise_stands_in_a_match_arm() {
    let run = run_source(
        r#"
enum CargoEdition { Literal(String), Inherit }
struct MissingWorkspaceField { field: String }
fn literal(edition: CargoEdition) -> String {
    match edition {
        CargoEdition::Literal(spelling) => spelling,
        CargoEdition::Inherit => fail MissingWorkspaceField { field: "workspace.package.edition" },
    }
}
#[test]
fn t() -> Stream<Check> {
    yield expect_eq(literal(CargoEdition::Literal("2021")), "2021");
    yield expect_eq(literal(CargoEdition::Inherit), "2021");
}
"#,
    )
    .expect("a raise in a match arm runs");
    assert!(run.agrees());
    assert!(
        run.plain.checks[0].passed,
        "the literal arm is unaffected by the arm that raises"
    );
    assert!(
        matches!(
            run.plain.checks[1].failure,
            Some(FailureValue::Raised { .. })
        ),
        "the inherit arm raised, got {:?}",
        run.plain.checks[1].failure,
    );
}

/// A raise inside a mapped closure poisons the demand that asked for the
/// mapped collection, and only that one — the same rule the machine's own
/// failures follow, because propagation is a rule of the machine.
#[test]
fn a_raise_inside_a_closure_poisons_what_demanded_it() {
    let run = run_source(
        r#"
struct Negative { n: Int }
fn doubled(xs: [Int]) -> [Int] {
    xs.map(|n: Int| if n < 0 { fail Negative { n } } else { n * 2 })
}
#[test]
fn t() -> Stream<Check> {
    yield expect_eq(doubled([1, 2]).len(), 2);
    yield expect_eq(doubled([1, 0 - 2]).len(), 2);
}
"#,
    )
    .expect("a raise inside a closure runs");
    assert!(run.agrees());
    assert!(
        run.plain.checks[0].passed,
        "the map with no negative element still has its value"
    );
    assert!(
        matches!(
            run.plain.checks[1].failure,
            Some(FailureValue::Raised { .. })
        ),
        "the map containing a negative element failed, got {:?}",
        run.plain.checks[1].failure,
    );
}

/// The payload is any value with a schema and a content hash, not only a
/// record: a string is a weak payload and a legal one, and an enum payload
/// keeps its variant.
#[test]
fn a_payload_is_any_value() {
    let run = run_source(
        r#"
enum Reason { Missing, Malformed(String) }
fn weak() -> Int { fail "something went wrong" }
fn typed() -> Int { fail Reason::Malformed("cargo:x") }
#[test]
fn t() -> Stream<Check> {
    yield expect_eq(weak(), 1);
    yield expect_eq(typed(), 1);
}
"#,
    )
    .expect("both payload shapes run");
    assert!(run.agrees());
    let schemas = run
        .plain
        .checks
        .iter()
        .map(|check| match check.failure.clone() {
            Some(FailureValue::Raised { payload, .. }) => payload.schema,
            other => panic!("expected a raise, got {other:?}"),
        })
        .collect::<Vec<_>>();
    assert_eq!(schemas[0], Type::String.schema_ref(), "a string payload");
    let reason = Type::Enum(EnumType::new(
        "Reason",
        vec![
            EnumVariant {
                name: "Missing".to_owned(),
                payload: VariantPayload::Unit,
            },
            EnumVariant {
                name: "Malformed".to_owned(),
                payload: VariantPayload::Tuple(vec![Type::String]),
            },
        ],
    ))
    .schema_ref();
    assert_eq!(schemas[1], reason, "an enum payload keeps its own schema");
}

/// Two payload types raised from one island get two outcome variants, and a
/// collection payload realizes like any other value. Nothing about the shape of
/// what you raise changes what raising means.
#[test]
fn one_island_carries_several_payload_types() {
    let run = run_source(
        r#"
struct Unsatisfiable { requirement: String }
struct Conflict { versions: [Int] }
fn resolve(k: Int) -> Int {
    if k == 0 {
        fail Unsatisfiable { requirement: "libc ^2.0" }
    } else {
        if k == 1 { fail Conflict { versions: [1, 2, 3] } } else { k }
    }
}
#[test]
fn t() -> Stream<Check> {
    yield expect_eq(resolve(2), 2);
    yield expect_eq(resolve(0), 2);
    yield expect_eq(resolve(1), 2);
}
"#,
    )
    .expect("two payload types in one island run");
    assert!(run.agrees());
    assert!(run.plain.checks[0].passed, "the arm that resolves is unaffected");
    let payloads = run.plain.checks[1..]
        .iter()
        .map(|check| match check.failure.clone() {
            Some(FailureValue::Raised { payload, .. }) => payload.schema,
            other => panic!("expected a raise, got {other:?}"),
        })
        .collect::<Vec<_>>();
    assert_eq!(payloads[0], record_schema("Unsatisfiable", "requirement"));
    assert_eq!(
        payloads[1],
        Type::Record(RecordType::new(
            "Conflict",
            vec![RecordField {
                name: "versions".to_owned(),
                ty: Type::Array(Box::new(Type::Int)),
            }],
        ))
        .schema_ref(),
        "an array payload realizes and interns like any other value",
    );
}

/// A payload has to be a value. A closure is an island-interior recipe with no
/// published form, so raising one is a typed source diagnostic rather than a
/// failure with nothing in it.
#[test]
fn a_closure_is_not_a_payload() {
    let error = run_source(
        r#"
fn boom() -> Int { fail |n: Int| n }
#[test]
fn t() -> Stream<Check> {
    yield expect_eq(boom(), 1);
}
"#,
    )
    .expect_err("a closure payload is rejected");
    let rendered = format!("{error:?}");
    assert!(
        rendered.contains("TypeMismatch"),
        "raising a closure is a type mismatch, got {rendered}",
    );
}

/// The interpreter and the native JIT are two separately-authored op
/// authorities. A raise travels through the same outcome enum on both, so the
/// failure value, its payload identity, and its attribution have to be
/// identical — a raise is semantics, not a lane artifact.
#[test]
fn both_lanes_raise_the_same_failure() {
    if !weavy::jit::task_lane::available() {
        return;
    }
    const SOURCE: &str = r#"
struct Missing { field: String }
fn edition(present: Bool) -> String {
    if present { "2021" } else { fail Missing { field: "workspace.package.edition" } }
}
#[test]
fn t() -> Stream<Check> {
    yield expect_eq(edition(false), "2021");
}
"#;
    let expectations = SnapshotExpectations::default();
    let native = run_source_with_snapshots_and_lane(SOURCE, &expectations, LaneRequest::Native)
        .expect("the native lane raises");
    let interpreter =
        run_source_with_snapshots_and_lane(SOURCE, &expectations, LaneRequest::Interpreter)
            .expect("the interpreter lane raises");
    assert!(
        native.plain.counters.native_task_spawns > 0,
        "the native run really used the native authority",
    );
    assert!(
        interpreter.plain.counters.interpreter_task_spawns > 0,
        "the interpreter run really used the interpreter authority",
    );
    assert_eq!(
        native.plain.check_family(),
        interpreter.plain.check_family(),
        "the two authorities raise the same failure, payload identity included",
    );
    assert_eq!(native.plain.value_family(), interpreter.plain.value_family());
}
