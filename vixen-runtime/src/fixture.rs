//! The offline harness's fixture vocabulary: the `fixture_tree` /
//! `fixture_registry` surface spellings, declared as injected constant
//! surfaces ([`vix::binding::ConstantSurfaceDecl`]).
//!
//! These are HARNESS vocabulary, not language vocabulary: they name entries
//! in the harness's fixture root, their values re-verify against the fixture
//! store the harness installs as its origin adapter, and a production
//! embedding never declares them. `vix-core` spells neither name anywhere —
//! the machine sees only a declared typed byte-leaf constant.
//!
//! The value identities are the coordinate encodings, unchanged from the
//! retired dedicated ops (`machine.identity` acceptance of the origin-rail
//! note): a fixture tree is the `Tree`-framed handle
//! `fixture-tree\0<name>` — the identity of its *coordinate*, not its
//! content, which is the point: the store simulates "the world changed under
//! the same name", and the rerun audit re-verifies the receipts against live
//! content. The registry is the `Registry`-framed leaf `fixture-registry`.

use vix::binding::ConstantSurfaceDecl;
use vix::vir::{ExternKind, Type};

/// The byte prefix of fixture tree handles — the fixture adapter's declared
/// tree-handle namespace. The origin-rail goal: spelled in exactly one
/// place, this module. (Transitionally the `vix-core` fixture store still
/// spells it for its own adapter declaration; the store moves here next.)
pub(crate) const FIXTURE_TREE_NAMESPACE: &[u8] = b"fixture-tree\0";

/// The `Tree` host type the fixture-tree constant is framed as.
fn fixture_tree_type() -> Type {
    Type::Extern(ExternKind::Host(vix::binding::TREE))
}

/// Encode `fixture_tree(name)`'s one literal into the lazy handle bytes.
fn encode_fixture_tree(args: &[&str]) -> Vec<u8> {
    let mut bytes = FIXTURE_TREE_NAMESPACE.to_vec();
    bytes.extend(args[0].as_bytes());
    bytes
}

fn fixture_registry_type() -> Type {
    Type::Extern(ExternKind::Registry)
}

fn encode_fixture_registry(_args: &[&str]) -> Vec<u8> {
    b"fixture-registry".to_vec()
}

/// The two fixture surfaces, as injected constant-surface declarations.
///
/// `fixture_tree` declares the literal-argument constraint the review
/// amendment demanded (every constant-surface parameter is a declared
/// literal): a computed fixture coordinate is rejected at lowering, so a
/// harness's fixture requirement set stays static.
///
/// The publication shapes mirror the retired ops exactly: `fixture_tree` is
/// an in-frame realized constant (non-root), `fixture_registry` is its own
/// scheduler-published effect island (root) — downstream registry consumers
/// take its value as a published input.
pub const FIXTURE_CONSTANTS: &[ConstantSurfaceDecl] = &[
    ConstantSurfaceDecl {
        name: "fixture_tree",
        literal_params: 1,
        result: fixture_tree_type,
        encode: encode_fixture_tree,
        root: false,
    },
    ConstantSurfaceDecl {
        name: "fixture_registry",
        literal_params: 0,
        result: fixture_registry_type,
        encode: encode_fixture_registry,
        root: true,
    },
];
