use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    println!("cargo:rustc-cfg=vix_slice3_build_script");
    println!("cargo::rustc-check-cfg=cfg(vix_slice3_build_script)");
    println!("cargo:warning=vix slice 3a build script ran");
    println!("cargo:rerun-if-changed=build.rs");

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set"));
    fs::create_dir_all(&out_dir).expect("OUT_DIR exists");
    fs::write(
        out_dir.join("generated.rs"),
        "pub const GENERATED: &str = \"vix-build-script-generated\";\n",
    )
    .expect("write generated.rs");
}
