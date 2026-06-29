use std::{env, path::PathBuf, process::Command};

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
    println!(
        "cargo:rerun-if-changed={}",
        include_dir.join("wctype.h").display()
    );

    let mut build = cc::Build::new();
    build.file(scanner).include(include_dir).warnings(false);
    if env::var("TARGET").as_deref() == Ok("wasm32-unknown-unknown") {
        if let Some(llvm_ar) = rust_toolchain_llvm_ar() {
            build.archiver(llvm_ar);
        }
        build.ranlib("true");
    }
    build.compile("tree_sitter_css_reduced_scanner");
}

fn rust_toolchain_llvm_ar() -> Option<PathBuf> {
    let rustc = env::var_os("RUSTC")?;
    let output = Command::new(rustc)
        .args(["--print", "sysroot"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let sysroot = String::from_utf8(output.stdout).ok()?;
    let host = env::var("HOST").ok()?;
    let llvm_ar = PathBuf::from(sysroot.trim())
        .join("lib")
        .join("rustlib")
        .join(host)
        .join("bin")
        .join("llvm-ar");
    llvm_ar.exists().then_some(llvm_ar)
}
