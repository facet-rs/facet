//! Generate TypeScript type definitions and JSON Schema from run_types.rs
//!
//! This binary generates:
//! - TypeScript types for the frontend SPA (run-types.d.ts)
//! - JSON Schema for validating run.json (run-schema.json)
//!
//! Run with: `cargo run -p benchmark-analyzer --bin gen-run-types [output_dir]`
//! Or:       cargo xtask gen-types

mod run_types;

use facet_json_schema::to_schema;
use facet_typescript::TypeScriptGenerator;
use run_types::RunJson;
use serde_json::Value;
use std::fs;
use std::path::PathBuf;

fn main() {
    let workspace_root = find_workspace_root().expect("Could not find workspace root");

    // Parse arguments
    let args: Vec<String> = std::env::args().collect();
    let output_dir = if args.len() > 1 {
        PathBuf::from(&args[1])
    } else {
        workspace_root.join("scripts")
    };

    fs::create_dir_all(&output_dir).expect("Failed to create output directory");

    // Generate TypeScript types
    let ts_path = output_dir.join("run-types.d.ts");
    generate_typescript(&ts_path);

    // Generate JSON Schema
    let schema_path = output_dir.join("run-schema.json");
    generate_json_schema(&schema_path);

    println!("Generated:");
    println!("  TypeScript: {}", ts_path.display());
    println!("  JSON Schema: {}", schema_path.display());
}

fn generate_typescript(output_path: &PathBuf) {
    let mut generator = TypeScriptGenerator::new();
    generator.add_type::<RunJson>();
    let ts = generator.finish();

    let output = format!(
        r#"// Generated from tools/benchmark-analyzer/src/run_types.rs
// Do not edit manually - regenerated on each benchmark run
//
// These types match the run-v1.json schema produced by benchmark-analyzer

{ts}"#
    );

    fs::write(output_path, &output).expect("Failed to write TypeScript types");
}

fn generate_json_schema(output_path: &PathBuf) {
    let schema_str = to_schema::<RunJson>();

    // Parse the JSON, remove nulls, and re-serialize
    let mut schema: Value =
        serde_json::from_str(&schema_str).expect("Invalid JSON from schema generator");
    remove_nulls(&mut schema);

    // Add a comment at the root level
    if let Value::Object(ref mut map) = schema {
        // Insert $comment at the beginning by rebuilding
        let mut new_map = serde_json::Map::new();
        new_map.insert(
            "$comment".to_string(),
            Value::String(
                "Generated from tools/benchmark-analyzer/src/run_types.rs - do not edit manually"
                    .to_string(),
            ),
        );
        for (k, v) in map.iter() {
            new_map.insert(k.clone(), v.clone());
        }
        *map = new_map;
    }

    let output = serde_json::to_string_pretty(&schema).expect("Failed to serialize schema");
    fs::write(output_path, &output).expect("Failed to write JSON Schema");
}

/// Recursively remove null values from a JSON Value
fn remove_nulls(value: &mut Value) {
    match value {
        Value::Object(map) => {
            // Remove keys with null values
            map.retain(|_, v| !v.is_null());
            // Recursively clean remaining values
            for v in map.values_mut() {
                remove_nulls(v);
            }
        }
        Value::Array(arr) => {
            // Remove null elements and clean remaining
            arr.retain(|v| !v.is_null());
            for v in arr.iter_mut() {
                remove_nulls(v);
            }
        }
        _ => {}
    }
}

fn find_workspace_root() -> Option<PathBuf> {
    let mut current = std::env::current_dir().ok()?;
    loop {
        let cargo_toml = current.join("Cargo.toml");
        if cargo_toml.exists()
            && let Ok(content) = fs::read_to_string(&cargo_toml)
            && content.contains("[workspace]")
        {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}
