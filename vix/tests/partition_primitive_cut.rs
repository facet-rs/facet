//! FV-D1D partition contract certificates.
//!
//! The registered-primitive authority is the scheduler-owned generic dispatch
//! reached through a verified Weavy `HostCallYield`. A machine-plane effect
//! island (evaluated by the legacy `evaluate_effect_node` interpreter) has no
//! primitive authority, so an `Op::InvokePrimitive` node must never live inside
//! an `IslandPurpose::Effect` island — neither in its root nodes nor anywhere in
//! its carried callee closure. Calls whose reachable helper closure invokes a
//! primitive also remain explicit value-publication boundaries, so receipt
//! revalidation can reuse their typed result before downstream pure work runs.
//!
//! These structural checks are paired with one production-runtime certificate:
//! the helper publication must lower, suspend on its primitive, resume, and
//! publish through its checked result ABI.

use vix::compiler::Compiler;
use vix::lowering::{LoweringCache, attribution_for};
use vix::runtime::{ChaosPolicy, EventLog, IslandInputs, Location, Runtime};
use vix::vir::{IslandPurpose, Op, PartitionedTest};

/// Every effect island in the partition that carries an `InvokePrimitive` in its
/// root nodes or its callee closure — a violation of the primitive-authority
/// boundary. Returns `(island_debug_label, offending_op)` for each.
fn primitives_inside_effect_islands(partitioned: &PartitionedTest) -> Vec<String> {
    let mut violations = Vec::new();
    let mut scan = |label: &str, island: &vix::vir::Island| {
        if island.purpose != IslandPurpose::Effect {
            return;
        }
        for node in &island.nodes {
            if matches!(node.op, Op::InvokePrimitive { .. }) {
                violations.push(format!("{label}: root node {:?} is {:?}", node.id, node.op));
            }
        }
        for callee in &island.callees {
            for node in &callee.nodes {
                if matches!(node.op, Op::InvokePrimitive { .. }) {
                    violations.push(format!(
                        "{label}: callee {:?} node {:?} is {:?}",
                        callee.id, node.id, node.op
                    ));
                }
            }
        }
    };
    for (i, value) in partitioned.values.iter().enumerate() {
        scan(&format!("value[{i}]"), &value.island);
    }
    for (i, wire) in partitioned.wire_islands.iter().enumerate() {
        scan(&format!("wire[{i}]"), &wire.island);
    }
    for (i, island) in partitioned.islands.iter().enumerate() {
        scan(&format!("check[{i}]"), island);
    }
    if let Some(generator) = &partitioned.generator {
        scan("generator", generator);
    }
    violations
}

fn partition(src: &str) -> PartitionedTest {
    let module = Compiler::new().compile(src).expect("source compiles");
    let test = module.tests.first().expect("source declares one #[test]");
    module.partition_test(test)
}

fn assert_no_primitive_in_effect_island(src: &str) {
    let partitioned = partition(src);
    let violations = primitives_inside_effect_islands(&partitioned);
    assert!(
        violations.is_empty(),
        "primitive invocations must not live inside an effect island:\n{}",
        violations.join("\n"),
    );
}

const MANIFEST_NAME_SHAPE: &str = r#"
fn manifest_name(tree: Tree) -> String {
    let m: Manifest = toml_decode((tree / "Cargo.toml").text());
    m.package.name
}
struct Package { name: String, version: String }
struct Manifest { package: Package }

#[test]
fn unchanged_tree_read() -> Stream<Check> {
    yield expect_eq(manifest_name(fixture_tree("small-crate")), "small-crate");
}
"#;

/// The persistence_journal `manifest_name` shape carries two registered
/// primitive invocations in one helper body. Neither may leak into the legacy
/// effect evaluator.
#[test]
fn manifest_name_helper_cuts_decode_out_of_the_effect_island() {
    assert_no_primitive_in_effect_island(MANIFEST_NAME_SHAPE);
}

const NESTED_MIXED_HELPER: &str = r#"
fn inner(tree: Tree) -> String {
    (tree / "Cargo.toml").text()
}
fn outer(tree: Tree) -> String {
    let m: Manifest = toml_decode(inner(tree));
    m.package.name
}
struct Package { name: String, version: String }
struct Manifest { package: Package }

#[test]
fn nested_mixed_read() -> Stream<Check> {
    yield expect_eq(outer(fixture_tree("small-crate")), "small-crate");
}
"#;

/// Primitive invocations live in different helpers along one call chain; the
/// reachable closure must still stay out of a legacy effect island.
#[test]
fn nested_mixed_helper_cuts_decode_out_of_the_effect_island() {
    assert_no_primitive_in_effect_island(NESTED_MIXED_HELPER);
}

const TWO_DISTINCT_INVOCATIONS: &str = r#"
fn manifest_name(tree: Tree) -> String {
    let m: Manifest = toml_decode((tree / "Cargo.toml").text());
    m.package.name
}
struct Package { name: String, version: String }
struct Manifest { package: Package }

#[test]
fn two_distinct_reads() -> Stream<Check> {
    yield expect_eq(manifest_name(fixture_tree("small-crate")), "small-crate");
    yield expect_eq(manifest_name(fixture_tree("other-crate")), "other-crate");
}
"#;

/// Two invocations of one helper with distinct arguments must not collide merely
/// because they share callee identity, and neither may leave a primitive inside
/// an effect island.
#[test]
fn two_distinct_invocations_each_cut_their_decode() {
    assert_no_primitive_in_effect_island(TWO_DISTINCT_INVOCATIONS);
}

const EARLY_CUTOFF_SHAPE: &str = r#"
fn line_count(tree: Tree) -> Int {
    (tree / "data.txt").text().lines().len()
}
fn render(n: Int) -> String { "lines: " + n.to_string() }

#[test]
fn early_cutoff() -> Stream<Check> {
    yield expect_eq(render(line_count(fixture_tree("touched-fixture"))), "lines: 3");
}
"#;

#[test]
fn primitive_helper_result_remains_a_publication_boundary() {
    let module = Compiler::new()
        .compile(EARLY_CUTOFF_SHAPE)
        .expect("source compiles");
    let test = module.tests.first().expect("source declares one #[test]");
    let partitioned = module.partition_test(test);
    let helper = partitioned
        .values
        .iter()
        .find(|value| {
            value
                .island
                .nodes
                .iter()
                .find(|node| node.id == value.island.output)
                .is_some_and(|node| matches!(node.op, Op::Call(_)))
        })
        .expect("the primitive-bearing helper result is an eager value publication");
    assert!(
        partitioned
            .islands
            .iter()
            .any(|island| island.value_inputs.contains(&helper.id)),
        "the downstream check consumes the helper result by identity",
    );
    let mut cache = LoweringCache::default();
    let lowered = cache
        .get_or_lower_owned(&helper.island)
        .expect("the primitive-bearing helper lowers");
    let attribution = attribution_for(&helper.island);
    let mut runtime = Runtime::new(EventLog::default());
    let result = runtime.evaluate(
        helper.island.id,
        &Location::for_test_value("primitive-helper", "result"),
        lowered,
        &attribution,
        IslandInputs {
            arguments: Vec::new(),
            wires: Vec::new(),
        },
        ChaosPolicy::default(),
    );
    assert!(
        result.is_ok(),
        "the primitive-bearing helper publishes through its checked result ABI: {result:#?}\nevents: {:#?}",
        runtime.sink().events(),
    );
}
