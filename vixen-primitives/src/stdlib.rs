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
//! `Result<T, DecodeError>`.
//!
//! Order is significant — it affects function ids, module counts, and the
//! constant-fold of literal decodes — so it must match what the ratchet goldens
//! were vetted against: `Format`, `json_decode`, `toml_decode`,
//! `try_json_decode`, `try_toml_decode`.

/// The canonical `std` module assembled from the separately authored Vix items.
pub const STD_MODULE_SOURCE: &str = concat!(
    "mod std {\n",
    include_str!("stdlib/format.vix"),
    include_str!("stdlib/json_decode.vix"),
    include_str!("stdlib/toml_decode.vix"),
    include_str!("stdlib/try_json_decode.vix"),
    include_str!("stdlib/try_toml_decode.vix"),
    include_str!("stdlib/combinators.vix"),
    "}\n",
);

/// The registered sources in injection order. The top-level entries retain the
/// historical unqualified spellings as compatibility aliases; new code should
/// use the canonical items in [`STD_MODULE_SOURCE`] through `std::`.
pub const PRELUDE_SOURCES: &[&str] = &[
    include_str!("stdlib/format.vix"),
    include_str!("stdlib/json_decode.vix"),
    include_str!("stdlib/toml_decode.vix"),
    include_str!("stdlib/try_json_decode.vix"),
    include_str!("stdlib/try_toml_decode.vix"),
    include_str!("stdlib/combinators.vix"),
    STD_MODULE_SOURCE,
];

#[cfg(test)]
mod tests {
    use vix::compiler::{Compiler, CompilerConfig};

    fn with_stdlib() -> Compiler {
        Compiler::with_config(CompilerConfig {
            prelude: super::PRELUDE_SOURCES,
            methods: crate::DOMAIN_METHODS,
            host_types: crate::HOST_TYPES,
            ..CompilerConfig::default()
        })
    }

    fn without_stdlib() -> Compiler {
        Compiler::with_config(CompilerConfig {
            prelude: &[],
            ..CompilerConfig::default()
        })
    }

    // Throwaway prelude items standing in for "some registered stdlib function",
    // so the injection-mechanism tests below don't depend on which real stdlib
    // functions ship. Mirrors the real prelude's shape: an unqualified compat
    // alias plus the canonical item under `std`.
    const DEMO_PRELUDE: &[&str] = &[
        "pub fn demo_blank(text: String) -> Bool {\n    text == \"\"\n}\n",
        "mod std {\n    pub fn demo_blank(text: String) -> Bool {\n        text == \"\"\n    }\n}\n",
    ];

    fn with_prelude(prelude: &'static [&'static str]) -> Compiler {
        Compiler::with_config(CompilerConfig {
            prelude,
            ..CompilerConfig::default()
        })
    }

    #[test]
    fn registered_prelude_fn_is_callable_like_user_code() {
        let program = "fn check(text: String) -> Bool {\n    demo_blank(text)\n}\n";

        // Without a prelude, `demo_blank` is an unknown name…
        assert!(
            with_prelude(&[]).compile(program).is_err(),
            "demo_blank is not available without the prelude"
        );
        // …with it registered, the program compiles as if `demo_blank` were
        // written right here — no special path.
        assert!(
            with_prelude(DEMO_PRELUDE).compile(program).is_ok(),
            "registered prelude fn resolves and lowers like user code"
        );
    }

    #[test]
    fn registered_std_fn_is_callable_through_its_module() {
        let program = "fn check(text: String) -> Bool {\n    std::demo_blank(text)\n}\n";

        assert!(
            with_prelude(&[]).compile(program).is_err(),
            "std is not available without a prelude"
        );
        assert!(
            with_prelude(DEMO_PRELUDE).compile(program).is_ok(),
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
        // The program declares its own `demo_blank`; injection is if-absent, so
        // this compiles rather than raising a duplicate definition.
        let program = concat!(
            "fn demo_blank(text: String) -> Bool {\n    text == \"nope\"\n}\n",
            "fn check(text: String) -> Bool {\n    demo_blank(text)\n}\n",
        );
        assert!(with_prelude(DEMO_PRELUDE).compile(program).is_ok());
    }

    #[test]
    fn the_combinators_are_std_vix_reached_by_method_syntax() {
        // `any`/`all`/`contains`/`find_min`/`find_max` are std vix over `fold` /
        // the stream core — no longer dedicated ops — reached through the generic
        // uniform method-call fallback (and, for the stream ones, inlined).
        let program = concat!(
            "#[test]\n",
            "fn t() -> Stream<Check> {\n",
            "    yield expect(([2, 4, 6]).all(|n| n.rem(2) == 0));\n",
            "    yield expect(([1, 2, 3]).any(|n| n == 2));\n",
            "    yield expect(([1, 2, 3]).contains(2));\n",
            "    yield expect_eq([3, 1, 2].stream().find_min(|n| n > 1), Some(2));\n",
            "    yield expect_eq([3, 1, 2].stream().find_max(|_| true), Some(3));\n",
            "}\n",
        );
        assert!(
            with_stdlib().compile(program).is_ok(),
            "combinators resolve via the std prelude"
        );
        // Without the stdlib they are not builtin either — migrated off the ops.
        assert!(without_stdlib().compile(program).is_err());
    }

    #[test]
    fn a_concrete_method_and_the_generic_contains_share_a_name() {
        // A receiver-typed `contains` (on `Bag`) coexists with the generic array
        // `contains<T>`: method dispatch resolves each by receiver type.
        let program = concat!(
            "struct Bag { xs: [Int] }\n",
            "fn contains(bag: Bag) where { value: Int } -> Bool {\n",
            "    (bag.xs).any(|x| x == value)\n",
            "}\n",
            "#[test]\n",
            "fn t() -> Stream<Check> {\n",
            "    let bag = Bag { xs: [1, 2, 3] };\n",
            "    yield expect(bag.contains(2));\n",
            "    yield expect(([4, 5]).contains(5));\n",
            "}\n",
        );
        assert!(
            with_stdlib().compile(program).is_ok(),
            "concrete and generic `contains` coexist, dispatched by receiver type"
        );
    }
}
