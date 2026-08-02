//! The corpus manifests and the schema types are one artifact: these tests
//! decode the actual files under `vix-core/corpus-next/package/` against
//! [`vix_package_schema::PackageManifest`], so a respelling on either side
//! fails here rather than drifting.

use vix_package_schema::{
    Input, PackageManifest, ProgramSource, Protocol, Target, Unpack,
};

const FACET_MANIFEST: &str =
    include_str!("../../vix-core/corpus-next/package/facet/package.styx");
const CARGO_MANIFEST: &str =
    include_str!("../../vix-core/corpus-next/package/cargo/package.styx");

#[test]
fn the_facet_source_package_manifest_decodes() {
    let manifest: PackageManifest =
        facet_styx::from_str(FACET_MANIFEST).expect("facet/package.styx decodes");

    assert_eq!(manifest.package.name, "facet");
    assert_eq!(manifest.package.version, "0.1.0");

    // One @package input, pinned by the provider's tree identity.
    assert_eq!(manifest.input.len(), 1);
    let Input::Package { version, hash } = &manifest.input["cargo"] else {
        panic!("the cargo input is the @package arm");
    };
    assert_eq!(version, "0.1.0");
    assert!(hash.starts_with("blake3:"));

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
fn the_cargo_tool_package_manifest_decodes() {
    let manifest: PackageManifest =
        facet_styx::from_str(CARGO_MANIFEST).expect("cargo/package.styx decodes");

    assert_eq!(manifest.package.name, "cargo");

    // The @fetch input arm: origin coordinate + pins.md digest + unpack.
    let Input::Fetch {
        origin,
        hash,
        unpack,
    } = &manifest.input["rust-dist"]
    else {
        panic!("rust-dist is the @fetch arm");
    };
    assert!(origin.starts_with("registry://"));
    assert!(hash.starts_with("sha256:"));
    assert_eq!(*unpack, Unpack::TarXz);

    // Programs carved out of the fetched tree, with their contracts.
    let rustc = &manifest.program["rustc"];
    assert_eq!(
        rustc.source,
        ProgramSource::Input {
            name: "rust-dist".to_owned(),
            path: "rustc/bin/rustc".to_owned(),
        }
    );
    assert_eq!(rustc.protocol, Protocol::ExitOnly);
    assert_eq!(
        rustc.target,
        Target::ArgvFlag {
            flag: "--target".to_owned(),
        }
    );
    assert!(manifest.program.contains_key("cargo"));

    assert_eq!(manifest.command["build"].r#fn, "cargo::build");

    // The cargo package exposes no artifacts; the absent section defaults.
    assert!(manifest.artifact.is_empty());
}

#[test]
fn a_minimal_manifest_needs_only_the_package_header() {
    let manifest: PackageManifest = facet_styx::from_str(
        "package {\n  name tiny\n  version \"0.0.1\"\n}\n",
    )
    .expect("sections default to empty");
    assert_eq!(manifest.package.name, "tiny");
    assert!(manifest.input.is_empty());
    assert!(manifest.command.is_empty());
    assert!(manifest.artifact.is_empty());
    assert!(manifest.program.is_empty());
}
