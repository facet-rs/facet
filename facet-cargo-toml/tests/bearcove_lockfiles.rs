//! Data-driven tests: parse every Cargo.lock from ~/bearcove/

use facet_cargo_toml::CargoLock;
use std::path::Path;

fn parse_lockfile(path: &Path) -> datatest_stable::Result<()> {
    let lockfile = match CargoLock::from_path(camino::Utf8Path::from_path(path).unwrap()) {
        Ok(lockfile) => lockfile,
        Err(e) => {
            eprintln!("{e}");
            panic!("parsing failed");
        }
    };

    println!(
        "  version: {}, {} packages",
        lockfile.version,
        lockfile.packages.len()
    );

    Ok(())
}

datatest_stable::harness!(
    parse_lockfile,
    "tests/fixtures-lockfile",
    r"^.*Cargo\.lock$"
);
