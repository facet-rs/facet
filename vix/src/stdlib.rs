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
//! Status: the mechanism is flag-gated ([`CompilerConfig::stdlib`], default
//! off). Turning it on for every compilation perturbs function ids and the
//! machine's module-set hash, so that flip belongs behind a full golden/ratchet
//! re-vet and is not done here.
//!
//! [`CompilerConfig::stdlib`]: crate::compiler::CompilerConfig::stdlib

use std::collections::BTreeSet;

use crate::diagnostic::Diagnostics;
use crate::surface::{SurfaceParser, ast};

/// Registered prelude functions: pure vix source, one top-level `fn` each.
///
/// A deliberately tiny first registration (`is_blank`) exercises the loader end
/// to end against the current surface. Add entries here to grow the prelude.
const PRELUDE_FUNCTIONS: &[&str] = &["fn is_blank(text: String) -> Bool {\n    text == \"\"\n}\n"];

fn item_name(item: &ast::Item) -> Option<&str> {
    match item {
        ast::Item::Fn(function) => Some(function.name.value.as_str()),
        ast::Item::Struct(record) => Some(record.name.value.as_str()),
        ast::Item::Enum(enumeration) => Some(enumeration.name.value.as_str()),
        ast::Item::Import(_) => None,
    }
}

/// Merge the registered prelude functions into `file` as ordinary top-level
/// items, skipping any whose name the program already declares (the program
/// shadows the stdlib). Each registration is parsed with `parser` so stdlib
/// functions travel the same front end as user code.
pub fn inject_prelude(
    parser: &SurfaceParser,
    file: &mut ast::SourceFile,
) -> Result<(), Diagnostics> {
    let declared: BTreeSet<String> = file
        .items
        .iter()
        .filter_map(|item| item_name(item).map(str::to_owned))
        .collect();

    for source in PRELUDE_FUNCTIONS {
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
            stdlib: true,
            ..CompilerConfig::default()
        })
    }

    #[test]
    fn registered_prelude_fn_is_callable_like_user_code() {
        let program = "fn check(text: String) -> Bool {\n    is_blank(text)\n}\n";

        // Without the stdlib, `is_blank` is an unknown name…
        assert!(
            Compiler::default().compile(program).is_err(),
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
