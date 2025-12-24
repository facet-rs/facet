//! Test parsing a Cargo.toml file
use facet_cargo_toml::CargoToml;
use std::env;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <path-to-Cargo.toml>", args[0]);
        std::process::exit(1);
    }

    let path = &args[1];
    let _manifest = CargoToml::from_path(camino::Utf8Path::new(path))?;
    Ok(())
}
