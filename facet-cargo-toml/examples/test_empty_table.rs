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
struct Config {
    profile: HashMap<String, Profile>,
}

fn main() {
    let toml = r#"
[profile.release.package]
# zed = { codegen-units = 16 }

[profile.release-fast]
debug = 1
"#;

    match facet_format_toml::from_str::<Config>(toml) {
        Ok(config) => {
            println!("✓ Parsed successfully!");
            println!("  release.package: {:?}", config.profile.get("release").and_then(|p| p.package.as_ref()));
        }
        Err(e) => {
            eprintln!("✗ Parse error: {}", e);
            std::process::exit(1);
        }
    }
}
