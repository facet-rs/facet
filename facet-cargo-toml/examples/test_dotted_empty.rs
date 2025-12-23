use facet::Facet;
use std::collections::HashMap;

#[derive(Facet, Debug)]
struct PackageProfile {
    opt_level: Option<i64>,
}

#[derive(Facet, Debug)]
struct Profile {
    package: Option<HashMap<String, PackageProfile>>,
}

#[derive(Facet, Debug)]
struct Manifest {
    profile: Option<HashMap<String, Profile>>,
}

fn main() {
    let toml = r#"
[profile.release.package]
# comment only
"#;

    match facet_format_toml::from_str::<Manifest>(toml) {
        Ok(_) => println!("✓ Parsed"),
        Err(e) => {
            eprintln!("✗ Error: {}", e);
            std::process::exit(1);
        }
    }
}
