//! Build script that uses roam-codegen to generate Rust code
//! from ServiceDetail for spec-proto services.

use std::env;
use std::fs;
use std::path::Path;

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!(
        "cargo::rerun-if-changed={}",
        Path::new("../../spec/spec-proto/src/lib.rs").display()
    );

    // Generate code for all services in spec-proto
    let mut code = String::new();
    for detail in spec_proto::all_services() {
        code.push_str(&roam_codegen::targets::rust::generate_service(&detail));
        code.push('\n');
    }

    // Write to OUT_DIR
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("generated.rs");
    fs::write(&dest_path, code).unwrap();

    println!("cargo::warning=Generated code at {:?}", dest_path);
}
