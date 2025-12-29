use facet::Facet;

#[derive(Facet, Debug)]
#[repr(u8)]
#[facet(untagged)]
enum DebugLevel {
    Bool(bool),
    Number(u8),
    String(String),
}

#[derive(Facet, Debug)]
struct Profile {
    debug: Option<DebugLevel>,
}

#[derive(Facet, Debug)]
struct Manifest {
    profile: Option<std::collections::HashMap<String, Profile>>,
}

fn main() {
    // Test 1: i64 value 0 should coerce to u8
    let toml1 = r#"
[profile.dev]
debug = 0
"#;

    match facet_toml::from_str::<Manifest>(toml1) {
        Ok(_) => println!("✓ Test 1: i64 0 → u8 - PASSED"),
        Err(e) => println!("✗ Test 1: i64 0 → u8 - FAILED: {}", e),
    }

    // Test 2: i64 value 2 should coerce to u8
    let toml2 = r#"
[profile.dev]
debug = 2
"#;

    match facet_toml::from_str::<Manifest>(toml2) {
        Ok(_) => println!("✓ Test 2: i64 2 → u8 - PASSED"),
        Err(e) => println!("✗ Test 2: i64 2 → u8 - FAILED: {}", e),
    }

    // Test 3: i64 value that fits in i32
    #[derive(Facet, Debug)]
    struct LintConfig {
        priority: Option<i32>,
    }

    let toml3 = r#"
priority = -1
"#;

    match facet_toml::from_str::<LintConfig>(toml3) {
        Ok(_) => println!("✓ Test 3: i64 -1 → i32 - PASSED"),
        Err(e) => println!("✗ Test 3: i64 -1 → i32 - FAILED: {}", e),
    }
}
