//! Data-driven tests: parse every Cargo.toml from ~/bearcove/

use facet_cargo_toml::CargoToml;
use std::path::Path;

fn parse_manifest(path: &Path) -> datatest_stable::Result<()> {
    let manifest = match CargoToml::from_path(camino::Utf8Path::from_path(path).unwrap()) {
        Ok(manifest) => manifest,
        Err(e) => {
            eprintln!("{e}");
            panic!("parsing failed");
        }
    };

    if let Some(package) = &manifest.package {
        if let Some(name) = &package.name {
            println!("  package: {}", name.value);
        }
    } else if manifest.workspace.is_some() {
        println!("  workspace manifest");
    }

    Ok(())
}

datatest_stable::harness! {
    { test = parse_manifest, root = "tests/fixtures", pattern = r"^.*\.toml$" },
}
