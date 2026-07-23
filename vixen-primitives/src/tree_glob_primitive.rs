use vix::schema::SchemaPattern;
use vix::vir::{ExternKind, Type};

use crate::rt::{
    CodataDrainCtx, CodataPrimitive, FixtureEntryKind, PrimitiveDescriptor, PrimitiveMachineError,
    PrimitiveMemoPolicy, TarMember, fixture_tree_name, parse_ustar, tree_glob_primitive_id,
    tree_glob_request_type,
};

/// `Tree.glob(pattern) -> Stream<Path, Path>` — the build language's "find
/// files" op. Its result is codata (a stream recipe), so it is realized by a
/// [`CodataPrimitive`] rather than a [`crate::rt::RawPrimitive`]: where a raw
/// primitive completes an async ticket with one interned value, this drains a
/// stream recipe into its ordered path elements when `.collect()` demands them.
///
/// The domain logic — glob pattern matching, and enumeration of a fixture-backed
/// tree's directories or an archive tree's members — lives here in
/// `vixen-primitives`; `vix-core` supplies only the source value and the
/// witnessing context (its `Op::TreeGlob` recipe names this primitive's id).
pub struct TreeGlobPrimitive {
    descriptor: PrimitiveDescriptor,
}

impl Default for TreeGlobPrimitive {
    fn default() -> Self {
        Self {
            descriptor: PrimitiveDescriptor {
                id: tree_glob_primitive_id(),
                request_schema: SchemaPattern::exact(&tree_glob_request_type().schema_ref()),
                response_schema: SchemaPattern::exact(
                    &Type::stream(Type::Path, Type::Path).schema_ref(),
                ),
                failure_schema: SchemaPattern::Var {
                    name: "TreeGlobFailure".to_owned(),
                },
                memo_policy: PrimitiveMemoPolicy::Observed,
                protocol_version: 1,
                capability_schemas: vec![SchemaPattern::exact(
                    &Type::Extern(ExternKind::Tree).schema_ref(),
                )],
            },
        }
    }
}

impl CodataPrimitive for TreeGlobPrimitive {
    fn descriptor(&self) -> &PrimitiveDescriptor {
        &self.descriptor
    }

    fn drain(
        &self,
        pattern: &str,
        ctx: &mut dyn CodataDrainCtx,
    ) -> Result<Vec<String>, PrimitiveMachineError> {
        let (directory, wildcard) = pattern
            .rsplit_once('/')
            .map_or(("", pattern), |(directory, wildcard)| (directory, wildcard));
        let (prefix, suffix) = wildcard.split_once('*').unwrap_or((wildcard, ""));
        let matches = |path: &str| {
            let name = path.rsplit('/').next().unwrap_or(path);
            (directory.is_empty()
                || path
                    .strip_prefix(directory)
                    .is_some_and(|rest| rest.starts_with('/')))
                && name.starts_with(prefix)
                && name.ends_with(suffix)
        };

        // A fixture-backed tree is a lazy handle: enumerate the pattern's
        // directory through the witnessing context, which records the listing as
        // a `Directory` read. Copy the fixture name out first so the immutable
        // borrow of `ctx` ends before the `&mut` directory read.
        let fixture_name = fixture_tree_name(ctx.source_bytes()).map(<[u8]>::to_vec);
        if let Some(name) = fixture_name {
            let name = core::str::from_utf8(&name)
                .map_err(|_| invalid("fixture tree name was not UTF-8"))?;
            let projection = if directory.is_empty() {
                name.to_owned()
            } else {
                format!("{name}/{directory}")
            };
            let entries = ctx.fixture_directory(&projection)?;
            let mut paths = entries
                .into_iter()
                .filter_map(|(entry, kind)| (kind == FixtureEntryKind::File).then_some(entry))
                .map(|entry| {
                    if directory.is_empty() {
                        entry
                    } else {
                        format!("{directory}/{entry}")
                    }
                })
                .filter(|path| matches(path))
                .collect::<Vec<_>>();
            paths.sort();
            return Ok(paths);
        }

        // An archive tree carries its members in its resident bytes — a pure
        // enumeration, no directory read.
        let mut paths = parse_ustar(ctx.source_bytes())
            .map_err(|_| invalid("archive tree resident bytes were malformed"))?
            .into_iter()
            .filter_map(|member| match member {
                TarMember::File { path, .. } if matches(&path) => Some(path),
                _ => None,
            })
            .collect::<Vec<_>>();
        paths.sort();
        Ok(paths)
    }
}

fn invalid(detail: &str) -> PrimitiveMachineError {
    PrimitiveMachineError::AuthorityViolation {
        detail: detail.to_owned(),
    }
}
