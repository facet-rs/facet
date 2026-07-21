//! The single `observe(origin, Mode::…)` binding.
//!
//! `observe` and `refresh` were two compiler intrinsics over one primitive
//! (`observe_primitive_id`), the refresh flag chosen by which name you called.
//! They are now one binding whose mode is a surface argument — `observe(origin,
//! Mode::Observe)` / `observe(origin, Mode::Refresh)` — the twin of `decode(doc,
//! Format)`. `refresh` is retired into a stdlib vix wrapper
//! (`fn refresh<Origin>(origin) -> Blob { observe(origin, Mode::Refresh) }`).
//!
//! An origin is a structural `OriginHint` record that is not surface-nameable
//! and not constructible from vix source, so these tests exercise the binding
//! shape (arity, mode parsing, origin-type enforcement) rather than a full
//! observe-and-run — there is no way to spell a valid origin in vix yet.

use vix::compiler::{Compiler, CompilerConfig};

// The stdlib prelude is supplied by the vixen runtime (`vix-core` alone ships
// none), so `refresh` — a stdlib vix wrapper — resolves only with it installed.
// `PRELUDE_SOURCES` is `&[&str]` data, so naming it across the dev-dependency is
// free of any crate-copy identity hazard.
fn error(src: &str) -> String {
    let compiler = Compiler::with_config(CompilerConfig {
        prelude: vixen_primitives::stdlib::PRELUDE_SOURCES,
        ..CompilerConfig::default()
    });
    format!(
        "{:?}",
        compiler.compile(src).expect_err("expected a compile error")
    )
}

#[test]
fn observe_requires_a_mode_argument() {
    // The mode is now a required surface argument, so the bare one-argument
    // form no longer lowers — the binding expects two.
    let err = error("fn go(origin: String) -> Blob { observe(origin) }\n");
    assert!(err.contains("InvalidArity"), "{err}");
    assert!(err.contains("expected: 2"), "{err}");
}

#[test]
fn observe_reads_the_mode_then_enforces_the_origin_type() {
    // Mode::Refresh parses as the selector; lowering then rejects the origin
    // because a String is not an OriginHint. Reaching the origin-type check —
    // rather than a mode or arity error — is the proof the mode was read.
    let err = error("fn go(origin: String) -> Blob { observe(origin, Mode::Refresh) }\n");
    assert!(err.contains("TypeMismatch"), "{err}");
    assert!(err.contains("OriginHint"), "{err}");
}

#[test]
fn observe_rejects_an_unknown_mode() {
    let err = error("fn go(origin: String) -> Blob { observe(origin, Mode::Spin) }\n");
    assert!(err.contains("unknown observe mode `Mode::Spin`"), "{err}");
}

#[test]
fn observe_rejects_a_non_mode_argument() {
    let err = error("fn go(origin: String) -> Blob { observe(origin, \"json\") }\n");
    assert!(
        err.contains("observe mode `Mode::Observe` or `Mode::Refresh`"),
        "{err}"
    );
}

#[test]
fn refresh_resolves_to_the_stdlib_wrapper_and_forwards_into_observe() {
    // `refresh` is no longer an intrinsic: it resolves to the stdlib vix wrapper
    // `refresh<Origin>(origin) -> Blob { observe(origin, Mode::Refresh) }`. A
    // String origin reaches observe's origin-type check, so the OriginHint
    // mismatch proves refresh both resolved (no unknown-name error) and forwarded
    // into the observe binding with the origin type still enforced.
    let err = error("fn go(origin: String) -> Blob { refresh(origin) }\n");
    assert!(err.contains("TypeMismatch"), "{err}");
    assert!(err.contains("OriginHint"), "{err}");
}

#[test]
fn refresh_lives_in_the_stdlib_not_as_a_builtin() {
    // Without the prelude, `refresh` is an unknown name — the retirement moved it
    // out of the compiler's intrinsic set into stdlib vix source. The error is
    // then unknown-name resolution, never the forwarded observe origin-type check.
    let without_stdlib = Compiler::with_config(CompilerConfig {
        prelude: &[],
        ..CompilerConfig::default()
    });
    let err = format!(
        "{:?}",
        without_stdlib
            .compile("fn go(origin: String) -> Blob { refresh(origin) }\n")
            .expect_err("refresh is not a builtin")
    );
    assert!(!err.contains("OriginHint"), "{err}");
}
