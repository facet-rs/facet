//! The derived stdlib layer — `arrays`, `option`, `strings`, `paths`.
//!
//! Every function under test is pure std vix over the axiom methods, so these
//! run through the production path with ZERO host calls: that assertion is the
//! point of the layer. A helper that needed a primitive would be in the wrong
//! layer (`surface-0.1-scope-and-layering`, the EFFECT/AXIOM/VIX table), and
//! `pure_host_calls == 0` is how that stays true rather than merely intended.

use vixen_runtime::ratchet::run_source;

fn run(label: &str, source: &str, checks: usize) {
    let report = run_source(source).unwrap_or_else(|error| panic!("{label} compiles: {error:#?}"));
    assert!(report.passed(), "{label} checks pass: {:?}", report.plain.checks);
    assert!(report.agrees(), "{label} agrees across lanes");
    assert_eq!(report.plain.checks.len(), checks, "{label} check count");
    assert_eq!(
        report.plain.counters.pure_host_calls, 0,
        "{label} is pure vix — no host call"
    );
}

#[test]
fn array_join_take_and_to_set() {
    run(
        "arrays",
        r#"
#[test]
fn t() -> Stream<Check> {
    yield expect_eq((["a", "b", "c"]).join("-"), "a-b-c");
    yield expect_eq((["solo"]).join("-"), "solo");
    let none: [String] = [];
    yield expect_eq(none.join("-"), "");
    // A leading empty element survives — the Option accumulator exists for this.
    yield expect_eq((["", "b"]).join("-"), "-b");

    let none_int: [Int] = [];
    yield expect_eq(([1, 2, 3, 4]).take(2), [1, 2]);
    yield expect_eq(([1, 2]).take(9), [1, 2]);
    yield expect_eq(([1, 2]).take(0), none_int);

    // Canonical key order, and duplicates collapsed.
    yield expect_eq(([3, 1, 3]).to_set().values(), [1, 3]);
    yield expect_eq(([3, 1, 3]).to_set().len(), 2);
}
"#,
        9,
    );
}

#[test]
fn option_combinators() {
    run(
        "option",
        r#"
fn some_edition() -> Option<String> { Some("2021") }
fn no_edition() -> Option<String> { None }

#[test]
fn t() -> Stream<Check> {
    yield expect_eq(some_edition().unwrap_or("2015"), "2021");
    yield expect_eq(no_edition().unwrap_or("2015"), "2015");
    yield expect(some_edition().is_some());
    yield expect(no_edition().is_none());
    yield expect_eq(no_edition().or_else(Some("2015")), Some("2015"));
    yield expect_eq(some_edition().or_else(Some("2015")), Some("2021"));
}
"#,
        6,
    );
}

#[test]
fn string_surgery() {
    run(
        "strings",
        r#"
#[test]
fn t() -> Stream<Check> {
    yield expect_eq("a,b,c".split(","), ["a", "b", "c"]);
    // A missing separator is NOT a failure — the whole text, one element.
    yield expect_eq("abc".split(","), ["abc"]);
    yield expect_eq("a,,b".split(","), ["a", "", "b"]);
    yield expect_eq("x86_64-unknown-linux-gnu".split("-").len(), 4);

    yield expect("src/lib.rs".starts_with("src/"));
    yield expect(!"lib.rs".starts_with("src/"));
    yield expect_eq("src/lib.rs".strip_prefix("src/"), Some("lib.rs"));
    yield expect_eq("lib.rs".strip_prefix("src/"), None);

    yield expect_eq("small-crate".replace("-", "_"), "small_crate");
    yield expect_eq("a.b.c".replace(".", "/"), "a/b/c");
}
"#,
        10,
    );
}

#[test]
fn path_components() {
    run(
        "paths",
        r#"
#[test]
fn t() -> Stream<Check> {
    let manifest = p"crates" / "taxon" / "Cargo.toml";

    yield expect_eq(manifest.segments(), ["crates", "taxon", "Cargo.toml"]);
    yield expect_eq(manifest.file_name(), Some("Cargo.toml"));
    yield expect_eq(manifest.parent_text(), "crates/taxon");
    yield expect_eq(manifest.extension(), Some("toml"));
    yield expect_eq(manifest.stem(), Some("Cargo"));

    let leaf = p"lib.rs";
    yield expect_eq(leaf.parent_text(), "");
    yield expect_eq(leaf.extension(), Some("rs"));

    let bare = p"README";
    let no_extension: Option<String> = None;
    yield expect_eq(bare.extension(), no_extension);
    yield expect_eq(bare.stem(), Some("README"));

    // A trailing dot: the text after the last dot is empty, and says so rather
    // than pretending there was no dot.
    yield expect_eq((p"foo.").extension(), Some(""));
    yield expect_eq((p"foo.").stem(), Some("foo"));

    // The crate-name shape the build walk needs off a globbed source path.
    yield expect_eq(("libsmall_crate.rlib").split(".")[0], "libsmall_crate");
}
"#,
        12,
    );
}
