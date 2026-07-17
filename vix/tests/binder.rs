//! Binder oracle: scopes, references, rename sets over lua.vix + targeted snippets.

use vix::VixParser;
use vix::ast::Span;
use vix::binder::{self, SymbolId, SymbolKind};

fn lua_source() -> String {
    std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../playgrounds/snark/src/bundled/vix/samples/lua.vix"
    ))
    .expect("read lua.vix corpus")
}

/// Byte offsets of every occurrence of `name` in `src` as a standalone word.
fn word_offsets(src: &str, name: &str) -> Vec<u32> {
    let is_word = |b: Option<u8>| b.is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_');
    src.match_indices(name)
        .filter(|(i, _)| {
            !is_word(src.as_bytes().get(i.wrapping_sub(1)).copied())
                && !is_word(src.as_bytes().get(i + name.len()).copied())
        })
        .map(|(i, _)| i as u32)
        .collect()
}

fn starts(spans: &[Span]) -> Vec<u32> {
    spans.iter().map(|s| s.start).collect()
}

#[test]
fn lua_sketch_binds() {
    let src = lua_source();
    let file = VixParser::new().parse(&src).expect("lua.vix parses");
    let b = binder::bind(&file);

    // `units`: let def + one ref (`units.map(…)`) — the rename set is exactly the
    // word occurrences in the source.
    let units_at = word_offsets(&src, "units");
    assert_eq!(units_at.len(), 2);
    let units = b.symbol_at(units_at[0]).expect("units resolves");
    assert_eq!(b.symbol(units).kind, SymbolKind::Let);
    assert_eq!(starts(&b.occurrences(units)), units_at);
    let edits = b.rename_edits(units, "translation_units");
    assert_eq!(edits.len(), 2);

    // `cc` in `fn lua`: let def + two `object(cc, …)` refs + the `cc!` command
    // invocation in the tail — command names ARE value references.
    let cc_at = word_offsets(&src, "cc");
    // 0: doc comment mention (not code). fn object: param def + cc! command (1..3).
    // fn lua: let def + 2 `object(cc, …)` refs + cc! tail (3..7).
    assert_eq!(cc_at.len(), 7);
    assert_eq!(b.symbol_at(cc_at[0]), None, "comment text binds nothing");
    let object_cc = b.symbol_at(cc_at[1]).expect("object's cc param");
    assert_eq!(b.symbol(object_cc).kind, SymbolKind::Param);
    assert_eq!(starts(&b.occurrences(object_cc)), cc_at[1..3]);
    let lua_cc = b.symbol_at(cc_at[3]).expect("lua's cc let");
    assert_eq!(b.symbol(lua_cc).kind, SymbolKind::Let);
    assert_eq!(starts(&b.occurrences(lua_cc)), cc_at[3..]);

    // The two closures bind separate `u`s: filter's has 3 refs, map's has 1.
    let u_at = word_offsets(&src, "u");
    assert_eq!(u_at.len(), 6);
    let filter_u = b.symbol_at(u_at[0]).expect("filter's u");
    assert_eq!(b.symbol(filter_u).kind, SymbolKind::ClosureParam);
    assert_eq!(starts(&b.occurrences(filter_u)), u_at[..4]);
    let map_u = b.symbol_at(u_at[4]).expect("map's u");
    assert_ne!(filter_u, map_u);
    assert_eq!(starts(&b.occurrences(map_u)), u_at[4..]);

    // `sources`: doc-comment mention, fn def, one call ref — the definition the
    // call resolves to is the fn's NAME span.
    let sources_at = word_offsets(&src, "sources");
    assert_eq!(sources_at.len(), 3);
    let sources = b.symbol_at(sources_at[2]).expect("sources call resolves");
    assert_eq!(b.symbol(sources).kind, SymbolKind::Fn);
    assert_eq!(b.symbol(sources).def.start, sources_at[1]);

    // `Cc::acquire` resolves `Cc` to the import from `use caps::{Cc, Ar}`.
    let cc_import = b
        .symbols()
        .find(|(_, s)| s.name == "Cc" && s.kind == SymbolKind::Import)
        .map(|(id, _)| id)
        .expect("Cc imported");
    assert!(!b.references(cc_import).is_empty());

    // Unresolved is a VALUE list, not an error list: the `extract` primitive
    // still awaiting a prelude binding, constructor-like patterns, and the
    // unimported Flag type. `fetch` now resolves through the prelude binding
    // registry (`vix::binding`), so it is NOT here.
    let unresolved: Vec<&str> = b.unresolved().iter().map(|s| s.value.as_str()).collect();
    assert!(
        !unresolved.contains(&"fetch"),
        "fetch resolves through the prelude registry, got {unresolved:?}"
    );
    for expected in ["extract", "Linux", "Macos", "Flag"] {
        assert!(
            unresolved.contains(&expected),
            "expected `{expected}` in unresolved, got {unresolved:?}"
        );
    }
    // …and nothing that IS in scope leaks into it.
    for bound in [
        "cc", "ar", "src", "defines", "units", "objs", "lib", "main", "u",
    ] {
        assert!(
            !unresolved.contains(&bound),
            "`{bound}` should resolve, unresolved: {unresolved:?}"
        );
    }
}

#[test]
fn let_shadowing_is_sequential() {
    // `let x = g(x)` sees the OUTER x (the param); the tail sees the let.
    let src = "fn f(x: Tree) -> Tree { let x = g(x); x }";
    let file = VixParser::new().parse(src).expect("snippet parses");
    let b = binder::bind(&file);

    let x_at = word_offsets(src, "x");
    assert_eq!(x_at.len(), 4); // param, let, initializer ref, tail ref

    let param = b.symbol_at(x_at[0]).expect("param x");
    assert_eq!(b.symbol(param).kind, SymbolKind::Param);
    let shadow = b.symbol_at(x_at[1]).expect("let x");
    assert_eq!(b.symbol(shadow).kind, SymbolKind::Let);
    assert_ne!(param, shadow);

    // g(x): x resolves to the param, not the let being defined.
    assert_eq!(b.symbol_at(x_at[2]), Some(param));
    // tail x: the shadow.
    assert_eq!(b.symbol_at(x_at[3]), Some(shadow));
}

#[test]
fn rename_produces_disjoint_sorted_edits() {
    let src = lua_source();
    let file = VixParser::new().parse(&src).expect("lua.vix parses");
    let b = binder::bind(&file);

    for (id, _) in b.symbols() {
        let occ = b.occurrences(id);
        for pair in occ.windows(2) {
            assert!(
                pair[0].end <= pair[1].start,
                "occurrences overlap for {:?}: {pair:?}",
                b.symbol(SymbolId(id.0))
            );
        }
    }
}
