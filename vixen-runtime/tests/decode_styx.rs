//! Typed decode of STYX documents — the format the package manifest speaks.
//!
//! Three seams under test, each with a differential that pins identity, not
//! just shape:
//!
//! - `Format::Styx` through both lowering lanes: a literal `decode(…)`
//!   constant-folds in `compiler::lower_decoded_value`, while the
//!   `styx_decode` wrapper always crosses the registered primitive
//!   (`runtime::decode_value::primitive_value_from_decoded`). `expect_eq`
//!   between the two is an identity claim — the folded value and the
//!   runtime-decoded value must intern to the same handle.
//! - Variant TAGS (`@exit-only`, `@argv-flag{…}`) selecting enum variants by
//!   the kebab rule — including unit variants, which the string/table shape
//!   forms cannot reach at all.
//! - `Map<String, _>` decode, where the differential against a `%{}`-built
//!   map is what proves the canonical row order: the document spells rows in
//!   one order, the map adds them in another, and both must be THE map value.

use vixen_runtime::ratchet::run_source;

fn run(label: &str, source: &str, checks: usize) {
    let report = run_source(source).unwrap_or_else(|error| panic!("{label}: {error:#?}"));
    assert!(
        report.passed(),
        "{label} checks pass: {:?}",
        report.plain.checks
    );
    assert!(report.agrees(), "{label} agrees across lanes");
    assert_eq!(report.plain.checks.len(), checks, "{label} check count");
}

#[test]
fn styx_documents_decode_and_both_lanes_agree() {
    run(
        "struct",
        r#"
struct PackageMeta { name: String, version: String }

#[test]
fn t() -> Stream<Check> {
    let doc = "name facet\nversion \"0.1.0\"";
    let runtime: PackageMeta = styx_decode(doc);
    yield expect_eq(runtime.name, "facet");
    yield expect_eq(runtime.version, "0.1.0");
    let folded: PackageMeta = decode("name facet\nversion \"0.1.0\"", Format::Styx);
    yield expect_eq(folded, runtime);
}
"#,
        3,
    );
}

#[test]
fn styx_variant_tags_reach_unit_variants() {
    run(
        "unit tags",
        r#"
enum Protocol { ExitOnly, ProgressiveLinesV1 }
struct Program { protocol: Protocol }

#[test]
fn t() -> Stream<Check> {
    let exit: Program = styx_decode("protocol @exit-only");
    yield expect_eq(exit.protocol, Protocol::ExitOnly);
    let lines: Program = styx_decode("protocol @progressive-lines-v1");
    yield expect_eq(lines.protocol, Protocol::ProgressiveLinesV1);
    let folded: Program = decode("protocol @exit-only", Format::Styx);
    yield expect_eq(folded, exit);
}
"#,
        3,
    );
}

#[test]
fn styx_variant_tags_carry_record_payloads() {
    run(
        "record payload tags",
        r#"
enum Target {
    Neutral,
    ArgvFlag { flag: String },
}
struct Program { target: Target }

#[test]
fn t() -> Stream<Check> {
    let flagged: Program = styx_decode("target @argv-flag{flag \"--target\"}");
    yield match flagged.target {
        Target::ArgvFlag { flag } => expect_eq(flag, "--target"),
        Target::Neutral => expect(false),
    };
    let neutral: Program = styx_decode("target @neutral");
    yield expect_eq(neutral.target, Target::Neutral);
}
"#,
        2,
    );
}

#[test]
fn a_decoded_map_is_the_map_value_a_program_builds() {
    // The document spells rows b-then-a; the program adds a-then-b. If the
    // decoded value is really THE canonical map, the two intern identically —
    // this is the assertion that pins the runtime lane's row ordering and the
    // fold lane's `Op::Map` construction to the machine's own semantics.
    run(
        "map identity",
        r#"
#[test]
fn t() -> Stream<Check> {
    let decoded: Map<String, Int> = styx_decode("b 2\na 1");
    let empty: Map<String, Int> = %{};
    let built = empty + ("a", 1) + ("b", 2);
    yield expect_eq(decoded, built);
    yield expect_eq(decoded.get("a"), 1);
    yield expect_eq(decoded.len(), 2);
    let folded: Map<String, Int> = decode("b 2\na 1", Format::Styx);
    yield expect_eq(folded, built);
}
"#,
        4,
    );
}

#[test]
fn maps_of_records_decode_the_manifest_section_shape() {
    run(
        "manifest sections",
        r#"
struct Command { fn: String }
struct Manifest { package: PackageMeta, command: Map<String, Command> }
struct PackageMeta { name: String, version: String }

#[test]
fn t() -> Stream<Check> {
    let doc = "package {\n  name facet\n  version \"0.1.0\"\n}\ncommand {\n  build { fn facet::build }\n  test { fn facet::test }\n}";
    let manifest: Manifest = styx_decode(doc);
    yield expect_eq(manifest.package.name, "facet");
    yield expect_eq(manifest.command.len(), 2);
    yield expect_eq(manifest.command.get("build").fn, "facet::build");
    yield expect_eq(manifest.command.get("test").fn, "facet::test");
}
"#,
        4,
    );
}

#[test]
fn a_failed_styx_decode_is_a_typed_result() {
    run(
        "try_styx_decode",
        r#"
struct PackageMeta { name: String, version: String }

#[test]
fn t() -> Stream<Check> {
    let missing: Result<PackageMeta, DecodeError> = try_styx_decode("name facet");
    yield match missing {
        Ok(_) => expect(false),
        Err(error) => expect_eq(error.kind, "missing-field"),
    };
    let fine: Result<PackageMeta, DecodeError> = try_styx_decode("name facet\nversion \"0.1.0\"");
    yield match fine {
        Ok(meta) => expect_eq(meta.name, "facet"),
        Err(_) => expect(false),
    };
}
"#,
        2,
    );
}
