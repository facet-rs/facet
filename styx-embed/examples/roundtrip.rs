//! Example demonstrating embedding a schema and extracting it from the binary.
//!
//! Run with: cargo run -p styx-embed --example roundtrip

use styx_embed::extract_schemas_from_file;

// Embed the schema at compile time using inline string
styx_embed::embed_inline!(
    r#"meta {
  id example-config
  version 1.0.0
  description "Example schema for testing embedding"
}

schema {
  @ @object{
    host @string
    port @int{ min 1, max 65535 }
    debug @optional(@bool)
  }
}
"#
);

fn main() {
    // Get path to our own executable
    let exe_path = std::env::current_exe().expect("failed to get current exe path");
    println!("Extracting schemas from: {}", exe_path.display());

    // Extract schemas from the binary
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
