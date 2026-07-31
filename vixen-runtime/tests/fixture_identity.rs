//! Acceptance 1 of the origin-rail note: **identities survive the move.**
//! `fixture_tree(name)` / `fixture_registry()` left `vix-core` (dedicated ops
//! 90/94) for harness-declared constant surfaces, and this test pins that the
//! relocation changed no value identity byte: the lowered constants carry the
//! exact coordinate encodings the ops produced, and their framed identities
//! equal literals captured from the pre-move pipeline.

use vix::runtime::FramedNode;
use vix::vir::{ExternKind, Op, Type};

/// The pre-move value identity of `fixture_tree("small-crate")`: the
/// `Tree`-schema-framed leaf `fixture-tree\0small-crate`, captured from the
/// `Op::FixtureTree` pipeline before the ops retired. If this ever shifts,
/// a re-key is being shipped — stop and call it out, per the exec precedent.
const SMALL_CRATE_TREE_IDENTITY: &str =
    "a2553f6546a38cdb43f65e18b603486f56f7cf8ded2f34d67e8fe0821d2c5102";

/// The pre-move value identity of `fixture_registry()`: the
/// `Registry`-framed leaf `fixture-registry`, captured from `Op::FixtureRegistry`.
const FIXTURE_REGISTRY_IDENTITY: &str =
    "7d689b18d46eb667b8c39f53a2c0204ff6d80023e3b799e7f367b807af19f374";

const PROGRAM: &str = r#"
#[test]
fn t() -> Stream<Check> {
    let tree = fixture_tree("small-crate");
    let manifest = (tree / "Cargo.toml").text();
    yield expect(manifest.contains("[package]"));
}

fn fetch_pinned() -> Blob {
    fetch(fixture_registry().url("tokio-1.52.3.crate"))
}
"#;

fn declared_consts(module: &vix::vir::Module) -> Vec<(Vec<u8>, bool, Type)> {
    module
        .functions
        .iter()
        .flat_map(|function| &function.nodes)
        .filter_map(|node| match &node.op {
            Op::DeclaredConst { bytes, root } => Some((bytes.clone(), *root, node.ty.clone())),
            _ => None,
        })
        .collect()
}

#[test]
fn fixture_identities_survive_the_move_byte_for_byte() {
    let compilation = vixen_runtime::default_compiler()
        .compile(PROGRAM)
        .unwrap_or_else(|diagnostics| panic!("the fixture surfaces resolve: {diagnostics:#?}"));

    let consts = declared_consts(&compilation.module);
    let tree = consts
        .iter()
        .find(|(bytes, ..)| bytes.starts_with(b"fixture-tree\0"))
        .expect("the fixture_tree constant is lowered");
    let registry = consts
        .iter()
        .find(|(bytes, ..)| bytes.as_slice() == b"fixture-registry")
        .expect("the fixture_registry constant is lowered");

    // The coordinate encodings are byte-identical to what the retired ops
    // constructed in the scheduler and lowering arms.
    assert_eq!(tree.0, b"fixture-tree\0small-crate");
    assert_eq!(
        tree.2,
        Type::Extern(ExternKind::Host(vix::binding::TREE)),
        "the tree constant keeps its Tree framing"
    );
    assert!(!tree.1, "the tree constant stays an in-frame constant");
    assert_eq!(
        registry.2,
        Type::Extern(ExternKind::Registry),
        "the registry constant keeps its Registry framing"
    );
    assert!(registry.1, "the registry constant stays an effect root");

    // And the framed value identities equal the pre-move pipeline's, pinned
    // as literals so a silent re-key cannot ship.
    assert_eq!(
        FramedNode::leaf(tree.2.schema_ref(), tree.0.clone())
            .identity()
            .content
            .hex(),
        SMALL_CRATE_TREE_IDENTITY,
    );
    assert_eq!(
        FramedNode::leaf(registry.2.schema_ref(), registry.0.clone())
            .identity()
            .content
            .hex(),
        FIXTURE_REGISTRY_IDENTITY,
    );
}

/// The review-amended literal-only constraint holds across the move: a
/// computed fixture coordinate is a compile diagnostic naming the surface,
/// with the same diagnostic quality the intrinsic arm had.
#[test]
fn a_computed_fixture_coordinate_stays_rejected() {
    let error = vixen_runtime::default_compiler()
        .compile(concat!(
            "fn open(name: String) -> Tree {\n",
            "    fixture_tree(name)\n",
            "}\n",
        ))
        .expect_err("a dynamic fixture coordinate is rejected, never evaluated");
    let error = format!("{error:?}");
    assert!(
        error.contains("fixture_tree string literal"),
        "the diagnostic names the surface and the expected literal: {error}"
    );
}
