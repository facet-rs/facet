//! The injected constant-surface rail: an embedder declares a free-function
//! name that lowers to a declared typed byte-leaf constant
//! (`Op::DeclaredConst`), and the declared literal-argument constraint is
//! checked at lowering — a computed argument is a compile diagnostic naming
//! the surface, never an evaluated expression. This is the rail the harness's
//! `fixture_tree`/`fixture_registry` spellings ride once they leave the
//! language core.

use vix::binding::ConstantSurfaceDecl;
use vix::compiler::{Compiler, CompilerConfig};
use vix::vir::{EffectKind, Op, Type};

/// A `String`-typed in-frame constant with one declared literal parameter.
const PINNED_NOTE: ConstantSurfaceDecl = ConstantSurfaceDecl {
    name: "pinned_note",
    literal_params: 1,
    result: || Type::String,
    encode: |args| {
        let mut bytes = b"note\0".to_vec();
        bytes.extend(args[0].as_bytes());
        bytes
    },
    root: false,
};

/// A zero-argument constant declared as an effect root — the
/// `fixture_registry()` publication shape.
const HARNESS_TOKEN: ConstantSurfaceDecl = ConstantSurfaceDecl {
    name: "harness_token",
    literal_params: 0,
    result: || Type::String,
    encode: |_| b"harness-token".to_vec(),
    root: true,
};

const CONSTANTS: &[ConstantSurfaceDecl] = &[PINNED_NOTE, HARNESS_TOKEN];

fn compiler() -> Compiler {
    Compiler::with_config(CompilerConfig {
        constants: CONSTANTS,
        ..CompilerConfig::default()
    })
}

/// Find every `DeclaredConst` node in the compiled module.
fn declared_consts(module: &vix::vir::Module) -> Vec<(Vec<u8>, bool, EffectKind, Type)> {
    module
        .functions
        .iter()
        .flat_map(|function| &function.nodes)
        .filter_map(|node| match &node.op {
            Op::DeclaredConst { bytes, root } => {
                Some((bytes.clone(), *root, node.effect.kind, node.ty.clone()))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn a_literal_call_lowers_to_the_declared_constant() {
    let compilation = compiler()
        .compile("fn f() -> String {\n    pinned_note(\"hi\")\n}\n")
        .expect("a literal constant-surface call compiles");
    let consts = declared_consts(&compilation.module);
    assert_eq!(consts.len(), 1, "one declared constant is lowered");
    let (bytes, root, effect, ty) = &consts[0];
    // The encoding is the declaration's, byte for byte — this is what makes
    // the constant's value identity a property of the declaration.
    assert_eq!(bytes, b"note\0hi");
    assert!(!root, "an in-frame declaration lowers non-root");
    // Effect-marked: a declared constant names an authority-backed value.
    assert_eq!(*effect, EffectKind::Effect);
    assert_eq!(*ty, Type::String);
}

#[test]
fn a_root_declaration_lowers_root() {
    let compilation = compiler()
        .compile("fn f() -> String {\n    harness_token()\n}\n")
        .expect("a root constant-surface call compiles");
    let consts = declared_consts(&compilation.module);
    assert_eq!(consts.len(), 1);
    assert_eq!(consts[0].0, b"harness-token");
    assert!(consts[0].1, "the declaration's root shape is preserved");
}

#[test]
fn the_std_spelling_resolves_like_any_injected_surface() {
    // A `std::` spelling only exists once an embedder installs a prelude (the
    // inline `std` module); the injected declaration then answers under it,
    // exactly as an injected primitive surface does.
    let compiler = Compiler::with_config(CompilerConfig {
        constants: CONSTANTS,
        prelude: &["mod std {\n    pub fn prelude_stub() -> Int {\n        1\n    }\n}\n"],
        ..CompilerConfig::default()
    });
    let compilation = compiler
        .compile("fn f() -> String {\n    std::pinned_note(\"hi\")\n}\n")
        .expect("the canonical std:: spelling compiles");
    assert_eq!(declared_consts(&compilation.module).len(), 1);

    let imported = compiler
        .compile("import std::{pinned_note};\n\nfn f() -> String {\n    pinned_note(\"hi\")\n}\n")
        .expect("`import std::{pinned_note}` resolves the injected declaration");
    assert_eq!(declared_consts(&imported.module).len(), 1);
}

/// The literal-argument constraint is declared and checked at lowering: a
/// computed coordinate-like argument is rejected with a diagnostic naming
/// the surface, so a program's constant-surface requirement set stays static
/// (the origin-rail note's review amendment;
/// `vixen.machine.requirements-are-static`).
#[test]
fn a_computed_argument_is_a_diagnostic_naming_the_surface() {
    let error = compiler()
        .compile("fn f(name: String) -> String {\n    pinned_note(name)\n}\n")
        .expect_err("a computed argument is rejected, never evaluated");
    let error = format!("{error:?}");
    assert!(
        error.contains("pinned_note string literal"),
        "the diagnostic names the surface and the expected literal: {error}"
    );
}

#[test]
fn arity_is_checked_against_the_declaration() {
    let error = compiler()
        .compile("fn f() -> String {\n    pinned_note(\"a\", \"b\")\n}\n")
        .expect_err("a wrong-arity call is rejected");
    let error = format!("{error:?}");
    assert!(
        error.contains("Arity") || error.contains("arity"),
        "the diagnostic is the arity check's: {error}"
    );
}

/// A constant surface that spells a registered builtin binding's name would
/// silently shadow it (constants resolve before primitive shapes) or be
/// silently dead (`decode` matches even earlier): the adapter-set
/// install-rejection principle applies — the collision is rejected loudly at
/// the seam, never resolved by match ordering.
#[test]
fn a_constant_surface_cannot_shadow_builtin_vocabulary() {
    const SHADOW_FETCH: &[ConstantSurfaceDecl] = &[ConstantSurfaceDecl {
        name: "fetch",
        ..PINNED_NOTE
    }];
    const SHADOW_DECODE: &[ConstantSurfaceDecl] = &[ConstantSurfaceDecl {
        name: "decode",
        ..PINNED_NOTE
    }];
    const SHADOW_TRY_DECODE: &[ConstantSurfaceDecl] = &[ConstantSurfaceDecl {
        name: "try_decode",
        ..PINNED_NOTE
    }];
    for (shadowed, constants) in [
        ("fetch", SHADOW_FETCH),
        ("decode", SHADOW_DECODE),
        ("try_decode", SHADOW_TRY_DECODE),
    ] {
        let error = Compiler::with_config(CompilerConfig {
            constants,
            ..CompilerConfig::default()
        })
        .compile("fn f() -> Int {\n    1\n}\n")
        .expect_err("a builtin-shadowing constant surface is rejected");
        let error = format!("{error:?}");
        assert!(
            error.contains(shadowed) && error.contains("collides with a registered builtin"),
            "the rejection names the colliding surface: {error}"
        );
    }
}

/// The same collision against an *injected* primitive surface: whichever rail
/// injected the name first, a second declaration of it refuses rather than
/// winning by arm order.
#[test]
fn a_constant_surface_cannot_shadow_an_injected_primitive_surface() {
    let fetch = vix::runtime::builtin_primitive_surfaces()
        .into_iter()
        .next()
        .expect("fetch surface");
    let injected = vix::runtime::PrimitiveSurface {
        surface_name: "grab",
        ..fetch
    };
    const GRAB_CONSTANT: &[ConstantSurfaceDecl] = &[ConstantSurfaceDecl {
        name: "grab",
        ..PINNED_NOTE
    }];
    let error = Compiler::with_config(CompilerConfig {
        constants: GRAB_CONSTANT,
        ..CompilerConfig::default()
    })
    .with_primitive_surfaces([injected])
    .compile("fn f() -> Int {\n    1\n}\n")
    .expect_err("a surface-shadowing constant declaration is rejected");
    let error = format!("{error:?}");
    assert!(
        error.contains("grab") && error.contains("injected primitive surface"),
        "the rejection names both rails: {error}"
    );
}

/// A name declared twice in the constant set is the same degenerate
/// declaration set — first-match-wins resolution would hide the second.
#[test]
fn a_constant_surface_declared_twice_is_rejected() {
    const TWICE: &[ConstantSurfaceDecl] = &[PINNED_NOTE, PINNED_NOTE];
    let error = Compiler::with_config(CompilerConfig {
        constants: TWICE,
        ..CompilerConfig::default()
    })
    .compile("fn f() -> Int {\n    1\n}\n")
    .expect_err("a duplicate constant-surface name is rejected");
    let error = format!("{error:?}");
    assert!(
        error.contains("pinned_note") && error.contains("declared twice"),
        "the rejection names the duplicate: {error}"
    );
}

#[test]
fn the_bare_language_declares_no_constant_surfaces() {
    // Without the injected declaration the name is an unknown call — the
    // bare language ships none.
    assert!(
        Compiler::new()
            .compile("fn f() -> String {\n    pinned_note(\"hi\")\n}\n")
            .is_err(),
        "an undeclared constant surface does not resolve"
    );
}
