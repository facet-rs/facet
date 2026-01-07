//! Build script that uses roam-codegen to generate Rust code
//! from ServiceDetail.

use std::env;
use std::fs;
use std::path::Path;

fn main() {
    // Access ServiceDetail from the proto crate
    let detail = codegen_test_proto::service_detail();

    println!("cargo::rerun-if-changed=build.rs");
    println!(
        "cargo::rerun-if-changed={}",
        Path::new("../codegen-test-proto/src/lib.rs").display()
    );

    // Generate Rust code using roam-codegen
    let code = roam_codegen::targets::rust::generate_service(&detail);

    // Write to OUT_DIR
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("generated.rs");
    fs::write(&dest_path, code).unwrap();

    println!("cargo::warning=Generated code at {:?}", dest_path);
}
