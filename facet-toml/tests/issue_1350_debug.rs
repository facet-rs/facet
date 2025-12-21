// Debug version with more detailed output

use facet::Facet;
use std::collections::HashMap;

#[derive(Facet, Debug)]
struct TestManifest {
    target: Option<HashMap<String, TargetSpec>>,
}

#[derive(Facet, Debug, Default)]
#[facet(rename_all = "kebab-case")]
struct TargetSpec {
    dependencies: Option<HashMap<String, String>>,
}

#[test]
fn test_two_level_nested_table() {
    let toml = r#"
[target.x86_64]
"#;

    let result: Result<TestManifest, _> = facet_toml::from_str(toml);

    match &result {
        Ok(manifest) => {
            eprintln!("Success: {:#?}", manifest);
            let target = manifest.target.as_ref().expect("target should exist");
            assert!(target.contains_key("x86_64"));
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            panic!("Failed to parse TOML: {}", e);
        }
    }
}

#[test]
fn test_three_level_no_values() {
    let toml = r#"
[target.x86_64.dependencies]
"#;

    let result: Result<TestManifest, _> = facet_toml::from_str(toml);

    match &result {
        Ok(manifest) => {
            eprintln!("Success: {:#?}", manifest);
            let target = manifest.target.as_ref().expect("target should exist");
            let x86_64 = target.get("x86_64").expect("x86_64 key should exist");
            assert!(x86_64.dependencies.is_some());
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            panic!("Failed to parse TOML: {}", e);
        }
    }
}
