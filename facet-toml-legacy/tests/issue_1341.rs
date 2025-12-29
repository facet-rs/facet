//! Test for issue #1341: Option<Vec<T>> fails to parse TOML array of tables
//!
//! This test reproduces the bug where facet-toml fails to deserialize
//! TOML array-of-tables syntax when the target field is typed as Option<Vec<T>>.

use facet::Facet;

#[derive(Facet, Debug, PartialEq)]
struct BinTarget {
    name: String,
    path: String,
}

#[derive(Facet, Debug, PartialEq)]
struct Manifest {
    bin: Option<Vec<BinTarget>>,
}

#[test]
fn test_option_vec_array_of_tables_some() {
    let toml = r#"
[[bin]]
name = "hello"
path = "src/main.rs"

[[bin]]
name = "world"
path = "src/world.rs"
"#;

    let result = facet_toml_legacy::from_str::<Manifest>(toml);
    if let Err(e) = &result {
        eprintln!("Error: {e}");
    }

    assert!(
        result.is_ok(),
        "Should parse Option<Vec<T>> with array-of-tables"
    );

    let manifest = result.unwrap();
    assert!(manifest.bin.is_some());

    let bins = manifest.bin.unwrap();
    assert_eq!(bins.len(), 2);
    assert_eq!(bins[0].name, "hello");
    assert_eq!(bins[0].path, "src/main.rs");
    assert_eq!(bins[1].name, "world");
    assert_eq!(bins[1].path, "src/world.rs");
}

#[test]
fn test_option_vec_array_of_tables_none() {
    let toml = r#""#;

    let result = facet_toml_legacy::from_str::<Manifest>(toml);
    if let Err(e) = &result {
        eprintln!("Error: {e}");
    }

    assert!(
        result.is_ok(),
        "Should parse empty TOML with Option<Vec<T>> as None"
    );

    let manifest = result.unwrap();
    assert!(manifest.bin.is_none());
}

#[test]
fn test_option_vec_array_of_tables_single_entry() {
    let toml = r#"
[[bin]]
name = "single"
path = "src/single.rs"
"#;

    let result = facet_toml_legacy::from_str::<Manifest>(toml);
    if let Err(e) = &result {
        eprintln!("Error: {e}");
    }

    assert!(result.is_ok(), "Should parse single entry array-of-tables");

    let manifest = result.unwrap();
    assert!(manifest.bin.is_some());

    let bins = manifest.bin.unwrap();
    assert_eq!(bins.len(), 1);
    assert_eq!(bins[0].name, "single");
    assert_eq!(bins[0].path, "src/single.rs");
}
