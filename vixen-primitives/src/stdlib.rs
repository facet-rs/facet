//! The pure-vix standard library the `vixen` runtime installs into a
//! compilation as a prelude.
//!
//! Each entry of [`PRELUDE_SOURCES`] is ordinary vix source — a single
//! top-level item authored in its own `.vix` file under `stdlib/`. They are
//! merged into every module's root item set before lowering (via
//! `vix::prelude::inject_prelude`, driven by
//! `vix::compiler::CompilerConfig::prelude`), so each resolves and lowers through
//! exactly the same front end as a user-defined function — no bespoke intrinsic,
//! no parallel machinery. Injection is *if-absent*: a program that declares a
//! function of the same name shadows the stdlib one.
//!
//! `json_decode`/`toml_decode` are the retired decode intrinsics, now ordinary
//! vix functions over the single `decode(document, Format)` binding: the format
//! is a request field, and the target `T` is forwarded from the caller's
//! expected type by return-position inference. `try_json_decode`/`try_toml_decode`
//! are the fallible twins over `try_decode(document, Format)`, returning
//! `Result<T, DecodeError>`. `refresh` is the retired observe *mode* intrinsic,
//! now `observe(origin, Mode::Refresh)`.
//!
//! Order is significant — it affects function ids, module counts, and the
//! constant-fold of literal decodes — so it must match what the ratchet goldens
//! were vetted against: `is_blank`, `Format`, `json_decode`, `toml_decode`,
//! `try_json_decode`, `try_toml_decode`, `Mode`, `refresh`.

/// The registered prelude items, one self-contained vix top-level item each, in
/// the order they are injected.
pub const PRELUDE_SOURCES: &[&str] = &[
    include_str!("stdlib/is_blank.vix"),
    include_str!("stdlib/format.vix"),
    include_str!("stdlib/json_decode.vix"),
    include_str!("stdlib/toml_decode.vix"),
    include_str!("stdlib/try_json_decode.vix"),
    include_str!("stdlib/try_toml_decode.vix"),
    include_str!("stdlib/mode.vix"),
    include_str!("stdlib/refresh.vix"),
];

#[cfg(test)]
mod tests {
    use vix::compiler::{Compiler, CompilerConfig};

    fn with_stdlib() -> Compiler {
        Compiler::with_config(CompilerConfig {
            prelude: super::PRELUDE_SOURCES,
            ..CompilerConfig::default()
        })
    }

    fn without_stdlib() -> Compiler {
        Compiler::with_config(CompilerConfig {
            prelude: &[],
            ..CompilerConfig::default()
        })
    }

    #[test]
    fn registered_prelude_fn_is_callable_like_user_code() {
        let program = "fn check(text: String) -> Bool {\n    is_blank(text)\n}\n";

        // Without the stdlib, `is_blank` is an unknown name…
        assert!(
            without_stdlib().compile(program).is_err(),
            "is_blank is not available without the stdlib"
        );
        // …with it registered, the program compiles as if `is_blank` were
        // written right here — no special path.
        assert!(
            with_stdlib().compile(program).is_ok(),
            "registered prelude fn resolves and lowers like user code"
        );
    }

    #[test]
    fn a_program_may_shadow_a_registered_prelude_fn() {
        // The program declares its own `is_blank`; injection is if-absent, so
        // this compiles rather than raising a duplicate definition.
        let program = concat!(
            "fn is_blank(text: String) -> Bool {\n    text == \"nope\"\n}\n",
            "fn check(text: String) -> Bool {\n    is_blank(text)\n}\n",
        );
        assert!(with_stdlib().compile(program).is_ok());
    }
}
