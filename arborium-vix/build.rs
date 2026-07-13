fn main() {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let src_dir = manifest_dir.join("grammar/src");

    println!(
        "cargo:rerun-if-changed={}",
        src_dir.join("parser.c").display()
    );

    cc::Build::new()
        .include(&src_dir)
        .include(src_dir.join("tree_sitter"))
        .warnings(false)
        .file(src_dir.join("parser.c"))
        .compile("tree_sitter_vix");
}
