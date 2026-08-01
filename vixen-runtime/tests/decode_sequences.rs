//! Typed decode of SEQUENCES — the gap that made a `Cargo.lock` undecodable.
//!
//! Before this, `decode_value` handled `Int`/`Bool`/`String`/`Record`/`Enum`/
//! `Option` and every array target fell through to `UnsupportedTarget`. Rungs
//! 062–066 never noticed because not one of them has an array field, and an
//! infallible `decode` reports the failure as `InvalidRequest` — a machine
//! fault that kills the run — so the symptom did not name its cause either.
//!
//! Both lowering paths are exercised on purpose. A decode of a LITERAL document
//! constant-folds in `compiler::lower_decoded_value`; a decode of a value read
//! at run time goes through `runtime::decode_value::primitive_value_from_decoded`.
//! They are separate arms and only one of them is exercised by the corpus, so a
//! test that used a literal alone would leave the path a real build takes cold.

use vixen_runtime::ratchet::run_source;

fn run(label: &str, source: &str, checks: usize) {
    let report = run_source(source).unwrap_or_else(|error| panic!("{label}: {error:#?}"));
    assert!(report.passed(), "{label} checks pass: {:?}", report.plain.checks);
    assert!(report.agrees(), "{label} agrees across lanes");
    assert_eq!(report.plain.checks.len(), checks, "{label} check count");
}

#[test]
fn arrays_of_scalars_and_records_decode() {
    run(
        "literals",
        r#"
struct Row { name: String }
struct Doc { names: [String], rows: [Row], counts: [Int] }

#[test]
fn t() -> Stream<Check> {
    let d: Doc = json_decode(
        "{\"names\":[\"a\",\"b\"],\"rows\":[{\"name\":\"x\"},{\"name\":\"y\"}],\"counts\":[1,2,3]}"
    );
    yield expect_eq(d.names, ["a", "b"]);
    yield expect_eq(d.rows.len(), 2);
    yield expect_eq(d.rows[1].name, "y");
    yield expect_eq(d.counts, [1, 2, 3]);
}
"#,
        4,
    );
}

#[test]
fn an_empty_array_decodes_to_an_empty_array() {
    run(
        "empty",
        r#"
struct Doc { names: [String] }

#[test]
fn t() -> Stream<Check> {
    let d: Doc = json_decode("{\"names\":[]}");
    let none: [String] = [];
    yield expect_eq(d.names, none);
    yield expect_eq(d.names.len(), 0);
}
"#,
        2,
    );
}

#[test]
fn toml_array_of_tables_is_one_array_across_many_headers() {
    // `[[package]]` repeats the field key, one single-element sequence per
    // block. Accumulating them IS what the spelling means.
    run(
        "array-of-tables",
        r#"
struct Package { name: String, version: String }
struct Lock { version: Int, package: [Package] }

#[test]
fn t() -> Stream<Check> {
    let lock: Lock = toml_decode(
        "version = 4\n\n[[package]]\nname = \"libc\"\nversion = \"0.2.155\"\n\n[[package]]\nname = \"log\"\nversion = \"0.4.22\"\n"
    );
    yield expect_eq(lock.version, 4);
    yield expect_eq(lock.package.len(), 2);
    // Document order, not sorted order.
    yield expect_eq(lock.package[0].name, "libc");
    yield expect_eq(lock.package[1].name, "log");
    yield expect_eq(lock.package[0].version, "0.2.155");
}
"#,
        5,
    );
}

#[test]
fn a_single_table_header_is_still_a_one_element_array() {
    run(
        "single-header",
        r#"
struct Package { name: String }
struct Lock { package: [Package] }

#[test]
fn t() -> Stream<Check> {
    let lock: Lock = toml_decode("[[package]]\nname = \"only\"\n");
    yield expect_eq(lock.package.len(), 1);
    yield expect_eq(lock.package[0].name, "only");
}
"#,
        2,
    );
}

#[test]
fn optional_and_nested_arrays_decode() {
    run(
        "optional-nested",
        r#"
struct Package { name: String, dependencies: Option<[String]> }
struct Lock { package: [Package] }

fn deps(p: Package) -> [String] {
    let none: [String] = [];
    p.dependencies.unwrap_or(none)
}

#[test]
fn t() -> Stream<Check> {
    let lock: Lock = toml_decode(
        "[[package]]\nname = \"root\"\ndependencies = [\"a\", \"b\"]\n\n[[package]]\nname = \"leaf\"\n"
    );
    yield expect_eq(deps(lock.package[0]), ["a", "b"]);
    // An absent array field is None, not an empty array — the distinction the
    // lock relies on to tell a leaf from a package with no deps recorded.
    yield expect_none(lock.package[1].dependencies);
    yield expect_eq(deps(lock.package[1]).len(), 0);
}
"#,
        3,
    );
}

#[test]
fn a_runtime_document_decodes_through_the_primitive() {
    // The path a real build takes: bytes read from a tree, decoded at RUN time
    // through `DecodePrimitive` rather than constant-folded in the compiler.
    run(
        "runtime",
        r#"
struct Package { name: String, version: String, dependencies: Option<[String]> }
struct Lock { version: Int, package: [Package] }

#[test]
fn t() -> Stream<Check> {
    let tree = fixture_tree("locked-crate");
    let lock: Lock = toml_decode((tree / "Cargo.lock").text());
    yield expect_eq(lock.package.len(), 3);
    yield expect_eq(lock.package[2].name, "small-crate");
    yield expect_eq(lock.package[2].dependencies, Some(["libc", "log"]));
}
"#,
        3,
    );
}

#[test]
fn a_json_duplicate_key_is_still_a_duplicate() {
    // The array-of-tables accumulation is TOML-only. A repeated key in JSON is a
    // genuine duplicate and must stay a typed failure, or the fix would have
    // bought TOML's spelling by loosening every format.
    let source = r#"
struct Doc { names: [String] }

#[test]
fn t() -> Stream<Check> {
    let d: Result<Doc, DecodeError> = try_json_decode("{\"names\":[\"a\"],\"names\":[\"b\"]}");
    yield match d {
        Ok(_) => expect(false),
        Err(e) => expect_eq(e.kind, "duplicate-field"),
    };
}
"#;
    let report = run_source(source).expect("compiles and runs");
    assert!(
        report.passed(),
        "the duplicate is typed and reported: {:?}",
        report.plain.checks
    );
}

#[test]
fn a_non_array_document_for_an_array_target_is_typed() {
    let source = r#"
struct Doc { names: [String] }

#[test]
fn t() -> Stream<Check> {
    let d: Result<Doc, DecodeError> = try_json_decode("{\"names\":\"not-an-array\"}");
    yield match d {
        Ok(_) => expect(false),
        Err(e) => expect_eq(e.kind, "expected-array"),
    };
}
"#;
    let report = run_source(source).expect("compiles and runs");
    assert!(
        report.passed(),
        "the shape mismatch names itself: {:?}",
        report.plain.checks
    );
}

#[test]
fn array_of_tables_accumulates_into_an_optional_field_too() {
    // Whether `[[x]]` decodes must not depend on whether the field is `[T]` or
    // `Option<[T]>`: the wrapper is about absence, not about how repetition is
    // spelled. Before the accumulation saw through `OptionSome`, the second
    // block hit the duplicate-field arm and a lock-adjacent struct would fail
    // for a reason that had nothing to do with its document.
    run(
        "optional-array-of-tables",
        r#"
struct Package { name: String }
struct Lock { package: Option<[Package]> }

fn count(lock: Lock) -> Int {
    let none: [Package] = [];
    lock.package.unwrap_or(none).len()
}

#[test]
fn t() -> Stream<Check> {
    let lock: Lock = toml_decode("[[package]]\nname = \"a\"\n\n[[package]]\nname = \"b\"\n");
    yield expect_eq(count(lock), 2);
}
"#,
        1,
    );
}

#[test]
fn a_repeated_inline_toml_array_key_accumulates_a_known_lenience() {
    // PINNING A LENIENCE, not endorsing it. `names = [...]` twice is INVALID
    // TOML, and a stricter decoder would reject it. Ours cannot: the parser
    // presents an inline array and an array-of-tables block as the same events,
    // so the rule that makes `[[package]]` work necessarily also accepts this.
    // Telling them apart needs the parser to distinguish the two spellings.
    // This test exists so the day that lands, the change in behaviour is
    // deliberate and visible rather than a silently altered decode.
    run(
        "inline-duplicate-lenience",
        r#"
struct Doc { names: [String] }

#[test]
fn t() -> Stream<Check> {
    let d: Doc = toml_decode("names = [\"a\"]\nnames = [\"b\"]\n");
    yield expect_eq(d.names, ["a", "b"]);
}
"#,
        1,
    );
}
