//! Debug script for issue 2010

use facet::Facet;
use std::collections::HashMap;

#[derive(Clone, Debug, Facet, PartialEq)]
#[facet(tag = "kind")]
#[repr(C)]
pub enum Inner {
    TypeA { value: f64 },
    TypeB { alpha: f64, beta: f64 },
}

#[derive(Clone, Debug, Facet, PartialEq)]
pub struct Item {
    #[facet(flatten)]
    pub inner: Inner,
    pub extra: Option<String>,
}

#[derive(Clone, Debug, Facet, PartialEq)]
pub struct Container {
    pub items: Option<HashMap<String, Item>>,
}

#[derive(Clone, Debug, Facet, PartialEq)]
pub struct Outer {
    pub container: Container,
}

fn main() {
    // Simpler test - just Item directly
    println!("=== Test 1: Item directly, tag first ===");
    let json1 = r#"{"kind": "TypeB", "alpha": 1.0, "beta": 2.0, "extra": "test"}"#;
    match facet_json::from_str::<Item>(json1) {
        Ok(item) => println!("OK: {:?}", item),
        Err(e) => println!("ERR: {:?}", e),
    }

    println!("\n=== Test 2: Item directly, tag in middle ===");
    let json2 = r#"{"alpha": 1.0, "kind": "TypeB", "beta": 2.0, "extra": "test"}"#;
    match facet_json::from_str::<Item>(json2) {
        Ok(item) => println!("OK: {:?}", item),
        Err(e) => println!("ERR: {:?}", e),
    }

    println!("\n=== Test 3: Item directly, tag last ===");
    let json3 = r#"{"alpha": 1.0, "beta": 2.0, "extra": "test", "kind": "TypeB"}"#;
    match facet_json::from_str::<Item>(json3) {
        Ok(item) => println!("OK: {:?}", item),
        Err(e) => println!("ERR: {:?}", e),
    }

    println!("\n=== Test 4: HashMap<String, Item>, tag first ===");
    let json4 = r#"{"x": {"kind": "TypeB", "alpha": 1.0, "beta": 2.0, "extra": "test"}}"#;
    match facet_json::from_str::<HashMap<String, Item>>(json4) {
        Ok(map) => println!("OK: {:?}", map),
        Err(e) => println!("ERR: {:?}", e),
    }

    println!("\n=== Test 5: HashMap<String, Item>, tag in middle ===");
    let json5 = r#"{"x": {"alpha": 1.0, "kind": "TypeB", "beta": 2.0, "extra": "test"}}"#;
    match facet_json::from_str::<HashMap<String, Item>>(json5) {
        Ok(map) => println!("OK: {:?}", map),
        Err(e) => println!("ERR: {:?}", e),
    }

    println!("\n=== Test 6: Full Outer struct, tag first ===");
    let json6 = r#"{
        "container": {
            "items": {
                "x": {
                    "kind": "TypeB",
                    "alpha": 1.0,
                    "beta": 2.0,
                    "extra": "test"
                }
            }
        }
    }"#;
    match facet_json::from_str::<Outer>(json6) {
        Ok(outer) => println!("OK: {:?}", outer),
        Err(e) => println!("ERR: {:?}", e),
    }

    println!("\n=== Test 7: Full Outer struct, tag in middle ===");
    let json7 = r#"{
        "container": {
            "items": {
                "x": {
                    "alpha": 1.0,
                    "extra": "test",
                    "kind": "TypeB",
                    "beta": 2.0
                }
            }
        }
    }"#;
    match facet_json::from_str::<Outer>(json7) {
        Ok(outer) => println!("OK: {:?}", outer),
        Err(e) => println!("ERR: {:?}", e),
    }

    // Narrow down - is it Option<HashMap> or just HashMap in Container?
    println!("\n=== Test 8: Container directly, tag first ===");
    let json8 =
        r#"{"items": {"x": {"kind": "TypeB", "alpha": 1.0, "beta": 2.0, "extra": "test"}}}"#;
    match facet_json::from_str::<Container>(json8) {
        Ok(c) => println!("OK: {:?}", c),
        Err(e) => println!("ERR: {:?}", e),
    }

    println!("\n=== Test 9: Container directly, tag in middle ===");
    let json9 =
        r#"{"items": {"x": {"alpha": 1.0, "kind": "TypeB", "beta": 2.0, "extra": "test"}}}"#;
    match facet_json::from_str::<Container>(json9) {
        Ok(c) => println!("OK: {:?}", c),
        Err(e) => println!("ERR: {:?}", e),
    }

    // Without Option
    #[derive(Clone, Debug, Facet, PartialEq)]
    pub struct ContainerNoOption {
        pub items: HashMap<String, Item>,
    }

    println!("\n=== Test 10: ContainerNoOption, tag first ===");
    let json10 =
        r#"{"items": {"x": {"kind": "TypeB", "alpha": 1.0, "beta": 2.0, "extra": "test"}}}"#;
    match facet_json::from_str::<ContainerNoOption>(json10) {
        Ok(c) => println!("OK: {:?}", c),
        Err(e) => println!("ERR: {:?}", e),
    }

    println!("\n=== Test 11: ContainerNoOption, tag in middle ===");
    let json11 =
        r#"{"items": {"x": {"alpha": 1.0, "kind": "TypeB", "beta": 2.0, "extra": "test"}}}"#;
    match facet_json::from_str::<ContainerNoOption>(json11) {
        Ok(c) => println!("OK: {:?}", c),
        Err(e) => println!("ERR: {:?}", e),
    }

    // Outer without Option
    #[derive(Clone, Debug, Facet, PartialEq)]
    pub struct OuterNoOption {
        pub container: ContainerNoOption,
    }

    println!("\n=== Test 12: OuterNoOption, tag first ===");
    let json12 = r#"{"container": {"items": {"x": {"kind": "TypeB", "alpha": 1.0, "beta": 2.0, "extra": "test"}}}}"#;
    match facet_json::from_str::<OuterNoOption>(json12) {
        Ok(o) => println!("OK: {:?}", o),
        Err(e) => println!("ERR: {:?}", e),
    }

    println!("\n=== Test 13: OuterNoOption, tag in middle ===");
    let json13 = r#"{"container": {"items": {"x": {"alpha": 1.0, "kind": "TypeB", "beta": 2.0, "extra": "test"}}}}"#;
    match facet_json::from_str::<OuterNoOption>(json13) {
        Ok(o) => println!("OK: {:?}", o),
        Err(e) => println!("ERR: {:?}", e),
    }

    // Maybe it's specific to Outer wrapping Container with Option<HashMap>
    // Let's try Outer2 wrapping Container (which has Option<HashMap>)
    #[derive(Clone, Debug, Facet, PartialEq)]
    pub struct Outer2 {
        pub c: Container,
    }

    println!("\n=== Test 14: Outer2 with Container (Option<HashMap>), tag first ===");
    let json14 =
        r#"{"c": {"items": {"x": {"kind": "TypeB", "alpha": 1.0, "beta": 2.0, "extra": "test"}}}}"#;
    match facet_json::from_str::<Outer2>(json14) {
        Ok(o) => println!("OK: {:?}", o),
        Err(e) => println!("ERR: {:?}", e),
    }

    println!("\n=== Test 15: Outer2 with Container (Option<HashMap>), tag in middle ===");
    let json15 =
        r#"{"c": {"items": {"x": {"alpha": 1.0, "kind": "TypeB", "beta": 2.0, "extra": "test"}}}}"#;
    match facet_json::from_str::<Outer2>(json15) {
        Ok(o) => println!("OK: {:?}", o),
        Err(e) => println!("ERR: {:?}", e),
    }
}
