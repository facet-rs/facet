use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Serialize)]
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

#[derive(Debug, Serialize)]
struct Nested0 {
    id: u64,
    name: String,
}

fn main() {
    let mut map = HashMap::new();
    map.insert("a".to_string(), 1);
    map.insert("b".to_string(), 2);

    let data = Wide {
        field01: "value 01".to_string(),
        field02: 12345678901234567,
        field03: -123456789,
        field04: 3.141592653589793,
        field05: true,
        field06: Some("optional value 06".to_string()),
        field07: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 0],
        field08: "value 08".to_string(),
        field09: 98765432109876543,
        field10: 987654321,
        field11: 2.718281828459045,
        field12: false,
        field13: None,
        field14: vec![0, 9, 8, 7, 6, 5, 4, 3, 2, 1],
        field15: "value 15".to_string(),
        field16: 1111111111111111111,
        field17: -111111111,
        field18: 1.618033988749895,
        field19: true,
        field20: Some("optional value 20".to_string()),
        field21: vec![10, 20, 30],
        field22: "value 22".to_string(),
        field23: 2222222222222222222,
        field24: -222222222,
        field25: 0.5772156649015329,
        field26: false,
        field27: None,
        field28: vec![],
        field29: map,
        field30: Nested0 {
            id: 0,
            name: "Base Nested".to_string(),
        },
    };

    let toml_string = toml::to_string(&data).expect("Failed to create TOML");
    println!("{}", &toml_string[..200]); // First 200 chars
}
