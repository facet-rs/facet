//! Prelude *injection*: the mechanism that merges embedder-supplied prelude
//! source functions into a compilation as ordinary top-level items.
//!
//! The language core ships no prelude of its own — [`inject_prelude`] takes the
//! sources as data ([`crate::compiler::CompilerConfig::prelude`]), so a program
//! compiled by bare `vix-core` sees only what it wrote itself. The `vixen`
//! runtime supplies the actual standard library (`vixen_primitives::stdlib`).
//!
//! A registered prelude function is ordinary vix source: it is merged into the
//! root item set before lowering and resolves and lowers through exactly the
//! same front end as a user-defined function — no bespoke intrinsic, no parallel
//! machinery. Injection is *if-absent*: a program that declares a function of the
//! same name shadows the supplied one, matching the prelude precedence the binder
//! already applies (see [`crate::binding::is_prelude_name`], checked after local
//! scope).

use std::collections::BTreeSet;

use crate::diagnostic::Diagnostics;
use crate::surface::{SurfaceParser, ast};

fn item_name(item: &ast::Item) -> Option<&str> {
    match item {
        ast::Item::Fn(function) => Some(function.name.value.as_str()),
        ast::Item::Struct(record) => Some(record.name.value.as_str()),
        ast::Item::Enum(enumeration) => Some(enumeration.name.value.as_str()),
        ast::Item::Command(command) => Some(command.name.value.as_str()),
        ast::Item::Mod(_) | ast::Item::Import(_) => None,
    }
}

/// Merge the given prelude `sources` into `file` as ordinary top-level items,
/// skipping any whose name the program already declares (the program shadows the
/// stdlib). Each registration is parsed with `parser` so prelude functions travel
/// the same front end as user code. The sources are pure data supplied by the
/// embedder (`vix-core` ships none; the `vixen` runtime supplies
/// `vixen_primitives::stdlib::PRELUDE_SOURCES`).
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
