use std::{env, path::PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let workspace_dir = manifest_dir.parent().expect("workspace dir");
    let scanner =
        workspace_dir.join("snark/tests/fixtures/packages/tree-sitter-css-reduced/src/scanner.c");
    let include_dir = manifest_dir.join("include");

    println!("cargo:rerun-if-changed={}", scanner.display());
    println!(
        "cargo:rerun-if-changed={}",
        include_dir.join("tree_sitter/parser.h").display()
    );

    cc::Build::new()
        .file(scanner)
        .include(include_dir)
        .warnings(false)
        .compile("tree_sitter_css_reduced_scanner");
}
