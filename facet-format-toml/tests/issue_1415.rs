// Test for issue #1415: facet-format-toml fails to parse dotted keys with array values
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

#[test]
fn test_dotted_key_with_string_value() {
    // ✓ This should work - dotted key with string value
    let toml = r#"
[workspace]
members = []

[workspace.metadata.typos]
default.simple = "value"
"#;

    let result = facet_format_toml::from_str::<Manifest>(toml);
    match &result {
        Ok(manifest) => {
            assert!(manifest.workspace.is_some());
            let workspace = manifest.workspace.as_ref().unwrap();
            assert!(workspace.metadata.is_some());
        }
        Err(e) => {
            panic!("Failed to parse dotted key with string value: {:?}", e);
        }
    }
}

#[test]
fn test_dotted_key_with_array_value() {
    // This currently fails - dotted key with array value
    let toml = r#"
[workspace]
members = []

[workspace.metadata.typos]
default.extend-ignore-re = ["clonable"]
"#;

    let result = facet_format_toml::from_str::<Manifest>(toml);
    match &result {
        Ok(manifest) => {
            assert!(manifest.workspace.is_some());
            let workspace = manifest.workspace.as_ref().unwrap();
            assert!(workspace.metadata.is_some());

            // Verify the structure is correct
            let metadata = workspace.metadata.as_ref().unwrap();
            assert!(metadata.is_object());
            let obj = metadata.as_object().unwrap();
            assert!(obj.contains_key("typos"));
        }
        Err(e) => {
            panic!("Failed to parse dotted key with array value: {:?}", e);
        }
    }
}

#[test]
fn test_dotted_key_with_various_types() {
    // Test multiple value types with dotted keys
    let toml = r#"
[workspace]
members = []

[workspace.metadata.test]
a.string = "value"
b.integer = 42
c.boolean = true
d.array = [1, 2, 3]
e.inline-table = { key = "value" }
"#;

    let result = facet_format_toml::from_str::<Manifest>(toml);
    match &result {
        Ok(manifest) => {
            assert!(manifest.workspace.is_some());
            let workspace = manifest.workspace.as_ref().unwrap();
            assert!(workspace.metadata.is_some());
        }
        Err(e) => {
            panic!("Failed to parse dotted keys with various types: {:?}", e);
        }
    }
}
