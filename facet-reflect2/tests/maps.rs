use facet::Facet;
use facet_reflect2::{Op, Partial};
use std::collections::HashMap;

// =============================================================================
// Basic HashMap tests
// =============================================================================

#[test]
fn build_empty_map() {
    let mut partial = Partial::alloc::<HashMap<String, u32>>().unwrap();

    // For root-level maps, no End needed - Build initializes the map
    partial.apply(&[Op::set().build()]).unwrap();

    let result: HashMap<String, u32> = partial.build().unwrap();
    assert!(result.is_empty());
}

#[test]
fn build_map_with_imm_values() {
    let mut partial = Partial::alloc::<HashMap<String, u32>>().unwrap();

    let mut key1 = String::from("one");
    let mut key2 = String::from("two");
    let mut val1 = 1u32;
    let mut val2 = 2u32;

    partial
        .apply(&[
            Op::set().build_with_len_hint(2),
            Op::insert(&mut key1).imm(&mut val1),
            Op::insert(&mut key2).imm(&mut val2),
        ])
        .unwrap();
    std::mem::forget(key1);
    std::mem::forget(key2);

    let result: HashMap<String, u32> = partial.build().unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(result.get("one"), Some(&1));
    assert_eq!(result.get("two"), Some(&2));
}

#[test]
fn build_map_with_default_values() {
    let mut partial = Partial::alloc::<HashMap<String, u32>>().unwrap();

    let mut key1 = String::from("first");
    let mut key2 = String::from("second");

    partial
        .apply(&[
            Op::set().build(),
            Op::insert(&mut key1).default(),
            Op::insert(&mut key2).default(),
        ])
        .unwrap();
    std::mem::forget(key1);
    std::mem::forget(key2);

    let result: HashMap<String, u32> = partial.build().unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(result.get("first"), Some(&0)); // u32::default() = 0
    assert_eq!(result.get("second"), Some(&0));
}

// =============================================================================
// HashMap with complex values (Insert with Build)
// =============================================================================

#[derive(Debug, PartialEq, Facet)]
struct Server {
    host: String,
    port: u16,
}

#[test]
fn build_map_with_struct_values() {
    let mut partial = Partial::alloc::<HashMap<String, Server>>().unwrap();

    let mut key = String::from("primary");
    let host = String::from("localhost");
    let port = 8080u16;

    partial
        .apply(&[
            Op::set().build_with_len_hint(1),
            // Insert with Build for complex value
            Op::insert(&mut key).build(),
            Op::set().at(0).imm(&mut host),
            Op::set().at(1).imm(&mut port),
            Op::End,
        ])
        .unwrap();
    std::mem::forget(key);
    std::mem::forget(host);

    let result: HashMap<String, Server> = partial.build().unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(
        result.get("primary"),
        Some(&Server {
            host: String::from("localhost"),
            port: 8080
        })
    );
}

#[test]
fn build_map_with_multiple_struct_values() {
    let mut partial = Partial::alloc::<HashMap<String, Server>>().unwrap();

    let mut key1 = String::from("primary");
    let mut key2 = String::from("secondary");
    let host1 = String::from("host1.example.com");
    let host2 = String::from("host2.example.com");
    let port1 = 8080u16;
    let port2 = 9090u16;

    partial
        .apply(&[
            Op::set().build_with_len_hint(2),
            // First entry
            Op::insert(&mut key1).build(),
            Op::set().at(0).imm(&mut host1),
            Op::set().at(1).imm(&mut port1),
            Op::End,
            // Second entry
            Op::insert(&mut key2).build(),
            Op::set().at(0).imm(&mut host2),
            Op::set().at(1).imm(&mut port2),
            Op::End,
        ])
        .unwrap();
    std::mem::forget(key1);
    std::mem::forget(key2);
    std::mem::forget(host1);
    std::mem::forget(host2);

    let result: HashMap<String, Server> = partial.build().unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(
        result.get("primary"),
        Some(&Server {
            host: String::from("host1.example.com"),
            port: 8080
        })
    );
    assert_eq!(
        result.get("secondary"),
        Some(&Server {
            host: String::from("host2.example.com"),
            port: 9090
        })
    );
}

// =============================================================================
// Struct with HashMap field
// =============================================================================

#[derive(Debug, PartialEq, Facet)]
struct Config {
    name: String,
    env: HashMap<String, String>,
}

#[test]
fn build_struct_with_map_field() {
    let mut partial = Partial::alloc::<Config>().unwrap();

    let name = String::from("my-app");
    let env_key = String::from("PATH");
    let env_value = String::from("/usr/bin");

    partial
        .apply(&[
            // Set name field
            Op::set().at(0).imm(&mut name),
            // Build env field
            Op::set().at(1).build_with_len_hint(1),
            Op::insert(&mut env_key).imm(&mut env_value),
            Op::End,
        ])
        .unwrap();
    std::mem::forget(name);
    std::mem::forget(env_key);
    std::mem::forget(env_value);

    let result: Config = partial.build().unwrap();
    assert_eq!(result.name, "my-app");
    assert_eq!(result.env.len(), 1);
    assert_eq!(result.env.get("PATH"), Some(&String::from("/usr/bin")));
}

#[test]
fn build_struct_with_empty_map_field() {
    let mut partial = Partial::alloc::<Config>().unwrap();

    let name = String::from("empty-config");

    partial
        .apply(&[
            Op::set().at(0).imm(&mut name),
            Op::set().at(1).build(), // empty map
            Op::End,
        ])
        .unwrap();
    std::mem::forget(name);

    let result: Config = partial.build().unwrap();
    assert_eq!(result.name, "empty-config");
    assert!(result.env.is_empty());
}

// =============================================================================
// HashMap with Vec values
// =============================================================================

#[test]
fn build_map_with_vec_values() {
    let mut partial = Partial::alloc::<HashMap<String, Vec<u32>>>().unwrap();

    let mut key = String::from("numbers");
    let mut a = 1u32;
    let mut b = 2u32;
    let mut c = 3u32;

    partial
        .apply(&[
            Op::set().build(),
            Op::insert(&mut key).build(),
            Op::push().imm(&mut a),
            Op::push().imm(&mut b),
            Op::push().imm(&mut c),
            Op::End,
        ])
        .unwrap();
    std::mem::forget(key);

    let result: HashMap<String, Vec<u32>> = partial.build().unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result.get("numbers"), Some(&vec![1, 2, 3]));
}

// =============================================================================
// Integer keys
// =============================================================================

#[test]
fn build_map_with_integer_keys() {
    let mut partial = Partial::alloc::<HashMap<u32, String>>().unwrap();

    let mut key1 = 1u32;
    let mut key2 = 2u32;
    let mut val1 = String::from("one");
    let mut val2 = String::from("two");

    partial
        .apply(&[
            Op::set().build(),
            Op::insert(&mut key1).imm(&mut val1),
            Op::insert(&mut key2).imm(&mut val2),
        ])
        .unwrap();
    std::mem::forget(val1);
    std::mem::forget(val2);

    let result: HashMap<u32, String> = partial.build().unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(result.get(&1), Some(&String::from("one")));
    assert_eq!(result.get(&2), Some(&String::from("two")));
}

// =============================================================================
// Error cases
// =============================================================================

#[test]
fn insert_on_non_map_errors() {
    let mut partial = Partial::alloc::<u32>().unwrap();

    let mut key = String::from("key");
    let mut val = 42u32;
    let err = partial
        .apply(&[Op::insert(&mut key).imm(&mut val)])
        .unwrap_err();
    assert!(matches!(
        err.kind,
        facet_reflect2::ReflectErrorKind::NotAMap
    ));
}

#[test]
fn insert_wrong_key_type_errors() {
    let mut partial = Partial::alloc::<HashMap<String, u32>>().unwrap();

    let mut wrong_key = 42u32; // Should be String
    let mut val = 1u32;

    partial.apply(&[Op::set().build()]).unwrap();
    let err = partial
        .apply(&[Op::insert(&mut wrong_key).imm(&mut val)])
        .unwrap_err();
    assert!(matches!(
        err.kind,
        facet_reflect2::ReflectErrorKind::KeyShapeMismatch { .. }
    ));
}

#[test]
fn insert_wrong_value_type_errors() {
    let mut partial = Partial::alloc::<HashMap<String, u32>>().unwrap();

    let mut key = String::from("key");
    let mut wrong_val = String::from("not a u32");

    partial.apply(&[Op::set().build()]).unwrap();
    let err = partial
        .apply(&[Op::insert(&mut key).imm(&mut wrong_val)])
        .unwrap_err();
    assert!(matches!(
        err.kind,
        facet_reflect2::ReflectErrorKind::ValueShapeMismatch { .. }
    ));
}

// =============================================================================
// Drop/cleanup tests
// =============================================================================

#[test]
fn drop_partial_map_mid_construction() {
    // Start building a HashMap, insert some entries, then drop without finishing
    let mut partial = Partial::alloc::<HashMap<String, String>>().unwrap();

    let mut key = String::from("key");
    let mut val = String::from("value");

    partial
        .apply(&[Op::set().build(), Op::insert(&mut key).imm(&mut val)])
        .unwrap();
    std::mem::forget(key);
    std::mem::forget(val);

    // Drop the partial - should clean up the map and its entries
    drop(partial);
    // If we get here without Miri complaining, we're good
}

#[test]
fn drop_partial_map_mid_value_build() {
    // Start building a HashMap, start building a value, then drop
    let mut partial = Partial::alloc::<HashMap<String, Server>>().unwrap();

    let mut key = String::from("server");
    let host = String::from("localhost");

    let result = partial.apply(&[
        Op::set().build(),
        Op::insert(&mut key).build(),
        Op::set().at(0).imm(&mut host),
        // Don't set port, don't End - just drop
    ]);
    std::mem::forget(key);
    std::mem::forget(host);

    // This might error (incomplete value) or succeed depending on implementation
    // Either way, dropping should be safe
    let _ = result;
    drop(partial);
}
