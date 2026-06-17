use facet_cargo_toml::CargoToml;

fn main() {
    let toml = std::fs::read_to_string("/tmp/profiles.toml").unwrap();

    match CargoToml::parse(&toml) {
        Ok(_) => println!("✓ Parsed profiles successfully"),
        Err(e) => {
            eprintln!("✗ Parse error: {}", e);
            std::process::exit(1);
        }
    }
}
