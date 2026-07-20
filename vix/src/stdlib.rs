//! Standard-library registration: pure vix functions that ship with the
//! compiler and are available to a program as if it had written them itself.
//!
//! A registered prelude function is ordinary vix source. It is merged into the
//! root item set before lowering (`compiler::compile_with_modules`, gated on
//! [`CompilerConfig::stdlib`]), so it resolves and lowers through exactly the
//! same front end as a user-defined function — no bespoke intrinsic, no
//! parallel machinery. Registration is therefore just "these functions exist."
//!
//! Injection is *if-absent*: a program that declares a function of the same
//! name shadows the stdlib one, matching the prelude precedence the binder
//! already applies (see [`crate::binding::is_prelude_name`], checked after
//! local scope).
//!
//! This is the vix-function half of the surface-binding design
//! (`docs/content/registered-primitives.md`, "Surface bindings"): behavioural
//! aliases over primitives (`refresh` over `observe`, …) belong here as pure
//! vix functions rather than as extra primitives or compiler intrinsics. Those
//! particular aliases additionally need the underlying primitive to accept its
//! mode as a surface argument, which is separate, still-pending work; the
//! functions registered here today are the ones that compile against the
//! current surface.
//!
//! Status: on by default ([`CompilerConfig::stdlib`]) — the retired decode and
//! `refresh` intrinsics now live here as vix functions, so a compilation needs
//! the prelude to resolve them. Turning it on perturbed function ids, exact
//! module counts, and the constant-fold of literal decodes (a decode behind a
//! wrapper parameter no longer folds); the ratchet goldens were re-vetted for
//! that, with the decode-fold rungs `#[ignore]`d until pure vix functions
//! constant-fold and folding is restored by default. Tests that assert on the
//! bare user program disable the flag.
//!
//! [`CompilerConfig::stdlib`]: crate::compiler::CompilerConfig::stdlib

use std::collections::BTreeSet;

use crate::diagnostic::Diagnostics;
use crate::surface::{SurfaceParser, ast};

/// Registered prelude items: pure vix source, one top-level item each (`fn`,
/// `enum`, or `struct`). They travel the same front end as user code and are
/// injected into every module. An uninstantiated *generic* fn costs nothing (it
/// lowers only per call), but a non-generic fn (`is_blank`) and the enums
/// (`Format`, `Mode`) are always emitted into the module — there is no
/// reachability pruning — so they perturb module counts even when unused.
///
/// `json_decode`/`toml_decode` are the retired decode intrinsics, now ordinary
/// vix functions over the single `decode(document, Format)` binding: the format
/// is a request field, and the target `T` is forwarded from the caller's
/// expected type by return-position inference. `try_json_decode`/`try_toml_decode`
/// are the fallible twins over `try_decode(document, Format)`, returning
/// `Result<T, DecodeError>`; `T` is recovered from the expected `Result` — no
/// call-site turbofish, matching the language's inference-only instantiation.
///
/// `refresh` is the retired observe *mode* intrinsic, now an ordinary vix
/// function over the single `observe(origin, Mode)` binding — `refresh` is
/// `observe` with `Mode::Refresh`, exactly as `refresh` and `observe` already
/// share one primitive (`observe_primitive_id`). The origin type (`OriginHint`)
/// is not surface-nameable, so — like `json_decode`'s `T` — the parameter is
/// generic and the `observe` binding enforces the real origin type at the call.
pub const PRELUDE_SOURCES: &[&str] = &[
    "fn is_blank(text: String) -> Bool {\n    text == \"\"\n}\n",
    "enum Format {\n    Json,\n    Toml,\n}\n",
    "fn json_decode<T>(text: String) -> T {\n    decode(text, Format::Json)\n}\n",
    "fn toml_decode<T>(text: String) -> T {\n    decode(text, Format::Toml)\n}\n",
    "fn try_json_decode<T>(text: String) -> Result<T, DecodeError> {\n    try_decode(text, Format::Json)\n}\n",
    "fn try_toml_decode<T>(text: String) -> Result<T, DecodeError> {\n    try_decode(text, Format::Toml)\n}\n",
    "enum Mode {\n    Observe,\n    Refresh,\n}\n",
    "fn refresh<Origin>(origin: Origin) -> Blob {\n    observe(origin, Mode::Refresh)\n}\n",
];

fn item_name(item: &ast::Item) -> Option<&str> {
    match item {
        ast::Item::Fn(function) => Some(function.name.value.as_str()),
        ast::Item::Struct(record) => Some(record.name.value.as_str()),
        ast::Item::Enum(enumeration) => Some(enumeration.name.value.as_str()),
        ast::Item::Command(command) => Some(command.name.value.as_str()),
        ast::Item::Import(_) => None,
    }
}

/// Merge the given prelude `sources` into `file` as ordinary top-level items,
/// skipping any whose name the program already declares (the program shadows the
/// stdlib). Each registration is parsed with `parser` so stdlib functions travel
/// the same front end as user code. The sources are pure data supplied by the
/// embedder (`vix-core` ships none; the `vixen` runtime supplies
/// [`PRELUDE_SOURCES`]).
pub fn inject_prelude(
    parser: &SurfaceParser,
    sources: &[&str],
    file: &mut ast::SourceFile,
) -> Result<(), Diagnostics> {
    let declared: BTreeSet<String> = file
        .items
        .iter()
        .filter_map(|item| item_name(item).map(str::to_owned))
        .collect();

    for source in sources {
        let parsed = parser.parse(source)?;
        for item in parsed.items {
            let shadowed = item_name(&item).is_some_and(|name| declared.contains(name));
            if !shadowed {
                file.items.push(item);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::compiler::{Compiler, CompilerConfig};

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
