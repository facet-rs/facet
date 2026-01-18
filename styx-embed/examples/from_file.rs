//! Example demonstrating embedding a schema from a file.
//!
//! Run with: cargo run -p styx-embed --example from_file

use styx_embed::extract_schemas_from_file;

// Embed the schema from a file at compile time
styx_embed::embed_file!("crates/styx-embed/examples/test_schema.styx");

fn main() {
    let exe_path = std::env::current_exe().expect("failed to get current exe path");
    println!("Extracting schemas from: {}", exe_path.display());

    match extract_schemas_from_file(&exe_path) {
        Ok(schemas) => {
            println!("Found {} schema(s):\n", schemas.len());
            for (i, schema) in schemas.iter().enumerate() {
                println!("=== Schema {} ===", i + 1);
                println!("{}", schema);
            }
        }
        Err(e) => {
            eprintln!("Failed to extract schemas: {}", e);
            std::process::exit(1);
        }
    }
}
