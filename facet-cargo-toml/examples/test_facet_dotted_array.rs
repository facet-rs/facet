use facet::Facet;

#[derive(Facet, Debug)]
struct Workspace {
    members: Vec<String>,
    metadata: Option<facet_value::Value>,
}

#[derive(Facet, Debug)]
struct Manifest {
    workspace: Option<Workspace>,
}

fn main() {
    // This works - dotted key with string value
    let toml1 = r#"
[workspace]
members = []

[workspace.metadata.typos]
default.simple = "value"
"#;

    match facet_toml::from_str::<Manifest>(toml1) {
        Ok(_) => println!("✓ Test 1: dotted key with string - PASSED"),
        Err(e) => println!("✗ Test 1: dotted key with string - FAILED: {}", e),
    }

    // This fails - dotted key with array value
    let toml2 = r#"
[workspace]
members = []

[workspace.metadata.typos]
default.extend-ignore-re = ["clonable"]
"#;

    match facet_toml::from_str::<Manifest>(toml2) {
        Ok(_) => println!("✓ Test 2: dotted key with array - PASSED"),
        Err(e) => println!("✗ Test 2: dotted key with array - FAILED: {}", e),
    }
}
