//! Data-driven tests: parse every Cargo.toml from ~/bearcove/
//!
//! Uses datatest-stable to automatically generate one test per file.
//! Each Cargo.toml becomes a separate test case with proper reporting.

use std::path::Path;
use facet_cargo_toml::CargoManifest;

/// Test function that parses a single Cargo.toml file
fn parse_manifest(path: &Path) -> datatest_stable::Result<()> {
    let manifest = match CargoManifest::from_path(path) {
        Ok(manifest) => manifest,
        Err(e) => {
            eprintln!("{e}");
            return Err(e);
        },
    };

    // Print some basic info about what we parsed
    if let Some(package) = &manifest.package {
        if let Some(name) = &package.name {
            println!("  ✓ package: {}", name);
        }
    } else if manifest.workspace.is_some() {
        println!("  ✓ workspace manifest");
    }

    // Success - we parsed it!
    Ok(())
}

// Generate tests for all .toml files in the fixtures directory
// Path is relative to the crate root
datatest_stable::harness!(
    parse_manifest,
    "tests/fixtures",
    r"^.*\.toml$"
);
