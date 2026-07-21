//! The pure-vix standard library the `vixen` runtime installs under `std::`.
//!
//! Each entry of [`PRELUDE_SOURCES`] is ordinary Vix source assembled from the
//! items authored under `stdlib/`. They are merged into a compilation before
//! lowering (via `vix::prelude::inject_prelude`, driven by
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

/// The canonical `std` module assembled from the separately authored Vix items.
pub const STD_MODULE_SOURCE: &str = concat!(
    "mod std {\n",
    include_str!("stdlib/is_blank.vix"),
    include_str!("stdlib/format.vix"),
    include_str!("stdlib/json_decode.vix"),
    include_str!("stdlib/toml_decode.vix"),
    include_str!("stdlib/try_json_decode.vix"),
    include_str!("stdlib/try_toml_decode.vix"),
    include_str!("stdlib/mode.vix"),
    include_str!("stdlib/refresh.vix"),
    "}\n",
);

/// The registered sources in injection order. The top-level entries retain the
/// historical unqualified spellings as compatibility aliases; new code should
/// use the canonical items in [`STD_MODULE_SOURCE`] through `std::`.
pub const PRELUDE_SOURCES: &[&str] = &[
    include_str!("stdlib/is_blank.vix"),
    include_str!("stdlib/format.vix"),
    include_str!("stdlib/json_decode.vix"),
    include_str!("stdlib/toml_decode.vix"),
    include_str!("stdlib/try_json_decode.vix"),
    include_str!("stdlib/try_toml_decode.vix"),
    include_str!("stdlib/mode.vix"),
    include_str!("stdlib/refresh.vix"),
    STD_MODULE_SOURCE,
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
    fn registered_std_fn_is_callable_through_its_module() {
        let program = "fn check(text: String) -> Bool {\n    std::is_blank(text)\n}\n";

        assert!(
            without_stdlib().compile(program).is_err(),
            "std is not available without the vixen standard library"
        );
        assert!(
            with_stdlib().compile(program).is_ok(),
            "registered std function resolves and lowers through its module"
        );
    }

    #[test]
    fn std_items_can_be_imported() {
        let program = r#"
import std::{fetch, fixture_registry};

fn fetch_fixture() -> Blob {
    fetch(fixture_registry().url("case.crate"))
}

"#;

        with_stdlib()
            .compile(program)
            .unwrap_or_else(|diagnostics| panic!("std imports compile: {diagnostics:#?}"));
    }

    #[test]
    fn primitives_and_std_types_resolve_through_std() {
        let program = r#"
struct Row { name: String }

fn fetch_from_fixture() -> Blob {
    std::fetch(std::fixture_registry().url("case.crate"))
}

fn decode_row() -> Row {
    std::decode("{\"name\":\"vix\"}", std::Format::Json)
}

fn fetch_fresh() -> Blob {
    std::observe(
        std::fixture_registry().coordinate("case.crate"),
        std::Mode::Refresh,
    )
}
"#;

        with_stdlib()
            .compile(program)
            .unwrap_or_else(|diagnostics| {
                panic!("primitives, intrinsics, and enum variants resolve through std: {diagnostics:#?}")
            });
    }

    #[test]
    fn adding_std_does_not_shift_user_function_ids() {
        let program = "fn answer() -> Int { 42 }\n";
        let bare = without_stdlib()
            .compile(program)
            .expect("bare Vix compiles");
        let with_std = with_stdlib()
            .compile(program)
            .expect("Vix with std compiles");
        let function_id = |compilation: &vix::compiler::Compilation| {
            compilation
                .module
                .functions
                .iter()
                .find(|function| function.name == "answer")
                .expect("answer function")
                .id
        };

        assert_eq!(function_id(&bare), function_id(&with_std));
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
