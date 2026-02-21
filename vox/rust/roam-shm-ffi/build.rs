fn main() {
    let crate_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let output_file = std::path::PathBuf::from(&out_dir).join("roam_shm_ffi.h");

    cbindgen::Builder::new()
        .with_crate(&crate_dir)
        .with_config(cbindgen::Config::from_file("cbindgen.toml").unwrap())
        .generate()
        .expect("Unable to generate bindings")
        .write_to_file(&output_file);

    println!("cargo::rerun-if-changed=src/lib.rs");
    println!("cargo::rerun-if-changed=cbindgen.toml");
}
