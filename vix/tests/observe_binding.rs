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

use vix::compiler::Compiler;

fn error(src: &str) -> String {
    format!(
        "{:?}",
        Compiler::default()
            .compile(src)
            .expect_err("expected a compile error")
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
