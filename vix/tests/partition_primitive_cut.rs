//! FV-D1D Layer 1 — partition contract certificates.
//!
//! The registered-primitive authority is the scheduler-owned generic dispatch
//! reached through a verified Weavy `HostCallYield`. A machine-plane effect
//! island (evaluated by the legacy `evaluate_effect_node` interpreter) has no
//! primitive authority, so an `Op::InvokePrimitive` node must never live inside
//! an `IslandPurpose::Effect` island — neither in its root nodes nor anywhere in
//! its carried callee closure. When a helper mixes effect roots (tree text) with
//! a registered primitive (decode), the partition must cut the primitive out as
//! its own verified Weavy value publication, leaving the effect roots to publish
//! their outputs and the pure continuation to consume the typed result.
//!
//! These are structural certificates over the partition only. They must pass
//! before any scheduler/evaluator change (Layer 2).

use vix::compiler::Compiler;
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

/// The persistence_journal `manifest_name` shape: a helper that reads tree text
/// (effect roots) and decodes it (registered primitive) in one body.
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

/// The effect root and the primitive live in different helpers along one call
/// chain; the cut must still separate them.
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
