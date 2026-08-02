//! The vix-side package-manifest reader (`cargo-vix/package.vix`) over the
//! REAL corpus manifests (`vix-core/corpus-next/package/*/package.styx`) —
//! the same bytes `vix-package-schema`'s Rust tests decode. Three artifacts,
//! one seam: if the manifest respells, the Rust schema drifts, or the vix
//! mirror diverges, exactly one of these suites breaks and names the side.
//!
//! The manifest text reaches the program as an ordinary string literal (a
//! runner would read it through an origin adapter; the decode path from text
//! onward is identical), and the handler module is imported the way any vix
//! module is — nothing here is test-only machinery. The root interrogates
//! the unions through the handler's accessors because a variant path does
//! not cross a module boundary in today's vix (`package::Input::Fetch`
//! resolves nowhere outside the module) — the accessor surface is the
//! handler's API, not a test convenience.

use vix::modules::ModuleSource;
use vixen_runtime::ratchet::run_source_with_modules;

const PACKAGE_VIX: &str = include_str!("../../cargo-vix/package.vix");
const FACET_MANIFEST: &str =
    include_str!("../../vix-core/corpus-next/package/facet/package.styx");
const CARGO_MANIFEST: &str =
    include_str!("../../vix-core/corpus-next/package/cargo/package.styx");

/// Spell `text` as a vix string literal (the escapes `unquote` reverses).
fn vix_string_literal(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

fn run(label: &str, source: &str, checks: usize) {
    let modules = [ModuleSource {
        name: "package",
        source: PACKAGE_VIX,
    }];
    let report = run_source_with_modules(source, &modules)
        .unwrap_or_else(|error| panic!("{label}: {error:#?}"));
    assert!(
        report.passed(),
        "{label} checks pass: {:?}",
        report.plain.checks
    );
    assert!(report.agrees(), "{label} agrees across lanes");
    assert_eq!(report.plain.checks.len(), checks, "{label} check count");
}

#[test]
fn the_facet_source_package_manifest_reads_in_vix() {
    let doc = vix_string_literal(FACET_MANIFEST);
    let source = format!(
        r#"
import package::{{manifest, inputs, artifacts, programs, command_fn, dangling_program_sources, input_label, input_version, input_pin, source_label, source_name, source_path, protocol_label, target_label}};

#[test]
fn reads_facet() -> Stream<Check> {{
    let m = manifest({doc});
    yield expect_eq(m.package.name, "facet");
    yield expect_eq(m.package.version, "0.1.0");

    // The @package input arm, pinned by provider tree identity.
    let cargo_input = inputs(m).get("cargo");
    yield expect_eq(input_label(cargo_input), "package");
    yield expect_eq(input_version(cargo_input), Some("0.1.0"));
    yield expect(input_pin(cargo_input).starts_with("blake3:"));

    // Commands and artifacts are bare fn refs keyed by public name.
    yield expect_eq(command_fn(m) where {{ name: "build" }}, "facet::build");
    yield expect_eq(command_fn(m) where {{ name: "test" }}, "facet::test");
    yield expect_eq(artifacts(m).get("bins").fn, "facet::build");

    // The exposed program, carved out of an artifact's value.
    let fx = programs(m).get("fx");
    yield expect_eq(protocol_label(fx.protocol), "exit-only");
    yield expect_eq(target_label(fx.target), "neutral");
    yield expect_eq(source_label(fx.source), "artifact");
    yield expect_eq(source_name(fx.source), "bins");
    yield expect_eq(source_path(fx.source), "bin/fx");

    // Manifest coherence: every program source names a declared thing.
    let none: [String] = [];
    yield expect_eq(dangling_program_sources(m), none);
}}
"#
    );
    run("facet manifest", &source, 14);
}

#[test]
fn the_cargo_tool_package_manifest_reads_in_vix() {
    let doc = vix_string_literal(CARGO_MANIFEST);
    let source = format!(
        r#"
import package::{{manifest, inputs, programs, command_fn, dangling_program_sources, input_label, input_pin, fetch_origin, fetch_unpack, source_label, source_name, source_path, protocol_label, target_label, target_flag}};

#[test]
fn reads_cargo() -> Stream<Check> {{
    let m = manifest({doc});
    yield expect_eq(m.package.name, "cargo");

    // The @fetch input arm: origin coordinate, pins.md digest, unpack step.
    let dist = inputs(m).get("rust-dist");
    yield expect_eq(input_label(dist), "fetch");
    yield expect_eq(fetch_origin(dist), Some("registry://rust-1.89.0-x86_64-unknown-linux-gnu.tar.xz"));
    yield expect(input_pin(dist).starts_with("sha256:"));
    yield expect_eq(fetch_unpack(dist), Some("tar-xz"));

    // Programs carved out of the fetched tree, with their contracts.
    let rustc = programs(m).get("rustc");
    yield expect_eq(protocol_label(rustc.protocol), "exit-only");
    yield expect_eq(target_label(rustc.target), "argv-flag");
    yield expect_eq(target_flag(rustc.target), Some("--target"));
    yield expect_eq(source_label(rustc.source), "input");
    yield expect_eq(source_name(rustc.source), "rust-dist");
    yield expect_eq(source_path(rustc.source), "rustc/bin/rustc");
    yield expect(programs(m).has("cargo"));

    yield expect_eq(command_fn(m) where {{ name: "build" }}, "cargo::build");

    // No artifact section: the accessor answers with the empty map, and
    // coherence still holds (both programs source from the declared input).
    let none: [String] = [];
    yield expect_eq(dangling_program_sources(m), none);
}}
"#
    );
    run("cargo manifest", &source, 14);
}
