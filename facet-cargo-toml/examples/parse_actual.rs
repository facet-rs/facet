use facet_cargo_toml::CargoToml;

fn main() {
    let path = std::env::args().nth(1).expect("Usage: parse_actual <path>");

    match CargoToml::from_path(camino::Utf8Path::new(&path)) {
        Ok(manifest) => {
            if let Some(pkg) = &manifest.package {
                println!("✓ Parsed package: {:?}", pkg.name);
            } else if manifest.workspace.is_some() {
                println!("✓ Parsed workspace manifest");
            }
        }
        Err(e) => {
            eprintln!("✗ Parse error: {}", e);
            std::process::exit(1);
        }
    }
}
