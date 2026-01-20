//! Build script for dibs-cli.
//!
//! Generates Styx schemas from the Facet types and embeds them in the binary.

fn main() {
    // Re-run if schema source files change
    println!("cargo::rerun-if-changed=../dibs-config/src/lib.rs");
    println!("cargo::rerun-if-changed=../dibs-query-schema/src/lib.rs");

    // Generate config schema
    let config_schema = facet_styx::GenerateSchema::<dibs_config::Config>::new()
        .crate_name("dibs")
        .version("1")
        .cli("dibs")
        .generate();

    // Generate query schema
    let query_schema = facet_styx::GenerateSchema::<dibs_query_schema::QueryFile>::new()
        .crate_name("dibs-queries")
        .version("1")
        .cli("dibs")
        .generate();

    // Write schemas to OUT_DIR for embedding
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");
    let out_path = std::path::Path::new(&out_dir);

    std::fs::write(out_path.join("dibs-config.styx"), &config_schema)
        .expect("Failed to write config schema");

    std::fs::write(out_path.join("dibs-queries.styx"), &query_schema)
        .expect("Failed to write query schema");

    // Write combined file for styx-embed (which embeds a single file)
    let combined = format!(
        "# dibs-config.styx\n{}\n\n# dibs-queries.styx\n{}",
        config_schema, query_schema
    );
    std::fs::write(out_path.join("dibs-schemas.styx"), combined)
        .expect("Failed to write combined schema");
}
