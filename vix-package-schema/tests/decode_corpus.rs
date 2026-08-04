//! The corpus packages and the schema types are one artifact: these tests
//! take the actual `main.vix` files under `vix-core/corpus-next/package/`,
//! slice the manifest out of the frontmatter fence, and decode it against
//! [`vix_package_schema::PackageManifest`] — so a respelling on either side
//! fails here rather than drifting.
//!
//! Extraction is exercised on real files on purpose. It is the whole content
//! of `r[vixen.package.frontmatter-reads-without-vix]`: a package's word must
//! be readable without an evaluator, and a test that pasted the styx inline
//! would prove nothing about that.

use vix_package_schema::{PackageManifest, ProgramSource, Protocol, Target, frontmatter};

const FACET_SOURCE: &str = include_str!("../../vix-core/corpus-next/package/facet/main.vix");
const CARGO_SOURCE: &str = include_str!("../../vix-core/corpus-next/package/cargo/main.vix");

/// Slice and decode, the way any reader without a vix evaluator must.
fn read(source: &str, what: &str) -> PackageManifest {
    let fm = frontmatter::extract(source).unwrap_or_else(|e| panic!("{what}: {e:?}"));
    facet_styx::from_str(fm.styx).unwrap_or_else(|e| panic!("{what} decodes: {e}"))
}

#[test]
fn the_facet_source_package_decodes_from_its_frontmatter() {
    let manifest = read(FACET_SOURCE, "facet/main.vix");

    assert_eq!(manifest.package.name, "facet");
    assert_eq!(manifest.package.version, "0.1.0");

    // Commands and artifacts are bare fn refs.
    assert_eq!(manifest.command["build"].r#fn, "facet::build");
    assert_eq!(manifest.command["test"].r#fn, "facet::test");
    assert_eq!(manifest.artifact["bins"].r#fn, "facet::build");

    // The exposed program is carved out of an artifact's value.
    let fx = &manifest.program["fx"];
    assert_eq!(
        fx.source,
        ProgramSource::Artifact {
            name: "bins".to_owned(),
            path: "bin/fx".to_owned(),
        }
    );
    assert_eq!(fx.protocol, Protocol::ExitOnly);
    assert_eq!(fx.target, Target::Neutral);
}

#[test]
fn the_cargo_tool_package_decodes_from_its_frontmatter() {
    let manifest = read(CARGO_SOURCE, "cargo/main.vix");

    assert_eq!(manifest.package.name, "cargo");
    assert_eq!(manifest.command["build"].r#fn, "cargo::build");
    assert_eq!(manifest.command["test"].r#fn, "cargo::test");
    assert!(manifest.artifact.is_empty());
}

#[test]
fn the_cargo_package_cannot_yet_declare_its_programs() {
    // Not an aspiration test — a pin on a known hole. cargo exposes `rustc`
    // and `cargo` from a fetched toolchain, and a program sourced from
    // outside the package has no spelling since the `@input` arm was struck
    // with `r[vixen.package.inputs-are-provider-pins]`. The surviving arm
    // names one of the package's OWN artifacts, which a fetched toolchain is
    // not.
    //
    // When the acquirable-toolchain spelling lands, this assertion is the one
    // that fails, and the corpus file is the one to fix.
    let manifest = read(CARGO_SOURCE, "cargo/main.vix");
    assert!(
        manifest.program.is_empty(),
        "cargo's programs became expressible — see the Retracted section of \
         /spec/vixen/packages and update the corpus"
    );
}

#[test]
fn the_extracted_offset_points_into_the_real_file() {
    // The reason extraction carries an offset: a styx diagnostic inside the
    // fence must report at its true position in main.vix, never at line 1 of
    // a document that does not exist on disk.
    let fm = frontmatter::extract(FACET_SOURCE).expect("facet has frontmatter");
    assert_eq!(&FACET_SOURCE[fm.offset..fm.offset + fm.styx.len()], fm.styx);
    assert!(
        FACET_SOURCE[..fm.offset].contains("manifest"),
        "the offset must fall after the opening fence"
    );
}

#[test]
fn a_schema_declaration_line_is_tolerated() {
    // `@schema <ref>` at the document root (schema[schema.declaration]) is
    // editor/validator metadata; decode skips it. Pinned so a manifest can
    // grow the line the day the published schema exists without any reader
    // changing.
    let manifest: PackageManifest = facet_styx::from_str(
        "@schema https://example.invalid/schemas/vix-package.styx\n\
         package {\n  name tiny\n  version \"0.0.1\"\n}\n",
    )
    .expect("a leading @schema line decodes away");
    assert_eq!(manifest.package.name, "tiny");
}

#[test]
fn a_minimal_manifest_needs_only_the_package_header() {
    let manifest: PackageManifest =
        facet_styx::from_str("package {\n  name tiny\n  version \"0.0.1\"\n}\n")
            .expect("sections default to empty");
    assert_eq!(manifest.package.name, "tiny");
    assert!(manifest.command.is_empty());
    assert!(manifest.artifact.is_empty());
    assert!(manifest.program.is_empty());
}
