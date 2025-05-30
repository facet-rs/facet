use facet::Facet;
use std::collections::HashMap;

#[derive(Debug, PartialEq, Clone, Facet)]
struct Wide {
    field01: String,
    field02: u64,
    field03: i32,
    field04: f64,
    field05: bool,
    field06: Option<String>,
    field07: Vec<u32>,
    field08: String,
    field09: u64,
    field10: i32,
    field11: f64,
    field12: bool,
    field13: Option<String>,
    field14: Vec<u32>,
    field15: String,
    field16: u64,
    field17: i32,
    field18: f64,
    field19: bool,
    field20: Option<String>,
    field21: Vec<u32>,
    field22: String,
    field23: u64,
    field24: i32,
    field25: f64,
    field26: bool,
    field27: Option<String>,
    field28: Vec<u32>,
    field29: HashMap<String, i32>,
    field30: Nested0,
}

#[derive(Debug, PartialEq, Clone, Facet)]
struct Nested0 {
    id: u64,
    name: String,
}

#[test]
fn test_wide_struct_deserialization() {
    // Start with a simpler test
    let toml_str = r#"
field01 = "value 01"
field02 = 12345678901234567
field03 = -123456789
field04 = 3.141592653589793
field05 = true
field06 = "optional value 06"
field07 = [1, 2, 3, 4, 5]
"#;

    let result: Result<Wide, _> = facet_toml::from_str(toml_str);
    if let Err(e) = &result {
        eprintln!("Error: {}", e);
    }
    assert!(result.is_ok());
}
