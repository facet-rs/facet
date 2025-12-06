fn main() {
    // OUT_DIR is something like:
    // /path/to/target/debug/build/plugin-a-hash/out
    // We want: /path/to/target/debug
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let target_debug = std::path::Path::new(&out_dir)
        .ancestors()
        .nth(3) // out -> plugin-a-hash -> build -> debug
        .expect("couldn't find target/debug dir");

    println!("cargo::warning=target_debug = {}", target_debug.display());
    println!("cargo:rustc-link-search=native={}", target_debug.display());
    println!("cargo:rustc-link-lib=dylib=registry");
}
