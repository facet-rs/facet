// Issue #1350: facet-toml fails to parse nested table headers like [target.'cfg(windows)'.dependencies]

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
fn test_nested_table_header_with_quoted_key() {
    let toml = r#"
[target.'cfg(windows)'.dependencies]
windows-targets = "0.52.6"
"#;

    let result: Result<TestManifest, _> = facet_toml_legacy::from_str(toml);

    match &result {
        Ok(manifest) => {
            let target = manifest.target.as_ref().expect("target should exist");
            let cfg_windows = target
                .get("cfg(windows)")
                .expect("cfg(windows) key should exist");
            let deps = cfg_windows
                .dependencies
                .as_ref()
                .expect("dependencies should exist");
            assert_eq!(deps.get("windows-targets"), Some(&"0.52.6".to_string()));
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            panic!("Failed to parse TOML: {}", e);
        }
    }
}

#[test]
fn test_nested_table_header_simple() {
    let toml = r#"
[target.x86_64.dependencies]
some-dep = "1.0"
"#;

    let result: Result<TestManifest, _> = facet_toml_legacy::from_str(toml);

    match &result {
        Ok(manifest) => {
            let target = manifest.target.as_ref().expect("target should exist");
            let x86_64 = target.get("x86_64").expect("x86_64 key should exist");
            let deps = x86_64
                .dependencies
                .as_ref()
                .expect("dependencies should exist");
            assert_eq!(deps.get("some-dep"), Some(&"1.0".to_string()));
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            panic!("Failed to parse TOML: {}", e);
        }
    }
}
